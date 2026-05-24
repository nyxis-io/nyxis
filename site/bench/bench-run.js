// Shared benchmark scenarios for browser (and importable patterns for Node).
import { NxsReader, NxsStreamReader } from "/sdk/nxs.js";

/** Visible on chart bars — JSON/CSV parse cost is outside the timed region. */
export const LABEL_PRE = " (pre-parsed)";
/** Visible on chart bars — NXS decodes from .nxb bytes in the timed region. */
export const LABEL_LAZY = " (lazy decode)";

// ── CSV / JSON helpers ──────────────────────────────────────────────────────

export function parseCsv(str) {
  const lines = str.split("\n");
  const headers = lines[0].split(",");
  const nCols = headers.length;
  const out = new Array(lines.length - 2);
  for (let i = 1; i < lines.length; i++) {
    const line = lines[i];
    if (line.length === 0) continue;
    const row = {};
    let start = 0, col = 0;
    for (let j = 0; j <= line.length && col < nCols; j++) {
      if (j === line.length || line.charCodeAt(j) === 44) {
        row[headers[col]] = line.slice(start, j);
        col++;
        start = j + 1;
      }
    }
    out[i - 1] = row;
  }
  return out;
}

export function sumCsvScore(str) {
  let p = str.indexOf("\n") + 1;
  let sum = 0;
  const len = str.length;
  while (p < len) {
    let commas = 0, scoreStart = p;
    while (p < len) {
      const c = str.charCodeAt(p);
      if (c === 44) {
        commas++;
        if (commas === 6) scoreStart = p + 1;
        else if (commas === 7) {
          sum += +str.slice(scoreStart, p);
          while (p < len && str.charCodeAt(p) !== 10) p++;
          p++; break;
        }
      }
      p++;
    }
  }
  return sum;
}

/** Scan JSON text for username lengths without JSON.parse (C-style lower bound). */
export function scanJsonUsernameLengths(str) {
  const needle = '"username":"';
  let acc = 0;
  let p = 0;
  while (p < str.length) {
    const i = str.indexOf(needle, p);
    if (i < 0) break;
    const start = i + needle.length;
    const end = str.indexOf('"', start);
    if (end < 0) break;
    acc += end - start;
    p = end + 1;
  }
  return acc;
}

// ── Timing / UI ─────────────────────────────────────────────────────────────

export function bench(iters, fn) {
  const warmup = Math.max(3, iters / 10 | 0);
  for (let i = 0; i < warmup; i++) fn();
  const t0 = performance.now();
  for (let i = 0; i < iters; i++) fn();
  return (performance.now() - t0) / iters;
}

export async function benchAsync(iters, fn) {
  const warmup = Math.max(1, Math.min(3, iters));
  for (let i = 0; i < warmup; i++) await fn();
  const t0 = performance.now();
  for (let i = 0; i < iters; i++) await fn();
  return (performance.now() - t0) / iters;
}

export function fmtTime(ms) {
  if (ms < 0.001) return `${(ms * 1e6).toFixed(0)} ns`;
  if (ms < 1) return `${(ms * 1e3).toFixed(1)} µs`;
  if (ms < 1000) return `${ms.toFixed(2)} ms`;
  return `${(ms / 1000).toFixed(2)} s`;
}

export function fmtPerRecord(ms, recordCount) {
  if (!recordCount) return "";
  const ns = (ms * 1e6) / recordCount;
  return ns < 1000 ? `${ns.toFixed(0)} ns/rec` : `${(ns / 1000).toFixed(2)} µs/rec`;
}

/** Let the browser paint between heavy chart sections. */
export function yieldFrame() {
  return new Promise((resolve) => requestAnimationFrame(resolve));
}

export function drawChart(containerEl, rows, recordCount = 0) {
  if (!containerEl) return;
  containerEl.innerHTML = "";
  const timed = rows.filter(r => !r.failed && (typeof r.ms === "number" || typeof r.barValue === "number"));
  // Scale bars to the slowest row in this chart only. Do not floor maxBar at 1 ms — warm/random
  // paths are often sub-µs per op; a 1 ms floor collapses every bar to the 1% minimum width.
  const maxBar = timed.length
    ? Math.max(...timed.map(r => r.barValue ?? r.ms), Number.EPSILON)
    : 1;
  for (const r of rows) {
    const label = document.createElement("div");
    label.className = "label";
    label.textContent = r.label;

    const track = document.createElement("div");
    track.className = "bar-track";
    const bar = document.createElement("div");
    if (r.failed) {
      bar.className = "bar failed";
      bar.style.width = "100%";
    } else {
      bar.className = `bar ${r.klass ?? ""}`;
      const barVal = r.barValue ?? r.ms;
      bar.style.width = `${Math.max(1, (barVal / maxBar) * 100)}%`;
    }
    track.appendChild(bar);

    const value = document.createElement("div");
    if (r.failed) {
      value.className = "value failed";
      value.textContent = r.failText || "failed";
    } else {
      value.className = "value";
      const extra = r.perRecord && recordCount && r.displayText == null ? ` · ${fmtPerRecord(r.ms, recordCount)}` : "";
      value.textContent = (r.displayText ?? fmtTime(r.ms)) + extra;
    }
    containerEl.append(label, track, value);
  }
}

