<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { RotateCcw, Trash2 } from "lucide-vue-next";

import { useAutosave } from "../../composables/useAutosave";
import { entriesUpdate } from "../../services/entryApi";
import type {
  EntryDetail,
  EntryPatch,
  EntryStatus,
  EntryType,
} from "../../types/generated";
import ConfirmDialog from "../shared/ConfirmDialog.vue";
import IconButton from "../shared/IconButton.vue";
import SaveState from "../shared/SaveState.vue";
import EntryStatusSelect from "./EntryStatusSelect.vue";
import EntryTypeSelect from "./EntryTypeSelect.vue";
import TagInput from "./TagInput.vue";

const props = defineProps<{
  detail: EntryDetail | null;
  loading: boolean;
}>();

const emit = defineEmits<{
  saved: [entry: EntryDetail];
  trash: [];
  restore: [];
  deleteForever: [];
}>();

const title = ref("");
const currentContent = ref("");
const entryType = ref<EntryType>("unclear");
const status = ref<EntryStatus>("pending");
const tags = ref<string[]>([]);
const baseRevision = ref(0);
const editingEntryId = ref<string | null>(null);
const confirmTrash = ref(false);
const confirmDelete = ref(false);
let initializing = false;
let syncVersion = 0;

const autosave = useAutosave<EntryDetail>({
  delay: 500,
  save: () => {
    const patch: EntryPatch = {
      title: title.value,
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
    const currentSync = ++syncVersion;
    initializing = true;
    await autosave.flush();
    if (currentSync !== syncVersion) {
      return;
    }
    editingEntryId.value = entry?.id || null;
    title.value = entry?.title || "";
    currentContent.value = entry?.currentContent || "";
    entryType.value = entry?.entryType || "unclear";
    status.value = entry?.status || "pending";
    tags.value = entry?.tags.map((tag) => tag.name) || [];
    baseRevision.value = entry?.revision || 0;
    autosave.reset();
    await nextTick();
    initializing = false;
  },
  { immediate: true },
);

watch(
  [title, currentContent, entryType, status, tags],
  () => {
    if (!initializing && props.detail && !props.detail.deletedAt) {
      autosave.markDirty();
    }
  },
  { deep: true },
);

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
  await autosave.flush();
  return autosave.state.value !== "failed";
}

defineExpose({
  flushPendingSave,
});
</script>

<template>
  <section class="entry-detail">
    <div
      v-if="loading"
      class="empty-state"
    >
      加载中
    </div>
    <div
      v-else-if="!detail"
      class="empty-state"
    >
      选择一条记录
    </div>
    <template v-else>
      <header class="detail-toolbar">
        <SaveState
          :state="autosave.state.value"
          :error="autosave.error.value"
        />
        <div class="toolbar-actions">
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

      <input
        v-model="title"
        class="title-input"
        type="text"
        placeholder="标题"
        :disabled="Boolean(detail.deletedAt)"
      >

      <div class="detail-controls">
        <EntryTypeSelect v-model="entryType" />
        <EntryStatusSelect v-model="status" />
      </div>

      <TagInput v-model="tags" />

      <textarea
        v-model="currentContent"
        class="content-editor"
        :disabled="Boolean(detail.deletedAt)"
      />

      <details class="original-content">
        <summary>原始内容</summary>
        <pre>{{ detail.originalContent }}</pre>
      </details>

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
