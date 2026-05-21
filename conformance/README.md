# NXS Conformance Corpus

This directory contains the language-agnostic conformance test vectors for the
Nyxis v1.0 binary format. Every NXS reader implementation must pass
all vectors in this corpus before a release tag is applied.

## Vector Format

Each test vector consists of two files:

| File | Description |
|------|-------------|
| `<name>.nxb` | The binary NXS file to read |
| `<name>.expected.json` | The expected decoded contents |

### Positive vector — expected.json schema

```json
{
  "record_count": 1,
  "keys": ["id", "name", "active"],
  "records": [
    { "id": 42, "name": "alice", "active": true }
  ]
}
```

- `record_count`: total number of records in the file
- `keys`: schema key names in slot order
- `records`: array of objects; each maps field names to their decoded values
  - absent fields are omitted from the map (not null)
  - null fields are represented as JSON `null`
  - i64/f64 values use JSON numbers
  - bool values use JSON booleans
  - string values use JSON strings
  - lists are JSON arrays
  - time values are JSON numbers (Unix nanoseconds)

### Negative vector — expected.json schema

```json
{ "error": "ERR_BAD_MAGIC" }
```

Supported error codes: `ERR_BAD_MAGIC`, `ERR_DICT_MISMATCH`, `ERR_OUT_OF_BOUNDS`, `ERR_INVALID_FLAGS`, `ERR_INCOMPATIBLE_FLAGS`, `ERR_INVALID_PAGE_MAGIC`

## Conformance Runner Contract

Each runner accepts the conformance directory as its first argument:

```
runner conformance/
```

For **positive vectors**: the runner reads the `.nxb` file, reads the
`.expected.json`, and asserts that every field in every record matches the
expected value. Fields absent from the expected map are ignored (allowing
partial assertions).

For **negative vectors**: the runner attempts to open the `.nxb` file and
asserts that the error code in the thrown/returned error matches the `"error"`
field of the expected JSON.

The runner exits **0** if every vector passes, **1** if any vector fails.

## Generating Vectors

```bash
cd rust
cargo run --release --bin gen_conformance -- ../conformance
```

## Vectors

| Vector | Type | What it tests |
|--------|------|---------------|
| `minimal` | positive | 1 record, 3 fields (i64, str, bool) |
| `all_sigils` | positive | 1 record, one field of every supported sigil type |
| `null_vs_absent` | positive | 3 records: field present, null, absent |
| `sparse` | positive | 100 records with different random subsets of 8 fields |
| `nested` | positive | Object with 3 levels of nested objects |
| `list_i64` | positive | Record with a list of i64 values |
| `list_f64` | positive | Record with a list of f64 values |
| `unicode_strings` | positive | Strings with multibyte UTF-8 and emoji |
| `large` | positive | 10,000 records (exercises tail-index lookup) |
| `max_keys` | positive | Schema with 255 keys (LEB128 boundary) |
| `jumbo_string` | positive | Single string field of 128 KB |
| `bad_magic` | negative | Corrupt preamble → ERR_BAD_MAGIC |
| `bad_dict_hash` | negative | Valid file, corrupted DictHash → ERR_DICT_MISMATCH |
| `truncated` | negative | File cut at byte 20 → ERR_OUT_OF_BOUNDS |
| `columnar_flat8_dense_100` | positive | 100 records, dense flat-8, columnar layout |
| `columnar_flat8_sparse_10pct_100` | positive | 100 records, 10% sparse, columnar |
| `columnar_invalid_flags_both` | negative | FLAG_COLUMNAR + FLAG_PAX → ERR_INVALID_FLAGS |
| `columnar_invalid_streaming` | negative | Columnar + TailPtr=0 → ERR_INCOMPATIBLE_FLAGS |
| `pax_flat8_dense_p256_1000` | positive | 1000 records, page size 256, dense PAX |
| `pax_flat8_sparse_10pct_p256` | positive | 1000 records, 10% sparse, PAX |
| `pax_streaming_unsealed` | negative | Unsealed PAX (3 pages, no footer) — batch open → ERR_BAD_MAGIC |
| `pax_invalid_page_magic` | negative | Corrupt NXSP at first page → ERR_INVALID_PAGE_MAGIC |
