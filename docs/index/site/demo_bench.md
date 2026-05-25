---
room: demo_bench
subdomain: site
source_paths: [site/demo/, site/bench/, site/bench/wasm/]
see_also: ["web_app.md", "../bench/harness.md"]
architectural_health: normal
security_tier: normal
---

# site/demo & site/bench — Local Servers & Browser Bench

Subdomain: site/
Source paths: site/demo/, site/bench/, site/bench/wasm/

## TASK → LOAD

| Task | Load |
|------|------|
| Serve site with SharedArrayBuffer headers | demo_bench.md |
| Run browser-side NXS vs JSON bench | demo_bench.md |

---

# site/bench/bench-run.js

DOES: Orchestrates browser benchmark runs using bench-worker for NXS/JSON comparisons.
SYMBOLS:
- runBench(config) → metrics

---

# site/bench/bench-worker.js

DOES: Web Worker that executes timed read/serialize loops off the main thread.
SYMBOLS:
- onmessage handler, timing loops

---

# site/bench/bench.js

DOES: Main-thread bench UI glue: posts jobs to worker, aggregates JSON results for BenchView.
SYMBOLS:
- startBench(), renderResults()

---

# site/bench/wasm/build.sh

DOES: Builds freestanding WASM reducers linked for site bench (no libc).
SYMBOLS:
- clang/wasm-ld invocation flags

---

# site/bench/wasm/nxs_reducers.c

DOES: C WASM reducers mirroring js/wasm/nxs_reducers.c for in-site benchmark parity.
SYMBOLS:
- sum_f64_column, count_non_null reducers
PATTERNS: freestanding-wasm

---

# site/demo/server.py

DOES: Threading HTTP static server adding COOP/COEP/CORP headers so SharedArrayBuffer works in demos.
SYMBOLS:
- COOPCOEPRequestHandler.end_headers()
- main() → serves port argv or 8000
CONFIG: port CLI arg
PATTERNS: coop-coep-headers
USE WHEN: Local demo hosting instead of python -m http.server
