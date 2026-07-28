import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import EntryListItem from "./EntryListItem.vue";
import type { EntryListItem as EntryListItemType } from "../../types/generated";

describe("EntryListItem", () => {
  it("uses the selected token without an inset shadow for active items", () => {
    const wrapper = mount(EntryListItem, {
      props: { item: knowledgeItem(), active: true },
    });

    const classes = wrapper.get("button").classes();
    expect(classes).toContain("bg-selected");
    expect(classes).toContain("border-l-2");
    expect(classes).toContain("border-l-brand");
    expect(classes).not.toContain("border-l-transparent");
    expect(classes).not.toContain("bg-brand-subtle");
    expect(
      classes.some((className) => className.includes("shadow-[inset")),
    ).toBe(false);
  });

  it("uses only the transparent left border token for inactive items", () => {
    const wrapper = mount(EntryListItem, {
      props: { item: knowledgeItem(), active: false },
    });

    const classes = wrapper.get("button").classes();
    expect(classes).toContain("border-l-transparent");
    expect(classes).not.toContain("border-l-brand");
  });

  it("separates the knowledge badge from the primary type metadata", () => {
    const wrapper = mount(EntryListItem, {
      props: { item: knowledgeItem(), active: false },
    });
    const compactText = wrapper.text().replace(/\s/g, "");
    expect(compactText).toContain("知识·想法·已归档");
    expect(compactText).not.toContain("··");
  });

  it("shows two tags and gives the overflow count a full tag title", () => {
    const item = knowledgeItem();
    item.tags = [tag("one"), tag("two"), tag("three"), tag("four")];
    const wrapper = mount(EntryListItem, {
      props: { item, active: false },
    });

    const visibleTags = wrapper.findAll("[data-entry-tag]");
    expect(visibleTags.map((node) => node.text())).toEqual(["#one", "#two"]);
    expect(wrapper.get("[data-tag-overflow]").text()).toBe("+2");
    expect(wrapper.get("[data-tag-overflow]").attributes("title")).toBe(
      "#one, #two, #three, #four",
    );
  });

  it("renders search snippets as text instead of html", () => {
    const item = knowledgeItem();
    item.searchSnippet = {
      parts: [
        { text: "matched ", highlighted: false },
        {
          text: '<img src=x onerror="alert(1)">',
          highlighted: true,
        },
      ],
    };

    const wrapper = mount(EntryListItem, {
      props: { item, active: false },
    });

    expect(wrapper.find("img").exists()).toBe(false);
    expect(wrapper.get("mark").text()).toContain(
      '<img src=x onerror="alert(1)">',
    );
  });
});

function tag(name: string) {
  return {
    id: `tag-${name}`,
    name,
    normalizedName: name,
    createdAt: "2026-07-15T00:00:00Z",
    entryCount: 1,
  };
}

function knowledgeItem(): EntryListItemType {
  return {
    id: "knowledge-1",
    title: "知识节点",
    summary: "正文",
    entryType: "idea",
    status: "archived",
    knowledgeState: "knowledge",
    tags: [],
    searchSnippet: null,
    revision: 1,
    createdAt: "2026-07-15T00:00:00Z",
    updatedAt: "2026-07-15T00:00:00Z",
    deletedAt: null,
  };
}