export function scenario(label, klass, failed, failText, run, opts = {}) {
  if (failed) return { label, klass, failed: true, failText, perRecord: opts.perRecord };
  return { label, klass, ms: run(), perRecord: opts.perRecord };
}

/** Pre-timed bar row (e.g. after benchAsync). */
export function scenarioMs(label, klass, ms, opts = {}) {
  return { label, klass, ms, perRecord: opts.perRecord };
}

function scatteredIndices(recordCount, max = 500) {
  const n = Math.min(max, recordCount);
  const out = new Uint32Array(n);
  const step = Math.max(1, Math.floor(recordCount / n));
  for (let i = 0; i < n; i++) out[i] = (i * step) % recordCount;
  return out;
}

function sumScoreFromFieldIndex(idx) {
  let s = 0;
  for (let i = 0; i < idx.offsets.length; i++) s += idx.getF64At(i);
  return s;
}

// ── Memory (Chrome) ─────────────────────────────────────────────────────────

function maybeGc() {
  if (typeof globalThis.gc === "function") globalThis.gc();
}

/** Hold parsed data alive until after measureUserAgentSpecificMemory's second sample. */
let _memHold;

async function measureHeapDeltaChrome(setupFn) {
  const mem = performance.memory;
  if (!mem) return null;
  maybeGc();
  _memHold = undefined;
  await new Promise((r) => setTimeout(r, 0));
  const base = mem.usedJSHeapSize;
  _memHold = setupFn();
  // usedJSHeapSize updates asynchronously after large allocations.
  await new Promise((r) => setTimeout(r, 50));
  return Math.max(0, mem.usedJSHeapSize - base);
}

async function measureHeapDeltaUA(setupFn) {
  if (!(globalThis.crossOriginIsolated && performance.measureUserAgentSpecificMemory)) return null;
  try {
    maybeGc();
    _memHold = undefined;
    const before = await performance.measureUserAgentSpecificMemory();
    _memHold = setupFn();
    await new Promise((r) => setTimeout(r, 0));
    const after = await performance.measureUserAgentSpecificMemory();
    return Math.max(0, after.bytes - before.bytes);
  } catch {
    return null;
  }
}

/** Prefer Chrome heap counter; fall back to UA API when COOP/COEP is on but delta is 0. */
async function measureHeapDelta(setupFn) {
  let delta = await measureHeapDeltaChrome(setupFn);
  if (delta != null && delta > 0) return delta;
  const ua = await measureHeapDeltaUA(setupFn);
  if (ua != null && ua > 0) return ua;
  if (delta != null) return delta;
  return ua;
}

/** Measure §17 before the main suite mutates the heap; render later via renderMemoryBench. */
export async function captureMemoryBench(ctx) {
  const { jsonStr, nxbBuf, jsonFailText } = ctx;

  if (!jsonStr) {
    return { ok: false, reason: "No JSON fixture — heap comparison unavailable" };
  }
  if (jsonFailText) {
    return { ok: false, reason: `JSON unavailable (${jsonFailText})` };
  }
  if (!performance.memory && !(globalThis.crossOriginIsolated && performance.measureUserAgentSpecificMemory)) {
    return { ok: false, reason: "performance.memory unavailable — use Chrome for heap comparison" };
  }

  try {
    const jsonDelta = await measureHeapDelta(() => {
      const hold = JSON.parse(jsonStr);
      void hold.length;
      if (hold.length > 0) {
        void hold[0].username;
        void hold[hold.length - 1]?.score;
      }
      return hold;
    });
    _memHold = undefined;
    maybeGc();
    await new Promise((r) => setTimeout(r, 0));

    const nxsDelta = await measureHeapDelta(() => {
      const r = new NxsReader(nxbBuf);
      void r.recordCount;
      const slot = r.slot("username");
      const c = r.cursor();
      if (r.recordCount > 0) {
        c.seek(0);
        void c.getStrBySlot(slot);
      }
      return r;
    });

    if (jsonDelta === null || nxsDelta === null) {
      return { ok: false, reason: "performance.memory unavailable — use Chrome for heap comparison" };
    }

    const method = performance.memory
      ? "performance.memory"
      : "measureUserAgentSpecificMemory";
    return { ok: true, jsonDelta, nxsDelta, method };
  } catch (e) {
    return { ok: false, reason: e.message };
  } finally {
    _memHold = undefined;
  }
}

