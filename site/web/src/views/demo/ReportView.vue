<template>
<main ref="root" class="page-main interactive-page page-main--report">
  <header class="page-header">
    <p class="page-eyebrow">Layout demo · not a benchmark</p>
    <h1 class="page-title">Your report data, row vs columnar</h1>
    <p class="page-lead">
      Upload or paste CSV (header row required). The demo transcodes to row and columnar
      <code>.nxb</code> in your browser, runs three access patterns on <em>your</em> workload, and renders a chart from
      <code>col_buffer()</code> — no separate benchmark harness.
    </p>
  </header>

  <p class="what-this">
    <strong>What you should notice:</strong> columnar <code>sum</code> stays fast on large numeric columns; row layout
    wins for the first record and random row access; the chart is built from mmap-style column buffers, not
    <code>JSON.parse</code>. Numbers are side effects of doing real work — see
    <a href="https://github.com/nyxis-io/nyxis/blob/main/BENCHMARK.md#workload-comparison-suite">BENCHMARK.md</a> for frozen cross-format tables.
  </p>

  <div class="panel">
    <h2>Data</h2>
    <div class="input-row">
      <select id="sample-dataset" aria-label="Sample dataset">
        <option value="sales">Sales (100k)</option>
        <option value="logs">Access logs (100k)</option>
        <option value="sparse">Sparse numeric (100k)</option>
      </select>
      <span id="sample-desc" class="hint"></span>
    </div>
    <div class="input-row">
      <input type="file" id="csv-file" accept=".csv,text/csv,text/plain" />
      <button type="button" class="primary" id="run-demo">Run on sample / paste</button>
    </div>
    <label for="csv-paste" class="hint">Or paste CSV (overrides sample when non-empty)</label>
    <textarea id="csv-paste" placeholder="region,product,revenue,units&#10;North,Widget,120.5,3&#10;..."></textarea>
    <p id="status">Choose a sample or upload CSV, then run.</p>
  </div>

  <div class="report-grid">
    <div class="panel">
      <h2>Operations on your file</h2>
      <table class="ops-table" aria-describedby="status">
        <thead>
          <tr>
            <th scope="col">Operation</th>
            <th scope="col">Row layout</th>
            <th scope="col">Columnar layout</th>
          </tr>
        </thead>
        <tbody id="ops-body">
          <tr><td colspan="3" class="muted">Run the demo to fill timings.</td></tr>
        </tbody>
      </table>
    </div>
    <div class="panel">
      <h2>File sizes</h2>
      <ul id="file-sizes">
        <li class="muted">Waiting for run…</li>
      </ul>
      <p class="hint" style="margin-top: 12px;">
        Columnar compile uses WASM (<code>compile_nxs_columnar</code>); text columns stay in row layout only.
        Numeric types inferred from the header row (<code>~</code> float, <code>=</code> int, <code>?</code> bool).
      </p>
    </div>
    <div class="panel chart-wrap">
      <h2>Chart</h2>
      <canvas id="report-chart" aria-label="Chart from columnar buffers"></canvas>
    </div>
  </div>
</main>
</template>

<script setup lang="ts">
import { useDemoPage } from "@/composables/useDemoPage";

useDemoPage(async (el) => {
  const { wireReportPage } = await import("@/demos/report");
  wireReportPage(el);
});
</script>
