# Nyxis core — Rust compiler, conformance vectors, MCP, and cross-repo driver orchestration.
#
# Language SDKs live in ../nyxis-drivers (MIT). Run `make -C ../nyxis-drivers test` for driver-only CI.

DRV ?= ../nyxis-drivers
CONF ?= $(abspath conformance)

# NXS — lint, fix, test, conformance, and fuzz for all ten language implementations.
#
# Usage:
#   make lint           # strict: every step must pass (no swallowed failures)
#   make fix            # auto-fix all fixable issues
#   make test           # run all language test suites (needs fixtures: make fixtures)
#   make fixtures       # generate fixtures (FIXTURE_COUNT=1000; see FIXTURE_DIR / FIXTURE_OUT)
#   make test-py-ci     # Python + C extension parity (matches CI)
#   make test-ruby-ci   # Ruby + C extension smoke (matches CI)
#   make test-php-ci    # PHP + C extension tests (matches CI)
#   make test-rust-ci   # Rust tests + compile examples/ (matches CI)
#   make conformance    # generate vectors + run all conformance runners
#   make conformance-run-js  # … single runner (see Makefile)
#   make fuzz           # run cargo-fuzz for 60s (requires nightly)
#   make all            # fix + test + conformance
#   make install-git-hooks   # pre-commit → make lint (SKIP_HOOKS=1 to bypass once)

.PHONY: all lint fix test conformance fuzz fixtures sdk rust-examples install-git-hooks demo bench-node \
        lint-rust  fix-rust  test-rust \
        lint-js    fix-js    test-js \
        lint-py    fix-py    test-py \
        lint-go    fix-go    test-go \
        lint-ruby  fix-ruby  test-ruby \
        lint-php   fix-php   test-php \
        lint-c     fix-c     test-c \
        lint-swift fix-swift test-swift \
        lint-kotlin           test-kotlin \
        lint-csharp fix-csharp test-csharp \
        test-rust-ci test-py-ci test-ruby-ci test-php-ci \
        conformance-run conformance-run-js conformance-run-py conformance-run-go \
        conformance-run-ruby conformance-run-php conformance-run-c conformance-run-swift \
        conformance-run-kotlin conformance-run-csharp conformance-run-rust \
        lint-mcp fix-mcp build-mcp test-mcp install-mcp

FIXTURE_DIR     ?= bench/fixtures
FIXTURE_COUNT   ?= 1000
# Writable path for .nxb/.json/.csv: defaults to FIXTURE_DIR, or out/fixtures when that dir is not writable.
FIXTURE_OUT     := $(shell d="$(FIXTURE_DIR)"; mkdir -p "$$d" out/fixtures 2>/dev/null; \
	if touch "$$d/.nxs_wprobe" 2>/dev/null; then rm -f "$$d/.nxs_wprobe"; printf '%s' "$$d"; \
	elif touch out/fixtures/.nxs_wprobe 2>/dev/null; then rm -f out/fixtures/.nxs_wprobe; \
	echo "nxs: not writable: $$d — using out/fixtures (override with FIXTURE_DIR=...)" 1>&2; printf '%s' out/fixtures; \
	else printf '%s' "$$d"; fi)
JAVA_HOME       ?= /opt/homebrew/opt/openjdk@21
# Default to net10 so `make conformance-run` works with a single current SDK; CI sets net9 where needed.
DOTNET_FRAMEWORK ?= net10.0

# ── Demos & benchmarks (core-owned) ───────────────────────────────────────────

demo:
	docker compose up

bench-node:
	node bench/bench.js $(FIXTURE_OUT)

# ── Top-level ─────────────────────────────────────────────────────────────────

all: fix test conformance

install-git-hooks:
	git config core.hooksPath .githooks
	@echo "Git hooks path set to .githooks (pre-commit runs: make lint). Bypass once: SKIP_HOOKS=1 git commit …"

lint: lint-rust lint-mcp

lint-all: lint-rust lint-js lint-py lint-go lint-ruby lint-php lint-c lint-swift lint-kotlin lint-csharp lint-mcp

