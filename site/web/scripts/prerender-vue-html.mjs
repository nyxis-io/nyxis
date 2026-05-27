#!/usr/bin/env node
/**
 * Prerender each route by rendering the Vue app (Playwright) after vite build.
 * Replaces markdown-only prerender so no-JS HTML matches live page content.
 */
import { spawn } from "node:child_process";
import { createConnection } from "node:net";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { chromium } from "playwright";
import {
  assetRefsFromViteIndex,
  buildPrerenderedPageHtml,
  htmlOutputRel,
  loadRoutes,
  loadSiteConfig,
} from "./lib/page-html.mjs";

const webRoot = resolve(import.meta.dirname, "..");
const distDir = resolve(webRoot, "../dist");
const previewPort = Number(process.env.PRERENDER_PORT ?? 4173);
const previewHost = process.env.PRERENDER_HOST ?? "127.0.0.1";
const previewBase = `http://${previewHost}:${previewPort}`;

const routes = loadRoutes(webRoot);
const site = loadSiteConfig(webRoot);
const viteIndex = readFileSync(join(distDir, "index.html"), "utf8");
const assets = assetRefsFromViteIndex(viteIndex);

const CONTENT_SELECTOR =
  "#app nav, #app .landing-hero, #app .page-main, #app .interactive-page";

function waitForPort(port, host, timeoutMs = 90_000) {
  const started = Date.now();
  return new Promise((resolve, reject) => {
    const attempt = () => {
      const socket = createConnection({ port, host }, () => {
        socket.end();
        resolve();
      });
      socket.on("error", () => {
        if (Date.now() - started > timeoutMs) {
          reject(new Error(`Timed out waiting for ${host}:${port}`));
          return;
        }
        setTimeout(attempt, 250);
      });
    };
    attempt();
  });
}

function startPreview() {
  const child = spawn(
    "npx",
    ["vite", "preview", "--host", previewHost, "--port", String(previewPort), "--strictPort"],
    { cwd: webRoot, stdio: ["ignore", "pipe", "pipe"] },
  );
  child.stderr?.on("data", (chunk) => process.stderr.write(chunk));
  return child;
}

const preview = startPreview();
let browser;

try {
  await waitForPort(previewPort, previewHost);
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();

  for (const route of routes) {
    const url = `${previewBase}${route.path}`;
    console.log(`Prerender ${route.path} ← ${url}`);

    await page.goto(url, { waitUntil: "domcontentloaded", timeout: 60_000 });
    await page.waitForSelector(CONTENT_SELECTOR, { timeout: 30_000 });
    // Allow async route components and layout to settle.
    await page.waitForTimeout(route.interactive ? 800 : 400);

    const appHtml = await page.locator("#app").innerHTML();
    const rel = htmlOutputRel(route.path);
    const out = join(distDir, rel);
    mkdirSync(dirname(out), { recursive: true });
    writeFileSync(out, buildPrerenderedPageHtml(route, appHtml, assets, site));
    console.log(`  → dist/${rel} (${appHtml.length} bytes body)`);
  }
} finally {
  if (browser) await browser.close();
  preview.kill("SIGTERM");
}
