import { createRouter, createWebHistory, type RouteRecordRaw } from "vue-router";
import { useHead } from "@unhead/vue";
import { GA_ID, usePageSeo, type PageSeo } from "./seo";

const HomeView = () => import("@/views/HomeView.vue");
const UseCasesView = () => import("@/views/UseCasesView.vue");
const PricingView = () => import("@/views/PricingView.vue");
const BenchView = () => import("@/views/BenchView.vue");
const DemoIndexView = () => import("@/views/demo/DemoIndexView.vue");
const TickerView = () => import("@/views/demo/TickerView.vue");
const WorkersView = () => import("@/views/demo/WorkersView.vue");
const ExplorerView = () => import("@/views/demo/ExplorerView.vue");
const ReportView = () => import("@/views/demo/ReportView.vue");
const WalView = () => import("@/views/demo/WalView.vue");

type AppRouteMeta = {
  nav?: string;
  demoSubnav?: boolean;
  footer?: boolean;
  seo: PageSeo;
  chartJs?: boolean;
};

const routes: RouteRecordRaw[] = [
  {
    path: "/",
    name: "home",
    component: HomeView,
    meta: {
      nav: "home",
      footer: true,
      seo: {
        title: "Nyxis — Zero-Copy Binary Serialization (NXS)",
        description:
          "Nyxis is a bi-modal serialization format: .nxs source compiles to memory-mapped .nxb with row, columnar, and PAX layouts. Read one record without JSON.parse — open core, MIT drivers, MCP for agents.",
        canonical: "https://nyxis.io/",
      },
    } satisfies AppRouteMeta,
  },
  {
    path: "/use-cases/",
    name: "use-cases",
    component: UseCasesView,
    meta: {
      nav: "use-cases",
      seo: {
        title: "Nyxis — High-Throughput System Topologies & Use Cases",
        description:
          "Production topologies for the Nyxis zero-copy serialization protocol: mmap .nxb ingestion, streamable v1.2 sealing, row/columnar/PAX layouts, append-only WALs, Arrow bridges, and multi-terabyte data-grid deployments.",
        canonical: "https://nyxis.io/use-cases/",
      },
    } satisfies AppRouteMeta,
  },
  {
    path: "/pricing/",
    name: "pricing",
    component: PricingView,
    meta: {
      nav: "pricing",
      seo: {
        title: "Nyxis — Commercial Pricing",
        description:
          "Transparent Nyxis open-core pricing: free BSL production tier, Startup/Growth at $3,500/year, Enterprise Core at $15,000/year, and Principal custom quotes for hyper-scale workloads.",
        canonical: "https://nyxis.io/pricing/",
      },
    } satisfies AppRouteMeta,
  },
  {
    path: "/bench/",
    name: "bench",
    component: BenchView,
    meta: {
      nav: "bench",
      footer: false,
      seo: {
        title: "Benchmark — Nyxis NXS",
        description:
          "Interactive NXS vs JSON vs CSV benchmark charts at 1k–10M records in the browser, with optional fixture upload.",
        canonical: "https://nyxis.io/bench/",
      },
    } satisfies AppRouteMeta,
  },
  {
    path: "/demo/",
    name: "demo",
    component: DemoIndexView,
    meta: {
      nav: "demo",
      demoSubnav: true,
      footer: false,
      seo: {
        title: "Demo — Nyxis NXS",
        description:
          "Live browser demos: NXS vs JSON and CSV benchmarks, ticker, workers, explorer, WAL, and report layout — with SharedArrayBuffer and worker handoffs.",
        canonical: "https://nyxis.io/demo/",
      },
    } satisfies AppRouteMeta,
  },
  {
    path: "/demo/ticker",
    alias: "/demo/ticker.html",
    component: TickerView,
    meta: {
      nav: "ticker",
      demoSubnav: true,
      footer: false,
      seo: {
        title: "Ticker — Nyxis NXS",
        description:
          "Side-by-side JSON stringify/parse vs in-place float64 patch on mapped .nxb — frame timing and drop rate under pressure.",
        canonical: "https://nyxis.io/demo/ticker",
      },
    } satisfies AppRouteMeta,
  },
  {
    path: "/demo/workers",
    alias: "/demo/workers.html",
    component: WorkersView,
    meta: {
      nav: "workers",
      demoSubnav: true,
      footer: false,
      seo: {
        title: "Workers — Nyxis NXS",
        description:
          "Four Web Workers: JSON structured clone vs SharedArrayBuffer handoff for the same .nxb dataset.",
        canonical: "https://nyxis.io/demo/workers",
      },
    } satisfies AppRouteMeta,
  },
  {
    path: "/demo/explorer",
    alias: "/demo/explorer.html",
    component: ExplorerView,
    meta: {
      nav: "explorer",
      demoSubnav: true,
      footer: false,
      seo: {
        title: "Log explorer — Nyxis NXS",
        description: "Virtual scroll over millions of lines backed by mapped .nxb.",
        canonical: "https://nyxis.io/demo/explorer",
      },
    } satisfies AppRouteMeta,
  },
  {
    path: "/demo/report",
    alias: "/demo/report.html",
    component: ReportView,
    meta: {
      nav: "report",
      demoSubnav: true,
      footer: false,
      chartJs: true,
      seo: {
        title: "Report layout — Nyxis NXS",
        description: "CSV to row and columnar .nxb in the browser; chart from col_buffer.",
        canonical: "https://nyxis.io/demo/report",
      },
    } satisfies AppRouteMeta,
  },
  {
    path: "/demo/wal",
    alias: "/demo/wal.html",
    component: WalView,
    meta: {
      nav: "wal",
      demoSubnav: true,
      footer: false,
      seo: {
        title: "WAL / spans — Nyxis NXS",
        description: "OTel-style span ingestion — append-only WAL vs JSON payloads.",
        canonical: "https://nyxis.io/demo/wal",
      },
    } satisfies AppRouteMeta,
  },
  // Legacy paths without trailing slash
  { path: "/use-cases", redirect: "/use-cases/" },
  { path: "/pricing", redirect: "/pricing/" },
  { path: "/bench", redirect: "/bench/" },
  { path: "/demo", redirect: "/demo/" },
];

const router = createRouter({
  history: createWebHistory(),
  routes,
  scrollBehavior(to) {
    if (to.hash) return { el: to.hash, behavior: "smooth" };
    return { top: 0 };
  },
});

let gaLoaded = false;

function ensureGtag() {
  if (gaLoaded || typeof document === "undefined") return;
  gaLoaded = true;
  const s = document.createElement("script");
  s.async = true;
  s.src = `https://www.googletagmanager.com/gtag/js?id=${GA_ID}`;
  document.head.appendChild(s);
  window.dataLayer = window.dataLayer ?? [];
  window.gtag = function gtag(...args: unknown[]) {
    window.dataLayer?.push(args);
  };
  window.gtag("js", new Date());
  window.gtag("config", GA_ID);
}

router.beforeEach((to) => {
  const meta = to.meta as AppRouteMeta;
  if (meta.seo) useHead(usePageSeo(meta.seo));
  if (meta.chartJs) {
    useHead({
      script: [
        {
          src: "https://cdn.jsdelivr.net/npm/chart.js@4.4.1/dist/chart.umd.min.js",
          crossorigin: "anonymous",
        },
      ],
    });
  }
  return true;
});

router.afterEach(() => {
  ensureGtag();
});

export default router;
