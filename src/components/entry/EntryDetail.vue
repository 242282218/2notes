<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  ref,
  watch,
  watchEffect,
} from "vue";
import { ChevronRight, FileText } from "lucide-vue-next";

import { useAutosave } from "../../composables/useAutosave";
import {
  applyWikiLinkCompletion,
  findWikiLinkCompletion,
  type WikiLinkCompletion,
} from "../../composables/useWikiLinkCompletion";
import { entriesUpdate } from "../../services/entryApi";
import {
  knowledgeDemote,
  knowledgePromote,
  knowledgeSuggest,
} from "../../services/knowledgeApi";
import type {
  EntryDetail,
  EntryPatch,
  EntryStatus,
  EntryType,
  KnowledgeSuggestion,
} from "../../types/generated";
import type { EntryDetailToolbarState } from "./entryDetailToolbar";
import ConfirmDialog from "../shared/ConfirmDialog.vue";
import EmptyState from "../shared/EmptyState.vue";
import EntryStatusSelect from "./EntryStatusSelect.vue";
import EntryTypeSelect from "./EntryTypeSelect.vue";
import KnowledgeRelations from "./KnowledgeRelations.vue";
import TagInput from "./TagInput.vue";
import WikiLinkSuggestions from "./WikiLinkSuggestions.vue";

const props = withDefaults(
  defineProps<{
    detail: EntryDetail | null;
    loading: boolean;
    refreshToken?: number;
    selectionGeneration?: number;
  }>(),
  { refreshToken: 0, selectionGeneration: 0 },
);

const emit = defineEmits<{
  saved: [entry: EntryDetail, selectionGeneration: number];
  entryUpdated: [entry: EntryDetail];
  trash: [];
  restore: [];
  deleteForever: [];
  openRelated: [id: string];
  toolbarChange: [state: EntryDetailToolbarState];
}>();

const title = ref("");
const currentContent = ref("");
const entryType = ref<EntryType>("unclear");
const status = ref<EntryStatus>("pending");
const tagInputRef = ref<{ commitDraft: () => void } | null>(null);
const contentEditorRef = ref<HTMLTextAreaElement | null>(null);
const tags = ref<string[]>([]);
const baseRevision = ref(0);
const editingEntryId = ref<string | null>(null);
const knowledgeState = ref<EntryDetail["knowledgeState"] | null>(null);
const deletedAt = ref<string | null>(null);
const originalContent = ref("");
const confirmTrash = ref(false);
const confirmDelete = ref(false);
const knowledgeError = ref("");
const wikiLinkCompletion = ref<WikiLinkCompletion | null>(null);
const wikiLinkSuggestions = ref<KnowledgeSuggestion[]>([]);
const wikiLinkActiveIndex = ref(0);
const wikiLinkListboxId = "entry-wiki-link-suggestions";
const wikiLinkSuggestionsOpen = computed(
  () =>
    wikiLinkCompletion.value !== null && wikiLinkSuggestions.value.length > 0,
);
const editorDisabled = computed(() => Boolean(deletedAt.value));
const hasEditingEntry = computed(() => Boolean(editingEntryId.value));
let initializing = false;
let syncVersion = 0;
let titleDirty = false;
let wikiLinkRequestId = 0;
let wikiLinkTimer: number | undefined;
let wikiLinkHandledKey: string | null = null;
let operationGeneration = 0;

const autosave = useAutosave<{
  entry: EntryDetail;
  selectionGeneration: number;
  operationGeneration: number;
}>({
  delay: 500,
  save: async () => {
    const requestSelectionGeneration = props.selectionGeneration;
    const requestOperationGeneration = operationGeneration;
    const patch: EntryPatch = {
      title: titleDirty ? title.value : null,
      document: null,
      currentContent: currentContent.value,
      entryType: entryType.value,
      status: status.value,
      tags: tags.value,
    };
    if (!editingEntryId.value) {
      throw new Error("未选择条目");
    }
    const entry = await entriesUpdate(
      editingEntryId.value,
      patch,
      baseRevision.value,
    );
    return {
      entry,
      selectionGeneration: requestSelectionGeneration,
      operationGeneration: requestOperationGeneration,
    };
  },
  onSaved: ({
    entry,
    selectionGeneration,
    operationGeneration: requestGeneration,
  }) => {
    if (
      editingEntryId.value === entry.id &&
      operationGeneration === requestGeneration
    ) {
      applySavedRevision(entry);
      emit("saved", entry, selectionGeneration);
    } else {
      emit("entryUpdated", entry);
    }
  },
  onStaleSaved: ({ entry, operationGeneration: requestGeneration }) => {
    if (
      editingEntryId.value === entry.id &&
      operationGeneration === requestGeneration
    ) {
      applySavedRevision(entry);
    }
    emit("entryUpdated", entry);
  },
});