export function renderMemoryBench(ctx, captured) {
  const { $ } = ctx;
  const memEl = $("#memory-info");
  const chartEl = $("#chart-memory");

  function renderNa(reason) {
    if (memEl) memEl.innerHTML = `<span class="tag empty">${reason}</span>`;
    drawChart(chartEl, [
      { label: "JSON.parse heap growth", klass: "json", failed: true, failText: "N/A" },
      { label: "NxsReader heap growth", klass: "nxs", failed: true, failText: "N/A" },
    ], 0);
  }

  if (!captured?.ok) {
    renderNa(captured?.reason ?? "Memory benchmark skipped");
    return;
  }

  const { jsonDelta, nxsDelta, method } = captured;
  if (memEl) {
    memEl.innerHTML = `
      <span class="tag">Heap Δ after JSON.parse: ${fmtBytes(jsonDelta)}</span>
      <span class="tag">Heap Δ after NxsReader: ${fmtBytes(nxsDelta)}</span>
      <span class="tag">(${method}; measured before other charts)</span>
    `;
  }
  drawChart(chartEl, [
    { label: "JSON.parse heap growth", klass: "json", ms: 0, barValue: jsonDelta, displayText: fmtBytes(jsonDelta) },
    { label: "NxsReader heap growth", klass: "nxs", ms: 0, barValue: nxsDelta, displayText: fmtBytes(nxsDelta) },
  ], 0);
}

// ── Main runner ─────────────────────────────────────────────────────────────

/**
 * @param {object} ctx
 * @param {(sel:string)=>Element} ctx.$
 * @param {Uint8Array} ctx.nxbBuf
 * @param {Uint8Array|null} ctx.nxbColBuf — optional columnar `.nxb` for aggregate charts
 * @param {string|undefined} ctx.jsonStr
 * @param {string|undefined} ctx.csvStr
 * @param {string|null} ctx.jsonFailText
 * @param {string|null} ctx.csvFailText
 * @param {object|null} ctx.wasm — NxsWasm from loadWasm
 * @param {number} ctx.selectedN — UI preset (display only)
 * @param {(msg: string) => void} [ctx.onProgress] — optional status updates while running
 */
