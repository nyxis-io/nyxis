<template>
<main ref="root" class="page-main page-main--bench interactive-page">
  <header class="page-header">
    <p class="page-eyebrow">NXS · Nyxis</p>
    <h1 class="page-title">Browser benchmark</h1>
    <p class="page-lead">
      Workflow outcomes first — time to interactive, filter latency, stream responsiveness, and browser memory — then
      raw throughput. The same fixtures used in Node and Go benches, run in your browser over <code>fetch()</code>.
    </p>
  </header>

  <section class="landing-section bench-workflow-intro">
    <h2>What we measure first</h2>
    <ul class="pillars">
      <li>
        <strong>Time to interactive</strong>
        <span>Open or stream until the first row is usable — sections 1–5 below.</span>
      </li>
      <li>
        <strong>Browser memory</strong>
        <span><code>performance.memory</code> delta after parse vs <code>NxsReader</code> — section 17.</span>
      </li>
      <li>
        <strong>Filter &amp; scan latency</strong>
        <span>Predicate and aggregate passes — sections 12–16.</span>
      </li>
      <li>
        <strong>Raw throughput</strong>
        <span>Warm random access and WAL reference charts — sections 8–11 and 19.</span>
      </li>
    </ul>
  </section>

  <section class="what-tested" aria-label="What this page tests">
    <strong>What this page tests</strong>
    Eighteen browser workloads plus WAL reference charts. Row <code>.nxb</code> plus <code>records_&lt;N&gt;_columnar.nxb</code> for aggregate comparisons. Use size presets or upload matching fixtures.
  </section>

  <p class="bench-legend" id="bench-legend">
    <strong>pre-parsed</strong> on JSON/CSV bars — full parse happened before the timer.
    <strong>lazy decode</strong> on NXS row bars — reads from <code>.nxb</code> bytes inside the timed region.
    Purple <strong>columnar</strong> bars use <code>records_&lt;N&gt;_columnar.nxb</code> (<code>colSumF64</code>).
  </p>

  <div class="controls">
    <div>
      <label>Size:</label>
      <span class="sizes" id="sizes">
        <button data-n="1000">1,000</button>
        <button data-n="10000" class="active">10,000</button>
        <button data-n="100000">100,000</button>
        <button data-n="1000000">1,000,000</button>
        <button data-n="10000000" title="~1.5 GB JSON — exceeds V8 string limit">10,000,000</button>
      </span>
    </div>
    <button class="primary" id="run">Run benchmark</button>
    <span class="status" id="status">Ready.</span>
  </div>

  <div class="upload-row" id="upload-drop" title="Drop .nxb plus optional matching .json and .csv">
    <span>Upload your own fixtures (same record counts):</span>
    <div class="upload-actions">
      <button type="button" id="pick-suite">Choose files…</button>
      <input type="file" id="suite-files" multiple accept=".nxb,.json,.csv,.NXB,.JSON,.CSV,application/json,text/csv,application/octet-stream" hidden>
      <button type="button" id="clear-upload" hidden>Clear</button>
    </div>
    <span id="upload-summary" class="upload-summary empty">No files selected — using size buttons above.</span>
  </div>

  <div class="sizes-info" id="sizes-info"></div>

  <section class="card card--lead">
    <h2>1. Open file — parse the entire structure</h2>
    <p class="desc">JSON and CSV eagerly parse every byte; NXS reads only the header + tail-index.</p>
    <div class="chart" id="chart-open"></div>
  </section>

  <section class="card card--lead">
    <h2>2. Cold first field — bytes already in memory</h2>
    <p class="desc">Parse/open then read one field at record <code>n/2</code>. Purple prefetch bar: <code>prefetch_viewport(0, min(49,n−1))</code> then <code>cursor(k)</code> (Workload F1 pattern, in-memory).</p>
    <div class="chart" id="chart-cold-mem"></div>
  </section>

  <section class="card card--lead">
    <h2>3. Cold pipeline — fetch + parse + one field</h2>
    <p class="desc">Models the page-load path after <code>fetch()</code>: open or parse, then first field at <code>n/2</code>. Includes the same <code>prefetch_viewport</code> bar as §2.</p>
    <div class="chart" id="chart-cold-fetch"></div>
  </section>

  <section class="card card--lead">
    <h2>4. Cold pipeline — open + sum score</h2>
    <p class="desc">No pre-parsed state: parse entire file then reduce (realistic analytics cold start).</p>
    <div class="chart" id="chart-cold-reduce"></div>
  </section>

  <section class="card card--lead">
    <h2>5. Stream — time to first record</h2>
    <p class="desc">Chunked <code>NxsStreamReader</code> vs <code>JSON.parse</code> until first row is usable.</p>
    <div class="chart" id="chart-stream"></div>
  </section>

  <section class="card">
    <h2>6. Open + iterate all records</h2>
    <p class="desc">End-to-end: open the format, then walk every record and read <code>username</code> once. Includes <code>open + prefetch_viewport(0,49) + scan</code> (virtual-scroll warm path).</p>
    <div class="chart" id="chart-iterate-all"></div>
  </section>

  <section class="card">
    <h2>7. JSON raw scan vs parse + loop</h2>
    <p class="desc">Substring scan for <code>"username":</code> lengths without <code>JSON.parse</code> vs full parse + loop vs NXS scan.</p>
    <div class="chart" id="chart-json-scan"></div>
  </section>

  <section class="card card--warm">
    <h2>8. Iterate only — warm structures <span class="warm-tag">· JSON/CSV pre-parsed</span></h2>
    <p class="desc">Data already parsed/opened; walk every record and read <code>username</code>. Includes a reader with <code>prefetch_viewport(0, min(49,n−1))</code> held warm across iterations.</p>
    <div class="chart" id="chart-iterate-warm"></div>
  </section>

  <section class="card card--warm">
    <h2>9. Random-access read — one field from record k <span class="warm-tag">· JSON/CSV pre-parsed</span></h2>
    <p class="desc">JSON/CSV: property access on a pre-parsed array. NXS: decode from row <code>.nxb</code> — <code>record</code> vs <code>cursor</code> vs <code>buildFieldIndex</code> (optional WASM build).</p>
    <div class="chart" id="chart-random"></div>
  </section>

  <section class="card card--warm">
    <h2>10. Random-access read — multiple fields from record k <span class="warm-tag">· JSON/CSV pre-parsed</span></h2>
    <p class="desc">Four fields per random record. NXS <code>seekWarm(k)</code> amortises the rank cache when reading several fields on the same row.</p>
    <div class="chart" id="chart-random-multi"></div>
  </section>

  <section class="card card--warm">
    <h2>11. Scattered access — strided indices <span class="warm-tag">· JSON/CSV pre-parsed</span></h2>
    <p class="desc">~500 reads at evenly spaced record indices (simulates non-sequential UI paging).</p>
    <div class="chart" id="chart-scattered"></div>
  </section>

  <section class="card card--warm">
    <h2>12. Full scan — four fields per record</h2>
    <p class="desc">Open + linear pass reading username, age, balance, active on every row. NXS uses zero-alloc <code>cursor.scan</code> (not <code>seekWarm</code> per row).</p>
    <div class="chart" id="chart-multi-scan"></div>
  </section>

  <section class="card card--warm">
    <h2>13. Filter — count where score &gt; 80 <span class="warm-tag">· JSON/CSV pre-parsed</span></h2>
    <p class="desc">Predicate over all records: JSON loop vs NXS <code>cursor.seek</code> + <code>getF64BySlot</code>. Row layout walks the bitmask per field — use columnar layout for filter-heavy analytics.</p>
    <div class="chart" id="chart-filter"></div>
  </section>

  <section class="card card--warm">
    <h2>14. Aggregate — sum of 'score' (warm) <span class="warm-tag">· JSON/CSV pre-parsed</span></h2>
    <p class="desc">Pre-parsed JSON loop, raw CSV column scan, row <code>sumF64</code> (WASM when loaded), and columnar <code>colSumF64</code> when the columnar fixture is present.</p>
    <div class="chart" id="chart-reduce"></div>
  </section>

  <section class="card card--warm">
    <h2>15. Indexed sum vs column reducer <span class="warm-tag">· row vs columnar layout</span></h2>
    <p class="desc">Row: <code>buildFieldIndex("score")</code> + loop vs <code>sumF64</code>. Columnar: dense <code>colSumF64</code> (SIMD path in native drivers).</p>
    <div class="chart" id="chart-indexed-sum"></div>
  </section>

  <section class="card card--warm" id="section-column-prefetch">
    <h2>16. Columnar aggregate vs JSON <span class="warm-tag">· §7.4 / analytics headline</span></h2>
    <p class="desc">
      Sum <code>score</code> at the same record count: JSON warm <code>(pre-parsed)</code> vs JSON cold (parse each iteration) vs NXS columnar cold, mistaken prefetch (new reader each time), and persistent reader with <code>prefetch_column</code> once.
      Requires <code>records_&lt;N&gt;_columnar.nxb</code> and matching JSON.
    </p>
    <div class="chart" id="chart-column-prefetch"></div>
    <p class="bench-footnote" id="column-prefetch-footnote">
      NXS cold (new reader each iteration) beats pre-parsed warm JSON by ~10% without any warmup.
      NXS warm (persistent reader + <code>prefetch_column</code>) beats pre-parsed warm JSON by ~45%.
      The <em>prefetch_column + sum (new reader)</em> bar shows the cost of calling prefetch without retaining the reader — avoid that pattern.
    </p>
  </section>

  <section class="card">
    <h2>17. Memory — heap growth (Chrome)</h2>
    <p class="desc">Indicative <code>performance.memory</code> delta after parse vs <code>NxsReader</code> (not file size on disk).</p>
    <div class="sizes-info" id="memory-info"></div>
    <div class="chart" id="chart-memory"></div>
  </section>

  <section class="card">
    <h2>18. Workers — parallel sum score</h2>
    <p class="desc">Main-thread <code>sumF64</code> vs four module workers each summing a chunk (buffer copy per worker).</p>
    <div class="chart" id="chart-worker"></div>
  </section>

  <section class="card" id="wal-section">
    <h2>19. WAL span ingestion — append · recover · seal · roundtrip</h2>
    <p class="desc">Rust <code>cargo run --release --bin bench</code> on Apple M-series (tmpfs I/O for append). Reference charts below; values match <a href="https://github.com/nyxis-io/nyxis/blob/main/BENCHMARK.md">BENCHMARK.md</a>. Append is amortised <strong>append-batch</strong> (encode + write all spans per run).</p>

    <div style="margin-bottom:14px">
      <label style="color:var(--muted);font-size:13px;margin-right:8px">Span count:</label>
      <span class="sizes" id="wal-sizes">
        <button data-n="1000" class="active">1,000</button>
        <button data-n="10000">10,000</button>
        <button data-n="100000">100,000</button>
        <button data-n="1000000">1,000,000</button>
        <button data-n="10000000">10,000,000</button>
        <button data-n="100000000">100,000,000</button>
      </span>
    </div>

    <div style="margin-bottom:18px">
      <div class="sizes-info" id="wal-sizes-info"></div>
    </div>

    <h3 style="font-size:13px;font-weight:600;margin:0 0 8px;color:var(--muted)">Timings (ns / span, lower is better)</h3>
    <div class="chart" id="chart-wal-timing"></div>

    <h3 style="font-size:13px;font-weight:600;margin:16px 0 8px;color:var(--muted)">File size vs JSON NDJSON baseline</h3>
    <div class="chart" id="chart-wal-size"></div>
  </section>

  <section class="card bench-tradeoffs">
    <h2>Honest tradeoffs</h2>
    <p class="desc">
      Nyxis is not a drop-in replacement for your entire data stack. Published comparisons acknowledge where other
      formats win.
    </p>
    <ul>
      <li><strong>Apache Arrow</strong> remains superior for dense analytical scans — use NXS <code>columnar</code> layout or the Arrow bridge for hybrid pipelines.</li>
      <li><strong>FlatBuffers / Cap'n Proto</strong> may cold-open faster on tiny warm-cache files — see <a href="../BENCHMARK.md#workload-b">Workload B</a>.</li>
      <li><strong>Parquet</strong> remains excellent warehouse storage — NXS targets transport, browser UIs, and streaming ingest.</li>
      <li><strong>JSON (warm)</strong> can tie NXS on simple field access when the entire document is already parsed — bars label <strong>pre-parsed</strong> explicitly.</li>
    </ul>
  </section>

  <section class="card bench-repro">
    <h2>Reproducibility</h2>
    <p class="desc">Re-run the same workloads locally or in CI.</p>
    <ul>
      <li><strong>Public fixtures</strong> — <code>nyxis/site/bench/fixtures/records_*.nxb</code> (+ matching <code>.json</code> / <code>.csv</code>)</li>
      <li><strong>Browser harness</strong> — this page (<code>site/web/src/demos/bench-page.js</code>)</li>
      <li><strong>Native benches</strong> — <code>cargo run --release --bin bench</code> in <a href="https://github.com/nyxis-io/nyxis" rel="noopener">nyxis</a></li>
      <li><strong>Methodology &amp; hardware</strong> — <a href="../BENCHMARK.md#workload-comparison-suite">BENCHMARK.md</a> (Apple Silicon dev hosts, Linux x86_64, EPYC AVX-512 rows)</li>
      <li><strong>Generate fixtures</strong> — <code>make -C nyxis bench-fixtures</code> (see repo <code>Makefile</code>)</li>
    </ul>
  </section>

  <section class="card" id="wal-lang-section">
    <h2>19. WAL encoder — cross-language comparison</h2>
    <p class="desc">Per-span append throughput across all ten implementations, measured at 10,000 spans on Apple M-series (arm64). NXS encodes directly to the binary format; JSON baseline uses each language's standard library serialiser. JS numbers are from the live benchmark above.</p>

    <div id="chart-wal-lang-nxs" class="chart" style="margin-bottom:20px"></div>
    <div id="chart-wal-lang-json" class="chart"></div>
  </section>

</main>
</template>

<script setup lang="ts">
import { useDemoPage } from "@/composables/useDemoPage";

useDemoPage(
  async (el) => {
    const { wireBenchPage } = await import("@/demos/bench-page");
    await wireBenchPage(el);
  },
  () => {
    import("@/demos/bench-page").then((m) => m.unwireBenchPage());
  },
);
</script>
