/**
 * Report layout demo — your CSV, row vs columnar .nxb, real chart from col_buffer.
 */
import { parseCsv } from "/bench/bench-run.js";
import { NxsReader } from "/sdk/nxs.js";
import { NxsWriter } from "/sdk/nxs_writer.js";
import { compileNxsColumnar } from "/sdk/nxs_compile.js";
import { loadWasm } from "/sdk/wasm.js";

const MAX_ROWS = 100_000;
const MAX_COLUMNAR_ROWS = 100_000;
const SAMPLE_ROW_COUNT = 100_000;
const WARMUP = 3;
const ITERS = 12;

/** @type {Chart | null} */
let chart = null;

/** Deterministic [0, 1) — stable across runs for sample generators. */
function sampleRand(seed) {
  let s = seed >>> 0;
  return () => {
    s = (s + 0x6d2b79f5) >>> 0;
    let t = Math.imul(s ^ (s >>> 15), 1 | s);
    t = (Math.imul(t ^ (t >>> 7), 61 | t) ^ t) >>> 0;
    return (t ^ (t >>> 14)) / 4294967296;
  };
}

export const SAMPLE_DATASETS = {
  sales: {
    label: "Sales (100k rows)",
    description: "12 regions × 12 SKUs, discounts & seasonality — revenue by region (bar)",
    rowCount: SAMPLE_ROW_COUNT,
    chartKind: "region_bar",
    types: {
      region: "string",
      product: "string",
      channel: "string",
      revenue: "float",
      units: "int",
      discount_pct: "float",
      date: "int",
    },
    headers: ["region", "product", "channel", "revenue", "units", "discount_pct", "date"],
    generateRows(n = SAMPLE_ROW_COUNT) {
      const regions = [
        "North", "South", "East", "West", "EMEA", "APAC",
        "LATAM", "Central", "Nordics", "DACH", "ANZ", "Japan",
      ];
      const products = [
        "Widget", "Gadget", "Sprocket", "Bolt", "Gear", "Valve",
        "Sensor", "Cable", "Motor", "Pump", "Filter", "Bracket",
      ];
      const channels = ["web", "retail", "partner", "inside_sales", "marketplace"];
      const rand = sampleRand(0x53414c45);
      const rows = new Array(n);
      for (let i = 0; i < n; i++) {
        const region = regions[Math.floor(rand() * regions.length)];
        const product = products[Math.floor(rand() * products.length)];
        const channel = channels[Math.floor(rand() * channels.length)];
        const units = 1 + Math.floor(rand() * 48);
        const discount = rand() < 0.22 ? rand() * 0.35 : 0;
        const season = 0.85 + 0.3 * Math.sin((i / n) * Math.PI * 4);
        const noise = 0.7 + rand() * 0.9;
        const base = 12 + rand() * 180;
        const revenue = Math.round(base * units * season * noise * (1 - discount) * 100) / 100;
        const daySpread = Math.floor(rand() * 730);
        rows[i] = {
          region,
          product,
          channel,
          revenue,
          units,
          discount_pct: Math.round(discount * 1000) / 10,
          date: 1_700_000_000_000 + daySpread * 86_400_000 + Math.floor(rand() * 86_400_000),
        };
      }
      return { headers: this.headers, rows, types: this.types };
    },
    generate(n = SAMPLE_ROW_COUNT) {
      const { headers, rows } = this.generateRows(n);
      const lines = [headers.join(",")];
      for (const r of rows) {
        lines.push(
          `${r.region},${r.product},${r.channel},${r.revenue.toFixed(2)},${r.units},${r.discount_pct},${r.date}`,
        );
      }
      return lines.join("\n");
    },
    metricKey: "revenue",
    regionKey: "region",
  },
  logs: {
    label: "Access logs (100k rows)",
    description: "20 endpoints, mixed status & tail latency — latency_ms line from col_buffer",
    rowCount: SAMPLE_ROW_COUNT,
    chartKind: "line",
    types: {
      endpoint: "string",
      method: "string",
      status: "int",
      latency_ms: "float",
      bytes: "int",
    },
    headers: ["endpoint", "method", "status", "latency_ms", "bytes"],
    generateRows(n = SAMPLE_ROW_COUNT) {
      const endpoints = [
        "/api/v1/users", "/api/v1/orders", "/api/v1/search", "/api/v1/checkout",
        "/api/v1/inventory", "/api/v2/recommendations", "/api/v2/billing",
        "/graphql", "/health", "/metrics", "/static/assets", "/auth/token",
        "/webhooks/stripe", "/admin/reports", "/export/csv", "/uploads",
        "/cdn/purge", "/internal/jobs", "/sse/events", "/ws/connect",
      ];
      const methods = ["GET", "POST", "PUT", "PATCH", "DELETE"];
      const statuses = [200, 201, 204, 301, 400, 401, 403, 404, 408, 429, 500, 502, 503];
      const weights = [52, 8, 4, 2, 6, 3, 2, 9, 2, 4, 3, 2, 3];
      const rand = sampleRand(0x4c4f4753);
      const rows = new Array(n);
      for (let i = 0; i < n; i++) {
        const ep = endpoints[Math.floor(rand() * endpoints.length)];
        const method = methods[Math.floor(rand() * methods.length)];
        let r = rand();
        let status = 200;
        for (let w = 0; w < statuses.length; w++) {
          r -= weights[w] / 100;
          if (r <= 0) {
            status = statuses[w];
            break;
          }
        }
        const epBias = ep.length % 7;
        let latency = 1.5 + rand() * (8 + epBias * 3);
        if (status >= 500) latency += 40 + rand() * 220;
        else if (status === 429) latency += 80 + rand() * 120;
        else if (rand() < 0.08) latency += 25 + rand() * 90;
        const bytes =
          status >= 400
            ? 80 + Math.floor(rand() * 400)
            : 400 + Math.floor(rand() * 120_000);
        rows[i] = {
          endpoint: ep,
          method,
          status,
          latency_ms: Math.round(latency * 100) / 100,
          bytes,
        };
      }
      return { headers: this.headers, rows, types: this.types };
    },
    generate(n = SAMPLE_ROW_COUNT) {
      const { headers, rows } = this.generateRows(n);
      const lines = [headers.join(",")];
      for (const r of rows) {
        lines.push(
          `${r.endpoint},${r.method},${r.status},${r.latency_ms.toFixed(2)},${r.bytes}`,
        );
      }
      return lines.join("\n");
    },
    metricKey: "latency_ms",
    regionKey: null,
  },
  sparse: {
    label: "Sparse numeric (100k rows)",
    description: "Per-field fill rates (5–38%) + varied score — bar chart from column null bitmaps",
    rowCount: SAMPLE_ROW_COUNT,
    chartKind: "sparse_fill",
    types: Object.assign(
      { id: "int", score: "float" },
      Object.fromEntries(Array.from({ length: 20 }, (_, i) => [`m${i}`, "int"])),
    ),
    headers: ["id", "score", ...Array.from({ length: 20 }, (_, i) => `m${i}`)],
    generateRows(n = SAMPLE_ROW_COUNT) {
      const rand = sampleRand(0x53504152);
      const fillPct = Array.from({ length: 20 }, (_, f) => 5 + ((f * 17 + 11) % 34));
      const rows = new Array(n);
      for (let i = 0; i < n; i++) {
        const r = {
          id: i,
          score:
            Math.round(
              (18 +
                Math.sin(i / 137) * 22 +
                Math.cos(i / 59) * 11 +
                rand() * 45 +
                (i % 97) * 0.19) *
                100,
            ) / 100,
        };
        for (let f = 0; f < 20; f++) {
          if (rand() * 100 < fillPct[f]) {
            r[`m${f}`] = Math.floor(rand() * 10_000) + f * 13;
          }
        }
        rows[i] = r;
      }
      return { headers: this.headers, rows, types: this.types };
    },
    generate(n = SAMPLE_ROW_COUNT) {
      const { headers, rows } = this.generateRows(n);
      const lines = [headers.join(",")];
      for (const r of rows) {
        const cells = headers.map((h) => {
          const v = r[h];
          return v == null || v === "" ? "" : String(v);
        });
        lines.push(cells.join(","));
      }
      return lines.join("\n");
    },
    metricKey: "score",
    regionKey: null,
  },
};

