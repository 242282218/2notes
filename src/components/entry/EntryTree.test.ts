import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";

import { knowledgeTreeGet } from "../../services/knowledgeApi";
import EntryTree from "./EntryTree.vue";

vi.mock("../../services/knowledgeApi", () => ({
  knowledgeTreeGet: vi.fn(),
}));

const tree = [
  {
    id: "root",
    title: "根",
    children: [{ id: "child", title: "子", children: [] }],
  },
];

describe("EntryTree", () => {
  it("expands, returns focus to parent, and opens with Enter", async () => {
    vi.mocked(knowledgeTreeGet).mockResolvedValue(tree);
    const wrapper = mount(EntryTree, {
      attachTo: document.body,
      props: { selectedId: "root", refreshToken: 0 },
    });
    await flushPromises();

    const root = wrapper.get('[data-tree-node="root"]');
    await root.trigger("keydown", { key: "ArrowRight" });
    expect(wrapper.find('[data-tree-node="child"]').exists()).toBe(true);

    const child = wrapper.get('[data-tree-node="child"]');
    (child.element as HTMLElement).focus();
    await child.trigger("keydown", { key: "ArrowLeft" });
    expect(document.activeElement).toBe(root.element);

    await root.trigger("keydown", { key: "Enter" });
    expect(wrapper.emitted("select")).toEqual([["root"]]);
    wrapper.unmount();
  });

  it("does not offer the node or descendants as move targets", async () => {
    vi.mocked(knowledgeTreeGet).mockResolvedValue(tree);
    const wrapper = mount(EntryTree, {
      props: { selectedId: "root", refreshToken: 0 },
    });
    await flushPromises();

    await wrapper.get('[aria-label="移动到"]').trigger("click");
    const targets = wrapper
      .findAll('[role="menuitem"]')
      .map((item) => item.text());
    expect(targets).toEqual(["根级"]);
  });
});
