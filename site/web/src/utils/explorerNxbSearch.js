/**
 * NXB substring search — optimized scan paths (field index + reader.scan).
 * Shared by the Explorer demo main thread and explorer_worker.js.
 */

import { WIRE_SIGILS } from "/sdk/nxs.js";

const ABSENT_OFFSET = 0xffffffff;
const CANCEL_BATCH = 8192;
const PROGRESS_EVERY = 100_000;

/** @type {WeakMap<object, Map<string, import("/sdk/nxs.js").NxsFieldIndex | null>>} */
const fieldIndexCache = new WeakMap();

export function safeReaderSlot(reader, key) {
  try {
    return reader.slot(key);
  } catch {
    return undefined;
  }
}

/**
 * @param {import("/sdk/nxs.js").NxsReader} reader
 * @param {{ key: string, slot?: number, sigil?: number }} searchColumn
 */
export function buildExplorerSearchSpec(reader, searchColumn) {
  const key = searchColumn.key;
  const valueSlot = searchColumn.slot ?? safeReaderSlot(reader, key);
  if (valueSlot === undefined) return null;
  const sigil =
    searchColumn.sigil ??
    (reader.keySigils && reader.keyIndex ? reader.keySigils[reader.keyIndex.get(key)] : undefined);
  return { key, valueSlot, sigil };
}

function byteAt(bytes, i) {
  if (bytes?._isSparseBytes) return bytes.readByte(i);
  return bytes[i];
}

function rdU32At(bytes, off) {
  return (
    byteAt(bytes, off) |
    (byteAt(bytes, off + 1) << 8) |
    (byteAt(bytes, off + 2) << 16) |
    (byteAt(bytes, off + 3) << 24)
  ) >>> 0;
}

/**
 * Case-insensitive substring search on UTF-8 bytes without allocating a string
 * when both needle and haystack are ASCII.
 *
 * @param {Uint8Array | import("/sdk/sparse_bytes.js").SparseBytes} bytes
 * @param {number} start
 * @param {number} len
 * @param {string} needle lowercased, non-empty
 */
function utf8RangeIncludesNeedleU8(bytes, start, len, needle) {
  const nlen = needle.length;
  if (len < nlen) return false;
  const end = start + len - nlen;
  outer: for (let i = start; i <= end; i++) {
    for (let j = 0; j < nlen; j++) {
      let a = bytes[i + j];
      if (a & 0x80) {
        return slowUtf8Includes(bytes, start, len, needle);
      }
      const b = needle.charCodeAt(j);
      if (a >= 65 && a <= 90) a += 32;
      if (a !== b) continue outer;
    }
    return true;
  }
  return false;
}

/** @param {Uint8Array | { _isSparseBytes?: boolean }} bytes */
export function utf8RangeIncludesNeedle(bytes, start, len, needle) {
  if (bytes instanceof Uint8Array && !bytes._isSparseBytes) {
    return utf8RangeIncludesNeedleU8(bytes, start, len, needle);
  }
  const nlen = needle.length;
  if (len < nlen) return false;
  const end = start + len - nlen;
  outer: for (let i = start; i <= end; i++) {
    for (let j = 0; j < nlen; j++) {
      let a = byteAt(bytes, i + j);
      if (a & 0x80) {
        return slowUtf8Includes(bytes, start, len, needle);
      }
      const b = needle.charCodeAt(j);
      if (a >= 65 && a <= 90) a += 32;
      if (a !== b) continue outer;
    }
    return true;
  }
  return false;
}

function slowUtf8Includes(bytes, start, len, needle) {
  const end = start + len;
  return String.fromCharCode.apply(null, bytes.subarray(start, end)).toLowerCase().includes(needle);
}

function rdU16At(bytes, off) {
  return byteAt(bytes, off) | (byteAt(bytes, off + 1) << 8);
}

/**
 * Absolute NYXO start offset per record from the tail index (already resident for sparse open).
 *
 * @param {import("/sdk/nxs.js").NxsReader} reader
 */
export function extractRecordAbsOffsets(reader) {
  const n = reader.recordCount;
  const view = reader.view;
  const tailStart = reader._tailStart;
  const out = new Uint32Array(n);
  for (let i = 0; i < n; i++) {
    const entryOff = tailStart + i * 10 + 2;
    const lo = view.getUint32(entryOff, true);
    const hi = view.getUint32(entryOff + 4, true);
    out[i] = hi * 0x100000000 + lo;
  }
  return out;
}

/**
 * Uniform row-layout bench files: fixed offset-table position for one slot.
 *
 * @param {import("/sdk/nxs.js").NxsReader} reader
 * @param {{ valueSlot: number }} spec
 */
