import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import type { KnowledgeSuggestion } from "../../types/generated";
import WikiLinkSuggestions from "./WikiLinkSuggestions.vue";

const suggestions: KnowledgeSuggestion[] = [
  { id: "one", title: "Canonical One", matchedAlias: null },
  { id: "two", title: "Canonical Two", matchedAlias: "Legacy Two" },
];

describe("WikiLinkSuggestions", () => {
  it("renders a stable listbox with the active option selected", () => {
    const wrapper = mount(WikiLinkSuggestions, {
      props: { suggestions, activeIndex: 1, listboxId: "wiki-links" },
    });

    expect(wrapper.get('[role="listbox"]').attributes("id")).toBe("wiki-links");
    const options = wrapper.findAll('[role="option"]');
    expect(options.map((option) => option.attributes("id"))).toEqual([
      "wiki-links-option-0",
      "wiki-links-option-1",
    ]);
    expect(options[0].attributes("aria-selected")).toBe("false");
    expect(options[1].attributes("aria-selected")).toBe("true");
  });

  it("shows canonical titles and identifies alias matches", () => {
    const wrapper = mount(WikiLinkSuggestions, {
      props: { suggestions, activeIndex: 0, listboxId: "wiki-links" },
    });

    expect(wrapper.text()).toContain("Canonical One");
    expect(wrapper.text()).toContain("Canonical Two");
    expect(wrapper.text()).toContain("历史标题：Legacy Two");
  });

  it("prevents mouse focus transfer and emits the clicked suggestion", async () => {
    const wrapper = mount(WikiLinkSuggestions, {
      props: { suggestions, activeIndex: 0, listboxId: "wiki-links" },
    });
    const option = wrapper.findAll('[role="option"]')[1];
    const mouseDown = new MouseEvent("mousedown", {
      bubbles: true,
      cancelable: true,
    });

    option.element.dispatchEvent(mouseDown);
    await option.trigger("click");

    expect(mouseDown.defaultPrevented).toBe(true);
    expect(wrapper.emitted("select")).toEqual([[suggestions[1]]]);
  });
});