function applySavedRevision(entry: EntryDetail) {
  if (entry.revision > baseRevision.value) {
    baseRevision.value = entry.revision;
  }
  titleDirty = false;
  knowledgeState.value = entry.knowledgeState;
  deletedAt.value = entry.deletedAt;
}

function isAutosaveBusy() {
  const state = autosave.state.value;
  return state === "dirty" || state === "saving" || state === "failed";
}

function applyEntrySnapshot(entry: EntryDetail | null) {
  editingEntryId.value = entry?.id || null;
  title.value = entry?.title || "";
  currentContent.value = entry?.currentContent || "";
  entryType.value = entry?.entryType || "unclear";
  status.value = entry?.status || "pending";
  tags.value = entry?.tags.map((tag) => tag.name) || [];
  baseRevision.value = entry?.revision || 0;
  knowledgeState.value = entry?.knowledgeState ?? null;
  deletedAt.value = entry?.deletedAt ?? null;
  originalContent.value = entry?.originalContent || "";
  titleDirty = false;
}

watchEffect(() => {
  emit("toolbarChange", {
    saveState: autosave.state.value,
    saveError: autosave.error.value,
    showPromote:
      hasEditingEntry.value &&
      !deletedAt.value &&
      knowledgeState.value === "capture",
    canPromote:
      hasEditingEntry.value &&
      !deletedAt.value &&
      knowledgeState.value === "capture" &&
      Boolean(title.value.trim()),
    canDemote:
      hasEditingEntry.value &&
      !deletedAt.value &&
      knowledgeState.value === "knowledge",
    deleted: Boolean(deletedAt.value),
  });
});

watch(
  () => props.detail,
  async (entry) => {
    resetWikiLinkCompletion();
    const nextId = entry?.id ?? null;
    const sameEntry = nextId !== null && nextId === editingEntryId.value;

    if (sameEntry) {
      // Same-id replacement: never re-run full local reset while the user is
      // still editing. Only fully hydrate when autosave is clean and the
      // incoming revision is strictly newer.
      if (isAutosaveBusy()) {
        if (entry && entry.revision > baseRevision.value) {
          baseRevision.value = entry.revision;
        }
        return;
      }
      if (!entry || entry.revision <= baseRevision.value) {
        return;
      }
      initializing = true;
      applyEntrySnapshot(entry);
      autosave.reset();
      await nextTick();
      initializing = false;
      return;
    }

    const currentSync = ++syncVersion;
    initializing = true;
    try {
      // Identity change: flush the previous entry before swapping local state.
      // AppShell owns selection commit; this only drains the previous cycle.
      await autosave.flush();
      if (currentSync !== syncVersion) {
        return;
      }
    } catch {
      initializing = false;
      return;
    }

    operationGeneration += 1;
    applyEntrySnapshot(entry);
    autosave.reset();
    await nextTick();
    initializing = false;
  },
  { immediate: true },
);

onBeforeUnmount(() => {
  resetWikiLinkCompletion();
  autosave.dispose();
});

watch(title, () => {
  if (!initializing && editingEntryId.value && !deletedAt.value) {
    titleDirty = true;
    autosave.markDirty();
  }
});

watch([currentContent, entryType, status, tags], () => {
  if (!initializing && editingEntryId.value && !deletedAt.value) {
    autosave.markDirty();
  }
});

async function confirmMoveToTrash() {
  if (!(await flushPendingSave())) {
    return;
  }
  confirmTrash.value = false;
  emit("trash");
}

function confirmDeleteForever() {
  confirmDelete.value = false;
  emit("deleteForever");
}

function requestMoveToTrash() {
  confirmTrash.value = true;
}

function restore() {
  emit("restore");
}

function requestDeleteForever() {
  confirmDelete.value = true;
}

function retrySave() {
  void autosave.retry();
}

async function flushPendingSave(): Promise<boolean> {
  try {
    tagInputRef.value?.commitDraft();
    await nextTick();
    await autosave.flush();
    return autosave.state.value !== "failed";
  } catch {
    return false;
  }
}

async function promoteToKnowledge() {
  knowledgeError.value = "";
  if (!(await flushPendingSave()) || !editingEntryId.value) return;
  const requestEntryId = editingEntryId.value;
  const requestRevision = baseRevision.value;
  const requestGeneration = operationGeneration;
  const requestSelectionGeneration = props.selectionGeneration;
  try {
    const updated = await knowledgePromote(requestEntryId, requestRevision);
    if (
      editingEntryId.value !== requestEntryId ||
      operationGeneration !== requestGeneration ||
      props.selectionGeneration !== requestSelectionGeneration
    ) {
      emit("entryUpdated", updated);
      return;
    }
    applySavedRevision(updated);
    emit("saved", updated, requestSelectionGeneration);
  } catch (error) {
    if (
      editingEntryId.value !== requestEntryId ||
      operationGeneration !== requestGeneration ||
      props.selectionGeneration !== requestSelectionGeneration
    )
      return;
    knowledgeError.value = error instanceof Error ? error.message : "沉淀失败";
  }
}

