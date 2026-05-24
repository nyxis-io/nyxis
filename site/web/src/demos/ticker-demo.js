import { NxsReader } from "/sdk/nxs.js";

let demoQuery = (sel) => document.querySelector(sel);
const $ = (sel) => demoQuery(sel);

let running = false;
let rafId = 0;
/** @type {(() => void) | null} */
let teardownTicker = null;

export function wireTickerPage(root) {
  if (!root) {
    console.warn("wireTickerPage: missing root element");
    return;
  }
  if (root.dataset.tickerWired === "1") return;
  unwireTickerPage();
  root.dataset.tickerWired = "1";
  demoQuery = (sel) => root.querySelector(sel);
  initTickerDemo();
  teardownTicker = () => {
    running = false;
    if (rafId) cancelAnimationFrame(rafId);
    rafId = 0;
    delete root.dataset.tickerWired;
    demoQuery = (sel) => document.querySelector(sel);
  };
}

export function unwireTickerPage() {
  teardownTicker?.();
  teardownTicker = null;
}

function initTickerDemo() {
// ── Utilities ─────────────────────────────────────────────────────────────


// Auto-selects µs when < 1 ms for sub-millisecond work times.
function fmtWork(ms) {
  if (ms < 0.001) return "0 µs";
  if (ms < 1) return `${(ms * 1000).toFixed(0)} µs`;
  return `${ms.toFixed(2)} ms`;
}

function setStatus(msg, cls = "") {
  const el = $("#status");
  el.textContent = msg;
  el.className = `status ${cls}`;
}

const FIXTURE_NXB  = "/bench/fixtures/records_100000.nxb";
const FIXTURE_JSON = "/bench/fixtures/records_100000.json";
const FRAME_BUDGET_MS = 20;      // anything longer than this is a "dropped frame"
const SPARK_LEN = 50;

// ── State ─────────────────────────────────────────────────────────────────

let reader = null;        // NxsReader
let scoreByteOffset = -1; // absolute byte offset of record 0's score f64
let nxsView = null;       // DataView aliased over reader.bytes

let jsonParsed = null;    // live JS array
let jsonSerialized = ""; // most recent serialised form (for re-parse path)


const stats = {
  json: { last: 0, max: 0, maxWork: 0, drops: 0, frames: 0, totalTime: 0, totalWork: 0, spark: new Array(SPARK_LEN).fill(0) },
  nxs:  { last: 0, max: 0, maxWork: 0, drops: 0, frames: 0, totalTime: 0, totalWork: 0, spark: new Array(SPARK_LEN).fill(0) },
};

let prevFrameTs = 0; // last rAF timestamp (shared; rAF fires for both columns in same tick)

// ── Fixture loading ───────────────────────────────────────────────────────

async function loadFixtures() {
  setStatus("Fetching fixtures…", "running");
  const [nxbBuf, jsonText] = await Promise.all([
    fetch(FIXTURE_NXB).then(r => r.arrayBuffer()),
    fetch(FIXTURE_JSON).then(r => r.text()),
  ]);

  // NXS: construct reader over a mutable Uint8Array so in-place patches work.
  const bytes = new Uint8Array(nxbBuf);
  reader = new NxsReader(bytes);
  nxsView = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);

  // Resolve record 0's score field to an absolute byte offset. We do this
  // once and cache — every subsequent frame just writes 8 bytes.
  const slot = reader.slot("score");
  const obj  = reader.record(0);
  obj.getF64BySlot(slot); // forces header parsing + populates _offsetTableStart
  const rawOff = obj._resolveSlot(slot);
  if (rawOff < 0) throw new Error("score field missing on record 0");
  scoreByteOffset = rawOff;

  // JSON: parse once, keep both the parsed form and the serialised form.
  jsonParsed = JSON.parse(jsonText);
  jsonSerialized = jsonText;

  setStatus(`Loaded ${reader.recordCount.toLocaleString()} records — NXS ${(bytes.length/1048576).toFixed(1)} MB, JSON ${(jsonText.length/1048576).toFixed(1)} MB.`, "done");
}

// ── Frame work ────────────────────────────────────────────────────────────
// Two independent work functions so one column can be "hot" while the other stays idle.

function workJson(newScore, frameIndex, reparseEvery) {
  // 1) In-place mutate the parsed structure (cheap).
  jsonParsed[0].score = newScore;

  // 2) Every K frames, re-serialise + re-parse the whole array. This is the
  //    realistic "server just pushed a new snapshot" path — JSON has no way
  //    to patch bytes in place so you must walk the whole doc again.
  if (frameIndex % reparseEvery === 0) {
    jsonSerialized = JSON.stringify(jsonParsed);
    jsonParsed = JSON.parse(jsonSerialized);
  }

  return jsonParsed[0].score;
}

function workNxs(newScore /*, frameIndex */) {
  // Overwrite 8 bytes at the cached offset. Zero allocations.
  nxsView.setFloat64(scoreByteOffset, newScore, true);
  // Read it back (the "display" path — same cost a real consumer would pay).
  return nxsView.getFloat64(scoreByteOffset, true);
}

// ── rAF loop ──────────────────────────────────────────────────────────────

let frameIndex = 0;

