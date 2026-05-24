#!/usr/bin/env node
/** Quick smoke test for /bench/ and /demo/ticker on localhost:8000 */
import { chromium } from "playwright";

const BASE = process.env.BASE_URL ?? "http://localhost:8000";

async function smoke(path, checks) {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (msg) => {
    if (msg.type() === "error") errors.push(msg.text());
  });

  const url = `${BASE}${path}`;
  console.log(`\n→ ${url}`);
  const res = await page.goto(url, { waitUntil: "networkidle", timeout: 60_000 });
  console.log(`  HTTP ${res?.status()}`);

  await page.waitForTimeout(2000);

  for (const [name, fn] of checks) {
    try {
      const ok = await fn(page);
      console.log(`  ${ok ? "✓" : "✗"} ${name}${ok ? "" : " (failed)"}`);
    } catch (e) {
      console.log(`  ✗ ${name}: ${e.message}`);
    }
  }

  if (errors.length) {
    console.log("  Console errors:");
    for (const e of errors.slice(0, 8)) console.log(`    - ${e.slice(0, 200)}`);
  } else {
    console.log("  ✓ no console errors");
  }

  await browser.close();
  return errors.length === 0;
}

const benchOk = await smoke("/bench/", [
  ["#run button", async (p) => (await p.$("#run")) !== null],
  ["#chart-open", async (p) => (await p.$("#chart-open")) !== null],
  ["chart bars after auto-run", async (p) => {
    const bars = await p.$$("#chart-open .bar");
    return bars.length > 0;
  }],
  ["status not stuck on Ready", async (p) => {
    const t = await p.textContent("#status");
    return t && !/^Ready\.$/.test(t.trim());
  }],
]);

const tickerOk = await smoke("/demo/ticker", [
  ["#run button", async (p) => (await p.$("#run")) !== null],
  ["#reparse slider", async (p) => (await p.$("#reparse")) !== null],
  ["fixtures loaded", async (p) => {
    const t = await p.textContent("#status");
    return t && /Loaded|MB|records/i.test(t);
  }],
  ["Run starts loop", async (p) => {
    await p.click("#run");
    await p.waitForTimeout(500);
    const run = await p.textContent("#run");
    return run?.trim() === "Stop";
  }],
]);

process.exit(benchOk && tickerOk ? 0 : 1);
