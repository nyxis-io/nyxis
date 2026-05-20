---
room: _root
subdomain: js
source_paths: js/
see_also: docs/index/_root.md
architectural_health: normal
security_tier: normal
---

# JavaScript — Building Router

Subdomain: js/
Source paths: js/

## TASK → LOAD

| Task | Load |
|------|------|
| Read .nxb files in Node or browser | reader.md |
| Write / emit .nxb from JavaScript | reader.md |
| Run or add JS reader/writer tests | reader.md |
| Benchmark JS NXS performance | reader.md |
| Load WASM reducers / attach to NxsReader | wasm_workers.md |
| Add a WASM aggregate function | wasm_workers.md |
| Build or modify browser log-explorer | wasm_workers.md |
| Use SharedArrayBuffer with Web Workers | wasm_workers.md |
| Serve the demo with COOP/COEP headers | wasm_workers.md |

## Rooms

| Room | Source paths | Files |
|------|-------------|-------|
| reader.md | js/nxs.js, nxs_writer.js, test.js, bench.js, eslint.config.js | 5 |
| wasm_workers.md | js/wasm.js, wasm/nxs_reducers.c, nxs_worker.js, explorer_worker.js, json_worker.js, test_wasm.js, theme.js, server.py | 8 |
