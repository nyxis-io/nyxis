// NXS worker — reads (and optionally writes) records from a shared Uint8Array
// backed by either a SharedArrayBuffer (real zero-copy) or a per-worker copy
// (fallback when crossOriginIsolated is false).

import { NxsReader } from "/sdk/nxs.js";

let reader = null;
let view = null;           // DataView over the same buffer as reader.bytes
let bytes = null;          // Uint8Array
let slotCache = new Map(); // key → slot index
let offsetCache = new Map(); // `${recordIndex}:${slot}` → absolute byte offset

function ensureOffset(recordIndex, slot) {
  const cacheKey = recordIndex * 10000 + slot; // slot < 10000 safely
  const cached = offsetCache.get(cacheKey);
  if (cached !== undefined) return cached;
  const obj = reader.record(recordIndex);
  // Force header parse by accessing via typed getter once.
  obj.getF64BySlot(slot);
  const off = obj._resolveSlot(slot);
  offsetCache.set(cacheKey, off);
  return off;
}

function getSlot(key) {
  let s = slotCache.get(key);
  if (s === undefined) {
    s = reader.slot(key);
    slotCache.set(key, s);
  }
  return s;
}

self.onmessage = (ev) => {
  const msg = ev.data;

  if (msg.type === "init") {
    const t0 = performance.now();
    bytes = new Uint8Array(msg.buffer, 0, msg.size);
    reader = new NxsReader(bytes);
    view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const t1 = performance.now();
    self.postMessage({
      type: "ready",
      workerId: msg.workerId,
      initMs: t1 - t0,
      recordCount: reader.recordCount,
      shared: bytes.buffer instanceof SharedArrayBuffer,
    });
    return;
  }

  if (msg.type === "read") {
    const { index, key, requestId } = msg;
    const slot = getSlot(key);
    const obj = reader.record(index);
    const sigil = reader.keySigils[slot];
    let value;
    // Map sigil to accessor
    if (sigil === 0x22) value = obj.getStrBySlot(slot);       // "
    else if (sigil === 0x7E) value = obj.getF64BySlot(slot);  // ~
    else if (sigil === 0x3F) value = obj.getBoolBySlot(slot); // ?
    else value = obj.getI64BySlot(slot);                       // =, @, default
    self.postMessage({ type: "read-result", requestId, index, key, value });
    return;
  }

  if (msg.type === "write-f64") {
    const { index, key, value } = msg;
    const slot = getSlot(key);
    const off = ensureOffset(index, slot);
    if (off < 0) {
      self.postMessage({ type: "write-result", ok: false, reason: "field-absent" });
      return;
    }
    // In-place write. If backed by SAB, other workers see this immediately.
    view.setFloat64(off, value, true);
    self.postMessage({ type: "write-result", ok: true, index, key, value });
    return;
  }

  if (msg.type === "read-f64-fast") {
    // Repeated polling from main — no requestId needed, just pushes values.
    const { index, key, tag } = msg;
    const slot = getSlot(key);
    const off = ensureOffset(index, slot);
    const value = off < 0 ? null : view.getFloat64(off, true);
    self.postMessage({ type: "read-fast-result", tag, index, key, value });
    return;
  }
};
