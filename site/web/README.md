# Nyxis marketing site (Vue 3 + Vite)

The public site at [nyxis.io](https://nyxis.io) — landing pages, browser demos, and the interactive benchmark UI.

## Develop

From `nyxis/`:

```bash
make sdk          # once: clone nyxis-drivers for /sdk
make site-dev     # Vite on http://localhost:5173
```

Vite resolves `import "/sdk/…"` from `nyxis-drivers/js` directly. Keep something on port **8000** (e.g. `make demo` or the static server) so `/bench/fixtures` and `/examples` proxy correctly when you run benchmarks or load demo fixtures.

## Production build

```bash
make site-build   # writes site/dist/
make demo         # nginx serves site/dist + bench/fixtures + /sdk
```

Open http://localhost:8000/

## Layout

| Path | Source |
|------|--------|
| `/`, `/use-cases/`, `/pricing/`, `/demo/*`, `/bench/` | `site/web` → `site/dist` (Vue SPA) |
| `/bench/fixtures/` | `site/bench/fixtures/` |
| `/bench/wasm/` | `site/bench/wasm/` |
| `/sdk/` | `nyxis-drivers/js/` (compose mount) |

Demo logic lives under `src/demos/` (migrated from legacy inline `<script type="module">` blocks). The MIT reader stays external at `/sdk/*` at runtime.
