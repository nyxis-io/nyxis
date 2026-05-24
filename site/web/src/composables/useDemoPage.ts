import { nextTick, onMounted, onUnmounted, useTemplateRef } from "vue";

/**
 * Mount a legacy DOM demo script after the route template is in the document.
 * The route SFC must include `<main ref="root">` (or any element with ref="root").
 */
export function useDemoPage(
  wire: (root: HTMLElement) => void | Promise<void>,
  unwire?: () => void,
): void {
  const root = useTemplateRef<HTMLElement>("root");

  onMounted(async () => {
    await nextTick();
    const el = root.value;
    if (!el) {
      console.error("useDemoPage: missing ref=\"root\" on the page root element");
      return;
    }
    await wire(el);
  });

  onUnmounted(() => {
    unwire?.();
  });
}
