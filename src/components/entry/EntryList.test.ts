import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it } from "vitest";

import EntryList from "./EntryList.vue";
import type { EntryListItem } from "../../types/generated";

describe("EntryList", () => {
  beforeEach(() => {
    // jsdom reports zero layout, and the virtualizer renders nothing for a
    // zero-height viewport. Fake a usable viewport so rows exist in the DOM.
    Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
      configurable: true,
      value: 600,
    });
    Object.defineProperty(HTMLElement.prototype, "offsetWidth", {
      configurable: true,
      value: 800,
    });
  });

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

  it("uses a padding-based item flow without panel or item divider borders", () => {
    const wrapper = mount(EntryList, {
      props: {
        items: [item("a"), item("b")],
        selectedId: "a",
        loading: false,
        hasMore: false,
      },
    });

    expect(wrapper.get(".entry-list").classes()).not.toContain("border-r");
    expect(wrapper.get("[data-testid='entry-items']").classes()).not.toContain(
      "gap-1",
    );
    for (const button of wrapper.findAll(".entry-list-item")) {
      expect(button.classes()).not.toContain("border-b");
      expect(button.classes()).toContain("rounded-md");
    }
  });

  it("renders rows that arrive after mount", async () => {
    const wrapper = mount(EntryList, {
      props: {
        items: [],
        selectedId: null,
        loading: true,
        hasMore: false,
      },
      attachTo: document.body,
    });
    await flushPromises();
    expect(wrapper.findAll(".entry-list-item")).toHaveLength(0);

    await wrapper.setProps({ items: [item("a"), item("b")], loading: false });
    await flushPromises();

    expect(wrapper.findAll(".entry-list-item")).toHaveLength(2);
    wrapper.unmount();
  });

  it("renders rows appended by load more", async () => {
    const wrapper = mount(EntryList, {
      props: {
        items: [item("a")],
        selectedId: null,
        loading: false,
        hasMore: true,
      },
      attachTo: document.body,
    });
    await flushPromises();
    expect(wrapper.findAll(".entry-list-item")).toHaveLength(1);

    await wrapper.setProps({ items: [item("a"), item("b"), item("c")] });
    await flushPromises();

    expect(wrapper.findAll(".entry-list-item")).toHaveLength(3);
    wrapper.unmount();
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
    await flushPromises();

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
    await flushPromises();

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
