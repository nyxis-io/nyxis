<template>
<main ref="root" class="page-main page-main--wide interactive-page">
  <header class="page-header">
    <p class="page-eyebrow">NXS · Nyxis · Demo</p>
    <h1 class="page-title">Live ticker</h1>
    <p class="page-lead">A 60 FPS rAF loop updates record 0's <code>score</code> every frame on both paths. JSON re-parses the whole array every K frames (simulating a server push). NXS patches 8 bytes in place.</p>
  </header>

  <section class="what-tested" aria-label="What this page tests">
    <strong>What this page tests</strong>
    Main-thread frame budgeting under a continuous <code>requestAnimationFrame</code> loop. Both columns update the same logical field (<code>record[0].score</code>) each frame; <strong>pressure</strong> repeats that work multiple times per frame. The JSON column mutates a parsed array and, every <em>K</em> frames, <code>JSON.stringify</code>s and <code>JSON.parse</code>s the full ~15&nbsp;MB document (simulating a replaced server payload). The NXS column writes via <code>DataView.setFloat64</code> at a cached byte offset into mapped <code>.nxb</code> bytes — no per-frame allocations. The stats and sparklines show FPS, per-path work time, and frames over a 20&nbsp;ms budget; the <strong>Long tasks</strong> panel lists browser <code>longtask</code> entries (&gt;50&nbsp;ms), which usually spike on the JSON path when re-parse aligns with rAF.
  </section>

  <div class="controls">
    <button class="primary" id="run">Run</button>
    <div class="group">
      <label>JSON re-parse every K frames</label>
      <input type="range" id="reparse" min="1" max="60" value="10">
      <span class="val" id="reparse-val">10</span>
    </div>
    <div class="group">
      <label>Pressure (updates/frame)</label>
      <input type="range" id="pressure" min="1" max="480" step="1" value="1">
      <span class="val" id="pressure-val">1</span>
    </div>
    <span class="status" id="status">Ready.</span>
  </div>

  <div class="columns">
    <!-- JSON column -->
    <section class="card json" id="col-json">
      <h2><span class="dot"></span>JSON path</h2>
      <p class="desc">Mutates <code>parsed[0].score</code>; re-serialises + re-parses the full 15 MB array every K frames.</p>
      <div class="big-score json" id="score-json">—</div>
      <div class="stats">
        <div class="stat"><div class="k">FPS</div><div class="v" id="fps-json">0</div></div>
        <div class="stat"><div class="k">Last frame</div><div class="v" id="last-json">—</div></div>
        <div class="stat"><div class="k">Avg frame</div><div class="v" id="avg-json">—</div></div>
        <div class="stat"><div class="k">Max frame</div><div class="v" id="max-json">—</div></div>
        <div class="stat drops"><div class="k">Dropped (&gt;20ms)</div><div class="v" id="drops-json">0</div></div>
      </div>
      <div class="sparkline">
        <div class="bars" id="spark-json"></div>
        <div class="legend">50 most recent work-times · red &gt; 20 ms (over budget)</div>
      </div>
    </section>

    <!-- NXS column -->
    <section class="card nxs" id="col-nxs">
      <h2><span class="dot"></span>NXS path</h2>
      <p class="desc">In-place <code>DataView.setFloat64</code> at the cached byte offset of record 0's <code>score</code>. Zero allocations per frame.</p>
      <div class="big-score nxs" id="score-nxs">—</div>
      <div class="stats">
        <div class="stat"><div class="k">FPS</div><div class="v" id="fps-nxs">0</div></div>
        <div class="stat"><div class="k">Last frame</div><div class="v" id="last-nxs">—</div></div>
        <div class="stat"><div class="k">Avg frame</div><div class="v" id="avg-nxs">—</div></div>
        <div class="stat"><div class="k">Max frame</div><div class="v" id="max-nxs">—</div></div>
        <div class="stat drops"><div class="k">Dropped (&gt;20ms)</div><div class="v" id="drops-nxs">0</div></div>
      </div>
      <div class="sparkline">
        <div class="bars" id="spark-nxs"></div>
        <div class="legend">50 most recent work-times · red &gt; 20 ms (over budget)</div>
      </div>
    </section>
  </div>

  <section class="card" style="margin-top: 16px;">
    <h2>Long tasks (PerformanceObserver)</h2>
    <p class="desc">Entries of type <code>longtask</code> (&gt; 50 ms) reported by the browser while the demo runs. If the JSON path is dominating the main thread, they land here.</p>
    <div id="longtasks" style="font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 12px; color: var(--muted); max-height: 140px; overflow-y: auto;">No long tasks observed.</div>
  </section>

</main>
</template>

<script setup lang="ts">
import { useDemoPage } from "@/composables/useDemoPage";

useDemoPage(
  async (el) => {
    const { wireTickerPage } = await import("@/demos/ticker-demo");
    wireTickerPage(el);
  },
  () => {
    import("@/demos/ticker-demo").then((m) => m.unwireTickerPage());
  },
);
</script>
