/**
 * WebMCP tool registration for agent discovery (Chrome EPP / WebMCP spec).
 * Registers site navigation and documentation tools when the API is available.
 */
import type { Router } from "vue-router";

const NAV_ROUTES: Record<string, { title: string; description: string }> = {
  "/": { title: "Home", description: "Nyxis landing page with performance highlights and layout overview" },
  "/use-cases/": { title: "Use cases", description: "Production topologies and deployment scenarios" },
  "/pricing/": { title: "Pricing", description: "Commercial tiers and licensing" },
  "/bench/": { title: "Benchmark", description: "Interactive NXS vs JSON vs CSV browser benchmark" },
  "/demo/": { title: "Demos", description: "Live browser demos index" },
  "/demo/ticker": { title: "Ticker demo", description: "JSON re-parse vs in-place float64 patch" },
  "/demo/workers": { title: "Workers demo", description: "Structured clone vs SharedArrayBuffer handoff" },
  "/demo/explorer": { title: "Log explorer", description: "Virtual scroll over millions of .nxb-backed lines" },
  "/demo/report": { title: "Report demo", description: "CSV to row/columnar .nxb with chart rendering" },
  "/demo/wal": { title: "WAL demo", description: "OTel-style span ingestion comparison" },
};

interface ModelContextTool {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
  execute: (input: Record<string, unknown>, signal?: AbortSignal) => Promise<unknown>;
}

interface ModelContext {
  registerTool: (tool: ModelContextTool, options?: { signal?: AbortSignal }) => void;
}

declare global {
  interface Navigator {
    modelContext?: ModelContext;
  }
}

export function initWebMcp(router: Router): void {
  const mc = navigator.modelContext;
  if (!mc?.registerTool) return;

  const controller = new AbortController();
  const { signal } = controller;

  mc.registerTool(
    {
      name: "navigate",
      description: "Navigate to a Nyxis site page by route path",
      inputSchema: {
        type: "object",
        properties: {
          path: {
            type: "string",
            description: "Site path (e.g. /demo/, /use-cases/, /bench/)",
            enum: Object.keys(NAV_ROUTES),
          },
        },
        required: ["path"],
      },
      execute: async (input) => {
        const path = String(input.path ?? "/");
        const route = NAV_ROUTES[path];
        if (!route) {
          return { error: `Unknown path: ${path}`, available: Object.keys(NAV_ROUTES) };
        }
        await router.push(path);
        return { navigated: path, title: route.title };
      },
    },
    { signal },
  );

  mc.registerTool(
    {
      name: "list_routes",
      description: "List navigable Nyxis site routes with titles and descriptions",
      inputSchema: { type: "object", properties: {} },
      execute: async () => ({
        routes: Object.entries(NAV_ROUTES).map(([path, meta]) => ({ path, ...meta })),
      }),
    },
    { signal },
  );

  mc.registerTool(
    {
      name: "fetch_discovery",
      description: "Fetch a Nyxis agent discovery document (api-catalog, MCP server card, or agent skills index)",
      inputSchema: {
        type: "object",
        properties: {
          resource: {
            type: "string",
            enum: ["api-catalog", "mcp-server-card", "agent-skills", "health"],
          },
        },
        required: ["resource"],
      },
      execute: async (input) => {
        const paths: Record<string, string> = {
          "api-catalog": "/.well-known/api-catalog",
          "mcp-server-card": "/.well-known/mcp/server-card.json",
          "agent-skills": "/.well-known/agent-skills/index.json",
          health: "/.well-known/health",
        };
        const key = String(input.resource);
        const url = paths[key];
        const res = await fetch(url, { headers: { Accept: "application/json" } });
        if (!res.ok) return { error: `HTTP ${res.status}`, url };
        return { url, data: await res.json() };
      },
    },
    { signal },
  );

  window.addEventListener("pagehide", () => controller.abort(), { once: true });
}
