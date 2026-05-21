# Cross-format benchmark suite (see bench/README.md, ../BENCHMARK_SUITE.md)
ROOT     := $(abspath ..)
BENCH    := $(abspath .)
DRV      := $(abspath ../../nyxis-drivers)
DATA_JSON := $(BENCH)/data/json
DATA_BIN  := $(BENCH)/data/bin
BENCH_RECORDS ?= 10000
BENCH_CFLAGS  ?= -O3 -std=c99 -Wall -Wextra
TRANSCODE    := $(BENCH)/generators/transcode_rust/target/release/bench-transcode
HARNESS_C    := $(BENCH)/harness/c/harness
VENV_PY      := $(shell test -x $(ROOT)/.venv-bench/bin/python && echo $(ROOT)/.venv-bench/bin/python || echo python3)
HARNESS_PY   := $(VENV_PY) $(BENCH)/harness/python/harness.py
HARNESS_GO   := cd $(DRV)/go && go run $(BENCH)/harness/go/main.go
HARNESS_RUST := $(BENCH)/harness/rust/target/release/bench-harness
FORMATS_NXS  := nxs
FORMATS_OTHER := proto capnp
# fb arrow added when tools/deps present (see transcode-formats)

STREAM_D := $(BENCH)/harness/stream_d/target/release/bench-stream-d

.PHONY: all gen codegen transcode-nxs transcode-formats transcode harness \
        run run-b run-c run-a run-d run-d-smoke run-d-throughput quick ci matrix bench-cold results results-1m results-1m-fast \
        setup-venv render-tables render-all verdicts clean

all: gen transcode harness

gen:
	$(VENV_PY) $(BENCH)/generators/gen.py --out $(DATA_JSON) --records $(BENCH_RECORDS)

codegen:
	bash $(BENCH)/generators/codegen.sh

transcode-nxs: $(TRANSCODE)
	@mkdir -p $(DATA_BIN)
	@test -f $(DATA_JSON)/workload_B_$(BENCH_RECORDS).json
	$(TRANSCODE) --workload B --json $(DATA_JSON)/workload_B_$(BENCH_RECORDS).json \
	  --out $(DATA_BIN)/workload_B_nxs_$(BENCH_RECORDS).nxb
	@test -f $(DATA_JSON)/workload_C_$(BENCH_RECORDS).json
	$(TRANSCODE) --workload C --json $(DATA_JSON)/workload_C_$(BENCH_RECORDS).json \
	  --out $(DATA_BIN)/workload_C_nxs_$(BENCH_RECORDS).nxb
	@for pop in 10 25 50 90; do \
	  f=$(DATA_JSON)/workload_A_pop$${pop}_$(BENCH_RECORDS).json; \
	  test -f $$f || continue; \
	  $(TRANSCODE) --workload A --json $$f \
	    --out $(DATA_BIN)/workload_A_nxs_$(BENCH_RECORDS)_pop$${pop}.nxb; \
	done

transcode-formats: codegen
	@mkdir -p $(DATA_BIN)
	@f=$(DATA_JSON)/workload_B_$(BENCH_RECORDS).json; \
	ext=pb; \
	  $(VENV_PY) $(BENCH)/generators/transcode.py --workload B --format proto \
	  --json $$f --out $(DATA_BIN)/workload_B_proto_$(BENCH_RECORDS).$$ext || true; \
	$(VENV_PY) $(BENCH)/generators/transcode.py --workload B --format fb \
	  --json $$f --out $(DATA_BIN)/workload_B_fb_$(BENCH_RECORDS).bfbs || true; \
	$(VENV_PY) $(BENCH)/generators/transcode.py --workload B --format capnp \
	  --json $$f --out $(DATA_BIN)/workload_B_capnp_$(BENCH_RECORDS).capnp || true
	@f=$(DATA_JSON)/workload_C_$(BENCH_RECORDS).json; \
	$(VENV_PY) $(BENCH)/generators/transcode.py --workload C --format proto \
	  --json $$f --out $(DATA_BIN)/workload_C_proto_$(BENCH_RECORDS).pb || true; \
	$(VENV_PY) $(BENCH)/generators/transcode.py --workload C --format capnp \
	  --json $$f --out $(DATA_BIN)/workload_C_capnp_$(BENCH_RECORDS).capnp || true; \
	$(VENV_PY) $(BENCH)/generators/transcode.py --workload C --format arrow \
	  --json $$f --out $(DATA_BIN)/workload_C_arrow_$(BENCH_RECORDS).arrow || true
	@for pop in 10 25 50 90; do \
	  f=$(DATA_JSON)/workload_A_pop$${pop}_$(BENCH_RECORDS).json; \
	  test -f $$f || continue; \
	  for fmt in proto fb capnp; do \
	    ext=pb; \
	    [ "$$fmt" = fb ] && ext=bfbs; \
	    [ "$$fmt" = capnp ] && ext=capnp; \
	    $(VENV_PY) $(BENCH)/generators/transcode.py --workload A --format $$fmt \
	      --json $$f --out $(DATA_BIN)/workload_A_$${fmt}_$(BENCH_RECORDS)_pop$${pop}.$$ext || true; \
	  done; \
	done

