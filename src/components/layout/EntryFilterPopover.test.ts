import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import EntryFilterPopover from "./EntryFilterPopover.vue";

describe("EntryFilterPopover", () => {
  it("renders type and status options when open", () => {
    const wrapper = mount(EntryFilterPopover, {
      props: {
        open: true,
        entryType: "",
        status: "",
      },
    });

    expect(wrapper.find("#entry-filter-popover").exists()).toBe(true);
    expect(wrapper.find('select[aria-label="类型筛选"]').exists()).toBe(true);
    expect(wrapper.find('select[aria-label="状态筛选"]').exists()).toBe(true);
  });

  it("emits typeChange and statusChange from selects", async () => {
    const wrapper = mount(EntryFilterPopover, {
      props: {
        open: true,
        entryType: "",
        status: "",
      },
    });

    const typeSelect = wrapper.get('select[aria-label="类型筛选"]');
    await typeSelect.setValue("idea");
    expect(wrapper.emitted("typeChange")?.[0]).toEqual(["idea"]);

    const statusSelect = wrapper.get('select[aria-label="状态筛选"]');
    await statusSelect.setValue("done");
    expect(wrapper.emitted("statusChange")?.[0]).toEqual(["done"]);
  });

  it("emits close with returnFocus on Escape", async () => {
    const wrapper = mount(EntryFilterPopover, {
      props: {
        open: true,
        entryType: "",
        status: "",
      },
      attachTo: document.body,
    });

    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
    );
    await flushPromises();

    expect(wrapper.emitted("close")?.[0]).toEqual([true]);
    wrapper.unmount();
  });

  it("does not render when closed", () => {
    const wrapper = mount(EntryFilterPopover, {
      props: {
        open: false,
        entryType: "",
        status: "",
      },
    });

    expect(wrapper.find("#entry-filter-popover").exists()).toBe(false);
  });
});
