import "@unocss/reset/tailwind.css";
import "uno.css";
import "./styles/main.css";

import { createPinia } from "pinia";
import { createApp } from "vue";

import App from "./app/App.vue";
// Importing this module registers matchMedia listener and applies the
// persisted theme to <html> before the first paint.
import "./composables/useTheme";

createApp(App).use(createPinia()).mount("#app");