export async function prepareUniformStringSearch(reader, spec) {
  if (!isStringSigil(spec.sigil)) return null;
  // isUniform() is O(n) and touches every record — catastrophic on sparse/IDB (10M random reads).
  if (!reader._sparse) {
    try {
      if (!reader.isUniform()) return null;
    } catch {
      return null;
    }
  } else {
    await reader.prefetch_viewport(0, Math.min(reader.recordCount - 1, 2));
  }
  const layout = reader._computeFastLayout(spec.valueSlot);
  if (!layout.present) return null;
  const offsetTablePos = 8 + layout.bitmaskLen + layout.tableIdx * 2;
  return { recordAbs: extractRecordAbsOffsets(reader), offsetTablePos };
}

/**
 * Scan by walking IndexedDB chunks in file order (sequential read, no per-record IDB random I/O).
 *
 * @param {{ forEachChunkSorted: (fn: (chunk: { start: number, data: Uint8Array }) => void | Promise<void>) => Promise<void> }} idb
 * @param {{ recordAbs: Uint32Array, offsetTablePos: number }} layout
 * @param {string} needle
 * @param {number} recordCount
 * @param {*} opts
 */
/**
 * Synchronous scan over preloaded IDB chunks (one getAll, no per-chunk async).
 *
 * @param {{ start: number, data: Uint8Array }[]} sortedChunks
 */
export function scanExplorerNxbChunksSequential(sortedChunks, layout, needle, recordCount, opts) {
  const { recordAbs, offsetTablePos } = layout;
  let results = new Int32Array(Math.min(recordCount, 1024));
  let matchCount = 0;
  let i = 0;
  let lastProgressMs = 0;

  for (const chunk of sortedChunks) {
    if (opts.token !== opts.getActiveToken?.()) {
      return { matches: new Int32Array(0), aborted: true };
    }
    const chunkStart = chunk.start;
    const data = chunk.data;
    const chunkEnd = chunkStart + data.byteLength;

    while (i < recordCount && recordAbs[i] < chunkStart) i++;

    for (; i < recordCount && recordAbs[i] < chunkEnd; i++) {
      const recAbs = recordAbs[i];
      const rel = recAbs - chunkStart;
      if (rel + offsetTablePos + 2 > data.byteLength) continue;

      const relOff = data[rel + offsetTablePos] | (data[rel + offsetTablePos + 1] << 8);
      const inChunk = recAbs + relOff - chunkStart;
      if (inChunk + 4 > data.byteLength) continue;

      const len =
        data[inChunk] |
        (data[inChunk + 1] << 8) |
        (data[inChunk + 2] << 16) |
        (data[inChunk + 3] << 24);
      const strStart = inChunk + 4;
      if (strStart + len > data.byteLength) continue;

      if (utf8RangeIncludesNeedleU8(data, strStart, len, needle)) {
        ({ results, matchCount } = pushMatch(results, matchCount, i));
      }
    }

    const now = performance.now();
    if (now - lastProgressMs >= 150) {
      lastProgressMs = now;
      opts.onProgress?.({ scanned: i, total: recordCount, matches: matchCount, searchMode: "sequential" });
    }
  }
  opts.onProgress?.({ scanned: recordCount, total: recordCount, matches: matchCount, searchMode: "sequential" });

  if (opts.token !== opts.getActiveToken?.()) {
    return { matches: new Int32Array(0), aborted: true };
  }
  return { matches: results.slice(0, matchCount), aborted: false };
}

export async function scanExplorerNxbIdbSequential(idb, layout, needle, recordCount, opts) {
  const chunks = idb.getAllSorted
    ? await idb.getAllSorted()
    : await readChunksViaCursor(idb);
  return scanExplorerNxbChunksSequential(chunks, layout, needle, recordCount, opts);
}

async function readChunksViaCursor(idb) {
  /** @type {{ start: number, data: Uint8Array }[]} */
  const chunks = [];
  if (!idb.forEachChunkSorted) return chunks;
  await idb.forEachChunkSorted((chunk) => {
    chunks.push(chunk);
  });
  chunks.sort((a, b) => a.start - b.start);
  return chunks;
}

function cachedFieldIndex(reader, key) {
  let perReader = fieldIndexCache.get(reader);
  if (!perReader) {
    perReader = new Map();
    fieldIndexCache.set(reader, perReader);
  }
  if (!perReader.has(key)) {
    try {
      perReader.set(key, reader.buildFieldIndex(key));
    } catch {
      perReader.set(key, null);
    }
  }
  return perReader.get(key);
}

function pushMatch(results, matchCount, recordIndex) {
  if (matchCount >= results.length) {
    const grown = new Int32Array(results.length * 2);
    grown.set(results);
    results = grown;
  }
  results[matchCount] = recordIndex;
  return { results, matchCount: matchCount + 1 };
}

function isStringSigil(sigil) {
  return sigil === undefined || sigil === WIRE_SIGILS.str;
}

