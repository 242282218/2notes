<script setup lang="ts">
import { Clipboard, Send, X } from "lucide-vue-next";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { onMounted, onUnmounted, ref, watch } from "vue";

import { draftClear, draftGet, draftUpdate } from "../../services/draftApi";
import { entriesCreate } from "../../services/entryApi";
import { appQuitReady, windowHideQuickCapture } from "../../services/windowApi";
import IconButton from "../shared/IconButton.vue";

const content = ref("");
const revision = ref(0);
const saving = ref(false);
const submitting = ref(false);
const error = ref<string | null>(null);
let hydrated = false;
let timer: number | undefined;
let localVersion = 0;
let draftInFlight = false;
let pendingDraftSave = false;
let activeDraftSave: Promise<void> | null = null;
let unlistenQuit: (() => void) | null = null;

onMounted(async () => {
  unlistenQuit = await listen<{ requestId: string }>(
    "app-quit-requested",
    async (event) => {
      await flushDraft();
      await appQuitReady(event.payload.requestId, getCurrentWindow().label);
    },
  );
  const draft = await draftGet();
  content.value = draft.content;
  revision.value = draft.revision;
  hydrated = true;
});

onUnmounted(() => {
  unlistenQuit?.();
});

watch(content, () => {
  if (!hydrated) {
    return;
  }
  error.value = null;
  localVersion += 1;
  if (timer) {
    window.clearTimeout(timer);
  }
  timer = window.setTimeout(() => {
    void flushDraft();
  }, 250);
});

async function flushDraft() {
  if (timer) {
    window.clearTimeout(timer);
    timer = undefined;
  }
  if (draftInFlight) {
    pendingDraftSave = true;
    await activeDraftSave;
    return;
  }
  pendingDraftSave = true;
  while (pendingDraftSave) {
    pendingDraftSave = false;
    await saveDraftOnce();
  }
}

async function saveDraftOnce() {
  if (!hydrated) {
    return;
  }
  const requestVersion = localVersion;
  draftInFlight = true;
  saving.value = true;
  activeDraftSave = (async () => {
    const draft = await draftUpdate(content.value, revision.value);
    revision.value = draft.revision;
    if (requestVersion === localVersion) {
      error.value = null;
    } else {
      pendingDraftSave = true;
    }
  })();
  try {
    await activeDraftSave;
  } catch (saveError) {
    error.value =
      saveError instanceof Error ? saveError.message : "草稿保存失败";
    throw saveError;
  } finally {
    activeDraftSave = null;
    draftInFlight = false;
    if (!pendingDraftSave) {
      saving.value = false;
    }
  }
}

async function submit() {
  const trimmed = content.value.trim();
  if (!trimmed || submitting.value) {
    return;
  }
  submitting.value = true;
  error.value = null;
  try {
    await flushDraft();
    await entriesCreate(trimmed);
    const cleared = await draftClear();
    hydrated = false;
    content.value = "";
    revision.value = cleared.revision;
    localVersion += 1;
    pendingDraftSave = false;
    hydrated = true;
    await windowHideQuickCapture();
  } catch (submitError) {
    error.value =
      submitError instanceof Error ? submitError.message : "提交失败";
  } finally {
    submitting.value = false;
  }
}

async function copyContent() {
  await navigator.clipboard.writeText(content.value);
}

async function hideQuickCapture() {
  await flushDraft();
  await windowHideQuickCapture();
}
</script>

<template>
  <main class="quick-capture">
    <header>
      <strong>快速记录</strong>
      <span>{{ saving ? "草稿保存中" : "草稿已就绪" }}</span>
      <IconButton
        label="隐藏"
        :icon="X"
        @click="hideQuickCapture"
      />
    </header>
    <textarea
      v-model="content"
      autofocus
      placeholder="记下现在这件事"
      @keydown.enter.exact.prevent="submit"
      @keydown.esc.prevent="hideQuickCapture"
    />
    <footer>
      <p
        v-if="error"
        class="error-text"
      >
        {{ error }}
      </p>
      <span v-else />
      <div class="quick-actions">
        <IconButton
          label="复制"
          :icon="Clipboard"
          @click="copyContent"
        />
        <button
          type="button"
          class="primary-button"
          :disabled="!content.trim() || submitting"
          @click="submit"
        >
          <Send :size="16" />
          保存
        </button>
      </div>
    </footer>
  </main>
</template>
