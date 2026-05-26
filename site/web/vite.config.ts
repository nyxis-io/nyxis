import { defineConfig, type Plugin } from "vite";
import vue from "@vitejs/plugin-vue";
import { createReadStream, readFileSync, statSync } from "node:fs";
import type { IncomingMessage, ServerResponse } from "node:http";
import { resolve } from "node:path";

const benchDir = resolve(__dirname, "../bench");
const benchWorkerSrc = resolve(benchDir, "bench-worker.js");
const publicDir = resolve(__dirname, "public");
const webRoot = resolve(__dirname, ".");
/** MIT reader served at /sdk/ in production; resolved from nyxis-drivers in Vite dev. */
const sdkDir = resolve(__dirname, "../../../nyxis-drivers/js");

function sdkExists(): boolean {
  try {
    statSync(resolve(sdkDir, "nxs.js"));
    return true;
  } catch {
    return false;
  }
}

/**
 * Map `import … from "/sdk/…"` to nyxis-drivers/js so Vite dev does not fail import-analysis.
 * Production build keeps /sdk/* external (nginx or compose serves the same URLs).
 */
const SDK_PREFIX = "/sdk/";
const SDK_JS_TYPES: Record<string, string> = {
  ".js": "application/javascript",
  ".mjs": "application/javascript",
  ".wasm": "application/wasm",
  ".json": "application/json",
};

/**
 * Serve MIT SDK files at /sdk/* in dev/preview (workers import by URL).
 */
function sdkStaticPlugin(): Plugin {
  const serveSdk = (
    req: IncomingMessage,
    res: ServerResponse,
    next: () => void,
  ): void => {
    const url = req.url?.split("?")[0] ?? "";
    if (!url.startsWith(SDK_PREFIX)) {
      next();
      return;
    }
    const rel = decodeURIComponent(url.slice(SDK_PREFIX.length));
    if (!rel || rel.includes("..") || rel.includes("/") || rel.includes("\\")) {
      next();
      return;
    }
    const filePath = resolve(sdkDir, rel);
    try {
      const st = statSync(filePath);
      if (!st.isFile()) {
        next();
        return;
      }
    } catch {
      next();
      return;
    }
    const ext = rel.includes(".") ? rel.slice(rel.lastIndexOf(".")) : "";
    res.statusCode = 200;
    res.setHeader("Content-Type", SDK_JS_TYPES[ext] ?? "application/octet-stream");
    res.setHeader("Cross-Origin-Resource-Policy", "same-origin");
    res.setHeader("Cache-Control", "no-cache");
    if (req.method === "HEAD") {
      res.setHeader("Content-Length", String(statSync(filePath).size));
      res.end();
      return;
    }
    if (req.method !== "GET") {
      res.statusCode = 405;
      res.end();
      return;
    }
    createReadStream(filePath).pipe(res);
  };
  return {
    name: "sdk-static-dev",
    configureServer(server) {
      server.middlewares.use(serveSdk);
    },
    configurePreviewServer(server) {
      server.middlewares.use(serveSdk);
    },
  };
}

function sdkDevResolvePlugin(): Plugin {
  return {
    name: "sdk-dev-resolve",
    resolveId(id) {
      if (!id.startsWith("/sdk/")) return null;
      const rel = id.slice("/sdk/".length);
      const file = resolve(sdkDir, rel);
      try {
        statSync(file);
        return file;
      } catch {
        return null;
      }
    },
  };
}

type StaticRoute = {
  path: string;
  markdown: string;
  title: string;
  description: string;
  interactive?: boolean;
};

const staticRoutes: StaticRoute[] = JSON.parse(
  readFileSync(resolve(webRoot, "content/routes.json"), "utf8"),
);
const markdownByPath = new Map(staticRoutes.map((r) => [r.path, r.markdown]));

const AGENT_LINK_HEADER =
  '</.well-known/api-catalog>; rel="api-catalog", ' +
  '</.well-known/agent-skills/index.json>; rel="describedby", ' +
  '</.well-known/mcp/server-card.json>; rel="service-meta", ' +
  '<https://github.com/nyxis-io/nyxis/blob/main/SPEC.md>; rel="service-desc", ' +
  '<https://github.com/nyxis-io/nyxis/blob/main/GETTING_STARTED.md>; rel="service-doc"';

const WELL_KNOWN_TYPES: Record<string, string> = {
  "/.well-known/api-catalog": 'application/linkset+json; profile="https://www.rfc-editor.org/info/rfc9727"',
  "/.well-known/oauth-authorization-server": "application/json",
  "/.well-known/oauth-protected-resource": "application/json",
  "/.well-known/health": "application/json",
};

function approxMarkdownTokens(text: string): number {
  return Math.ceil(text.split(/\s+/).filter(Boolean).length * 1.33);
}

function resolvePublicFile(url: string): string | null {
  const filePath = resolve(publicDir, url.slice(1));
  try {
    statSync(filePath);
    return filePath;
  } catch {
    return null;
  }
}

function serveMarkdown(
  res: {
    statusCode: number;
    setHeader: (k: string, v: string) => void;
    end: (b: string) => void;
  },
  mdPath: string,
  linkHeader = false,
): void {
  const body = readFileSync(mdPath, "utf8");
  res.statusCode = 200;
  res.setHeader("Content-Type", "text/markdown");
  res.setHeader("x-markdown-tokens", String(approxMarkdownTokens(body)));
  if (linkHeader) res.setHeader("Link", AGENT_LINK_HEADER);
  res.end(body);
}

