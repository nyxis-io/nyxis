import { createApp } from "vue";
import { createHead } from "@unhead/vue/client";
import App from "./App.vue";
import router from "./router";
import "./assets/theme.css";
import "./assets/interactive-pages.css";
import { initTheme } from "./composables/useTheme";
import { initWebMcp } from "./agent/webmcp";

initTheme();

const app = createApp(App);
app.use(createHead());
app.use(router);
initWebMcp(router);
app.mount("#app");
