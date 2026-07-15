import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import EntryListItem from "./EntryListItem.vue";
import type { EntryListItem as EntryListItemType } from "../../types/generated";

describe("EntryListItem", () => {
  it("shows a text knowledge badge", () => {
    const wrapper = mount(EntryListItem, {
      props: { item: knowledgeItem(), active: false },
    });
    expect(wrapper.text()).toContain("知识");
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