export async function runBenchmarks(ctx) {
  const {
    $, nxbBuf, nxbColBuf, jsonStr, csvStr,
    jsonFailText, csvFailText, wasm, selectedN, onProgress,
  } = ctx;

  const progress = (msg) => onProgress?.(msg);
  const chart = async (label, el, rows, recordCount = 0) => {
    progress(label);
    drawChart(el, rows, recordCount);
    await yieldFrame();
  };

  const iters = {
    parse: selectedN >= 1e7 ? 3 : selectedN >= 1e6 ? 5 : selectedN >= 1e5 ? 20 : 200,
    iterateAll: selectedN >= 1e7 ? 2 : selectedN >= 1e6 ? 3 : selectedN >= 1e5 ? 10 : 100,
    iterateWarm: selectedN >= 1e7 ? 2 : selectedN >= 1e6 ? 5 : selectedN >= 1e5 ? 20 : 200,
    random: selectedN >= 1e7 ? 20000 : selectedN >= 1e6 ? 50000 : 100000,
    cold: selectedN >= 1e7 ? 2 : selectedN >= 1e6 ? 3 : selectedN >= 1e5 ? 20 : 200,
    scan: selectedN >= 1e7 ? 2 : selectedN >= 1e6 ? 3 : selectedN >= 1e5 ? 20 : 500,
    coldReduce: selectedN >= 1e7 ? 2 : selectedN >= 1e6 ? 3 : selectedN >= 1e5 ? 10 : 50,
  };

  // §17 heap deltas must be captured before JSON.parse / indexes warm the heap (rendered last).
  progress("§17 Memory (measuring)…");
  const memoryCaptured = await captureMemoryBench(ctx);

  const reader = new NxsReader(nxbBuf);
  const recordCount = reader.recordCount;
  let colReader = null;
  if (nxbColBuf?.byteLength) {
    try {
      const cr = new NxsReader(nxbColBuf);
      if (cr.layout === "columnar") colReader = cr;
    } catch { /* optional fixture */ }
  }
  let wasmReader = null;
  if (wasm) {
    wasmReader = new NxsReader(nxbBuf);
    wasmReader.useWasm(wasm);
  }

  let parsedJson, parsedCsv, jsonParseErr, csvParseErr;
  if (jsonStr !== undefined) {
    try { parsedJson = JSON.parse(jsonStr); }
    catch (e) { jsonParseErr = e.message; }
  }
  if (csvStr !== undefined) {
    try { parsedCsv = parseCsv(csvStr); }
    catch (e) { csvParseErr = e.message; }
  }
  if (parsedJson && parsedJson.length !== recordCount) {
    jsonParseErr = `${parsedJson.length} JSON rows ≠ NXS recordCount ${recordCount}`;
    parsedJson = undefined;
  }
  if (parsedCsv && parsedCsv.length !== recordCount) {
    csvParseErr = `${parsedCsv.length} CSV rows ≠ NXS recordCount ${recordCount}`;
    parsedCsv = undefined;
  }
  const jsonFail = jsonFailText || (jsonParseErr ? `JSON: ${jsonParseErr}` : null);
  const csvFail = csvFailText || (csvParseErr ? `CSV: ${csvParseErr}` : null);

  const uSlot = reader.slot("username");
  const scoreSlot = reader.slot("score");
  const sUser = uSlot;
  const sAge = reader.slot("age");
  const sBal = reader.slot("balance");
  const sAct = reader.slot("active");
  const usernameIndex = reader.buildFieldIndex("username");
  const scoreIndex = reader.buildFieldIndex("score");
  let usernameIndexWasm = null;
  let scoreIndexWasm = null;
  if (wasmReader) {
    usernameIndexWasm = wasmReader.buildFieldIndex("username");
    scoreIndexWasm = wasmReader.buildFieldIndex("score");
  }
  const randCur = reader.cursor();
  const multiCur = reader.cursor();
  const scattered = scatteredIndices(recordCount);
  const idxs = new Array(iters.random);
  for (let i = 0; i < iters.random; i++) idxs[i] = Math.floor(Math.random() * recordCount);
  let ii;

  const pr = { perRecord: true };

  const k = Math.floor(recordCount / 2);
  const vpEnd = recordCount > 0 ? Math.min(49, recordCount - 1) : 0;

  const prefetchColdReadMs = recordCount > 0
    ? await benchAsync(Math.max(3, iters.cold), async () => {
      const r = new NxsReader(nxbBuf);
      await r.prefetch_viewport(0, vpEnd);
      const c = r.cursor();
      c.seek(k);
      return c.getStrBySlot(uSlot);
    })
    : null;

  const vpWarmReader = new NxsReader(nxbBuf);
  if (recordCount > 0) await vpWarmReader.prefetch_viewport(0, vpEnd);

  const openPrefetchScanMs = recordCount > 0
    ? await benchAsync(Math.max(2, Math.min(iters.iterateAll, 20)), async () => {
      const r = new NxsReader(nxbBuf);
      await r.prefetch_viewport(0, vpEnd);
      let acc = 0;
      r.scan(cur => { acc += cur.getStrBySlot(uSlot).length; });
      return acc;
    })
    : null;

  // 1. Open
  await chart("§1 Open file", $("#chart-open"), [
    scenario("JSON.parse", "json", !!jsonFail, jsonFail, () => bench(iters.parse, () => JSON.parse(jsonStr))),
    scenario("parseCsv", "csv", !!csvFail, csvFail, () => bench(iters.parse, () => parseCsv(csvStr))),
    scenario("new NxsReader", "nxs", false, null, () => bench(iters.parse, () => new NxsReader(nxbBuf))),
  ], recordCount);

  // 2. Open + iterate all
  await chart("§6 Open + iterate all", $("#chart-iterate-all"), [
    scenario("JSON parse + for-of username", "json", !!jsonFail, jsonFail, () =>
      bench(iters.iterateAll, () => {
        let acc = 0;
        for (const r of JSON.parse(jsonStr)) acc += r.username.length;
        return acc;
      }), pr),
    scenario("CSV parse + for-of username", "csv", !!csvFail, csvFail, () =>
      bench(iters.iterateAll, () => {
        let acc = 0;
        for (const r of parseCsv(csvStr)) acc += r.username.length;
        return acc;
      }), pr),
    scenario("NXS open + records() loop", "nxs", false, null, () =>
      bench(iters.iterateAll, () => {
        const r = new NxsReader(nxbBuf);
        let acc = 0;
        for (const rec of r.records()) acc += rec.getStrBySlot(uSlot).length;
        return acc;
      }), pr),
    scenario("NXS open + cursor.scan", "nxs", false, null, () =>
      bench(iters.iterateAll, () => {
        const r = new NxsReader(nxbBuf);
        let acc = 0;
        r.scan(cur => { acc += cur.getStrBySlot(uSlot).length; });
        return acc;
      }), pr),
    scenario("NXS open + buildIndex + loop", "nxs", false, null, () =>
      bench(iters.iterateAll, () => {
        const r = new NxsReader(nxbBuf);
        const idx = r.buildFieldIndex("username");
        let acc = 0;
        for (let i = 0; i < r.recordCount; i++) acc += idx.getStrAt(i).length;
        return acc;
      }), pr),
    ...(openPrefetchScanMs != null
      ? [scenarioMs(
        `NXS open + prefetch_viewport(0,${vpEnd}) + scan`,
        "nxs", openPrefetchScanMs, pr,
      )]
      : []),
  ], recordCount);

  // 3. Iterate only (warm)
  await chart("§8 Warm iterate", $("#chart-iterate-warm"), [
    scenario(`JSON for-of${LABEL_PRE}`, "json", !parsedJson, jsonFail, () =>
      bench(iters.iterateWarm, () => {
        let acc = 0;
        for (const r of parsedJson) acc += r.username.length;
        return acc;
      }), pr),
    scenario(`CSV for-of${LABEL_PRE}`, "csv", !parsedCsv, csvFail, () =>
      bench(iters.iterateWarm, () => {
        let acc = 0;
        for (const r of parsedCsv) acc += r.username.length;
        return acc;
      }), pr),
    scenario(`NXS cursor.scan${LABEL_LAZY}`, "nxs", false, null, () =>
      bench(iters.iterateWarm, () => {
        let acc = 0;
        reader.scan(cur => { acc += cur.getStrBySlot(uSlot).length; });
        return acc;
      }), pr),
    scenario(`NXS field index${LABEL_LAZY}`, "nxs", false, null, () =>
      bench(iters.iterateWarm, () => {
        let acc = 0;
        for (let i = 0; i < recordCount; i++) acc += usernameIndex.getStrAt(i).length;
        return acc;
      }), pr),
    scenario("NXS prefetch_viewport warm + cursor.scan", "nxs", false, null, () =>
      bench(iters.iterateWarm, () => {
        let acc = 0;
        vpWarmReader.scan(cur => { acc += cur.getStrBySlot(uSlot).length; });
        return acc;
      }), pr),
  ], recordCount);

  // 4. Random 1-field
  const randomRows = [
    scenario(`JSON arr[k].username${LABEL_PRE}`, "json", !parsedJson, jsonFail, () => {
      ii = 0; return bench(iters.random, () => parsedJson[idxs[ii++ % iters.random]].username);
    }),
    scenario(`CSV arr[k].username${LABEL_PRE}`, "csv", !parsedCsv, csvFail, () => {
      ii = 0; return bench(iters.random, () => parsedCsv[idxs[ii++ % iters.random]].username);
    }),
    scenario(`NXS record(k)${LABEL_LAZY}`, "nxs", false, null, () => {
      ii = 0; return bench(iters.random, () => reader.record(idxs[ii++ % iters.random]).getStrBySlot(uSlot));
    }),
    scenario(`NXS cursor.seek${LABEL_LAZY}`, "nxs", false, null, () => {
      ii = 0;
      return bench(iters.random, () => {
        randCur.seek(idxs[ii++ % iters.random]);
        return randCur.getStrBySlot(uSlot);
      });
    }),
    scenario(`NXS field index${LABEL_LAZY}`, "nxs", false, null, () => {
      ii = 0; return bench(iters.random, () => usernameIndex.getStrAt(idxs[ii++ % iters.random]));
    }),
  ];
  if (usernameIndexWasm) {
    randomRows.push(scenario("NXS field index (WASM build)", "nxs-wasm", false, null, () => {
      ii = 0; return bench(iters.random, () => usernameIndexWasm.getStrAt(idxs[ii++ % iters.random]));
    }));
  }
  await chart("§9 Random access", $("#chart-random"), randomRows, recordCount);

  // 5. Random multi-field
  await chart("§10 Random multi-field", $("#chart-random-multi"), [
    scenario(`JSON 4-field${LABEL_PRE}`, "json", !parsedJson, jsonFail, () => {
      ii = 0;
      return bench(iters.random, () => {
        const r = parsedJson[idxs[ii++ % iters.random]];
        return r.username.length + r.age + r.balance + (r.active ? 1 : 0);
      });
    }),
    scenario(`CSV 4-field${LABEL_PRE}`, "csv", !parsedCsv, csvFail, () => {
      ii = 0;
      return bench(iters.random, () => {
        const r = parsedCsv[idxs[ii++ % iters.random]];
        return r.username.length + +r.age + +r.balance + (r.active === "true" ? 1 : 0);
      });
    }),
    scenario(`NXS 4-field record(k)${LABEL_LAZY}`, "nxs", false, null, () => {
      ii = 0;
      return bench(iters.random, () => {
        const obj = reader.record(idxs[ii++ % iters.random]);
        return obj.getStrBySlot(sUser).length + obj.getI64BySlot(sAge)
          + obj.getF64BySlot(sBal) + (obj.getBoolBySlot(sAct) ? 1 : 0);
      });
    }),
    scenario(`NXS 4-field seekWarm(k)${LABEL_LAZY}`, "nxs", false, null, () => {
      ii = 0;
      return bench(iters.random, () => {
        multiCur.seekWarm(idxs[ii++ % iters.random]);
        return multiCur.getStrBySlot(sUser).length + multiCur.getI64BySlot(sAge)
          + multiCur.getF64BySlot(sBal) + (multiCur.getBoolBySlot(sAct) ? 1 : 0);
      });
    }),
  ], recordCount);

  // 6. Scattered access
  await chart("§11 Scattered access", $("#chart-scattered"), [
    scenario(`JSON scattered${LABEL_PRE}`, "json", !parsedJson, jsonFail, () =>
      bench(iters.scan, () => {
        let acc = 0;
        for (let j = 0; j < scattered.length; j++) acc += parsedJson[scattered[j]].username.length;
        return acc;
      })),
    scenario(`NXS cursor scattered${LABEL_LAZY}`, "nxs", false, null, () =>
      bench(iters.scan, () => {
        let acc = 0;
        for (let j = 0; j < scattered.length; j++) {
          randCur.seek(scattered[j]);
          acc += randCur.getStrBySlot(uSlot).length;
        }
        return acc;
      })),
    scenario(`NXS field index scattered${LABEL_LAZY}`, "nxs", false, null, () =>
      bench(iters.scan, () => {
        let acc = 0;
        for (let j = 0; j < scattered.length; j++) acc += usernameIndex.getStrAt(scattered[j]).length;
        return acc;
      })),
  ], recordCount);

  // 7. Multi-field full scan
  await chart("§12 Multi-field scan", $("#chart-multi-scan"), [
    scenario("JSON parse + 4-field loop", "json", !!jsonFail, jsonFail, () =>
      bench(iters.iterateAll, () => {
        let acc = 0;
        for (const r of JSON.parse(jsonStr)) {
          acc += r.username.length + r.age + r.balance + (r.active ? 1 : 0);
        }
        return acc;
      }), pr),
    scenario(`NXS open + cursor.scan${LABEL_LAZY}`, "nxs", false, null, () =>
      bench(iters.iterateAll, () => {
        const r = new NxsReader(nxbBuf);
        let acc = 0;
        r.scan(cur => {
          acc += cur.getStrBySlot(sUser).length + cur.getI64BySlot(sAge)
            + cur.getF64BySlot(sBal) + (cur.getBoolBySlot(sAct) ? 1 : 0);
        });
        return acc;
      }), pr),
  ], recordCount);

  // 8. Filter count (score > 80)
  const scoreThreshold = 80;
  await chart("§13 Filter", $("#chart-filter"), [
    scenario(`JSON filter count${LABEL_PRE}`, "json", !parsedJson, jsonFail, () =>
      bench(iters.scan, () => {
        let c = 0;
        for (const r of parsedJson) if (r.score > scoreThreshold) c++;
        return c;
      }), pr),
    scenario(`NXS cursor filter (lazy decode, row bitmask)`, "nxs", false, null, () =>
      bench(iters.scan, () => {
        let c = 0;
        const cur = reader.cursor();
        for (let i = 0; i < recordCount; i++) {
          cur.seek(i);
          if (cur.getF64BySlot(scoreSlot) > scoreThreshold) c++;
        }
        return c;
      }), pr),
  ], recordCount);

  // 9. Cold first field (bytes already in memory)
  await chart("§2 Cold first field", $("#chart-cold-mem"), [
    scenario("JSON parse + arr[k]", "json", !!jsonFail, jsonFail, () =>
      bench(iters.cold, () => JSON.parse(jsonStr)[k].username)),
    scenario("CSV parse + arr[k]", "csv", !!csvFail, csvFail, () =>
      bench(iters.cold, () => parseCsv(csvStr)[k].username)),
    scenario("NXS new reader + cursor(k)", "nxs", false, null, () =>
      bench(iters.cold, () => {
        const c = new NxsReader(nxbBuf).cursor();
        c.seek(k);
        return c.getStrBySlot(uSlot);
      })),
    ...(prefetchColdReadMs != null
      ? [scenarioMs(
        `NXS prefetch_viewport(0,${vpEnd}) + cursor(k)`,
        "nxs", prefetchColdReadMs, pr,
      )]
      : []),
  ], recordCount);

  // 10. Cold fetch + first field (same as before)
  await chart("§3 Cold pipeline", $("#chart-cold-fetch"), [
    scenario("JSON parse + arr[k]", "json", !!jsonFail, jsonFail, () =>
      bench(iters.cold, () => JSON.parse(jsonStr)[k].username)),
    scenario("CSV parse + arr[k]", "csv", !!csvFail, csvFail, () =>
      bench(iters.cold, () => parseCsv(csvStr)[k].username)),
    scenario("NXS open + cursor(k)", "nxs", false, null, () =>
      bench(iters.cold, () => {
        const c = new NxsReader(nxbBuf).cursor();
        c.seek(k);
        return c.getStrBySlot(uSlot);
      })),
    ...(prefetchColdReadMs != null
      ? [scenarioMs(
        `NXS prefetch_viewport(0,${vpEnd}) + cursor(k)`,
        "nxs", prefetchColdReadMs, pr,
      )]
      : []),
  ], recordCount);

  // 11. Aggregate (warm)
  const reduceRows = [
    scenario(`JSON sum score${LABEL_PRE}`, "json", !parsedJson, jsonFail, () =>
      bench(iters.scan, () => { let s = 0; for (const r of parsedJson) s += r.score; return s; }), pr),
    scenario("CSV sumCsvScore (raw bytes)", "csv", !csvStr, csvFail, () =>
      bench(iters.scan, () => sumCsvScore(csvStr), pr)),
    scenario(`NXS row sumF64${LABEL_LAZY}`, "nxs", false, null, () =>
      bench(iters.scan, () => reader.sumF64("score"), pr)),
  ];
  if (wasmReader) {
    reduceRows.push(scenario(`NXS row sumF64 WASM${LABEL_LAZY}`, "nxs-wasm", false, null, () =>
      bench(iters.scan, () => wasmReader.sumF64("score"), pr)));
  }
  if (colReader) {
    reduceRows.push(scenario("NXS columnar colSumF64", "nxs-col", false, null, () =>
      bench(iters.scan, () => colReader.colSumF64("score"), pr)));
  }
  await chart("§14 Aggregate (warm)", $("#chart-reduce"), reduceRows, recordCount);

  // 12. Indexed sum vs reducer
  const indexedRows = [
    scenario(`JSON sum score${LABEL_PRE}`, "json", !parsedJson, jsonFail, () =>
      bench(iters.scan, () => {
        let s = 0;
        for (const r of parsedJson) s += r.score;
        return s;
      }), pr),
    scenario(`NXS row sumF64${LABEL_LAZY}`, "nxs", false, null, () =>
      bench(iters.scan, () => reader.sumF64("score"), pr)),
    scenario(`NXS row buildIndex + loop${LABEL_LAZY}`, "nxs", false, null, () =>
      bench(iters.scan, () => {
        const idx = reader.buildFieldIndex("score");
        return sumScoreFromFieldIndex(idx);
      }), pr),
  ];
  if (colReader) {
    indexedRows.push(scenario("NXS columnar colSumF64", "nxs-col", false, null, () =>
      bench(iters.scan, () => colReader.colSumF64("score"), pr)));
  }
  if (scoreIndexWasm) {
    indexedRows.push(scenario("NXS WASM index + loop", "nxs-wasm", false, null, () =>
      bench(iters.scan, () => sumScoreFromFieldIndex(scoreIndexWasm), pr)));
  }
  await chart("§15 Indexed sum", $("#chart-indexed-sum"), indexedRows, recordCount);

  // 12b. Columnar prefetch_column (§7.4) — NXS vs JSON aggregate honesty
  if (colReader && nxbColBuf?.byteLength) {
    const colWarm = new NxsReader(nxbColBuf);
    if (colWarm.layout === "columnar") colWarm.prefetch_column("score");
    const colPrefetchRows = [
      scenario(`JSON sum score${LABEL_PRE}`, "json", !parsedJson, jsonFail, () =>
        bench(iters.scan, () => {
          let s = 0;
          for (const r of parsedJson) s += r.score;
          return s;
        }, pr)),
      scenario("JSON parse + sum score (cold)", "json", !!jsonFail, jsonFail, () =>
        bench(iters.scan, () => {
          let s = 0;
          for (const r of JSON.parse(jsonStr)) s += r.score;
          return s;
        }, pr)),
      scenario("NXS colSumF64 (new reader)", "nxs-col", false, null, () =>
        bench(iters.scan, () => new NxsReader(nxbColBuf).colSumF64("score"), pr)),
      scenario("NXS prefetch_column + sum (new reader)", "nxs-col", false, null, () =>
        bench(iters.scan, () => {
          const r = new NxsReader(nxbColBuf);
          r.prefetch_column("score");
          return r.colSumF64("score");
        }, pr)),
      scenario("NXS prefetch warm + colSumF64", "nxs-col", false, null, () =>
        bench(iters.scan, () => colWarm.colSumF64("score"), pr)),
    ];
    await chart("§16 Column prefetch", $("#chart-column-prefetch"), colPrefetchRows, recordCount);
  } else {
    const el = $("#chart-column-prefetch");
    if (el) el.innerHTML = "<p class=\"desc\">Columnar fixture not loaded — chart skipped.</p>";
  }

  // 13. Cold pipeline: open + sum (no pre-parse)
  const coldReduceRows = [
    scenario("JSON parse + sum score", "json", !!jsonFail, jsonFail, () =>
      bench(iters.coldReduce, () => {
        let s = 0;
        for (const r of JSON.parse(jsonStr)) s += r.score;
        return s;
      }), pr),
    scenario("CSV sumCsvScore", "csv", !csvStr, csvFail, () =>
      bench(iters.coldReduce, () => sumCsvScore(csvStr), pr)),
    scenario("NXS open + sumF64", "nxs", false, null, () =>
      bench(iters.coldReduce, () => new NxsReader(nxbBuf).sumF64("score"), pr)),
  ];
  if (wasmReader) {
    coldReduceRows.push(scenario("NXS open + sumF64 WASM", "nxs-wasm", false, null, () =>
      bench(iters.coldReduce, () => {
        const r = new NxsReader(nxbBuf);
        r.useWasm(wasm);
        return r.sumF64("score");
      }), pr));
  }
  await chart("§4 Cold aggregate", $("#chart-cold-reduce"), coldReduceRows, recordCount);

  // 14. JSON raw scan (no full parse)
  await chart("§7 JSON scan", $("#chart-json-scan"), [
    scenario("JSON.parse + loop", "json", !!jsonFail, jsonFail, () =>
      bench(iters.scan, () => {
        let acc = 0;
        for (const r of JSON.parse(jsonStr)) acc += r.username.length;
        return acc;
      }), pr),
    scenario('JSON scan "username" (no parse)', "json", !jsonStr, jsonFail, () =>
      bench(iters.scan, () => scanJsonUsernameLengths(jsonStr), pr)),
    scenario("NXS cursor.scan username", "nxs", false, null, () =>
      bench(iters.scan, () => {
        let acc = 0;
        reader.scan(cur => { acc += cur.getStrBySlot(uSlot).length; });
        return acc;
      }), pr),
  ], recordCount);

  // 15. Stream — time to first record
  const streamRows = [
    scenario("NXS stream chunked → 1st record", "nxs", false, null, () => {
      const chunk = Math.min(65536, Math.max(4096, (nxbBuf.length / 32) | 0));
      return bench(Math.max(3, iters.cold), () => {
        let first = null;
        const t0 = performance.now();
        const sr = new NxsStreamReader({
          onRecord() {
            if (first === null) first = performance.now() - t0;
          },
        });
        for (let p = 0; p < nxbBuf.length; p += chunk) {
          sr.push(nxbBuf.subarray(p, Math.min(p + chunk, nxbBuf.length)));
        }
        sr.finish();
        return first ?? 0;
      });
    }),
    scenario("JSON.parse → 1st record", "json", !!jsonFail, jsonFail, () =>
      bench(iters.cold, () => {
        const t0 = performance.now();
        const arr = JSON.parse(jsonStr);
        const elapsed = performance.now() - t0;
        void arr[0].username;
        return elapsed;
      })),
  ];
  await chart("§5 Stream TTFR", $("#chart-stream"), streamRows, recordCount);

  // 17. Worker parallel sum (optional)
  progress("§18 Workers…");
  await runWorkerBench(ctx, recordCount, iters.scan);
  await yieldFrame();

  progress("§17 Memory…");
  renderMemoryBench(ctx, memoryCaptured);
  await yieldFrame();

  return recordCount;
}

