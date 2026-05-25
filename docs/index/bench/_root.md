---
room: _root
subdomain: bench
see_also: ["../_root.md", "mcp_server.md", "site/_root.md"]
architectural_health: normal
security_tier: normal
---

# Benchmark Suite — Building Router

Subdomain: bench/
Source paths: bench/

## TASK → LOAD

| Task | Load |
|------|------|
| Run cross-format harness (Go, Rust, C, Python, D) | harness.md |
| Run Workload F adaptive-prefetch harness | harness.md |
| Generate canonical JSON workloads A/B/C | generators.md |
| Transcode JSON to NXB, Parquet, Cap'n Proto, etc. | generators.md |
| Render benchmark tables or freeze BENCHMARK_SUITE.md | scripts.md |
| Aggregate JSONL results into reports | scripts.md |

## Rooms

| Room | Source paths | Focus |
|------|-------------|-------|
| harness.md | bench/harness/ | Go/Rust/C/Python/D/prefetch drivers |
| generators.md | bench/generators/ | JSON gen, transcode, codegen outputs |
| scripts.md | bench/scripts/, scripts/ | Reporting, CI helpers, sequential bench shell |
