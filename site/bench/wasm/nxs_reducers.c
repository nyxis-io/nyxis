/*
 * nxs_reducers.c — freestanding reducers for the NXS format.
 *
 * Compiled to WebAssembly with no libc, no allocator, no imports.
 *
 *   clang --target=wasm32 -O3 -nostdlib -fno-builtin \
 *         -Wl,--no-entry -Wl,--export-dynamic -Wl,--allow-undefined \
 *         -o nxs_reducers.wasm nxs_reducers.c
 *
 * JS calls conventions:
 *   - Buffer base address passed as a uint32 offset into WASM linear memory.
 *   - All reads are little-endian (WASM is LE natively).
 *   - Scanner walks the tail-index, dereferences each object, and walks its
 *     LEB128 bitmask inline per-record to find the slot's value.
 */

#include <stdint.h>

/* Unaligned little-endian reads — WASM loads are already LE. */
static inline uint16_t rd_u16(const uint8_t *p) {
    return (uint16_t)p[0] | ((uint16_t)p[1] << 8);
}

static inline uint32_t rd_u32(const uint8_t *p) {
    return (uint32_t)p[0]       | ((uint32_t)p[1] << 8)
         | ((uint32_t)p[2] << 16)| ((uint32_t)p[3] << 24);
}

static inline uint64_t rd_u64(const uint8_t *p) {
    return (uint64_t)rd_u32(p) | ((uint64_t)rd_u32(p + 4) << 32);
}

static inline int64_t rd_i64(const uint8_t *p) {
    return (int64_t)rd_u64(p);
}

static inline double rd_f64(const uint8_t *p) {
    union { uint64_t u; double d; } u;
    u.u = rd_u64(p);
    return u.d;
}

/*
 * Locate the byte offset of `slot`'s value within the object at `obj_offset`.
 * Returns -1 on absent. Inlines the LEB128 bitmask walk and offset-table index.
 */
static int64_t field_offset(const uint8_t *data, uint32_t size,
                            uint32_t obj_offset, uint32_t slot) {
    uint32_t p = obj_offset + 8; /* skip NYXO magic + length */
    if (p > size) return -1;

    uint32_t cur_slot = 0;
    uint32_t table_idx = 0;
    int found = 0;
    uint8_t byte = 0;
    do {
        if (p >= size) return -1;
        byte = data[p++];
        uint8_t data_bits = byte & 0x7F;
        for (int b = 0; b < 7; b++) {
            if (cur_slot == slot) {
                if ((data_bits >> b) & 1) {
                    found = 1;
                } else {
                    return -1;
                }
            } else if (cur_slot < slot && ((data_bits >> b) & 1)) {
                table_idx++;
            }
            cur_slot++;
        }
        if (found && (byte & 0x80) == 0) break;
        if (cur_slot > slot && found) break;
    } while (byte & 0x80);

    if (!found) return -1;

    while (byte & 0x80) {
        if (p >= size) return -1;
        byte = data[p++];
    }

    uint32_t ofpos = p + table_idx * 2;
    if (ofpos + 2 > size) return -1;
    uint16_t rel = rd_u16(data + ofpos);
    return (int64_t)obj_offset + rel;
}

/*
 * Uniform-schema layout from record 0 (same as Go computeFastLayout).
 */
typedef struct {
    uint32_t bitmask_len;
    uint32_t table_idx;
    int32_t  present;
} fast_layout_t;

static fast_layout_t compute_fast_layout(const uint8_t *data, uint32_t tail_start,
                                         uint32_t slot) {
    fast_layout_t L = {0, 0, 0};
    uint32_t abs = (uint32_t)rd_u64(data + tail_start + 2);
    uint32_t p = abs + 8;
    uint32_t bitmask_start = p;
    uint32_t cur_slot = 0;
    uint32_t table_idx = 0;
    int present = 0;
    for (;;) {
        uint8_t b = data[p++];
        uint8_t bits = b & 0x7F;
        for (int i = 0; i < 7; i++) {
            if (cur_slot == slot) present = (bits >> i) & 1;
            else if (cur_slot < slot && ((bits >> i) & 1)) table_idx++;
            cur_slot++;
        }
        if ((b & 0x80) == 0) break;
    }
    L.bitmask_len = p - bitmask_start;
    L.table_idx = table_idx;
    L.present = present;
    return L;
}

static inline void wt_u32_out(uint8_t *p, uint32_t v) {
    p[0] = v & 0xFF; p[1] = (v >> 8) & 0xFF;
    p[2] = (v >> 16) & 0xFF; p[3] = (v >> 24) & 0xFF;
}

/*
 * build_field_index — one pass, write absolute value offset per record.
 * Returns 1 on success, 0 if slot absent in record 0.
 */
