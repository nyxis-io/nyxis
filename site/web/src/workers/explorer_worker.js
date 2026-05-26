// Explorer search worker — field-index / reader.scan fast paths; sparse prefetch during scan.

import { HINT_RANDOM, NxsReader, NxsStreamReader, SparseBytes } from "/sdk/nxs.js";
import {
  buildExplorerSearchSpec,
  scanExplorerNxbRecords,
  scanExplorerNxbRecordsSparse,
} from "../utils/explorerNxbSearch.js";

const NXS_MAGIC = 0x4E595842; // NYXB
const NXS_FOOTER_MAGIC = 0x2153584E; // NXS!

let reader = null;
/** @type {ReturnType<typeof buildExplorerSearchSpec> | null} */
let searchSpec = null;
/** @type {Awaited<ReturnType<typeof openChunkCache>> | null} */
let chunkCache = null;
let loadGeneration = 0;
/** @type {{ query: string, token: number } | null} */
let pendingSearch = null;
let activeToken = 0;

function idbRequest(req) {
  return new Promise((resolve, reject) => {
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error || new Error("IndexedDB request failed"));
  });
}

async function openChunkCache(dbName) {
  const db = await idbRequest(indexedDB.open(dbName));

  return {
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
          try {
            tx.abort();
          } catch {}
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
    async getAllSorted() {
      const tx = db.transaction("chunks", "readonly");
      const all = await idbRequest(tx.objectStore("chunks").getAll());
      all.sort((a, b) => a.start - b.start);
      return all;
    },
  };
}

async function openCachedReader(dbName, fileSize) {
  const cache = await openChunkCache(dbName);
  chunkCache = cache;
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
  reader = new NxsReader(sparse, { fetchRange, hint: HINT_RANDOM, maxPages: 512 });
}

function finishLoad(searchColumn) {
  searchSpec = searchColumn ? buildExplorerSearchSpec(reader, searchColumn) : null;
  const searchMode = reader._sparse ? "sparse" : "memory";
  self.postMessage({ type: "loaded", recordCount: reader.recordCount, searchMode });
  if (pendingSearch !== null) {
    const pending = pendingSearch;
    pendingSearch = null;
    void runSearch(pending.query, pending.token);
  }
}

async function loadFromFetch(url, gen, searchColumn) {
  const res = await fetch(url);
  if (gen !== loadGeneration) return;
  if (!res.ok) throw new Error(`HTTP ${res.status}`);

  if (!res.body) {
    const buf = await res.arrayBuffer();
    if (gen !== loadGeneration) return;
    reader = new NxsReader(new Uint8Array(buf));
    finishLoad(searchColumn);
    return;
  }

  let parsed = 0;
  const sr = new NxsStreamReader({
    compactionEnabled: false,
    onRecord(_obj, idx) {
      parsed = idx + 1;
      if ((idx & 0x3fff) === 0) {
        self.postMessage({ type: "load-progress", parsed });
      }
    },
    onError(err) {
      if (gen !== loadGeneration) return;
      self.postMessage({ type: "load-error", message: err.message });
    },
  });

  const webReader = res.body.getReader();
  while (true) {
    const { done, value } = await webReader.read();
    if (gen !== loadGeneration) {
      await webReader.cancel?.();
      return;
    }
    if (done) break;
    sr.push(value);
  }
  if (gen !== loadGeneration) return;
  reader = sr.finish();
  finishLoad(searchColumn);
}

async function runSearch(query, replyToken) {
  const token = replyToken;
  activeToken = replyToken;
  if (!reader || !searchSpec) {
    self.postMessage({ type: "search-done", token, matches: new Int32Array(0), aborted: true });
    return;
  }
  if (!query) {
    self.postMessage({ type: "search-done", token, matches: new Int32Array(0) });
    return;
  }

  const needle = query.trim().toLowerCase();
  const t0 = performance.now();
  const progress = (payload) => {
    self.postMessage({
      type: "search-progress",
      token,
      scanned: payload.scanned,
      total: payload.total,
      matches: payload.matches,
      searchMode: payload.searchMode,
      elapsedMs: performance.now() - t0,
    });
  };

  const opts = {
    token,
    getActiveToken: () => activeToken,
    onProgress: progress,
  };

  const { matches, aborted } = reader._sparse
    ? await scanExplorerNxbRecordsSparse(reader, searchSpec, needle, opts, chunkCache)
    : scanExplorerNxbRecords(reader, searchSpec, null, needle, opts);

  if (aborted || token !== activeToken) {
    self.postMessage({ type: "search-done", token, matches: new Int32Array(0), aborted: true });
    return;
  }

  self.postMessage(
    { type: "search-done", token, matches, elapsedMs: performance.now() - t0 },
    [matches.buffer],
  );
}

self.addEventListener("message", async (ev) => {
  const msg = ev.data;

  if (msg.type === "load-url") {
    const gen = ++loadGeneration;
    reader = null;
    chunkCache = null;
    searchSpec = null;
    pendingSearch = null;
    try {
      await loadFromFetch(msg.url, gen, msg.searchColumn);
    } catch (e) {
      if (gen !== loadGeneration) return;
      self.postMessage({ type: "load-error", message: e.message });
    }
    return;
  }

  if (msg.type === "load") {
    loadGeneration++;
    chunkCache = null;
    reader = new NxsReader(new Uint8Array(msg.buffer));
    finishLoad(msg.searchColumn);
    return;
  }

  if (msg.type === "load-cache") {
    const gen = ++loadGeneration;
    reader = null;
    chunkCache = null;
    searchSpec = null;
    pendingSearch = null;
    try {
      await openCachedReader(msg.dbName, msg.fileSize);
      if (gen !== loadGeneration) return;
      finishLoad(msg.searchColumn);
    } catch (e) {
      if (gen !== loadGeneration) return;
      self.postMessage({ type: "load-error", message: e.message });
    }
    return;
  }

  if (msg.type === "search") {
    const query = msg.query;
    const token = msg.token;
    if (!reader) {
      pendingSearch = query ? { query, token } : null;
      return;
    }
    void runSearch(query, token);
  }
});
