import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import EntryList from "./EntryList.vue";
import type { EntryListItem } from "../../types/generated";

describe("EntryList", () => {
  it("renders six stable skeleton items during the initial load", () => {
    const wrapper = mount(EntryList, {
      props: {
        items: [],
        selectedId: null,
        loading: true,
        hasMore: false,
      },
    });

    const status = wrapper.get('[role="status"][aria-label="正在加载条目"]');
    expect(status.attributes("aria-busy")).toBe("true");
    expect(status.findAll("[data-entry-skeleton]")).toHaveLength(6);
    expect(status.findAll("[data-skeleton-line]")).toHaveLength(18);
  });

  it("uses a filling empty state when the list has no entries", () => {
    const wrapper = mount(EntryList, {
      props: {
        items: [],
        selectedId: null,
        loading: false,
        hasMore: false,
      },
    });

    expect(wrapper.get("[data-empty-state]").classes()).toContain("h-full");
  });

  it("uses a gap-based item flow without panel or item divider borders", () => {
    const wrapper = mount(EntryList, {
      props: {
        items: [item("a"), item("b")],
        selectedId: "a",
        loading: false,
        hasMore: false,
      },
    });

    expect(wrapper.get(".entry-list").classes()).not.toContain("border-r");
    expect(wrapper.get("[data-testid='entry-items']").classes()).toContain(
      "gap-1",
    );
    for (const button of wrapper.findAll(".entry-list-item")) {
      expect(button.classes()).not.toContain("border-b");
      expect(button.classes()).toContain("rounded-md");
    }
  });

  it("navigates items with arrow keys", async () => {
    const wrapper = mount(EntryList, {
      props: {
        items: [item("a"), item("b"), item("c")],
        selectedId: null,
        loading: false,
        hasMore: false,
      },
      attachTo: document.body,
    });

    const buttons = wrapper.findAll(".entry-list-item");
    expect(buttons.length).toBe(3);

    (buttons[0].element as HTMLButtonElement).focus();
    await buttons[0].trigger("keydown", { key: "ArrowDown" });

    expect(document.activeElement).toBe(buttons[1].element);
    expect(wrapper.emitted("select")?.[0]).toEqual(["b"]);

    await buttons[1].trigger("keydown", { key: "ArrowUp" });
    expect(document.activeElement).toBe(buttons[0].element);
    expect(wrapper.emitted("select")?.[1]).toEqual(["a"]);

    wrapper.unmount();
  });

  it("jumps to first and last items with Home and End", async () => {
    const wrapper = mount(EntryList, {
      props: {
        items: [item("a"), item("b"), item("c")],
        selectedId: null,
        loading: false,
        hasMore: false,
      },
      attachTo: document.body,
    });

    const buttons = wrapper.findAll(".entry-list-item");
    (buttons[1].element as HTMLButtonElement).focus();

    await buttons[1].trigger("keydown", { key: "Home" });
    expect(document.activeElement).toBe(buttons[0].element);

    await buttons[0].trigger("keydown", { key: "End" });
    expect(document.activeElement).toBe(buttons[2].element);

    wrapper.unmount();
  });
});

function item(id: string): EntryListItem {
  return {
    id,
    title: `Entry ${id}`,
    summary: "summary",
    entryType: "idea",
    status: "pending",
    knowledgeState: "capture",
    tags: [],
    searchSnippet: null,
    revision: 1,
    createdAt: "2026-07-15T00:00:00Z",
    updatedAt: "2026-07-15T00:00:00Z",
    deletedAt: null,
  };
}
