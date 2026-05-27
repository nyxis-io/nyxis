/**
 * Shared static HTML shell for prerendered route files.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";

export function loadSiteConfig(webRoot) {
  return JSON.parse(readFileSync(join(webRoot, "content/site.json"), "utf8"));
}

export function loadRoutes(webRoot) {
  return JSON.parse(readFileSync(join(webRoot, "content/routes.json"), "utf8"));
}

export function assetRefsFromViteIndex(viteIndexHtml) {
  const cssHref =
    viteIndexHtml.match(/<link[^>]+rel="stylesheet"[^>]+href="([^"]+)"/)?.[1] ?? "";
  const jsSrc =
    viteIndexHtml.match(/<script[^>]+type="module"[^>]+src="([^"]+)"/)?.[1] ?? "";
  return { cssHref, jsSrc };
}

export function htmlOutputRel(routePath) {
  if (routePath === "/") return "index.html";
  if (routePath.endsWith("/")) return `${routePath.slice(1)}index.html`;
  return `${routePath.slice(1)}.html`;
}

function escAttr(value) {
  return String(value).replace(/&/g, "&amp;").replace(/"/g, "&quot;");
}

/**
 * @param {object} route — routes.json entry
 * @param {string} appHtml — rendered #app inner HTML
 * @param {{ cssHref: string, jsSrc: string }} assets
 * @param {{ origin: string, ogImagePath: string, ogImageAlt: string }} site
 */
export function buildPrerenderedPageHtml(route, appHtml, assets, site) {
  const ogImageUrl = `${site.origin}${site.ogImagePath}`;
  const canonical = route.canonical;
  const css = assets.cssHref
    ? `<link rel="stylesheet" crossorigin href="${assets.cssHref}">`
    : "";
  const js = assets.jsSrc
    ? `<script type="module" crossorigin src="${assets.jsSrc}"></script>`
    : "";
  const desc = escAttr(route.description);
  const title = escAttr(route.title);
  const ogAlt = escAttr(site.ogImageAlt);

  return `<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <link rel="icon" href="/favicon.svg" type="image/svg+xml" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>${title}</title>
    <meta name="description" content="${desc}" />
    <meta property="og:site_name" content="Nyxis" />
    <meta property="og:type" content="website" />
    <meta property="og:url" content="${canonical}" />
    <meta property="og:title" content="${title}" />
    <meta property="og:description" content="${desc}" />
    <meta property="og:image" content="${ogImageUrl}" />
    <meta property="og:image:type" content="image/png" />
    <meta property="og:image:width" content="1400" />
    <meta property="og:image:height" content="933" />
    <meta property="og:image:alt" content="${ogAlt}" />
    <meta name="twitter:card" content="summary_large_image" />
    <meta name="twitter:title" content="${title}" />
    <meta name="twitter:description" content="${desc}" />
    <meta name="twitter:image" content="${ogImageUrl}" />
    <meta name="twitter:image:alt" content="${ogAlt}" />
    <link rel="canonical" href="${canonical}" />
    <link rel="alternate" type="text/markdown" href="${route.markdown}" />
    ${css}
  </head>
  <body>
    <div id="app">${appHtml}</div>
    ${js}
  </body>
</html>
`;
}
