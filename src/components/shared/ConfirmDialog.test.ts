import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, describe, expect, it } from "vitest";

import ConfirmDialog from "./ConfirmDialog.vue";

const props = {
  open: true,
  title: "删除",
  message: "确认删除？",
  confirmLabel: "删除",
};

function getDialog() {
  return document.body.querySelector<HTMLElement>('[role="dialog"]');
}

function dispatchKey(target: HTMLElement, key: string, shiftKey = false) {
  target.dispatchEvent(
    new KeyboardEvent("keydown", {
      key,
      shiftKey,
      bubbles: true,
      cancelable: true,
    }),
  );
}

afterEach(() => {
  document.body.innerHTML = "";
});

describe("ConfirmDialog", () => {
  it("teleports an accessible modal to body", () => {
    const wrapper = mount(ConfirmDialog, { props });
    const backdrop =
      document.body.querySelector<HTMLElement>(".modal-backdrop");
    const dialog = getDialog();

    expect(backdrop).not.toBeNull();
    expect(backdrop?.classList.contains("fixed")).toBe(true);
    expect(dialog?.getAttribute("aria-modal")).toBe("true");
    expect(dialog?.getAttribute("aria-labelledby")).toBe(
      dialog?.querySelector("h2")?.id,
    );
    expect(dialog?.getAttribute("aria-describedby")).toBe(
      dialog?.querySelector("p")?.id,
    );
    expect(dialog?.querySelector(".modal-actions")).not.toBeNull();

    wrapper.unmount();
    expect(getDialog()).toBeNull();
  });

  it("focuses cancel button when opened and restores focus when closed", async () => {
    const opener = document.createElement("button");
    document.body.appendChild(opener);
    opener.focus();
    const wrapper = mount(ConfirmDialog, {
      props: { ...props, open: false },
    });

    await wrapper.setProps({ open: true });
    await flushPromises();
    expect(document.activeElement?.textContent).toContain("取消");

    await wrapper.setProps({ open: false });
    expect(document.activeElement).toBe(opener);

    wrapper.unmount();
  });

  it("keeps Tab focus inside the dialog", async () => {
    const wrapper = mount(ConfirmDialog, { props });
    await flushPromises();
    const dialog = getDialog()!;
    const [cancelButton, confirmButton] = Array.from(
      dialog.querySelectorAll<HTMLButtonElement>("button"),
    );

    cancelButton.focus();
    dispatchKey(dialog, "Tab", true);
    expect(document.activeElement).toBe(confirmButton);

    confirmButton.focus();
    dispatchKey(dialog, "Tab");
    expect(document.activeElement).toBe(cancelButton);

    wrapper.unmount();
  });

  it("emits cancel on Escape", async () => {
    const wrapper = mount(ConfirmDialog, { props });
    await flushPromises();

    dispatchKey(getDialog()!, "Escape");

    expect(wrapper.emitted("cancel")).toHaveLength(1);
    wrapper.unmount();
  });
});
