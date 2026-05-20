// Log Explorer search worker.
//
// The worker receives the raw .nxb ArrayBuffer (transferred zero-copy) and
// the schema slot to search. It performs substring matches and streams
// progress back to the main thread. Results accumulate as an Int32Array
// of matching record indices.

import { NxsReader } from "/sdk/nxs.js";

let reader = null;
let usernameSlot = -1;
let emailSlot = -1;

// A monotonic token that lets the main thread cancel a stale search by
// starting a new one. Only the most-recent token's results are emitted.
let activeToken = 0;

self.addEventListener("message", async (ev) => {
  const msg = ev.data;

  if (msg.type === "load-url") {
    // Fetch the file directly — zero bytes copied from the main thread.
    try {
      const res = await fetch(msg.url);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const buf = await res.arrayBuffer();
      reader = new NxsReader(new Uint8Array(buf));
      usernameSlot = reader.slot("username");
      try { emailSlot = reader.slot("email"); } catch { emailSlot = -1; }
      self.postMessage({ type: "loaded", recordCount: reader.recordCount });
    } catch (e) {
      self.postMessage({ type: "load-error", message: e.message });
    }
    return;
  }

  if (msg.type === "load") {
    // Fallback: transfer of the ArrayBuffer (drag-and-drop, no URL available).
    reader = new NxsReader(new Uint8Array(msg.buffer));
    usernameSlot = reader.slot("username");
    try { emailSlot = reader.slot("email"); } catch { emailSlot = -1; }
    self.postMessage({ type: "loaded", recordCount: reader.recordCount });
    return;
  }

  if (msg.type === "search") {
    const token = ++activeToken;
    const query = msg.query;
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
    // Reusable cursor avoids per-record allocation.
    const cur = reader.cursor();

    // Pre-allocate a results buffer sized pessimistically; we'll trim it.
    // A 10M-record search at worst matches all — 40 MB is fine.
    let results = new Int32Array(Math.min(n, 1024));
    let matchCount = 0;

    // Progress cadence: yield every ~250k records so the main thread can
    // report progress and keep the cancel path responsive.
    const BATCH = 250_000;
    let last = performance.now();

    for (let i = 0; i < n; i++) {
      cur.seek(i);
      const u = cur.getStrBySlot(usernameSlot);
      // Case-insensitive substring. toLowerCase() allocates, but the
      // username field is ~10 chars so the cost is absorbed by the cursor
      // path and still hits tens of millions of reads per second.
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
          // A newer search has started; abandon this one without emitting.
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

    // Trim to actual match count before transferring.
    const trimmed = results.slice(0, matchCount);
    self.postMessage({ type: "search-done", token, matches: trimmed }, [trimmed.buffer]);
    return;
  }
});
