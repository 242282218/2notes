import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { Draft } from "../../types/generated";
import QuickCapture from "./QuickCapture.vue";

const draftGet = vi.fn();
const draftUpdate = vi.fn();
const quickCaptureSubmit = vi.fn();
const databaseRestoreReady = vi.fn();
const windowHideQuickCapture = vi.fn();
const eventListeners = new Map<
  string,
  (event: { payload: unknown }) => unknown
>();

vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => true,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (
      event: string,
      callback: (event: { payload: unknown }) => unknown,
    ) => {
      eventListeners.set(event, callback);
      return vi.fn();
    },
  ),
}));

vi.mock("../../services/backupApi", () => ({
  databaseRestoreReady: (...args: unknown[]) => databaseRestoreReady(...args),
}));

vi.mock("../../services/draftApi", () => ({
  draftGet: (...args: unknown[]) => draftGet(...args),
  draftUpdate: (...args: unknown[]) => draftUpdate(...args),
  quickCaptureSubmit: (...args: unknown[]) => quickCaptureSubmit(...args),
}));

vi.mock("../../services/windowApi", () => ({
  appQuitReady: vi.fn(),
  windowHideQuickCapture: (...args: unknown[]) =>
    windowHideQuickCapture(...args),
}));

vi.mock("../../composables/useWindowReveal", () => ({
  revealCurrentWindow: vi.fn(),
}));

