<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { RotateCcw, Trash2 } from "lucide-vue-next";

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
import ConfirmDialog from "../shared/ConfirmDialog.vue";
import IconButton from "../shared/IconButton.vue";
import SaveState from "../shared/SaveState.vue";
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
  }>(),
  { refreshToken: 0 },
);

const emit = defineEmits<{
  saved: [entry: EntryDetail];
  trash: [];
  restore: [];
  deleteForever: [];
  openRelated: [id: string];
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
let initializing = false;
let syncVersion = 0;
let titleDirty = false;
let wikiLinkRequestId = 0;
let wikiLinkTimer: number | undefined;
let wikiLinkHandledKey: string | null = null;

const autosave = useAutosave<EntryDetail>({
  delay: 500,
  save: () => {
    const patch: EntryPatch = {
      title: titleDirty ? title.value : null,
      currentContent: currentContent.value,
      entryType: entryType.value,
      status: status.value,
      tags: tags.value,
    };
    if (!editingEntryId.value) {
      throw new Error("未选择条目");
    }
    return entriesUpdate(editingEntryId.value, patch, baseRevision.value);
  },
  onSaved: (entry) => {
    if (props.detail?.id === entry.id) {
      baseRevision.value = entry.revision;
      titleDirty = false;
      emit("saved", entry);
    }
  },
  onStaleSaved: (entry) => {
    if (editingEntryId.value === entry.id) {
      baseRevision.value = entry.revision;
    }
  },
});

watch(
  () => props.detail,
  async (entry) => {
    resetWikiLinkCompletion();
    const currentSync = ++syncVersion;
    initializing = true;
    try {
      // Flush must complete before we touch local refs, but stale flushes
      // from older watcher invocations must be discarded.
      await autosave.flush();
      // Double-check that this watcher is still the newest.
      if (currentSync !== syncVersion) {
        return;
      }
    } catch {
      initializing = false;
      return;
    }
    editingEntryId.value = entry?.id || null;
    title.value = entry?.title || "";
    currentContent.value = entry?.currentContent || "";
    entryType.value = entry?.entryType || "unclear";
    status.value = entry?.status || "pending";
    tags.value = entry?.tags.map((tag) => tag.name) || [];
    baseRevision.value = entry?.revision || 0;
    titleDirty = false;
    autosave.reset();
    await nextTick();
    initializing = false;
  },
  { immediate: true },
);

onBeforeUnmount(resetWikiLinkCompletion);

watch(title, () => {
  if (!initializing && props.detail && !props.detail.deletedAt) {
    titleDirty = true;
    autosave.markDirty();
  }
});

watch([currentContent, entryType, status, tags], () => {
  if (!initializing && props.detail && !props.detail.deletedAt) {
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
  try {
    const updated = await knowledgePromote(
      editingEntryId.value,
      baseRevision.value,
    );
    baseRevision.value = updated.revision;
    emit("saved", updated);
  } catch (error) {
    knowledgeError.value = error instanceof Error ? error.message : "沉淀失败";
  }
}

async function demoteFromKnowledge() {
  knowledgeError.value = "";
  if (!(await flushPendingSave()) || !editingEntryId.value) return;
  try {
    const updated = await knowledgeDemote(
      editingEntryId.value,
      baseRevision.value,
    );
    baseRevision.value = updated.revision;
    emit("saved", updated);
  } catch (error) {
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
});
</script>

<template>
  <section class="entry-detail">
    <div v-if="loading" class="empty-state">加载中</div>
    <div v-else-if="!detail" class="empty-state">选择一条记录</div>
    <template v-else>
      <header class="detail-toolbar">
        <SaveState
          :state="autosave.state.value"
          :error="autosave.error.value"
          @retry="autosave.retry"
        />
        <div class="toolbar-actions">
          <button
            v-if="!detail.deletedAt && detail.knowledgeState === 'capture'"
            type="button"
            class="secondary-button"
            aria-label="沉淀为知识"
            :disabled="!title.trim()"
            @click="promoteToKnowledge"
          >
            沉淀
          </button>
          <button
            v-if="!detail.deletedAt && detail.knowledgeState === 'knowledge'"
            type="button"
            class="secondary-button"
            aria-label="移出知识库"
            @click="demoteFromKnowledge"
          >
            移出知识库
          </button>
          <IconButton
            v-if="detail.deletedAt"
            label="恢复"
            :icon="RotateCcw"
            @click="$emit('restore')"
          />
          <IconButton
            v-if="detail.deletedAt"
            label="永久删除"
            :icon="Trash2"
            danger
            @click="confirmDelete = true"
          />
          <IconButton
            v-else
            label="移到回收站"
            :icon="Trash2"
            danger
            @click="confirmTrash = true"
          />
        </div>
      </header>

      <p v-if="knowledgeError" class="error-text" role="alert">
        {{ knowledgeError }}
      </p>

      <input
        v-model="title"
        class="title-input"
        type="text"
        placeholder="标题"
        aria-label="标题"
        :disabled="Boolean(detail.deletedAt)"
      />

      <div class="detail-controls">
        <EntryTypeSelect
          v-model="entryType"
          :disabled="Boolean(detail.deletedAt)"
        />
        <EntryStatusSelect
          v-model="status"
          :disabled="Boolean(detail.deletedAt)"
        />
      </div>

      <TagInput
        ref="tagInputRef"
        v-model="tags"
        :disabled="Boolean(detail.deletedAt)"
      />

      <div class="content-editor-wrap">
        <textarea
          ref="contentEditorRef"
          v-model="currentContent"
          class="content-editor"
          role="combobox"
          aria-label="正文"
          :aria-autocomplete="wikiLinkSuggestionsOpen ? 'list' : undefined"
          :aria-expanded="wikiLinkSuggestionsOpen ? 'true' : undefined"
          :aria-controls="
            wikiLinkSuggestionsOpen ? wikiLinkListboxId : undefined
          "
          :aria-activedescendant="
            wikiLinkSuggestionsOpen
              ? `${wikiLinkListboxId}-option-${wikiLinkActiveIndex}`
              : undefined
          "
          :disabled="Boolean(detail.deletedAt)"
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

      <details class="original-content">
        <summary>原始内容</summary>
        <pre>{{ detail.originalContent }}</pre>
      </details>

      <KnowledgeRelations
        :entry-id="detail.id"
        :revision="detail.revision"
        :refresh-token="props.refreshToken"
        @open-related="emit('openRelated', $event)"
      />

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
