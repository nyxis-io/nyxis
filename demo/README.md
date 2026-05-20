# Browser demos (Nyxis core)

Interactive pages for WAL ingestion, workers, log explorer, and live ticker. They load the MIT JavaScript reader from `/sdk/` (`nyxis-drivers/js`) and benchmark fixtures from `../bench/fixtures/`.

## Run locally

From `nyxis/` (requires [Docker](https://www.docker.com/) and the `shared_network` compose network, or adjust `docker-compose.yml`):

```bash
make sdk          # clone nyxis-drivers if ../nyxis-drivers/js is empty
make fixtures     # optional: benchmark datasets under bench/fixtures/
docker compose up
```

Open:

- http://localhost:8000/demo/ — home
- http://localhost:8000/bench/ — browser benchmark charts

Static layout:

| URL path | Files |
|----------|--------|
| `/demo/` | This directory |
| `/bench/` | `../bench/` |
| `/sdk/` | `../../nyxis-drivers/js/` (reader, writer, `wasm.js`) |

For COOP/COEP (SharedArrayBuffer), use nginx via compose — not plain `python3 -m http.server`.
