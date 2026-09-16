import { flushPromises, mount } from "@vue/test-utils";
import { defineComponent, h } from "vue";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { useAppQuitRequest } from "./useAppQuitRequest";

const mocks = vi.hoisted(() => ({
  isTauri: vi.fn(() => true),
  listen: vi.fn(),
  appQuitReady: vi.fn(),
  revealCurrentWindow: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ isTauri: mocks.isTauri }));
vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));
vi.mock("../services/windowApi", () => ({
  appQuitReady: mocks.appQuitReady,
}));
vi.mock("./useWindowReveal", () => ({
  revealCurrentWindow: mocks.revealCurrentWindow,
}));

describe("useAppQuitRequest", () => {
  beforeEach(() => {
    mocks.isTauri.mockReset().mockReturnValue(true);
    mocks.listen.mockReset();
    mocks.appQuitReady.mockReset().mockResolvedValue(undefined);
    mocks.revealCurrentWindow.mockReset().mockResolvedValue(undefined);
  });

  it("starts listener registration during setup before mounted work yields", async () => {
    const stop = vi.fn();
    mocks.listen.mockResolvedValue(stop);
    const prepareQuit = vi.fn().mockResolvedValue(true);
    const component = defineComponent({
      setup() {
        useAppQuitRequest(prepareQuit);
        return () => h("div");
      },
    });

    mount(component);

    expect(mocks.listen).toHaveBeenCalledWith(
      "app-quit-requested",
      expect.any(Function),
    );
    const listener = mocks.listen.mock.calls[0]?.[1] as (event: {
      payload: { requestId: string };
    }) => Promise<void>;
    await listener({ payload: { requestId: "quit-1" } });
    expect(prepareQuit).toHaveBeenCalledOnce();
    expect(mocks.appQuitReady).toHaveBeenCalledWith("quit-1");
    await flushPromises();
  });

  it("cleans up a listener that resolves after unmount", async () => {
    let resolveListener!: (stop: () => void) => void;
    const stop = vi.fn();
    mocks.listen.mockReturnValue(
      new Promise((resolve) => {
        resolveListener = resolve;
      }),
    );
    const component = defineComponent({
      setup() {
        useAppQuitRequest(() => true);
        return () => h("div");
      },
    });

    const wrapper = mount(component);
    wrapper.unmount();
    resolveListener(stop);
    await flushPromises();

    expect(stop).toHaveBeenCalledOnce();
  });

  it("reveals the window instead of quitting when prepareQuit refuses", async () => {
    const prepareQuit = vi.fn().mockResolvedValue(false);
    const stop = vi.fn();
    mocks.listen.mockResolvedValue(stop);
    const component = defineComponent({
      setup() {
        useAppQuitRequest(prepareQuit);
        return () => h("div");
      },
    });

    mount(component);
    const listener = mocks.listen.mock.calls[0]?.[1] as (event: {
      payload: { requestId: string };
    }) => Promise<void>;
    await listener({ payload: { requestId: "quit-2" } });
    await flushPromises();

    expect(prepareQuit).toHaveBeenCalledOnce();
    expect(mocks.revealCurrentWindow).toHaveBeenCalledOnce();
    expect(mocks.appQuitReady).not.toHaveBeenCalled();
  });

  it("reveals the window when prepareQuit throws", async () => {
    const prepareQuit = vi.fn().mockRejectedValue(new Error("dirty state"));
    const stop = vi.fn();
    mocks.listen.mockResolvedValue(stop);
    const component = defineComponent({
      setup() {
        useAppQuitRequest(prepareQuit);
        return () => h("div");
      },
    });

    mount(component);
    const listener = mocks.listen.mock.calls[0]?.[1] as (event: {
      payload: { requestId: string };
    }) => Promise<void>;
    await listener({ payload: { requestId: "quit-3" } });
    await flushPromises();

    expect(mocks.revealCurrentWindow).toHaveBeenCalledOnce();
    expect(mocks.appQuitReady).not.toHaveBeenCalled();
  });

  it("does nothing when running outside Tauri", async () => {
    mocks.isTauri.mockReturnValue(false);
    const component = defineComponent({
      setup() {
        useAppQuitRequest(() => true);
        return () => h("div");
      },
    });

    mount(component);

    expect(mocks.listen).not.toHaveBeenCalled();
  });
});
