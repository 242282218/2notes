import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import EmptyState from "./EmptyState.vue";

describe("EmptyState", () => {
  it("uses the compact size by default", () => {
    const wrapper = mount(EmptyState, { props: { title: "暂无内容" } });

    expect(wrapper.get("[data-empty-state]").classes()).toContain("min-h-40");
    expect(wrapper.get("[data-empty-state]").classes()).not.toContain("h-full");
  });

  it("fills its container and centers content in fill mode", () => {
    const wrapper = mount(EmptyState, {
      props: { title: "暂无内容", size: "fill" },
    });

    const state = wrapper.get("[data-empty-state]");
    expect(state.classes()).toContain("h-full");
    expect(state.classes()).toContain("content-center");
  });
});
