import { computed, ref, watch } from "vue";

import type { ThemeMode } from "../types/generated";

const storageKey = "2notes-theme-mode";
const html = document.documentElement;
const query = window.matchMedia("(prefers-color-scheme: dark)");

function readInitialMode(): ThemeMode {
  try {
    const saved = localStorage.getItem(storageKey);
    if (saved === "light" || saved === "dark" || saved === "system") {
      return saved;
    }
  } catch {
    // Private mode / blocked storage: fall back to system.
  }
  return "system";
}

// Module-level state: shared across all callers, initialized once.
const mode = ref<ThemeMode>(readInitialMode());
const systemIsDark = ref(query.matches);

function onQueryChange(event: MediaQueryListEvent) {
  systemIsDark.value = event.matches;
}

// Register once at module load; test-setup.ts mocks matchMedia before any
// test file imports this module.
query.addEventListener("change", onQueryChange);

const resolvedTheme = computed<"light" | "dark">(() => {
  if (mode.value === "light") return "light";
  if (mode.value === "dark") return "dark";
  return systemIsDark.value ? "dark" : "light";
});

function applyTheme(theme: "light" | "dark") {
  html.setAttribute("data-theme", theme);
  html.style.colorScheme = theme;
}

function setMode(next: ThemeMode) {
  mode.value = next;
  try {
    localStorage.setItem(storageKey, next);
  } catch {
    // Keep in-memory mode even when persistence is unavailable.
  }
}

// Side effect runs once on module import; keeps DOM in sync with resolvedTheme.
watch(resolvedTheme, applyTheme, { immediate: true });

export function useTheme() {
  return {
    mode,
    resolvedTheme,
    setMode,
    options: [
      { value: "system" as const, label: "跟随系统" },
      { value: "light" as const, label: "浅色" },
      { value: "dark" as const, label: "深色" },
    ],
  };
}

export type { ThemeMode };
