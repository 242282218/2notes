import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import type { BlockDocument } from "../../types/generated";
import EntryOutline from "./EntryOutline.vue";

const document: BlockDocument = {
  schemaVersion: 1,
  blocks: [
    {
      id: "heading-id",
      kind: "heading",
      attrs: { level: 2, language: null, start: null },
      content: [{ type: "text", text: "标题", marks: [] }],
      children: [],
    },
    {
      id: "paragraph-id",
      kind: "paragraph",
      attrs: { level: null, language: null, start: null },
      content: [{ type: "text", text: "正文", marks: [] }],
      children: [],
    },
  ],
};

describe("EntryOutline", () => {
  it("only renders heading blocks and focuses their stable id", async () => {
    const wrapper = mount(EntryOutline, { props: { document } });

    expect(wrapper.text()).toContain("标题");
    expect(wrapper.text()).not.toContain("正文");
    await wrapper.get("button").trigger("click");
    expect(wrapper.emitted("focus")).toEqual([["heading-id"]]);
  });

  it("does not render an empty outline", () => {
    const wrapper = mount(EntryOutline, {
      props: { document: { schemaVersion: 1, blocks: [document.blocks[1]] } },
    });

    expect(wrapper.find("nav").exists()).toBe(false);
  });
});
