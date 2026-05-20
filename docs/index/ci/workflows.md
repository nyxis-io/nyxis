---
room: ci/workflows
source_paths: [.github/workflows/]
file_count: 11
architectural_health: normal
security_tier: normal
hot_paths: [fixtures.yml]
see_also: []
---

# c.yml

DOES: Builds the C NXS reader with `make test` and runs the smoke-test binary against generated fixtures on every push or PR to `c/`.
SYMBOLS:
- fixtures (reusable call)
- test (make test, ./test ../js/fixtures)
DEPENDS: fixtures.yml
PATTERNS: reusable-workflow, artifact-passing
USE WHEN: Changes to `c/` are pushed; depends on fixtures.yml for test data.

---

# csharp.yml

DOES: Builds and tests the C# NXS reader with .NET 9 (passing `-p:NxsTargetFramework=net9.0`) against fixtures on every push or PR to `csharp/`.
SYMBOLS:
- fixtures (reusable call)
- test (setup-dotnet@v4, dotnet run)
DEPENDS: fixtures.yml
PATTERNS: reusable-workflow, framework-version-override
USE WHEN: Changes to `csharp/` are pushed; framework override lets CI use .NET 9 while local dev targets .NET 10.

---

# fixtures.yml

DOES: Reusable `workflow_call` job: compiles the Rust gen_fixtures binary and generates 1000-record .nxb/.json/.csv fixtures, uploading them as an artifact named `fixtures-<SHA>` for downstream language workflows.
SYMBOLS:
- generate (cargo build, gen_fixtures, upload-artifact)
DEPENDS: none
PATTERNS: reusable-workflow, artifact-generation, cargo-release-build
USE WHEN: Called by every language workflow (except rust.yml) to produce shared test data; the single source of fixture truth in CI.

---

# go.yml

DOES: Runs `go test ./...` for the Go NXS reader with Go 1.23 against downloaded fixtures on push or PR to `go/`.
SYMBOLS:
- fixtures (reusable call)
- test (setup-go@v5, go test ./...)
DEPENDS: fixtures.yml
PATTERNS: reusable-workflow, go-module-cache
USE WHEN: Changes to `go/` are pushed.

---

# javascript.yml

DOES: Runs `node js/test.js` with Node 22 against downloaded fixtures on push or PR to `js/`.
SYMBOLS:
- fixtures (reusable call)
- test (setup-node@v4, node test.js)
DEPENDS: fixtures.yml
PATTERNS: reusable-workflow, esm-node
USE WHEN: Changes to `js/` are pushed.

---

# kotlin.yml

DOES: Builds and tests the Kotlin NXS reader with Gradle (using `./gradlew`) on JDK 21 against downloaded fixtures on push or PR to `kotlin/`.
SYMBOLS:
- fixtures (reusable call)
- test (setup-java@v4, gradle/actions/setup-gradle@v4, ./gradlew run)
DEPENDS: fixtures.yml
PATTERNS: reusable-workflow, gradle-wrapper, jdk-21
USE WHEN: Changes to `kotlin/` are pushed; uses the committed `gradlew` wrapper.

---

# php.yml

DOES: Tests both pure-PHP (`php test.php`) and the C extension (`bash build.sh && php -d extension=nxs.so test.php`) against fixtures on push or PR to `php/`.
SYMBOLS:
- fixtures (reusable call)
- test (shivammathur/setup-php@v2, build C ext, run both test targets)
DEPENDS: fixtures.yml
PATTERNS: reusable-workflow, c-extension-build, dual-implementation-test
USE WHEN: Changes to `php/` or `php/nxs_ext/` are pushed.

---

# python.yml

DOES: Tests pure-Python (`python test_nxs.py`) and C extension (`bash build_ext.sh && python test_c_ext.py`) with Python 3.13 against fixtures on push or PR to `py/`.
SYMBOLS:
- fixtures (reusable call)
- test (setup-python@v5, build C ext, run both test targets)
DEPENDS: fixtures.yml
PATTERNS: reusable-workflow, c-extension-build, dual-implementation-test
USE WHEN: Changes to `py/` are pushed.

---

# ruby.yml

DOES: Tests pure-Ruby (`ruby test.rb`) and C extension (`bash ext/build.sh && ruby bench_c.rb`) with Ruby 3.3 against fixtures on push or PR to `ruby/`.
SYMBOLS:
- fixtures (reusable call)
- test (ruby/setup-ruby@v1, build C ext, run both targets)
DEPENDS: fixtures.yml
PATTERNS: reusable-workflow, c-extension-build, dual-implementation-test
USE WHEN: Changes to `ruby/` or `ruby/ext/` are pushed.

---

# rust.yml

DOES: Runs `cargo test --release` and `cargo build --release --bin nxs`, then compiles all `examples/*.nxs` files with the built binary on every push or PR to `rust/` or `examples/`. Does not depend on fixtures.yml — Rust is the fixture generator.
SYMBOLS:
- test (dtolnay/rust-toolchain@stable, cargo test --release)
- build compiler (cargo build --release --bin nxs)
- compile examples (loop over examples/*.nxs)
DEPENDS: none
PATTERNS: cargo-release-build, example-compilation, self-contained
USE WHEN: Changes to `rust/` or `examples/` are pushed; this is the source-of-truth job that all other workflows depend on transitively.

---

# swift.yml

DOES: Runs `swift run nxs-test` on a macOS 15 runner against downloaded fixtures on push or PR to `swift/`.
SYMBOLS:
- fixtures (reusable call)
- test (macos-15 runner, swift run nxs-test)
DEPENDS: fixtures.yml
PATTERNS: reusable-workflow, macos-runner
USE WHEN: Changes to `swift/` are pushed; macOS runner required because Swift toolchain is not available on ubuntu in GitHub Actions without extra setup.
