import { defineConfig, type Plugin } from "vite";
import vue from "@vitejs/plugin-vue";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const benchDir = resolve(__dirname, "../bench");
const benchWorkerSrc = resolve(benchDir, "bench-worker.js");

/**
 * Serve bench-worker.js in Vite dev/preview only.
 * Do not copy into dist/bench/ — that directory makes nginx serve /bench/ as a
 * folder listing (403) instead of the Vue SPA route.
 */
function benchWorkerPlugin(): Plugin {
  const serveWorker = (
    req: { url?: string },
    res: { setHeader: (k: string, v: string) => void; end: (b: Buffer) => void },
    next: () => void,
  ) => {
    if (req.url === "/bench/bench-worker.js" || req.url?.startsWith("/bench/bench-worker.js?")) {
      res.setHeader("Content-Type", "application/javascript");
      res.setHeader("Cross-Origin-Resource-Policy", "same-origin");
      res.end(readFileSync(benchWorkerSrc));
      return;
    }
    next();
  };
  return {
    name: "bench-worker-static",
    configureServer(server) {
      server.middlewares.use(serveWorker);
    },
    configurePreviewServer(server) {
      server.middlewares.use(serveWorker);
    },
  };
}

export default defineConfig({
  plugins: [vue(), benchWorkerPlugin()],
  resolve: {
    alias: {
      "@": resolve(__dirname, "src"),
      "@bench": resolve(__dirname, "../bench"),
    },
  },
  base: "/",
  server: {
    port: 5173,
    headers: {
      "Cross-Origin-Opener-Policy": "same-origin",
      "Cross-Origin-Embedder-Policy": "require-corp",
    },
    proxy: {
      "/sdk": {
        target: "http://127.0.0.1:8000",
        changeOrigin: true,
      },
      "/bench/fixtures": {
        target: "http://127.0.0.1:8000",
        changeOrigin: true,
      },
      "/examples": {
        target: "http://127.0.0.1:8000",
        changeOrigin: true,
      },
    },
  },
  worker: {
    format: "es",
    rollupOptions: {
      external: (id) => id.startsWith("/sdk/"),
    },
  },
  build: {
    outDir: resolve(__dirname, "../dist"),
    emptyOutDir: true,
    target: "es2022",
    rollupOptions: {
      external: (id) => id.startsWith("/sdk/"),
    },
  },
});
