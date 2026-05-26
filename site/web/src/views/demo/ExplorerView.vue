<template>
<main ref="root" class="page-main page-main--explorer interactive-page">
  <header class="hdr">
    <div class="drop-zone" id="drop">
      <div class="title-block">
        <p class="page-eyebrow">NXS · Nyxis · Demo</p>
        <h1 class="page-title">Log explorer</h1>
        <p class="page-lead">Millions of rows — virtual scroll, search, and zero-copy reads in the browser</p>
      </div>
      <ul class="explorer-context" aria-label="Dataset context">
        <li><strong>1M+ records</strong> built-in fixtures</li>
        <li><strong>Fully browser-side</strong></li>
        <li><strong>No hydration pass</strong></li>
        <li><strong>No server-side filtering</strong></li>
      </ul>
      <div class="hint" id="drop-hint">
        Drop <code>.nxb</code>, <code>.nxs</code>, or <code>.json</code> here, <label for="file" class="upload-label">browse</label>, or
      </div>
      <button type="button" id="pick">Choose file</button>
      <input type="file" id="file" accept=".nxb,.NXB,.nxs,.NXS,.json,.JSON,text/plain,application/json,application/octet-stream" hidden>
      <div class="file-info" id="file-info"></div>
    </div>
  </header>

  <div class="explorer-workspace">
    <section class="explorer-telemetry" aria-label="Live instrumentation">
      <div class="tel-grid">
        <div class="tel-item"><span class="tel-label">Open</span><span class="tel-value" id="tel-open">—</span></div>
        <div class="tel-item"><span class="tel-label">Filter / search</span><span class="tel-value" id="tel-filter">—</span></div>
        <div class="tel-item"><span class="tel-label">Heap Δ (Chrome)</span><span class="tel-value" id="tel-memory">—</span></div>
        <div class="tel-item"><span class="tel-label">Rows streamed</span><span class="tel-value" id="tel-streamed">—</span></div>
        <div class="tel-item"><span class="tel-label">Rows loaded</span><span class="tel-value" id="tel-loaded">—</span></div>
        <div class="tel-item"><span class="tel-label">Format</span><span class="tel-value" id="tel-format">—</span></div>
      </div>
      <div class="explorer-compare-bar" id="compare-bar" hidden>
        <span id="compare-nxs">NXS: —</span>
        <span class="sep">│</span>
        <span id="compare-json">JSON: —</span>
        <button type="button" id="compare-run" class="compare-run-btn">Run JSON vs NXS at this size</button>
      </div>
    </section>

    <div class="toolbar">
      <div class="search-wrap">
        <input type="text" id="search" placeholder="Search…  (Ctrl/Cmd+F)" autocomplete="off" spellcheck="false">
        <span class="badge" id="search-badge"></span>
      </div>
      <div class="nav-btns">
        <button id="prev-match" title="Previous match (Shift+Enter)">‹</button>
        <button id="next-match" title="Next match (Enter)">›</button>
      </div>
      <div class="jump">
        <label>Jump to line:</label>
        <input type="number" id="jump-input" min="1" placeholder="1">
        <button id="jump-btn">Go</button>
      </div>
    </div>

    <div class="viewer">
      <div class="col-header hide" id="col-header" aria-hidden="true"></div>
      <div class="scroll" id="scroll" tabindex="0">
        <div class="spacer" id="spacer"></div>
      </div>
      <div class="overlay" id="overlay">Loading fixture…</div>
    </div>

    <div class="status">
      <span id="status-pos">—</span>
      <span class="sep">│</span>
      <span id="status-matches">No search active</span>
      <span class="grow"></span>
      <span id="status-frame" class="ok">—</span>
      <span class="sep">│</span>
      <span class="muted">
        <kbd>↑↓</kbd> <kbd>PgUp</kbd> <kbd>PgDn</kbd> <kbd>Home</kbd> <kbd>End</kbd>
        <kbd>Enter</kbd> next match
      </span>
    </div>
  </div>

  <details class="explorer-how">
    <summary>How it works</summary>
    <p>
      Virtual scroll over very large log-shaped datasets without one DOM node per line.
      <code>.nxb</code> uses <code>NxsStreamReader</code> then tail-index random access;
      <code>.json</code> parses once for comparison — use <strong>Run JSON vs NXS</strong> after loading a fixture.
    </p>
    <ul>
      <li><strong>Zero-copy reads</strong> — mapped wire cells; no <code>JSON.parse()</code> graph.</li>
      <li><strong>Selective access</strong> — tail-index jumps to the rows your viewport needs.</li>
      <li><strong>Progressive loading</strong> — stream records before the file footer seals.</li>
      <li><strong>Append-friendly layout</strong> — same row format from ingest through seal.</li>
    </ul>
  </details>
</main>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted } from "vue";
import { useDemoPage } from "@/composables/useDemoPage";

useDemoPage(
  async (el) => {
    const { wireExplorerPage } = await import("@/demos/explorer-demo");
    wireExplorerPage(el);
  },
  () => {
    import("@/demos/explorer-demo").then((m) => m.unwireExplorerPage());
  },
);

onMounted(() => {
  document.documentElement.classList.add("route-explorer");
});

onUnmounted(() => {
  document.documentElement.classList.remove("route-explorer");
});
</script>
