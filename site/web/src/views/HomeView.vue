<template>
<section class="landing-hero">
  <div class="hero-grid">
    <div class="hero-copy">
      <p class="page-eyebrow">NXS · zero-copy serialization</p>
      <h1 class="page-title">Read one record from a gigabyte file <em>without</em> parsing JSON</h1>
      <p class="page-lead">
        Nyxis is a bi-modal wire format for infrastructure: human-readable <code>.nxs</code> source compiles to
        memory-mapped <code>.nxb</code> binaries. Query by tail-index, decode aligned cells in place — alongside
        Protobuf, FlatBuffers, and Cap'n Proto, not application software.
      </p>
      <div class="landing-actions">
        <a class="btn btn-primary" href="/demo/">Try live demos</a>
        <a class="btn btn-secondary" href="/use-cases/">Production use cases</a>
        <a class="btn btn-secondary" href="https://github.com/nyxis-io/nyxis/blob/main/SPEC.md" rel="noopener">Specification</a>
      </div>
      <p class="hero-v8-link">
        <a href="/use-cases/#v8-limits">Why <code>JSON.parse()</code> fails on 512&nbsp;MB+ datasets →</a>
      </p>
    </div>

    <aside class="hero-visual" aria-label="Bimodal format preview">
      <div class="hero-visual__tabs">
        <span class="hero-visual__tab hero-visual__tab--active">.nxs source</span>
        <span class="hero-visual__tab">.nxb binary</span>
      </div>
      <pre class="hero-visual__body"><span class="key">user</span> {
  <span class="key">id</span> <span class="sigil">=</span> 42
  <span class="key">name</span> <span class="sigil">"</span>ada_lovelace"
  <span class="key">score</span> <span class="sigil">~</span> 98.6
  <span class="key">active</span> <span class="sigil">?</span> true
}</pre>
      <div class="hero-visual__arrow" aria-hidden="true">compile · mmap · seek</div>
      <pre class="hero-visual__body" style="color: var(--text); font-size: 11px;">NYXB │ schema │ records │ tail-index
       └─ O(1) record offset
       └─ zero-copy field reads</pre>
    </aside>
  </div>
</section>

<div class="landing-badges" aria-label="Project status">
  <a class="landing-badge" href="https://github.com/nyxis-io/nyxis/actions/workflows/rust.yml" rel="noopener" title="Rust CI on nyxis-io/nyxis">
    <span class="dot" aria-hidden="true"></span>
    <span class="label">Build</span>
    <span class="value">Passing</span>
  </a>
  <a class="landing-badge" href="https://github.com/nyxis-io/nyxis/actions/workflows/conformance.yml" rel="noopener" title="Cross-language conformance on nyxis-io/nyxis">
    <span class="dot" aria-hidden="true"></span>
    <span class="label">Conformance</span>
    <span class="value">10 languages</span>
  </a>
  <a class="landing-badge" href="https://github.com/nyxis-io/nyxis/blob/main/SPEC.md" rel="noopener" title="NXS binary specification">
    <span class="dot" aria-hidden="true"></span>
    <span class="label">Spec</span>
    <span class="value">v1.2.1</span>
  </a>
  <a class="landing-badge" href="https://github.com/nyxis-io/nyxis/tree/main/mcp" rel="noopener" title="MCP server for agent access">
    <span class="dot" aria-hidden="true"></span>
    <span class="label">MCP</span>
    <span class="value">Agent-native</span>
  </a>
</div>

<div class="stat-row">
  <div class="stat">
    <div class="value">7&nbsp;µs</div>
    <div class="label">Row time-to-first-record P50 — EPYC 9R14 streaming (<a href="../BENCHMARK.md#workload-d">D</a>)</div>
  </div>
  <div class="stat">
    <div class="value">1.3×</div>
    <div class="label">Columnar vs Arrow IPC — EPYC AVX-512 (<a href="../BENCHMARK.md#workload-c">C</a>)</div>
  </div>
  <div class="stat">
    <div class="value">&lt;1&nbsp;µs</div>
    <div class="label">Warm selective field read — all platforms (<a href="../BENCHMARK.md#workload-a">A</a>)</div>
  </div>
</div>
<p class="proof-strip">
  macOS Apple Silicon · Linux x86_64 (Haswell, AVX2) · AWS EPYC 9R14 (AVX-512) —
  <a href="../BENCHMARK.md#workload-comparison-suite">methodology &amp; full tables</a> ·
  <a href="/bench/">interactive charts</a>
</p>

