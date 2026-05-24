#!/usr/bin/env node
/**
 * Prerender static HTML from markdown so HTML fetchers (no JS) see page content.
 * Runs after vite build; writes route-specific HTML files into site/dist.
 */
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { marked } from "marked";

const webRoot = resolve(import.meta.dirname, "..");
const publicDir = join(webRoot, "public");
const distDir = resolve(webRoot, "../dist");
const siteOrigin = "https://www.nyxis.io";

const routes = JSON.parse(readFileSync(join(webRoot, "content/routes.json"), "utf8"));
const viteIndex = readFileSync(join(distDir, "index.html"), "utf8");

const cssHref = viteIndex.match(/<link[^>]+rel="stylesheet"[^>]+href="([^"]+)"/)?.[1] ?? "";
const jsSrc = viteIndex.match(/<script[^>]+type="module"[^>]+src="([^"]+)"/)?.[1] ?? "";

marked.setOptions({ gfm: true, headerIds: true });

function htmlOutputRel(routePath) {
  if (routePath === "/") return "index.html";
  if (routePath.endsWith("/")) return `${routePath.slice(1)}index.html`;
  return `${routePath.slice(1)}.html`;
}

function pageHtml(route, articleHtml) {
  const canonical = `${siteOrigin}${route.path}`;
  const css = cssHref ? `<link rel="stylesheet" crossorigin href="${cssHref}">` : "";
  const js = jsSrc ? `<script type="module" crossorigin src="${jsSrc}"></script>` : "";
  const desc = route.description.replace(/"/g, "&quot;");
  return `<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <link rel="icon" href="/favicon.svg" type="image/svg+xml" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>${route.title}</title>
    <meta name="description" content="${desc}" />
    <link rel="canonical" href="${canonical}" />
    <link rel="alternate" type="text/markdown" href="${route.markdown}" />
    ${css}
  </head>
  <body>
    <div id="app">
      <main class="static-prerender">
        ${articleHtml}
      </main>
    </div>
    ${js}
  </body>
</html>
`;
}

for (const route of routes) {
  const md = readFileSync(join(publicDir, route.markdown.slice(1)), "utf8");
  const articleHtml = marked.parse(md);
  const rel = htmlOutputRel(route.path);
  const out = join(distDir, rel);
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, pageHtml(route, articleHtml));
  console.log(`Prerendered ${route.path} → dist/${rel}`);
}
