<template>
  <nav class="site-nav">
    <RouterLink class="nav-brand" to="/">
      <svg class="nav-logo" width="22" height="22" viewBox="0 0 24 24" aria-hidden="true">
        <rect x="2" y="2" width="9" height="9" rx="1.5" fill="currentColor" opacity="0.9" />
        <rect x="13" y="2" width="9" height="9" rx="1.5" fill="currentColor" opacity="0.45" />
        <rect x="2" y="13" width="9" height="9" rx="1.5" fill="currentColor" opacity="0.45" />
        <rect x="13" y="13" width="9" height="9" rx="1.5" fill="currentColor" />
      </svg>
      <span class="nav-wordmark">Nyxis</span>
      <span class="nav-tag">NXS</span>
    </RouterLink>
    <div class="nav-links">
      <RouterLink
        v-for="item in globalLinks"
        :key="item.id"
        :to="item.to"
        :aria-current="navCurrent === item.id ? 'page' : undefined"
      >
        {{ item.label }}
      </RouterLink>
      <a class="nav-github" href="https://github.com/nyxis-io/nyxis" rel="noopener" target="_blank">
        GitHub
      </a>
    </div>
    <button ref="themeBtn" type="button" class="theme-toggle" aria-label="Theme" />
  </nav>
  <nav v-if="showDemoSubnav" class="demo-subnav" aria-label="Demo pages">
    <RouterLink
      v-for="item in demoTools"
      :key="item.id"
      :to="item.to"
      :aria-current="navCurrent === item.id ? 'page' : undefined"
    >
      {{ item.label }}
    </RouterLink>
  </nav>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRoute } from "vue-router";
import { bindThemeToggle } from "@/composables/useTheme";

const route = useRoute();
const themeBtn = ref<HTMLButtonElement | null>(null);

const globalLinks = [
  { id: "home", label: "Home", to: "/" },
  { id: "use-cases", label: "Use cases", to: "/use-cases/" },
  { id: "pricing", label: "Commercial pricing", to: "/pricing/" },
  { id: "demo", label: "Demo", to: "/demo/" },
  { id: "bench", label: "Benchmark", to: "/bench/" },
] as const;

const demoTools = [
  { id: "demo", label: "All demos", to: "/demo/" },
  { id: "ticker", label: "Ticker", to: "/demo/ticker" },
  { id: "workers", label: "Workers", to: "/demo/workers" },
  { id: "explorer", label: "Explorer", to: "/demo/explorer" },
  { id: "report", label: "Report", to: "/demo/report" },
  { id: "wal", label: "WAL", to: "/demo/wal" },
] as const;

const navCurrent = computed(() => (route.meta.nav as string) ?? "");
const showDemoSubnav = computed(() => route.meta.demoSubnav === true);

onMounted(() => {
  if (themeBtn.value) bindThemeToggle(themeBtn.value);
});
</script>
