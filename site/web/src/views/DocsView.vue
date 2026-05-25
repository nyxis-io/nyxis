<template>
<main class="page-main">
  <header class="page-header">
    <p class="page-eyebrow">Documentation</p>
    <h1 class="page-title">5-minute quickstart</h1>
    <p class="page-lead">
      Reach first interaction in the browser fast. Nyxis ships as a single ES module — no build step required for the
      MIT JavaScript reader.
    </p>
  </header>

  <section class="landing-section">
    <h2>Install the browser SDK</h2>
    <p class="section-intro">
      Copy <code>nxs.js</code> from
      <a href="https://github.com/nyxis-io/nyxis-drivers/tree/main/js" rel="noopener">nyxis-drivers/js</a>
      or serve it from your app at <code>/sdk/nxs.js</code> (as on this site). Node.js 18+ and modern browsers supported.
    </p>
    <pre class="code-block"># From your app (Vite, Next, etc.)
# vendor copy or alias:
# import { NxsReader } from "./vendor/nxs.js"

# Optional: WASM compile-in-browser (for .nxs source)
# bash nyxis-drivers/js/build_compile_wasm.sh</pre>
  </section>

  <section class="landing-section">
    <h2>Open, filter, render</h2>
    <pre class="code-block"><span class="key">import</span> { NxsReader, NxsStreamReader } from "./nxs.js";

<span class="key">// Open — tail-index only; no full-file parse</span>
const reader = await NxsReader.open("/data/logs.nxb");

<span class="key">// Filter — walk with cursor (or colSumF64 on columnar layout)</span>
const cur = reader.cursor();
let hits = 0;
for (let i = 0; i &lt; reader.recordCount; i++) {
  cur.seek(i);
  if (cur.getF64("score") &gt; 80) hits++;
}

<span class="key">// Render — prefetch viewport for virtual scroll</span>
await reader.prefetch_viewport(0, 49);
for (let i = 0; i &lt; 50; i++) {
  const row = reader.record(i);
  mountRow(i, row.getStr("username"), row.getF64("score"));
}

<span class="key">// Stream — rows before download finishes</span>
const sr = new NxsStreamReader({
  onRecord(obj, idx) { appendRow(idx, obj); },
});
const res = await fetch("/data/stream.nxb");
const body = res.body.getReader();
while (true) {
  const { done, value } = await body.read();
  if (done) break;
  sr.push(value);
}
const sealed = sr.finish();</pre>
  </section>

  <section class="landing-section">
    <h2>Frontend patterns</h2>
    <ul class="pillars">
      <li>
        <strong>React / Vue tables</strong>
        <span>Fixed row pool + <code>prefetch_viewport</code> — see the <a href="/demo/explorer">log explorer</a>.</span>
      </li>
      <li>
        <strong>Virtualized lists</strong>
        <span>Tail-index seeks map scroll position to record index without a million DOM nodes.</span>
      </li>
      <li>
        <strong>Web Workers</strong>
        <span>Search and aggregate off the main thread; hand off via <code>SharedArrayBuffer</code> with COOP/COEP —
          <a href="/demo/workers">workers demo</a>.</span>
      </li>
      <li>
        <strong>Electron &amp; desktop shells</strong>
        <span>Same reader in Node (<code>readFileSync</code>) or Chromium — mmap-friendly cold opens.</span>
      </li>
      <li>
        <strong>Streaming UIs</strong>
        <span><code>NxsStreamReader</code> for live reports and tailing WAL segments before seal.</span>
      </li>
      <li>
        <strong>Browser analytics</strong>
        <span>Columnar <code>.nxb</code> + <code>colSumF64</code> for chart data without JSON reduce loops —
          <a href="/demo/report">report demo</a>.</span>
      </li>
    </ul>
  </section>

  <section class="landing-section">
    <h2>Next steps</h2>
    <div class="landing-actions" style="justify-content: flex-start;">
      <a class="btn btn-primary" href="/demo/explorer">Try the Explorer</a>
      <a class="btn btn-secondary" href="https://github.com/nyxis-io/nyxis/blob/main/SPEC.md" rel="noopener">Specification</a>
      <a class="btn btn-secondary" href="/use-cases/">Production topologies</a>
      <a class="btn btn-secondary" href="/bench/">Browser benchmarks</a>
    </div>
  </section>
</main>
</template>

<script setup lang="ts">
</script>
