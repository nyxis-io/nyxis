---
room: scripts
subdomain: bench
source_paths: [bench/scripts/, scripts/]
see_also: ["harness.md", "generators.md", "../ci_workflows.md"]
architectural_health: normal
security_tier: normal
---

# bench/scripts/ — Results & Automation

Subdomain: bench/
Source paths: bench/scripts/, scripts/

## TASK → LOAD

| Task | Load |
|------|------|
| Render markdown tables from JSONL bench output | scripts.md |
| Freeze BENCHMARK_SUITE.md from latest results | scripts.md |
| Run full multi-driver benchmark matrix | scripts.md |

---

# bench/scripts/drop_caches.sh

DOES: Drops Linux page cache before benchmark runs for stable cold-read measurements (requires root).
SYMBOLS:
- shell sync && echo 3 > /proc/sys/vm/drop_caches

---

# bench/scripts/freeze_benchmark_md.py

DOES: Reads latest JSONL results and rewrites frozen sections of BENCHMARK_SUITE.md with measured numbers.
SYMBOLS:
- main()
- (+table formatters)

---

# bench/scripts/render_tables.py

DOES: Converts benchmark JSONL aggregates into markdown tables for docs and CI artifacts.
SYMBOLS:
- main()
- render_table(rows) → str

---

# bench/scripts/report.py

DOES: Aggregates multi-workload JSONL logs into summary statistics and verdict lines.
SYMBOLS:
- main()
- load_jsonl(path) → list
- summarize(run) → dict

---

# bench/scripts/run_all.sh

DOES: Orchestrates full benchmark matrix across harness drivers and workloads; writes JSONL under bench/results/.
SYMBOLS:
- invokes harness binaries with standard flags

---

# bench/scripts/save_results.sh

DOES: Archives benchmark JSONL and metadata into dated result directories.
SYMBOLS:
- copies artifacts with timestamp prefix

---

# bench/scripts/setup_venv.sh

DOES: Creates Python venv with dependencies for generators and harness.py.
SYMBOLS:
- pip install -r requirements steps

---

# bench/scripts/verdicts.py

DOES: Applies pass/fail thresholds to benchmark metrics and prints CI-friendly verdict summary.
SYMBOLS:
- main()
- check_threshold(metric, value) → bool

---

# scripts/bench-sequential.sh

DOES: Top-level helper to run sequential benchmark scenarios outside the main bench/Makefile flow.
SYMBOLS:
- invokes make/cargo targets for sequential workloads
