#!/usr/bin/env node
/**
 * One-time helper: extract page templates and inline demo scripts from legacy HTML.
 */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const siteRoot = path.resolve(__dirname, "../..");
const srcRoot = path.resolve(__dirname, "../src");

function read(rel) {
  return fs.readFileSync(path.join(siteRoot, rel), "utf8");
}

function write(rel, content) {
  const out = path.join(srcRoot, rel);
  fs.mkdirSync(path.dirname(out), { recursive: true });
  fs.writeFileSync(out, content);
}

function extractModuleScript(html) {
  const m = html.match(/<script type="module">\s*([\s\S]*?)<\/script>/);
  return m ? m[1].trim() : null;
}

function extractMain(html) {
  const m = html.match(/<main[\s\S]*?<\/main>/);
  return m ? m[0] : "";
}

function extractStyles(html) {
  const m = html.match(/<style>([\s\S]*?)<\/style>/);
  return m ? m[1].trim() : "";
}

function bodyContent(html) {
  const body = html.match(/<body[^>]*>([\s\S]*)<\/body>/i)?.[1] ?? "";
  return body
    .replace(/<div id="site-nav-root"><\/div>\s*/i, "")
    .replace(/<script[\s\S]*$/i, "")
    .trim();
}

function landingContent(html) {
  const body = html.match(/<body[^>]*>([\s\S]*)<\/body>/i)?.[1] ?? "";
  const start = body.indexOf("<section class=\"landing-hero\"");
  const end = body.indexOf("<footer class=\"site-footer\"");
  if (start < 0 || end < 0) return bodyContent(html);
  return body.slice(start, end).trim();
}

function toVueTemplate(htmlFragment, scopedStyles = "") {
  let t = htmlFragment
    .replace(/\.\.\/bench\/fixtures\//g, "/bench/fixtures/")
    .replace(/\.\.\/\.\.\/BENCHMARK\.md/g, "https://github.com/nyxis-io/nyxis/blob/main/BENCHMARK.md")
    .replace(/href="\/demo\/(\w+)\.html"/g, 'href="/demo/$1"')
    .replace(/href="(\w+)\.html"/g, 'href="/demo/$1"')
    .replace(/href="\.\.\/bench\/"/g, 'href="/bench/"');
  const styleBlock = scopedStyles
    ? `\n<style scoped>\n${scopedStyles}\n</style>\n`
    : "";
  return `<template>\n${t}\n</template>${styleBlock}`;
}

function fixDemoCode(code) {
  return code
    .replace(/\.\.\/bench\/fixtures\//g, "/bench/fixtures/")
    .replace(/from "\.\/bench-run\.js"/g, 'from "@/demos/bench-run.js"')
    .replace(
      /new Worker\("\.\/explorer_worker\.js"/g,
      'new Worker(new URL("../workers/explorer_worker.js", import.meta.url)',
    )
    .replace(
      /new Worker\("\.\/nxs_worker\.js"/g,
      'new Worker(new URL("../workers/nxs_worker.js", import.meta.url)',
    )
    .replace(
      /new Worker\("\.\/json_worker\.js"/g,
      'new Worker(new URL("../workers/json_worker.js", import.meta.url)',
    );
}

const demos = [
  { html: "demo/ticker.html", name: "Ticker", out: "demos/ticker-demo.js", view: "views/demo/TickerView.vue" },
  { html: "demo/workers.html", name: "Workers", out: "demos/workers-demo.js", view: "views/demo/WorkersView.vue" },
  { html: "demo/explorer.html", name: "Explorer", out: "demos/explorer-demo.js", view: "views/demo/ExplorerView.vue" },
  { html: "demo/wal.html", name: "Wal", out: "demos/wal-demo.js", view: "views/demo/WalView.vue" },
  { html: "bench/index.html", name: "Bench", out: "demos/bench-page.js", view: "views/BenchView.vue" },
];

for (const d of demos) {
  const html = read(d.html);
  const script = extractModuleScript(html);
  if (!script) {
    console.warn("no module script:", d.html);
    continue;
  }
  write(d.out, fixDemoCode(script));
  const vue =
    toVueTemplate(extractMain(html), extractStyles(html)) +
    `\n<script setup lang="ts">\nimport { onMounted } from "vue";\n\nonMounted(() => import("@/demos/${path.basename(d.out, ".ts").replace(/\\.ts$/, ".js")}"));\n</script>\n`;
  write(d.view, vue);
}

// Report — already modular
const reportHtml = read("demo/report.html");
write(
  "views/demo/ReportView.vue",
  toVueTemplate(extractMain(reportHtml), extractStyles(reportHtml)) +
    `\n<script setup lang="ts">\nimport { onMounted } from "vue";\nimport { wireReportPage } from "@/demos/report";\n\nonMounted(() => wireReportPage());\n</script>\n`,
);

// Copy report.js
fs.copyFileSync(path.join(siteRoot, "demo/report.js"), path.join(srcRoot, "demos/report.js"));

// Demo index
write(
  "views/demo/DemoIndexView.vue",
  toVueTemplate(extractMain(read("demo/index.html"))) + `\n<script setup lang="ts">\n</script>\n`,
);

// Static pages
write(
  "views/HomeView.vue",
  toVueTemplate(landingContent(read("index.html"))) + `\n<script setup lang="ts">\n</script>\n`,
);
write(
  "views/UseCasesView.vue",
  toVueTemplate(extractMain(read("use-cases/index.html"))) + `\n<script setup lang="ts">\n</script>\n`,
);
write(
  "views/PricingView.vue",
  toVueTemplate(extractMain(read("pricing/index.html"))) + `\n<script setup lang="ts">\n</script>\n`,
);

// Workers
fs.mkdirSync(path.join(srcRoot, "workers"), { recursive: true });
for (const w of ["explorer_worker.js", "nxs_worker.js", "json_worker.js"]) {
  fs.copyFileSync(path.join(siteRoot, "demo", w), path.join(srcRoot, "workers", w));
}

// Theme css
fs.mkdirSync(path.join(srcRoot, "assets"), { recursive: true });
fs.copyFileSync(path.join(siteRoot, "demo/theme.css"), path.join(srcRoot, "assets/theme.css"));

console.log("Migration extract complete.");
