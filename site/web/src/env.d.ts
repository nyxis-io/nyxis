/// <reference types="vite/client" />

declare module "*.vue" {
  import type { DefineComponent } from "vue";
  const component: DefineComponent<object, object, unknown>;
  export default component;
}

interface Window {
  dataLayer?: unknown[];
  gtag?: (...args: unknown[]) => void;
}

declare const Chart: typeof import("chart.js").Chart;

declare module "@/demos/bench-page" {
  export function wireBenchPage(root: Element): Promise<void>;
  export function unwireBenchPage(): void;
}

declare module "@/demos/explorer-demo" {
  export function wireExplorerPage(root: Element): void;
  export function unwireExplorerPage(): void;
}

declare module "@/demos/report" {
  export function wireReportPage(root?: Element | Document): void;
}

declare module "@/demos/ticker-demo" {
  export function wireTickerPage(root: Element): void;
  export function unwireTickerPage(): void;
}

declare module "@/demos/wal-demo" {
  export function wireWalPage(root: Element): Promise<void>;
  export function unwireWalPage(): void;
}

declare module "@/demos/workers-demo" {
  export function wireWorkersPage(root: Element): void;
  export function unwireWorkersPage(): void;
}