function agentDiscoveryMiddleware(
  req: { url?: string; headers: { accept?: string } },
  res: {
    statusCode: number;
    setHeader: (k: string, v: string) => void;
    end: (b: string) => void;
  },
  next: () => void,
): void {
  const url = req.url?.split("?")[0] ?? "";
  const accept = req.headers.accept ?? "";

  if (url.endsWith(".md")) {
    const mdPath = resolvePublicFile(url);
    if (mdPath) {
      serveMarkdown(res, mdPath, url === "/index.md");
      return;
    }
  }

  const mdRoute = markdownByPath.get(url);
  if (mdRoute && accept.includes("text/markdown")) {
    const mdPath = resolvePublicFile(mdRoute);
    if (mdPath) {
      serveMarkdown(res, mdPath, url === "/");
      return;
    }
  }

  const contentType = WELL_KNOWN_TYPES[url];
  if (contentType) {
    const filePath = resolvePublicFile(url);
    if (filePath) {
      res.statusCode = 200;
      res.setHeader("Content-Type", contentType);
      res.end(readFileSync(filePath, "utf8"));
      return;
    }
  }

  if (url === "/") {
    res.setHeader("Link", AGENT_LINK_HEADER);
  }

  next();
}

/**
 * Agent discovery: Link headers, markdown negotiation, well-known content types.
 */
function agentDiscoveryPlugin(): Plugin {
  return {
    name: "agent-discovery",
    configureServer(server) {
      server.middlewares.use(agentDiscoveryMiddleware);
    },
    configurePreviewServer(server) {
      server.middlewares.use(agentDiscoveryMiddleware);
    },
  };
}

/**
 * Serve bench-worker.js in Vite dev/preview only.
 * Do not copy into dist/bench/ — that directory makes nginx serve /bench/ as a
 * folder listing (403) instead of the Vue SPA route.
 */
const BENCH_FIXTURES_PREFIX = "/bench/fixtures/";

/**
 * Serve bench .nxb/.json fixtures from disk in dev/preview (HEAD + Range).
 * Replaces the nginx proxy on :8000 so `npm run dev` alone works for the explorer.
 */
function benchFixturesPlugin(): Plugin {
  const fixturesDir = resolve(benchDir, "fixtures");

  const serveFixture = (
    req: IncomingMessage,
    res: ServerResponse,
    next: () => void,
  ): void => {
    const url = req.url?.split("?")[0] ?? "";
    if (!url.startsWith(BENCH_FIXTURES_PREFIX)) {
      next();
      return;
    }
    const name = decodeURIComponent(url.slice(BENCH_FIXTURES_PREFIX.length));
    if (!name || name.includes("..") || name.includes("/") || name.includes("\\")) {
      next();
      return;
    }
    const filePath = resolve(fixturesDir, name);
    let size: number;
    try {
      const st = statSync(filePath);
      if (!st.isFile()) {
        next();
        return;
      }
      size = st.size;
    } catch {
      next();
      return;
    }

    const baseHeaders: Record<string, string> = {
      "Content-Type": "application/octet-stream",
      "Accept-Ranges": "bytes",
      "Cache-Control": "no-cache",
      "Cross-Origin-Resource-Policy": "same-origin",
    };

    if (req.method === "HEAD") {
      res.statusCode = 200;
      for (const [k, v] of Object.entries(baseHeaders)) res.setHeader(k, v);
      res.setHeader("Content-Length", String(size));
      res.end();
      return;
    }
    if (req.method !== "GET") {
      res.statusCode = 405;
      res.end();
      return;
    }

    const range = req.headers.range;
    if (range) {
      const m = /^bytes=(\d*)-(\d*)/.exec(range);
      if (!m) {
        res.statusCode = 416;
        res.setHeader("Content-Range", `bytes */${size}`);
        res.end();
        return;
      }
      let start = m[1] !== "" ? parseInt(m[1], 10) : 0;
      let end = m[2] !== "" ? parseInt(m[2], 10) : size - 1;
      if (
        Number.isNaN(start) ||
        Number.isNaN(end) ||
        start > end ||
        start >= size
      ) {
        res.statusCode = 416;
        res.setHeader("Content-Range", `bytes */${size}`);
        res.end();
        return;
      }
      end = Math.min(end, size - 1);
      const len = end - start + 1;
      res.statusCode = 206;
      for (const [k, v] of Object.entries(baseHeaders)) res.setHeader(k, v);
      res.setHeader("Content-Range", `bytes ${start}-${end}/${size}`);
      res.setHeader("Content-Length", String(len));
      createReadStream(filePath, { start, end }).pipe(res);
      return;
    }

    res.statusCode = 200;
    for (const [k, v] of Object.entries(baseHeaders)) res.setHeader(k, v);
    res.setHeader("Content-Length", String(size));
    createReadStream(filePath).pipe(res);
  };

  return {
    name: "bench-fixtures-dev",
    configureServer(server) {
      server.middlewares.use(serveFixture);
    },
    configurePreviewServer(server) {
      server.middlewares.use(serveFixture);
    },
  };
}

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

if (!sdkExists()) {
  console.warn(
    "[vite] nyxis-drivers/js not found at",
    sdkDir,
    "— run `make sdk` from nyxis/ or clone nyxis-drivers; /sdk imports will fail in dev.",
  );
}

export default defineConfig({
  plugins: [
    vue(),
    sdkDevResolvePlugin(),
    sdkStaticPlugin(),
    benchFixturesPlugin(),
    benchWorkerPlugin(),
    agentDiscoveryPlugin(),
  ],
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
      // /sdk and /bench/fixtures served by dev plugins (local files + Range)
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
