import { defineComponent, h, reactive } from "vue";
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import AppShell from "./AppShell.vue";

vi.mock("../../services/settingsApi", () => ({
  settingsGet: vi.fn().mockResolvedValue({
    dataDir: "data",
    logDir: "logs",
    backupDir: "backups",
    shortcut: "Ctrl+Alt+N",
    shortcutRegistered: true,
    shortcutError: null,
    autostartEnabled: false,
    themeMode: "system",
  }),
  settingsUpdate: vi.fn(),
}));

vi.mock("../../services/backupApi", () => ({
  backupsList: vi.fn().mockResolvedValue([]),
  backupsCreate: vi.fn(),
  backupsRestore: vi.fn(),
}));

const knowledgeMocks = vi.hoisted(() => ({
  knowledgeMove: vi.fn(),
}));

vi.mock("../../services/knowledgeApi", () => ({
  knowledgeMove: knowledgeMocks.knowledgeMove,
  knowledgeRebuildIndex: vi.fn(),
}));

vi.mock("../../services/exportApi", () => ({
  exportMarkdown: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn() }));

const tauriMocks = vi.hoisted(() => ({
  isTauri: vi.fn(() => false),
  listen: vi.fn(),
}));

const wrappers: VueWrapper[] = [];

const detailCommands = {
  flushPendingSave: vi.fn().mockResolvedValue(true),
  focusTitle: vi.fn(),
  requestMoveToTrash: vi.fn(),
};

const entries = reactive({
  view: "inbox",
  filters: { query: "", entryType: "", status: "", tag: "" },
  items: [],
  tags: [],
  tagsError: null as string | null,
  selectedId: "entry-1",
  detail: {
    id: "entry-1",
    title: "记录",
    knowledgeState: "capture",
    deletedAt: null,
    revision: 0,
  },
  loading: false,
  detailLoading: false,
  error: null,
  externalChangeToken: 0,
  selectionGeneration: 0,
  hasMore: false,
  load: vi.fn(),
  loadMore: vi.fn(),
  refreshTags: vi.fn(),
  select: vi.fn(),
  openEntry: vi.fn(),
  createAndSelect: vi.fn(),
  noteExternalChange: vi.fn(),
  applySavedEntry: vi.fn(),
  applyEntryListUpdate: vi.fn(),
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
  emits: ["open-related", "toolbar-change"],
  setup(_, { expose }) {
    expose(detailCommands);
    return () => h("section", { "data-testid": "entry-detail" });
  },
});

const EntryListStub = defineComponent({
  name: "EntryList",
  emits: ["select", "more"],
  setup() {
    return () =>
      h(
        "div",
        {
          class: "entry-list",
          tabindex: 0,
          "data-testid": "entry-list",
        },
        "list",
      );
  },
});

function mountShell(attachTo?: HTMLElement) {
  const wrapper = mount(AppShell, {
    attachTo,
    global: {
      stubs: {
        SidebarNav: true,
        EntryList: EntryListStub,
        EntryTree: true,
        KnowledgeHealthView: true,
        EntryDetail: EntryDetailStub,
        SettingsView: true,
      },
    },
  });
  wrappers.push(wrapper);
  return wrapper;
}

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

