import { NxsReader, NxsStreamReader, NxsObject, WIRE_SIGILS } from "/sdk/nxs.js";
import { compileNxsText, loadNxsDataset } from "/sdk/nxs_compile.js";

let demoRoot=null;
let demoQuery=(sel)=>document.querySelector(sel);
const $=(sel)=>demoQuery(sel);
let teardown=null;
export function wireExplorerPage(root){if(!root)return;if(root.dataset.demoWired==='1')return;teardown?.();root.dataset.demoWired='1';demoRoot=root;demoQuery=(sel)=>root.querySelector(sel);initDemo();teardown=()=>{delete root.dataset.demoWired;demoRoot=null;demoQuery=(sel)=>document.querySelector(sel);};}
export function unwireExplorerPage(){teardown?.();teardown=null;}
function initDemo(){
  // ── Constants ──────────────────────────────────────────────────────────────
  const ROW_HEIGHT = 22;
  const BUFFER_ROWS = 8;                 // rows rendered above/below viewport
  // Browsers cap CSS element height. Safe common ceiling is ~33_000_000 px
  // (Chrome, Safari, Firefox all safe well above 16M, but we stay conservative).
  const MAX_VIRTUAL_PX = 16_000_000;
  const NXS_MAGIC = 0x4E595842;
  // Default to the 1M fixture (~137 MB) — safe for all browsers / memory sizes.
  // Users can pick 10M explicitly from the toolbar if they have the RAM.
  const DEFAULT_FIXTURE = "/bench/fixtures/records_1000000.nxb";
  
  // ── DOM refs ──────────────────────────────────────────────────────────────
  const scrollEl = $("#scroll");
  const spacerEl = $("#spacer");
  const colHeaderEl = $("#col-header");
  const overlayEl = $("#overlay");
  const searchEl = $("#search");
  const searchBadge = $("#search-badge");
  const statusPos = $("#status-pos");
  const statusMatches = $("#status-matches");
  const statusFrame = $("#status-frame");
  const dropEl = $("#drop");
  const fileInput = $("#file");
  const fileInfoEl = $("#file-info");
  
  const fmtInt = n => n.toLocaleString();
  const fmtBytes = n =>
    n < 1024 ? `${n} B` :
    n < 1048576 ? `${(n/1024).toFixed(1)} KB` :
    n < 1073741824 ? `${(n/1048576).toFixed(1)} MB` :
                    `${(n/1073741824).toFixed(2)} GB`;
  
  // ── State ─────────────────────────────────────────────────────────────────
  let reader = null;          // NxsReader (binary mode, after stream finishes)
  let streamReader = null;    // NxsStreamReader while bytes are still arriving
  let recordOffsets = null;   // per-record NYXO offsets during streaming (Uint32Array)
  let cursor = null;          // reusable cursor for render
  /** @type {{ key: string, slot?: number, sigil?: number, json?: boolean }[]} */
  let columns = [];
  /** @type {{ key: string, slot?: number, sigil?: number, json?: boolean } | null} */
  let searchColumn = null;
  let jsonRecords = null;     // null = .nxb mode; else array of parsed JSON objects
  let recordCount = 0;
  let rawBuffer = null;       // underlying ArrayBuffer (to send to worker)
  let _lastLoadedUrl = null;  // set when loading from a known URL; null for drag-and-drop
  let loadGeneration = 0;     // cancels stale fetch/stream loads
  
  let virtualHeight = 0;      // actual CSS height of spacer
  let scrollScale = 1;        // recordCount / (virtualHeight / ROW_HEIGHT) when capped
  let totalRowsVirtual = 0;   // virtualHeight / ROW_HEIGHT
  
  // Render pool: ~60 DOM rows reused on every scroll. Each is { el, cells[], renderedIndex }.
  const rowPool = [];
  
  let currentMatches = null;  // Int32Array of matched record indices
  let currentMatchIdx = -1;
  let searchToken = 0;
  
  // Worker for search over .nxb (not used for JSON uploads).
  let worker = null;
  
  function teardownExplorerWorker() {
    if (worker) {
      worker.removeEventListener("message", onWorkerMessage);
      worker.terminate();
      worker = null;
    }
  }
  
  /** Discover column keys from the first records in a JSON array. */
  function jsonDiscoverColumns(arr) {
    const keys = [];
    const seen = new Set();
    const sample = Math.min(arr.length, 200);
    for (let i = 0; i < sample; i++) {
      const o = arr[i];
      if (!o || typeof o !== "object") continue;
      for (const k of Object.keys(o)) {
        if (!seen.has(k)) {
          seen.add(k);
          keys.push(k);
        }
      }
    }
    return keys.map(key => ({ key, json: true }));
  }
  
  /** Accepts gen_fixtures-style rows or any array of objects. */
  function jsonPrepare(parsed) {
    let arr;
    if (Array.isArray(parsed)) arr = parsed;
    else if (parsed && typeof parsed === "object" && Array.isArray(parsed.records)) arr = parsed.records;
    else {
      throw new Error("JSON must be an array of objects or { \"records\": [...] }");
    }
    const records = new Array(arr.length);
    for (let idx = 0; idx < arr.length; idx++) {
      const o = arr[idx];
      records[idx] = o && typeof o === "object" ? o : {};
    }
    return { records, columns: jsonDiscoverColumns(records) };
  }
  
  function formatJsonValue(v) {
    if (v === null || v === undefined) return "";
    if (typeof v === "boolean") return v ? "●" : "○";
    if (typeof v === "number") {
      return Number.isInteger(v) ? String(v) : v.toFixed(2);
    }
    if (typeof v === "object") return JSON.stringify(v);
    return String(v);
  }
  
  function updateViewportMetrics(resetScroll) {
    const ideal = recordCount * ROW_HEIGHT;
    if (ideal <= MAX_VIRTUAL_PX) {
      virtualHeight = ideal;
      scrollScale = 1;
    } else {
      virtualHeight = MAX_VIRTUAL_PX;
      scrollScale = recordCount * ROW_HEIGHT / virtualHeight;
    }
    totalRowsVirtual = Math.floor(virtualHeight / ROW_HEIGHT);
    spacerEl.style.height = `${virtualHeight}px`;
    if (resetScroll) scrollEl.scrollTop = 0;
  }
  
  function applyViewportLayout(name, sizeBytes, sourceLabel) {
    updateViewportMetrics(true);
    matchesSet.clear();
    currentMatches = null;
    currentMatchIdx = -1;
    lastFirstVr = -1;
    searchBadge.textContent = "";
    searchBadge.className = "badge";
    updateMatchesStatus();
  
    const tag = sourceLabel ? `${escapeHtml(name)} <span style="color:var(--muted)">(${sourceLabel})</span>` : escapeHtml(name);
    fileInfoEl.innerHTML = `<strong>${tag}</strong> — ${fmtBytes(sizeBytes)} — ${fmtInt(recordCount)} records`;
    overlayEl.classList.add("hide");
  
    ensureRowPool();
    scheduleRender();
  }
  
  function applyStreamingProgress(name, receivedBytes, totalBytes) {
    if (virtualHeight === 0 && recordCount > 0) {
      updateViewportMetrics(false);
      overlayEl.classList.add("hide");
      ensureRowPool();
    } else if (recordCount > 0) {
      updateViewportMetrics(false);
    }
    const total = totalBytes > 0 ? ` — ${fmtBytes(receivedBytes)} / ${fmtBytes(totalBytes)}` : receivedBytes > 0 ? ` — ${fmtBytes(receivedBytes)} received` : "";
    fileInfoEl.innerHTML =
      `<strong>${escapeHtml(name)}</strong> <span style="color:var(--warn)">(streaming)</span>${total} — ${fmtInt(recordCount)} records`;
    scheduleRender();
  }
  
  function isNxsObject(v) {
    return v && typeof v === "object" && typeof v.get === "function" && typeof v.toObject === "function";
  }
  
  /** Columns from embedded schema (streaming, before records are parsed). */
  function buildColumnsFromSchema(keys, keySigils) {
    return keys.map((key, slot) => ({
      key,
      slot,
      sigil: keySigils[slot],
    }));
  }
  
  /** Columns from fields present in record 0 (not the full schema dict). */
  function buildColumnsFromReader(r) {
    if (typeof r.record !== "function") {
      return buildColumnsFromSchema(r.keys, r.keySigils);
    }
    if (r.recordCount === 0) return [];
    const present = r.record(0).toObject();
    const topKeys = Object.keys(present);
  
    // Multi-record bench fixtures: flat fields on each record.
    if (r.recordCount > 1) {
      return topKeys.map(key => ({
        key,
        slot: r.keyIndex.get(key),
        sigil: r.keySigils[r.keyIndex.get(key)],
      }));
    }
  
    // Single compiled .nxs document — unwrap one nested root object (e.g. user { … }).
    if (topKeys.length === 1 && isNxsObject(present[topKeys[0]])) {
      const parent = topKeys[0];
      return Object.keys(present[parent].toObject()).map(key => ({ key, nestedIn: parent }));
    }
  
    return topKeys.map(key => ({
      key,
      slot: r.keyIndex.get(key),
      sigil: r.keySigils[r.keyIndex.get(key)],
    }));
  }
  
  function pickSearchColumn(cols) {
    const username = cols.find(c => c.key === "username");
    if (username && (!username.sigil || username.sigil === WIRE_SIGILS.str)) return username;
    return cols.find(c => c.sigil === WIRE_SIGILS.str) ?? cols[0] ?? null;
  }
  
  function applyColumns(cols) {
    columns = cols;
    searchColumn = pickSearchColumn(cols);
    colHeaderEl.replaceChildren();
    for (const col of cols) {
      const span = document.createElement("span");
      span.className = "col-cell";
      span.textContent = col.key;
      colHeaderEl.appendChild(span);
    }
    colHeaderEl.classList.remove("hide");
    colHeaderEl.setAttribute("aria-hidden", "false");
    const sk = searchColumn?.key ?? "field";
    searchEl.placeholder = `Search ${sk} substring…  (Ctrl/Cmd+F)`;
    for (const row of rowPool) row.el.remove();
    rowPool.length = 0;
    lastFirstVr = -1;
    ensureRowPool();
  }
  
  function clearColumns() {
    columns = [];
    searchColumn = null;
    colHeaderEl.replaceChildren();
    colHeaderEl.classList.add("hide");
    colHeaderEl.setAttribute("aria-hidden", "true");
    searchEl.placeholder = "Search…  (Ctrl/Cmd+F)";
  }
  
  function formatAnyValue(v) {
    if (v === null || v === undefined) return "";
    if (typeof v === "boolean") return v ? "●" : "○";
    if (typeof v === "number") {
      return Number.isInteger(v) ? String(v) : v.toFixed(2);
    }
    if (typeof v === "string") return v;
    if (typeof v === "bigint") return String(v);
    if (isNxsObject(v)) {
      const o = v.toObject();
      const parts = Object.entries(o).map(([k, val]) => `${k}=${formatAnyValue(val)}`);
      return parts.length <= 4 ? parts.join(" ") : `{${parts.length} fields}`;
    }
    if (Array.isArray(v)) {
      return v.length <= 3
        ? `[${v.map(formatAnyValue).join(", ")}]`
        : `[${v.length} items]`;
    }
    return String(v);
  }
  
  function formatNxbValue(accessor, col) {
    let v;
    if (col.nestedIn) {
      const parent = accessor.get(col.nestedIn);
      v = isNxsObject(parent) ? parent.get(col.key) : undefined;
    } else if (col.slot !== undefined) {
      v = accessor.get(col.key);
    } else {
      v = accessor.get(col.key);
    }
    return formatAnyValue(v);
  }
  
  function nxbCellClass(v) {
    if (typeof v === "boolean") return v ? "col-cell bool on" : "col-cell bool";
    return "col-cell";
  }
  
  // ── Virtual scroller ─────────────────────────────────────────────────────
  //
  // Strategy: scrollEl has a fixed viewport height; spacerEl has a height of
  // recordCount * ROW_HEIGHT (capped at MAX_VIRTUAL_PX). When capped, each
  // ROW_HEIGHT band maps to ~scrollScale records; we render one pool row per
  // virtual row vr at translateY(vr * ROW_HEIGHT) with record floor(vr * scrollScale).
  // The pool is sized once; only textContent changes per frame.
  
  function computePoolSize() {
    const viewportRows = Math.ceil(scrollEl.clientHeight / ROW_HEIGHT);
    // Window = visible + buffer on each side.
    return viewportRows + BUFFER_ROWS * 2;
  }
  
  function ensureRowPool() {
    const target = computePoolSize();
    const colCount = Math.max(columns.length, 1);
    while (rowPool.length < target) {
      const el = document.createElement("div");
      el.className = "row";
      const cells = [];
      for (let c = 0; c < colCount; c++) {
        const span = document.createElement("span");
        span.className = "col-cell";
        cells.push(span);
      }
      el.append(...cells);
      spacerEl.appendChild(el);
      rowPool.push({ el, cells, renderedIndex: -1 });
    }
    // If pool is larger than needed after a resize, hide the surplus.
    for (let i = target; i < rowPool.length; i++) {
      rowPool[i].el.style.display = "none";
    }
    for (let i = 0; i < target; i++) {
      rowPool[i].el.style.display = "";
    }
  }
  
  // Inverse: record index -> scroll position.
  function recordIdxToScrollTop(idx) {
    return Math.floor(idx / scrollScale) * ROW_HEIGHT;
  }
  
  // Track the currently-rendered *virtual row* window (spacer slots). When
  // scrollScale > 1, consecutive record indices share one slot — we must index
  // by virtual row, not raw record index, or multiple pool rows get the same
  // translateY and stack (broken UI on 10M+ files).
  let lastFirstVr = -1;
  let lastWindowSize = 0;
  let frameAvgMs = 0;
  const matchesSet = new Set();  // O(1) lookup of record idx -> is-a-match
  
  let rafPending = false;
  function scheduleRender() {
    if (rafPending) return;
    rafPending = true;
    requestAnimationFrame(render);
  }
  
  function render() {
    rafPending = false;
    if (!reader && !streamReader && jsonRecords === null) return;
    if (columns.length === 0) return;
  
    const t0 = performance.now();
    const scrollTop = scrollEl.scrollTop;
    // Index by virtual row (one spacer band per ROW_HEIGHT px). Each band shows
    // the first record in that band: floor(vr * scrollScale).
    const firstVr = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - BUFFER_ROWS);
    const poolSize = rowPool.length;
    const maxVr = totalRowsVirtual - 1;
    const lastVr = Math.min(maxVr, firstVr + poolSize - 1);
    const activeRows = firstVr > maxVr ? 0 : lastVr - firstVr + 1;
  
    // Fast path: nothing changed.
    if (firstVr === lastFirstVr && lastWindowSize === activeRows) {
      return;
    }
    lastFirstVr = firstVr;
    lastWindowSize = activeRows;
  
    for (let i = 0; i < poolSize; i++) {
      const vr = firstVr + i;
      const row = rowPool[i];
      if (vr < 0 || vr > maxVr) {
        if (row.renderedIndex !== -1) {
          row.el.style.display = "none";
          row.renderedIndex = -1;
        }
        continue;
      }
      const idx = Math.min(recordCount - 1, Math.floor(vr * scrollScale));
      row.el.style.display = "";
      row.renderedIndex = idx;
      // One pool row per virtual band — never duplicate translateY.
      const virtualTop = vr * ROW_HEIGHT;
      row.el.style.transform = `translateY(${virtualTop}px)`;
      renderRow(row, idx);
    }
  
    const elapsed = performance.now() - t0;
    frameAvgMs = frameAvgMs * 0.8 + elapsed * 0.2;
    updateStatus(scrollTop);
  }
  
  function renderRow(row, idx) {
    const ncols = columns.length;
    if (jsonRecords !== null) {
      const rec = jsonRecords[idx] ?? {};
      for (let c = 0; c < ncols; c++) {
        const col = columns[c];
        const cell = row.cells[c];
        const v = rec[col.key];
        cell.textContent = formatJsonValue(v);
        if (typeof v === "boolean") {
          cell.className = v ? "col-cell bool on" : "col-cell bool";
        } else {
          cell.className = "col-cell";
        }
      }
    } else {
      let accessor;
      if (streamReader && recordOffsets) {
        accessor = new NxsObject(streamReader, recordOffsets[idx]);
      } else {
        cursor.seek(idx);
        accessor = cursor;
      }
      for (let c = 0; c < ncols; c++) {
        const col = columns[c];
        const cell = row.cells[c];
        let raw;
        if (col.nestedIn) {
          const parent = accessor.get(col.nestedIn);
          raw = isNxsObject(parent) ? parent.get(col.key) : undefined;
        } else {
          raw = accessor.get(col.key);
        }
        cell.textContent = formatAnyValue(raw);
        cell.className = nxbCellClass(raw);
      }
    }
  
    // Match highlighting
    let cls = "row";
    if (matchesSet.has(idx)) {
      cls = currentMatches && currentMatchIdx >= 0 && currentMatches[currentMatchIdx] === idx
        ? "row current-match" : "row match";
    }
    if (row.el.className !== cls) row.el.className = cls;
  }
  
  function updateStatus(scrollTop) {
    // Center of viewport as a record index (works for scrollScale > 1).
    const viewportRows = Math.ceil(scrollEl.clientHeight / ROW_HEIGHT);
    const topVr = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT));
    const centerVr =
      totalRowsVirtual <= 0 ? 0 : Math.min(totalRowsVirtual - 1, topVr + (viewportRows >> 1));
    const centerIdx =
      recordCount <= 0 ? 0 : Math.min(recordCount - 1, Math.floor(centerVr * scrollScale));
    statusPos.innerHTML = `line <strong>${fmtInt(centerIdx + 1)}</strong> of <strong>${fmtInt(recordCount)}</strong>`;
    statusFrame.textContent = `render ${frameAvgMs.toFixed(1)} ms`;
    statusFrame.className = frameAvgMs < 4 ? "ok" : frameAvgMs < 10 ? "warn" : "bad";
  }
  
  // ── Loading ──────────────────────────────────────────────────────────────
  function attachWorkerForNxb() {
    ensureWorker();
    if (!worker || !searchColumn) return;
    const searchKey = searchColumn.key;
    // Only re-fetch URL for binary .nxb assets — never for .nxs text (worker needs compiled bytes).
    if (_lastLoadedUrl && !pathLooksLikeNxs(_lastLoadedUrl)) {
      worker.postMessage({ type: "load-url", url: _lastLoadedUrl, searchKey });
    } else if (rawBuffer) {
      const copy = rawBuffer.slice(0);
      worker.postMessage({ type: "load", buffer: copy, searchKey }, [copy]);
    }
  }
  
  async function loadFromReadableStream(body, name, sizeBytes) {
    const gen = ++loadGeneration;
    jsonRecords = null;
    reader = null;
    cursor = null;
    streamReader = null;
    recordOffsets = null;
    rawBuffer = null;
    recordCount = 0;
    virtualHeight = 0;
    clearColumns();
    teardownExplorerWorker();
  
    overlayEl.classList.remove("hide");
    overlayEl.textContent = `Loading ${name}…`;
  
    let sr;
    try {
      sr = new NxsStreamReader({
        onSchema(keys, keySigils) {
          if (gen !== loadGeneration) return;
          applyColumns(buildColumnsFromSchema(keys, keySigils));
          if (_lastLoadedUrl) attachWorkerForNxb();
        },
        onRecord(obj, idx) {
          if (gen !== loadGeneration) return;
          if (!recordOffsets) recordOffsets = new Uint32Array(4096);
          if (idx >= recordOffsets.length) {
            const grown = new Uint32Array(recordOffsets.length * 2);
            grown.set(recordOffsets);
            recordOffsets = grown;
          }
          recordOffsets[idx] = obj.offset;
          recordCount = idx + 1;
          if (idx === 0 || (idx & 0x3fff) === 0) {
            applyStreamingProgress(name, 0, sizeBytes);
          }
        },
        onError(err) {
          if (gen !== loadGeneration) return;
          showError(`Failed to parse ${name}: ${err.message}`);
        },
      });
      streamReader = sr;
  
      const webReader = body.getReader();
      let received = 0;
      while (true) {
        const { done, value } = await webReader.read();
        if (gen !== loadGeneration) {
          await webReader.cancel?.();
          return;
        }
        if (done) break;
        received += value.byteLength;
        sr.push(value);
        if ((received & 0xfffff) < value.byteLength || recordCount === 1) {
          overlayEl.textContent =
            `Loading ${name}… ${fmtBytes(received)}${sizeBytes ? ` / ${fmtBytes(sizeBytes)}` : ""} — ${fmtInt(recordCount)} records`;
          if (recordCount > 0) applyStreamingProgress(name, received, sizeBytes);
        }
      }
      if (gen !== loadGeneration) return;
  
      reader = sr.finish();
      streamReader = null;
      recordOffsets = null;
      recordCount = reader.recordCount;
      cursor = reader.cursor();
      applyColumns(buildColumnsFromReader(reader));
      const view = reader.bytes;
      rawBuffer = view.buffer.slice(view.byteOffset, view.byteOffset + view.byteLength);
      attachWorkerForNxb();
      applyViewportLayout(name, sizeBytes || received);
    } catch (e) {
      if (gen !== loadGeneration) return;
      showError(`Failed to load ${name}: ${e.message}`);
    }
  }
  
  async function loadBuffer(buffer, name, sizeBytes, sourceLabel = null) {
    const gen = ++loadGeneration;
    jsonRecords = null;
    streamReader = null;
    recordOffsets = null;
    virtualHeight = 0;
    clearColumns();
    // Validate magic.
    const dv = new DataView(buffer);
    if (dv.byteLength < 4 || dv.getUint32(0, true) !== NXS_MAGIC) {
      showError(`Not an NXS file: ${name}. Magic bytes don't match.`);
      return;
    }
  
    try {
      reader = new NxsReader(buffer);
    } catch (e) {
      showError(`Failed to parse ${name}: ${e.message}`);
      return;
    }
    if (gen !== loadGeneration) return;
  
    rawBuffer = buffer;
    recordCount = reader.recordCount;
    cursor = reader.cursor();
    applyColumns(buildColumnsFromReader(reader));
    attachWorkerForNxb();
    applyViewportLayout(name, sizeBytes, sourceLabel);
  }
  
  async function loadNxsString(text, name, sourceBytes) {
    const gen = ++loadGeneration;
    _lastLoadedUrl = null;
    overlayEl.classList.remove("hide");
    overlayEl.textContent = `Compiling ${name}…`;
    try {
      const u8 = await compileNxsText(text);
      if (gen !== loadGeneration) return;
      const buf = u8.buffer.slice(u8.byteOffset, u8.byteOffset + u8.byteLength);
      const display = name.replace(/\.nxs$/i, ".nxb");
      await loadBuffer(buf, display, buf.byteLength, "NXS source");
    } catch (e) {
      if (gen !== loadGeneration) return;
      showError(`Compile failed (${name}): ${e.message}`);
    }
  }
  
  async function loadJsonString(text, name, sizeBytes) {
    loadGeneration++;
    let parsed;
    try {
      parsed = JSON.parse(text);
    } catch (e) {
      showError(`Invalid JSON in ${name}: ${e.message}`);
      return;
    }
  
    let prepared;
    try {
      prepared = jsonPrepare(parsed);
    } catch (e) {
      showError(`${name}: ${e.message}`);
      return;
    }
  
    teardownExplorerWorker();
    reader = null;
    streamReader = null;
    recordOffsets = null;
    cursor = null;
    rawBuffer = null;
    virtualHeight = 0;
    jsonRecords = prepared.records;
    recordCount = jsonRecords.length;
    _lastLoadedUrl = null;
    applyColumns(prepared.columns);
    applyViewportLayout(name, sizeBytes, "JSON");
  }
  
  function ensureWorker() {
    if (worker) return;
    try {
      worker = new Worker(new URL("../workers/explorer_worker.js", import.meta.url), { type: "module" });
      worker.addEventListener("message", onWorkerMessage);
    } catch (e) {
      console.warn("Worker unavailable, falling back to main-thread search:", e);
      worker = null;
    }
  }
  
  function onWorkerMessage(ev) {
    const msg = ev.data;
    if (msg.type === "loaded") {
      // Worker has its own reader now; searches will use it.
    } else if (msg.type === "load-progress") {
      if (!searchEl.value.trim()) {
        statusMatches.textContent = `indexing ${fmtInt(msg.parsed)} records for search…`;
      }
    } else if (msg.type === "load-error") {
      console.warn("Explorer worker load failed:", msg.message);
    } else if (msg.type === "search-progress") {
      if (msg.token !== searchToken) return;
      const pct = ((msg.scanned / msg.total) * 100).toFixed(0);
      searchBadge.textContent = `scanning ${pct}% · ${fmtInt(msg.matches)} so far`;
      searchBadge.className = "badge searching";
    } else if (msg.type === "search-done") {
      if (msg.token !== searchToken) return;
      if (msg.aborted) return;
      currentMatches = msg.matches;
      matchesSet.clear();
      for (let i = 0; i < currentMatches.length; i++) matchesSet.add(currentMatches[i]);
      if (currentMatches.length > 0) {
        currentMatchIdx = 0;
        searchBadge.textContent = `${fmtInt(currentMatches.length)} matches`;
        searchBadge.className = "badge active";
        jumpToRecord(currentMatches[0]);
      } else {
        currentMatchIdx = -1;
        searchBadge.textContent = "no matches";
        searchBadge.className = "badge";
      }
      updateMatchesStatus();
      lastFirstVr = -1;
      scheduleRender();
    }
  }
  
  // ── Search (main-thread fallback) ────────────────────────────────────────
  //
  // Only used if Worker isn't available. For 10M records this will block the
  // UI for ~0.5–1 s which is acceptable for a fallback path.
  function searchMainThread(query) {
    const token = ++searchToken;
    if (!query) {
      currentMatches = null;
      matchesSet.clear();
      currentMatchIdx = -1;
      searchBadge.textContent = "";
      searchBadge.className = "badge";
      updateMatchesStatus();
      lastFirstVr = -1;
      scheduleRender();
      return;
    }
    searchBadge.textContent = "scanning…";
    searchBadge.className = "badge searching";
  
    // Defer to a microtask so the UI paints the "scanning" badge first.
    Promise.resolve().then(() => {
      if (token !== searchToken) return;
      const needle = query.toLowerCase();
      const results = [];
      if (!searchColumn) return;
      if (jsonRecords !== null) {
        const key = searchColumn.key;
        for (let i = 0; i < recordCount; i++) {
          const u = jsonRecords[i][key];
          if (u != null && String(u).toLowerCase().indexOf(needle) !== -1) results.push(i);
        }
      } else {
        const cur = reader.cursor();
        for (let i = 0; i < recordCount; i++) {
          cur.seek(i);
          let u;
          if (searchColumn.nestedIn) {
            const parent = cur.get(searchColumn.nestedIn);
            u = isNxsObject(parent) ? parent.get(searchColumn.key) : undefined;
          } else {
            u = cur.get(searchColumn.key);
          }
          if (u != null && String(u).toLowerCase().indexOf(needle) !== -1) results.push(i);
        }
      }
      if (token !== searchToken) return;
      currentMatches = new Int32Array(results);
      matchesSet.clear();
      for (const i of results) matchesSet.add(i);
      currentMatchIdx = results.length ? 0 : -1;
      searchBadge.textContent = results.length ? `${fmtInt(results.length)} matches` : "no matches";
      searchBadge.className = results.length ? "badge active" : "badge";
      if (results.length) jumpToRecord(results[0]);
      updateMatchesStatus();
      lastFirstVr = -1;
      scheduleRender();
    });
  }
  
  function startSearch(query) {
    query = query.trim();
    // Clear old match state immediately so the UI feels responsive.
    matchesSet.clear();
    currentMatchIdx = -1;
    lastFirstVr = -1;
  
    if (streamReader) {
      searchBadge.textContent = query ? "waiting for file…" : "";
      searchBadge.className = "badge";
      updateMatchesStatus();
      return;
    }
  
    if (!query) {
      searchToken++;  // cancel any in-flight
      currentMatches = null;
      searchBadge.textContent = "";
      searchBadge.className = "badge";
      updateMatchesStatus();
      scheduleRender();
      return;
    }
  
    if (jsonRecords !== null) {
      searchMainThread(query);
      return;
    }
  
    if (worker) {
      searchToken++;
      searchBadge.textContent = "scanning…";
      searchBadge.className = "badge searching";
      worker.postMessage({ type: "search", query, token: searchToken });
    } else {
      searchMainThread(query);
    }
  }
  
  function updateMatchesStatus() {
    if (!currentMatches || currentMatches.length === 0) {
      statusMatches.textContent = searchEl.value ? "No matches" : "No search active";
      return;
    }
    const pos = currentMatchIdx + 1;
    statusMatches.innerHTML = `match <strong>${fmtInt(pos)}</strong> of <strong>${fmtInt(currentMatches.length)}</strong>`;
  }
  
  function nextMatch(dir) {
    if (!currentMatches || currentMatches.length === 0) return;
    currentMatchIdx = (currentMatchIdx + dir + currentMatches.length) % currentMatches.length;
    jumpToRecord(currentMatches[currentMatchIdx]);
    updateMatchesStatus();
    lastFirstVr = -1;
    scheduleRender();
  }
  
  // ── Navigation ───────────────────────────────────────────────────────────
  function jumpToRecord(idx) {
    const clamped = Math.max(0, Math.min(recordCount - 1, idx));
    // Center the target in the viewport when possible.
    const viewportRows = Math.ceil(scrollEl.clientHeight / ROW_HEIGHT);
    const target = recordIdxToScrollTop(clamped) - (viewportRows >> 1) * ROW_HEIGHT;
    scrollEl.scrollTop = Math.max(0, target);
    scheduleRender();
  }
  
  // ── Event wiring ─────────────────────────────────────────────────────────
  scrollEl.addEventListener("scroll", scheduleRender, { passive: true });
  window.addEventListener("resize", () => { ensureRowPool(); lastFirstVr = -1; scheduleRender(); });
  
  // Debounced search (100ms).
  let searchDebounce = null;
  searchEl.addEventListener("input", () => {
    if (searchDebounce) clearTimeout(searchDebounce);
    searchDebounce = setTimeout(() => startSearch(searchEl.value), 100);
  });
  
  searchEl.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      if (currentMatches && currentMatches.length) {
        nextMatch(e.shiftKey ? -1 : 1);
      } else {
        // Force immediate search if no matches yet.
        if (searchDebounce) clearTimeout(searchDebounce);
        startSearch(searchEl.value);
      }
    } else if (e.key === "Escape") {
      searchEl.value = "";
      startSearch("");
      scrollEl.focus();
    }
  });
  
  $("#next-match").addEventListener("click", () => nextMatch(1));
  $("#prev-match").addEventListener("click", () => nextMatch(-1));
  
  $("#jump-btn").addEventListener("click", () => {
    const v = parseInt($("#jump-input").value, 10);
    if (Number.isFinite(v)) jumpToRecord(v - 1);
  });
  $("#jump-input").addEventListener("keydown", (e) => {
    if (e.key === "Enter") { e.preventDefault(); $("#jump-btn").click(); }
  });
  
  // Keyboard navigation when the scroll pane (or body) is focused.
  document.addEventListener("keydown", (e) => {
    // Cmd/Ctrl+F focuses search.
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f") {
      e.preventDefault();
      searchEl.focus();
      searchEl.select();
      return;
    }
    // Don't hijack keys while typing in an input.
    if (e.target.tagName === "INPUT" || e.target.tagName === "TEXTAREA") return;
    if (!reader && !streamReader && jsonRecords === null) return;
  
    const viewportRows = Math.ceil(scrollEl.clientHeight / ROW_HEIGHT);
    const page = Math.max(1, viewportRows - 2);
    let handled = true;
    switch (e.key) {
      case "ArrowDown": scrollEl.scrollTop += ROW_HEIGHT; break;
      case "ArrowUp":   scrollEl.scrollTop -= ROW_HEIGHT; break;
      case "PageDown":  scrollEl.scrollTop += page * ROW_HEIGHT; break;
      case "PageUp":    scrollEl.scrollTop -= page * ROW_HEIGHT; break;
      case "Home":      scrollEl.scrollTop = 0; break;
      case "End":       scrollEl.scrollTop = virtualHeight; break;
      default: handled = false;
    }
    if (handled) { e.preventDefault(); scheduleRender(); }
  });
  
  // Drag-and-drop.
  ;["dragenter", "dragover"].forEach(t => dropEl.addEventListener(t, (e) => {
    e.preventDefault(); e.stopPropagation();
    dropEl.classList.add("drag");
  }));
  ;["dragleave", "drop"].forEach(t => dropEl.addEventListener(t, (e) => {
    e.preventDefault(); e.stopPropagation();
    if (t === "dragleave" && e.target !== dropEl) return;
    dropEl.classList.remove("drag");
  }));
  dropEl.addEventListener("drop", async (e) => {
    const f = e.dataTransfer?.files?.[0];
    if (f) await loadFile(f);
  });
  
  $("#pick").addEventListener("click", () => fileInput.click());
  fileInput.addEventListener("change", async (e) => {
    const f = e.target.files?.[0];
    if (f) await loadFile(f);
    fileInput.value = "";
  });
  
  function fileLooksLikeJson(file) {
    const n = (file.name || "").toLowerCase();
    const t = (file.type || "").toLowerCase();
    return n.endsWith(".json") || t === "application/json" || t === "text/json";
  }
  
  function fileLooksLikeNxs(file) {
    const n = (file.name || "").toLowerCase();
    return n.endsWith(".nxs");
  }
  
  function pathLooksLikeNxs(path) {
    return (path || "").toLowerCase().endsWith(".nxs");
  }
  
  async function loadFile(file) {
    overlayEl.classList.remove("hide");
    overlayEl.textContent = `Loading ${file.name}…`;
    try {
      _lastLoadedUrl = null;  // drag-and-drop: no URL for worker to re-fetch
      if (fileLooksLikeNxs(file)) {
        const text = await file.text();
        await loadNxsString(text, file.name, file.size);
      } else if (fileLooksLikeJson(file)) {
        const text = await file.text();
        await loadJsonString(text, file.name, file.size);
      } else if (typeof file.stream === "function") {
        await loadFromReadableStream(file.stream(), file.name, file.size);
      } else {
        const buf = await file.arrayBuffer();
        await loadBuffer(buf, file.name, file.size);
      }
    } catch (e) {
      showError(`Failed to load ${file.name}: ${e.message}`);
    }
  }
  
  function showError(msg) {
    overlayEl.textContent = msg;
    overlayEl.classList.remove("hide");
    console.error(msg);
  }
  
  function escapeHtml(s) {
    return s.replace(/[&<>"']/g, c => ({ "&":"&amp;", "<":"&lt;", ">":"&gt;", "\"":"&quot;", "'":"&#39;" }[c]));
  }
  
  // ── Boot: show quick-load buttons, don't auto-fetch ───────────────────────
  ensureRowPool();
  
  const QUICK_SIZES = [
    { label: "1,000 records (~127 KB)",    path: "/bench/fixtures/records_1000.nxb" },
    { label: "10,000 records (~1.2 MB)",   path: "/bench/fixtures/records_10000.nxb" },
    { label: "100,000 records (~13 MB)",   path: "/bench/fixtures/records_100000.nxb" },
    { label: "1,000,000 records (~132 MB)", path: "/bench/fixtures/records_1000000.nxb" },
    { label: "10,000,000 records (~1.3 GB)", path: "/bench/fixtures/records_10000000.nxb" },
  ];
  
  const QUICK_NXS = [
    { label: "Example: user_profile.nxs (1 doc — compile in browser)", path: "/examples/user_profile.nxs" },
  ];
  
  overlayEl.innerHTML = `
    <div style="max-width:480px;text-align:center">
      <p style="margin:0 0 16px;font-size:15px;color:var(--muted)">
        Drop <code>.nxb</code> / <code>.nxs</code> / <code>.json</code>, click <strong>Choose file</strong>,<br>
        or load a built-in fixture:
      </p>
      <div style="display:flex;flex-direction:column;gap:8px">
        ${QUICK_NXS.map(s => `<button
          class="load-btn load-btn--nxs"
          style="background:var(--panel-2);border:1px solid var(--accent);color:var(--text);
                 border-radius:6px;padding:8px 16px;cursor:pointer;font-family:inherit;font-size:13px"
          data-path="${s.path}">${s.label}</button>`).join("")}
        ${QUICK_SIZES.map(s => `<button
          class="load-btn"
          style="background:var(--panel-2);border:1px solid var(--border);color:var(--text);
                 border-radius:6px;padding:8px 16px;cursor:pointer;font-family:inherit;font-size:13px"
          data-path="${s.path}">${s.label}</button>`).join("")}
      </div>
    </div>
  `;
  
  overlayEl.querySelectorAll(".load-btn").forEach(btn => {
    btn.addEventListener("click", async () => {
      const path = btn.dataset.path;
      const name = path.split("/").pop();
      overlayEl.innerHTML = `<div><p>Fetching ${name}…</p></div>`;
      try {
        if (pathLooksLikeNxs(path)) {
          const gen = ++loadGeneration;
          _lastLoadedUrl = null;
          overlayEl.textContent = `Fetching ${name}…`;
          const { reader: r, buffer } = await loadNxsDataset(path);
          if (gen !== loadGeneration) return;
          jsonRecords = null;
          streamReader = null;
          recordOffsets = null;
          cursor = null;
          rawBuffer = buffer;
          reader = r;
          recordCount = reader.recordCount;
          cursor = reader.cursor();
          applyColumns(buildColumnsFromReader(reader));
          attachWorkerForNxb();
          applyViewportLayout(name, buffer.byteLength, "NXS source");
        } else {
          _lastLoadedUrl = path;
          const res = await fetch(path);
          if (!res.ok) throw new Error(`HTTP ${res.status}`);
          const len = parseInt(res.headers.get("content-length") || "0", 10) || 0;
          if (res.body) {
            await loadFromReadableStream(res.body, name, len);
          } else {
            const buf = await res.arrayBuffer();
            await loadBuffer(buf, name, buf.byteLength);
          }
        }
      } catch (e) {
        overlayEl.innerHTML = `
          <div>
            <p style="color:var(--bad)">Failed to load: ${escapeHtml(e.message)}</p>
            <p>Drop a <code>.nxb</code> file above or click <strong>Choose file</strong>.</p>
          </div>
        `;
      }
    });
  });
}
