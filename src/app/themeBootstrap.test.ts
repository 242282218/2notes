import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { settingsGet } from "../services/settingsApi";
import type { AppSettings } from "../types/generated";
import { useSettingsStore } from "../stores/settings";
import { useTheme } from "../composables/useTheme";

vi.mock("../services/settingsApi", () => ({
  settingsGet: vi.fn(),
  settingsUpdate: vi.fn(),
}));

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const indexHtml = readFileSync(resolve(root, "index.html"), "utf8");
const bootstrapSource = readFileSync(
  resolve(root, "public/theme-bootstrap.js"),
  "utf8",
);

const baseSettings: AppSettings = {
  dataDir: "data",
  logDir: "logs",
  backupDir: "backups",
  shortcut: "Ctrl+Alt+N",
  shortcutRegistered: true,
  shortcutError: null,
  autostartEnabled: false,
  themeMode: "system",
};

function relativeLuminance(hex: string): number {
  const normalized = hex.replace("#", "");
  const channels = [0, 2, 4].map((offset) => {
    const value =
      Number.parseInt(normalized.slice(offset, offset + 2), 16) / 255;
    return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
}

function contrastRatio(foreground: string, background: string): number {
  const lighter = Math.max(
    relativeLuminance(foreground),
    relativeLuminance(background),
  );
  const darker = Math.min(
    relativeLuminance(foreground),
    relativeLuminance(background),
  );
  return (lighter + 0.05) / (darker + 0.05);
}

function extractThemeTokens(css: string, selector: string) {
  const blockMatch = css.match(
    new RegExp(
      `${selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\s*\\{([\\s\\S]*?)\\n\\}`,
    ),
  );
  if (!blockMatch) {
    throw new Error(`Missing CSS block for ${selector}`);
  }
  const block = blockMatch[1];
  const read = (name: string) => {
    const match = block.match(new RegExp(`${name}:\\s*([^;]+);`));
    if (!match) {
      throw new Error(`Missing token ${name} in ${selector}`);
    }
    return match[1].trim();
  };
  return {
    brand: read("--color-brand"),
    danger: read("--color-danger"),
    onBrand: read("--color-on-brand"),
    onDanger: read("--color-on-danger"),
    focusRing: read("--color-focus-ring").includes("var(")
      ? read("--color-brand")
      : read("--color-focus-ring"),
    bgBase: read("--color-bg-base"),
  };
}

function runBootstrap(options: {
  mode?: string | null;
  throwOnGet?: boolean;
  systemDark?: boolean;
}) {
  document.documentElement.removeAttribute("data-theme");
  document.documentElement.style.colorScheme = "";

  const storage: Record<string, string> = {};
  if (options.mode != null) {
    storage["2notes-theme-mode"] = options.mode;
  }

  Object.defineProperty(window, "localStorage", {
    configurable: true,
    value: {
      getItem(key: string) {
        if (options.throwOnGet) {
          throw new Error("localStorage blocked");
        }
        return storage[key] ?? null;
      },
      setItem: vi.fn(),
      removeItem: vi.fn(),
      clear: vi.fn(),
    },
  });

  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: vi.fn().mockImplementation((query: string) => ({
      matches:
        Boolean(options.systemDark) &&
        query.includes("prefers-color-scheme: dark"),
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
  });

  // Execute the production bootstrap script in this isolated DOM context.
  new Function(bootstrapSource)();
}

describe("theme bootstrap script wiring", () => {
  it("loads a sync theme bootstrap script before the Vue module entry", () => {
    const headMatch = indexHtml.match(/<head>([\s\S]*?)<\/head>/i);
    expect(headMatch).not.toBeNull();
    const head = headMatch![1];

    const bootstrapTag =
      /<script(?![^>]*\b(?:defer|async)\b)[^>]*\bdata-theme-bootstrap\b[^>]*\bsrc=["']\/theme-bootstrap\.js["'][^>]*><\/script>/i;
    expect(head).toMatch(bootstrapTag);

    const bootstrapIndex = head.search(bootstrapTag);
    const moduleInHead = head.search(
      /<script[^>]*\btype=["']module["'][^>]*\bsrc=["']\/src\/main\.ts["']/i,
    );
    expect(bootstrapIndex).toBeGreaterThanOrEqual(0);
    if (moduleInHead >= 0) {
      expect(bootstrapIndex).toBeLessThan(moduleInHead);
    }

    const bodyModule = indexHtml.search(
      /<script[^>]*\btype=["']module["'][^>]*\bsrc=["']\/src\/main\.ts["']/i,
    );
    const bootstrapInDoc = indexHtml.search(bootstrapTag);
    expect(bootstrapInDoc).toBeGreaterThanOrEqual(0);
    expect(bodyModule).toBeGreaterThan(bootstrapInDoc);
  });
});

describe("theme-bootstrap.js runtime", () => {
  it.each([
    { mode: "dark", systemDark: false, expected: "dark" },
    { mode: "light", systemDark: true, expected: "light" },
    { mode: "system", systemDark: true, expected: "dark" },
    { mode: "system", systemDark: false, expected: "light" },
    { mode: null, systemDark: true, expected: "dark" },
  ] as const)(
    "applies $expected for mode=$mode systemDark=$systemDark",
    ({ mode, systemDark, expected }) => {
      runBootstrap({ mode, systemDark });
      expect(document.documentElement.getAttribute("data-theme")).toBe(
        expected,
      );
      expect(document.documentElement.style.colorScheme).toBe(expected);
    },
  );

  it("falls back to system when localStorage get throws", () => {
    runBootstrap({ throwOnGet: true, systemDark: true });
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
    expect(document.documentElement.style.colorScheme).toBe("dark");
  });
});

describe("settings load aligns theme with backend", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.mocked(settingsGet).mockReset();
    useTheme().setMode("system");
  });

  it("applies backend themeMode after ensureLoaded", async () => {
    vi.mocked(settingsGet).mockResolvedValue({
      ...baseSettings,
      themeMode: "dark",
    });

    const store = useSettingsStore();
    await store.ensureLoaded();

    expect(store.settings?.themeMode).toBe("dark");
    expect(useTheme().mode.value).toBe("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
  });
});

describe("theme contrast tokens", () => {
  const css = readFileSync(resolve(root, "src/styles/main.css"), "utf8");

  it("meets WCAG contrast for brand/danger button text and focus ring", () => {
    const light = extractThemeTokens(css, ":root");
    const dark = extractThemeTokens(css, '[data-theme="dark"]');

    expect(contrastRatio(light.onBrand, light.brand)).toBeGreaterThanOrEqual(
      4.5,
    );
    expect(contrastRatio(light.onDanger, light.danger)).toBeGreaterThanOrEqual(
      4.5,
    );
    expect(contrastRatio(dark.onBrand, dark.brand)).toBeGreaterThanOrEqual(4.5);
    expect(contrastRatio(dark.onDanger, dark.danger)).toBeGreaterThanOrEqual(
      4.5,
    );

    expect(contrastRatio(light.focusRing, light.bgBase)).toBeGreaterThanOrEqual(
      3,
    );
    expect(contrastRatio(dark.focusRing, dark.bgBase)).toBeGreaterThanOrEqual(
      3,
    );
  });
});