describe("AppShell selection commit boundary", () => {
  beforeEach(() => {
    detailCommands.flushPendingSave.mockReset().mockResolvedValue(true);
    detailCommands.focusTitle.mockReset();
    detailCommands.requestMoveToTrash.mockReset();
    entries.view = "inbox";
    entries.selectedId = "entry-1";
    entries.filters.query = "";
    entries.error = null;
    entries.load.mockReset();
    entries.refreshTags.mockReset();
    entries.select.mockReset();
    entries.openEntry.mockReset();
    entries.createAndSelect.mockReset();
    knowledgeMocks.knowledgeMove.mockReset();
    entries.noteExternalChange.mockReset();
    entries.moveSelectedToTrash.mockReset();
    entries.setView.mockReset();
    entries.setQuery.mockReset();
    tauriMocks.isTauri.mockReset().mockReturnValue(false);
    tauriMocks.listen.mockReset();
  });

  afterEach(() => {
    while (wrappers.length > 0) {
      wrappers.pop()?.unmount();
    }
  });

  it("renders the health workspace without the entry list or detail", async () => {
    entries.view = "health";
    const wrapper = mountShell();
    await flushPromises();

    expect(
      wrapper.findComponent({ name: "KnowledgeHealthView" }).exists(),
    ).toBe(true);
    expect(wrapper.find('[data-testid="entry-list"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="entry-detail"]').exists()).toBe(false);
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

  it("does not reload entries when an external change arrives in health", async () => {
    const handlers: Array<() => Promise<void> | void> = [];
    tauriMocks.isTauri.mockReturnValue(true);
    tauriMocks.listen.mockImplementation(async (_event, handler) => {
      handlers.push(handler as () => Promise<void> | void);
      return vi.fn();
    });
    entries.view = "health";

    mountShell();
    await flushPromises();
    entries.load.mockClear();
    entries.refreshTags.mockClear();
    entries.noteExternalChange.mockClear();

    await handlers[0]();
    await flushPromises();

    expect(entries.load).not.toHaveBeenCalled();
    expect(entries.refreshTags).not.toHaveBeenCalled();
    expect(entries.noteExternalChange).toHaveBeenCalledOnce();
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

  it("drops a tree move when selection changes while its flush is pending", async () => {
    const pendingFlush = deferred<boolean>();
    detailCommands.flushPendingSave.mockReturnValue(pendingFlush.promise);
    entries.view = "knowledge";
    entries.selectedId = "entry-1";
    entries.selectionGeneration = 1;
    entries.detail = {
      id: "entry-1",
      title: "记录",
      knowledgeState: "knowledge",
      deletedAt: null,
      revision: 4,
    };
    const wrapper = mountShell();
    await flushPromises();

    const tree = wrapper.findComponent({ name: "EntryTree" });
    await tree.vm.$emit("move", "entry-1", null);
    await flushPromises();

    entries.selectedId = "entry-2";
    entries.selectionGeneration = 2;
    entries.detail = {
      id: "entry-2",
      title: "新记录",
      knowledgeState: "knowledge",
      deletedAt: null,
      revision: 9,
    };
    pendingFlush.resolve(true);
    await flushPromises();

    expect(knowledgeMocks.knowledgeMove).not.toHaveBeenCalled();
  });

  it("flushes before creating an entry and focuses its title after success", async () => {
    entries.createAndSelect.mockResolvedValue({ id: "created" });
    const wrapper = mountShell();
    await flushPromises();

    await wrapper.get("button.btn-primary").trigger("click");
    await flushPromises();

    expect(detailCommands.flushPendingSave).toHaveBeenCalled();
    expect(entries.createAndSelect).toHaveBeenCalledOnce();
    expect(detailCommands.focusTitle).toHaveBeenCalledOnce();
  });

  it("creates only one entry while a creation request is pending", async () => {
    const pendingCreate = deferred<{ id: string }>();
    entries.createAndSelect.mockReturnValue(pendingCreate.promise);
    const wrapper = mountShell();
    await flushPromises();

    const topbar = wrapper.findComponent({ name: "AppTopbar" });
    await topbar.vm.$emit("create");
    await topbar.vm.$emit("create");
    await flushPromises();

    expect(entries.createAndSelect).toHaveBeenCalledOnce();
    expect(
      wrapper.get("button.btn-primary").attributes("disabled"),
    ).toBeDefined();

    pendingCreate.resolve({ id: "created" });
    await flushPromises();
    expect(detailCommands.focusTitle).toHaveBeenCalledOnce();
  });

  it("does not focus when a pending creation no longer owns the selection", async () => {
    entries.createAndSelect.mockResolvedValue(null);
    const wrapper = mountShell();
    await flushPromises();

    await wrapper.get("button.btn-primary").trigger("click");
    await flushPromises();

    expect(detailCommands.focusTitle).not.toHaveBeenCalled();
  });

  it("does not create or focus when flush fails", async () => {
    detailCommands.flushPendingSave.mockResolvedValue(false);
    const wrapper = mountShell();
    await flushPromises();

    await wrapper.get("button.btn-primary").trigger("click");
    await flushPromises();

    expect(entries.createAndSelect).not.toHaveBeenCalled();
    expect(detailCommands.focusTitle).not.toHaveBeenCalled();
  });

  it("does not focus the title when entry creation fails", async () => {
    entries.createAndSelect.mockRejectedValue(new Error("create failed"));
    const wrapper = mountShell();
    await flushPromises();

    await wrapper.get("button.btn-primary").trigger("click");
    await flushPromises();

    expect(entries.createAndSelect).toHaveBeenCalledOnce();
    expect(detailCommands.focusTitle).not.toHaveBeenCalled();
  });
});

describe("AppShell delete flush boundary", () => {
  beforeEach(() => {
    detailCommands.flushPendingSave.mockReset().mockResolvedValue(true);
    detailCommands.focusTitle.mockReset();
    detailCommands.requestMoveToTrash.mockReset();
    entries.view = "inbox";
    entries.selectedId = "entry-1";
    entries.filters.query = "";
    entries.error = null;
    entries.moveSelectedToTrash.mockReset();
    entries.setQuery.mockReset();
    tauriMocks.isTauri.mockReset().mockReturnValue(false);
    tauriMocks.listen.mockReset();
  });

  afterEach(() => {
    while (wrappers.length > 0) {
      wrappers.pop()?.unmount();
    }
  });

  it("does not move to trash when flush fails on Delete", async () => {
    detailCommands.flushPendingSave.mockResolvedValue(false);
    const host = document.createElement("div");
    document.body.appendChild(host);
    const wrapper = mountShell(host);
    await flushPromises();

    const list = wrapper.get("[data-testid='entry-list']")
      .element as HTMLElement;
    list.focus();
    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Delete", bubbles: true }),
    );
    await flushPromises();

    expect(detailCommands.flushPendingSave).toHaveBeenCalled();
    expect(detailCommands.requestMoveToTrash).not.toHaveBeenCalled();
    expect(entries.moveSelectedToTrash).not.toHaveBeenCalled();
    host.remove();
  });

  it("requests move to trash via detail after successful flush on Delete", async () => {
    detailCommands.flushPendingSave.mockResolvedValue(true);
    const host = document.createElement("div");
    document.body.appendChild(host);
    const wrapper = mountShell(host);
    await flushPromises();

    const list = wrapper.get("[data-testid='entry-list']")
      .element as HTMLElement;
    list.focus();
    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Delete", bubbles: true }),
    );
    await flushPromises();

    expect(detailCommands.flushPendingSave).toHaveBeenCalled();
    expect(detailCommands.requestMoveToTrash).toHaveBeenCalled();
    expect(entries.moveSelectedToTrash).not.toHaveBeenCalled();
    host.remove();
  });
});

describe("AppShell search input race", () => {
  beforeEach(() => {
    detailCommands.flushPendingSave.mockReset().mockResolvedValue(true);
    detailCommands.focusTitle.mockReset();
    detailCommands.requestMoveToTrash.mockReset();
    entries.view = "inbox";
    entries.selectedId = "entry-1";
    entries.filters.query = "";
    entries.error = null;
    entries.setQuery.mockReset();
    tauriMocks.isTauri.mockReset().mockReturnValue(false);
    tauriMocks.listen.mockReset();
  });

  afterEach(() => {
    while (wrappers.length > 0) {
      wrappers.pop()?.unmount();
    }
  });

  it("updates search input immediately without awaiting detail flush", async () => {
    let resolveFlush: ((value: boolean) => void) | undefined;
    detailCommands.flushPendingSave.mockImplementation(
      () =>
        new Promise<boolean>((resolve) => {
          resolveFlush = resolve;
        }),
    );

    const wrapper = mountShell();
    await flushPromises();

    const input = wrapper.get("#global-search");

    await input.setValue("知");
    await input.setValue("知识");
    await input.setValue("知识库");

    expect((input.element as HTMLInputElement).value).toBe("知识库");
    expect(entries.setQuery).toHaveBeenCalledWith("知");
    expect(entries.setQuery).toHaveBeenCalledWith("知识");
    expect(entries.setQuery).toHaveBeenCalledWith("知识库");
    expect(detailCommands.flushPendingSave).not.toHaveBeenCalled();

    resolveFlush?.(true);
    await flushPromises();
  });
});

describe("AppShell accessibility semantics", () => {
  beforeEach(() => {
    detailCommands.flushPendingSave.mockReset().mockResolvedValue(true);
    detailCommands.focusTitle.mockReset();
    detailCommands.requestMoveToTrash.mockReset();
    entries.view = "inbox";
    entries.selectedId = "entry-1";
    entries.filters.query = "";
    entries.filters.tag = "";
    entries.error = null;
    entries.detail = {
      id: "entry-1",
      title: "记录",
      knowledgeState: "capture",
      deletedAt: null,
      revision: 0,
    };
    entries.setView.mockReset();
    entries.setTagFilter.mockReset();
    tauriMocks.isTauri.mockReset().mockReturnValue(false);
    tauriMocks.listen.mockReset();
  });

  afterEach(() => {
    while (wrappers.length > 0) {
      wrappers.pop()?.unmount();
    }
  });

  it("keeps a single main landmark when settings is open", async () => {
    setActivePinia(createPinia());
    entries.view = "settings";
    const wrapper = mount(AppShell, {
      global: {
        stubs: {
          SidebarNav: true,
          EntryList: EntryListStub,
          EntryDetail: EntryDetailStub,
          AppearanceSettings: true,
          ConfirmDialog: true,
        },
      },
    });
    wrappers.push(wrapper);
    await flushPromises();

    expect(wrapper.findAll("main")).toHaveLength(1);
    expect(wrapper.find('section[aria-label="设置内容"]').exists()).toBe(true);
  });

  it("closes the detail menu with Escape and returns focus to its trigger", async () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const wrapper = mountShell(host);
    await flushPromises();

    const trigger = wrapper.get('button[aria-label="更多详情操作"]');
    expect(trigger.attributes("aria-haspopup")).toBe("true");
    expect(trigger.attributes("aria-expanded")).toBe("false");
    expect(trigger.attributes("aria-controls")).toBe("detail-actions-menu");

    (trigger.element as HTMLButtonElement).focus();
    await trigger.trigger("click");
    expect(trigger.attributes("aria-expanded")).toBe("true");
    expect(wrapper.find("#detail-actions-menu").exists()).toBe(true);

    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
    );
    await flushPromises();

    expect(wrapper.find("#detail-actions-menu").exists()).toBe(false);
    expect(trigger.attributes("aria-expanded")).toBe("false");
    expect(document.activeElement).toBe(trigger.element);
    host.remove();
  });

  it("hides decorative icons inside IconButton and SaveState", async () => {
    const wrapper = mountShell();
    await flushPromises();

    const entryDetail = wrapper.findComponent(EntryDetailStub);
    await entryDetail.vm.$emit("toolbar-change", {
      saveState: "saving",
      saveError: null,
      showPromote: true,
      canPromote: true,
      canDemote: false,
      deleted: false,
    });
    await flushPromises();

    const saveState = wrapper.get('[data-testid="save-state"]');
    expect(saveState.find("svg").attributes("aria-hidden")).toBe("true");

    const trashButton = wrapper
      .findAll("button")
      .find((button) => button.attributes("aria-label") === "移到回收站");
    expect(trashButton).toBeDefined();
    expect(trashButton!.find("svg").attributes("aria-hidden")).toBe("true");
  });
});
