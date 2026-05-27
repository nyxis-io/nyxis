import { createRouter, createWebHistory, type RouteRecordRaw } from "vue-router";
import { useHead } from "@unhead/vue";
import { GA_ID, usePageSeo, type PageSeo } from "./seo";
import { seoForPath } from "./manifest";

const HomeView = () => import("@/views/HomeView.vue");
const DocsView = () => import("@/views/DocsView.vue");
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
      seo: seoForPath("/"),
    } satisfies AppRouteMeta,
  },
  {
    path: "/docs/",
    name: "docs",
    component: DocsView,
    meta: {
      nav: "docs",
      footer: true,
      seo: seoForPath("/docs/"),
    } satisfies AppRouteMeta,
  },
  {
    path: "/use-cases/",
    name: "use-cases",
    component: UseCasesView,
    meta: {
      nav: "use-cases",
      seo: seoForPath("/use-cases/"),
    } satisfies AppRouteMeta,
  },
  {
    path: "/pricing/",
    name: "pricing",
    component: PricingView,
    meta: {
      nav: "pricing",
      seo: seoForPath("/pricing/"),
    } satisfies AppRouteMeta,
  },
  {
    path: "/bench/",
    name: "bench",
    component: BenchView,
    meta: {
      nav: "bench",
      footer: false,
      seo: seoForPath("/bench/"),
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
      seo: seoForPath("/demo/"),
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
      seo: seoForPath("/demo/ticker"),
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
      seo: seoForPath("/demo/workers"),
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
      seo: seoForPath("/demo/explorer"),
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
      seo: seoForPath("/demo/report"),
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
      seo: seoForPath("/demo/wal"),
    } satisfies AppRouteMeta,
  },
  // Legacy paths without trailing slash
  { path: "/docs", redirect: "/docs/" },
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
