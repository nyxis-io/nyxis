import { NxsReader } from "/sdk/nxs.js";
import { loadWasm } from "/sdk/wasm.js";
import { runBenchmarks } from "@bench/bench-run.js";

let demoRoot = null;
let demoQuery = (sel) => document.querySelector(sel);
const $ = (sel) => demoQuery(sel);

/** @type {(() => void) | null} */
let teardownBench = null;

export async function wireBenchPage(root) {
  if (!root) {
    console.warn("wireBenchPage: missing root element");
    return;
  }
  if (root.dataset.benchWired === "1") return;
  teardownBench?.();
  root.dataset.benchWired = "1";
  demoRoot = root;
  demoQuery = (sel) => root.querySelector(sel);
  await initDemo();
  teardownBench = () => {
    delete root.dataset.benchWired;
    demoRoot = null;
    demoQuery = (sel) => document.querySelector(sel);
  };
}

export function unwireBenchPage() {
  teardownBench?.();
  teardownBench = null;
}

async function initDemo() {
  // ── Utilities ─────────────────────────────────────────────────────────────
  
  const fmtBytes = n =>
    n < 1024        ? `${n} B` :
    n < 1048576     ? `${(n/1024).toFixed(1)} KB` :
                      `${(n/1048576).toFixed(2)} MB`;
  
  function setStatus(msg, cls = "") {
    const el = $("#status");
    el.textContent = msg;
    el.className = `status ${cls}`;
  }
  
  // ── Main ──────────────────────────────────────────────────────────────────
  
  let selectedN = 10000;
  /** When set, Run uses uploaded buffers instead of fetch(fixtures). */
  let uploadedSuite = null;
  
  function restoreSizeButtonActive() {
    demoRoot.querySelectorAll("#sizes button").forEach(b => {
      b.classList.toggle("active", parseInt(b.dataset.n, 10) === selectedN);
    });
  }
  
  demoRoot.querySelectorAll("#sizes button").forEach(btn => {
    btn.addEventListener("click", () => {
      uploadedSuite = null;
      demoRoot.querySelectorAll("#sizes button").forEach(b => b.classList.remove("active"));
      btn.classList.add("active");
      selectedN = parseInt(btn.dataset.n, 10);
      $("#upload-summary").textContent = "No files selected — using size buttons above.";
      $("#upload-summary").classList.add("empty");
      $("#clear-upload").hidden = true;
      $("#suite-files").value = "";
    });
  });
  
  // V8/JSC/SpiderMonkey all cap strings at roughly 2^30 characters (~512 MB).
  // `fetch(...).text()` materialises the response as a JS string, so files
  // above this limit cannot be decoded as strings at all — JSON.parse never
  // gets a chance to fail. We detect this up-front and mark the bars as failed.
  const STRING_LIMIT_BYTES = 512 * 1024 * 1024;
  
  // ── WAL reference chart ───────────────────────────────────────────────────────
  // `make bench-sequential` → Rust WAL section (BENCHMARK.md, Apple M-series).
  // Keyed by span count → { appendBatch, recover, seal, roundtrip } ns/span + file sizes.
  const WAL_DATA = {
    1000: {
      timing: { appendBatch: 1640, recover: 1213, seal: 3541, roundtrip: 6087 },
      wal_bytes: 116096, nxb_bytes: 123699, json_bytes: 179507,
    },
    10000: {
      timing: { appendBatch: 742, recover: 1039, seal: 3090, roundtrip: 4527 },
      wal_bytes: 1163264, nxb_bytes: 1247805, json_bytes: 1795507,
    },
    100000: {
      timing: { appendBatch: 644, recover: 1050, seal: 3422, roundtrip: 4589 },
      wal_bytes: 11060000, nxb_bytes: 12020000, json_bytes: 17200000,
    },
    // Extrapolated from 100k plateau (ns/span stable; bytes scale linearly at
    // 110.6 B/span WAL · 120.2 B/span NXB · 172.0 B/span JSON)
    1000000: {
      timing: { appendBatch: 644, recover: 1050, seal: 3422, roundtrip: 4589 },
      wal_bytes: 110600000, nxb_bytes: 120200000, json_bytes: 172000000,
    },
    10000000: {
      timing: { appendBatch: 644, recover: 1050, seal: 3422, roundtrip: 4589 },
      wal_bytes: 1106000000, nxb_bytes: 1202000000, json_bytes: 1720000000,
    },
    100000000: {
      timing: { appendBatch: 644, recover: 1050, seal: 3422, roundtrip: 4589 },
      wal_bytes: 11060000000, nxb_bytes: 12020000000, json_bytes: 17200000000,
    },
  };
  
  const fmtNs = ns =>
    ns < 1000    ? `${ns.toFixed(0)} ns` :
    ns < 1000000 ? `${(ns/1000).toFixed(1)} µs` :
                   `${(ns/1000000).toFixed(2)} ms`;
  
  function drawWalTimingChart(data) {
    const { timing } = data;
    const rows = [
      { label: "append-batch", ns: timing.appendBatch, klass: "nxs"      },
      { label: "recover",   ns: timing.recover,   klass: "csv"      },
      { label: "seal",      ns: timing.seal,       klass: "nxs-wasm" },
      { label: "roundtrip", ns: timing.roundtrip,  klass: "json"     },
    ];
    const maxNs = Math.max(...rows.map(r => r.ns));
  const el = $("#chart-wal-timing");
  if (!el) return;
  el.innerHTML = "";
    for (const r of rows) {
      const label = document.createElement("div");
      label.className = "label";
      label.textContent = r.label;
      const track = document.createElement("div");
      track.className = "bar-track";
      const bar = document.createElement("div");
      bar.className = `bar ${r.klass}`;
      bar.style.width = `${Math.max(1, (r.ns / maxNs) * 100)}%`;
      track.appendChild(bar);
      const value = document.createElement("div");
      value.className = "value";
      value.textContent = fmtNs(r.ns) + " / span";
      el.append(label, track, value);
    }
  }
  
  function drawWalSizeChart(data) {
    const el = $("#chart-wal-size");
    if (!el) return;
    const { wal_bytes, nxb_bytes, json_bytes } = data;
    const rows = [
      { label: "WAL (.nxsw)",   bytes: wal_bytes,  klass: "nxs"      },
      { label: "Sealed (.nxb)", bytes: nxb_bytes,  klass: "nxs-wasm" },
      { label: "JSON NDJSON",   bytes: json_bytes, klass: "json"     },
    ];
    const maxBytes = Math.max(...rows.map(r => r.bytes));
    el.innerHTML = "";
    for (const r of rows) {
      const label = document.createElement("div");
      label.className = "label";
      label.textContent = r.label;
      const track = document.createElement("div");
      track.className = "bar-track";
      const bar = document.createElement("div");
      bar.className = `bar ${r.klass}`;
      bar.style.width = `${Math.max(1, (r.bytes / maxBytes) * 100)}%`;
      track.appendChild(bar);
      const pct = (r.bytes / json_bytes * 100).toFixed(1);
      const value = document.createElement("div");
      value.className = "value";
      value.textContent = `${fmtBytes(r.bytes)} (${pct}%)`;
      el.append(label, track, value);
    }
  }
  
  function renderWal(n) {
    const data = WAL_DATA[n];
    if (!data) return;
    drawWalTimingChart(data);
    drawWalSizeChart(data);
    $("#wal-sizes-info").innerHTML = `
      <span class="tag">${n.toLocaleString()} spans</span>
      <span class="tag">WAL ${fmtBytes(data.wal_bytes)} (${(data.wal_bytes/data.json_bytes*100).toFixed(1)}% of JSON)</span>
      <span class="tag">Sealed ${fmtBytes(data.nxb_bytes)} (${(data.nxb_bytes/data.json_bytes*100).toFixed(1)}% of JSON)</span>
      <span class="tag">JSON ${fmtBytes(data.json_bytes)}</span>
    `;
  }
  
  let selectedWalN = 1000;
  demoRoot.querySelectorAll("#wal-sizes button").forEach(btn => {
    btn.addEventListener("click", () => {
      demoRoot.querySelectorAll("#wal-sizes button").forEach(b => b.classList.remove("active"));
      btn.classList.add("active");
      selectedWalN = parseInt(btn.dataset.n, 10);
      renderWal(selectedWalN);
    });
  });
  renderWal(selectedWalN);
  
  // ── Cross-language WAL comparison ────────────────────────────────────────────
  // Pure in-memory encode (no I/O), n=10,000 spans — BENCHMARK.md / bench-sequential.
  const WAL_LANG = [
    // lang,               nxs_ns, json_ns
    ["C",                  73,      270   ],
    ["Go",                 131,     301   ],
    ["Rust",               131,     131   ],
    ["JS (fast)",          250,     320   ],
    ["JS (WASM)",          375,     320   ],
    ["Python (C ext)",     438,     1383  ],
    ["Ruby (C ext)",       336,     383   ],
    ["JS (generic)",       750,     320   ],
    ["Python (pure)",      3800,    1383  ],
    ["Ruby (pure)",        5300,    383   ],
  ];
  
  function drawWalLangChart(containerId, key, label, klass) {
    const el = $(`#${containerId}`);
    if (!el) return;
    el.innerHTML = "";
    const rows = WAL_LANG.filter(r => r[key === "nxs" ? 1 : 2] != null);
    rows.sort((a, b) => a[key === "nxs" ? 1 : 2] - b[key === "nxs" ? 1 : 2]);
    const maxNs = Math.max(...rows.map(r => r[key === "nxs" ? 1 : 2]));
    const h3 = document.createElement("h3");
    h3.style.cssText = "font-size:13px;font-weight:600;margin:0 0 8px;color:var(--muted)";
    h3.textContent = label;
    el.appendChild(h3);
    for (const r of rows) {
      const ns = r[key === "nxs" ? 1 : 2];
      const langLabel = document.createElement("div");
      langLabel.className = "label";
      langLabel.textContent = r[0];
      const track = document.createElement("div");
      track.className = "bar-track";
      const bar = document.createElement("div");
      bar.className = `bar ${klass}`;
      bar.style.width = `${Math.max(1, (ns / maxNs) * 100)}%`;
      track.appendChild(bar);
      const kps = (1e9 / ns / 1000).toFixed(0);
      const value = document.createElement("div");
      value.className = "value";
      value.textContent = `${fmtNs(ns)} / span  (${kps}k/s)`;
      el.append(langLabel, track, value);
    }
  }
  
  drawWalLangChart("chart-wal-lang-nxs",  "nxs",  "NXS WAL append (ns/span, lower is better)",  "nxs");
  drawWalLangChart("chart-wal-lang-json", "json", "JSON serialise (ns/span, lower is better)",   "json");
  
  // Load WASM in parallel with fixture fetch on Run — don't block page wiring.
  let wasm = null;
  const wasmPromise = loadWasm("/bench/wasm/nxs_reducers.wasm", { initialPages: 2200 })
    .then((w) => {
      wasm = w;
      return w;
    })
    .catch((e) => {
      console.warn("WASM load failed:", e);
      return null;
    });
  
  // Cache fetched resources per size so rerunning the same N doesn't redownload
  // gigabytes. Values are { nxbBuf, jsonBytes, csvBytes, jsonStr?, csvStr?, err? }.
  const fetchCache = new Map();
  
  // Safely convert an ArrayBuffer of UTF-8 bytes into a JS string. Returns
  // { str } on success or { err } if the result would exceed the engine's
  // string-length cap. The cap cannot be reliably probed, so we rely on both
  // byte-length heuristics and a try/catch around TextDecoder.
  function bytesToStringSafe(bytes) {
    if (bytes.byteLength > STRING_LIMIT_BYTES) {
      return { err: `exceeds JS string limit (${fmtBytes(bytes.byteLength)} > 512 MB)` };
    }
    try {
      const str = new TextDecoder("utf-8").decode(bytes);
      return { str };
    } catch (e) {
      return { err: e.message };
    }
  }
  
  // Fetch each fixture independently. A failure on one does not prevent the
  // others from being used — JSON at 1.5 GB may fail with ERR_CONTENT_LENGTH
  // or string-length errors; NXS should still run.
  async function fetchBytesSafe(url) {
    try {
      const res = await fetch(url);
      if (!res.ok) return { err: `HTTP ${res.status}` };
      const ab = await res.arrayBuffer();
      return { bytes: new Uint8Array(ab) };
    } catch (e) {
      return { err: e.message };
    }
  }
  
  function buildEntryFromBuffers(nxbBuf, nxbColBuf, jsonBytes, csvBytes, jsonMissingErr, csvMissingErr) {
    const jsonDec = jsonBytes?.byteLength
      ? bytesToStringSafe(jsonBytes)
      : { err: jsonMissingErr ?? "not uploaded" };
    const csvDec = csvBytes?.byteLength
      ? bytesToStringSafe(csvBytes)
      : { err: csvMissingErr ?? "not uploaded" };
    return {
      nxbBuf,
      nxbColBuf: nxbColBuf?.byteLength ? nxbColBuf : null,
      jsonBytes,
      csvBytes,
      jsonStr: jsonDec.str,
      csvStr: csvDec.str,
      jsonErr: jsonDec.err,
      csvErr: csvDec.err,
    };
  }
  
  async function loadFixtures(n) {
    if (fetchCache.has(n)) return fetchCache.get(n);
  
    setStatus(`Fetching ${n.toLocaleString()} records…`, "running");
    const [nRes, colRes, jRes, cRes] = await Promise.all([
    fetchBytesSafe(`/bench/fixtures/records_${n}.nxb`),
    fetchBytesSafe(`/bench/fixtures/records_${n}_columnar.nxb`),
    fetchBytesSafe(`/bench/fixtures/records_${n}.json`),
    fetchBytesSafe(`/bench/fixtures/records_${n}.csv`),
    ]);
  
    if (nRes.err) throw new Error(`NXS fetch failed: ${nRes.err}`);
    const nxbBuf = nRes.bytes;
    const nxbColBuf = colRes.err ? null : colRes.bytes;
    if (colRes.err) {
      console.warn(`Columnar fixture not loaded (${colRes.err}); §14–15 columnar bars omitted.`);
    }
    const jsonBytes = jRes.bytes;
    const csvBytes = cRes.bytes;
  
    const entry = buildEntryFromBuffers(
      nxbBuf,
      nxbColBuf,
      jsonBytes,
      csvBytes,
      jsonBytes ? undefined : `fetch: ${jRes.err}`,
      csvBytes ? undefined : `fetch: ${cRes.err}`,
    );
    fetchCache.set(n, entry);
    return entry;
  }
  
  async function applyUploadedFiles(fileList) {
    const files = [...fileList];
    let nxbFile = null;
    let nxbColFile = null;
    let jsonFile = null;
    let csvFile = null;
    for (const f of files) {
      const low = f.name.toLowerCase();
      if (low.endsWith(".nxb") && low.includes("columnar")) nxbColFile = f;
      else if (low.endsWith(".nxb")) nxbFile = f;
      else if (low.endsWith(".json")) jsonFile = f;
      else if (low.endsWith(".csv")) csvFile = f;
    }
    if (!nxbFile) {
      setStatus("Pick at least one .nxb file (optional .json / .csv for comparison).", "error");
      return;
    }
    const nxbBuf = new Uint8Array(await nxbFile.arrayBuffer());
    let n;
    try {
      n = new NxsReader(nxbBuf).recordCount;
    } catch (e) {
      setStatus(`Not a valid .nxb: ${e.message}`, "error");
      return;
    }
    const nxbColBuf = nxbColFile
      ? new Uint8Array(await nxbColFile.arrayBuffer())
      : null;
    const jsonBytes = jsonFile ? new Uint8Array(await jsonFile.arrayBuffer()) : undefined;
    const csvBytes = csvFile ? new Uint8Array(await csvFile.arrayBuffer()) : undefined;
    const entry = buildEntryFromBuffers(
      nxbBuf,
      nxbColBuf,
      jsonBytes,
      csvBytes,
      jsonFile ? undefined : "not uploaded",
      csvFile ? undefined : "not uploaded",
    );
    uploadedSuite = { n, entry };
    const parts = [nxbFile.name];
    if (nxbColFile) parts.push(nxbColFile.name);
    if (jsonFile) parts.push(jsonFile.name);
    if (csvFile) parts.push(csvFile.name);
    $("#upload-summary").textContent = `Using upload: ${parts.join(" + ")} — n=${n.toLocaleString()}`;
    $("#upload-summary").classList.remove("empty");
    $("#clear-upload").hidden = false;
    document.querySelectorAll("#sizes button").forEach(b => b.classList.remove("active"));
    setStatus("Files loaded — click Run benchmark.", "done");
  }
  
  const uploadDrop = $("#upload-drop");
  const suiteInput = $("#suite-files");
  $("#pick-suite").addEventListener("click", () => suiteInput.click());
  suiteInput.addEventListener("change", async () => {
    const fl = suiteInput.files;
    if (fl?.length) await applyUploadedFiles(fl);
    suiteInput.value = "";
  });
  ;["dragenter", "dragover"].forEach(t =>
    uploadDrop.addEventListener(t, e => { e.preventDefault(); uploadDrop.classList.add("drag"); }));
  ;["dragleave", "drop"].forEach(t =>
    uploadDrop.addEventListener(t, e => {
      e.preventDefault();
      if (t === "dragleave" && e.target !== uploadDrop) return;
      uploadDrop.classList.remove("drag");
    }));
  uploadDrop.addEventListener("drop", async e => {
    const fl = e.dataTransfer?.files;
    if (fl?.length) await applyUploadedFiles(fl);
  });
  $("#clear-upload").addEventListener("click", () => {
    uploadedSuite = null;
    $("#upload-summary").textContent = "No files selected — using size buttons above.";
    $("#upload-summary").classList.add("empty");
    $("#clear-upload").hidden = true;
    restoreSizeButtonActive();
  });
  
  $("#run").addEventListener("click", async () => {
    $("#run").disabled = true;
  
    try {
      const n = uploadedSuite ? uploadedSuite.n : selectedN;
      const entryPromise = uploadedSuite
        ? Promise.resolve(uploadedSuite.entry)
        : loadFixtures(n);
      const [entry, wasmMod] = await Promise.all([entryPromise, wasmPromise]);
      if (wasmMod) wasm = wasmMod;
      const {
        nxbBuf, nxbColBuf, jsonBytes, csvBytes,
        jsonStr, csvStr, jsonErr, csvErr,
      } = entry;
  
      // Size info
      const jsonLen = jsonBytes?.length ?? 0;
      const csvLen = csvBytes?.length ?? 0;
      const nxbPct = jsonLen ? (nxbBuf.length / jsonLen * 100).toFixed(0) : "?";
      const colLen = nxbColBuf?.length ?? 0;
      const colPct = jsonLen && colLen ? (colLen / jsonLen * 100).toFixed(0) : "?";
      const csvPct = jsonLen ? (csvLen / jsonLen * 100).toFixed(0) : "?";
      const srcLabel = uploadedSuite ? "uploaded" : `records_${n}`;
      $("#sizes-info").innerHTML = `
        <span class="tag">${srcLabel}</span>
        <span class="tag">.nxb row ${fmtBytes(nxbBuf.length)} (${nxbPct}% of JSON)</span>
        <span class="tag">.nxb col ${colLen ? `${fmtBytes(colLen)} (${colPct}% of JSON)` : "missing"}</span>
        <span class="tag">.json ${jsonBytes ? fmtBytes(jsonLen) : jsonErr}</span>
        <span class="tag">.csv ${csvBytes ? fmtBytes(csvLen) : csvErr} ${csvBytes ? `(${csvPct}% of JSON)` : ""}</span>
      `;
  
      const jsonFailText = jsonErr ?? null;
      const csvFailText = csvErr ?? null;
      const recordCount = await runBenchmarks({
        $,
        nxbBuf,
        nxbColBuf,
        jsonStr,
        csvStr,
        jsonFailText,
        csvFailText,
        wasm,
        selectedN: n,
        onProgress: (msg) => setStatus(`${msg} · n=${n.toLocaleString()}`, "running"),
      });
  
      setStatus(`Done. n=${recordCount.toLocaleString()} — scroll for results.`, "done");
    } catch (e) {
      setStatus(`Error: ${e.message}`, "error");
      console.error(e);
    } finally {
      $("#run").disabled = false;
    }
  });
  
  // Run once automatically on page load so the user sees something immediately.
  $("#run").click();
}
