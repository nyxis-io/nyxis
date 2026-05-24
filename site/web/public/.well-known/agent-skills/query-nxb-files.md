---
name: query-nxb-files
description: Inspect and query Nyxis .nxb binary files using the nxs-mcp MCP server or Rust CLI tools.
---

# Query Nyxis .nxb Files

Use this skill when an agent needs to read schema, records, or exports from Nyxis binary (`.nxb`) files.

## MCP tools (recommended)

Install and wire `nxs-mcp` from [nyxis-io/nyxis](https://github.com/nyxis-io/nyxis):

```bash
cd rust && cargo build --release && cd ..
make build-mcp
```

| Tool | Use when |
|------|----------|
| `nxs_schema` | You only need field names and sigil types |
| `nxs_inspect` | Schema plus a few decoded records (default 3) |
| `nxs_record` | One record by zero-based index |
| `nxs_export_json` | JSON export (default limit 100 records) |
| `nxs_export_csv` | CSV export with optional column filter |

Pass `--data-dir` so `.nxb` files under that directory appear as `nxb:///` MCP resources.

## CLI fallback

```bash
nxs-inspect --json --records 3 path/to/file.nxb
nxs-export --json path/to/file.nxb
```

## Format reference

- Human-readable source: `.nxs` (sigil-typed schema + records)
- Binary wire format: `.nxb` (mmap-friendly, tail-indexed, O(1) seek)
- Spec: https://github.com/nyxis-io/nyxis/blob/main/SPEC.md
