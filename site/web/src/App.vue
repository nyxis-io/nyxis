<template>
  <SiteNav />
  <RouterView />
  <SiteFooter v-if="showFooter" />
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useRoute } from "vue-router";
import SiteNav from "@/components/SiteNav.vue";
import SiteFooter from "@/components/SiteFooter.vue";

const route = useRoute();
const showFooter = computed(() => route.meta.footer !== false);
</script>