function fmtTime(ms) {
  if (ms < 0.001) return `${(ms * 1e6).toFixed(0)} ns`;
  if (ms < 1) return `${(ms * 1e3).toFixed(1)} µs`;
  if (ms < 1000) return `${ms.toFixed(2)} ms`;
  return `${(ms / 1000).toFixed(2)} s`;
}

function fmtBytes(n) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(2)} MB`;
}

function benchMs(fn) {
  for (let i = 0; i < WARMUP; i++) fn();
  const t0 = performance.now();
  for (let i = 0; i < ITERS; i++) fn();
  return (performance.now() - t0) / ITERS;
}

function inferColumnType(values) {
  let seenNum = 0;
  let seenInt = 0;
  let seenBool = 0;
  let seenStr = 0;
  for (const raw of values) {
    if (raw === "" || raw == null) continue;
    const s = String(raw).trim();
    if (s === "true" || s === "false") {
      seenBool++;
      continue;
    }
    const n = Number(s);
    if (!Number.isFinite(n)) {
      seenStr++;
      continue;
    }
    seenNum++;
    if (Number.isInteger(n) && Math.abs(n) <= Number.MAX_SAFE_INTEGER) seenInt++;
  }
  if (seenStr > 0) return "string";
  if (seenBool > 0 && seenNum === 0) return "bool";
  if (seenInt === seenNum && seenNum > 0) return "int";
  if (seenNum > 0) return "float";
  return "string";
}

/**
 * @param {object[]} rows
 * @param {string[]} headers
 */
function buildSchema(rows, headers) {
  const types = {};
  const regionCodes = new Map();
  let nextCode = 0;
  for (const h of headers) {
    const col = rows.map((r) => r[h] ?? "");
    types[h] = inferColumnType(col);
    if (types[h] === "string" && h.toLowerCase().includes("region")) {
      for (const r of rows) {
        const v = r[h];
        if (v != null && v !== "" && !regionCodes.has(v)) {
          regionCodes.set(v, nextCode++);
        }
      }
    }
  }
  return { types, regionCodes };
}

function nxsValue(type, raw) {
  if (raw === "" || raw == null) return null;
  const s = String(raw).trim();
  switch (type) {
    case "int":
      return { sigil: "=", text: String(Math.trunc(Number(s))) };
    case "float":
      return { sigil: "~", text: String(Number(s)) };
    case "bool":
      return { sigil: "?", text: s === "true" ? "true" : "false" };
    default:
      return { sigil: '"', text: JSON.stringify(s) };
  }
}

function columnarKeys(headers, types) {
  const out = [];
  for (const h of headers) {
    const t = types[h];
    if (t === "int" || t === "float" || t === "bool") out.push(h);
  }
  if (types.region_id) out.push("region_id");
  return out;
}

/**
 * @param {object[]} rows
 * @param {string[]} headers
 * @param {Record<string, string>} types
 * @param {Map<string, number>} regionCodes
 */
function enrichRows(rows, headers, types, regionCodes) {
  if (regionCodes.size === 0) return;
  for (const r of rows) {
    for (const h of headers) {
      if (types[h] === "string" && h.toLowerCase().includes("region")) {
        const code = regionCodes.get(r[h]);
        if (code !== undefined) r.region_id = code;
      }
    }
  }
  if (!headers.includes("region_id") && regionCodes.size > 0) {
    headers.push("region_id");
    types.region_id = "int";
  }
}

function coerceNumericRows(rows, headers, types) {
  for (const r of rows) {
    for (const h of headers) {
      const t = types[h];
      const raw = r[h];
      if (raw === "" || raw == null) continue;
      if (t === "int") r[h] = Math.trunc(Number(raw));
      else if (t === "float") r[h] = Number(raw);
      else if (t === "bool") r[h] = String(raw).trim() === "true";
    }
  }
}

function buildColumnarNxsSource(rows, keys, types) {
  const lines = [];
  const lim = Math.min(rows.length, MAX_COLUMNAR_ROWS);
  for (let i = 0; i < lim; i++) {
    const parts = [];
    for (const k of keys) {
      const t = types[k];
      if (t === "string") continue;
      const cell = nxsValue(t, rows[i][k]);
      if (!cell) continue;
      parts.push(`${k}: ${cell.sigil}${cell.text}`);
    }
    lines.push(`r${i} { ${parts.join(" ")} }`);
  }
  return lines.join("\n");
}

function buildRowBytes(headers, rows) {
  return NxsWriter.fromRecords(headers, rows);
}

async function buildColumnarBytes(rows, headers, types, regionCodes) {
  enrichRows(rows, headers, types, regionCodes);
  const keys = columnarKeys(headers, types);
  if (keys.length === 0) {
    throw new Error("No numeric columns for columnar layout");
  }
  const src = buildColumnarNxsSource(rows, keys, types);
  return compileNxsColumnar(src);
}

function aggregateByRegionFromBuffers(regionIdBuf, revenueBuf, regionNames, topN = 12) {
  const n = regionIdBuf.count;
  const rid = regionIdBuf.values;
  const rev = revenueBuf.values;
  const buckets = new Map();
  for (let i = 0; i < n; i++) {
    const off = i * 8;
    const code = rdI64Safe(rid, off);
    const v = rdF64(rev, off);
    buckets.set(code, (buckets.get(code) || 0) + v);
  }
  const entries = [...buckets.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, topN);
  const labels = entries.map(([code]) => regionNames[code] ?? `region ${code}`);
  const data = entries.map(([, v]) => v);
  return { labels, data };
}

function rdF64(bytes, off) {
  const buf = new ArrayBuffer(8);
  const u8 = new Uint8Array(buf);
  u8.set(bytes.subarray(off, off + 8));
  return new Float64Array(buf)[0];
}

function rdI64Safe(bytes, off) {
  const lo = (bytes[off] | (bytes[off + 1] << 8) | (bytes[off + 2] << 16) | (bytes[off + 3] << 24)) >>> 0;
  const hi = (bytes[off + 4] | (bytes[off + 5] << 8) | (bytes[off + 6] << 16) | (bytes[off + 7] << 24)) | 0;
  return hi * 0x100000000 + lo;
}

function renderChart(canvas, labels, data, metricLabel) {
  if (chart) chart.destroy();
  chart = new Chart(canvas, {
    type: "bar",
    data: {
      labels,
      datasets: [{
        label: metricLabel,
        data,
        backgroundColor: "rgba(56, 189, 248, 0.55)",
        borderColor: "rgba(56, 189, 248, 1)",
        borderWidth: 1,
      }],
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      plugins: {
        legend: { display: false },
        title: {
          display: true,
          text: "From columnar buffers (region_id + metric via col_buffer)",
          color: "#94a3b8",
          font: { size: 12 },
        },
      },
      scales: {
        x: { ticks: { color: "#94a3b8", maxRotation: 45 } },
        y: { ticks: { color: "#94a3b8" } },
      },
    },
  });
}

/** Evenly spaced indices so periodic columns do not alias to a flat line. */
function sampleSeriesIndices(count, maxPoints = 200) {
  const pts = Math.min(maxPoints, count);
  if (pts <= 1) return [0];
  const out = new Array(pts);
  for (let k = 0; k < pts; k++) {
    out[k] = Math.floor((k * (count - 1)) / (pts - 1));
  }
  return out;
}

function renderLineFromColBuffer(canvas, values, count, metricLabel) {
  if (chart) chart.destroy();
  const labels = [];
  const data = [];
  for (const i of sampleSeriesIndices(count)) {
    labels.push(String(i));
    data.push(rdF64(values, i * 8));
  }
  chart = new Chart(canvas, {
    type: "line",
    data: {
      labels,
      datasets: [{
        label: metricLabel,
        data,
        borderColor: "rgba(56, 189, 248, 1)",
        backgroundColor: "rgba(56, 189, 248, 0.12)",
        fill: true,
        pointRadius: 0,
        borderWidth: 1.5,
      }],
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      plugins: {
        legend: { display: false },
        title: {
          display: true,
          text: `${metricLabel} sampled across row index (col_buffer)`,
          color: "#94a3b8",
          font: { size: 12 },
        },
      },
      scales: {
        x: { display: false },
        y: { ticks: { color: "#94a3b8" } },
      },
    },
  });
}

function sparseFillRatesFromColReader(colReader, fieldCount = 20) {
  const n = colReader.recordCount;
  const labels = [];
  const data = [];
  for (let f = 0; f < fieldCount; f++) {
    const key = `m${f}`;
    labels.push(key);
    try {
      const { bitmap } = colReader.colBuffer(key);
      let set = 0;
      for (let i = 0; i < n; i++) {
        if (colReader._colBit(bitmap, i)) set++;
      }
      data.push(Math.round((set / n) * 1000) / 10);
    } catch {
      data.push(0);
    }
  }
  return { labels, data };
}

function renderSparseFillChart(canvas, labels, data) {
  if (chart) chart.destroy();
  chart = new Chart(canvas, {
    type: "bar",
    data: {
      labels,
      datasets: [{
        label: "fill %",
        data,
        backgroundColor: "rgba(167, 139, 250, 0.55)",
        borderColor: "rgba(167, 139, 250, 1)",
        borderWidth: 1,
      }],
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      plugins: {
        legend: { display: false },
        title: {
          display: true,
          text: "Optional metric fill rate (% rows present) from column null bitmaps",
          color: "#94a3b8",
          font: { size: 12 },
        },
      },
      scales: {
        x: { ticks: { color: "#94a3b8", maxRotation: 0 } },
        y: {
          ticks: { color: "#94a3b8" },
          title: { display: true, text: "% filled", color: "#94a3b8" },
          min: 0,
          max: 100,
        },
      },
    },
  });
}

/**
 * @param {string[]} headers
 * @param {object[]} rows
 * @param {Record<string, string>} types
 * @param {{ metricKey?: string, regionKey?: string, csvBytes?: number, chartKind?: string }} opts
 */
export async function runReportFromRecords(headers, rows, types, opts = {}) {
  if (rows.length > MAX_ROWS) rows = rows.slice(0, MAX_ROWS);

  const regionCodes = new Map();
  let nextCode = 0;
  for (const h of headers) {
    if (types[h] === "string" && h.toLowerCase().includes("region")) {
      for (const r of rows) {
        const v = r[h];
        if (v != null && v !== "" && !regionCodes.has(v)) {
          regionCodes.set(v, nextCode++);
        }
      }
    }
  }
  enrichRows(rows, headers, types, regionCodes);
  coerceNumericRows(rows, headers, types);

  let metricKey = opts.metricKey;
  if (!metricKey || !headers.includes(metricKey)) {
    metricKey = headers.find((h) => types[h] === "float") || headers.find((h) => types[h] === "int");
  }
  if (!metricKey) throw new Error("No numeric column found for sum/chart");

  const regionKey = opts.regionKey && headers.includes(opts.regionKey) ? opts.regionKey : null;
  const regionNames = regionCodes.size
    ? [...regionCodes.entries()].sort((a, b) => a[1] - b[1]).map(([name]) => name)
    : [];

  const statusEl = document.getElementById("status");
  const setStatus = (msg) => {
    if (statusEl) statusEl.textContent = msg;
  };

  setStatus("Building row .nxb…");
  const tBuild0 = performance.now();
  const rowBytes = buildRowBytes(headers, rows);
  const rowBuildMs = performance.now() - tBuild0;

  let colBytes = null;
  let colBuildMs = 0;
  let colBuildNote = "";
  if (rows.length <= MAX_COLUMNAR_ROWS) {
    setStatus(
      `Compiling columnar .nxb (${rows.length.toLocaleString()} rows, WASM)…`,
    );
    const t1 = performance.now();
    colBytes = await buildColumnarBytes(rows, headers, types, regionCodes);
    colBuildMs = performance.now() - t1;
  } else {
    colBuildNote = `Columnar compile capped at ${MAX_COLUMNAR_ROWS.toLocaleString()} rows`;
  }

  const wasm = await loadWasm("/bench/wasm/nxs_reducers.wasm");
  const rowReader = new NxsReader(rowBytes.buffer.slice(rowBytes.byteOffset, rowBytes.byteOffset + rowBytes.byteLength));
  rowReader.useWasm(wasm);

  let colReader = null;
  if (colBytes) {
    colReader = new NxsReader(colBytes.buffer.slice(colBytes.byteOffset, colBytes.byteOffset + colBytes.byteLength));
  }

  const n = rowReader.recordCount;
  const mid = Math.min(n - 1, Math.floor(n / 2));

  const results = {
    n,
    metricKey,
    rowSize: rowBytes.length,
    colSize: colBytes ? colBytes.length : null,
    rowBuildMs,
    colBuildMs,
    colBuildNote,
    csvBytes: opts.csvBytes ?? 0,
    ops: {},
  };

  results.ops.firstRow = {
    row: benchMs(() => {
      rowReader.record(0).getF64(metricKey);
    }),
    columnar: colReader
      ? null
      : { label: "n/a", detail: "requires sealed columnar file" },
  };
  if (colReader) {
    results.ops.firstRow.columnar = {
      label: "after full file",
      detail: "columnar layout writes the full segment before aggregates",
      ms: null,
    };
  }

  results.ops.sum = {
    row: benchMs(() => rowReader.sumF64(metricKey)),
    columnar: colReader ? benchMs(() => colReader.colSumF64(metricKey)) : null,
  };

  results.ops.random = {
    row: benchMs(() => rowReader.record(mid).getF64(metricKey)),
    columnar: colReader ? benchMs(() => colReader.colGetF64(metricKey, mid)) : null,
  };

  const canvas = document.getElementById("report-chart");
  const chartKind = opts.chartKind ?? "auto";
  if (canvas && colReader && chartKind === "sparse_fill") {
    const tChart0 = performance.now();
    const fill = sparseFillRatesFromColReader(colReader);
    results.chartMs = performance.now() - tChart0;
    renderSparseFillChart(canvas, fill.labels, fill.data);
  } else if (
    canvas &&
    colReader &&
    (chartKind === "region_bar" || chartKind === "auto") &&
    regionNames.length &&
    headers.includes("region_id")
  ) {
    const tChart0 = performance.now();
    const ridBuf = colReader.colBuffer("region_id");
    const revBuf = colReader.colBuffer(metricKey);
    const agg = aggregateByRegionFromBuffers(ridBuf, revBuf, regionNames);
    results.chartMs = performance.now() - tChart0;
    renderChart(canvas, agg.labels, agg.data, metricKey);
  } else if (canvas && colReader) {
    const tChart0 = performance.now();
    const { values, count } = colReader.colBuffer(metricKey);
    results.chartMs = performance.now() - tChart0;
    renderLineFromColBuffer(canvas, values, count, metricKey);
  }

  return results;
}

/**
 * @param {string} csvText
 * @param {{ metricKey?: string, regionKey?: string }} opts
 */
export async function runReportDemo(csvText, opts = {}) {
  const lines = csvText.trim().split("\n");
  if (lines.length < 2) throw new Error("Need a header row and at least one data row");

  const headers = lines[0].split(",").map((h) => h.trim());
  const rows = parseCsv(csvText).filter((r) => r && Object.keys(r).length);
  const { types } = buildSchema(rows, headers);
  return runReportFromRecords(headers, rows, types, {
    ...opts,
    csvBytes: new TextEncoder().encode(csvText).length,
  });
}

export function renderResults(results) {
  const tbody = document.getElementById("ops-body");
  const sizes = document.getElementById("file-sizes");
  if (!tbody) return;

  const opRows = [
    {
      name: "First record / TTFR",
      row: results.ops.firstRow.row,
      col: results.ops.firstRow.columnar,
    },
    {
      name: `sum(${results.metricKey})`,
      row: results.ops.sum.row,
      col: results.ops.sum.columnar,
    },
    {
      name: `Record #${Math.min(results.n - 1, Math.floor(results.n / 2)).toLocaleString()} read`,
      row: results.ops.random.row,
      col: results.ops.random.columnar,
    },
  ];

  tbody.innerHTML = opRows
    .map((op) => {
      const rowCell =
        op.row != null
          ? fmtTime(op.row)
          : "—";
      let colCell = "—";
      if (op.col != null && typeof op.col === "number") colCell = fmtTime(op.col);
      else if (op.col && op.col.label) {
        colCell = `<span class="muted">${op.col.label}</span>`;
        if (op.col.detail) colCell += `<br><span class="hint">${op.col.detail}</span>`;
      }
      return `<tr><th scope="row">${op.name}</th><td>${rowCell}</td><td>${colCell}</td></tr>`;
    })
    .join("");

  if (sizes) {
    sizes.innerHTML = `
      <li>Input: <strong>${
        results.csvBytes > 0 ? fmtBytes(results.csvBytes) + " CSV" : "built-in sample (no CSV parse)"
      }</strong></li>
      <li>Row <code>.nxb</code>: <strong>${fmtBytes(results.rowSize)}</strong> (built in ${fmtTime(results.rowBuildMs)})</li>
      <li>Columnar <code>.nxb</code>: ${
        results.colSize != null
          ? `<strong>${fmtBytes(results.colSize)}</strong> (compiled in ${fmtTime(results.colBuildMs)})`
          : `<span class="muted">${results.colBuildNote || "not built"}</span>`
      }</li>
      <li>Records: <strong>${results.n.toLocaleString()}</strong></li>
      ${
        results.chartMs != null
          ? `<li>Chart from <code>col_buffer</code>: <strong>${fmtTime(results.chartMs)}</strong> (buffer walk + render)</li>`
          : ""
      }
    `;
  }
}

