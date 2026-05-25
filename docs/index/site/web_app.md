---
room: web_app
subdomain: site
source_paths: [site/web/, site/web/src/]
see_also: ["web_scripts.md", "demo_bench.md", "../js/reader.md"]
hot_paths: [site/web/src/main.ts, site/web/src/router/index.ts]
architectural_health: normal
security_tier: normal
---

# site/web/ — Vue Marketing Site & Demos

Subdomain: site/
Source paths: site/web/, site/web/src/

## TASK → LOAD

| Task | Load |
|------|------|
| Add a marketing route or SEO meta | web_app.md |
| Wire a new interactive demo page | web_app.md |
| Change global theme (light/dark) | web_app.md |

---

# site/web/src/App.vue

DOES: Root Vue shell mounting router-view, site chrome, and global styles.
SYMBOLS:
- App component template/script setup
PROPS: (none — root)

---

# site/web/src/agent/webmcp.ts

DOES: Initializes WebMCP agent bridge with Vue router for tool-discoverable demo navigation.
SYMBOLS:
- initWebMcp(router) → void
HOOKS: (registers MCP tools for routes)
DEPENDS: router/index.ts

---

# site/web/src/components/SiteFooter.vue

DOES: Site footer with links, license, and repository CTAs.
SYMBOLS:
- SiteFooter SFC

---

# site/web/src/components/SiteNav.vue

DOES: Top navigation bar with route links and theme toggle affordance.
SYMBOLS:
- SiteNav SFC
HOOKS: useTheme

---

# site/web/src/composables/useDemoPage.ts

DOES: Shared composable for demo pages: loads SDK, handles mount/teardown, error surfacing.
SYMBOLS:
- useDemoPage(demoId) → { ready, error, mount, unmount }
HOOKS: useDemoPage

---

# site/web/src/composables/useTheme.ts

DOES: Persists light/dark theme in localStorage and applies data-theme on documentElement.
SYMBOLS:
- initTheme() → void
- useTheme() → { theme, toggle }
HOOKS: useTheme
PATTERNS: local-storage-preference

---

# site/web/src/demos/bench-page.js

DOES: Demo script backing in-browser benchmark view; connects to site/bench workers.
SYMBOLS:
- initBenchPage(rootEl) → void

---

# site/web/src/demos/explorer-demo.js

DOES: Column explorer demo using explorer_worker.js and NXS reader WASM/JS.
SYMBOLS:
- initExplorerDemo(el) → void

---

# site/web/src/demos/report.js

DOES: Report demo visualizing aggregate stats over sample .nxb.
SYMBOLS:
- initReportDemo(el) → void

---

# site/web/src/demos/ticker-demo.js

DOES: High-frequency ticker stream demo over NXS records.
SYMBOLS:
- initTickerDemo(el) → void

---

# site/web/src/demos/wal-demo.js

DOES: WAL append/seal visualization demo using nxs WAL APIs in browser.
SYMBOLS:
- initWalDemo(el) → void

---

# site/web/src/demos/workers-demo.js

DOES: Spawns JSON vs NXS workers side-by-side to compare throughput in UI.
SYMBOLS:
- initWorkersDemo(el) → void
DEPENDS: workers/json_worker.js, workers/nxs_worker.js

---

# site/web/src/env.d.ts

DOES: Vite/Vue TypeScript ambient module declarations (.vue imports, import.meta.env).
SYMBOLS:
- (ambient types)

---

# site/web/src/main.ts

DOES: Vue app bootstrap: createApp, Unhead SEO, router, theme init, WebMCP init, mount #app.
SYMBOLS:
- (top-level bootstrap)
DEPENDS: App.vue, router, composables/useTheme, agent/webmcp

---

# site/web/src/router/index.ts

DOES: Vue Router history routes for marketing pages and /demo/* interactive views.
SYMBOLS:
- router instance, route table
ROUTES:
- GET / → HomeView
- GET /pricing → PricingView
- GET /use-cases → UseCasesView
- GET /bench → BenchView
- GET /demo → DemoIndexView
- (+demo child routes)
DEPENDS: views/*.vue

---

# site/web/src/router/seo.ts

DOES: Per-route Unhead meta (title, description, og tags) helpers.
SYMBOLS:
- routeSeoMeta(name) → HeadMeta object

---

# site/web/src/sdk.d.ts

DOES: Type declarations for browser NXS SDK globals used by demos.
SYMBOLS:
- NxsReader, NxsWriter interface stubs

---

# site/web/src/views/BenchView.vue

DOES: Page hosting in-browser benchmark UI (site/bench integration).
SYMBOLS:
- BenchView SFC
HOOKS: useDemoPage

---

# site/web/src/views/HomeView.vue

DOES: Marketing landing page hero, feature grid, and primary CTAs.
SYMBOLS:
- HomeView SFC

---

# site/web/src/views/PricingView.vue

DOES: Pricing tiers and enterprise contact section.
SYMBOLS:
- PricingView SFC

---

# site/web/src/views/UseCasesView.vue

DOES: Use-case narratives (analytics, edge, agent tooling).
SYMBOLS:
- UseCasesView SFC

---

# site/web/src/views/demo/DemoIndexView.vue

DOES: Index of interactive demos with cards linking to WAL, workers, explorer, etc.
SYMBOLS:
- DemoIndexView SFC

---

# site/web/src/views/demo/ExplorerView.vue

DOES: Vue shell for column explorer demo; loads explorer-demo.js on mount.
SYMBOLS:
- ExplorerView SFC

---

# site/web/src/views/demo/ReportView.vue

DOES: Vue shell for report aggregation demo.
SYMBOLS:
- ReportView SFC

---

# site/web/src/views/demo/TickerView.vue

DOES: Vue shell for ticker throughput demo.
SYMBOLS:
- TickerView SFC

---

# site/web/src/views/demo/WalView.vue

DOES: Vue shell for WAL visualization demo.
SYMBOLS:
- WalView SFC

---

# site/web/src/views/demo/WorkersView.vue

DOES: Vue shell for JSON vs NXS worker comparison demo.
SYMBOLS:
- WorkersView SFC

---

# site/web/src/workers/explorer_worker.js

DOES: Web Worker scanning NXB columns for explorer heatmap UI.
SYMBOLS:
- onmessage, column scan loops

---

# site/web/src/workers/json_worker.js

DOES: Web Worker baseline JSON parse/aggregate for workers demo.
SYMBOLS:
- onmessage JSON bench handlers

---

# site/web/src/workers/nxs_worker.js

DOES: Web Worker NXS decode/aggregate using JS reader for workers demo.
SYMBOLS:
- onmessage NXS bench handlers
DEPENDS: js/nxs.js (bundled)

---

# site/web/vite.config.ts

DOES: Vite config: Vue plugin, aliases, build targets for site and demo chunks.
SYMBOLS:
- defineConfig export with resolve.alias, build.rollupOptions
CONFIG: VITE_* env vars