fix: fix-rust fix-js fix-py fix-go fix-ruby fix-php fix-c fix-swift fix-csharp
	@echo "\n✅  All auto-fixes applied."

test: test-rust

test-all: test-rust test-js test-py test-go test-ruby test-php test-c test-swift test-kotlin test-csharp
	@echo "\n✅  All tests passed."

# ── Rust ──────────────────────────────────────────────────────────────────────

lint-rust:
	cd rust && cargo fmt --check && cargo clippy --lib --bin nxs --bin bench --bin gen_fixtures -- -D warnings -A dead_code -A unused_imports -A clippy::empty_line_after_doc_comments -A clippy::collapsible_if -A clippy::single_match -A clippy::manual_is_multiple_of -A clippy::manual_div_ceil -A clippy::same_item_push -A clippy::new_without_default -A clippy::len_without_is_empty

fix-rust:
	cd rust && cargo fmt
	cargo fmt -- conformance/generate.rs conformance/run_rust.rs 2>/dev/null || true

test-rust:
	cd rust && cargo test --release

rust-examples:
	cd rust && cargo build --release --bin nxs
	cd rust && for f in ../examples/*.nxs; do ./target/release/nxs "$$f" && echo "compiled $$f"; done

test-rust-ci: test-rust rust-examples

# Clone nyxis-drivers if missing (required for /sdk/ in docker and browser demos).
$(DRV)/js/nxs.js:
	@if [ ! -f "$(DRV)/js/nxs.js" ]; then \
	  echo "Cloning nyxis-drivers into $(DRV)…"; \
	  git clone --depth 1 https://github.com/nyxis-io/nyxis-drivers.git "$(DRV)"; \
	fi

sdk: $(DRV)/js/nxs.js

# Best-effort chmod; output path is FIXTURE_OUT (fallback: out/fixtures).
fixtures: $(DRV)/js/nxs.js
	@chmod -R u+w $(FIXTURE_DIR) out/fixtures 2>/dev/null || true
	cd rust && cargo run --release --bin gen_fixtures -- ../$(FIXTURE_OUT) $(FIXTURE_COUNT)

# ── JavaScript ───────────────────────────────────────────────────────────────

lint-js:
	cd $(DRV)/js && npm install --ignore-scripts --no-fund --no-audit
	cd $(DRV)/js && npm run lint

fix-js:
	cd $(DRV)/js && npm install --ignore-scripts --no-fund --no-audit
	cd $(DRV)/js && npx eslint --fix --max-warnings 0 nxs.js nxs_writer.js wasm.js test.js test_wasm.js

test-js:
	node $(DRV)/js/test.js $(FIXTURE_OUT)

# ── Python ───────────────────────────────────────────────────────────────────

lint-py:
	@command -v ruff >/dev/null 2>&1 || python3 -m pip install --user ruff
	cd py && ruff check --select E,W,F --ignore E501,E701,E702 .

fix-py:
	@command -v ruff >/dev/null 2>&1 || python3 -m pip install --user ruff
	cd py && ruff check --select E,W,F --ignore E501,E701,E702 --fix .

test-py:
	cd py && python test_nxs.py ../$(FIXTURE_OUT)

test-py-ci: test-py
	cd py && bash build_ext.sh
	cd py && python test_c_ext.py ../$(FIXTURE_OUT)

# ── Go ────────────────────────────────────────────────────────────────────────

lint-go:
	@cd go && { fmt=$$(gofmt -l .); [ -z "$$fmt" ] || { printf 'run gofmt -w on:\n%s\n' "$$fmt"; exit 1; }; }
	cd go && go vet ./...
	@PATH="$$PATH:$$(go env GOPATH)/bin"; \
	  command -v staticcheck >/dev/null 2>&1 || go install honnef.co/go/tools/cmd/staticcheck@latest; \
	  cd $(DRV)/go && staticcheck ./...

fix-go:
	cd go && gofmt -w .

test-go:
	cd go && go test ./...

# ── MCP Server (Go) ───────────────────────────────────────────────────────────

lint-mcp:
	@cd mcp && { fmt=$$(gofmt -l .); [ -z "$$fmt" ] || { printf 'run gofmt -w on:\n%s\n' "$$fmt"; exit 1; }; }
	cd mcp && go vet ./...

fix-mcp:
	cd mcp && gofmt -w .

build-mcp:
	@mkdir -p bin
	cd mcp && go build -o ../bin/nxs-mcp .

test-mcp:
	cd mcp && go test ./...

install-mcp: build-mcp
	install -m 0755 bin/nxs-mcp $(or $(PREFIX),/usr/local)/bin/nxs-mcp
	@echo "installed: $$(which nxs-mcp)"

# ── Ruby ─────────────────────────────────────────────────────────────────────

lint-ruby:
	@command -v rubocop >/dev/null 2>&1 || gem install rubocop --no-document
	rubocop $(DRV)/ruby/nxs.rb $(DRV)/ruby/test.rb $(DRV)/ruby/bench.rb --config $(DRV)/ruby/.rubocop.yml --no-color --cache false

fix-ruby:
	@command -v rubocop >/dev/null 2>&1 || gem install rubocop --no-document
	rubocop $(DRV)/ruby/nxs.rb $(DRV)/ruby/test.rb $(DRV)/ruby/bench.rb --config $(DRV)/ruby/.rubocop.yml --no-color --cache false -A

test-ruby:
	ruby $(DRV)/ruby/test.rb $(FIXTURE_OUT)

test-ruby-ci: test-ruby
	bash $(DRV)/ruby/ext/build.sh
	ruby $(DRV)/ruby/bench_c.rb $(FIXTURE_OUT)

# ── PHP ───────────────────────────────────────────────────────────────────────

lint-php:
	@command -v composer >/dev/null 2>&1 || { echo "Install Composer: https://getcomposer.org/" >&2; exit 1; }
	cd php && composer install --no-interaction --prefer-dist --no-progress
	cd php && ./vendor/bin/phpstan analyse Nxs.php --level=5 --no-progress

fix-php: lint-php

test-php:
	php $(DRV)/php/test.php $(FIXTURE_OUT)

test-php-ci: test-php
	bash $(DRV)/php/nxs_ext/build.sh
	php -d extension=$(DRV)/php/nxs_ext/modules/nxs.so php/test.php $(FIXTURE_OUT)

# ── C ─────────────────────────────────────────────────────────────────────────

lint-c:
	@command -v cppcheck >/dev/null 2>&1 || brew install cppcheck
	cppcheck --error-exitcode=1 --suppress=missingIncludeSystem c/nxs.c c/nxs.h

fix-c: lint-c

test-c:
	cd c && make test -s && ./test ../$(FIXTURE_OUT)

# ── Swift ─────────────────────────────────────────────────────────────────────

lint-swift:
	@command -v swiftlint >/dev/null 2>&1 || brew install swiftlint
	cd swift && swiftlint lint --strict --cache-path .swiftlint-cache Sources/NXS

fix-swift:
	@command -v swiftlint >/dev/null 2>&1 || brew install swiftlint
	cd swift && swiftlint --fix --strict --cache-path .swiftlint-cache Sources/NXS

test-swift:
	cd swift && swift run nxs-test ../$(FIXTURE_OUT)

# ── Kotlin ───────────────────────────────────────────────────────────────────

lint-kotlin:
	cd kotlin && JAVA_HOME=$(JAVA_HOME) PATH="$(JAVA_HOME)/bin:$$PATH" ./gradlew ktlintCheck -q

test-kotlin:
	cd kotlin && JAVA_HOME=$(JAVA_HOME) PATH=$(JAVA_HOME)/bin:$$PATH ./gradlew run --args="../$(FIXTURE_OUT)" -q

# ── C# ────────────────────────────────────────────────────────────────────────

lint-csharp:
	cd csharp && DOTNET_FRAMEWORK=$(DOTNET_FRAMEWORK) dotnet format nxs.csproj --verify-no-changes --severity warn

fix-csharp:
	cd csharp && DOTNET_FRAMEWORK=$(DOTNET_FRAMEWORK) dotnet format nxs.csproj

test-csharp:
	cd csharp && dotnet run -p:NxsTargetFramework=$(DOTNET_FRAMEWORK) -- ../$(FIXTURE_OUT)

# ── Conformance suite ─────────────────────────────────────────────────────────
# Generates canonical .nxb/.expected.json vectors, then runs all 10 language
# runners against them. Requires fixtures to be generated first.

conformance: conformance-generate conformance-run

conformance-generate:
	@echo "Generating conformance vectors..."
	cd rust && cargo run --release --bin gen_conformance -- ../conformance
	@echo "Vectors written to conformance/"

conformance-run-js:
	node conformance/run_js.js conformance/

conformance-run-py:
	python3 conformance/run_py.py conformance/

conformance-run-go:
	cd $(DRV)/go && go run $(CONF)/run_go.go $(CONF)/

conformance-run-ruby:
	ruby conformance/run_ruby.rb conformance/

conformance-run-php:
	php conformance/run_php.php conformance/

conformance-run-c:
	cc -std=c99 -O2 -I$(DRV)/c $(DRV)/c/nxs.c conformance/run_c.c -o /tmp/run_c_conf -lm -Wno-format-truncation -Wno-unused-result && /tmp/run_c_conf conformance/

conformance-run-swift:
	cd $(DRV)/swift && swift run nxs-conformance $(CONF)/

conformance-run-kotlin:
	cd $(DRV)/kotlin && JAVA_HOME=$(JAVA_HOME) PATH="$(JAVA_HOME)/bin:$$PATH" \
	  ./gradlew conformance -PconformanceDir=$(CONF) -q

conformance-run-csharp:
	cd $(DRV)/csharp && dotnet run -p:NxsTargetFramework=$(DOTNET_FRAMEWORK) -- --conformance $(CONF)/

conformance-run-rust:
	cd rust && cargo run --release --bin conformance_runner -- ../conformance/

conformance-run: conformance-run-js conformance-run-py conformance-run-go conformance-run-ruby conformance-run-php conformance-run-c conformance-run-swift conformance-run-kotlin conformance-run-csharp conformance-run-rust
	@echo "✅  Conformance: all runners finished."

# ── Fuzz ─────────────────────────────────────────────────────────────────────
# Requires Rust nightly: rustup install nightly
# Run for FUZZ_TIME seconds (default 60).

FUZZ_TIME ?= 60

fuzz:
	@echo "Fuzzing for $(FUZZ_TIME)s (requires nightly)..."
	cd rust && cargo +nightly fuzz run fuzz_decode -- -max_total_time=$(FUZZ_TIME) -rss_limit_mb=0 -max_len=8192
	cd rust && cargo +nightly fuzz run fuzz_writer_roundtrip -- -max_total_time=$(FUZZ_TIME) -rss_limit_mb=0 -max_len=4096
	@echo "✅  Fuzz complete — no crashes found."

convert-test:
	@echo "Running converter suite tests..."
	cd rust && cargo test --test e2e --test exit_codes --test json_import
	@echo "✅  Converter tests passed."

convert-demo:
	@echo "Running converter demo: JSON → .nxb → inspect → export"
	cd rust && cargo build --release --bin nxs-import --bin nxs-export --bin nxs-inspect 2>/dev/null
	echo '[{"id":1,"name":"alice","score":9.5},{"id":2,"name":"bob","score":8.1}]' \
	  | ./rust/target/release/nxs-import --from json - /tmp/nxs_demo.nxb
	./rust/target/release/nxs-inspect /tmp/nxs_demo.nxb
	./rust/target/release/nxs-export --to json --pretty /tmp/nxs_demo.nxb
	rm -f /tmp/nxs_demo.nxb
	@echo "✅  Demo complete."
