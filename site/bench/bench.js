// NXS vs JSON vs CSV benchmark — Node.js
//
// Four scenarios (same across formats):
//   1. Open file  — parse the entire structure
//   2. Random access — read 1 field from 1 record
//   3. Cold start — open + read 1 field
//   4. Full scan — sum the 'score' field across all records
//
// Usage: node bench.js <fixtures_dir>

import { readFileSync, openSync, fstatSync, readSync, closeSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { performance } from "node:perf_hooks";

const __benchDir = dirname(fileURLToPath(import.meta.url));
const WASM_PATH = join(__benchDir, "wasm/nxs_reducers.wasm");
import { NxsReader } from "../../../nyxis-drivers/js/nxs.js";
import { loadWasm } from "../../../nyxis-drivers/js/wasm.js";

// Synchronous zero-copy helper for the benchmark harness.
function readNxbIntoWasmSync(wasm, path) {
  const fd = openSync(path, "r");
  try {
    const size = fstatSync(fd).size;
    const buf = wasm.allocBuffer(size);
    readSync(fd, buf, 0, size, 0);
    return buf;
  } finally {
    closeSync(fd);
  }
}

// ── Timing harness ──────────────────────────────────────────────────────────

function bench(iters, fn) {
  for (let i = 0; i < Math.max(3, iters / 10 | 0); i++) fn(); // warmup
  const start = performance.now();
  for (let i = 0; i < iters; i++) fn();
  const elapsed = performance.now() - start;
  return { iters, totalMs: elapsed, perIter: elapsed / iters };
}

const fmtTime = ms =>
  ms < 0.001 ? `${(ms * 1e6).toFixed(0)} ns`
  : ms < 1    ? `${(ms * 1e3).toFixed(1)} µs`
  : ms < 1000 ? `${ms.toFixed(2)} ms`
              : `${(ms / 1000).toFixed(2)} s`;

const fmtBytes = n =>
  n < 1024        ? `${n} B`
  : n < 1048576   ? `${(n / 1024).toFixed(1)} KB`
                  : `${(n / 1048576).toFixed(2)} MB`;

function row(label, result, baseline) {
  const ratio = result.perIter / baseline.perIter;
  const ratioStr = baseline === result
    ? "baseline"
    : ratio < 1 ? `${(1/ratio).toFixed(1)}x faster`
                : `${ratio.toFixed(1)}x slower`;
  console.log(`  │  ${label.padEnd(44)} ${fmtTime(result.perIter).padStart(10)}   ${ratioStr}`);
}

function header(title) {
  console.log(`\n  ┌─ ${title} ${"─".repeat(Math.max(0, 74 - title.length))}┐`);
}
function footer() { console.log(`  └${"─".repeat(77)}┘`); }

// ── CSV parser (minimal — matches our fixture's no-escape, no-quote shape) ──

// Parse the whole CSV into Array<Object>. This is the equivalent of JSON.parse.
function parseCsv(str) {
  const lines = str.split("\n");
  const headers = lines[0].split(",");
  const nCols = headers.length;
  const result = new Array(lines.length - 2); // last line is empty due to trailing \n
  for (let i = 1; i < lines.length; i++) {
    const line = lines[i];
    if (line.length === 0) continue;
    const row = {};
    let start = 0, col = 0;
    for (let j = 0; j <= line.length && col < nCols; j++) {
      if (j === line.length || line.charCodeAt(j) === 44 /* , */) {
        row[headers[col]] = line.slice(start, j);
        col++;
        start = j + 1;
      }
    }
    result[i - 1] = row;
  }
  return result;
}

// For fair comparison on "random access": a *pre-parsed* CSV is an
// Array<Object> just like JSON. For "cold start" we must reparse each call.

// Typed sum for CSV 'score' column — tight scan of the score field only.
// This is what you'd actually write for an aggregate without full parse.
function sumCsvScore(str) {
  let p = str.indexOf("\n") + 1; // skip header
  let sum = 0;
  const len = str.length;
  while (p < len) {
    // find 6th comma on the line (id, username, email, age, balance, active, score)
    let commas = 0;
    let scoreStart = p;
    while (p < len) {
      const c = str.charCodeAt(p);
      if (c === 44 /* , */) {
        commas++;
        if (commas === 6) scoreStart = p + 1;
        else if (commas === 7) { // past score
          sum += +str.slice(scoreStart, p);
          // skip to end of line
          while (p < len && str.charCodeAt(p) !== 10 /* \n */) p++;
          p++;
          break;
        }
      }
      p++;
    }
  }
  return sum;
}

// ── Benchmark scenarios ─────────────────────────────────────────────────────

async function runScale(fixtureDir, n, wasm) {
  const nxbBuf  = readFileSync(join(fixtureDir, `records_${n}.nxb`));
  const jsonStr = readFileSync(join(fixtureDir, `records_${n}.json`), "utf8");
  const csvStr  = readFileSync(join(fixtureDir, `records_${n}.csv`),  "utf8");

  console.log(`\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  n = ${n.toLocaleString()}  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━`);
  console.log(`  .nxb  ${fmtBytes(nxbBuf.length).padStart(10)}   (${(nxbBuf.length  / jsonStr.length * 100).toFixed(0)}% of JSON)`);
  console.log(`  .json ${fmtBytes(jsonStr.length).padStart(10)}   (100% of JSON)`);
  console.log(`  .csv  ${fmtBytes(csvStr.length).padStart(10)}   (${(csvStr.length / jsonStr.length * 100).toFixed(0)}% of JSON)`);

  const parseIters  = n >= 1_000_000 ? 20 : n >= 100_000 ? 100 : 1000;
  const randomIters = 100_000;
  const coldIters   = n >= 1_000_000 ? 10 : n >= 100_000 ? 50 : 500;
  const iterateIters = n >= 1_000_000 ? 5 : n >= 100_000 ? 20 : 500;

  // Pre-parse (for random-access/warm scenarios)
  const parsedJson = JSON.parse(jsonStr);
  const parsedCsv  = parseCsv(csvStr);
  const reader     = new NxsReader(nxbBuf);
  const wasmReader = new NxsReader(nxbBuf);
  wasmReader.useWasm(wasm);

  // ── 1. Open file ─────────────────────────────────────────────────────────
  header("Open file (parse full structure)");
  const jsonOpen = bench(parseIters, () => JSON.parse(jsonStr));
  const csvOpen  = bench(parseIters, () => parseCsv(csvStr));
  const nxsOpen  = bench(parseIters, () => new NxsReader(nxbBuf));
  row("JSON.parse(entire document)", jsonOpen, jsonOpen);
  row("parseCsv(entire document)",   csvOpen,  jsonOpen);
  row("new NxsReader(buffer)",       nxsOpen,  jsonOpen);
  footer();

  // ── 2. Warm random access ────────────────────────────────────────────────
  header("Random-access read (1 field from 1 record)");
  const idxs = new Array(randomIters);
  for (let i = 0; i < randomIters; i++) idxs[i] = Math.floor(Math.random() * n);
  let ii = 0;
  const jsonRand = bench(randomIters, () => parsedJson[idxs[ii++ % randomIters]].username);
  ii = 0;
  const csvRand  = bench(randomIters, () => parsedCsv[idxs[ii++ % randomIters]].username);
  ii = 0;
  const nxsRand  = bench(randomIters, () => reader.record(idxs[ii++ % randomIters]).getStr("username"));
  // NXS with precomputed slot handle — skips the per-call Map lookup.
  const usernameSlot = reader.slot("username");
  ii = 0;
  const nxsRandSlot = bench(randomIters, () => reader.record(idxs[ii++ % randomIters]).getStrBySlot(usernameSlot));
  row("arr[k].username  (pre-parsed JSON)",    jsonRand,     jsonRand);
  row("arr[k].username  (pre-parsed CSV)",     csvRand,      jsonRand);
  row("reader.record(k).getStr('username')",   nxsRand,      jsonRand);
  row("reader.record(k).getStrBySlot(slot)",   nxsRandSlot,  jsonRand);
  footer();

  // ── 3. Cold start ────────────────────────────────────────────────────────
  header("First access — open + read 1 field (cold start)");
  const k = Math.floor(n / 2);
  const jsonCold = bench(coldIters, () => JSON.parse(jsonStr)[k].username);
  const csvCold  = bench(coldIters, () => parseCsv(csvStr)[k].username);
  const nxsCold  = bench(coldIters, () => new NxsReader(nxbBuf).record(k).getStr("username"));
  row("JSON.parse + arr[k].username",   jsonCold, jsonCold);
  row("parseCsv + arr[k].username",     csvCold,  jsonCold);
  row("new NxsReader + record(k)...",   nxsCold,  jsonCold);
  footer();

  // ── 4. Full scan (per-record) ────────────────────────────────────────────
  header("Full scan — sum of 'score' (per-record API)");
  const jsonIter = bench(iterateIters, () => {
    let sum = 0;
    for (const r of parsedJson) sum += r.score;
    return sum;
  });
  const csvIter = bench(iterateIters, () => {
    let sum = 0;
    for (const r of parsedCsv) sum += +r.score;
    return sum;
  });
  const nxsIter = bench(iterateIters, () => {
    let sum = 0;
    for (const r of reader.records()) sum += r.getF64("score");
    return sum;
  });
  const scoreSlot = reader.slot("score");
  const nxsIterSlot = bench(iterateIters, () => {
    let sum = 0;
    const rc = reader.recordCount;
    for (let i = 0; i < rc; i++) sum += reader.record(i).getF64BySlot(scoreSlot);
    return sum;
  });
  // Cursor-based scan: reuses one NxsCursor across all records
  const nxsIterCursor = bench(iterateIters, () => {
    let sum = 0;
    reader.scan(cur => { sum += cur.getF64BySlot(scoreSlot); });
    return sum;
  });
  row("for (r of arr) s += r.score  (JSON)",      jsonIter,      jsonIter);
  row("for (r of arr) s += +r.score (CSV)",        csvIter,       jsonIter);
  row("NXS per-record (by key name)",              nxsIter,       jsonIter);
  row("NXS per-record (by slot handle)",           nxsIterSlot,   jsonIter);
  row("NXS scan(cursor)  (zero-alloc)",            nxsIterCursor, jsonIter);
  footer();

  // ── 5. Columnar / reducer ────────────────────────────────────────────────
  header("Columnar scan — same sum, using bulk APIs");
  const jsonBulk = bench(iterateIters, () => {
    let sum = 0;
    for (const r of parsedJson) sum += r.score;
    return sum;
  });
  const csvBulk  = bench(iterateIters, () => sumCsvScore(csvStr));
  const nxsBulk  = bench(iterateIters, () => reader.sumF64("score"));
  const nxsBulkWasm = bench(iterateIters, () => wasmReader.sumF64("score"));
  row("JSON baseline (re-measured)",            jsonBulk,     jsonBulk);
  row("sumCsvScore(raw string) [cold scan]",    csvBulk,      jsonBulk);
  row("reader.sumF64('score')  [in-JS red.]",   nxsBulk,      jsonBulk);
  row("reader.sumF64('score')  [WASM red.]",    nxsBulkWasm,  jsonBulk);
  footer();

  // ── 6. Cold reducer pipeline: read bytes → parse → reduce ────────────────
  // This is the realistic workflow. JSON/CSV must fully parse before reducing;
  // NXS-WASM with zero-copy reads file bytes straight into WASM memory.
  header("Cold pipeline — open file + reduce (no pre-parsed state)");
  const coldReduceIters = n >= 1_000_000 ? 3 : n >= 100_000 ? 10 : 100;
  const nxbPath  = join(fixtureDir, `records_${n}.nxb`);
  const jsonPath = join(fixtureDir, `records_${n}.json`);
  const csvPath  = join(fixtureDir, `records_${n}.csv`);

  const jsonReduce = bench(coldReduceIters, () => {
    const s = readFileSync(jsonPath, "utf8");
    let sum = 0;
    for (const r of JSON.parse(s)) sum += r.score;
    return sum;
  });
  const csvReduce = bench(coldReduceIters, () => {
    return sumCsvScore(readFileSync(csvPath, "utf8"));
  });
  const nxsCopyReduce = bench(coldReduceIters, () => {
    const buf = readFileSync(nxbPath);
    const r = new NxsReader(buf);
    r.useWasm(wasm);
    return r.sumF64("score");
  });
  const nxsZeroCopyReduce = bench(coldReduceIters, () => {
    const buf = readNxbIntoWasmSync(wasm, nxbPath);
    const r = new NxsReader(buf);
    r.useWasm(wasm);
    return r.sumF64("score");
  });
  row("JSON: readFile + parse + loop",                     jsonReduce,         jsonReduce);
  row("CSV:  readFile + sumCsvScore",                      csvReduce,          jsonReduce);
  row("NXS-WASM copy-in (readFile + useWasm copies)",      nxsCopyReduce,      jsonReduce);
  row("NXS-WASM zero-copy (readNxbIntoWasm)",              nxsZeroCopyReduce,  jsonReduce);
  footer();
}

// ── Main ────────────────────────────────────────────────────────────────────

async function main() {
  const fixtureDir = process.argv[2];
  if (!fixtureDir) {
    console.error("Usage: node bench.js <fixtures_dir>");
    process.exit(1);
  }

  console.log("\n╔════════════════════════════════════════════════════════════════════════════════╗");
  console.log("║        NXS vs JSON vs CSV  —  JavaScript (Node.js) Benchmark                  ║");
  console.log("╚════════════════════════════════════════════════════════════════════════════════╝");
  console.log(`\n  Node:     ${process.version}`);
  console.log(`  Platform: ${process.platform} ${process.arch}`);
  console.log(`  Fixtures: ${fixtureDir}`);

  // Load the WASM accelerator once — grow to accommodate the largest fixture.
  const wasm = await loadWasm(WASM_PATH, { initialPages: 2200 }); // ~140 MB
  console.log(`  WASM:     loaded (reducers: sum_f64, sum_i64, min_f64, max_f64)`);

  for (const n of [1_000, 10_000, 100_000, 1_000_000]) {
    try { await runScale(fixtureDir, n, wasm); }
    catch (e) { console.log(`\n  ⚠  n=${n} skipped: ${e.message}`); }
  }

  console.log("\n" + "═".repeat(80));
  console.log("  Notes:");
  console.log("    • JSON.parse: V8 built-in (C++), highly optimised.");
  console.log("    • parseCsv: minimal hand-rolled JS parser (no escaping, no quotes).");
  console.log("    • NxsReader: pure JS, zero-copy, tail-indexed.");
  console.log("    • sumCsvScore scans the raw CSV string column-only (no full parse).\n");
}

main();
