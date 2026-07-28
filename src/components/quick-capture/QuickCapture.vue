<script setup lang="ts">
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Clipboard, Send, X } from "lucide-vue-next";
import { computed, onMounted, onUnmounted, ref } from "vue";

import { useAppQuitRequest } from "../../composables/useAppQuitRequest";
import { databaseRestoreReady } from "../../services/backupApi";
import { useAutosave } from "../../composables/useAutosave";
import { revealCurrentWindow } from "../../composables/useWindowReveal";
import {
  draftGet,
  draftUpdate,
  quickCaptureSubmit,
} from "../../services/draftApi";
import { windowHideQuickCapture } from "../../services/windowApi";
import IconButton from "../shared/IconButton.vue";

const content = ref("");
const revision = ref(0);
const hydrated = ref(false);
const submitting = ref(false);
const error = ref<string | null>(null);
const textareaRef = ref<HTMLTextAreaElement | null>(null);
let unlistenDatabaseRestored: (() => void) | null = null;
let unlistenDatabaseRestorePrepare: (() => void) | null = null;

const autosave = useAutosave({
  delay: 250,
  save: () => draftUpdate(content.value, revision.value),
  onSaved: (draft) => {
    revision.value = draft.revision;
    error.value = null;
  },
  onStaleSaved: (draft) => {
    revision.value = draft.revision;
  },
  onFailed: (saveError) => {
    error.value =
      saveError instanceof Error ? saveError.message : "草稿保存失败";
  },
});

const saving = computed(() => autosave.state.value === "saving");
const statusText = computed(() => {
  if (!hydrated.value) return "正在加载草稿";
  if (submitting.value) return "正在保存";
  if (saving.value) return "草稿保存中";
  return "就绪";
});

useAppQuitRequest(async () => {
  try {
    await autosave.flush();
    return true;
  } catch {
    return false;
  }
});

async function hydrateDraft() {
  hydrated.value = false;
  autosave.reset();
  try {
    const draft = await draftGet();
    content.value = draft.content;
    revision.value = draft.revision;
  } catch (loadError) {
    error.value =
      loadError instanceof Error ? loadError.message : "草稿加载失败";
  } finally {
    hydrated.value = true;
    textareaRef.value?.focus();
  }
}

type DatabaseRestorePreparePayload = {
  requestId: string;
};

async function prepareDatabaseRestore(requestId: string) {
  try {
    await autosave.flush();
    await databaseRestoreReady(requestId);
  } catch (restoreError) {
    error.value =
      restoreError instanceof Error ? restoreError.message : "草稿保存失败";
    await revealCurrentWindow();
  }
}

onMounted(async () => {
  await hydrateDraft();
  if (isTauri()) {
    unlistenDatabaseRestored = await listen("database-restored", hydrateDraft);
    unlistenDatabaseRestorePrepare =
      await listen<DatabaseRestorePreparePayload>(
        "database-restore-prepare",
        (event) => prepareDatabaseRestore(event.payload.requestId),
      );
  }
});

onUnmounted(() => {
  autosave.dispose();
  unlistenDatabaseRestored?.();
  unlistenDatabaseRestorePrepare?.();
});

function onContentChange() {
  if (!hydrated.value) {
    return;
  }
  error.value = null;
  autosave.markDirty();
}

async function submit() {
  const trimmed = content.value.trim();
  if (!trimmed || submitting.value) {
    return;
  }
  submitting.value = true;
  error.value = null;
  try {
    await autosave.flush();
    const cleared = await quickCaptureSubmit(trimmed, revision.value);
    hydrated.value = false;
    try {
      content.value = "";
      revision.value = cleared.revision;
      await windowHideQuickCapture();
    } finally {
      hydrated.value = true;
    }
  } catch (submitError) {
    error.value =
      submitError instanceof Error ? submitError.message : "提交失败";
  } finally {
    submitting.value = false;
  }
}

function onSubmitKeydown(event: KeyboardEvent) {
  if (event.isComposing) {
    return;
  }
  event.preventDefault();
  void submit();
}

async function copyContent() {
  try {
    await navigator.clipboard.writeText(content.value);
  } catch (copyError) {
    error.value = copyError instanceof Error ? copyError.message : "复制失败";
  }
}

async function hideQuickCapture() {
  try {
    await autosave.flush();
    await windowHideQuickCapture();
  } catch {
    await revealCurrentWindow();
  }
}
</script>

<template>
  <main
    data-quick-capture-shell
    class="flex h-screen w-screen min-h-0 min-w-0 items-center justify-center overflow-hidden bg-bg-base p-3"
  >
    <Transition
      appear
      enter-from-class="translate-y-1 opacity-0"
      enter-active-class="transition-[transform,opacity] duration-base ease-token"
      enter-to-class="translate-y-0 opacity-100"
    >
      <section
        data-quick-capture-card
        class="quick-capture-card elevation-2 grid h-full min-h-0 w-full max-h-full translate-y-0 grid-rows-[34px_minmax(0,1fr)_36px] gap-3 overflow-hidden rounded-lg bg-bg-elevated p-4 animate-fade-in"
        aria-labelledby="quick-capture-title"
      >
        <header class="flex min-w-0 items-center justify-between gap-3">
          <div class="flex min-w-0 items-center gap-3">
            <h1
              id="quick-capture-title"
              class="m-0 text-title text-text-primary"
            >
              快速记录
            </h1>
            <span
              class="whitespace-nowrap text-caption text-text-tertiary"
              role="status"
              aria-live="polite"
            >
              {{ statusText }}
            </span>
          </div>
          <IconButton label="隐藏" :icon="X" @click="hideQuickCapture" />
        </header>

        <label for="quick-capture-content" class="sr-only">记录内容</label>
        <textarea
          id="quick-capture-content"
          ref="textareaRef"
          v-model="content"
          autofocus
          :disabled="!hydrated || submitting"
          :aria-invalid="Boolean(error)"
          :aria-describedby="error ? 'quick-capture-error' : undefined"
          placeholder="记下现在这件事"
          class="input-base min-h-0 resize-none p-3 text-body"
          @input="onContentChange"
          @keydown.enter.exact="onSubmitKeydown"
          @keydown.esc.prevent="hideQuickCapture"
        />

        <footer class="flex min-w-0 items-center justify-between gap-3">
          <p
            v-if="error"
            id="quick-capture-error"
            class="m-0 min-w-0 truncate text-caption text-danger"
            role="alert"
            :title="error"
          >
            {{ error }}
          </p>
          <span v-else aria-hidden="true" />
          <div class="flex shrink-0 items-center justify-end gap-2">
            <IconButton
              label="复制"
              :icon="Clipboard"
              :disabled="!hydrated || !content"
              @click="copyContent"
            />
            <button
              type="button"
              class="btn-primary"
              :disabled="!hydrated || !content.trim() || submitting"
              @click="submit"
            >
              <Send :size="16" aria-hidden="true" />
              {{ submitting ? "保存中…" : "保存" }}
            </button>
          </div>
        </footer>
      </section>
    </Transition>
  </main>
</template>

<style scoped>
@media (max-height: 250px) {
  .quick-capture-card {
    grid-template-rows: 32px minmax(0, 1fr) 34px;
    gap: var(--space-2);
    padding: var(--space-3);
  }
}
</style>
