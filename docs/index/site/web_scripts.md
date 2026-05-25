---
room: web_scripts
subdomain: site
source_paths: [site/web/scripts/]
see_also: ["web_app.md", "../js/wasm_workers.md"]
architectural_health: normal
security_tier: normal
---

# site/web/scripts/ — Build & Content Tooling

Subdomain: site/
Source paths: site/web/scripts/

## TASK → LOAD

| Task | Load |
|------|------|
| Prerender static HTML for SEO | web_scripts.md |
| Generate per-page markdown from Vue routes | web_scripts.md |
| Smoke-test demo pages in CI | web_scripts.md |

---

# site/web/scripts/generate-agent-skills-index.mjs

DOES: Builds agent-skills index markdown consumed by the docs site from repository skill definitions.
SYMBOLS:
- main async flow, file writers

---

# site/web/scripts/generate-page-markdown.mjs

DOES: Walks Vue routes and emits static markdown snapshots for each marketing/demo page.
SYMBOLS:
- route → markdown generator

---

# site/web/scripts/migrate-from-html.mjs

DOES: One-shot migration helper converting legacy static HTML pages into Vue SFC structure.
SYMBOLS:
- parse HTML, emit Vue stubs

---

# site/web/scripts/prerender-static-html.mjs

DOES: Vite post-build prerender: visits each route with Puppeteer/playwright-less fetch and writes dist/*.html shells.
SYMBOLS:
- prerender(routes) → Promise
PATTERNS: static-prerender

---

# site/web/scripts/smoke-demos.mjs

DOES: CI smoke script hitting demo routes and asserting HTTP 200 + required DOM markers.
SYMBOLS:
- smoke(urls) → exit code