transcode: transcode-nxs transcode-formats

$(TRANSCODE):
	cd $(BENCH)/generators/transcode_rust && cargo build --release

harness-c: $(HARNESS_C)
harness-rust: $(HARNESS_RUST)

$(HARNESS_C):
	$(MAKE) -C $(BENCH)/harness/c BENCH_CFLAGS="$(BENCH_CFLAGS)"

$(HARNESS_RUST):
	cd $(BENCH)/harness/rust && cargo build --release

harness: harness-c harness-rust

# ── Workload B: all formats (Python harness for competitors) ─────────────────
run-b: gen transcode harness
	@for fmt in nxs proto capnp; do \
	  $(HARNESS_PY) --workload B --format $$fmt --records $(BENCH_RECORDS) --metric size --data-dir $(DATA_BIN) 2>/dev/null || true; \
	  $(HARNESS_PY) --workload B --format $$fmt --records $(BENCH_RECORDS) --metric open --data-dir $(DATA_BIN) 2>/dev/null || true; \
	  $(HARNESS_PY) --workload B --format $$fmt --records $(BENCH_RECORDS) --metric access --data-dir $(DATA_BIN) 2>/dev/null || true; \
	  $(HARNESS_PY) --workload B --format $$fmt --records $(BENCH_RECORDS) --metric scan --data-dir $(DATA_BIN) 2>/dev/null || true; \
	done
	@$(HARNESS_C) --workload B --format nxs --records $(BENCH_RECORDS) --metric size --data-dir $(DATA_BIN)
	@$(HARNESS_C) --workload B --format nxs --records $(BENCH_RECORDS) --metric open --data-dir $(DATA_BIN)

run-c: gen transcode harness
	@for fmt in nxs proto capnp arrow; do \
	  for met in size open scan distinct; do \
	    [ "$$fmt" != arrow ] && [ "$$met" = distinct ] && continue; \
	    [ "$$fmt" = arrow ] || [ "$$met" != distinct ] && \
	      $(HARNESS_PY) --workload C --format $$fmt --records $(BENCH_RECORDS) --metric $$met --data-dir $(DATA_BIN) 2>/dev/null || true; \
	  done; \
	done

run-a: gen transcode harness
	@for pop in 0.10 0.25 0.50 0.90; do \
	  for fmt in nxs proto fb capnp; do \
	    $(HARNESS_PY) --workload A --format $$fmt --records $(BENCH_RECORDS) --population $$pop \
	      --metric size --data-dir $(DATA_BIN) 2>/dev/null || true; \
	    $(HARNESS_PY) --workload A --format $$fmt --records $(BENCH_RECORDS) --population $$pop \
	      --metric selective --data-dir $(DATA_BIN) 2>/dev/null || true; \
	  done; \
	  if [ -x $(HARNESS_C) ]; then \
	    $(HARNESS_C) --workload A --format nxs --records $(BENCH_RECORDS) --population $$pop \
	      --metric size --data-dir $(DATA_BIN) 2>/dev/null || true; \
	    $(HARNESS_C) --workload A --format nxs --records $(BENCH_RECORDS) --population $$pop \
	      --metric selective --data-dir $(DATA_BIN) 2>/dev/null || true; \
	  fi; \
	done

run: run-b

$(STREAM_D):
	cd $(BENCH)/harness/stream_d && CARGO_TARGET_DIR=target cargo build --release

# Workload D: streaming TTFR (D2 file). Smoke = 20 TTFR trials × 1 record; seal on 100 rows.
run-d-smoke: BENCH_RECORDS_D ?= 1000
run-d-smoke: $(STREAM_D)
	@test -f $(DATA_JSON)/workload_B_$(BENCH_RECORDS_D).json || \
	  $(MAKE) gen BENCH_RECORDS=$(BENCH_RECORDS_D)
	$(STREAM_D) --records $(BENCH_RECORDS_D) --runs 20 --ttfr-records 1 \
	  --seal-records 100 --seal-runs 5 --flush-every 1 \
	  --json $(DATA_JSON)/workload_B_$(BENCH_RECORDS_D).json

