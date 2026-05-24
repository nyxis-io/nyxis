---
name: compile-nxs
description: Compile Nyxis .nxs text sources to memory-mapped .nxb binaries and import JSON/CSV/XML.
---

# Compile and Import Nyxis Data

Use this skill when converting human-readable `.nxs` sources or structured files into `.nxb` binaries.

## Compile .nxs → .nxb

```bash
nxs path/to/schema.nxs path/to/output.nxb
```

Or via MCP tool `nxs_compile`:

```json
{ "source": "records.nxs", "output": "records.nxb" }
```

## Import JSON / CSV / XML → .nxb

MCP tool `nxs_import`:

```json
{
  "source": "records.json",
  "output": "records.nxb",
  "format": "json"
}
```

CLI:

```bash
nxs-import --from json records.json records.nxb
```

## Layout selection

Choose layout at compile time: `row` (streaming/scroll), `columnar` (analytics), or `pax` (mixed). See SPEC §4 and https://nyxis.io/use-cases/ for workload guidance.

## Getting started

Full examples for all ten language SDKs: https://github.com/nyxis-io/nyxis/blob/main/GETTING_STARTED.md