export function wireReportPage() {
  const fileInput = document.getElementById("csv-file");
  const paste = document.getElementById("csv-paste");
  const runBtn = document.getElementById("run-demo");
  const sampleSel = document.getElementById("sample-dataset");

  async function runWithResults(promise, label) {
    runBtn.disabled = true;
    try {
      const results = await promise;
      renderResults(results);
      document.getElementById("status").textContent =
        `Done — ${label} (${results.n.toLocaleString()} rows) on this device.`;
    } catch (e) {
      const msg = e?.message ?? (typeof e === "string" ? e : String(e));
      document.getElementById("status").textContent = `Error: ${msg}`;
      console.error(e);
    } finally {
      runBtn.disabled = false;
    }
  }

  runBtn.addEventListener("click", async () => {
    const pasted = paste.value.trim();
    if (pasted) {
      return runWithResults(runReportDemo(pasted), "your paste");
    }
    const sample = SAMPLE_DATASETS[sampleSel.value];
    document.getElementById("status").textContent =
      `Building ${sample.label}…`;
    const { headers, rows, types } = sample.generateRows(sample.rowCount);
    return runWithResults(
      runReportFromRecords(headers, rows, types, {
        metricKey: sample.metricKey,
        regionKey: sample.regionKey,
        chartKind: sample.chartKind,
        csvBytes: 0,
      }),
      sample.label,
    );
  });

  fileInput.addEventListener("change", async () => {
    const f = fileInput.files?.[0];
    if (!f) return;
    const text = await f.text();
    paste.value = "";
    await runWithResults(runReportDemo(text), f.name);
  });

  sampleSel.addEventListener("change", () => {
    const s = SAMPLE_DATASETS[sampleSel.value];
    document.getElementById("sample-desc").textContent = s.description;
  });
  document.getElementById("sample-desc").textContent =
    SAMPLE_DATASETS[sampleSel.value].description;
}
