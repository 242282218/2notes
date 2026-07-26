import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import type { AppView } from "../../app/routes";
import type { EntryStatus, EntryType } from "../../types/generated";
import type { EntryDetailToolbarState } from "../entry/entryDetailToolbar";
import AppTopbar from "./AppTopbar.vue";

const toolbarState: EntryDetailToolbarState = {
  saveState: "idle",
  saveError: null,
  showPromote: false,
  canPromote: false,
  canDemote: false,
  deleted: false,
};

function mountTopbar(
  props: Partial<{
    viewLabel: string;
    modelValue: string;
    view: AppView;
    currentTag: string;
    entryType: EntryType | "";
    status: EntryStatus | "";
    showDetailActions: boolean;
    tagPanelOpen: boolean;
  }> = {},
) {
  return mount(AppTopbar, {
    props: {
      viewLabel: "收集箱",
      modelValue: "",
      view: "inbox",
      currentTag: "",
      entryType: "" as EntryType | "",
      status: "" as EntryStatus | "",
      showDetailActions: true,
      toolbarState,
      tagPanelOpen: false,
      ...props,
    },
  });
}

describe("AppTopbar", () => {
  it("emits update:modelValue when search input changes", async () => {
    const wrapper = mountTopbar({ modelValue: "" });
    const input = wrapper.get("#global-search");
    await input.setValue("知识");
    expect(wrapper.emitted("update:modelValue")?.[0]).toEqual(["知识"]);
  });

  it("emits typeChange from desktop type select", async () => {
    const wrapper = mountTopbar();
    const typeSelect = wrapper
      .findAll('select[aria-label="类型筛选"]')
      .find((select) => select.classes().includes("xl:block") || true);
    await typeSelect!.setValue("task");
    expect(wrapper.emitted("typeChange")?.[0]).toEqual(["task"]);
  });

  it("toggles tag panel open state", async () => {
    const wrapper = mountTopbar({ view: "tags", tagPanelOpen: false });
    const tagButton = wrapper.get('button[aria-controls="tag-panel"]');
    await tagButton.trigger("click");
    expect(wrapper.emitted("update:tagPanelOpen")?.[0]).toEqual([true]);
  });

  it("emits quickCapture when record button is clicked", async () => {
    const wrapper = mountTopbar();
    const buttons = wrapper.findAll('[data-testid="topbar-end"] button');
    const capture = buttons[buttons.length - 1];
    await capture.trigger("click");
    expect(wrapper.emitted("quickCapture")).toHaveLength(1);
  });

  it("forwards detail action emits from DetailActionBar", async () => {
    const wrapper = mountTopbar({ showDetailActions: true });
    const bar = wrapper.findComponent({ name: "DetailActionBar" });
    expect(bar.exists()).toBe(true);
    await bar.vm.$emit("moveToTrash");
    expect(wrapper.emitted("moveToTrash")).toHaveLength(1);
  });

  it("opens compact filter popover", async () => {
    const wrapper = mountTopbar();
    const filterButton = wrapper.get(
      'button[aria-controls="entry-filter-popover"]',
    );
    await filterButton.trigger("click");
    await flushPromises();
    expect(wrapper.find("#entry-filter-popover").exists()).toBe(true);
  });

  it("exposes focusSearch", async () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const wrapper = mount(AppTopbar, {
      attachTo: host,
      props: {
        viewLabel: "收集箱",
        modelValue: "",
        view: "inbox",
        currentTag: "",
        entryType: "",
        status: "",
        showDetailActions: false,
        toolbarState,
        tagPanelOpen: false,
      },
    });

    (wrapper.vm as { focusSearch: () => void }).focusSearch();
    await flushPromises();
    expect(document.activeElement).toBe(wrapper.get("#global-search").element);
    wrapper.unmount();
    host.remove();
  });
});
