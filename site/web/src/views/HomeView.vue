<template>
<section class="landing-hero">
  <div class="hero-grid">
    <div class="hero-copy">
      <p class="page-eyebrow">Browser-scale structured data</p>
      <h1 class="page-title">Open massive structured datasets instantly in the browser</h1>
      <p class="page-lead">
        Stream, filter, and explore GB-scale structured data without JSON hydration bottlenecks, browser freezes,
        or memory blowups.
      </p>
      <div class="landing-actions">
        <a class="btn btn-primary" href="/demo/explorer">Try the Explorer</a>
        <a class="btn btn-secondary" href="/bench/">See Benchmarks</a>
        <a class="btn btn-secondary" href="/docs/">Quickstart</a>
      </div>
    </div>

    <aside class="hero-visual" aria-label="Workflow comparison">
      <div class="workflow-compare">
        <div class="workflow-compare__col workflow-compare__col--slow">
          <span class="workflow-compare__label">Typical JSON workflow</span>
          <span class="workflow-compare__metric">12s+ before interaction</span>
          <span class="workflow-compare__detail">Full parse · heap inflation · UI freeze</span>
        </div>
        <div class="workflow-compare__vs" aria-hidden="true">vs</div>
        <div class="workflow-compare__col workflow-compare__col--fast">
          <span class="workflow-compare__label">Nyxis in the browser</span>
          <span class="workflow-compare__metric">Interactive immediately</span>
          <span class="workflow-compare__detail">Stream · selective reads · virtual scroll</span>
        </div>
      </div>
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

<section class="landing-section explorer-proof" id="explorer">
  <h2>See it work — log explorer</h2>
  <p class="section-intro">
    Scroll, search, and jump across millions of rows in the browser. The Explorer is the fastest way to feel the
    difference — no install, built-in fixtures up to 10M records.
  </p>
  <ul class="explorer-proof__labels" aria-label="What the demo proves">
    <li><strong>1M+ records</strong> in-browser</li>
    <li><strong>Fully browser-side</strong> — no server filter pass</li>
    <li><strong>No hydration pass</strong> — mapped <code>.nxb</code> bytes</li>
  </ul>
  <div class="explorer-proof__actions">
    <a class="btn btn-primary" href="/demo/explorer">Open the Explorer</a>
    <a class="btn btn-secondary" href="/demo/workers">Workers handoff demo</a>
  </div>
</section>

<div class="stat-row stat-row--workflow">
  <div class="stat">
    <div class="value">Immediate</div>
    <div class="label">Time to first scroll on streamed <code>.nxb</code> — before download finishes</div>
  </div>
  <div class="stat">
    <div class="value">142&nbsp;µs</div>
    <div class="label">Time to first record P50 — streamable row layout (<a href="../BENCHMARK.md#workload-d">Workload D</a>)</div>
  </div>
  <div class="stat">
    <div class="value">~0&nbsp;MB</div>
    <div class="label">Extra heap for multi-GB files — viewport-only decode vs full JSON graph</div>
  </div>
</div>
<p class="proof-strip">
  <a href="/bench/">Interactive browser benchmarks</a> ·
  <a href="../BENCHMARK.md#workload-comparison-suite">full methodology</a> ·
  <a href="/use-cases/#v8-limits">why JSON breaks at scale</a>
</p>

<section class="landing-section" id="why-json-breaks">
  <h2>Why JSON breaks in the browser</h2>
  <p class="section-intro">
    Modern observability and ops UIs ship multi-hundred-megabyte JSON exports. The pain is familiar before any wire
    format debate.
  </p>
  <ul class="pain-grid">
    <li>
      <strong>V8 string limits</strong>
      <span>Single strings above ~512&nbsp;MB–1&nbsp;GB throw <code>RangeError</code> before <code>JSON.parse()</code> runs.</span>
    </li>
    <li>
      <strong>AST memory inflation</strong>
      <span>A 100&nbsp;MB file can expand to 500&nbsp;MB+ of live objects — GC pauses lock the main thread.</span>
    </li>
    <li>
      <strong>Hydration overhead</strong>
      <span>Every field becomes a JS object before the grid can render one row.</span>
    </li>
    <li>
      <strong>Memory pressure</strong>
      <span>Virtual scroll on a parsed array still holds the entire graph in RAM.</span>
    </li>
    <li>
      <strong>Silent integer truncation</strong>
      <span>IDs above <code>2^53−1</code> lose precision with no error — only wrong analytics.</span>
    </li>
  </ul>
  <p class="section-intro section-intro--tail">
    <a href="/use-cases/#v8-limits">Full constraint breakdown →</a>
  </p>
