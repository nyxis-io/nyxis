import { NxsReader, NxsStreamReader, NxsObject, SparseBytes, WIRE_SIGILS, HINT_RANDOM } from "/sdk/nxs.js";
import { compileNxsText, loadNxsDataset } from "/sdk/nxs_compile.js";
import {
  buildExplorerSearchSpec,
  explorerNxbSearchSource,
  scanExplorerNxbRecords,
} from "../utils/explorerNxbSearch.js";

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
  const NYXO_MAGIC = 0x4E59584F; // NYXO object header
  // Default to the 1M fixture (~137 MB) — safe for all browsers / memory sizes.
  // Users can pick 10M explicitly from the toolbar if they have the RAM.
  const DEFAULT_FIXTURE = "/bench/fixtures/records_1000000.nxb";
  /** Below this size, seal stream to NxsReader after download (worker search). Larger stays on stream view. */
  const STREAM_SEAL_BYTES = 200 * 1024 * 1024;
  /** Coalesce streamed IDB chunks so search does not walk tens of thousands of tiny entries. */
  const IDB_WRITE_COALESCE_BYTES = 4 * 1024 * 1024;
  /** If the file fits, materialize IDB → one ArrayBuffer for in-memory search (fast path). */
  const SEARCH_MATERIALIZE_MAX_BYTES = Math.floor(1.6 * 1024 * 1024 * 1024);
  const NXS_FOOTER_MAGIC = 0x2153584E; // NXS!
  const LOCAL_CACHE_DB_PREFIX = "nyxis-explorer-nxb-";

  function fixtureSizeHint(path, headLen) {
    if (headLen > 0) return headLen;
    const m = (path || "").match(/records_(\d+)\.nxb$/i);
    if (!m) return 0;
    const n = parseInt(m[1], 10);
    if (n >= 10_000_000) return 1_400_000_000;
    if (n >= 1_000_000) return 140_000_000;
    if (n >= 100_000) return 13_000_000;
    if (n >= 10_000) return 1_200_000;
    if (n >= 1_000) return 127_000;
    return 0;
  }

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
  const telOpen = $("#tel-open");
  const telFilter = $("#tel-filter");
  const telMemory = $("#tel-memory");
  const telStreamed = $("#tel-streamed");
  const telLoaded = $("#tel-loaded");
  const telFormat = $("#tel-format");
  const compareBar = $("#compare-bar");
  const compareNxs = $("#compare-nxs");
  const compareJson = $("#compare-json");
  const compareRunBtn = $("#compare-run");
  const dropEl = $("#drop");
  const fileInput = $("#file");
  const fileInfoEl = $("#file-info");
  
  const fmtInt = n => n.toLocaleString();
  const fmtBytes = n =>
    n < 1024 ? `${n} B` :
    n < 1048576 ? `${(n/1024).toFixed(1)} KB` :
    n < 1073741824 ? `${(n/1048576).toFixed(1)} MB` :
                    `${(n/1073741824).toFixed(2)} GB`;

  function idbRequest(req) {
    return new Promise((resolve, reject) => {
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error || new Error("IndexedDB request failed"));
    });
  }

  function idbTxDone(tx) {
    return new Promise((resolve, reject) => {
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error || new Error("IndexedDB transaction failed"));
      tx.onabort = () => reject(tx.error || new Error("IndexedDB transaction aborted"));
    });
  }

  async function openLocalNxbCache(label) {
    if (!("indexedDB" in globalThis)) return null;
    const safe = label.replace(/[^a-z0-9_-]+/gi, "-").slice(0, 48);
    const dbName = `${LOCAL_CACHE_DB_PREFIX}${safe}-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    const req = indexedDB.open(dbName, 1);
    req.onupgradeneeded = () => {
      req.result.createObjectStore("chunks", { keyPath: "start" });
    };
    const db = await idbRequest(req);

    let coalesceStart = -1;
    let coalesceParts = [];
    let coalesceLen = 0;

    async function flushCoalesce() {
      if (coalesceLen === 0) return;
      const merged = new Uint8Array(coalesceLen);
      let off = 0;
      for (const part of coalesceParts) {
        merged.set(part, off);
        off += part.byteLength;
      }
      const tx = db.transaction("chunks", "readwrite");
      tx.objectStore("chunks").put({ start: coalesceStart, data: merged });
      await idbTxDone(tx);
      coalesceParts = [];
      coalesceLen = 0;
      coalesceStart = -1;
    }

    return {
      dbName,
      async write(start, data) {
        const slice = data.slice();
        if (coalesceStart < 0) coalesceStart = start;
        else if (coalesceStart + coalesceLen !== start) {
          await flushCoalesce();
          coalesceStart = start;
        }
        coalesceParts.push(slice);
        coalesceLen += slice.byteLength;
        if (coalesceLen >= IDB_WRITE_COALESCE_BYTES) await flushCoalesce();
      },
      async finalize() {
        await flushCoalesce();
      },
      async getAllSorted() {
        await flushCoalesce();
        const tx = db.transaction("chunks", "readonly");
        const all = await idbRequest(tx.objectStore("chunks").getAll());
        all.sort((a, b) => a.start - b.start);
        return all;
      },
      async materialize(fileSize) {
        const chunks = await this.getAllSorted();
        const out = new Uint8Array(fileSize);
        for (const chunk of chunks) {
          out.set(chunk.data, chunk.start);
        }
        return out;
      },
      async readRange(start, len) {
        return new Promise((resolve, reject) => {
          const out = new Uint8Array(len);
          const tx = db.transaction("chunks", "readonly");
          const store = tx.objectStore("chunks");
          let pos = start;
          let written = 0;
          let failed = false;
          const fail = (err) => {
            if (failed) return;
            failed = true;
            try { tx.abort(); } catch {}
            reject(err);
          };

          const firstReq = store.openCursor(IDBKeyRange.upperBound(start), "prev");
          firstReq.onerror = () => fail(firstReq.error || new Error("IndexedDB cursor failed"));
          firstReq.onsuccess = () => {
            const first = firstReq.result?.value;
            if (!first || start < first.start || start >= first.start + first.data.byteLength) {
              fail(new Error(`local cache miss at byte ${start}`));
              return;
            }
            const scanReq = store.openCursor(IDBKeyRange.lowerBound(first.start));
            scanReq.onerror = () => fail(scanReq.error || new Error("IndexedDB cursor failed"));
            scanReq.onsuccess = () => {
              if (failed) return;
              const cursor = scanReq.result;
              if (!cursor) {
                fail(new Error(`local cache miss at byte ${pos}`));
                return;
              }
              const chunk = cursor.value;
              if (pos < chunk.start) {
                fail(new Error(`local cache gap at byte ${pos}`));
                return;
              }
              if (pos < chunk.start + chunk.data.byteLength) {
                const inChunk = pos - chunk.start;
                const n = Math.min(len - written, chunk.data.byteLength - inChunk);
                out.set(chunk.data.subarray(inChunk, inChunk + n), written);
                pos += n;
                written += n;
              }
              if (written >= len) {
                resolve(out);
                return;
              }
              cursor.continue();
            };
          };
        });
      },
      close() {
        db.close();
      },
      async destroy() {
        db.close();
        await idbRequest(indexedDB.deleteDatabase(dbName));
      },
    };
  }

  async function closeLocalNxbCache() {
    if (!localNxbCache) return;
    const cache = localNxbCache;
    localNxbCache = null;
    try {
      await cache.destroy();
    } catch {
      cache.close?.();
    }
  }

  async function openCachedNxbReader(cache, fileSize) {
    const fetchRange = (byteStart, byteLength) => cache.readRange(byteStart, byteLength);
    const probeLen = Math.min(4096, fileSize);
    const probe = await fetchRange(fileSize - probeLen, probeLen);
    const probeView = new DataView(probe.buffer, probe.byteOffset, probe.byteLength);
    if (probeView.getUint32(probe.byteLength - 4, true) !== NXS_FOOTER_MAGIC) {
      throw new Error("local cache footer magic mismatch");
    }
    let tailPtr = Number(probeView.getBigUint64(probe.byteLength - 12, true));
    const headerLen = Math.min(262144, tailPtr > 0 ? tailPtr : fileSize);
    const header = await fetchRange(0, headerLen);
    const hView = new DataView(header.buffer, header.byteOffset, header.byteLength);
    if (hView.getUint32(0, true) !== NXS_MAGIC) {
      throw new Error("local cache preamble magic mismatch");
    }
    const preambleTail = Number(hView.getBigUint64(16, true));
    if (preambleTail > 0) tailPtr = preambleTail;
    if (tailPtr <= 0 || tailPtr >= fileSize) {
      throw new Error("local cache has invalid tail pointer");
    }
    const tail = await fetchRange(tailPtr, fileSize - tailPtr);
    const sparse = new SparseBytes(
      fileSize,
      [{ start: 0, data: header }, { start: tailPtr, data: tail }],
      fetchRange,
    );
    return new NxsReader(sparse, { fetchRange, hint: HINT_RANDOM });
  }
  
  // ── State ─────────────────────────────────────────────────────────────────
  let reader = null;          // NxsReader (binary mode, after stream finishes)
  let streamReader = null;    // NxsStreamReader while bytes are still arriving
  /** Evicted row marker in `recordOffsets` (buffer-relative offsets otherwise). */
  const OFFSET_EVICTED = 0xffffffff;
  let recordOffsets = null;   // per-record buffer-relative NYXO offsets during streaming (Uint32Array)
  let recordFileOffsets = null; // per-record absolute file offsets for local-cache streaming
  let streamCacheBytes = null;  // SparseBytes over IndexedDB chunks while streaming
  let streamCacheAccessor = null; // minimal reader shape for NxsObject over streamCacheBytes
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
  let localNxbCache = null;   // IndexedDB-backed bytes for large streamed files
  
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
  let workerSourceKey = "";
  /** Query applied to worker results (neon-dash: table browsable until this matches). */
  let appliedSearchQuery = "";
  let searchScanning = false;
  let searchScanMode = "";
  let workerSearchReady = false;

  function teardownExplorerWorker() {
    if (worker) {
      worker.removeEventListener("message", onWorkerMessage);
      worker.terminate();
      worker = null;
    }
    workerSourceKey = "";
    workerSearchReady = false;
  }

  function explorerSearchColumnPayload() {
    if (!searchColumn) return null;
    return {
      key: searchColumn.key,
      slot: searchColumn.slot,
      sigil: searchColumn.sigil,
    };
  }

  function ensureExplorerSearchWorker(source) {
    const key = `mem:${source.buffer.byteLength}:${source.recordIndexes.length}`;
    if (worker && workerSourceKey === key && workerSearchReady) return worker;

    teardownExplorerWorker();
    ensureWorker();
    if (!worker) return null;

    workerSourceKey = key;
    workerSearchReady = false;
    const searchColumnPayload = explorerSearchColumnPayload();
    // Clone for transfer — never detach the main-thread reader's backing buffer.
    const copy = source.buffer.slice(0);
    worker.postMessage(
      {
        type: "load",
        buffer: copy,
        searchColumn: searchColumnPayload,
      },
      [copy],
    );
    return worker;
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
    if (resetScroll && !streamReader) {
      scrollEl.scrollTop = 0;
    }
  }

  /** Pin scroll to the newest in-memory records after layout (avoids clientHeight=0). */
  function scrollToResidentTail() {
    invalidateResidentBoundsCache();
    const run = () => {
      if (!streamReader || recordCount <= 0) return;
      const { last } = residentRecordBounds();
      scrollEl.scrollTop = recordIdxToScrollTop(last);
      lastScrollTop = -1;
      lastRenderCompactGen = streamReader.compactGeneration ?? -1;
      updateStatus(scrollEl.scrollTop);
      scheduleRender();
    };
    requestAnimationFrame(() => requestAnimationFrame(run));
  }
  
  /** Large NXB: keep stream state; do not reset scroll/layout like a fresh file open. */
  function endLargeStreamView(name, sizeBytes) {
    streamExpectedBytes = 0;
    activeFormat = "NXB (tail window)";
    const tag = `${escapeHtml(name)} <span style="color:var(--muted)">(tail window)</span>`;
    fileInfoEl.innerHTML =
      `${tag} — ${fmtBytes(sizeBytes)} — ${fmtInt(recordCount)} records · scroll up for older in-memory rows`;
    rowsStreamedPeak = recordCount;
    overlayEl.classList.add("hide");
    updateTelemetry();
    updateViewportMetrics(false);
    lastFirstVr = -1;
    lastScrollTop = -1;
    lastRenderCompactGen = -1;
    ensureRowPool();
    scrollToResidentTail();
  }

  function applyViewportLayout(name, sizeBytes, sourceLabel) {
    updateViewportMetrics(true);
    matchesSet.clear();
    currentMatches = null;
    currentMatchIdx = -1;
    lastFirstVr = -1;
    lastScrollTop = -1;
    searchBadge.textContent = "";
    searchBadge.className = "badge";
    updateMatchesStatus();
  
    const tag = sourceLabel ? `${escapeHtml(name)} <span style="color:var(--muted)">(${sourceLabel})</span>` : escapeHtml(name);
    fileInfoEl.innerHTML = `<strong>${tag}</strong> — ${fmtBytes(sizeBytes)} — ${fmtInt(recordCount)} records`;
    rowsStreamedPeak = recordCount;
    activeFormat = sourceLabel || (jsonRecords !== null ? "JSON" : "NXB");
    updateTelemetry();
    overlayEl.classList.add("hide");

    ensureRowPool();
    if (streamReader && recordCount > 0) {
      scrollToResidentTail();
    } else {
      updateStatus(0);
      scheduleRender();
    }
  }
  
  function applyStreamingProgress(name, receivedBytes, totalBytes) {
    streamExpectedBytes = totalBytes || receivedBytes;
    if (recordCount > 0) {
      invalidateResidentBoundsCache();
      updateViewportMetrics(false);
      const maxScroll = maxScrollTop();
      const { last } = residentRecordBounds();
      const nearBottom = scrollEl.scrollTop >= maxScroll - ROW_HEIGHT * 3;
      if (!streamCacheAccessor && (nearBottom || recordCount <= Math.ceil(scrollEl.clientHeight / ROW_HEIGHT))) {
        scrollEl.scrollTop = recordIdxToScrollTop(last);
        lastScrollTop = -1;
      }
      if (virtualHeight > 0) overlayEl.classList.add("hide");
      ensureRowPool();
    }
    const total = totalBytes > 0 ? ` — ${fmtBytes(receivedBytes)} / ${fmtBytes(totalBytes)}` : receivedBytes > 0 ? ` — ${fmtBytes(receivedBytes)} received` : "";
    fileInfoEl.innerHTML =
      `<strong>${escapeHtml(name)}</strong> <span style="color:var(--warn)">(streaming)</span>${total} — ${fmtInt(recordCount)} records parsed`;
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
    lastScrollTop = -1;
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
  
  /** Read one cell using schema sigils — `get()` mis-decodes STR as i64. */
  function readNxbCell(accessor, col) {
    if (col.nestedIn) {
      const parent = accessor.get(col.nestedIn);
      return isNxsObject(parent) ? readNxbCell(parent, { ...col, nestedIn: undefined }) : undefined;
    }
    const slot = col.slot;
    if (slot === undefined) return accessor.get(col.key);
    const sig = col.sigil;
    if (sig === WIRE_SIGILS.str) return accessor.getStrBySlot(slot);
    if (sig === WIRE_SIGILS.float) return accessor.getF64BySlot(slot);
    if (sig === WIRE_SIGILS.bool) return accessor.getBoolBySlot(slot);
    if (sig === WIRE_SIGILS.int || sig === WIRE_SIGILS.time) return accessor.getI64BySlot(slot);
    if (sig === WIRE_SIGILS.binary) {
      const bin = accessor.getBinaryBySlot(slot);
      return bin ? `<binary ${bin.byteLength} B>` : undefined;
    }
    return accessor.get(col.key);
  }

  function formatNxbValue(accessor, col) {
    return formatAnyValue(readNxbCell(accessor, col));
  }
  
  function nxbCellClass(v) {
    if (typeof v === "boolean") return v ? "col-cell bool on" : "col-cell bool";
    return "col-cell";
  }
  
  // ── Virtual scroller ─────────────────────────────────────────────────────
  //
  // Strategy: spacer height tracks *parsed* recordCount (grows while streaming).
  // Scroll only covers records received so far; no HTTP range / fake 10M height
  // before bytes arrive. When parsed rows exceed MAX_VIRTUAL_PX, scrollScale>1
  // keeps consecutive rows in the viewport. Pool rows reuse DOM nodes.
  
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
  
  function maxScrollTop() {
    return Math.max(0, virtualHeight - scrollEl.clientHeight);
  }

  function useRecordWindowScroll() {
    return scrollScale > 1.001 || (streamReader != null && recordCount > 0 && !streamCacheAccessor);
  }

  let residentBoundsCache = null;

  function invalidateResidentBoundsCache() {
    residentBoundsCache = null;
  }

  function onStreamBufferCompact(cut) {
    if (!recordOffsets) return;
    for (let i = 0; i < recordCount; i++) {
      const o = recordOffsets[i];
      if (o === OFFSET_EVICTED) continue;
      if (o < cut) recordOffsets[i] = OFFSET_EVICTED;
      else recordOffsets[i] = o - cut;
    }
    invalidateResidentBoundsCache();
  }

  function ensureStreamCacheAccessor(totalBytes) {
    if (!localNxbCache || streamCacheAccessor) return;
    const fetchRange = (byteStart, byteLength) => localNxbCache.readRange(byteStart, byteLength);
    streamCacheBytes = new SparseBytes(totalBytes || streamExpectedBytes || Number.MAX_SAFE_INTEGER, [], fetchRange);
    streamCacheAccessor = {
      bytes: streamCacheBytes.asIndexed(),
      keys: streamReader.keys,
      keySigils: streamReader.keySigils,
      keyIndex: streamReader.keyIndex,
      _layout: "row",
    };
  }

  function rdU32At(bytes, off) {
    return (
      bytes[off] |
      (bytes[off + 1] << 8) |
      (bytes[off + 2] << 16) |
      (bytes[off + 3] << 24)
    ) >>> 0;
  }

  /** Absolute file offset for record idx, or null if not yet parsed / unset. */
  function recordFileOffsetFor(idx) {
    if (!recordFileOffsets || idx < 0 || idx >= recordCount) return null;
    const off = recordFileOffsets[idx];
    if (off === 0 && idx !== 0) return null;
    return off;
  }

  function objectMagicOk(bytes, off) {
    try {
      return rdU32At(bytes, off) === NYXO_MAGIC;
    } catch {
      return false;
    }
  }

  function streamCacheRecordReady(idx) {
    if (!streamCacheAccessor) return false;
    const off = recordFileOffsetFor(idx);
    if (off == null) return false;
    return objectMagicOk(streamCacheAccessor.bytes, off);
  }

  async function prefetchStreamCacheRows(firstIdx, lastIdx) {
    if (!streamCacheBytes || !localNxbCache || !recordFileOffsets) return;
    const first = Math.max(0, Math.min(firstIdx, lastIdx));
    const last = Math.min(recordCount - 1, Math.max(firstIdx, lastIdx));
    for (let idx = first; idx <= last; idx++) {
      if (streamBufferOffset(idx) !== null) continue;
      const off = recordFileOffsetFor(idx);
      if (off == null) continue;
      try {
        const header = await localNxbCache.readRange(off, 8);
        const len = header[4] | (header[5] << 8) | (header[6] << 16) | (header[7] << 24);
        if (len < 8) continue;
        streamCacheBytes.fillRange(off, await localNxbCache.readRange(off, len));
      } catch {
        // The row may be parsed before its chunk write transaction is visible; try again next frame.
      }
    }
  }

  function streamBufferOffset(idx) {
    if (!streamReader || !recordOffsets || idx < 0 || idx >= recordCount) return null;
    const o = recordOffsets[idx];
    if (o === OFFSET_EVICTED) return null;
    const bytes = streamReader.bytes;
    if (o + 8 > bytes.length) return null;
    if (!objectMagicOk(bytes, o)) return null;
    return o;
  }

  /** First record index whose bytes are still in the stream reader window. */
  function firstResidentRecordIndex() {
    if (!streamReader || !recordOffsets || recordCount <= 0) return 0;
    if (recordOffsets[0] !== OFFSET_EVICTED) return 0;
    let lo = 0;
    let hi = recordCount - 1;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (recordOffsets[mid] === OFFSET_EVICTED) lo = mid + 1;
      else hi = mid;
    }
    return recordOffsets[lo] === OFFSET_EVICTED ? Math.max(0, recordCount - 1) : lo;
  }

  /** Last record index whose NYXO bytes are still in the stream window. */
  function lastResidentRecordIndex() {
    if (!streamReader || !recordOffsets || recordCount <= 0) return Math.max(0, recordCount - 1);
    if (recordOffsets[recordCount - 1] !== OFFSET_EVICTED) return recordCount - 1;
    let lo = firstResidentRecordIndex();
    let hi = recordCount - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (recordOffsets[mid] !== OFFSET_EVICTED) lo = mid;
      else hi = mid - 1;
    }
    return lo;
  }

  function residentRecordBounds() {
    if (streamCacheAccessor && recordCount > 0) {
      return { first: 0, last: recordCount - 1 };
    }
    if (!streamReader || !recordOffsets || recordCount <= 0) {
      return { first: 0, last: Math.max(0, recordCount - 1) };
    }
    const gen = streamReader.compactGeneration;
    const len = streamReader.bytes.length;
    const base = streamReader.earliestRetainedOffset;
    if (
      residentBoundsCache &&
      residentBoundsCache.gen === gen &&
      residentBoundsCache.base === base &&
      residentBoundsCache.len === len &&
      residentBoundsCache.count === recordCount
    ) {
      return residentBoundsCache;
    }
    const first = firstResidentRecordIndex();
    const last = lastResidentRecordIndex();
    residentBoundsCache = { gen, base, len, count: recordCount, first, last };
    return residentBoundsCache;
  }

  /** First record aligned with the top visible row when scroll height is capped (10M+). */
  function firstRecordForScroll(scrollTop) {
    const maxScroll = maxScrollTop();
    const visibleRows = Math.max(1, Math.ceil(scrollEl.clientHeight / ROW_HEIGHT));
    const { first: residentFirst, last } = residentRecordBounds();
    const maxFirst = Math.max(residentFirst, last - visibleRows + 1);
    if (maxScroll <= 0 || recordCount <= 0) return residentFirst;
    if (maxFirst <= residentFirst) return residentFirst;
    const ratio = Math.min(1, Math.max(0, scrollTop / maxScroll));
    return Math.min(maxFirst, residentFirst + Math.floor(ratio * (maxFirst - residentFirst)));
  }

  /** Record index for pool slot i (consecutive window when scroll is compressed). */
  function recordIndexForPoolSlot(scrollTop, poolOffset) {
    const { first, last } = residentRecordBounds();
    if (!useRecordWindowScroll()) {
      const vr = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - BUFFER_ROWS) + poolOffset;
      return Math.min(last, first + Math.floor(vr * scrollScale));
    }
    const rowFirst = firstRecordForScroll(scrollTop);
    return Math.min(last, Math.max(first, rowFirst + (poolOffset - BUFFER_ROWS)));
  }

  /** Spacer Y for pool slot i — must match record mapping when scroll is compressed. */
  function virtualTopForPoolSlot(scrollTop, poolOffset) {
    if (!useRecordWindowScroll()) {
      const firstVr = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - BUFFER_ROWS);
      return (firstVr + poolOffset) * ROW_HEIGHT;
    }
    return scrollTop + (poolOffset - BUFFER_ROWS) * ROW_HEIGHT;
  }

  // Inverse: record index -> scroll position.
  function recordIdxToScrollTop(idx) {
    const { first: residentFirst, last } = residentRecordBounds();
    const clamped = Math.min(last, Math.max(residentFirst, idx));
    if (!useRecordWindowScroll()) {
      return Math.floor((clamped - residentFirst) / scrollScale) * ROW_HEIGHT;
    }
    const maxScroll = maxScrollTop();
    const visibleRows = Math.max(1, Math.ceil(scrollEl.clientHeight / ROW_HEIGHT));
    const maxFirst = Math.max(residentFirst, last - visibleRows + 1);
    if (maxScroll <= 0) return 0;
    if (maxFirst <= residentFirst) return maxScroll;
    return Math.floor(((clamped - residentFirst) / (maxFirst - residentFirst)) * maxScroll);
  }
  
  // Track the currently-rendered window (fast-path skip).
  let lastFirstVr = -1;
  let lastScrollTop = -1;
  let lastWindowSize = 0;
  let lastRenderCompactGen = -1;
  let frameAvgMs = 0;
  let lastOpenMs = null;
  let lastFilterMs = null;
  let rowsStreamedPeak = 0;
  let activeFormat = "—";
  let lastFixtureBase = null; // e.g. records_1000000 for comparison
  let streamExpectedBytes = 0; // Content-Length hint while streaming (0 = unknown)
  const matchesSet = new Set();  // O(1) lookup of record idx -> is-a-match

  function heapMb() {
    const m = performance.memory;
    return m ? m.usedJSHeapSize / 1048576 : null;
  }

  function updateTelemetry() {
    if (telOpen) telOpen.textContent = lastOpenMs != null ? `${lastOpenMs.toFixed(1)} ms` : "—";
    if (telFilter) telFilter.textContent = lastFilterMs != null ? `${lastFilterMs.toFixed(1)} ms` : "—";
    if (telMemory) {
      const mb = heapMb();
      telMemory.textContent = mb != null ? `${mb.toFixed(0)} MB` : "n/a";
    }
    if (telStreamed) telStreamed.textContent = rowsStreamedPeak > 0 ? fmtInt(rowsStreamedPeak) : "—";
    if (telLoaded) telLoaded.textContent = recordCount > 0 ? fmtInt(recordCount) : "—";
    if (telFormat) telFormat.textContent = activeFormat;
    if (compareBar && lastFixtureBase) compareBar.hidden = false;
  }

  function fixtureBaseFromPath(path) {
    const m = (path || "").match(/records_(\d+)(?:_columnar)?\.nxb/i);
    return m ? `records_${m[1]}` : null;
  }
  
  let rafPending = false;
  let renderInFlight = false;
  function scheduleRender() {
    if (rafPending) return;
    rafPending = true;
    requestAnimationFrame(() => { void render(); });
  }

  async function render() {
    rafPending = false;
    if (renderInFlight) return;
    if (!reader && !streamReader && jsonRecords === null) return;
    if (columns.length === 0) return;
    const renderGen = loadGeneration;
    renderInFlight = true;
    try {
      ensureRowPool();
      const t0 = performance.now();
      const scrollTop = scrollEl.scrollTop;
      const firstVr = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - BUFFER_ROWS);
      const poolSize = rowPool.length; // after ensureRowPool
      const maxVr = totalRowsVirtual - 1;
      const lastVr = Math.min(maxVr, firstVr + poolSize - 1);
      const activeRows = firstVr > maxVr ? 0 : lastVr - firstVr + 1;
      const windowScroll = useRecordWindowScroll();
      const compactGen = streamReader?.compactGeneration ?? 0;
      const compactChanged = compactGen !== lastRenderCompactGen;

      // Fast path: nothing changed.
      if (windowScroll) {
        if (!compactChanged && scrollTop === lastScrollTop && lastWindowSize === activeRows) {
          return;
        }
        lastScrollTop = scrollTop;
        lastRenderCompactGen = compactGen;
      } else if (!compactChanged && firstVr === lastFirstVr && lastWindowSize === activeRows) {
        return;
      }
      if (compactChanged) lastRenderCompactGen = compactGen;
      lastFirstVr = firstVr;
      lastWindowSize = activeRows;

      if (reader?._sparse && recordCount > 0 && activeRows > 0) {
        const firstIdx = windowScroll
          ? recordIndexForPoolSlot(scrollTop, 0)
          : Math.min(recordCount - 1, Math.floor(firstVr * scrollScale));
        const lastIdx = windowScroll
          ? recordIndexForPoolSlot(scrollTop, Math.max(0, poolSize - 1))
          : Math.min(recordCount - 1, Math.floor((firstVr + Math.max(0, poolSize - 1)) * scrollScale));
        await reader.prefetch_viewport(
          Math.max(0, Math.min(firstIdx, lastIdx)),
          Math.max(0, Math.max(firstIdx, lastIdx)),
        );
      }

      if (streamCacheAccessor && recordCount > 0 && activeRows > 0) {
        const firstIdx = windowScroll
          ? recordIndexForPoolSlot(scrollTop, 0)
          : Math.min(recordCount - 1, Math.floor(firstVr * scrollScale));
        const lastIdx = windowScroll
          ? recordIndexForPoolSlot(scrollTop, Math.max(0, poolSize - 1))
          : Math.min(recordCount - 1, Math.floor((firstVr + Math.max(0, poolSize - 1)) * scrollScale));
        await prefetchStreamCacheRows(firstIdx, lastIdx);
      }

      for (let i = 0; i < poolSize; i++) {
        if (renderGen !== loadGeneration) return;
        const row = rowPool[i];
        if (!row?.el) continue;
        const virtualTop = virtualTopForPoolSlot(scrollTop, i);
        if (!windowScroll) {
          const vr = firstVr + i;
          if (vr < 0 || vr > maxVr) {
            if (row.renderedIndex !== -1) {
              row.el.style.display = "none";
              row.renderedIndex = -1;
            }
            continue;
          }
        } else {
          const viewEnd = scrollTop + scrollEl.clientHeight;
          if (virtualTop + ROW_HEIGHT <= scrollTop || virtualTop >= viewEnd) {
            row.el.style.display = "none";
            row.renderedIndex = -1;
            continue;
          }
        }
        if (recordCount <= 0) {
          row.el.style.display = "none";
          row.renderedIndex = -1;
          continue;
        }
        const idx = windowScroll
          ? recordIndexForPoolSlot(scrollTop, i)
          : Math.min(recordCount - 1, Math.floor((firstVr + i) * scrollScale));
        if (streamReader && idx >= recordCount) {
          row.el.style.display = "";
          row.renderedIndex = -1;
          row.el.style.transform = `translateY(${virtualTop}px)`;
          for (let c = 0; c < row.cells.length; c++) {
            row.cells[c].textContent = "";
            row.cells[c].className = "col-cell";
          }
          continue;
        }
        row.el.style.display = "";
        row.renderedIndex = idx;
        row.el.style.transform = `translateY(${virtualTop}px)`;
        renderRow(row, idx);
      }

      const elapsed = performance.now() - t0;
      frameAvgMs = frameAvgMs * 0.8 + elapsed * 0.2;
      updateStatus(scrollTop);
    } finally {
      renderInFlight = false;
    }
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
        const rel = streamBufferOffset(idx);
        if (rel !== null) {
          accessor = new NxsObject(streamReader, rel);
        } else if (streamCacheRecordReady(idx)) {
          accessor = new NxsObject(streamCacheAccessor, recordFileOffsetFor(idx));
        } else {
          for (let c = 0; c < ncols; c++) {
            row.cells[c].textContent = "";
            row.cells[c].className = "col-cell";
          }
          return;
        }
      } else {
        cursor.seek(idx);
        accessor = cursor;
      }
      try {
        for (let c = 0; c < ncols; c++) {
          const col = columns[c];
          const cell = row.cells[c];
          const raw = readNxbCell(accessor, col);
          cell.textContent = formatAnyValue(raw);
          cell.className = nxbCellClass(raw);
        }
      } catch (e) {
        const msg = String(e?.message || e);
        if (
          msg.includes("not resident") ||
          msg.includes("ERR_BAD_MAGIC") ||
          msg.includes("ERR_INCOMPLETE")
        ) {
          for (let c = 0; c < ncols; c++) {
            row.cells[c].textContent = "";
            row.cells[c].className = "col-cell";
          }
          return;
        }
        throw e;
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
    const viewportRows = Math.ceil(scrollEl.clientHeight / ROW_HEIGHT);
    let centerIdx = 0;
    if (recordCount > 0) {
      if (useRecordWindowScroll()) {
        const first = firstRecordForScroll(scrollTop);
        centerIdx = Math.min(
          recordCount - 1,
          first + Math.floor(viewportRows / 2),
        );
      } else {
        const topVr = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT));
        const centerVr =
          totalRowsVirtual <= 0 ? 0 : Math.min(totalRowsVirtual - 1, topVr + (viewportRows >> 1));
        centerIdx = Math.min(recordCount - 1, Math.floor(centerVr * scrollScale));
      }
    }
    const totalLabel = streamReader
      ? `<strong>${fmtInt(recordCount)}</strong> parsed`
      : `<strong>${fmtInt(recordCount)}</strong>`;
    statusPos.innerHTML = `line <strong>${fmtInt(centerIdx + 1)}</strong> of ${totalLabel}`;
    statusFrame.textContent = `render ${frameAvgMs.toFixed(1)} ms`;
    statusFrame.className = frameAvgMs < 4 ? "ok" : frameAvgMs < 10 ? "warn" : "bad";
  }
  
  // ── Loading ──────────────────────────────────────────────────────────────
  function attachWorkerForNxb() {
    if (streamReader) return;
    ensureWorker();
    if (!worker || !searchColumn) return;

    const searchColumnPayload = explorerSearchColumnPayload();
    const source = reader ? explorerNxbSearchSource(reader, recordCount) : null;
    if (source) {
      // Worker loads on first search (cloned buffer). Do not transfer here — render needs the live buffer.
      workerSourceKey = `mem:${source.buffer.byteLength}:${recordCount}`;
      return;
    }

    if (reader?._sparse && localNxbCache?.dbName) {
      workerSourceKey = `cache:${localNxbCache.dbName}:${reader.bytes.length}`;
      worker.postMessage({
        type: "load-cache",
        dbName: localNxbCache.dbName,
        fileSize: reader.bytes.length,
        searchColumn: searchColumnPayload,
      });
      return;
    }

    if (_lastLoadedUrl && !pathLooksLikeNxs(_lastLoadedUrl)) {
      workerSourceKey = `url:${_lastLoadedUrl}`;
      worker.postMessage({ type: "load-url", url: _lastLoadedUrl, searchColumn: searchColumnPayload });
      return;
    }

    if (rawBuffer) {
      const copy = rawBuffer.slice(0);
      workerSourceKey = `buf:${copy.byteLength}`;
      worker.postMessage({ type: "load", buffer: copy, searchColumn: searchColumnPayload }, [copy]);
    }
  }
  
  async function loadFromReadableStream(body, name, sizeBytes) {
    const tOpen = performance.now();
    const gen = ++loadGeneration;
    await closeLocalNxbCache();
    jsonRecords = null;
    reader = null;
    cursor = null;
    streamReader = null;
    recordOffsets = null;
    recordFileOffsets = null;
    streamCacheBytes = null;
    streamCacheAccessor = null;
    rawBuffer = null;
    recordCount = 0;
    virtualHeight = 0;
    rowsStreamedPeak = 0;
    streamExpectedBytes = sizeBytes || 0;
    lastOpenMs = null;
    clearColumns();
    teardownExplorerWorker();
  
    overlayEl.classList.remove("hide");
    overlayEl.textContent = `Loading ${name}…`;
  
    const willSealAfterDownload =
      (sizeBytes > 0 && sizeBytes <= STREAM_SEAL_BYTES) ||
      (sizeBytes === 0 && fixtureSizeHint(name, 0) > 0 && fixtureSizeHint(name, 0) <= STREAM_SEAL_BYTES);
    const shouldCacheLocally = !willSealAfterDownload;
    const writeCache = shouldCacheLocally ? await openLocalNxbCache(name) : null;
    localNxbCache = writeCache;

    let sr;
    try {
      sr = new NxsStreamReader({
        compactionEnabled: !willSealAfterDownload,
        onCompact: onStreamBufferCompact,
        onSchema(keys, keySigils) {
          if (gen !== loadGeneration) return;
          applyColumns(buildColumnsFromSchema(keys, keySigils));
          if (shouldCacheLocally) ensureStreamCacheAccessor(sizeBytes);
        },
        onRecord(obj, idx) {
          if (gen !== loadGeneration) return;
          if (!recordOffsets) recordOffsets = new Uint32Array(4096);
          if (idx >= recordOffsets.length) {
            const grown = new Uint32Array(recordOffsets.length * 2);
            grown.set(recordOffsets);
            recordOffsets = grown;
          }
          if (shouldCacheLocally && !recordFileOffsets) recordFileOffsets = new Uint32Array(4096);
          if (recordFileOffsets && idx >= recordFileOffsets.length) {
            const grown = new Uint32Array(recordFileOffsets.length * 2);
            grown.set(recordFileOffsets);
            recordFileOffsets = grown;
          }
          if (recordFileOffsets) recordFileOffsets[idx] = sr.fileOffsetOf(obj.offset);
          recordOffsets[idx] = obj.offset;
          recordCount = idx + 1;
          invalidateResidentBoundsCache();
          rowsStreamedPeak = Math.max(rowsStreamedPeak, recordCount);
          if (idx === 0) lastOpenMs = performance.now() - tOpen;
          if (idx === 0 || (idx & 0xffff) === 0) {
            applyStreamingProgress(name, 0, sizeBytes);
            updateTelemetry();
          } else if ((idx & 0x3ff) === 0) {
            scheduleRender();
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
        if (value && value.byteLength > 0) {
          const chunkStart = received;
          received += value.byteLength;
          if (writeCache) await writeCache.write(chunkStart, value);
          if (!willSealAfterDownload && received > STREAM_SEAL_BYTES) {
            sr.compactionEnabled = true;
          }
          sr.push(value);
        }
        if (done) break;
        if ((received & 0x3fffff) < value.byteLength || recordCount === 1) {
          overlayEl.textContent =
            `Loading ${name}… ${fmtBytes(received)}${sizeBytes ? ` / ${fmtBytes(sizeBytes)}` : ""} — ${fmtInt(recordCount)} records`;
          if (recordCount > 0 && ((recordCount & 0xffff) === 0 || recordCount === 1)) {
            applyStreamingProgress(name, received, sizeBytes);
          }
        }
      }
      if (gen !== loadGeneration) return;

      const totalBytes = sizeBytes || received;
      if (received <= STREAM_SEAL_BYTES) {
        reader = sr.finish();
        streamReader = null;
        recordOffsets = null;
        recordFileOffsets = null;
        streamCacheBytes = null;
        streamCacheAccessor = null;
        cursor = reader.cursor();
        applyColumns(buildColumnsFromReader(reader));
        bindRawBufferFromReader();
        attachWorkerForNxb();
      } else {
        // Large file: keep stream buffer + offset table (no HTTP range, no second fetch).
        if (writeCache) {
          await writeCache.finalize();
          let materialized = false;
          if (totalBytes > 0 && totalBytes <= SEARCH_MATERIALIZE_MAX_BYTES) {
            try {
              overlayEl.textContent = `Materializing ${name} for fast search…`;
              const bytes = await writeCache.materialize(totalBytes);
              if (gen !== loadGeneration) return;
              reader = new NxsReader(bytes);
              cursor = reader.cursor();
              bindRawBufferFromReader();
              materialized = true;
              activeFormat = "NXB (memory)";
            } catch (e) {
              console.warn("Explorer: in-memory search materialize failed, using sparse cache", e);
            }
          }
          if (!materialized) {
            reader = await openCachedNxbReader(writeCache, totalBytes);
            cursor = reader.cursor();
            rawBuffer = null;
          }
          streamReader = null;
          recordOffsets = null;
          recordFileOffsets = null;
          streamCacheBytes = null;
          streamCacheAccessor = null;
          applyColumns(buildColumnsFromSchema(reader.keys, reader.keySigils));
          attachWorkerForNxb();
        } else {
          reader = null;
          cursor = null;
        }
        rawBuffer = null;
      }
      if (lastOpenMs == null) lastOpenMs = performance.now() - tOpen;
      if (received <= STREAM_SEAL_BYTES) {
        activeFormat = "NXB (streamed)";
        applyViewportLayout(name, totalBytes, null);
      } else if (reader) {
        activeFormat = "NXB (local cache)";
        applyViewportLayout(name, totalBytes, "local cache");
      } else {
        sr.endOfStream();
        invalidateResidentBoundsCache();
        endLargeStreamView(name, totalBytes);
      }
    } catch (e) {
      if (gen !== loadGeneration) return;
      showError(`Failed to load ${name}: ${e.message}`);
    }
  }

  async function loadNxbFromUrl(path, name, sizeBytes) {
    jsonRecords = null;
    streamReader = null;
    recordOffsets = null;
    recordFileOffsets = null;
    streamCacheBytes = null;
    streamCacheAccessor = null;
    clearColumns();
    teardownExplorerWorker();
    overlayEl.classList.remove("hide");
    overlayEl.textContent = `Loading ${name}…`;
    try {
      const res = await fetch(path);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const len =
        sizeBytes ||
        parseInt(res.headers.get("content-length") || "0", 10) ||
        0;
      if (!res.body) throw new Error("Streaming body not available");
      await loadFromReadableStream(res.body, name, len);
    } catch (e) {
      showError(`Failed to load ${name}: ${e.message}`);
    }
  }

  async function loadBuffer(buffer, name, sizeBytes, sourceLabel = null) {
    const tOpen = performance.now();
    const gen = ++loadGeneration;
    await closeLocalNxbCache();
    jsonRecords = null;
    streamReader = null;
    recordOffsets = null;
    recordFileOffsets = null;
    streamCacheBytes = null;
    streamCacheAccessor = null;
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
    lastOpenMs = performance.now() - tOpen;
    activeFormat = sourceLabel || "NXB";
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
    const tParse = performance.now();
    loadGeneration++;
    await closeLocalNxbCache();
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
    recordFileOffsets = null;
    streamCacheBytes = null;
    streamCacheAccessor = null;
    cursor = null;
    rawBuffer = null;
    virtualHeight = 0;
    jsonRecords = prepared.records;
    recordCount = jsonRecords.length;
    _lastLoadedUrl = null;
    applyColumns(prepared.columns);
    lastOpenMs = performance.now() - tParse;
    activeFormat = "JSON (full parse)";
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
      workerSearchReady = true;
      searchScanMode = msg.searchMode || "";
    } else if (msg.type === "load-progress") {
      if (!searchEl.value.trim()) {
        statusMatches.textContent = `indexing ${fmtInt(msg.parsed)} records for search…`;
      }
    } else if (msg.type === "load-error") {
      console.warn("Explorer worker load failed:", msg.message);
    } else if (msg.type === "search-progress") {
      if (msg.token !== searchToken) return;
      searchScanning = true;
      if (msg.searchMode) searchScanMode = msg.searchMode;
      const pct = ((msg.scanned / msg.total) * 100).toFixed(0);
      const mode = searchScanMode ? ` · ${searchScanMode}` : "";
      searchBadge.textContent = `scanning ${pct}%${mode} · ${fmtInt(msg.matches)} so far`;
      searchBadge.className = "badge searching";
      if (!searchEl.value.trim()) {
        statusMatches.textContent = "No search active";
      } else {
        statusMatches.textContent = "Scanning… table still browsable";
      }
    } else if (msg.type === "search-done") {
      if (msg.token !== searchToken) return;
      searchScanning = false;
      if (msg.aborted) return;
      const query = searchEl.value.trim().toLowerCase();
      if (!query || query !== appliedSearchQuery) return;
      if (msg.elapsedMs != null) {
        lastFilterMs = msg.elapsedMs;
        updateTelemetry();
      }
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
    appliedSearchQuery = query.trim().toLowerCase();
    if (!query) {
      searchScanning = false;
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
    searchScanning = true;
    searchBadge.textContent = "scanning…";
    searchBadge.className = "badge searching";
    statusMatches.textContent = streamReader
      ? "Scanning parsed rows… (full-file search after load)"
      : "Scanning… table still browsable";
  
    // Defer to a microtask so the UI paints the "scanning" badge first.
    Promise.resolve().then(() => {
      if (token !== searchToken) return;
      const t0 = performance.now();
      const needle = appliedSearchQuery;
      const results = [];
      if (!searchColumn) return;
      if (jsonRecords !== null) {
        const key = searchColumn.key;
        for (let i = 0; i < recordCount; i++) {
          const u = jsonRecords[i][key];
          if (u != null && String(u).toLowerCase().indexOf(needle) !== -1) results.push(i);
        }
      } else if (reader && !streamReader) {
        const spec = buildExplorerSearchSpec(reader, searchColumn);
        if (spec) {
          const { matches } = scanExplorerNxbRecords(reader, spec, null, needle, {
            token,
            getActiveToken: () => searchToken,
          });
          for (let i = 0; i < matches.length; i++) results.push(matches[i]);
        }
      } else if (streamReader && recordOffsets) {
        const { first, last } = residentRecordBounds();
        for (let i = first; i <= last; i++) {
          const rel = streamBufferOffset(i);
          if (rel === null) continue;
          let accessor;
          try {
            accessor = new NxsObject(streamReader, rel);
          } catch {
            continue;
          }
          try {
            const u = readNxbCell(accessor, searchColumn);
            if (u != null && String(u).toLowerCase().indexOf(needle) !== -1) results.push(i);
          } catch (_) { /* evicted mid-scan */ }
        }
      }
      if (token !== searchToken) return;
      searchScanning = false;
      searchScanMode = reader?._sparse ? "sparse" : "indexed";
      lastFilterMs = performance.now() - t0;
      updateTelemetry();
      currentMatches = new Int32Array(results);
      matchesSet.clear();
      for (const i of results) matchesSet.add(i);
      currentMatchIdx = results.length ? 0 : -1;
      searchBadge.textContent = results.length ? `${fmtInt(results.length)} matches` : "no matches";
      searchBadge.className = results.length ? "badge active" : "badge";
      if (results.length) jumpToRecord(results[0]);
      updateMatchesStatus();
      lastFirstVr = -1;
      lastScrollTop = -1;
      scheduleRender();
    });
  }
  
  function startSearch(query) {
    query = query.trim();
    appliedSearchQuery = query.toLowerCase();
    // Clear highlights while scanning; keep all rows visible (neon-dash pattern).
    matchesSet.clear();
    currentMatches = null;
    currentMatchIdx = -1;
    lastFirstVr = -1;
    scheduleRender();
  
    if (!query) {
      searchToken++;  // cancel any in-flight
      searchScanning = false;
      searchBadge.textContent = "";
      searchBadge.className = "badge";
      updateMatchesStatus();
      return;
    }
  
    if (jsonRecords !== null) {
      searchMainThread(query);
      return;
    }

    if (streamReader) {
      searchMainThread(query);
      return;
    }

    // In-memory NXB: search on main thread (field-index path) — avoids cloning ~GB for the worker.
    if (reader && !reader._sparse) {
      searchMainThread(query);
      return;
    }

    if (worker && reader) {
      const source = explorerNxbSearchSource(reader, recordCount);
      if (source) ensureExplorerSearchWorker(source);
      searchToken++;
      searchScanning = true;
      searchBadge.textContent = "scanning…";
      searchBadge.className = "badge searching";
      statusMatches.textContent = "Scanning… table still browsable";
      worker.postMessage({ type: "search", query, token: searchToken });
      return;
    }

    if (worker) {
      searchToken++;
      searchScanning = true;
      searchBadge.textContent = "scanning…";
      searchBadge.className = "badge searching";
      statusMatches.textContent = "Scanning… table still browsable";
      worker.postMessage({ type: "search", query, token: searchToken });
      return;
    }

    searchMainThread(query);
  }
  
  function updateMatchesStatus() {
    if (searchScanning && searchEl.value.trim()) {
      statusMatches.textContent = "Scanning… table still browsable";
      return;
    }
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
    lastScrollTop = -1;
    scheduleRender();
  }

  // ── Navigation ───────────────────────────────────────────────────────────
  function jumpToRecord(idx) {
    const clamped = Math.max(0, Math.min(recordCount - 1, idx));
    const viewportRows = Math.ceil(scrollEl.clientHeight / ROW_HEIGHT);
    if (useRecordWindowScroll()) {
      const visibleRows = Math.max(1, viewportRows);
      const maxFirst = Math.max(0, recordCount - visibleRows);
      const wantFirst = Math.min(maxFirst, Math.max(0, clamped - (viewportRows >> 1)));
      scrollEl.scrollTop = recordIdxToScrollTop(wantFirst);
    } else {
      const target = recordIdxToScrollTop(clamped) - (viewportRows >> 1) * ROW_HEIGHT;
      scrollEl.scrollTop = Math.max(0, target);
    }
    lastScrollTop = -1;
    lastFirstVr = -1;
    scheduleRender();
  }
  
  // ── Event wiring ─────────────────────────────────────────────────────────
  scrollEl.addEventListener("scroll", scheduleRender, { passive: true });
  window.addEventListener("resize", () => {
    ensureRowPool();
    lastFirstVr = -1;
    lastScrollTop = -1;
    scheduleRender();
  });
  
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

  function fileLooksLikeNxb(file) {
    const n = (file.name || "").toLowerCase();
    return n.endsWith(".nxb");
  }

  /** Reuse the reader's backing buffer when it already spans the full ArrayBuffer. */
  function bindRawBufferFromReader() {
    const view = reader.bytes;
    rawBuffer =
      view.byteOffset === 0 && view.byteLength === view.buffer.byteLength
        ? view.buffer
        : view.buffer.slice(view.byteOffset, view.byteOffset + view.byteLength);
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
      } else if (fileLooksLikeNxb(file)) {
        if (typeof file.stream === "function") {
          await loadFromReadableStream(file.stream(), file.name, file.size);
        } else {
          const buf = await file.arrayBuffer();
          await loadBuffer(buf, file.name, file.size);
        }
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
          await closeLocalNxbCache();
          _lastLoadedUrl = null;
          overlayEl.textContent = `Fetching ${name}…`;
          const { reader: r, buffer } = await loadNxsDataset(path);
          if (gen !== loadGeneration) return;
          jsonRecords = null;
          streamReader = null;
          recordOffsets = null;
          recordFileOffsets = null;
          streamCacheBytes = null;
          streamCacheAccessor = null;
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
          lastFixtureBase = fixtureBaseFromPath(path);
          let len = fixtureSizeHint(path, 0);
          try {
            const head = await fetch(path, { method: "HEAD" });
            if (head.ok) {
              len = parseInt(head.headers.get("content-length") || "0", 10) || len;
            }
          } catch {
            /* HEAD optional in dev */
          }
          await loadNxbFromUrl(path, name, len);
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

  async function runJsonNxsComparison() {
    if (!lastFixtureBase) {
      compareJson.textContent = "JSON: load a records_* fixture first";
      return;
    }
    const jsonPath = `/bench/fixtures/${lastFixtureBase}.json`;
    const nxbPath = `/bench/fixtures/${lastFixtureBase}.nxb`;
    compareRunBtn.disabled = true;
    compareJson.textContent = "JSON: fetching…";
    compareNxs.textContent = "NXS: fetching…";
    try {
      const jsonRes = await fetch(jsonPath);
      if (!jsonRes.ok) throw new Error(`no ${lastFixtureBase}.json`);
      const jsonText = await jsonRes.text();
      const tJson = performance.now();
      JSON.parse(jsonText);
      const jsonMs = performance.now() - tJson;
      compareJson.textContent = `JSON: parse ${jsonMs.toFixed(0)} ms (UI blocked)`;

      const nxbRes = await fetch(nxbPath);
      if (!nxbRes.ok) throw new Error(`no ${lastFixtureBase}.nxb`);
      const buf = await nxbRes.arrayBuffer();
      const tNxs = performance.now();
      const r = new NxsReader(buf);
      const nxsMs = performance.now() - tNxs;
      compareNxs.textContent = `NXS: open ${nxsMs.toFixed(2)} ms · ${fmtInt(r.recordCount)} rows`;
    } catch (e) {
      compareJson.textContent = `JSON: ${e.message}`;
    } finally {
      compareRunBtn.disabled = false;
    }
  }

  if (compareRunBtn) {
    compareRunBtn.addEventListener("click", () => runJsonNxsComparison());
  }
  updateTelemetry();
}
