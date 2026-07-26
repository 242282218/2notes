import { mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import ThemeToggle from "./ThemeToggle.vue";

beforeEach(() => {
  vi.resetModules();
  localStorage.clear();
  document.documentElement.removeAttribute("data-theme");
  document.documentElement.style.colorScheme = "";
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: vi.fn().mockReturnValue({
      matches: false,
      media: "(prefers-color-scheme: dark)",
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    }),
  });
});

describe("ThemeToggle", () => {
  it("emits change for a valid theme option", async () => {
    const wrapper = mount(ThemeToggle);
    const select = wrapper.get("select");

    await select.setValue("dark");

    expect(wrapper.emitted("change")).toEqual([["dark"]]);
    wrapper.unmount();
  });

  it("does not emit change for an invalid theme option", async () => {
    const wrapper = mount(ThemeToggle);
    const select = wrapper.get("select");
    const element = select.element as HTMLSelectElement;

    // Force a non-contract value that native select.value can still carry.
    Object.defineProperty(element, "value", {
      configurable: true,
      get: () => "blue",
    });
    await select.trigger("change");

    expect(wrapper.emitted("change")).toBeUndefined();
    wrapper.unmount();
  });
});
