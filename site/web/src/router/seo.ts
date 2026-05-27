import siteConfig from "../../content/site.json";

export type PageSeo = {
  title: string;
  description: string;
  canonical: string;
  ogTitle?: string;
  ogDescription?: string;
};

export const GA_ID = "G-9TP221ZJ9Z";

/** Social preview card (NXS hero infographic). Served from /public/og-image.png */
export const OG_IMAGE_URL = `${siteConfig.origin}${siteConfig.ogImagePath}`;
export const OG_IMAGE_ALT = siteConfig.ogImageAlt;

export function usePageSeo(meta: PageSeo) {
  return {
    title: meta.title,
    meta: [
      { name: "description", content: meta.description },
      { property: "og:site_name", content: "Nyxis" },
      { property: "og:type", content: "website" },
      { property: "og:url", content: meta.canonical },
      {
        property: "og:title",
        content: meta.ogTitle ?? meta.title,
      },
      {
        property: "og:description",
        content: meta.ogDescription ?? meta.description,
      },
      { property: "og:image", content: OG_IMAGE_URL },
      { property: "og:image:type", content: "image/png" },
      { property: "og:image:width", content: "1400" },
      { property: "og:image:height", content: "933" },
      { property: "og:image:alt", content: OG_IMAGE_ALT },
      { name: "twitter:card", content: "summary_large_image" },
      { name: "twitter:title", content: meta.ogTitle ?? meta.title },
      {
        name: "twitter:description",
        content: meta.ogDescription ?? meta.description,
      },
      { name: "twitter:image", content: OG_IMAGE_URL },
      { name: "twitter:image:alt", content: OG_IMAGE_ALT },
    ],
    link: [{ rel: "canonical", href: meta.canonical }],
  };
}
