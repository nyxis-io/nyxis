export type PageSeo = {
  title: string;
  description: string;
  canonical: string;
  ogTitle?: string;
  ogDescription?: string;
};

export const GA_ID = "G-9TP221ZJ9Z";

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
      { property: "og:image", content: "https://nyxis.io/favicon.svg" },
      { property: "og:image:alt", content: "Nyxis" },
      { name: "twitter:card", content: "summary" },
      { name: "twitter:title", content: meta.ogTitle ?? meta.title },
      {
        name: "twitter:description",
        content: meta.ogDescription ?? meta.description,
      },
      { name: "twitter:image", content: "https://nyxis.io/favicon.svg" },
    ],
    link: [{ rel: "canonical", href: meta.canonical }],
  };
}