<section class="landing-section" id="what-is-nyxis">
  <h2>What is Nyxis?</h2>
  <div class="what-is">
    <p class="what-is__lead">
      <strong>Nyxis moves parsing cost from query time to compile time.</strong>
      JSON and CSV force runtimes to materialize entire documents before a single field is useful.
      NXS writers emit structured bytes with interned field names, sparse presence bitmasks, and a footer tail-index —
      readers <code>mmap</code> the wire image and read by pointer.
    </p>
    <ul class="what-is__points">
      <li>
        <strong>Open core (BSL 1.1)</strong>
        Compiler, spec, conformance vectors, browser demos, and <code>nxs-mcp</code> for AI agents.
      </li>
      <li>
        <strong>MIT drivers</strong>
        Ten language SDKs in <code>nyxis-drivers</code> — embed in services without copyleft friction.
      </li>
      <li>
        <strong>Three binary layouts</strong>
        Row for streaming logs, columnar for analytics, PAX when you need both in one artifact.
      </li>
      <li>
        <strong>Streamable v1.2</strong>
        Emit records before the tail-index exists; seal when the segment completes, or use append-only WAL mode.
      </li>
    </ul>
  </div>
  <div class="format-pipeline">
    <span><strong>Edge / app</strong> writes <code>.nxs</code></span>
    <span class="pipe" aria-hidden="true">→</span>
    <span><strong>Compiler + drivers</strong> produce <code>.nxb</code></span>
    <span class="pipe" aria-hidden="true">→</span>
    <span><strong>BI / agents / browsers</strong> query without full decode</span>
  </div>
</section>

<section class="landing-section">
  <h2>Three layouts, three workloads</h2>
  <p class="section-intro">
    Pick <code>row</code>, <code>columnar</code>, or <code>pax</code> at compile time — not one layout for every problem.
    <a href="/use-cases/#bounds">Layout selection guide →</a>
  </p>
  <div class="layout-cards">
    <article class="layout-card">
      <div class="layout-card__icon">ROW</div>
      <h3>Row <code>.nxb</code></h3>
      <p class="layout-card__desc">
        Stream records as they arrive. O(1) seek via tail-index after seal. Sub-µs warm access to any field.
      </p>
      <p class="layout-card__metric">7&nbsp;µs TTFR P50 (EPYC) · 37&nbsp;µs Haswell</p>
      <p class="layout-card__use">Virtual scroll · log explorers · APM traces · ticker updates</p>
    </article>
    <article class="layout-card">
      <div class="layout-card__icon">COL</div>
      <h3>Columnar <code>.nxb</code></h3>
      <p class="layout-card__desc">
        Field buffers for charts and aggregates. No per-record traversal for column scans.
      </p>
      <p class="layout-card__metric">1.3× Arrow IPC on EPYC · 1.7× Apple Silicon</p>
      <p class="layout-card__use">
        OLAP · export pipelines ·
        <a href="/demo/report">CSV → chart demo</a>
      </p>
    </article>
    <article class="layout-card">
      <div class="layout-card__icon">PAX</div>
      <h3>PAX <code>.nxb</code></h3>
      <p class="layout-card__desc">
        Page-oriented hybrid: scroll rows and scan columns from one sealed file (SPEC §4.5).
      </p>
      <p class="layout-card__metric">
        <a href="../BENCHMARK.md#workload-e">Workload E</a> · page_size ≥ 32,768 on x86 server
      </p>
      <p class="layout-card__use">Dashboards that mix virtual scroll with column charts</p>
    </article>
  </div>
</section>

<section class="landing-section">
  <h2>Sigil-typed source</h2>
  <pre class="code-block"><span class="key">user</span> {
  <span class="key">id</span> <span class="sigil">=</span> 42
  <span class="key">name</span> <span class="sigil">"</span>ada_lovelace"
  <span class="key">score</span> <span class="sigil">~</span> 98.6
  <span class="key">active</span> <span class="sigil">?</span> true
}</pre>
  <p class="section-intro" style="margin-top: 16px; margin-bottom: 0;">
    Every value in <code>.nxs</code> carries a sigil that declares its binary encoding. The source file is the schema —
    no separate IDL. Compile once; warehouses and UIs read aligned cells for the lifetime of the payload.
  </p>
</section>

<section class="landing-section">
  <h2>Built for these jobs</h2>
  <ul class="pillars">
    <li>
      <strong>Fast</strong>
      <span>8-byte aligned atomic cells; zero-copy reads without deserialization.</span>
    </li>
    <li>
      <strong>Sparse</strong>
      <span>LEB128 presence bitmasks — absent fields cost nothing in the record body.</span>
    </li>
    <li>
      <strong>Compact</strong>
      <span>Interned field dictionary; records store 2-byte slot indices, not repeated key strings.</span>
    </li>
    <li>
      <strong>Human-readable</strong>
      <span><code>.nxs</code> is plain text you can diff, review, and hand-edit.</span>
    </li>
    <li>
      <strong>Streamable</strong>
      <span>Writers emit before the tail-index exists; WAL mode seals to indexed <code>.nxb</code> on demand.</span>
    </li>
    <li>
      <strong>AI-native</strong>
      <span><code>nxs-mcp</code> exposes typed tools so agents query <code>.nxb</code> without custom parsers.</span>
    </li>
  </ul>
</section>

