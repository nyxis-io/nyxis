<template>
<main ref="root" class="page-main page-main--wide interactive-page">
  <header class="page-header">
    <p class="page-eyebrow">NXS · Nyxis · Demo</p>
    <h1 class="page-title">WAL / span ingestion</h1>
    <p class="page-lead">Live comparison: NXS generic encoder, fast-path, sealed, WASM, and JSON NDJSON for OTel-style spans.</p>
  </header>

  <div class="controls">
    <label>Spans:</label>
    <span class="sizes" id="wal-sizes">
      <button data-n="1000"   class="active">1,000</button>
      <button data-n="10000"             >10,000</button>
      <button data-n="100000"            >100,000</button>
    </span>
    <button class="run-btn" id="run-btn" disabled>Loading WASM…</button>
    <span class="status-line" id="status"></span>
  </div>
  <div class="progress-wrap" id="pwrap"><div class="progress-inner" id="pbar"></div></div>

  <!-- Scorecards -->
  <div class="scorecards" id="scorecards">
    <div class="scorecard nxs-wal"  id="sc-nxs-wal">
      <div class="sc-label">NXS WAL</div>
      <div class="sc-tput sc-placeholder">—</div>
      <div class="sc-sub">click Run benchmark</div>
    </div>
    <div class="scorecard nxs-fast" id="sc-nxs-fast">
      <div class="sc-label">NXS Fast</div>
      <div class="sc-tput sc-placeholder">—</div>
      <div class="sc-sub">click Run benchmark</div>
    </div>
    <div class="scorecard nxs-seal" id="sc-nxs-seal">
      <div class="sc-label">NXS Sealed</div>
      <div class="sc-tput sc-placeholder">—</div>
      <div class="sc-sub">click Run benchmark</div>
    </div>
    <div class="scorecard nxs-wasm" id="sc-nxs-wasm">
      <div class="sc-label">NXS WASM</div>
      <div class="sc-tput sc-placeholder">—</div>
      <div class="sc-sub">click Run benchmark</div>
    </div>
    <div class="scorecard json-nd"  id="sc-json">
      <div class="sc-label">JSON NDJSON</div>
      <div class="sc-tput sc-placeholder">—</div>
      <div class="sc-sub">click Run benchmark</div>
    </div>
  </div>

  <!-- Encoder explanations -->
  <section class="card">
    <h2>What each encoder does</h2>
    <p class="desc">Five strategies, same output format — same binary bytes on disk.</p>
    <table class="detail">
      <thead><tr><th>Encoder</th><th>Strategy</th><th>Why it matters</th></tr></thead>
      <tbody>
        <tr>
          <td><span class="tag nxs">NXS WAL</span></td>
          <td>One <code>NxsWriter</code> per span. Each call to <code>finish()</code> allocates a chunk array, merges it, and wraps the record in a full preamble + schema + tail-index.</td>
          <td>Matches a real WAL append where each span is an independent object written to a rolling log. Slow because of per-span allocations and BigInt arithmetic.</td>
        </tr>
        <tr>
          <td><span class="tag fast">NXS Fast</span></td>
          <td>A single pre-allocated 128-byte <code>Uint8Array</code> is reused for every span. Fields are written with <code>DataView.setUint32</code> (two calls per i64 — no BigInt). Strings are pre-encoded to <code>Uint8Array</code> once at startup. Only variable offsets are patched per span.</td>
          <td>Shows the theoretical ceiling of NXS binary encoding in JS: eliminate GC pressure and BigInt. This is what a production WAL writer would look like — fixed schema, typed views, no dynamic allocation in the hot loop.</td>
        </tr>
        <tr>
          <td><span class="tag seal">NXS Sealed</span></td>
          <td>All spans written into one shared <code>NxsWriter</code>, then <code>finish()</code> called once. Produces a single <code>.nxb</code> file with one schema header, one preamble, and one tail-index covering all records.</td>
          <td>The batch/seal path used after WAL rotation. <code>finish()</code> triggers a single large <code>_materialize()</code> merge — fast for the tail-index write but re-encodes every span with BigInt, so throughput is similar to WAL.</td>
        </tr>
        <tr>
          <td><span class="tag wasm">NXS WASM</span></td>
          <td>A <code>WasmSpanWriter</code> fills a 72-byte input struct in WASM memory via <code>DataView</code>, then calls <code>encode_span(outPtr, fieldsPtr)</code> — a freestanding C function compiled to WASM. Strings are written directly into WASM memory with <code>TextEncoder.encodeInto()</code>. Zero JS allocations per span.</td>
          <td>Native WASM struct packing with no JS BigInt, no GC pressure, and no per-call memory allocation. The output bytes are a zero-copy view into WASM linear memory. Comparable to NXS Fast — WASM call overhead is the main cost.</td>
        </tr>
        <tr>
          <td><span class="tag json">JSON NDJSON</span></td>
          <td>One <code>JSON.stringify</code> per span, newline-delimited. IDs are serialised as decimal strings (BigInt cannot be JSON-serialised natively).</td>
          <td>V8's <code>JSON.stringify</code> is implemented in C++ and highly optimised — it wins on throughput for small objects. But it produces <strong>~2× more bytes</strong> because every field name is repeated as a UTF-8 string in every record.</td>
        </tr>
      </tbody>
    </table>
  </section>

  <!-- Throughput chart -->
  <section class="card">
    <h2>Throughput — spans / second (higher is better)</h2>
    <p class="desc">How many spans each format encodes per second in this browser tab.</p>
    <div class="cmp-chart" id="chart-tput">
      <div class="lbl">NXS WAL</div>    <div class="track"><div class="bar placeholder nxs-wal"></div></div>   <div class="val">—</div>
      <div class="lbl">NXS Fast</div>   <div class="track"><div class="bar placeholder nxs-fast"></div></div>  <div class="val">—</div>
      <div class="lbl">NXS Sealed</div> <div class="track"><div class="bar placeholder nxs-seal"></div></div>  <div class="val">—</div>
      <div class="lbl">NXS WASM</div>   <div class="track"><div class="bar placeholder nxs-wasm"></div></div>  <div class="val">—</div>
      <div class="lbl">JSON NDJSON</div><div class="track"><div class="bar placeholder json-nd"></div></div>   <div class="val">—</div>
    </div>
  </section>

  <!-- Size chart -->
  <section class="card">
    <h2>Output size (smaller is better)</h2>
    <p class="desc">Total bytes produced for all N spans. NXS omits field names and stores numbers in 8-byte binary; JSON repeats every key as a string.</p>
    <div class="cmp-chart" id="chart-size">
      <div class="lbl">NXS WAL</div>    <div class="track"><div class="bar placeholder nxs-wal"></div></div>   <div class="val">—</div>
      <div class="lbl">NXS Fast</div>   <div class="track"><div class="bar placeholder nxs-fast"></div></div>  <div class="val">—</div>
      <div class="lbl">NXS Sealed</div> <div class="track"><div class="bar placeholder nxs-seal"></div></div>  <div class="val">—</div>
      <div class="lbl">NXS WASM</div>   <div class="track"><div class="bar placeholder nxs-wasm"></div></div>  <div class="val">—</div>
      <div class="lbl">JSON NDJSON</div><div class="track"><div class="bar placeholder json-nd"></div></div>   <div class="val">—</div>
    </div>
  </section>

  <!-- Detail table -->
  <section class="card">
    <h2>Detail</h2>
    <p class="desc">Per-span cost and aggregate metrics for the current run.</p>
    <table class="detail" id="detail-table">
      <thead>
        <tr>
          <th>Format</th>
          <th class="num">ns / span</th>
          <th class="num">spans / sec</th>
          <th class="num">total time</th>
          <th class="num">output size</th>
          <th class="num">bytes / span</th>
          <th class="num">vs JSON size</th>
        </tr>
      </thead>
      <tbody id="detail-body">
        <tr><td colspan="7" style="color:var(--muted);padding:12px 0">Run the benchmark to see results.</td></tr>
      </tbody>
    </table>
    <div class="note" id="detail-note">NXS WAL encodes each span independently (append path); NXS Sealed writes all spans then calls <code>finish()</code> once (batch path). NXS WASM calls a native C function compiled to WebAssembly. JSON uses <code>JSON.stringify</code> per span.</div>
  </section>

  <!-- Cross-language comparison -->
  <section class="card" id="lang-cmp-section">
    <h2>Cross-language WAL comparison</h2>
    <p class="desc">NXS WAL append throughput vs each language's standard JSON serialiser — measured at 10,000 spans, Apple M-series (arm64). JS bars update live after each run above.</p>

    <h3 style="font-size:12px;font-weight:600;margin:0 0 8px;color:var(--muted);text-transform:uppercase;letter-spacing:.05em">NXS WAL ns/span (lower is better)</h3>
    <div class="cmp-chart" id="lang-nxs-chart" style="margin-bottom:20px">
      <!-- filled by renderLangComparison() -->
      <div class="lbl">C</div>            <div class="track"><div class="bar placeholder" style="background:#64748b"></div></div>  <div class="val">run above first</div>
    </div>

    <h3 style="font-size:12px;font-weight:600;margin:0 0 8px;color:var(--muted);text-transform:uppercase;letter-spacing:.05em">JSON ns/span (lower is better)</h3>
    <div class="cmp-chart" id="lang-json-chart">
      <div class="lbl">C</div>            <div class="track"><div class="bar placeholder" style="background:#64748b"></div></div>  <div class="val">run above first</div>
    </div>

    <div class="note" style="margin-top:14px">
      Reference numbers from <code>bench_wal.c</code>, <code>bench_wal_test.go</code>, <code>bench_wal.py</code>, <code>ruby/bench_wal.rb</code>, and <code>cargo run --release --bin bench</code>.
      Rust has no stdlib JSON encoder — only NXS append is shown.
      JS numbers update live from this browser tab when you run the benchmark.
    </div>
  </section>

  <footer>
    Benchmark runs entirely in this browser tab — no network, no disk I/O.
    NXS encoders: <code>/sdk/nxs_writer.js</code> (generic/sealed), hand-rolled DataView (fast), <code>/sdk/wasm.js</code> + <code>/bench/wasm/nxs_reducers.wasm</code> (WASM). JSON via built-in <code>JSON.stringify</code>.
    Timer: <code>performance.now()</code> (µs resolution). Results vary with browser, hardware, and JIT warm-up state.
  </footer>
</main>
</template>

<script setup lang="ts">
import { useDemoPage } from "@/composables/useDemoPage";

useDemoPage(
  async (el) => {
    const { wireWalPage } = await import("@/demos/wal-demo");
    await wireWalPage(el);
  },
  () => {
    import("@/demos/wal-demo").then((m) => m.unwireWalPage());
  },
);
</script>
