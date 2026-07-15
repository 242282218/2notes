import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import ConfirmDialog from "./ConfirmDialog.vue";

describe("ConfirmDialog", () => {
  it("focuses cancel button when opened and restores focus when closed", async () => {
    const opener = document.createElement("button");
    document.body.appendChild(opener);
    opener.focus();
    const wrapper = mount(ConfirmDialog, {
      props: {
        open: false,
        title: "删除",
        message: "确认删除？",
        confirmLabel: "删除",
      },
      attachTo: document.body,
    });

    await wrapper.setProps({ open: true });
    await flushPromises();
    expect(document.activeElement?.textContent).toContain("取消");

    await wrapper.setProps({ open: false });
    expect(document.activeElement).toBe(opener);

    wrapper.unmount();
    opener.remove();
  });

  it("emits cancel on Escape", async () => {
    const wrapper = mount(ConfirmDialog, {
      props: {
        open: true,
        title: "恢复",
        message: "确认恢复？",
        confirmLabel: "恢复",
      },
      attachTo: document.body,
    });

    await wrapper.find("section").trigger("keydown", { key: "Escape" });

    expect(wrapper.emitted("cancel")).toHaveLength(1);
    wrapper.unmount();
  });
});