describe("QuickCapture", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    draftGet.mockReset();
    draftUpdate.mockReset();
    quickCaptureSubmit.mockReset();
    databaseRestoreReady.mockReset();
    windowHideQuickCapture.mockReset();
    windowHideQuickCapture.mockResolvedValue(undefined);
    eventListeners.clear();
    draftGet.mockResolvedValue(draft("", 0));
    draftUpdate.mockResolvedValue(draft("", 1));
    quickCaptureSubmit.mockResolvedValue(draft("", 2));
  });

  it("uses the opaque window fallback with an elevated animated card", async () => {
    const wrapper = mount(QuickCapture);
    await flushPromises();

    const shell = wrapper.get("[data-quick-capture-shell]");
    const card = wrapper.get("[data-quick-capture-card]");
    expect(shell.classes()).toContain("bg-bg-base");
    expect(shell.classes()).toContain("p-3");
    expect(card.classes()).toContain("rounded-lg");
    expect(card.classes()).toContain("elevation-2");
    expect(card.classes()).toContain("animate-fade-in");
    expect(card.classes()).toContain("translate-y-0");
  });

  it("provides accessible input and live feedback semantics", async () => {
    draftGet.mockRejectedValue(new Error("草稿加载失败"));
    const wrapper = mount(QuickCapture);
    await flushPromises();

    const textarea = wrapper.get("textarea");
    expect(textarea.attributes("aria-invalid")).toBe("true");
    expect(wrapper.get('label[for="quick-capture-content"]').text()).toBe(
      "记录内容",
    );
    expect(wrapper.get('[role="status"]').attributes("aria-live")).toBe(
      "polite",
    );
    expect(wrapper.get('[role="alert"]').text()).toBe("草稿加载失败");
  });

  it("submits with Enter and hides with Escape", async () => {
    const wrapper = mount(QuickCapture);
    await flushPromises();
    const textarea = wrapper.get("textarea");
    await textarea.setValue("capture this");

    await textarea.trigger("keydown", { key: "Enter" });
    await flushPromises();
    expect(quickCaptureSubmit).toHaveBeenCalledWith("capture this", 1);
    expect(windowHideQuickCapture).toHaveBeenCalledTimes(1);

    await textarea.setValue("keep this draft");
    await textarea.trigger("keydown", { key: "Escape" });
    await flushPromises();
    expect(windowHideQuickCapture).toHaveBeenCalledTimes(2);
  });

  it("uses the revision returned by a stale save for the next save", async () => {
    let resolveFirstSave: ((value: Draft) => void) | undefined;
    draftUpdate
      .mockImplementationOnce(
        () =>
          new Promise<Draft>((resolve) => {
            resolveFirstSave = resolve;
          }),
      )
      .mockResolvedValueOnce(draft("second", 2));

    const wrapper = mount(QuickCapture);
    await flushPromises();
    const textarea = wrapper.get("textarea");

    await textarea.setValue("first");
    await vi.advanceTimersByTimeAsync(250);
    expect(draftUpdate).toHaveBeenNthCalledWith(1, "first", 0);

    await textarea.setValue("second");
    resolveFirstSave?.(draft("first", 1));
    await flushPromises();

    expect(draftUpdate).toHaveBeenNthCalledWith(2, "second", 1);
  });

  it("does not submit while an IME composition is active", async () => {
    const wrapper = mount(QuickCapture);
    await flushPromises();
    const textarea = wrapper.get("textarea");
    await textarea.setValue("中文输入");

    await textarea.trigger("keydown", {
      key: "Enter",
      isComposing: true,
    });
    await flushPromises();

    expect(quickCaptureSubmit).not.toHaveBeenCalled();
  });

  it("flushes the local draft before acknowledging database restore", async () => {
    draftUpdate.mockResolvedValue(draft("unsaved draft", 1));
    const wrapper = mount(QuickCapture);
    await flushPromises();
    await wrapper.get("textarea").setValue("unsaved draft");

    const listener = eventListeners.get("database-restore-prepare");
    expect(listener).toBeDefined();
    await listener?.({ payload: { requestId: "restore-1" } });
    await flushPromises();

    expect(draftUpdate).toHaveBeenCalledWith("unsaved draft", 0);
    expect(databaseRestoreReady).toHaveBeenCalledWith("restore-1");
  });

  it("reloads the restored draft after a database restore event", async () => {
    draftGet
      .mockResolvedValueOnce(draft("before restore", 5))
      .mockResolvedValueOnce(draft("restored draft", 2));
    const wrapper = mount(QuickCapture);
    await flushPromises();
    expect(wrapper.get("textarea").element.value).toBe("before restore");

    const listener = eventListeners.get("database-restored");
    expect(listener).toBeDefined();
    await listener?.({ payload: null });
    await flushPromises();

    expect(wrapper.get("textarea").element.value).toBe("restored draft");
    await wrapper.get("textarea").setValue("restored draft updated");
    await vi.advanceTimersByTimeAsync(250);
    expect(draftUpdate).toHaveBeenLastCalledWith("restored draft updated", 2);
  });

  it("does not autosave after the component is unmounted", async () => {
    draftUpdate.mockImplementation(
      () =>
        new Promise<Draft>(() => {
          // leave pending forever so we can assert it never starts after unmount
        }),
    );

    const wrapper = mount(QuickCapture);
    await flushPromises();
    const textarea = wrapper.get("textarea");

    await textarea.setValue("should not save after unmount");
    wrapper.unmount();
    await vi.advanceTimersByTimeAsync(250);
    await flushPromises();

    expect(draftUpdate).not.toHaveBeenCalled();
  });

  it("ignores inflight autosave results after unmount", async () => {
    let resolveSave: ((value: Draft) => void) | undefined;
    draftUpdate.mockImplementationOnce(
      () =>
        new Promise<Draft>((resolve) => {
          resolveSave = resolve;
        }),
    );

    const wrapper = mount(QuickCapture);
    await flushPromises();
    const textarea = wrapper.get("textarea");

    await textarea.setValue("inflight draft");
    await vi.advanceTimersByTimeAsync(250);
    expect(draftUpdate).toHaveBeenCalledTimes(1);

    wrapper.unmount();
    resolveSave?.(draft("inflight draft", 9));
    await flushPromises();

    // remount and ensure the discarded revision never leaked into a new cycle
    draftGet.mockResolvedValueOnce(draft("", 0));
    const remounted = mount(QuickCapture);
    await flushPromises();
    await remounted.get("textarea").setValue("after remount");
    await vi.advanceTimersByTimeAsync(250);
    await flushPromises();

    expect(draftUpdate).toHaveBeenLastCalledWith("after remount", 0);
    expect(remounted.find('[role="alert"]').exists()).toBe(false);
  });

  it("ignores stale autosave results that finish after database restore", async () => {
    let resolveFirstSave: ((value: Draft) => void) | undefined;
    draftGet
      .mockResolvedValueOnce(draft("before restore", 0))
      .mockResolvedValueOnce(draft("restored draft", 2));
    draftUpdate
      .mockImplementationOnce(
        () =>
          new Promise<Draft>((resolve) => {
            resolveFirstSave = resolve;
          }),
      )
      .mockResolvedValueOnce(draft("after restore edit", 3));

    const wrapper = mount(QuickCapture);
    await flushPromises();
    const textarea = wrapper.get("textarea");

    await textarea.setValue("pending before restore");
    await vi.advanceTimersByTimeAsync(250);
    expect(draftUpdate).toHaveBeenNthCalledWith(1, "pending before restore", 0);

    const listener = eventListeners.get("database-restored");
    expect(listener).toBeDefined();
    await listener?.({ payload: null });
    await flushPromises();
    expect(wrapper.get("textarea").element.value).toBe("restored draft");

    resolveFirstSave?.(draft("pending before restore", 1));
    await flushPromises();

    await wrapper.get("textarea").setValue("after restore edit");
    await vi.advanceTimersByTimeAsync(250);
    await flushPromises();

    expect(draftUpdate).toHaveBeenNthCalledWith(2, "after restore edit", 2);
    expect(draftUpdate).not.toHaveBeenCalledWith("after restore edit", 1);
  });
});

function draft(content: string, revision: number): Draft {
  return {
    content,
    revision,
    updatedAt: "2026-07-15T00:00:00Z",
  };
}
