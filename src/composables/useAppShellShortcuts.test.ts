import { defineComponent, h, nextTick } from "vue";
import { mount, type VueWrapper } from "@vue/test-utils";
import { afterEach, describe, expect, it, vi } from "vitest";

import { useAppShellShortcuts } from "./useAppShellShortcuts";

const wrappers: VueWrapper[] = [];

function mountShortcuts(options: Parameters<typeof useAppShellShortcuts>[0]) {
  const wrapper = mount(
    defineComponent({
      setup() {
        useAppShellShortcuts(options);
        return () => h("div");
      },
    }),
  );
  wrappers.push(wrapper);
  return wrapper;
}

function dispatchKey(key: string, init: KeyboardEventInit = {}): KeyboardEvent {
  const event = new KeyboardEvent("keydown", {
    key,
    bubbles: true,
    cancelable: true,
    ...init,
  });
  window.dispatchEvent(event);
  return event;
}

describe("useAppShellShortcuts", () => {
  afterEach(() => {
    while (wrappers.length > 0) {
      wrappers.pop()?.unmount();
    }
  });

  it("focuses search on Ctrl/Cmd+F", async () => {
    const focusSearch = vi.fn();
    mountShortcuts({
      focusSearch,
      closeTopmostOverlay: vi.fn(),
      canDelete: () => false,
      requestDelete: vi.fn(),
    });
    await nextTick();

    const ctrlEvent = dispatchKey("f", { ctrlKey: true });
    expect(ctrlEvent.defaultPrevented).toBe(true);
    expect(focusSearch).toHaveBeenCalledTimes(1);

    const metaEvent = dispatchKey("F", { metaKey: true });
    expect(metaEvent.defaultPrevented).toBe(true);
    expect(focusSearch).toHaveBeenCalledTimes(2);
  });

  it("closes the topmost overlay on Escape", async () => {
    const closeTopmostOverlay = vi.fn();
    mountShortcuts({
      focusSearch: vi.fn(),
      closeTopmostOverlay,
      canDelete: () => false,
      requestDelete: vi.fn(),
    });
    await nextTick();

    dispatchKey("Escape");
    expect(closeTopmostOverlay).toHaveBeenCalledTimes(1);
  });

  it("requests delete only when canDelete is true", async () => {
    const requestDelete = vi.fn();
    const canDelete = vi.fn(() => true);
    mountShortcuts({
      focusSearch: vi.fn(),
      closeTopmostOverlay: vi.fn(),
      canDelete,
      requestDelete,
    });
    await nextTick();

    const event = dispatchKey("Delete");
    expect(canDelete).toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(true);
    expect(requestDelete).toHaveBeenCalledTimes(1);
  });

  it("does not request delete when canDelete is false", async () => {
    const requestDelete = vi.fn();
    mountShortcuts({
      focusSearch: vi.fn(),
      closeTopmostOverlay: vi.fn(),
      canDelete: () => false,
      requestDelete,
    });
    await nextTick();

    const event = dispatchKey("Delete");
    expect(event.defaultPrevented).toBe(false);
    expect(requestDelete).not.toHaveBeenCalled();
  });

  it("does not request delete from editable elements", async () => {
    const requestDelete = vi.fn();
    mountShortcuts({
      focusSearch: vi.fn(),
      closeTopmostOverlay: vi.fn(),
      canDelete: () => true,
      requestDelete,
    });
    await nextTick();

    const input = document.createElement("input");
    document.body.appendChild(input);
    input.focus();

    const event = dispatchKey("Delete");
    expect(event.defaultPrevented).toBe(false);
    expect(requestDelete).not.toHaveBeenCalled();

    input.remove();
  });

  it("does not request delete from contenteditable elements", async () => {
    const requestDelete = vi.fn();
    mountShortcuts({
      focusSearch: vi.fn(),
      closeTopmostOverlay: vi.fn(),
      canDelete: () => true,
      requestDelete,
    });
    await nextTick();

    const editable = document.createElement("div");
    editable.setAttribute("contenteditable", "true");
    document.body.appendChild(editable);

    const event = new KeyboardEvent("keydown", {
      key: "Delete",
      bubbles: true,
      cancelable: true,
    });
    editable.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
    expect(requestDelete).not.toHaveBeenCalled();

    editable.remove();
  });

  it("removes the window listener on unmount", async () => {
    const focusSearch = vi.fn();
    const wrapper = mountShortcuts({
      focusSearch,
      closeTopmostOverlay: vi.fn(),
      canDelete: () => false,
      requestDelete: vi.fn(),
    });
    await nextTick();

    wrapper.unmount();
    wrappers.pop();

    dispatchKey("f", { ctrlKey: true });
    expect(focusSearch).not.toHaveBeenCalled();
  });
});