# Seal breakdown: tail-index write vs fsync + synthetic sync at 100KB/1MB/10MB tail sizes
# Throughput only (batched flush, 10k rows) — fast complement to full run-d
run-d-throughput: BENCH_RECORDS_D ?= 10000
run-d-throughput: $(STREAM_D)
	@test -f $(DATA_JSON)/workload_B_$(BENCH_RECORDS_D).json || \
	  $(MAKE) gen BENCH_RECORDS=$(BENCH_RECORDS_D)
	$(STREAM_D) --records $(BENCH_RECORDS_D) --runs 0 --ttfr-records 0 --seal-records 0 \
	  --flush-every 100 --formats nxs,proto,capnp \
	  --json $(DATA_JSON)/workload_B_$(BENCH_RECORDS_D).json

run-d-seal-profile: BENCH_RECORDS_D ?= 10000
run-d-seal-profile: $(STREAM_D)
	@test -f $(DATA_JSON)/workload_B_$(BENCH_RECORDS_D).json || \
	  $(MAKE) gen BENCH_RECORDS=$(BENCH_RECORDS_D)
	$(STREAM_D) --seal-profile --records $(BENCH_RECORDS_D) --seal-records $(BENCH_RECORDS_D) \
	  --seal-runs 10 --flush-every 100 \
	  --json $(DATA_JSON)/workload_B_$(BENCH_RECORDS_D).json

run-d: BENCH_RECORDS_D ?= $(BENCH_RECORDS)
run-d: $(STREAM_D)
	@test -f $(DATA_JSON)/workload_B_$(BENCH_RECORDS_D).json || \
	  $(MAKE) gen BENCH_RECORDS=$(BENCH_RECORDS_D)
	$(STREAM_D) --records $(BENCH_RECORDS_D) --runs 1000 --ttfr-records 1 \
	  --seal-records $(BENCH_RECORDS_D) --seal-runs 20 --flush-every 1 \
	  --formats nxs,proto,capnp,fb \
	  --json $(DATA_JSON)/workload_B_$(BENCH_RECORDS_D).json
	$(STREAM_D) --records $(BENCH_RECORDS_D) --runs 1000 --ttfr-records 1 \
	  --seal-records $(BENCH_RECORDS_D) --seal-runs 20 --flush-every 100 \
	  --formats nxs,proto,capnp,fb \
	  --json $(DATA_JSON)/workload_B_$(BENCH_RECORDS_D).json

matrix:
	@bash $(BENCH)/scripts/run_all.sh

bench-cold:
	BENCH_COLD=1 bash $(BENCH)/scripts/run_all.sh

results:
	bash $(BENCH)/scripts/save_results.sh

# 1M rows: fewer statistical samples + warm competitor paths + Rust NXS scan (see bench/README.md)
results-1m: BENCH_RECORDS=1000000
results-1m:
	bash $(BENCH)/scripts/save_results.sh

results-1m-fast: BENCH_RECORDS=1000000
results-1m-fast:
	BENCH_FAST=1 bash $(BENCH)/scripts/save_results.sh

ifneq ($(filter /%,$(RESULT_DIR)),)
RESULT_ABS := $(RESULT_DIR)
else
RESULT_ABS := $(abspath $(ROOT)/$(RESULT_DIR))
endif

render-tables:
	@test -n "$(RESULT_DIR)" || (echo "usage: make -C bench render-tables RESULT_DIR=bench/results/2026-05-21_host" >&2; exit 1)
	$(VENV_PY) $(BENCH)/scripts/render_tables.py $(RESULT_ABS)/summary.json

render-all:
	@test -n "$(RESULT_DIR)" || (echo "usage: make -C bench render-all RESULT_DIR=bench/results/2026-05-21_host" >&2; exit 1)
	@$(MAKE) render-tables RESULT_DIR=$(RESULT_DIR)
	@$(MAKE) verdicts RESULT_DIR=$(RESULT_DIR)

setup-venv:
	bash $(BENCH)/scripts/setup_venv.sh

quick: BENCH_RECORDS=1000
quick: run-b

# CI: 1k records, Workload B + sparse size/selective smoke (needs venv + codegen)
ci: BENCH_RECORDS=1000
ci: gen codegen transcode harness run-b run-a

verdicts:
	@test -n "$(RESULT_DIR)" || (echo "usage: make -C bench verdicts RESULT_DIR=bench/results/2026-05-21_host" >&2; exit 1)
	$(VENV_PY) $(BENCH)/scripts/verdicts.py $(RESULT_ABS)/summary.json --write $(RESULT_ABS)/outcomes.json

clean:
	rm -rf $(DATA_JSON) $(DATA_BIN)
	rm -rf $(BENCH)/generators/generated/nyxis $(BENCH)/generators/generated/*.py \
	  $(BENCH)/generators/generated/Flat8 $(BENCH)/generators/generated/Dense8 2>/dev/null || true
	$(MAKE) -C $(BENCH)/harness/c clean
	cd $(BENCH)/generators/transcode_rust && cargo clean
	cd $(BENCH)/harness/rust && cargo clean
