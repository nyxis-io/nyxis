import routeManifest from "../../content/routes.json";
import siteConfig from "../../content/site.json";
import type { PageSeo } from "./seo";

export type RouteManifestEntry = {
  path: string;
  markdown: string;
  title: string;
  description: string;
  canonical: string;
  interactive?: boolean;
  sitemap?: { priority: string; changefreq: string };
};

export const SITE_ORIGIN = siteConfig.origin;
export const ROUTE_MANIFEST = routeManifest as RouteManifestEntry[];

const seoByPath = new Map(ROUTE_MANIFEST.map((r) => [r.path, r]));

/** SEO metadata for a route path (single source: content/routes.json). */
export function seoForPath(path: string): PageSeo {
  const entry = seoByPath.get(path);
  if (!entry) {
    throw new Error(`No SEO manifest entry for path: ${path}`);
  }
  return {
    title: entry.title,
    description: entry.description,
    canonical: entry.canonical,
  };
}
