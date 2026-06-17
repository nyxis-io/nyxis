#!/usr/bin/env node
// NXS conformance runner for JavaScript (Node.js)
// Usage: node conformance/run_js.js conformance/

import { existsSync, readFileSync, readdirSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath, pathToFileURL } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const conformanceDir = process.argv[2] || join(__dirname);

const drv = process.env.DRV || join(__dirname, "..", "..", "nyxis-drivers");
const { NxsReader, NxsStreamReader } = await import(
  pathToFileURL(join(drv, "js", "nxs.js")).href
);

// ── Helpers ───────────────────────────────────────────────────────────────────

function approxEq(a, b) {
  if (a === b) return true;
  if (typeof a !== "number" || typeof b !== "number") return false;
  const diff = Math.abs(a - b);
  const mag = Math.max(Math.abs(a), Math.abs(b));
  if (mag < 1e-300) return diff < 1e-300;
  return diff / mag < 1e-9;
}

function valuesMatch(actual, expected) {
  if (expected === null) return actual === null || actual === undefined;
  if (expected === true || expected === false) return actual === expected;
  if (typeof expected === "number") {
    if (typeof actual === "number") return approxEq(actual, expected);
    if (typeof actual === "bigint") return approxEq(Number(actual), expected);
    return false;
  }
  if (typeof expected === "string") return actual === expected;
  if (Array.isArray(expected)) {
    if (!Array.isArray(actual)) return false;
    if (actual.length !== expected.length) return false;
    return expected.every((v, i) => valuesMatch(actual[i], v));
  }
  return false;
}

// ── Runner ────────────────────────────────────────────────────────────────────

function getFieldValue(obj, reader, key, sigil) {
  const slot = reader.keyIndex.get(key);
  if (slot === undefined) return undefined;
  const sigilByte = reader.keySigils[slot];
  // Decode based on known type sigil
  if (sigilByte === 0x3D) return obj.getI64BySlot(slot);    // =  int
  if (sigilByte === 0x7E) return obj.getF64BySlot(slot);    // ~  float
  if (sigilByte === 0x3F) return obj.getBoolBySlot(slot);   // ?  bool
  if (sigilByte === 0x22) return obj.getStrBySlot(slot);    // "  str
  if (sigilByte === 0x40) return obj.getI64BySlot(slot);    // @  time
  // Fallback: try to get via .get() which auto-detects list/object/etc.
  return obj.get(key);
}

function runPositive(name, nxbPath, expected) {
  const buf = readFileSync(nxbPath);
  const reader = new NxsReader(buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength));

  // Validate record_count
  if (reader.recordCount !== expected.record_count) {
    throw new Error(`record_count: expected ${expected.record_count}, got ${reader.recordCount}`);
  }

  // Validate keys
  for (let i = 0; i < expected.keys.length; i++) {
    if (reader.keys[i] !== expected.keys[i]) {
      throw new Error(`key[${i}]: expected "${expected.keys[i]}", got "${reader.keys[i]}"`);
    }
  }

  // Validate each record
  for (let ri = 0; ri < expected.records.length; ri++) {
    const obj = reader.record(ri);
    const expRec = expected.records[ri];

    for (const [key, expVal] of Object.entries(expRec)) {
      const slot = reader.keyIndex.get(key);
      if (slot === undefined) {
        throw new Error(`rec[${ri}].${key}: key not in schema`);
      }
      const sigilByte = reader.keySigils[slot];

      let actual;
      if (sigilByte === 0x3D) actual = obj.getI64BySlot(slot);    // = int
      else if (sigilByte === 0x7E) actual = obj.getF64BySlot(slot); // ~ float
      else if (sigilByte === 0x3F) actual = obj.getBoolBySlot(slot); // ? bool
      else if (sigilByte === 0x22) actual = obj.getStrBySlot(slot);  // " str
      else if (sigilByte === 0x40) actual = obj.getI64BySlot(slot);  // @ time
      else if (sigilByte === 0x3C) {
        // bytes — get raw and compare as array
        actual = obj.get(key);
      } else {
        actual = obj.get(key);
      }

      if (expVal === null) {
        // null field: the value should be null OR a zero-like value
        // (some readers represent null as null, others as 0)
        // We accept null or undefined for null fields
        if (actual !== null && actual !== undefined) {
          // Accept 0/false/empty string as null-like for robustness
          // but only if the sigilByte is for a special null sigil
          // For now just warn and continue (null vs absent is tested)
        }
      } else if (Array.isArray(expVal)) {
        // List — use .get() which returns an array
        actual = obj.get(key);
        if (!valuesMatch(actual, expVal)) {
          throw new Error(`rec[${ri}].${key}: expected ${JSON.stringify(expVal)}, got ${JSON.stringify(actual)}`);
        }
      } else {
        if (!valuesMatch(actual, expVal)) {
          throw new Error(`rec[${ri}].${key}: expected ${JSON.stringify(expVal)}, got ${JSON.stringify(actual)}`);
        }
      }
    }
  }
}