function frame(ts) {
  if (!running) return;

  const reparseEl = $("#reparse");
  const pressureEl = $("#pressure");
  if (!reparseEl || !pressureEl) {
    running = false;
    return;
  }
  const reparseEvery = parseInt(reparseEl.value, 10);
  const pressure     = parseInt(pressureEl.value, 10);

  // ── JSON column work ────────────────────────────────────────────
  const t0j = performance.now();
  let jsonScore = 0;
  for (let k = 0; k < pressure; k++) {
    // cycle newScore so the display is lively
    const v = ((frameIndex * pressure + k) % 1000) / 10;
    jsonScore = workJson(v, frameIndex * pressure + k, reparseEvery);
  }
  const t1j = performance.now();

  // ── NXS column work ─────────────────────────────────────────────
  const t0n = performance.now();
  let nxsScore = 0;
  for (let k = 0; k < pressure; k++) {
    const v = ((frameIndex * pressure + k) % 1000) / 10;
    nxsScore = workNxs(v);
  }
  const t1n = performance.now();

  // Frame deltas (compared against previous rAF timestamp).
  const dt = prevFrameTs === 0 ? 16.67 : ts - prevFrameTs;
  prevFrameTs = ts;

  // We attribute the overall frame delta to each column, since both ran
  // in the same tick. But we also show per-column *work time* in the
  // "last frame" stat — that's the part of the delta this column caused.
  recordFrame("json", dt, t1j - t0j);
  recordFrame("nxs",  dt, t1n - t0n);

  // Render
  $("#score-json").textContent = jsonScore.toFixed(2);
  $("#score-nxs").textContent  = nxsScore.toFixed(2);
  renderStats("json");
  renderStats("nxs");

  frameIndex++;
  rafId = requestAnimationFrame(frame);
}

function recordFrame(col, dt, workMs) {
  const s = stats[col];
  s.frames++;
  s.totalTime += dt;
  s.totalWork += workMs;
  s.last = workMs;
  if (workMs > s.maxWork) s.maxWork = workMs;
  if (workMs > FRAME_BUDGET_MS) s.drops++;
  // Sparkline shows per-column work time, same rule.
  s.spark.push(workMs);
  if (s.spark.length > SPARK_LEN) s.spark.shift();
}

function renderStats(col) {
  const s = stats[col];
  const fps = s.totalTime > 0 ? (s.frames / (s.totalTime / 1000)) : 0;
  const avg = s.frames > 0 ? s.totalWork / s.frames : 0;
  $(`#fps-${col}`).textContent   = fps.toFixed(1);
  $(`#last-${col}`).textContent  = fmtWork(s.last);
  $(`#max-${col}`).textContent   = fmtWork(s.maxWork);
  $(`#avg-${col}`).textContent   = fmtWork(avg);
  const drops = $(`#drops-${col}`);
  drops.textContent = s.drops.toString();
  drops.classList.toggle("hot", s.drops > 0);
  renderSparkline(col);
}

function renderSparkline(col) {
  const host = $(`#spark-${col}`);
  const spark = stats[col].spark;
  // Scale bars so 33 ms = 100% (2x budget). Anything >20 ms is red.
  const maxMs = 33;
  // Rebuild child nodes only if count mismatches (first render).
  if (host.childElementCount !== spark.length) {
    host.innerHTML = "";
    for (let i = 0; i < spark.length; i++) {
      const b = document.createElement("div");
      b.className = "bar";
      host.appendChild(b);
    }
  }
  const kids = host.children;
  for (let i = 0; i < spark.length; i++) {
    const v = spark[i];
    const h = Math.min(100, (v / maxMs) * 100);
    const el = kids[i];
    el.style.height = h.toFixed(0) + "%";
    if (v > FRAME_BUDGET_MS) el.classList.add("bad");
    else el.classList.remove("bad");
  }
}

// ── PerformanceObserver for long tasks ─────────────────────────────────────

function setupLongTaskObserver() {
  if (typeof PerformanceObserver === "undefined") return;
  const supported = PerformanceObserver.supportedEntryTypes || [];
  if (!supported.includes("longtask")) return;

  const list = $("#longtasks");
  let seen = 0;
  try {
    const obs = new PerformanceObserver(entries => {
      for (const e of entries.getEntries()) {
        seen++;
        if (seen === 1) list.innerHTML = "";
        const line = document.createElement("div");
        line.textContent = `[${new Date().toLocaleTimeString()}] longtask duration=${e.duration.toFixed(1)} ms`;
        list.prepend(line);
        while (list.childElementCount > 40) list.removeChild(list.lastChild);
      }
    });
    obs.observe({ entryTypes: ["longtask"] });
  } catch (err) {
    // Some browsers reject the observe call for specific entry types.
    console.warn("longtask observer unavailable:", err);
  }
}

// ── Wiring ────────────────────────────────────────────────────────────────

function resetStats() {
  for (const col of ["json", "nxs"]) {
    stats[col].last = 0;
    stats[col].max = 0;
    stats[col].drops = 0;
    stats[col].frames = 0;
    stats[col].totalTime = 0;
    stats[col].spark = new Array(SPARK_LEN).fill(0);
  }
  prevFrameTs = 0;
  frameIndex = 0;
}

async function start() {
  if (running) {
    // Stop
    running = false;
    cancelAnimationFrame(rafId);
    $("#run").textContent = "Run";
    setStatus("Stopped.", "");
    return;
  }

  if (!reader) {
    try {
      await loadFixtures();
    } catch (err) {
      console.error(err);
      setStatus("Load failed: " + err.message, "error");
      return;
    }
  }

  resetStats();
  running = true;
  $("#run").textContent = "Stop";
  setStatus("Running…", "running");
  prevFrameTs = 0;
  rafId = requestAnimationFrame(frame);
}

$("#run").addEventListener("click", start);

$("#reparse").addEventListener("input", e => {
  $("#reparse-val").textContent = e.target.value;
});
$("#pressure").addEventListener("input", e => {
  $("#pressure-val").textContent = e.target.value;
});

setupLongTaskObserver();

// Pre-load so the first Run click is snappy.
loadFixtures().catch(err => {
  console.error(err);
  setStatus("Load failed: " + err.message, "error");
});
}
