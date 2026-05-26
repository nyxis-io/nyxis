let demoRoot = null;
let demoQuery = (sel) => document.querySelector(sel);
const $ = (sel) => demoQuery(sel);
let teardown = null;

export function wireWorkersPage(root) {
  if (!root) return;
  if (root.dataset.demoWired === "1") return;
  teardown?.();
  root.dataset.demoWired = "1";
  demoRoot = root;
  demoQuery = (sel) => root.querySelector(sel);
  const stopDemo = initDemo();
  teardown = () => {
    stopDemo();
    delete root.dataset.demoWired;
    demoRoot = null;
    demoQuery = (sel) => document.querySelector(sel);
  };
}

export function unwireWorkersPage() {
  teardown?.();
  teardown = null;
}

function initDemo() {
  /** @type {(() => void) | null} */
  let disposeRun = null;

  function domReady() {
    return demoRoot && demoRoot.isConnected;
  }

  function setElText(el, text) {
    if (el) el.textContent = text;
  }

  function setElHtml(el, html) {
    if (el) el.innerHTML = html;
  }
  function fmtMs(ms) {
    if (ms < 1) return `${(ms * 1000).toFixed(0)} µs`;
    return `${ms.toFixed(2)} ms`;
  }

  function setStatus(msg, cls = "") {
    const el = $("#status");
    if (!el) return;
    el.textContent = msg;
    el.className = `status ${cls}`;
  }

  const NXB_URL = "/bench/fixtures/records_100000.nxb";
  const JSON_URL = "/bench/fixtures/records_100000.json";
  const N_WORKERS = 4;
  const WRITE_RECORD = 42;
  const WRITE_KEY = "score";

  // ── Isolation check ───────────────────────────────────────────────────────

  const hasSAB = typeof SharedArrayBuffer !== "undefined";
  const isolated = typeof crossOriginIsolated !== "undefined" && crossOriginIsolated;
  
  (function setBanner() {
    const b = $("#iso-banner");
    if (!b) return;
    if (hasSAB && isolated) {
      b.className = "banner ok";
      b.innerHTML = "<strong>Cross-origin isolated</strong> — <code>SharedArrayBuffer</code> is available. The NXS path uses true zero-copy sharing.";
    } else if (hasSAB && !isolated) {
      b.className = "banner warn";
      b.innerHTML = "<strong>Not cross-origin isolated.</strong> <code>SharedArrayBuffer</code> exists but isn't usable here. Falling back to a non-shared <code>ArrayBuffer</code> (NXS path will copy per worker). Serve with <code>python3 server.py</code> (COOP/COEP) to enable real sharing.";
    } else {
      b.className = "banner bad";
      b.innerHTML = "<strong>SharedArrayBuffer unavailable.</strong> Falling back to a per-worker copy. The NXS path will still work; the cross-worker write demo won't propagate (each worker has its own buffer).";
    }
  })();
  
  // ── Worker management ─────────────────────────────────────────────────────
  
  const nxsWorkers = [];
  const jsonWorkers = [];
  
  function renderWorkerRows(containerId, n, prefix) {
    const host = $(containerId);
    if (!host) return;
    host.innerHTML = "";
    for (let i = 0; i < n; i++) {
      const row = document.createElement("div");
      row.className = "worker-row";
      row.innerHTML = `<span class="wid">${prefix} #${i}</span><span class="wtime" id="${containerId.slice(1)}-t-${i}">—</span><span class="wval" id="${containerId.slice(1)}-v-${i}">idle</span>`;
      host.appendChild(row);
    }
  }
  
  // Pre-warm workers by spawning them and waiting until their module is loaded
  // and they signal ready from a no-op ping. This separates module-load time
  // (identical for both paths) from the actual data-transfer cost we want to compare.
  function pingWorker(w, type) {
    return new Promise(resolve => {
      w.addEventListener("message", function once(ev) {
        if (ev.data && ev.data.type === "ready") {
          w.removeEventListener("message", once);
          resolve();
        }
      });
    });
  }
  
  function stopWorkers() {
    for (const w of nxsWorkers) {
      w.onmessage = null;
      w.terminate();
    }
    for (const w of jsonWorkers) {
      w.onmessage = null;
      w.terminate();
    }
    nxsWorkers.length = 0;
    jsonWorkers.length = 0;
    stopReaderTicks();
    stopWriter();
    readTicks = {};
  }

  async function spawnNxsWorkers(sharedBuffer, size, runId) {
    if (!domReady() || runId !== activeRunId) return null;
    renderWorkerRows("#nxs-workers", N_WORKERS, "nxs");
  
    // Measure postMessage cost on the sender side (same methodology as JSON clone cost).
    // For SAB: postMessage registers a shared-memory pointer — O(1), no copy.
    // The worker's NxsReader construction is on its thread and doesn't block the sender.
    const senderCosts = [];
    const promises = [];
    for (let i = 0; i < N_WORKERS; i++) {
      const w = new Worker(new URL("../workers/nxs_worker.js", import.meta.url), { type: "module" });
      nxsWorkers.push(w);
      w.onmessage = (ev) => {
        if (runId !== activeRunId) return;
        handleNxsMessage(i, ev.data);
      };
      const t0 = performance.now();
      w.postMessage({ type: "init", workerId: i, buffer: sharedBuffer, size });
      const cost = performance.now() - t0; // sender-side postMessage cost (SAB pointer pass)
      senderCosts.push(cost);
      promises.push(new Promise(resolve => {
        w.addEventListener("message", function once(ev) {
          if (runId !== activeRunId) {
            w.removeEventListener("message", once);
            return;
          }
          if (ev.data && ev.data.type === "ready") {
            setElText($(`#nxs-workers-t-${i}`), fmtMs(cost) + " transfer");
            setElText(
              $(`#nxs-workers-v-${i}`),
              `ready · ${ev.data.recordCount.toLocaleString()} records · ${ev.data.shared ? "SHARED" : "COPY"}`,
            );
            w.removeEventListener("message", once);
            resolve(cost);
          }
        });
      }));
    }
    await Promise.all(promises);
    if (!domReady() || runId !== activeRunId) return null;
    // Sum (serial postMessages on main thread, like JSON path).
    const total = senderCosts.reduce((a, b) => a + b, 0);
    const avg = total / N_WORKERS;
    setElText($("#nxs-total"), fmtMs(total));
    setElText($("#nxs-avg"), fmtMs(avg));
    return { total, avg, timings: senderCosts };
  }
  
  async function spawnJsonWorkers(parsed, runId) {
    if (!domReady() || runId !== activeRunId) return null;
    renderWorkerRows("#json-workers", N_WORKERS, "json");
  
    // For JSON, structured-clone runs synchronously on the sender during postMessage —
    // the main thread blocks for the full copy duration before the call returns.
    // We measure that sender-side blocking time per postMessage call (the clone cost),
    // then wait for workers to finish receiving.
    const cloneMs = [];
    const promises = [];
    for (let i = 0; i < N_WORKERS; i++) {
      const w = new Worker(new URL("../workers/json_worker.js", import.meta.url), { type: "module" });
      jsonWorkers.push(w);
      const t0 = performance.now();
      w.postMessage({ type: "init", workerId: i, data: parsed }); // blocks until clone done
      const cost = performance.now() - t0;
      cloneMs.push(cost);
      promises.push(new Promise(resolve => {
        w.addEventListener("message", function once(ev) {
          if (runId !== activeRunId) {
            w.removeEventListener("message", once);
            return;
          }
          if (ev.data && ev.data.type === "ready") {
            setElText($(`#json-workers-t-${i}`), fmtMs(cost) + " clone");
            setElText($(`#json-workers-v-${i}`), `ready · ${ev.data.recordCount.toLocaleString()} records`);
            w.removeEventListener("message", once);
            resolve(cost);
          }
        });
      }));
    }
    await Promise.all(promises);
    if (!domReady() || runId !== activeRunId) return null;
    // Clone calls are serial on the main thread — total = sum.
    const total = cloneMs.reduce((a, b) => a + b, 0);
    const avg = total / N_WORKERS;
    setElText($("#json-total"), fmtMs(total));
    setElText($("#json-avg"), fmtMs(avg));
    return { total, avg, timings: cloneMs };
  }
  
  // ── Live readers ──────────────────────────────────────────────────────────
  
  let readTicks = {}; // workerId → {value, ts}
  let activeRunId = 0;
  
  function renderReaderTicks() {
    if (!domReady()) return;
    const host = $("#reader-ticks");
    if (!host) return;
    host.innerHTML = "";
    for (let i = 1; i < N_WORKERS; i++) {
      const t = readTicks[i] || { value: "—", ts: 0 };
      const row = document.createElement("div");
      row.className = "worker-row";
      const valStr = t.value == null ? "—" : (typeof t.value === "number" ? t.value.toFixed(3) : String(t.value));
      row.innerHTML = `<span class="wid">nxs #${i}</span><span class="wtime">${t.ts ? new Date(t.ts).toLocaleTimeString() : "—"}</span><span class="wval">${valStr}</span>`;
      host.appendChild(row);
    }
  }
  
  function handleNxsMessage(workerId, msg) {
    if (!msg || !domReady()) return;
    if (msg.type === "read-fast-result") {
      readTicks[workerId] = { value: msg.value, ts: Date.now() };
      renderReaderTicks();
    } else if (msg.type === "write-result") {
      if (msg.ok) {
        setElText($("#writer-val"), `${msg.value.toFixed(3)} → record ${msg.index}.${msg.key}`);
      }
    }
  }
  
  let readerInterval = null;
  let writerInterval = null;
  
  function startReaderTicks() {
    stopReaderTicks();
    readerInterval = setInterval(() => {
      // Workers 1-3 all read the same record 42 score (the thing the writer is updating).
      for (let i = 1; i < N_WORKERS; i++) {
        nxsWorkers[i].postMessage({
          type: "read-f64-fast",
          tag: "t",
          index: WRITE_RECORD,
          key: WRITE_KEY,
        });
      }
    }, 100);
  }
  
  function stopReaderTicks() {
    if (readerInterval) { clearInterval(readerInterval); readerInterval = null; }
  }
  
  function startWriter() {
    stopWriter();
    let v = 0;
    writerInterval = setInterval(() => {
      v = performance.now(); // timestamp as the value; any number works
      nxsWorkers[0].postMessage({
        type: "write-f64",
        index: WRITE_RECORD,
        key: WRITE_KEY,
        value: v,
      });
    }, 50);
  }
  
  function stopWriter() {
    if (writerInterval) { clearInterval(writerInterval); writerInterval = null; }
  }
  
  // ── Main flow ─────────────────────────────────────────────────────────────
  
  let nxbBytes = null;
  let parsedJson = null;
  
  async function loadData() {
    setStatus("Fetching fixtures…", "running");
    const [nxbBuf, jsonText] = await Promise.all([
      fetch(NXB_URL).then(r => r.arrayBuffer()),
      fetch(JSON_URL).then(r => r.text()),
    ]);
    nxbBytes = new Uint8Array(nxbBuf);
    parsedJson = JSON.parse(jsonText);
    setStatus(`Loaded ${parsedJson.length.toLocaleString()} records · NXS ${(nxbBytes.byteLength / 1048576).toFixed(1)} MB · JSON ${(jsonText.length / 1048576).toFixed(1)} MB.`, "done");
  }
  
  function makeSharedBuffer(size) {
    if (hasSAB && isolated) {
      const sab = new SharedArrayBuffer(size);
      return { buffer: sab, shared: true };
    }
    // Fallback: plain ArrayBuffer. Each postMessage will structured-clone it.
    return { buffer: new ArrayBuffer(size), shared: false };
  }
  
  let startInFlight = false;

  function onWriterToggle(e) {
    if (e.target.checked) {
      if (nxsWorkers.length < N_WORKERS) {
        setStatus("Start the demo first.", "warn");
        e.target.checked = false;
        return;
      }
      startWriter();
    } else {
      stopWriter();
      setElText($("#writer-val"), "idle");
    }
  }

  async function start() {
    const startBtn = $("#start");
    if (!startBtn || startInFlight) return;
    startInFlight = true;
    startBtn.disabled = true;
    const runId = ++activeRunId;
    disposeRun?.();
    disposeRun = () => {
      if (activeRunId === runId) activeRunId++;
    };
    try {
      if (!nxbBytes) await loadData();
      if (!domReady() || runId !== activeRunId) return;

      // Tear down any previous run
      stopWorkers();
      const writerToggle = $("#writer-toggle");
      if (writerToggle) writerToggle.checked = false;
  
      // Allocate the shared buffer and copy the .nxb bytes into it ONCE.
      setStatus("Allocating shared buffer + spawning workers…", "running");
      const { buffer, shared } = makeSharedBuffer(nxbBytes.byteLength);
      new Uint8Array(buffer).set(nxbBytes);
  
      const nxsSize = nxbBytes.byteLength;
  
      // Spawn JSON workers first — the structured-clone stall is most visible
      // if it runs on a warm main thread.
      const jsonResult = await spawnJsonWorkers(parsedJson, runId);
      const nxsResult = await spawnNxsWorkers(buffer, nxsSize, runId);
      if (!domReady() || runId !== activeRunId || !jsonResult || !nxsResult) return;

      // JSON "bytes copied": roughly JSON-array size × N (structured clone makes
      // a deep copy per worker).
      const jsonBytes = JSON.stringify(parsedJson).length * N_WORKERS;
      setElText($("#json-bytes"), `~${(jsonBytes / 1048576).toFixed(1)} MB`);
      setElText($("#nxs-bytes"), shared ? "0 B (shared)" : `${(nxsSize * N_WORKERS / 1048576).toFixed(1)} MB (fallback)`);

      const ratio = nxsResult.total > 0 ? (jsonResult.total / nxsResult.total).toFixed(0) : "∞";
      setElHtml($("#summary"), `
        <strong>JSON:</strong> ${(jsonBytes / 1048576).toFixed(1)} MB copied across ${N_WORKERS} workers =
        <strong>${fmtMs(jsonResult.total)}</strong> total spawn.<br>
        <strong>NXS:</strong> ${shared ? "0 bytes copied (SharedArrayBuffer)" : `${(nxsSize / 1048576).toFixed(1)} MB, copied (SAB fallback)`} =
        <strong>${fmtMs(nxsResult.total)}</strong> total spawn.<br>
        <strong>Speedup:</strong> NXS is ~<strong>${ratio}×</strong> faster to fan out to workers.
      `);
  
      // Start background readers
      startReaderTicks();
  
      setStatus("Running — reader workers polling every 100 ms.", "done");
    } catch (err) {
      console.error(err);
      if (domReady()) setStatus("Error: " + err.message, "error");
    } finally {
      startInFlight = false;
      if (domReady()) {
        const btn = $("#start");
        if (btn) btn.disabled = false;
      }
    }
  }

  const startBtn = $("#start");
  const writerToggle = $("#writer-toggle");
  startBtn?.addEventListener("click", start);
  writerToggle?.addEventListener("change", onWriterToggle);

  return () => {
    activeRunId++;
    disposeRun = null;
    stopWorkers();
    startBtn?.removeEventListener("click", start);
    writerToggle?.removeEventListener("change", onWriterToggle);
    startInFlight = false;
  };
}
