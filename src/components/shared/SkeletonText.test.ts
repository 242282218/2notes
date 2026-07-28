import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import SkeletonText from "./SkeletonText.vue";

describe("SkeletonText", () => {
  it("renders the requested number of stable text lines", () => {
    const wrapper = mount(SkeletonText, { props: { lines: 3 } });

    const lines = wrapper.findAll("[data-skeleton-line]");
    expect(lines).toHaveLength(3);
    expect(lines.map((line) => line.attributes("style"))).toEqual([
      "width: 72%;",
      "width: 92%;",
      "width: 64%;",
    ]);
    expect(wrapper.get("[data-skeleton-text]").classes()).toContain(
      "animate-fade-in",
    );
    expect(lines[0].classes()).toContain("skeleton-pulse");
  });
});