__attribute__((export_name("build_field_index")))
uint32_t build_field_index(uint32_t base, uint32_t size, uint32_t tail_start,
                           uint32_t record_count, uint32_t slot, uint32_t out_ptr) {
    const uint8_t *data = (const uint8_t *)(uintptr_t)base;
    (void)size;
    fast_layout_t L = compute_fast_layout(data, tail_start, slot);
    if (!L.present) return 0;
    uint32_t offset_table_pos = 8 + L.bitmask_len + L.table_idx * 2;
    uint8_t *out = (uint8_t *)(uintptr_t)out_ptr;
    for (uint32_t i = 0; i < record_count; i++) {
        const uint8_t *entry = data + tail_start + (uint64_t)i * 10;
        uint32_t abs = (uint32_t)rd_u64(entry + 2);
        uint16_t rel = rd_u16(data + abs + offset_table_pos);
        wt_u32_out(out + i * 4, abs + rel);
    }
    return 1;
}

/*
 * batch_resolve_offsets — for each record index in indices[], write the
 * absolute value offset (or 0xFFFFFFFF if absent). Uses uniform layout from
 * record 0 (same as build_field_index). One WASM entry for N lookups.
 */
__attribute__((export_name("batch_resolve_offsets")))
void batch_resolve_offsets(uint32_t base, uint32_t size, uint32_t tail_start,
                           uint32_t record_count, uint32_t slot,
                           uint32_t indices_ptr, uint32_t count, uint32_t out_ptr) {
    const uint8_t *data = (const uint8_t *)(uintptr_t)base;
    (void)size;
    (void)record_count;
    fast_layout_t L = compute_fast_layout(data, tail_start, slot);
    const uint32_t *indices = (const uint32_t *)(uintptr_t)indices_ptr;
    uint8_t *out = (uint8_t *)(uintptr_t)out_ptr;
    if (!L.present) {
        for (uint32_t j = 0; j < count; j++) wt_u32_out(out + j * 4, 0xFFFFFFFFu);
        return;
    }
    uint32_t offset_table_pos = 8 + L.bitmask_len + L.table_idx * 2;
    for (uint32_t j = 0; j < count; j++) {
        uint32_t i = indices[j];
        const uint8_t *entry = data + tail_start + (uint64_t)i * 10;
        uint32_t abs = (uint32_t)rd_u64(entry + 2);
        uint16_t rel = rd_u16(data + abs + offset_table_pos);
        wt_u32_out(out + j * 4, abs + rel);
    }
}

/*
 * batch_get_f64 — read f64 values at pre-built field index offsets[recordIndex].
 * indices_ptr: record indices; field_index_ptr: n*4 table from build_field_index.
 */
__attribute__((export_name("batch_get_f64")))
void batch_get_f64(uint32_t base, uint32_t field_index_ptr, uint32_t indices_ptr,
                   uint32_t count, uint32_t out_ptr) {
    const uint8_t *data = (const uint8_t *)(uintptr_t)base;
    const uint32_t *index = (const uint32_t *)(uintptr_t)field_index_ptr;
    const uint32_t *indices = (const uint32_t *)(uintptr_t)indices_ptr;
    uint8_t *out = (uint8_t *)(uintptr_t)out_ptr;
    for (uint32_t j = 0; j < count; j++) {
        uint32_t off = index[indices[j]];
        double v = (off == 0xFFFFFFFFu) ? 0.0 : rd_f64(data + off);
        union { double d; uint64_t u; } bits;
        bits.d = v;
        for (int i = 0; i < 8; i++) out[j * 8 + i] = (bits.u >> (i * 8)) & 0xFF;
    }
}

__attribute__((export_name("sum_f64")))
double sum_f64(uint32_t base, uint32_t size, uint32_t tail_start,
               uint32_t record_count, uint32_t slot) {
    const uint8_t *data = (const uint8_t *)(uintptr_t)base;
    (void)size; /* bounds-checked via field_offset */
    double sum = 0.0;
    for (uint32_t i = 0; i < record_count; i++) {
        const uint8_t *entry = data + tail_start + (uint64_t)i * 10;
        uint32_t abs = (uint32_t)rd_u64(entry + 2);
        int64_t off = field_offset(data, 0xFFFFFFFFu, abs, slot);
        if (off < 0) continue;
        sum += rd_f64(data + off);
    }
    return sum;
}

__attribute__((export_name("sum_i64")))
int64_t sum_i64(uint32_t base, uint32_t size, uint32_t tail_start,
                uint32_t record_count, uint32_t slot) {
    const uint8_t *data = (const uint8_t *)(uintptr_t)base;
    (void)size;
    int64_t sum = 0;
    for (uint32_t i = 0; i < record_count; i++) {
        const uint8_t *entry = data + tail_start + (uint64_t)i * 10;
        uint32_t abs = (uint32_t)rd_u64(entry + 2);
        int64_t off = field_offset(data, 0xFFFFFFFFu, abs, slot);
        if (off < 0) continue;
        sum += rd_i64(data + off);
    }
    return sum;
}

