import { defineComponent, h, reactive } from "vue";
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import AppShell from "./AppShell.vue";

const tauriMocks = vi.hoisted(() => ({
  isTauri: vi.fn(() => false),
  listen: vi.fn(),
}));

const wrappers: VueWrapper[] = [];

const detailCommands = {
  flushPendingSave: vi.fn().mockResolvedValue(true),
};

const entries = reactive({
  view: "inbox",
  filters: { query: "", entryType: "", status: "", tag: "" },
  items: [],
  tags: [],
  selectedId: "entry-1",
  detail: {
    id: "entry-1",
    title: "记录",
    knowledgeState: "capture",
    deletedAt: null,
  },
  loading: false,
  detailLoading: false,
  error: null,
  externalChangeToken: 0,
  hasMore: false,
  load: vi.fn(),
  loadMore: vi.fn(),
  refreshTags: vi.fn(),
  select: vi.fn(),
  openEntry: vi.fn(),
  noteExternalChange: vi.fn(),
  applySavedEntry: vi.fn(),
  moveSelectedToTrash: vi.fn(),
  restoreSelected: vi.fn(),
  deleteSelectedForever: vi.fn(),
  setView: vi.fn(),
  setTagFilter: vi.fn(),
  setTypeFilter: vi.fn(),
  setStatusFilter: vi.fn(),
  setQuery: vi.fn(),
});

vi.mock("../../stores/entries", () => ({
  useEntriesStore: () => entries,
}));

vi.mock("@tauri-apps/api/core", () => ({ isTauri: tauriMocks.isTauri }));
vi.mock("@tauri-apps/api/event", () => ({ listen: tauriMocks.listen }));
vi.mock("../../services/windowApi", () => ({
  windowOpenQuickCapture: vi.fn(),
}));
vi.mock("../../composables/useAppQuitRequest", () => ({
  useAppQuitRequest: vi.fn(),
}));

const EntryDetailStub = defineComponent({
  emits: ["open-related"],
  setup(_, { expose }) {
    expose(detailCommands);
    return () => h("section", { "data-testid": "entry-detail" });
  },
});

function mountShell(attachTo?: HTMLElement) {
  const wrapper = mount(AppShell, {
    attachTo,
    global: {
      stubs: {
        SidebarNav: true,
        EntryList: true,
        EntryDetail: EntryDetailStub,
        SettingsView: true,
      },
    },
  });
  wrappers.push(wrapper);
  return wrapper;
}

describe("AppShell selection commit boundary", () => {
  beforeEach(() => {
    detailCommands.flushPendingSave.mockReset().mockResolvedValue(true);
    entries.view = "inbox";
    entries.selectedId = "entry-1";
    entries.error = null;
    entries.load.mockReset();
    entries.refreshTags.mockReset();
    entries.select.mockReset();
    entries.openEntry.mockReset();
    entries.noteExternalChange.mockReset();
    entries.setView.mockReset();
    tauriMocks.isTauri.mockReset().mockReturnValue(false);
    tauriMocks.listen.mockReset();
  });

  afterEach(() => {
    while (wrappers.length > 0) {
      wrappers.pop()?.unmount();
    }
  });

  it("does not replace store detail when flush fails for selection and navigation", async () => {
    detailCommands.flushPendingSave.mockResolvedValue(false);
    const wrapper = mountShell();
    await flushPromises();

    const entryList = wrapper.findComponent({ name: "EntryList" });
    await entryList.vm.$emit("select", "entry-2");
    await flushPromises();
    expect(entries.select).not.toHaveBeenCalled();

    const entryDetail = wrapper.findComponent(EntryDetailStub);
    await entryDetail.vm.$emit("open-related", "entry-2");
    await flushPromises();
    expect(entries.openEntry).not.toHaveBeenCalled();

    const sidebar = wrapper.findComponent({ name: "SidebarNav" });
    await sidebar.vm.$emit("change", "knowledge");
    await flushPromises();
    expect(entries.setView).not.toHaveBeenCalled();
  });

  it("skips external entries-changed reload when flush fails", async () => {
    const handlers: Array<() => Promise<void> | void> = [];
    tauriMocks.isTauri.mockReturnValue(true);
    tauriMocks.listen.mockImplementation(async (_event, handler) => {
      handlers.push(handler as () => Promise<void> | void);
      return vi.fn();
    });
    detailCommands.flushPendingSave.mockResolvedValue(false);

    mountShell();
    await flushPromises();
    entries.load.mockClear();
    entries.refreshTags.mockClear();
    entries.noteExternalChange.mockClear();

    expect(handlers).toHaveLength(1);
    await handlers[0]();
    await flushPromises();

    expect(entries.load).not.toHaveBeenCalled();
    expect(entries.refreshTags).not.toHaveBeenCalled();
    expect(entries.noteExternalChange).not.toHaveBeenCalled();
  });

  it("commits selection after a successful flush", async () => {
    detailCommands.flushPendingSave.mockResolvedValue(true);
    const wrapper = mountShell();
    await flushPromises();

    const entryList = wrapper.findComponent({ name: "EntryList" });
    await entryList.vm.$emit("select", "entry-2");
    await flushPromises();
    expect(detailCommands.flushPendingSave).toHaveBeenCalled();
    expect(entries.select).toHaveBeenCalledWith("entry-2");
  });
});
