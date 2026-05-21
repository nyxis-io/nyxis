# Browser demos (Nyxis core)

Interactive pages for WAL ingestion, workers, log explorer, report layout comparison, and live ticker. They load the MIT JavaScript reader from `/sdk/` (`nyxis-drivers/js`) and benchmark fixtures from `../bench/fixtures/`.

## Run locally

From `nyxis/` (requires [Docker](https://www.docker.com/) and the `shared_network` compose network, or adjust `docker-compose.yml`):

```bash
make sdk          # clone nyxis-drivers if ../nyxis-drivers/js is empty
make fixtures     # optional: benchmark datasets under site/bench/fixtures/
docker compose up
```

Open:

- http://localhost:8000/demo/ — home
- http://localhost:8000/demo/report.html — **your CSV** or **100k-row built-in samples** → row + columnar `.nxb`, chart from `col_buffer`
- http://localhost:8000/bench/ — browser benchmark charts

Static layout:

| URL path | Files |
|----------|--------|
| `/demo/` | `site/demo/` |
| `/bench/` | `site/bench/` |
| `/sdk/` | `../../nyxis-drivers/js/` (reader, writer, `wasm.js`) |

For COOP/COEP (SharedArrayBuffer), use nginx via compose — not plain `python3 -m http.server`.

**Report demo** (`report.html`) needs an up-to-date compile WASM with `compile_nxs_columnar`. After pulling compiler changes:

```bash
bash nyxis-drivers/js/build_compile_wasm.sh
```

Then reload http://localhost:8000/demo/report.html (hard refresh). Docker mounts `nyxis-drivers/js` as `/sdk/`.