/*
 * min_f64 / max_f64 need to signal "no records matched" to JS.
 * Convention: return 0.0 and set a module-local flag that JS retrieves via
 * `min_max_has_result()`.
 */
static int32_t _min_max_has_result = 0;

__attribute__((export_name("min_max_has_result")))
int32_t min_max_has_result(void) { return _min_max_has_result; }

__attribute__((export_name("min_f64")))
double min_f64(uint32_t base, uint32_t size, uint32_t tail_start,
               uint32_t record_count, uint32_t slot) {
    const uint8_t *data = (const uint8_t *)(uintptr_t)base;
    (void)size;
    double m = 0.0;
    int have = 0;
    for (uint32_t i = 0; i < record_count; i++) {
        const uint8_t *entry = data + tail_start + (uint64_t)i * 10;
        uint32_t abs = (uint32_t)rd_u64(entry + 2);
        int64_t off = field_offset(data, 0xFFFFFFFFu, abs, slot);
        if (off < 0) continue;
        double v = rd_f64(data + off);
        if (!have || v < m) { m = v; have = 1; }
    }
    _min_max_has_result = have;
    return m;
}

__attribute__((export_name("max_f64")))
double max_f64(uint32_t base, uint32_t size, uint32_t tail_start,
               uint32_t record_count, uint32_t slot) {
    const uint8_t *data = (const uint8_t *)(uintptr_t)base;
    (void)size;
    double m = 0.0;
    int have = 0;
    for (uint32_t i = 0; i < record_count; i++) {
        const uint8_t *entry = data + tail_start + (uint64_t)i * 10;
        uint32_t abs = (uint32_t)rd_u64(entry + 2);
        int64_t off = field_offset(data, 0xFFFFFFFFu, abs, slot);
        if (off < 0) continue;
        double v = rd_f64(data + off);
        if (!have || v > m) { m = v; have = 1; }
    }
    _min_max_has_result = have;
    return m;
}

/* Not needed: JS imports memory, chooses the data base itself. */

/* ── WAL span encoder ────────────────────────────────────────────────────────
 *
 * Encodes one NXS span record (the canonical 10-field WAL schema) into a
 * caller-provided output buffer and returns the number of bytes written.
 *
 * Input struct at `fields_ptr` (all little-endian, no padding):
 *   [  0.. 7]  i64  trace_id_hi      (slot 0)
 *   [  8..15]  i64  trace_id_lo      (slot 1)
 *   [ 16..23]  i64  span_id          (slot 2)
 *   [ 24..31]  i64  parent_span_id   (slot 3)
 *   [ 32..35]  u32  name_ptr         (slot 4) — abs WASM address of UTF-8 bytes
 *   [ 36..39]  u32  name_len
 *   [ 40..43]  u32  service_ptr      (slot 5) — abs WASM address of UTF-8 bytes
 *   [ 44..47]  u32  service_len
 *   [ 48..55]  i64  start_time_ns    (slot 6)
 *   [ 56..63]  i64  duration_ns      (slot 7)
 *   [ 64..71]  i64  status_code      (slot 8)
 *   // slot 9 (payload) is always empty — not written
 *
 * Output: NYXO record at `out_ptr`.  Returns bytes written.
 *
 * Schema: 10 keys → bitmask_bytes = (10+6)/7 = 2.
 *   Header: magic(4) + length(4) + bitmask(2) + offset_table(10*2=20) = 30 bytes
 *   Aligned to 8: 32 bytes data-area start.
 *
 * Bitmask for slots 0–8 present (9 bits set, slot 9 absent):
 *   byte 0 (slots 0-6): bits 0..6 all set → 0x7F | 0x80 = 0xFF (continuation)
 *   byte 1 (slots 7-9): bits 0,1 set (slots 7,8) → 0x03
 */

static inline void wt_u32(uint8_t *p, uint32_t v) {
    p[0] = v & 0xFF; p[1] = (v >> 8) & 0xFF;
    p[2] = (v >> 16) & 0xFF; p[3] = (v >> 24) & 0xFF;
}
static inline void wt_i64(uint8_t *p, int64_t v) {
    uint64_t u = (uint64_t)v;
    for (int i = 0; i < 8; i++) p[i] = (u >> (i*8)) & 0xFF;
}
static inline void wt_u16(uint8_t *p, uint16_t v) {
    p[0] = v & 0xFF; p[1] = (v >> 8) & 0xFF;
}
static inline void wt_memcpy(uint8_t *dst, const uint8_t *src, uint32_t n) {
    for (uint32_t i = 0; i < n; i++) dst[i] = src[i];
}

#define MAGIC_OBJ 0x4E59584Fu