async function demoteFromKnowledge() {
  knowledgeError.value = "";
  if (!(await flushPendingSave()) || !editingEntryId.value) return;
  const requestEntryId = editingEntryId.value;
  const requestRevision = baseRevision.value;
  const requestGeneration = operationGeneration;
  const requestSelectionGeneration = props.selectionGeneration;
  try {
    const updated = await knowledgeDemote(requestEntryId, requestRevision);
    if (
      editingEntryId.value !== requestEntryId ||
      operationGeneration !== requestGeneration ||
      props.selectionGeneration !== requestSelectionGeneration
    ) {
      emit("entryUpdated", updated);
      return;
    }
    applySavedRevision(updated);
    emit("saved", updated, requestSelectionGeneration);
  } catch (error) {
    if (
      editingEntryId.value !== requestEntryId ||
      operationGeneration !== requestGeneration ||
      props.selectionGeneration !== requestSelectionGeneration
    )
      return;
    knowledgeError.value = error instanceof Error ? error.message : "移出失败";
  }
}

function refreshWikiLinkCompletion() {
  const editor = contentEditorRef.value;
  if (!editor) {
    resetWikiLinkCompletion();
    return;
  }
  const next = findWikiLinkCompletion(editor.value, editor.selectionStart);
  if (!next) {
    resetWikiLinkCompletion();
    return;
  }
  const current = wikiLinkCompletion.value;
  if (current?.start === next.start && current.query === next.query) return;

  clearWikiLinkTimer();
  const requestId = ++wikiLinkRequestId;
  wikiLinkCompletion.value = next;
  wikiLinkSuggestions.value = [];
  wikiLinkActiveIndex.value = 0;
  wikiLinkTimer = window.setTimeout(async () => {
    wikiLinkTimer = undefined;
    try {
      const suggestions = await knowledgeSuggest(next.query);
      if (requestId !== wikiLinkRequestId) return;
      wikiLinkSuggestions.value = suggestions;
      wikiLinkActiveIndex.value = 0;
    } catch {
      if (requestId === wikiLinkRequestId) {
        wikiLinkSuggestions.value = [];
      }
    }
  }, 120);
}

function handleWikiLinkKeyup(event: KeyboardEvent) {
  if (wikiLinkHandledKey === event.key) {
    wikiLinkHandledKey = null;
    return;
  }
  wikiLinkHandledKey = null;
  refreshWikiLinkCompletion();
}

function handleWikiLinkKeydown(event: KeyboardEvent) {
  wikiLinkHandledKey = null;
  if (!wikiLinkSuggestionsOpen.value || event.isComposing) return;
  const count = wikiLinkSuggestions.value.length;
  if (event.key === "ArrowDown") {
    wikiLinkHandledKey = event.key;
    event.preventDefault();
    wikiLinkActiveIndex.value = (wikiLinkActiveIndex.value + 1) % count;
  } else if (event.key === "ArrowUp") {
    wikiLinkHandledKey = event.key;
    event.preventDefault();
    wikiLinkActiveIndex.value = (wikiLinkActiveIndex.value - 1 + count) % count;
  } else if (event.key === "Enter") {
    wikiLinkHandledKey = event.key;
    event.preventDefault();
    void selectWikiLinkSuggestion(
      wikiLinkSuggestions.value[wikiLinkActiveIndex.value],
    );
  } else if (event.key === "Escape") {
    wikiLinkHandledKey = event.key;
    event.preventDefault();
    resetWikiLinkCompletion();
  }
}

async function selectWikiLinkSuggestion(suggestion: KnowledgeSuggestion) {
  const editor = contentEditorRef.value;
  const completion = wikiLinkCompletion.value;
  if (!editor || !completion) return;

  const applied = applyWikiLinkCompletion(
    editor.value,
    completion,
    suggestion.title,
  );
  currentContent.value = applied.value;
  resetWikiLinkCompletion();
  await nextTick();
  editor.focus();
  editor.setSelectionRange(applied.caret, applied.caret);
}

function resetWikiLinkCompletion() {
  clearWikiLinkTimer();
  wikiLinkRequestId += 1;
  wikiLinkCompletion.value = null;
  wikiLinkSuggestions.value = [];
  wikiLinkActiveIndex.value = 0;
}

function clearWikiLinkTimer() {
  if (wikiLinkTimer !== undefined) {
    window.clearTimeout(wikiLinkTimer);
    wikiLinkTimer = undefined;
  }
}

