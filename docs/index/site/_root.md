---
room: _root
subdomain: site
see_also: ["../_root.md", "js/reader.md", "bench/_root.md"]
architectural_health: normal
security_tier: normal
---

# Site & Demo — Building Router

Subdomain: site/
Source paths: site/

## TASK → LOAD

| Task | Load |
|------|------|
| Change marketing pages, routing, or theme | web_app.md |
| Edit interactive NXS demos (WAL, workers, explorer) | web_app.md |
| Add WebMCP agent hooks for the Vue app | web_app.md |
| Run prerender, smoke tests, or content migration scripts | web_scripts.md |
| Serve demos with COOP/COEP for SharedArrayBuffer | demo_bench.md |
| Run in-browser WASM benchmark harness | demo_bench.md |

## Rooms

| Room | Source paths | Focus |
|------|-------------|-------|
| web_app.md | site/web/, site/web/src/ | Vue 3 + Vite marketing site and demos |
| web_scripts.md | site/web/scripts/ | Build-time markdown, prerender, smoke |
| demo_bench.md | site/demo/, site/bench/ | Python static server, browser bench |