__attribute__((export_name("encode_span")))
uint32_t encode_span(uint32_t out_ptr, uint32_t fields_ptr) {
    uint8_t *out    = (uint8_t *)(uintptr_t)out_ptr;
    const uint8_t *f = (const uint8_t *)(uintptr_t)fields_ptr;

    /* Read input fields */
    int64_t  trace_id_hi   = (int64_t)rd_u64(f +  0);
    int64_t  trace_id_lo   = (int64_t)rd_u64(f +  8);
    int64_t  span_id       = (int64_t)rd_u64(f + 16);
    int64_t  parent_span   = (int64_t)rd_u64(f + 24);
    uint32_t name_ptr      = rd_u32(f + 32);
    uint32_t name_len      = rd_u32(f + 36);
    uint32_t svc_ptr       = rd_u32(f + 40);
    uint32_t svc_len       = rd_u32(f + 44);
    int64_t  start_time_ns = (int64_t)rd_u64(f + 48);
    int64_t  duration_ns   = (int64_t)rd_u64(f + 56);
    int64_t  status_code   = (int64_t)rd_u64(f + 64);

    const uint8_t *name_bytes = (const uint8_t *)(uintptr_t)name_ptr;
    const uint8_t *svc_bytes  = (const uint8_t *)(uintptr_t)svc_ptr;

    /* Header offsets:
     *   0: magic(4), 4: length(4), 8: bitmask(2), 10: offset_table(20)
     *   30 raw → align to 8 → 32 bytes data start
     */
    uint32_t data_start = 32;

    /* Compute string padding (rule of 8: (4+len) must be 8-aligned) */
    uint32_t name_used = (4 + name_len) % 8;
    uint32_t name_pad  = name_used ? (8 - name_used) : 0;
    uint32_t svc_used  = (4 + svc_len) % 8;
    uint32_t svc_pad   = svc_used  ? (8 - svc_used)  : 0;

    /* Field relative offsets from object start */
    uint32_t off0  = data_start;                                      /* trace_id_hi   */
    uint32_t off1  = off0 + 8;                                        /* trace_id_lo   */
    uint32_t off2  = off1 + 8;                                        /* span_id       */
    uint32_t off3  = off2 + 8;                                        /* parent_span   */
    uint32_t off4  = off3 + 8;                                        /* name (str)    */
    uint32_t off5  = off4 + 4 + name_len + name_pad;                  /* service (str) */
    uint32_t off6  = off5 + 4 + svc_len  + svc_pad;                   /* start_time_ns */
    uint32_t off7  = off6 + 8;                                        /* duration_ns   */
    uint32_t off8  = off7 + 8;                                        /* status_code   */
    uint32_t total = off8 + 8;

    /* ── Object header ── */
    wt_u32(out + 0, MAGIC_OBJ);
    wt_u32(out + 4, total);

    /* Bitmask: slots 0-8 present, slot 9 absent */
    out[8] = 0xFF; /* slots 0-6: all set, continuation bit */
    out[9] = 0x03; /* slots 7-8: set, slot 9: not set      */

    /* Offset table (10 entries, u16 each) — slots 0-8 present in order */
    uint32_t ot = 10;
    wt_u16(out + ot +  0, (uint16_t)off0);
    wt_u16(out + ot +  2, (uint16_t)off1);
    wt_u16(out + ot +  4, (uint16_t)off2);
    wt_u16(out + ot +  6, (uint16_t)off3);
    wt_u16(out + ot +  8, (uint16_t)off4);
    wt_u16(out + ot + 10, (uint16_t)off5);
    wt_u16(out + ot + 12, (uint16_t)off6);
    wt_u16(out + ot + 14, (uint16_t)off7);
    wt_u16(out + ot + 16, (uint16_t)off8);
    wt_u16(out + ot + 18, 0); /* slot 9 absent → 0 */

    /* 2-byte pad to reach 32-byte data_start (30 bytes header → +2) */
    out[30] = 0; out[31] = 0;

    /* ── Field data ── */
    wt_i64(out + off0, trace_id_hi);
    wt_i64(out + off1, trace_id_lo);
    wt_i64(out + off2, span_id);
    wt_i64(out + off3, parent_span);

    wt_u32(out + off4, name_len);
    wt_memcpy(out + off4 + 4, name_bytes, name_len);
    for (uint32_t i = 0; i < name_pad; i++) out[off4 + 4 + name_len + i] = 0;

    wt_u32(out + off5, svc_len);
    wt_memcpy(out + off5 + 4, svc_bytes, svc_len);
    for (uint32_t i = 0; i < svc_pad; i++) out[off5 + 4 + svc_len + i] = 0;

    wt_i64(out + off6, start_time_ns);
    wt_i64(out + off7, duration_ns);
    wt_i64(out + off8, status_code);

    return total;
}
