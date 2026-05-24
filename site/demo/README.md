# Browser demos (Nyxis core)

Interactive pages for WAL ingestion, workers, log explorer, report layout comparison, and live ticker. They load the MIT JavaScript reader from `/sdk/` (`nyxis-drivers/js`) and benchmark fixtures from `/bench/fixtures/`.

The UI is a **Vue 3 SPA** under [`../web/`](../web/README.md). Legacy static HTML has been removed; build with `make site-build` before `make demo`.

## Run locally

From `nyxis/` (requires [Docker](https://www.docker.com/) and the `shared_network` compose network, or adjust `docker-compose.yml`):

```bash
make sdk          # clone nyxis-drivers if ../nyxis-drivers/js is empty
make fixtures     # optional: benchmark datasets under site/bench/fixtures/
make demo         # site-build + docker compose up
```

Open:

- http://localhost:8000/ — landing
- http://localhost:8000/demo/report — **your CSV** or **100k-row built-in samples** → row + columnar `.nxb`, chart from `col_buffer`
- http://localhost:8000/bench/ — browser benchmark charts

For local UI development without rebuilding Docker on every change:

```bash
make site-dev     # http://localhost:5173 (proxies /sdk and fixtures to :8000)
```

Static layout:

| URL path | Files |
|----------|--------|
| `/`, `/demo/*`, `/bench/` | `site/web` → `site/dist` |
| `/bench/fixtures/` | `site/bench/fixtures/` |
| `/bench/wasm/` | `site/bench/wasm/` |
| `/bench/bench-worker.js` | `site/bench/bench-worker.js` |
| `/sdk/` | `../../nyxis-drivers/js/` (reader, writer, `wasm.js`) |

For COOP/COEP (SharedArrayBuffer), use nginx via compose — not plain `python3 -m http.server`.

**Report demo** (`/demo/report`) needs an up-to-date compile WASM with `compile_nxs_columnar`. After pulling compiler changes:

```bash
bash nyxis-drivers/js/build_compile_wasm.sh
```

Then reload http://localhost:8000/demo/report (hard refresh). Docker mounts `nyxis-drivers/js` as `/sdk/`.