<section class="landing-section" id="streaming">
  <h2>Stream, then seal</h2>
  <p class="section-intro">
    Producers emit aligned <code>.nxb</code> bytes while a segment is still open. Readers parse complete records as they
    arrive and only need the footer tail pointer once the writer seals the file.
  </p>
  <ul class="pillars" style="margin-top: 0;">
    <li>
      <strong>Streamable <code>.nxb</code></strong>
      <span>Preamble <code>TailPtr = 0</code> during ingest; sealing writes <code>FooterTailPtr</code> + <code>MagicFooter</code> at EOF.</span>
    </li>
    <li>
      <strong>Append-only WAL (<code>.nxsw</code>)</strong>
      <span>Hot paths append NYXO rows without rewriting the tail-index every span.</span>
    </li>
    <li>
      <strong>Incremental readers</strong>
      <span>MIT drivers expose stream parsers so browsers consume records before the download finishes.</span>
    </li>
  </ul>
  <div class="landing-actions" style="justify-content: flex-start; margin-top: 20px;">
    <a class="btn btn-secondary" href="/demo/wal">WAL / spans demo</a>
    <a class="btn btn-secondary" href="/use-cases/#streaming">Streaming use cases</a>
  </div>
</section>

<section class="landing-section">
  <h2>Honest benchmarks</h2>
  <p class="section-intro" style="margin-bottom: 0;">
    Five workloads (A–E) vs Protobuf, FlatBuffers, Cap'n Proto, and Apache Arrow — with published losses where NXS is not
    the winner. Full methodology: <a href="../BENCHMARK.md#workload-comparison-suite">BENCHMARK.md</a>.
    <a href="/bench/">Interactive browser charts</a>.
  </p>
</section>

<section class="landing-section">
  <h2>Try it in the browser</h2>
  <ul class="card-list">
    <li>
      <a href="/demo/">
        <div class="title">All demos</div>
        <div class="desc">Ticker, workers, log explorer, WAL, report — NXS vs JSON side by side.</div>
      </a>
    </li>
    <li>
      <a href="/demo/ticker">
        <div class="title">Ticker</div>
        <div class="desc">Per-frame JSON re-parse vs in-place <code>float64</code> patch on mapped bytes.</div>
      </a>
    </li>
    <li>
      <a href="/demo/explorer">
        <div class="title">Log explorer</div>
        <div class="desc">Virtual scroll over millions of lines backed by <code>.nxb</code>.</div>
      </a>
    </li>
    <li>
      <a href="/demo/workers">
        <div class="title">Workers</div>
        <div class="desc">Structured clone vs <code>SharedArrayBuffer</code> handoff across four workers.</div>
      </a>
    </li>
    <li>
      <a href="/demo/wal">
        <div class="title">WAL / spans</div>
        <div class="desc">OTel-style span ingest — append-only WAL vs JSON payloads.</div>
      </a>
    </li>
    <li>
      <a href="/bench/">
        <div class="title">Interactive bench</div>
        <div class="desc">Open, random access, aggregates at 1k–10M records.</div>
      </a>
    </li>
  </ul>
</section>

<section class="landing-section enterprise-teaser">
  <h2>Enterprise extensions</h2>
  <p class="section-intro">
    Core compiler and ten MIT SDKs are free for production within BSL limits. Closed-source extensions add multi-terabyte
    compaction, Arrow bridges, schema registry, and encrypt-at-rest operations.
  </p>
  <ul class="enterprise-modules">
    <li>
      <strong><code>nxs-compactd</code> — in-memory compactor</strong>
      <span>Reclaims data-sector slack from append-heavy WAL segments without racing active writers.</span>
    </li>
    <li>
      <strong>Apache Arrow zero-copy bridge</strong>
      <span>Stream payloads into Polars, DuckDB, Snowflake, and Tableau via the C Data Interface.</span>
    </li>
    <li>
      <strong><code>nxs-registryd</code> — schema registry</strong>
      <span>Centralized gRPC contract management with additive-only drift resolution.</span>
    </li>
  </ul>
  <div class="landing-actions" style="justify-content: flex-start; margin-top: 8px;">
    <a class="btn btn-primary" href="/use-cases/">Enterprise use cases</a>
    <a class="btn btn-secondary" href="/pricing/">Commercial pricing</a>
  </div>
  <p class="enterprise-footnote">
    Core: <a href="https://github.com/nyxis-io/nyxis/blob/main/LICENSE" rel="noopener">BSL 1.1</a>
    (free under $5M revenue / 10&nbsp;TB per month).
    SDKs: <a href="https://github.com/nyxis-io/nyxis-drivers" rel="noopener">MIT</a>.
  </p>
</section>
</template>

<script setup lang="ts">
</script>

<style scoped>
.hero-visual__body .key {
  color: var(--accent-secondary);
}
.hero-visual__body .sigil {
  color: var(--warm);
  font-weight: 600;
}
</style>
