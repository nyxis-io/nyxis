<template>
<main ref="root" class="page-main page-main--explorer interactive-page">
  <header class="hdr">
    <div class="drop-zone" id="drop">
      <div class="title-block">
        <p class="page-eyebrow">NXS · Nyxis · Demo</p>
        <h1 class="page-title">Log explorer</h1>
        <p class="page-lead">10M-line viewer — virtual scroll, in-place search, zero-copy parsing</p>
      </div>
      <div class="hint" id="drop-hint">
        Drop <code>.nxb</code>, <code>.nxs</code>, or <code>.json</code> here, <label for="file" class="upload-label">browse</label>, or
      </div>
      <button type="button" id="pick">Choose file</button>
      <input type="file" id="file" accept=".nxb,.NXB,.nxs,.NXS,.json,.JSON,text/plain,application/json,application/octet-stream" hidden>
      <div class="file-info" id="file-info"></div>
    </div>
  </header>

  <section class="what-tested" aria-label="What this page tests">
    <strong>What this page tests</strong>
    UI scalability for a <strong>very large</strong> log-shaped dataset (millions of rows): a fixed <strong>row pool</strong> and a tall inner <strong>spacer</strong> with scroll scaling map viewport position to record indices without creating one DOM node per line. With <code>.nxb</code>, loads use <code>NxsStreamReader</code> so schema and records parse while bytes arrive (v1.2 streamable row layout); after the footer seals, <code>NxsReader</code> tail-index enables <strong>random access</strong> without a full JS object graph. With <code>.nxs</code>, the server returns <code>text/plain</code> (readable in DevTools Network → Response); the page compiles source to <code>.nxb</code> in-browser via WASM, then uses the same zero-copy reader. With <code>.json</code>, the file is parsed once into an array, then the same virtual list runs over that in-memory model (useful for comparison). Search, jump-to-line, and the status bar’s <strong>render</strong> timing show whether scrolling stays smooth under load.
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