/**
 * @param {import("/sdk/nxs.js").NxsFieldIndex} index
 * @param {string} needle
 * @param {number} total
 * @param {{ token?: number, getActiveToken?: () => number, onProgress?: Function, recordIndexes?: Int32Array | null }} opts
 */
function scanFieldIndexStrings(index, needle, total, opts) {
  const bytes = index.reader.bytes;
  const offsets = index.offsets;
  const useMap = opts.recordIndexes != null && opts.recordIndexes.length !== total;
  let results = new Int32Array(Math.min(total, 1024));
  let matchCount = 0;

  if (useMap) {
    const recordIndexes = opts.recordIndexes;
    for (let pos = 0; pos < recordIndexes.length; pos++) {
      const i = recordIndexes[pos];
      const o = offsets[i];
      if (o === ABSENT_OFFSET) continue;
      const len = rdU32At(bytes, o);
      if (utf8RangeIncludesNeedle(bytes, o + 4, len, needle)) {
        ({ results, matchCount } = pushMatch(results, matchCount, i));
      }
      if ((pos & (CANCEL_BATCH - 1)) === CANCEL_BATCH - 1) {
        if (opts.token !== opts.getActiveToken?.()) {
          return { matches: new Int32Array(0), aborted: true };
        }
        if ((pos & (PROGRESS_EVERY - 1)) === PROGRESS_EVERY - 1) {
          opts.onProgress?.({ scanned: pos + 1, total: recordIndexes.length, matches: matchCount });
        }
      }
    }
    return { matches: results.slice(0, matchCount), aborted: false };
  }

  for (let i = 0; i < total; i++) {
    const o = offsets[i];
    if (o === ABSENT_OFFSET) continue;
    const len = rdU32At(bytes, o);
    if (utf8RangeIncludesNeedle(bytes, o + 4, len, needle)) {
      ({ results, matchCount } = pushMatch(results, matchCount, i));
    }
    if ((i & (CANCEL_BATCH - 1)) === CANCEL_BATCH - 1) {
      if (opts.token !== opts.getActiveToken?.()) {
        return { matches: new Int32Array(0), aborted: true };
      }
      if ((i & (PROGRESS_EVERY - 1)) === PROGRESS_EVERY - 1) {
        opts.onProgress?.({ scanned: i + 1, total, matches: matchCount });
      }
    }
  }
  return { matches: results.slice(0, matchCount), aborted: false };
}

/**
 * @param {ReturnType<import("/sdk/nxs.js").NxsReader["cursor"]>} cursor
 * @param {{ valueSlot: number, sigil?: number }} spec
 */
export function readExplorerSearchValue(cursor, spec) {
  const sig = spec.sigil;
  if (sig === WIRE_SIGILS.int || sig === WIRE_SIGILS.time) return cursor.getI64BySlot(spec.valueSlot);
  if (sig === WIRE_SIGILS.float) return cursor.getF64BySlot(spec.valueSlot);
  if (sig === WIRE_SIGILS.bool) return cursor.getBoolBySlot(spec.valueSlot);
  return cursor.getStrBySlot(spec.valueSlot);
}

function valueMatchesNeedle(value, needle, spec) {
  if (value === undefined) return false;
  if (isStringSigil(spec.sigil)) {
    return String(value).toLowerCase().includes(needle);
  }
  const num = Number(needle);
  if (!Number.isNaN(num) && typeof value === "number") {
    if (value === num) return true;
  }
  return String(value).toLowerCase().includes(needle);
}

/**
 * @param {import("/sdk/nxs.js").NxsReader} reader
 * @param {{ key: string, valueSlot: number, sigil?: number }} spec
 * @param {string} needle lowercased, non-empty
 * @param {number} total
 * @param {*} opts
 */
function scanWithReaderScan(reader, spec, needle, total, opts) {
  let results = new Int32Array(Math.min(total, 1024));
  let matchCount = 0;
  let pos = 0;
  reader.scan((cur, i) => {
    if (valueMatchesNeedle(readExplorerSearchValue(cur, spec), needle, spec)) {
      ({ results, matchCount } = pushMatch(results, matchCount, i));
    }
    pos++;
    if ((pos & (CANCEL_BATCH - 1)) === CANCEL_BATCH - 1) {
      if ((pos & (PROGRESS_EVERY - 1)) === PROGRESS_EVERY - 1) {
        opts.onProgress?.({ scanned: pos, total, matches: matchCount });
      }
    }
  });
  if (opts.token !== opts.getActiveToken?.()) {
    return { matches: new Int32Array(0), aborted: true };
  }
  return { matches: results.slice(0, matchCount), aborted: false };
}