</section>

<section class="landing-section">
  <h2>Outcome comparison</h2>
  <p class="section-intro">Compare what users experience — not which ecosystem you must rip out.</p>
  <table class="parity-matrix">
    <thead>
      <tr>
        <th scope="col">Workflow moment</th>
        <th scope="col">JSON in the browser</th>
        <th scope="col">Nyxis <code>.nxb</code></th>
      </tr>
    </thead>
    <tbody>
      <tr>
        <td>Open a large export</td>
        <td class="col-bad">Wait for full download + parse</td>
        <td class="col-good">Header + tail-index — stream rows as bytes arrive</td>
      </tr>
      <tr>
        <td>First interaction</td>
        <td class="col-bad">Often 10–15+ seconds on 100&nbsp;MB+</td>
        <td class="col-good">Scroll and search while the file is still loading</td>
      </tr>
      <tr>
        <td>Filter / search</td>
        <td class="col-bad">Scan in-memory object graph</td>
        <td class="col-good">Selective field reads + worker-backed index</td>
      </tr>
      <tr>
        <td>Memory at 1M+ rows</td>
        <td class="col-bad">Full dataset on the heap</td>
        <td class="col-good">Viewport pool — only visible rows decoded</td>
      </tr>
    </tbody>
  </table>
</section>

<section class="landing-section" id="what-is-nyxis">
  <h2>How Nyxis solves it</h2>
  <div class="what-is">
    <p class="what-is__lead">
      <strong>Nyxis moves parsing cost from query time to compile time.</strong>
      Human-readable <code>.nxs</code> compiles to memory-mapped <code>.nxb</code> with a tail-index for O(1) record
      seeks — readers use zero-copy selective access instead of materializing entire documents.
    </p>
    <ul class="what-is__points">
      <li>
        <strong>Zero-copy reads</strong>
        <code>mmap</code>-friendly wire cells; decode only the fields you touch.
      </li>
      <li>
        <strong>Selective access</strong>
        Tail-index jumps for virtual scroll, search, and agent field extraction.
      </li>
      <li>
        <strong>Streamable v1.2</strong>
        Emit records before the footer seals; WAL mode for append-heavy ingest.
      </li>
      <li>
        <strong>Bimodal workflow</strong>
        Git-diffable <code>.nxs</code> source; production <code>.nxb</code> for browsers and services.
      </li>
    </ul>
  </div>
  <div class="format-pipeline">
    <span><strong>Edge / app</strong> writes <code>.nxs</code></span>
    <span class="pipe" aria-hidden="true">→</span>
    <span><strong>Compiler + drivers</strong> produce <code>.nxb</code></span>
    <span class="pipe" aria-hidden="true">→</span>
    <span><strong>Browser / BI / agents</strong> query without full decode</span>
  </div>
</section>

<section class="landing-section">
  <h2>Three layouts, three workloads</h2>
  <p class="section-intro">
    Pick <code>row</code>, <code>columnar</code>, or <code>pax</code> at compile time — not one layout for every problem.
    <a href="/use-cases/#layout-selection">Layout selection guide →</a>
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
  <p class="section-intro section-intro--after-code">
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
  <p class="section-intro section-intro--tail">
    Workflow metrics first — time to interactive, filter latency, browser memory — then raw throughput vs Protobuf,
    FlatBuffers, Cap'n Proto, and Arrow. We publish losses where NXS is not the winner.
    <a href="/bench/">Interactive charts</a> ·
    <a href="../BENCHMARK.md#workload-comparison-suite">BENCHMARK.md</a>.
  </p>
</section>

<section class="landing-section">
  <h2>More demos</h2>
  <ul class="card-list">
    <li>
      <a href="/demo/ticker">
        <div class="title">Ticker</div>
        <div class="desc">Per-frame JSON re-parse vs in-place <code>float64</code> patch on mapped bytes.</div>
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
      <a href="/demo/">
        <div class="title">All demos</div>
        <div class="desc">Full catalog with COOP/COEP for production-like worker behavior.</div>
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
    <a class="btn btn-primary" href="/use-cases/">Production topologies</a>
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
