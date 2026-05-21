// Log Explorer search worker.
//
// Loads .nxb via NxsStreamReader when fetching a URL (chunks while downloading),
// or NxsReader when the main thread transfers a complete ArrayBuffer. Substring
// search streams progress back to the main thread; results are an Int32Array of
// matching record indices.

import { NxsReader, NxsStreamReader } from "/sdk/nxs.js";

let reader = null;
let usernameSlot = -1;
let emailSlot = -1;
let loadGeneration = 0;
/** @type {{ query: string, token: number } | null} */
let pendingSearch = null;

// A monotonic token that lets the main thread cancel a stale search by
// starting a new one. Only the most-recent token's results are emitted.
let activeToken = 0;

function bindSlots() {
  usernameSlot = reader.slot("username");
  try { emailSlot = reader.slot("email"); } catch { emailSlot = -1; }
}

function finishLoad() {
  bindSlots();
  self.postMessage({ type: "loaded", recordCount: reader.recordCount });
  if (pendingSearch !== null) {
    const pending = pendingSearch;
    pendingSearch = null;
    runSearch(pending.query, pending.token);
  }
}

async function loadFromFetch(url, gen) {
  const res = await fetch(url);
  if (gen !== loadGeneration) return;
  if (!res.ok) throw new Error(`HTTP ${res.status}`);

  if (!res.body) {
    const buf = await res.arrayBuffer();
    if (gen !== loadGeneration) return;
    reader = new NxsReader(new Uint8Array(buf));
    finishLoad();
    return;
  }

  let sr;
  let parsed = 0;
  sr = new NxsStreamReader({
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
  finishLoad();
}

function runSearch(query, replyToken) {
  const token = replyToken;
  activeToken = replyToken;
  if (!reader) {
    self.postMessage({ type: "search-done", token, matches: new Int32Array(0), aborted: true });
    return;
  }
  if (!query) {
    self.postMessage({ type: "search-done", token, matches: new Int32Array(0) });
    return;
  }

  const needle = query.toLowerCase();
  const n = reader.recordCount;
  const cur = reader.cursor();

  let results = new Int32Array(Math.min(n, 1024));
  let matchCount = 0;

  const BATCH = 250_000;
  let last = performance.now();

  for (let i = 0; i < n; i++) {
    cur.seek(i);
    const u = cur.getStrBySlot(usernameSlot);
    if (u && u.toLowerCase().indexOf(needle) !== -1) {
      if (matchCount >= results.length) {
        const grown = new Int32Array(results.length * 2);
        grown.set(results);
        results = grown;
      }
      results[matchCount++] = i;
    }

    if ((i & (BATCH - 1)) === (BATCH - 1)) {
      if (token !== activeToken) {
        self.postMessage({ type: "search-done", token, matches: new Int32Array(0), aborted: true });
        return;
      }
      const now = performance.now();
      self.postMessage({
        type: "search-progress",
        token,
        scanned: i + 1,
        total: n,
        matches: matchCount,
        elapsedMs: now - last,
      });
      last = now;
    }
  }

  const trimmed = results.slice(0, matchCount);
  self.postMessage({ type: "search-done", token, matches: trimmed }, [trimmed.buffer]);
}

self.addEventListener("message", async (ev) => {
  const msg = ev.data;

  if (msg.type === "load-url") {
    const gen = ++loadGeneration;
    reader = null;
    pendingSearch = null;
    try {
      await loadFromFetch(msg.url, gen);
    } catch (e) {
      if (gen !== loadGeneration) return;
      self.postMessage({ type: "load-error", message: e.message });
    }
    return;
  }

  if (msg.type === "load") {
    loadGeneration++;
    reader = new NxsReader(new Uint8Array(msg.buffer));
    finishLoad();
    return;
  }

  if (msg.type === "search") {
    const query = msg.query;
    const token = msg.token;
    if (!reader) {
      pendingSearch = query ? { query, token } : null;
      return;
    }
    runSearch(query, token);
    return;
  }
});
