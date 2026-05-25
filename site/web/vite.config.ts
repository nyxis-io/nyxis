import { defineConfig, type Plugin } from "vite";
import vue from "@vitejs/plugin-vue";
import { readFileSync, statSync } from "node:fs";
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
  plugins: [vue(), sdkDevResolvePlugin(), benchWorkerPlugin(), agentDiscoveryPlugin()],
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
