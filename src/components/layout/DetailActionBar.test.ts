import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import type { EntryDetailToolbarState } from "../entry/entryDetailToolbar";
import DetailActionBar from "./DetailActionBar.vue";

function toolbar(
  partial: Partial<EntryDetailToolbarState> = {},
): EntryDetailToolbarState {
  return {
    saveState: "idle",
    saveError: null,
    showPromote: false,
    canPromote: false,
    canDemote: false,
    deleted: false,
    ...partial,
  };
}

describe("DetailActionBar", () => {
  it("emits moveToTrash for active entries", async () => {
    const wrapper = mount(DetailActionBar, {
      props: { toolbarState: toolbar() },
    });

    const trash = wrapper
      .findAll("button")
      .find((button) => button.attributes("aria-label") === "移到回收站");
    await trash!.trigger("click");
    expect(wrapper.emitted("moveToTrash")).toHaveLength(1);
  });

  it("emits promote when promote is available", async () => {
    const wrapper = mount(DetailActionBar, {
      props: {
        toolbarState: toolbar({
          showPromote: true,
          canPromote: true,
        }),
      },
    });

    const promote = wrapper
      .findAll("button")
      .find((button) => button.attributes("aria-label") === "沉淀为知识");
    await promote!.trigger("click");
    expect(wrapper.emitted("promote")).toHaveLength(1);
  });

  it("emits restore and deleteForever for deleted entries", async () => {
    const wrapper = mount(DetailActionBar, {
      props: {
        toolbarState: toolbar({ deleted: true }),
      },
    });

    const restore = wrapper
      .findAll("button")
      .find((button) => button.attributes("aria-label") === "恢复");
    await restore!.trigger("click");
    expect(wrapper.emitted("restore")).toHaveLength(1);

    const del = wrapper
      .findAll("button")
      .find((button) => button.attributes("aria-label") === "永久删除");
    await del!.trigger("click");
    expect(wrapper.emitted("deleteForever")).toHaveLength(1);
  });

  it("closes the overflow menu with Escape and returns focus", async () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const wrapper = mount(DetailActionBar, {
      props: { toolbarState: toolbar() },
      attachTo: host,
    });

    const trigger = wrapper.get('button[aria-label="更多详情操作"]');
    (trigger.element as HTMLButtonElement).focus();
    await trigger.trigger("click");
    expect(wrapper.find("#detail-actions-menu").exists()).toBe(true);

    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
    );
    await flushPromises();

    expect(wrapper.find("#detail-actions-menu").exists()).toBe(false);
    expect(document.activeElement).toBe(trigger.element);
    wrapper.unmount();
    host.remove();
  });

  it("emits retry from SaveState", async () => {
    const wrapper = mount(DetailActionBar, {
      props: {
        toolbarState: toolbar({
          saveState: "failed",
          saveError: "保存失败",
        }),
      },
    });

    const retry = wrapper
      .get('[data-testid="save-state"]')
      .findAll("button")
      .find((button) => button.text() === "重试");
    await retry!.trigger("click");
    expect(wrapper.emitted("retry")).toHaveLength(1);
  });
});
