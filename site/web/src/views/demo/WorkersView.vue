<template>
<main ref="root" class="page-main page-main--wide interactive-page">
  <header class="page-header">
    <p class="page-eyebrow">NXS · Nyxis · Demo</p>
    <h1 class="page-title">Shared workers</h1>
    <p class="page-lead">Four Web Workers read the same dataset. JSON structured-clones per worker; NXS shares one <code>SharedArrayBuffer</code> when cross-origin isolated.</p>
  </header>

  <section class="what-tested" aria-label="What this page tests">
    <strong>What this page tests</strong>
    Cost of giving <strong>four workers</strong> the same dataset. JSON: main thread parses once, then each worker receives a <strong>structured clone</strong> of the full array — transfer time is the sender-side <code>postMessage</code> blocking time (clone), and total bytes copied scales with workers. NXS: one <code>SharedArrayBuffer</code> backed by the <code>.nxb</code> file when cross-origin isolated; each worker gets a view — handoff is O(1) with no byte copy. Init rows include module load + reader construction. Enable the <strong>writer</strong> checkbox to have worker&nbsp;0 patch record&nbsp;42’s <code>score</code> every 50&nbsp;ms while workers&nbsp;1–3 read it; coherency across workers only appears with a real SAB (the banner explains COOP/COEP and fallbacks).
  </section>

  <div id="iso-banner" class="banner warn">Checking cross-origin isolation…</div>

  <div class="controls">
    <button class="primary" id="start">Start</button>
    <label><input type="checkbox" id="writer-toggle"> Enable writer (worker 0 rewrites record 42's <code>score</code> every 50 ms)</label>
    <span class="status" id="status">Ready.</span>
  </div>

  <div class="columns">
    <section class="card json">
      <h2><span class="dot"></span>JSON — copy-per-worker</h2>
      <p class="desc">Main thread parses JSON once, then <code>postMessage</code>'s the array to each worker. Structured clone runs synchronously on the sender — the main thread blocks until the full copy is done. Transfer time = sender-side <code>postMessage</code> blocking duration (the clone cost).</p>
      <div id="json-workers" class="worker-list"></div>
      <div class="summary-grid">
        <div class="stat"><div class="k">Bytes copied (total)</div><div class="v" id="json-bytes">—</div></div>
        <div class="stat"><div class="k">Transfer time (total)</div><div class="v" id="json-total">—</div></div>
        <div class="stat"><div class="k">Per-worker avg</div><div class="v" id="json-avg">—</div></div>
      </div>
    </section>

    <section class="card nxs">
      <h2><span class="dot"></span>NXS — zero-copy SAB share</h2>
      <p class="desc">One <code>SharedArrayBuffer</code> holds the <code>.nxb</code> bytes. Each worker gets a <code>Uint8Array</code> view over the same memory. Transfer time = sender-side <code>postMessage</code> cost (SAB pointer registration, not a copy). Worker init time includes module load and NxsReader construction.</p>
      <div id="nxs-workers" class="worker-list"></div>
      <div class="summary-grid">
        <div class="stat"><div class="k">Bytes copied (total)</div><div class="v" id="nxs-bytes">0</div></div>
        <div class="stat"><div class="k">Transfer time (total)</div><div class="v" id="nxs-total">—</div></div>
        <div class="stat"><div class="k">Per-worker avg</div><div class="v" id="nxs-avg">—</div></div>
      </div>
      <div class="writer-box">
        <div><span class="lbl">Writer (worker 0) last wrote:</span><span class="val" id="writer-val">idle</span></div>
        <div style="margin-top: 6px;"><span class="lbl">Reader ticks (workers 1-3 reading record 42's score):</span></div>
        <div id="reader-ticks" class="worker-list" style="margin-top: 6px;"></div>
      </div>
    </section>
  </div>

  <section class="card">
    <h2>Summary</h2>
    <div id="summary" class="desc">Run the demo to populate.</div>
  </section>
</main>
</template>

<script setup lang="ts">
import { useDemoPage } from "@/composables/useDemoPage";

useDemoPage(
  async (el) => {
    const { wireWorkersPage } = await import("@/demos/workers-demo");
    wireWorkersPage(el);
  },
  () => {
    import("@/demos/workers-demo").then((m) => m.unwireWorkersPage());
  },
);
</script>