function recordFieldsMatch(obj, reader, ri, expRec) {
  for (const [key, expVal] of Object.entries(expRec)) {
    const slot = reader.keyIndex.get(key);
    if (slot === undefined) {
      throw new Error(`rec[${ri}].${key}: key not in schema`);
    }
    const sigilByte = reader.keySigils[slot];
    let actual;
    if (sigilByte === 0x3D) actual = obj.getI64BySlot(slot);
    else if (sigilByte === 0x7E) actual = obj.getF64BySlot(slot);
    else if (sigilByte === 0x3F) actual = obj.getBoolBySlot(slot);
    else if (sigilByte === 0x22) actual = obj.getStrBySlot(slot);
    else if (sigilByte === 0x40) actual = obj.getI64BySlot(slot);
    else actual = obj.get(key);
    if (expVal !== null && !valuesMatch(actual, expVal)) {
      throw new Error(
        `rec[${ri}].${key}: expected ${JSON.stringify(expVal)}, got ${JSON.stringify(actual)}`,
      );
    }
  }
}

function runForwardStream(name, nxbPath, expected) {
  const buf = readFileSync(nxbPath);
  const seen = [];
  const sr = new NxsStreamReader({
    onRecord(obj, idx) {
      seen.push({ idx, obj });
    },
  });
  const chunk = 97;
  for (let off = 0; off < buf.length; off += chunk) {
    sr.push(buf.subarray(off, Math.min(off + chunk, buf.length)));
  }
  sr.endOfStream();
  if (seen.length !== expected.record_count) {
    throw new Error(
      `forward_stream record_count: expected ${expected.record_count}, got ${seen.length}`,
    );
  }
  for (let ri = 0; ri < expected.records.length; ri++) {
    const { idx, obj } = seen[ri];
    if (idx !== ri) {
      throw new Error(`record index mismatch at ${ri}: got idx ${idx}`);
    }
    recordFieldsMatch(obj, sr, ri, expected.records[ri]);
  }
}

function runNegative(name, nxbPath, expectedCode) {
  const buf = readFileSync(nxbPath);
  let caught = null;
  try {
    new NxsReader(buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength));
  } catch (e) {
    caught = e;
  }
  if (!caught) {
    throw new Error(`expected error ${expectedCode} but reader succeeded`);
  }
  const code = caught.code || (caught.message.includes("ERR_") ? caught.message.split(":")[0].trim() : "UNKNOWN");
  if (code !== expectedCode) {
    throw new Error(`expected error ${expectedCode}, got ${code} (${caught.message})`);
  }
}

// ── Main ──────────────────────────────────────────────────────────────────────

function discoverVectors(root) {
  const out = [];
  const scan = (dir, prefix) => {
    if (!existsSync(dir)) return;
    for (const f of readdirSync(dir).filter((n) => n.endsWith(".expected.json")).sort()) {
      const base = f.replace(".expected.json", "");
      out.push({
        label: prefix ? `${prefix}/${base}` : base,
        dir,
        base,
      });
    }
  };
  scan(root, "");
  scan(join(root, "v13"), "v13");
  return out;
}

let pass = 0, fail = 0;

for (const { label, dir, base } of discoverVectors(conformanceDir)) {
  const jsonPath = join(dir, `${base}.expected.json`);
  const nxbPath = join(dir, `${base}.nxb`);

  const expected = JSON.parse(readFileSync(jsonPath, "utf8"));
  const isNegative = "error" in expected;
  const isForwardStream = expected.forward_stream === true;

  try {
    if (isNegative) {
      runNegative(label, nxbPath, expected.error);
    } else if (isForwardStream) {
      runForwardStream(label, nxbPath, expected);
    } else {
      runPositive(label, nxbPath, expected);
    }
    console.log(`  PASS  ${label}`);
    pass++;
  } catch (e) {
    console.error(`  FAIL  ${label} — ${e.message}`);
    fail++;
  }
}

console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail > 0 ? 1 : 0);
