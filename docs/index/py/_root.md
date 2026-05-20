---
room: _root
subdomain: py
source_paths: py/
see_also: docs/index/_root.md
architectural_health: normal
security_tier: normal
---

# Python — Building Router

Subdomain: py/
Source paths: py/

## TASK → LOAD

| Task | Load |
|------|------|
| Read .nxb files in pure Python | reader.md |
| Write .nxb output from Python | reader.md |
| Run or add Python reader/writer tests | reader.md |
| Benchmark pure-Python NXS vs JSON | reader.md |
| Use the C-accelerated reader/writer | c_ext.md |
| Benchmark C extension vs pure Python | c_ext.md |
| Measure WAL-append throughput | c_ext.md |
| Verify C extension parity | c_ext.md |

## Rooms

| Room | Source paths | Files |
|------|-------------|-------|
| reader.md | py/nxs.py, nxs_writer.py, test_nxs.py, bench.py | 4 |
| c_ext.md | py/_nxs.c, bench_c.py, bench_wal.py, test_c_ext.py | 4 |
