import { nextTick } from "vue";
import { beforeEach, describe, expect, it, vi } from "vitest";

beforeEach(() => {
  vi.resetModules();
  localStorage.clear();
  document.documentElement.removeAttribute("data-theme");
  document.documentElement.style.colorScheme = "";
});

describe("useTheme", () => {
  it("applies an explicit theme independently of the system preference", async () => {
    mockMatchMedia(false);
    const { useTheme } = await import("./useTheme");

    useTheme().setMode("dark");
    await nextTick();

    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(document.documentElement.style.colorScheme).toBe("dark");
    expect(localStorage.getItem("2notes-theme-mode")).toBe("dark");
  });

  it("tracks system preference changes only in system mode", async () => {
    const dispatchChange = mockMatchMedia(false);
    const { useTheme } = await import("./useTheme");

    expect(document.documentElement.dataset.theme).toBe("light");

    dispatchChange(true);
    await nextTick();
    expect(document.documentElement.dataset.theme).toBe("dark");

    useTheme().setMode("light");
    dispatchChange(true);
    await nextTick();
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("does not throw when localStorage get fails during module load", async () => {
    mockMatchMedia(true);
    Object.defineProperty(window, "localStorage", {
      configurable: true,
      value: {
        getItem: () => {
          throw new Error("blocked");
        },
        setItem: vi.fn(),
        removeItem: vi.fn(),
        clear: vi.fn(),
      },
    });

    const { useTheme } = await import("./useTheme");
    expect(useTheme().mode.value).toBe("system");
    await nextTick();
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it("keeps in-memory mode when localStorage set fails", async () => {
    mockMatchMedia(false);
    const setItem = vi.fn(() => {
      throw new Error("blocked");
    });
    Object.defineProperty(window, "localStorage", {
      configurable: true,
      value: {
        getItem: () => null,
        setItem,
        removeItem: vi.fn(),
        clear: vi.fn(),
      },
    });

    const { useTheme } = await import("./useTheme");
    expect(() => useTheme().setMode("dark")).not.toThrow();
    expect(useTheme().mode.value).toBe("dark");
    await nextTick();
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(setItem).toHaveBeenCalled();
  });
});

function mockMatchMedia(initialMatches: boolean) {
  let changeListener: ((event: MediaQueryListEvent) => void) | undefined;
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: vi.fn().mockReturnValue({
      matches: initialMatches,
      media: "(prefers-color-scheme: dark)",
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(
        (event: string, listener: (event: MediaQueryListEvent) => void) => {
          if (event === "change") changeListener = listener;
        },
      ),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    }),
  });

  return (matches: boolean) => {
    changeListener?.({ matches } as MediaQueryListEvent);
  };
}