/**
 * Scan record indexes for a substring match on one column.
 *
 * @param {import("/sdk/nxs.js").NxsReader} reader
 * @param {{ key: string, valueSlot: number, sigil?: number }} spec
 * @param {Int32Array | null} recordIndexes null = all records 0..recordCount-1
 * @param {string} needle lowercased, non-empty
 * @param {{ token?: number, getActiveToken?: () => number, onProgress?: Function }} [opts]
 * @returns {{ matches: Int32Array, aborted: boolean }}
 */
export function scanExplorerNxbRecords(reader, spec, recordIndexes, needle, opts = {}) {
  const total = recordIndexes?.length ?? reader.recordCount;
  const identity =
    !recordIndexes ||
    (recordIndexes.length === reader.recordCount &&
      recordIndexes[0] === 0 &&
      recordIndexes[recordIndexes.length - 1] === recordIndexes.length - 1);

  const scanOpts = { ...opts, recordIndexes: identity ? null : recordIndexes };

  if (isStringSigil(spec.sigil) && !reader._sparse) {
    const index = cachedFieldIndex(reader, spec.key);
    if (index) {
      opts.onProgress?.({ scanned: 0, total: reader.recordCount, matches: 0, searchMode: "indexed" });
      const out = scanFieldIndexStrings(index, needle, reader.recordCount, scanOpts);
      opts.onProgress?.({
        scanned: reader.recordCount,
        total: reader.recordCount,
        matches: out.matches.length,
        searchMode: "indexed",
      });
      return out;
    }
  }

  if (identity) {
    return scanWithReaderScan(reader, spec, needle, reader.recordCount, opts);
  }

  let results = new Int32Array(Math.min(total, 1024));
  let matchCount = 0;
  const cursor = reader.cursor();
  for (let pos = 0; pos < total; pos++) {
    const i = recordIndexes[pos];
    cursor.seek(i);
    if (valueMatchesNeedle(readExplorerSearchValue(cursor, spec), needle, spec)) {
      ({ results, matchCount } = pushMatch(results, matchCount, i));
    }
    if ((pos & (CANCEL_BATCH - 1)) === CANCEL_BATCH - 1) {
      if (opts.token !== opts.getActiveToken?.()) {
        return { matches: new Int32Array(0), aborted: true };
      }
      opts.onProgress?.({ scanned: pos + 1, total, matches: matchCount });
    }
  }
  return { matches: results.slice(0, matchCount), aborted: false };
}

/**
 * Sparse + IndexedDB: prefer sequential chunk scan; fall back to cursor scan.
 *
 * @param {{ forEachChunkSorted?: Function }} idb
 */
export async function scanExplorerNxbRecordsSparse(reader, spec, needle, opts, idb = null) {
  if (idb?.forEachChunkSorted && isStringSigil(spec.sigil)) {
    const layout = await prepareUniformStringSearch(reader, spec);
    if (layout) {
      return scanExplorerNxbIdbSequential(idb, layout, needle, reader.recordCount, opts);
    }
  }

  const n = reader.recordCount;
  let results = new Int32Array(Math.min(n, 1024));
  let matchCount = 0;
  const cursor = reader.cursor();

  for (let i = 0; i < n; i++) {
    if ((i & 0xffff) === 0) {
      if (opts.token !== opts.getActiveToken()) {
        return { matches: new Int32Array(0), aborted: true };
      }
      const end = Math.min(n - 1, i + 65_535);
      await reader.prefetch_viewport(i, end);
    }

    cursor.seek(i);
    if (valueMatchesNeedle(readExplorerSearchValue(cursor, spec), needle, spec)) {
      ({ results, matchCount } = pushMatch(results, matchCount, i));
    }

    if ((i & (PROGRESS_EVERY - 1)) === PROGRESS_EVERY - 1) {
      opts.onProgress?.({ scanned: i + 1, total: n, matches: matchCount });
    }
  }

  return { matches: results.slice(0, matchCount), aborted: false };
}

/**
 * @param {import("/sdk/nxs.js").NxsReader} reader
 * @param {number} recordCount
 * @returns {null | { buffer: ArrayBuffer, recordIndexes: Int32Array }}
 */
export function explorerNxbSearchSource(reader, recordCount) {
  if (!reader || recordCount <= 0) return null;
  const sourceBytes = reader.bytes;
  if (sourceBytes?._isSparseBytes) return null;

  const buffer =
    sourceBytes.byteOffset === 0 && sourceBytes.byteLength === sourceBytes.buffer.byteLength
      ? sourceBytes.buffer
      : sourceBytes.buffer.slice(sourceBytes.byteOffset, sourceBytes.byteOffset + sourceBytes.byteLength);

  const recordIndexes = new Int32Array(recordCount);
  for (let i = 0; i < recordCount; i++) recordIndexes[i] = i;
  return { buffer, recordIndexes };
}