async function runWorkerBench(ctx, recordCount, scanIters) {
  const el = ctx.$("#chart-worker");
  if (!el) return;
  const { nxbBuf } = ctx;
  const workers = Math.min(4, navigator.hardwareConcurrency || 4);
  const workerUrl = "/bench/bench-worker.js";

  try {
    const mainMs = bench(Math.max(2, Math.min(5, scanIters)), () => new NxsReader(nxbBuf).sumF64("score"));

    const workerMs = await benchAsync(2, async () => {
      const chunk = Math.ceil(recordCount / workers);
      const jobs = [];
      for (let w = 0; w < workers; w++) {
        const start = w * chunk;
        const end = Math.min(recordCount, start + chunk);
        if (start >= end) continue;
        jobs.push(new Promise((resolve, reject) => {
          const worker = new Worker(workerUrl, { type: "module" });
          const chunkBuf = nxbBuf.slice();
          const timer = setTimeout(() => {
            worker.terminate();
            reject(new Error("worker timed out"));
          }, 120_000);
          worker.onmessage = (ev) => {
            if (ev.data?.type === "sum-error") {
              clearTimeout(timer);
              worker.terminate();
              reject(new Error(ev.data.message ?? "worker error"));
              return;
            }
            if (ev.data?.type === "sum-result") {
              clearTimeout(timer);
              worker.terminate();
              resolve(ev.data.sum);
            }
          };
          worker.onerror = (e) => {
            clearTimeout(timer);
            worker.terminate();
            reject(e.error ?? new Error(e.message || "worker failed"));
          };
          worker.postMessage({
            type: "sum-chunk",
            workerId: w,
            buffer: chunkBuf.buffer,
            start,
            end,
          }, [chunkBuf.buffer]);
        }));
      }
      let total = 0;
      for (const p of await Promise.all(jobs)) total += p;
      return total;
    });

    drawChart(el, [
      { label: "Main-thread sumF64", klass: "nxs", ms: mainMs, perRecord: true },
      { label: `${workers} workers sum chunks (buffer copy each)`, klass: "nxs-wasm", ms: workerMs, perRecord: true },
    ], recordCount);
    await yieldFrame();
  } catch (e) {
    drawChart(el, [{ label: "Worker benchmark", klass: "nxs", failed: true, failText: e.message }], 0);
    await yieldFrame();
  }
}

function fmtBytes(n) {
  if (n < 1024) return `${n} B`;
  if (n < 1048576) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1048576).toFixed(2)} MB`;
}