defineExpose({
  flushPendingSave,
  promoteToKnowledge,
  demoteFromKnowledge,
  requestMoveToTrash,
  restore,
  requestDeleteForever,
  retrySave,
});
</script>

<template>
  <section class="min-h-0 min-w-0 overflow-auto bg-bg-secondary">
    <EmptyState v-if="loading" title="加载中" busy />
    <EmptyState
      v-else-if="!hasEditingEntry"
      :icon="FileText"
      title="选择一条记录"
      description="从左侧列表选择一条记录以查看详情"
    />
    <template v-else>
      <div class="mx-auto grid w-full max-w-[920px] px-5 py-5 lg:px-8 lg:py-7">
        <p
          v-if="knowledgeError"
          class="mb-4 mt-0 rounded-md border border-danger/25 bg-bg-elevated px-3 py-2 text-ui text-danger"
          role="alert"
        >
          {{ knowledgeError }}
        </p>

        <article
          class="elevation-panel grid overflow-visible rounded-lg bg-bg-elevated"
        >
          <input
            v-model="title"
            class="mx-5 mt-4 h-[52px] w-[calc(100%_-_40px)] border-none bg-transparent text-display text-text-primary outline-none placeholder:text-text-placeholder ring-focus disabled:opacity-55"
            type="text"
            placeholder="标题"
            aria-label="标题"
            :disabled="editorDisabled"
          />

          <div
            class="grid grid-cols-2 gap-3 border-y border-border bg-bg-secondary px-5 py-3"
          >
            <EntryTypeSelect v-model="entryType" :disabled="editorDisabled" />
            <EntryStatusSelect v-model="status" :disabled="editorDisabled" />
          </div>

          <div class="border-b border-border px-5 py-3">
            <TagInput
              ref="tagInputRef"
              v-model="tags"
              :disabled="editorDisabled"
            />
          </div>

          <div class="relative min-h-[260px]">
            <textarea
              ref="contentEditorRef"
              v-model="currentContent"
              class="min-h-[300px] w-full resize-y border-none bg-transparent px-5 py-4 font-sans text-body text-text-primary outline-none placeholder:text-text-placeholder ring-focus disabled:opacity-55"
              role="combobox"
              aria-label="正文"
              :aria-autocomplete="wikiLinkSuggestionsOpen ? 'list' : undefined"
              :aria-expanded="wikiLinkSuggestionsOpen ? 'true' : 'false'"
              :aria-controls="
                wikiLinkSuggestionsOpen ? wikiLinkListboxId : undefined
              "
              :aria-activedescendant="
                wikiLinkSuggestionsOpen
                  ? `${wikiLinkListboxId}-option-${wikiLinkActiveIndex}`
                  : undefined
              "
              :disabled="editorDisabled"
              @input="refreshWikiLinkCompletion"
              @click="refreshWikiLinkCompletion"
              @keyup="handleWikiLinkKeyup"
              @keydown="handleWikiLinkKeydown"
              @blur="resetWikiLinkCompletion"
            />
            <WikiLinkSuggestions
              v-if="wikiLinkSuggestionsOpen"
              :suggestions="wikiLinkSuggestions"
              :active-index="wikiLinkActiveIndex"
              :listbox-id="wikiLinkListboxId"
              @select="selectWikiLinkSuggestion"
            />
          </div>
        </article>

        <details class="group mt-4 py-4">
          <summary
            class="cursor-pointer select-none list-none rounded-sm text-ui font-medium text-text-secondary outline-none ring-focus marker:hidden [&::-webkit-details-marker]:hidden"
          >
            <span
              class="inline-flex items-center gap-1.5 group-open:text-text-primary"
            >
              <ChevronRight
                :size="14"
                class="transition-transform duration-fast ease-token group-open:rotate-90"
                aria-hidden="true"
              />
              原始内容
            </span>
          </summary>
          <pre
            class="mb-0 mt-3 max-h-[260px] overflow-auto whitespace-pre-wrap rounded-md bg-bg-inset p-4 font-mono text-caption text-text-secondary"
            >{{ originalContent }}</pre>
        </details>

        <KnowledgeRelations
          :entry-id="editingEntryId!"
          :revision="baseRevision"
          :refresh-token="props.refreshToken"
          @open-related="emit('openRelated', $event)"
        />
      </div>

      <ConfirmDialog
        :open="confirmTrash"
        title="移到回收站"
        message="这条记录会进入回收站，可从回收站恢复。"
        confirm-label="移到回收站"
        danger
        @cancel="confirmTrash = false"
        @confirm="confirmMoveToTrash"
      />
      <ConfirmDialog
        :open="confirmDelete"
        title="永久删除"
        message="永久删除后无法从应用内恢复。"
        confirm-label="永久删除"
        danger
        @cancel="confirmDelete = false"
        @confirm="confirmDeleteForever"
      />
    </template>
  </section>
</template>
