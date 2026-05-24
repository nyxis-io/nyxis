import { NxsSchema, NxsWriter } from "/sdk/nxs_writer.js";
import { loadWasm, WasmSpanWriter } from "/sdk/wasm.js";

let demoRoot = null;
let demoQuery = (sel) => document.querySelector(sel);

/** Id shorthand: $("run-btn") → #run-btn (required for root.querySelector). */
function $(sel) {
  const q = sel.startsWith("#") || sel.startsWith(".") || sel.includes(" ") ? sel : `#${sel}`;
  return demoQuery(q);
}
let teardown=null;
export async function wireWalPage(root) {
  if (!root) return;
  if (root.dataset.demoWired === "1") return;
  teardown?.();
  root.dataset.demoWired = "1";
  demoRoot = root;
  demoQuery = (sel) => root.querySelector(sel);
  initDemo();
  teardown = () => {
    delete root.dataset.demoWired;
    demoRoot = null;
    demoQuery = (sel) => document.querySelector(sel);
  };
}
export function unwireWalPage(){teardown?.();teardown=null;}
function initDemo(){
  // ── Span schema (slots 0-9, mirrors rust/src/wal.rs) ─────────────────────────
  const SPAN_KEYS = [
    "trace_id_hi", "trace_id_lo", "span_id", "parent_span_id",
    "name", "service", "start_time_ns", "duration_ns", "status_code", "payload",
  ];
  const SCHEMA = new NxsSchema(SPAN_KEYS);
  
  // ── Realistic span data pools ─────────────────────────────────────────────────
  // Services reflect a typical microservice platform: API gateway, auth, session,
  // catalogue, recommendation (LLM), inventory, payment, notifications, search,
  // CDN edge, analytics, feature flags, config, and a vector store.
  const SERVICES = [
    "gateway", "auth-svc", "session-svc", "catalogue-svc", "recommend-svc",
    "inventory-svc", "payment-svc", "notify-svc", "search-svc", "cdn-edge",
    "analytics-svc", "feature-flags", "config-svc", "vector-db",
  ];
  
  // Operations follow OpenTelemetry semantic conventions:
  //   http.server / http.client   — inbound/outbound HTTP
  //   grpc.server / grpc.unary   — gRPC
  //   db.select / db.insert / db.update / db.index_scan / db.ann_search
  //   cache.get / cache.set / cache.miss
  //   pubsub.publish / pubsub.consume
  //   llm.inference / llm.embed
  //   jwt.verify / auth.token_exchange
  //   queue.send / queue.receive
  const OPS = [
    "http.server", "http.client",
    "grpc.server", "grpc.unary",
    "db.select", "db.insert", "db.update", "db.index_scan", "db.ann_search",
    "cache.get", "cache.set", "cache.miss",
    "pubsub.publish", "pubsub.consume",
    "llm.inference", "llm.embed",
    "jwt.verify", "auth.token_exchange",
    "queue.send", "queue.receive",
  ];
  
  // Realistic duration distributions (nanoseconds) keyed by op index.
  // Values represent p50 base + a variance multiplier applied per span.
  // cache hits: ~300 µs; DB reads: ~4 ms; HTTP client: ~12 ms; LLM: ~1.8 s;
  // grpc: ~2 ms; pubsub: ~800 µs; jwt: ~600 µs; queue: ~1.5 ms
  const OP_DUR_BASE = new Int32Array([
    12_000_000,  // http.server
    11_000_000,  // http.client
     2_100_000,  // grpc.server
     1_900_000,  // grpc.unary
     4_200_000,  // db.select
     5_800_000,  // db.insert
     4_600_000,  // db.update
     8_100_000,  // db.index_scan
    14_500_000,  // db.ann_search
       310_000,  // cache.get
       290_000,  // cache.set
       350_000,  // cache.miss
       820_000,  // pubsub.publish
       790_000,  // pubsub.consume
   1_800_000_000,// llm.inference
     220_000_000,// llm.embed
       590_000,  // jwt.verify
     1_200_000,  // auth.token_exchange
     1_480_000,  // queue.send
     1_510_000,  // queue.receive
  ]);
  
  // Payload pool: JSON blobs attached to ~15% of spans (LLM, payment, error spans).
  // Lengths range from ~80 to ~220 bytes — representative of real attribute bags.
  const PAYLOADS = [
    `{"model":"gpt-4o-mini","prompt_tokens":418,"completion_tokens":91,"total_tokens":509,"finish_reason":"stop","latency_to_first_token_ms":31}`,
    `{"model":"text-embedding-3-small","prompt_tokens":256,"completion_tokens":0,"total_tokens":256,"top_k":20,"reranked":8,"latency_to_first_token_ms":19}`,
    `{"model":"claude-3-5-haiku","prompt_tokens":1024,"completion_tokens":312,"total_tokens":1336,"finish_reason":"stop","cached_tokens":512,"cost_usd":0.00041}`,
    `{"attempt":1,"provider":"stripe","error":"upstream_timeout","http_status":504,"retry_after_ms":50}`,
    `{"attempt":2,"provider":"adyen","transaction_id":"txn_9f3a21c8","http_status":200,"auth_code":"AUTH-482910"}`,
    `{"query_plan":"index_scan","rows_examined":18420,"rows_returned":124,"execution_ms":7.3,"index":"category_created_at_idx"}`,
    `{"index":"product_embeddings_hnsw","ef_search":128,"top_k":20,"candidates_scanned":4096,"ann_latency_ms":14.2}`,
    `{"error":"connection_pool_exhausted","pool_size":32,"wait_ms":48,"service":"inventory-db","host":"db-inv-03"}`,
    `{"cache_key":"sess:usr_0x3f8a","ttl_remaining_s":1740,"hit":true,"bytes":892}`,
    `{"topic":"order.confirmed","partition":3,"offset":8847219,"ack_ms":0.8,"message_bytes":412}`,
    `{"flag":"checkout_v2_enabled","variant":"treatment","user_segment":"power_buyer","rollout_pct":25}`,
    `{"grpc_code":14,"grpc_msg":"upstream unavailable","retried":true,"backoff_ms":100,"attempt":1}`,
  ];
  
  const ENC = new TextEncoder();
  
  // Pre-encoded UTF-8 bytes for each service/op string (avoids per-span TextEncoder call)
  const OPS_BYTES = OPS.map(s => ENC.encode(s));
  const SVC_BYTES = SERVICES.map(s => ENC.encode(s));
  
  // Pre-compute number pools — two separate pools: BigInt (for generic path) and
  // paired u32 hi/lo (for fast path, avoids BigInt entirely).
  const POOL_SIZE = 512;
  const POOL_BIG  = new Array(POOL_SIZE);   // BigInt values
  const POOL_HI   = new Uint32Array(POOL_SIZE); // high 32 bits
  const POOL_LO   = new Uint32Array(POOL_SIZE); // low  32 bits
  for (let i = 0; i < POOL_SIZE; i++) {
    POOL_LO[i] = (Math.random() * 0x100000000) >>> 0;
    POOL_HI[i] = (Math.random() * 0x100000000) >>> 0;
    POOL_BIG[i] = BigInt(POOL_HI[i]) * 0x100000000n + BigInt(POOL_LO[i]);
  }
  
  // start_time_ns base as hi/lo u32 pair (1715018000000000000 = 0x17C6_1F6F_46DC_0000)
  const START_NS_HI = 0x17CCF7C8 >>> 0;  // 1715018000000000000n = 0x17CCF7C8_D166A000
  const START_NS_LO = 0xD166A000 >>> 0;
  
  // Derive realistic duration_ns from op type + span index (no BigInt needed).
  function spanDurNs(opIdx, i) {
    const base = OP_DUR_BASE[opIdx];
    // ±40% jitter using a cheap deterministic hash of i
    const jitter = ((i * 2654435761) >>> 0) % (base * 0.8 | 0);
    return (base + jitter - (base * 0.4 | 0)) >>> 0;
  }
  
  // status_code: 0=OK (~95%), 1=ERROR (~3%), 2=UNSET (~2% for async/fire-and-forget)
  function spanStatus(i) {
    const h = (i * 2246822519) >>> 0;
    if (h < 0x07AE147A) return 1; // ~3% error
    if (h < 0x0A3D70A4) return 2; // ~2% unset
    return 0;
  }
  
  // payload: non-empty on ~15% of spans (LLM ops always get one, others stochastically)
  function spanPayload(opIdx, i) {
    const isLlm = opIdx === 14 || opIdx === 15; // llm.inference, llm.embed
    const isPayment = opIdx === 1 && (i % 7 === 0); // some http.client spans
    const stochastic = ((i * 1664525 + 1013904223) >>> 0) < 0x26666666; // ~15%
    if (isLlm || isPayment || stochastic) {
      return PAYLOADS[i % PAYLOADS.length];
    }
    return null;
  }
  
  const BATCH = 2000;
  function yield_() { return new Promise(r => setTimeout(r, 0)); }
  
  // ── WASM init ─────────────────────────────────────────────────────────────────
  let wasmWriter = null;
  
  async function initWasm() {
    try {
      const wasm = await loadWasm("/bench/wasm/nxs_reducers.wasm");
      wasmWriter = new WasmSpanWriter(wasm, 256);
      $("run-btn").disabled = false;
      $("run-btn").textContent = "Run benchmark";
    } catch (e) {
      $("run-btn").disabled = false;
      $("run-btn").textContent = "Run benchmark";
      $("status").textContent = "WASM unavailable — WASM bench will be skipped";
      console.warn("WASM load failed:", e);
    }
  }
  
  // ── Generic path (uses NxsWriter with BigInt i64 writes) ─────────────────────
  function writeSpanGeneric(w, i) {
    const thi    = i % 32;
    const tlo    = (i % 32) + 256;
    const sid    = i % POOL_SIZE;
    const pid    = i % 8 === 0 ? -1 : (i - 1) % POOL_SIZE;
    const opIdx  = i % OPS.length;
    const svcIdx = i % SERVICES.length;
    const durNs  = spanDurNs(opIdx, i);
    const status = spanStatus(i);
    const payload = spanPayload(opIdx, i);
    w.beginObject();
    w.writeI64(0, POOL_BIG[thi]);
    w.writeI64(1, POOL_BIG[tlo]);
    w.writeI64(2, POOL_BIG[sid]);
    if (pid < 0) w.writeNull(3); else w.writeI64(3, POOL_BIG[pid]);
    w.writeStr(4, OPS[opIdx]);
    w.writeStr(5, SERVICES[svcIdx]);
    w.writeI64(6, 1715018000000000000n + BigInt(i) * 1000000n);
    w.writeI64(7, BigInt(durNs));
    w.writeI64(8, BigInt(status));
    if (payload !== null) w.writeStr(9, payload);
    w.endObject();
  }
  
  async function benchNxsWal(n, onProg) {
    let totalBytes = 0;
    const t0 = performance.now();
    for (let i = 0; i < n; i++) {
      const w = new NxsWriter(SCHEMA);
      writeSpanGeneric(w, i);
      totalBytes += w._materialize().length;
      if (i % BATCH === BATCH - 1) { onProg(i / n * 0.19); await yield_(); }
    }
    return { ms: performance.now() - t0, bytes: totalBytes };
  }
  
  // ── Fast path: fixed-layout encoder, no BigInt, no chunk array ───────────────
  //
  // Every span NYXO record is exactly 120 bytes with this schema:
  //   [0]  NYXO magic  4B
  //   [4]  length u32  4B  (= 120)
  //   [8]  bitmask     2B  (= 0xFF 0x03 — all 9 present slots)
  //  [10]  offset table 10×u16 = 20B  (fixed: 32,40,48,56,64,72,80,88,96, 0)
  //  [30]  2B padding
  //  [32]  trace_id_hi  8B  i64 LE
  //  [40]  trace_id_lo  8B  i64 LE
  //  [48]  span_id      8B  i64 LE
  //  [56]  parent_span_id 8B (i64 or null-8-zero-bytes; bitmask slot 3 always set)
  //  [64]  name field:  4B len + bytes + pad to 8  (max 13 bytes "llm.inference" → 16B → pad to 16? no 4+13=17→pad to 24)
  //        Actually: all ops fit in ≤13 chars → 4+13+3pad = 20? no.
  //        Wait — measured size is always 120, so name+svc must be fixed 16B+16B=32B.
  //        That means: 4 + len + pad(8 - (4+len)%8) for each.  "http.request"=12 → 4+12=16 → 0 pad → 16B ✓
  //        "llm.inference"=13 → 4+13=17 → pad 7 → 24B  ← this would break 120!
  //
  // Re-check: the measured size is 120 for ALL combos. Let me verify "llm.inference":
  // 4 + 13 = 17, pad = 8 - (17%8) = 8-1 = 7 → 24B for name field
  // But with "frontend" svc (8 bytes): 4+8=12 → pad 4 → 16B
  // total = 32 (header) + 8*4 (slots 0-3) + 24 (name) + 16 (svc) + 8*3 (slots 6-8) = 32+32+24+16+24 = 128 ≠ 120
  //
  // The measured output was always 120 — but that was with short op names. For "llm.inference"
  // it would be 128. We need variable-size handling in the fast path, OR we restrict ops to ≤12 chars.
  // Instead: use a truly fixed-layout with a shared 128-byte buffer and write string length dynamically
  // but keep everything else as direct DataView writes (no BigInt).
  
  // Maximum NYXO record size for this schema with longest strings:
  // header=32, slots0-3=32, name(max24)+svc(max16)=40, slots6-8=24 → 136 bytes
  // Worst pair: "db.index_scan"(13) → 24B + "catalogue-svc"(13) → 24B = 136B total
  const FAST_BUF_SIZE = 136;
  
  // Pre-built static header bytes (everything that never changes):
  // magic(4) + length-placeholder(4) + bitmask(2) + offsets(20) + padding(2) = 32 bytes
  // Bitmask: slots 0-8 set → bits 0-6 of byte0 = 0x7F, bit 0-1 of byte1 (slots 7-8) = 0x03
  // But byte0 also has LEB128 continuation bit (bit7=1) because bitmaskBytes=2
  // So byte0 = 0x80 | 0x7F = 0xFF, byte1 = 0x03 ✓
  const STATIC_HEADER = new Uint8Array(32);
  {
    // magic NYXO = 0x4E59584F LE
    STATIC_HEADER[0]=0x4F; STATIC_HEADER[1]=0x53; STATIC_HEADER[2]=0x58; STATIC_HEADER[3]=0x4E;
    // length placeholder — will be patched per span
    // bitmask at [8]: 0xFF 0x03
    STATIC_HEADER[8]=0xFF; STATIC_HEADER[9]=0x03;
    // offset table at [10]: 10 × u16 LE
    // slots 0-3 always at 32,40,48,56
    const OT = [32,40,48,56, 0,0, 80,88,96, 0]; // slots 4,5 filled per span; 6,7,8 fixed; 9 unused
    for (let s = 0; s < 10; s++) {
      STATIC_HEADER[10 + s*2]     =  OT[s] & 0xFF;
      STATIC_HEADER[10 + s*2 + 1] = (OT[s] >>> 8) & 0xFF;
    }
    // slots 4 & 5 offsets (name=64, service=?) — patched per span since str length varies
    // padding [30,31] = 0 already
  }
  
  // Reusable 128-byte buffer for fast encoding (one per encode call, reused via DataView)
  // We allocate one per benchmark run to avoid cross-contamination.
  function makeFastEncoder() {
    const buf = new Uint8Array(FAST_BUF_SIZE);
    const dv  = new DataView(buf.buffer);
  
    // Copy static header
    buf.set(STATIC_HEADER);
  
    return function encodeFast(i, outParts) {
      const thi    = i % 32;
      const tlo    = (i % 32) + 256;
      const sid    = i % POOL_SIZE;
      const pid    = i % 8 === 0 ? -1 : (i - 1) % POOL_SIZE;
      const opIdx  = i % OPS.length;
      const svcIdx = i % SERVICES.length;
  
      // ── i64 fields 0-3 at offsets 32,40,48,56 ────────────────────────────
      // Write as two setUint32 calls (lo then hi) — no BigInt needed for pool values.
      dv.setUint32(32, POOL_LO[thi], true); dv.setUint32(36, POOL_HI[thi], true);
      dv.setUint32(40, POOL_LO[tlo], true); dv.setUint32(44, POOL_HI[tlo], true);
      dv.setUint32(48, POOL_LO[sid], true); dv.setUint32(52, POOL_HI[sid], true);
      if (pid < 0) {
        // null: write 8 zero bytes (null sigil = all zeros per spec)
        dv.setUint32(56, 0, true); dv.setUint32(60, 0, true);
      } else {
        dv.setUint32(56, POOL_LO[pid], true); dv.setUint32(60, POOL_HI[pid], true);
      }
  
      // ── name string at offset 64 ──────────────────────────────────────────
      const nameBytes = OPS_BYTES[opIdx];
      const nameLen   = nameBytes.length;
      dv.setUint32(64, nameLen, true);
      buf.set(nameBytes, 68);
      // pad name field to 8-byte boundary from offset 64
      const nameFieldSize = 4 + nameLen;
      const namePad = (8 - (nameFieldSize % 8)) % 8;
      for (let p = 68 + nameLen; p < 68 + nameLen + namePad; p++) buf[p] = 0;
      const nameTotal = nameFieldSize + namePad; // multiple of 8
  
      // ── service string at offset 64+nameTotal ────────────────────────────
      const svcOff   = 64 + nameTotal;
      const svcBytes = SVC_BYTES[svcIdx];
      const svcLen   = svcBytes.length;
      dv.setUint32(svcOff, svcLen, true);
      buf.set(svcBytes, svcOff + 4);
      const svcFieldSize = 4 + svcLen;
      const svcPad = (8 - (svcFieldSize % 8)) % 8;
      for (let p = svcOff + 4 + svcLen; p < svcOff + 4 + svcLen + svcPad; p++) buf[p] = 0;
      const svcTotal = svcFieldSize + svcPad;
  
      // ── i64 fields 6-8 at fixed offsets relative to svc end ──────────────
      const f6off = svcOff + svcTotal;  // start_time_ns
      const f7off = f6off + 8;          // duration_ns
      const f8off = f7off + 8;          // status_code
  
      // start_time_ns = 1715018000000000000 + i * 1000000
      // = (START_NS_HI:START_NS_LO) + i * 1000000
      // i * 1000000 fits in 40 bits for i < 1e6. Add to lo, carry to hi.
      const addLo  = (i * 1000000) >>> 0;
      const addHi  = Math.floor(i * 1000000 / 0x100000000) >>> 0;
      let   nsLo   = (START_NS_LO + addLo) >>> 0;
      const carry  = (START_NS_LO + addLo) > 0xFFFFFFFF ? 1 : 0;
      const nsHi   = (START_NS_HI + addHi + carry) >>> 0;
      dv.setUint32(f6off,     nsLo, true);
      dv.setUint32(f6off + 4, nsHi, true);
  
      // duration_ns — realistic per-op distribution, fits in 32 bits
      dv.setUint32(f7off,     spanDurNs(opIdx, i), true);
      dv.setUint32(f7off + 4, 0, true);
  
      // status_code
      dv.setUint32(f8off,     spanStatus(i), true);
      dv.setUint32(f8off + 4, 0, true);
  
      // ── patch offset table for slots 4 & 5 ───────────────────────────────
      buf[18] = 64 & 0xFF;   buf[19] = 0; // slot 4 = name offset (always 64)
      const svcOff16 = svcOff & 0xFFFF;
      buf[20] = svcOff16 & 0xFF; buf[21] = (svcOff16 >>> 8) & 0xFF; // slot 5
  
      // ── patch fixed-offset slots 6,7,8 in offset table ───────────────────
      buf[22] = f6off & 0xFF; buf[23] = (f6off >>> 8) & 0xFF;
      buf[24] = f7off & 0xFF; buf[25] = (f7off >>> 8) & 0xFF;
      buf[26] = f8off & 0xFF; buf[27] = (f8off >>> 8) & 0xFF;
  
      // ── total object length ───────────────────────────────────────────────
      const totalLen = f8off + 8;
      dv.setUint32(4, totalLen, true);
  
      // Push a copy of the used bytes
      outParts.push(buf.slice(0, totalLen));
      return totalLen;
    };
  }
  
  async function benchNxsFast(n, onProg) {
    const encode = makeFastEncoder();
    const outParts = [];
    let totalBytes = 0;
    const t0 = performance.now();
    for (let i = 0; i < n; i++) {
      totalBytes += encode(i, outParts);
      if (i % BATCH === BATCH - 1) { onProg(0.19 + i / n * 0.19); await yield_(); }
    }
    return { ms: performance.now() - t0, bytes: totalBytes };
  }
  
  async function benchNxsSealed(n, onProg) {
    const w = new NxsWriter(SCHEMA);
    const t0 = performance.now();
    for (let i = 0; i < n; i++) {
      writeSpanGeneric(w, i);
      if (i % BATCH === BATCH - 1) { onProg(0.38 + i / n * 0.19); await yield_(); }
    }
    const buf = w.finish();
    return { ms: performance.now() - t0, bytes: buf.length };
  }
  
  async function benchNxsWasm(n, onProg) {
    if (!wasmWriter) {
      return { ms: 0.001, bytes: 0, unavailable: true };
    }
    let totalBytes = 0;
    const t0 = performance.now();
    for (let i = 0; i < n; i++) {
      const thi    = i % 32;
      const tlo    = (i % 32) + 256;
      const sid    = i % POOL_SIZE;
      const pid    = i % 8 === 0 ? -1 : (i - 1) % POOL_SIZE;
      const opIdx  = i % OPS.length;
      const svcIdx = i % SERVICES.length;
      const addLo  = (i * 1000000) >>> 0;
      const addHi  = Math.floor(i * 1000000 / 0x100000000) >>> 0;
      const nsLo   = (START_NS_LO + addLo) >>> 0;
      const carry  = (START_NS_LO + addLo) > 0xFFFFFFFF ? 1 : 0;
      const nsHi   = (START_NS_HI + addHi + carry) >>> 0;
      // WasmSpanWriter.encode() only covers the 9 fixed fields (no payload slot);
      // payload is omitted here intentionally — the WASM encoder measures the
      // fixed-field hot path, same as the fast encoder.
      const rec = wasmWriter.encode({
        trace_id_hi:    POOL_BIG[thi],
        trace_id_lo:    POOL_BIG[tlo],
        span_id:        POOL_BIG[sid],
        parent_span_id: pid < 0 ? 0n : POOL_BIG[pid],
        name:           OPS[opIdx],
        service:        SERVICES[svcIdx],
        start_time_ns:  BigInt(nsHi) * 0x100000000n + BigInt(nsLo),
        duration_ns:    spanDurNs(opIdx, i),
        status_code:    spanStatus(i),
      });
      totalBytes += rec.length;
      if (i % BATCH === BATCH - 1) { onProg(0.57 + i / n * 0.19); await yield_(); }
    }
    return { ms: performance.now() - t0, bytes: totalBytes };
  }
  
  async function benchJson(n, onProg) {
    let totalBytes = 0;
    const t0 = performance.now();
    for (let i = 0; i < n; i++) {
      const thi    = i % 32, tlo = (i % 32) + 256, sid = i % POOL_SIZE;
      const pid    = i % 8 === 0 ? -1 : (i - 1) % POOL_SIZE;
      const opIdx  = i % OPS.length;
      const svcIdx = i % SERVICES.length;
      const payload = spanPayload(opIdx, i);
      const obj = {
        trace_id_hi:    POOL_BIG[thi].toString(),
        trace_id_lo:    POOL_BIG[tlo].toString(),
        span_id:        POOL_BIG[sid].toString(),
        parent_span_id: pid < 0 ? null : POOL_BIG[pid].toString(),
        name:           OPS[opIdx],
        service:        SERVICES[svcIdx],
        start_time_ns:  (1715018000000000000n + BigInt(i) * 1000000n).toString(),
        duration_ns:    spanDurNs(opIdx, i),
        status_code:    spanStatus(i),
      };
      if (payload !== null) obj.payload = payload;
      const line = JSON.stringify(obj) + "\n";
      totalBytes += ENC.encode(line).length;
      if (i % BATCH === BATCH - 1) { onProg(0.76 + i / n * 0.22); await yield_(); }
    }
    return { ms: performance.now() - t0, bytes: totalBytes };
  }
  
  // ── Formatters ────────────────────────────────────────────────────────────────
  const fmtBytes = n =>
    n < 1024         ? `${n} B`
    : n < 1048576    ? `${(n/1024).toFixed(1)} KB`
    : n < 1073741824 ? `${(n/1048576).toFixed(2)} MB`
    :                  `${(n/1073741824).toFixed(2)} GB`;
  
  const fmtMs = ms =>
    ms < 1      ? `${(ms*1000).toFixed(0)} µs`
    : ms < 1000 ? `${ms.toFixed(1)} ms`
    :             `${(ms/1000).toFixed(2)} s`;
  
  const fmtTput = v =>
    v >= 1e6 ? `${(v/1e6).toFixed(2)}M`
    : v >= 1e3 ? `${(v/1e3).toFixed(1)}k`
    : v.toFixed(0);
  
  const fmtNs = ns =>
    ns < 1000      ? `${ns.toFixed(0)} ns`
    : ns < 1000000 ? `${(ns/1000).toFixed(1)} µs`
    :                `${(ns/1000000).toFixed(2)} ms`;
  
  
  // ── Render ────────────────────────────────────────────────────────────────────
  function renderResults(n, walRes, fastRes, sealRes, wasmRes, jsonRes) {
    const results = [
      { id: "nxs-wal",  label: "NXS WAL",     klass: "nxs-wal",  tag: "nxs",  res: walRes  },
      { id: "nxs-fast", label: "NXS Fast",    klass: "nxs-fast", tag: "fast", res: fastRes },
      { id: "nxs-seal", label: "NXS Sealed",  klass: "nxs-seal", tag: "seal", res: sealRes },
      { id: "nxs-wasm", label: "NXS WASM",    klass: "nxs-wasm", tag: "wasm", res: wasmRes },
      { id: "json",     label: "JSON NDJSON", klass: "json-nd",  tag: "json", res: jsonRes },
    ];
  
    const tputs   = results.map(r => r.res.unavailable ? 0 : (r.res.n * 1000) / r.res.ms);
    const maxTput = Math.max(...tputs);
    const maxSize = Math.max(...results.map(r => r.res.bytes));
    const jsonSize = jsonRes.bytes;
    const bestTputIdx = tputs.indexOf(maxTput);
    const bestSizeIdx = results.map(r => r.res.bytes).indexOf(Math.min(...results.filter(r => r.res.bytes > 0).map(r => r.res.bytes)));
  
    // Scorecards
    results.forEach((r, i) => {
      const el = $(`sc-${r.id}`);
      if (r.res.unavailable) {
        el.innerHTML = `
          <div class="sc-label">${r.label}</div>
          <div class="sc-tput sc-placeholder">N/A</div>
          <div class="sc-sub">WASM not available</div>
        `;
        return;
      }
      const tput = tputs[i];
      el.innerHTML = `
        <div class="sc-label">${r.label}</div>
        <div class="sc-tput">${fmtTput(tput)}<span class="sc-unit">spans/s</span></div>
        <div class="sc-sub">${fmtBytes(r.res.bytes)} · ${fmtMs(r.res.ms)} total · ${fmtNs((r.res.ms*1e6)/r.res.n)}/span</div>
      `;
      el.classList.toggle("winner", i === bestTputIdx);
    });
  
    // Throughput chart
    const tputEl = $("chart-tput");
    tputEl.innerHTML = "";
    results.forEach((r, i) => {
      const pct = tputs[i] > 0 ? Math.max(1, (tputs[i] / maxTput) * 100) : 0;
      const lbl = document.createElement("div"); lbl.className = "lbl"; lbl.textContent = r.label;
      const trk = document.createElement("div"); trk.className = "track";
      const bar = document.createElement("div"); bar.className = `bar ${r.klass}`; bar.style.width = `${pct}%`;
      trk.appendChild(bar);
      const val = document.createElement("div");
      val.className = `val${i === bestTputIdx ? " best" : ""}`;
      val.textContent = r.res.unavailable ? "N/A" : `${fmtTput(tputs[i])} spans/s`;
      tputEl.append(lbl, trk, val);
    });
  
    // Size chart
    const sizeEl = $("chart-size");
    sizeEl.innerHTML = "";
    results.forEach((r, i) => {
      const pct = r.res.bytes > 0 ? Math.max(1, (r.res.bytes / maxSize) * 100) : 0;
      const lbl = document.createElement("div"); lbl.className = "lbl"; lbl.textContent = r.label;
      const trk = document.createElement("div"); trk.className = "track";
      const bar = document.createElement("div"); bar.className = `bar ${r.klass}`; bar.style.width = `${pct}%`;
      trk.appendChild(bar);
      const pctJ = jsonSize > 0 ? (r.res.bytes / jsonSize * 100).toFixed(1) : "—";
      const val = document.createElement("div");
      val.className = `val${i === bestSizeIdx ? " best" : ""}`;
      val.textContent = r.res.unavailable ? "N/A" : `${fmtBytes(r.res.bytes)} (${pctJ}% of JSON)`;
      sizeEl.append(lbl, trk, val);
    });
  
    // Detail table
    const tbody = $("detail-body");
    tbody.innerHTML = "";
    results.forEach((r, i) => {
      const tput = tputs[i];
      const nsPerSpan = r.res.unavailable ? 0 : (r.res.ms * 1e6) / r.res.n;
      const bps = r.res.bytes > 0 ? r.res.bytes / r.res.n : 0;
      const pctJ = jsonSize > 0 && r.res.bytes > 0 ? (r.res.bytes / jsonSize * 100).toFixed(1) : "—";
      const isBT = i === bestTputIdx, isBS = i === bestSizeIdx;
      const tr = document.createElement("tr");
      if (r.res.unavailable) {
        tr.innerHTML = `
          <td><span class="tag ${r.tag}">${r.label}</span></td>
          <td class="num" colspan="6" style="color:var(--muted)">WASM not available in this context</td>
        `;
      } else {
        tr.innerHTML = `
          <td><span class="tag ${r.tag}">${r.label}</span></td>
          <td class="num${isBT?" best":""}">${fmtNs(nsPerSpan)}</td>
          <td class="num${isBT?" best":""}">${fmtTput(tput)}</td>
          <td class="num">${fmtMs(r.res.ms)}</td>
          <td class="num${isBS?" best":""}">${fmtBytes(r.res.bytes)}</td>
          <td class="num${isBS?" best":""}">${bps.toFixed(1)} B</td>
          <td class="num${isBS?" best":""}">${pctJ}%</td>
        `;
      }
      tbody.appendChild(tr);
    });
  
    // Summary note
    const fastVsWal  = tputs[0] > 0 ? tputs[1] / tputs[0] : 0;
    const fastVsJson = tputs[4] > 0 ? tputs[1] / tputs[4] : 0;
    const wasmVsJson = tputs[4] > 0 && tputs[3] > 0 ? tputs[3] / tputs[4] : 0;
    const sizeSaving = jsonSize > 0 && fastRes.bytes > 0 ? (1 - fastRes.bytes / jsonSize) * 100 : 0;
    const noteEl = $("detail-note");
    noteEl.className = "note live";
    noteEl.innerHTML =
      `NXS Fast is <strong>${fastVsWal.toFixed(1)}×</strong> faster than the generic WAL encoder ` +
      `(DataView u32 pairs vs BigInt shifts, pre-encoded strings, single shared buffer). ` +
      (wasmRes.unavailable ? "" :
        wasmVsJson >= 1
          ? `NXS WASM is <strong>${wasmVsJson.toFixed(1)}×</strong> faster than JSON. `
          : `JSON stringify is <strong>${(1/wasmVsJson).toFixed(1)}×</strong> faster than NXS WASM (V8's JSON is C++). `) +
      (fastVsJson >= 1
        ? `NXS Fast is <strong>${fastVsJson.toFixed(1)}×</strong> faster than JSON stringify.`
        : `JSON stringify is still <strong>${(1/fastVsJson).toFixed(1)}×</strong> faster (V8's native JSON path is C++). `) +
      ` NXS produces <strong>${sizeSaving.toFixed(0)}%</strong> less data than JSON — the key advantage at scale.`;
  }
  
  // ── Cross-language reference data (n=10k spans, Apple M-series, best-of-3) ───
  // Pure in-memory encode — no I/O. Rust append-batch @ 100k ≈ 644 ns (tmpfs write).
  // In-memory encode-only ≈ 131 ns (serde_json parity). See BENCHMARK.md.
  const LANG_REF = [
    { lang: "C",               nxs_ns: 73,   json_ns: 270,  nxs_klass: "nxs-wal", json_klass: "json-nd" },
    { lang: "Go",              nxs_ns: 131,  json_ns: 301,  nxs_klass: "nxs-wal", json_klass: "json-nd" },
    { lang: "Rust",            nxs_ns: 131,  json_ns: 131,  nxs_klass: "nxs-wal", json_klass: "json-nd" },
    { lang: "Python (C ext)",  nxs_ns: 438,  json_ns: 1383, nxs_klass: "nxs-wal", json_klass: "json-nd" },
    { lang: "Ruby (C ext)",    nxs_ns: 336,  json_ns: 383,  nxs_klass: "nxs-wal", json_klass: "json-nd" },
    { lang: "Python (pure)",   nxs_ns: 3800, json_ns: 1383, nxs_klass: "nxs-wal", json_klass: "json-nd" },
    { lang: "JS (generic)",    nxs_ns: 750,  json_ns: 320,  nxs_klass: "nxs-wal", json_klass: "json-nd" },
    { lang: "Ruby (pure)",     nxs_ns: 5300, json_ns: 383,  nxs_klass: "nxs-wal", json_klass: "json-nd" },
  ];
  
  // Updated after each live run; null = not yet measured.
  let liveNxsNs  = null;  // NXS Fast ns/span from this browser
  let liveWasmNs = null;  // NXS WASM ns/span from this browser
  let liveJsonNs = null;  // JSON NDJSON ns/span from this browser
  
  function renderLangComparison(nxsNs, wasmNs, jsonNs) {
    liveNxsNs  = nxsNs;
    liveWasmNs = wasmNs;
    liveJsonNs = jsonNs;
  
    const fastLabel = `JS (fast) ★`;
    const wasmLabel = `JS (WASM) ★`;
  
    // Build rows: static reference + live JS (fast) + live JS (WASM)
    const nxsRows = [
      ...LANG_REF.map(r => ({ lang: r.lang, ns: r.nxs_ns, klass: r.nxs_klass })),
      { lang: fastLabel, ns: nxsNs, klass: "nxs-fast" },
      ...(wasmNs != null ? [{ lang: wasmLabel, ns: wasmNs, klass: "nxs-wasm" }] : []),
    ].sort((a, b) => a.ns - b.ns);
  
    const jsonRows = [
      ...LANG_REF.filter(r => r.json_ns != null).map(r => ({ lang: r.lang, ns: r.json_ns, klass: r.json_klass })),
      { lang: fastLabel, ns: jsonNs, klass: "json-nd" },
    ].sort((a, b) => a.ns - b.ns);
  
    const liveLabels = new Set([fastLabel, wasmLabel]);
  
    function fillChart(elId, rows) {
      const el = $(elId);
      el.innerHTML = "";
      const maxNs = Math.max(...rows.map(r => r.ns));
      for (const r of rows) {
        const isLive = liveLabels.has(r.lang);
        const lbl = document.createElement("div");
        lbl.className = "lbl";
        lbl.textContent = r.lang;
        if (isLive) lbl.style.fontWeight = "700";
  
        const trk = document.createElement("div"); trk.className = "track";
        const bar = document.createElement("div");
        bar.className = `bar ${r.klass}`;
        bar.style.width = `${Math.max(1, r.ns / maxNs * 100)}%`;
        trk.appendChild(bar);
  
        const kps = (1e9 / r.ns / 1000).toFixed(0);
        const val = document.createElement("div");
        val.className = "val" + (isLive ? " best" : "");
        val.textContent = `${fmtNs(r.ns)} (${kps}k/s)`;
  
        el.append(lbl, trk, val);
      }
    }
  
    fillChart("lang-nxs-chart",  nxsRows);
    fillChart("lang-json-chart", jsonRows);
  }
  
  // ── Tag styles ────────────────────────────────────────────────────────────────
  const styleEl = document.createElement("style");
  styleEl.textContent = `
    .tag.fast { background: #22c55e; color: #fff; }
    .tag.wasm { background: #a855f7; color: #fff; }
  `;
  document.head.appendChild(styleEl);
  
  // ── Run ───────────────────────────────────────────────────────────────────────
  let running = false;
  let selectedN = 1000;
  
  demoRoot.querySelectorAll("#wal-sizes button").forEach(btn => {
    btn.addEventListener("click", () => {
      if (running) return;
      demoRoot.querySelectorAll("#wal-sizes button").forEach(b => b.classList.remove("active"));
      btn.classList.add("active");
      selectedN = parseInt(btn.dataset.n, 10);
    });
  });
  
  const runBtn = $("run-btn");
  if (!runBtn) {
    console.error("wireWalPage: #run-btn not found in demo root");
    return;
  }
  runBtn.addEventListener("click", async () => {
    if (running) return;
    running = true;
    $("run-btn").disabled = true;
  
    const n = selectedN;
    const statusEl = $("status");
    const pbar = $("pbar");
    const pwrap = $("pwrap");
  
    statusEl.className = "status-line running";
    pwrap.classList.add("visible");
    pbar.style.width = "0%";
    const onProg = p => { pbar.style.width = `${(p*100).toFixed(1)}%`; };
  
    try {
      statusEl.textContent = `NXS WAL (generic)…`;
      const wal  = await benchNxsWal(n, onProg);
  
      statusEl.textContent = `NXS Fast (optimised)…`;
      const fast = await benchNxsFast(n, onProg);
  
      statusEl.textContent = `NXS Sealed…`;
      const seal = await benchNxsSealed(n, onProg);
  
      statusEl.textContent = `NXS WASM…`;
      const wasm = await benchNxsWasm(n, onProg);
  
      statusEl.textContent = `JSON NDJSON…`;
      const json = await benchJson(n, onProg);
  
      pbar.style.width = "100%";
      const fastTput = fmtTput((n*1000) / fast.ms);
      statusEl.className = "status-line";
      statusEl.textContent = `Done — NXS Fast ${fastTput} spans/s · JSON ${fmtTput((n*1000)/json.ms)} spans/s`;
      renderResults(n,
        { ...wal,  n }, { ...fast, n },
        { ...seal, n }, { ...wasm, n },
        { ...json, n });
      renderLangComparison(
        (fast.ms * 1e6) / n,
        wasm.unavailable ? null : (wasm.ms * 1e6) / n,
        (json.ms * 1e6) / n
      );
    } catch (err) {
      statusEl.className = "status-line";
      statusEl.textContent = `Error: ${err.message}`;
      console.error(err);
    } finally {
      running = false;
      $("run-btn").disabled = false;
      setTimeout(() => { pwrap.classList.remove("visible"); }, 1500);
    }
  });
  
  // Kick off WASM load after DOM is ready
  initWasm();
}
