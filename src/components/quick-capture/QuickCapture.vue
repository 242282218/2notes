<script setup lang="ts">
import { Clipboard, Send, X } from "lucide-vue-next";
import { computed, onMounted, ref } from "vue";

import { useAppQuitRequest } from "../../composables/useAppQuitRequest";
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

const autosave = useAutosave({
  delay: 250,
  save: () => draftUpdate(content.value, revision.value),
  onSaved: (draft) => {
    revision.value = draft.revision;
    error.value = null;
  },
  onFailed: (saveError) => {
    error.value =
      saveError instanceof Error ? saveError.message : "草稿保存失败";
  },
});

const saving = computed(() => autosave.state.value === "saving");

useAppQuitRequest(async () => {
  try {
    await autosave.flush();
    return true;
  } catch {
    return false;
  }
});

onMounted(async () => {
  try {
    const draft = await draftGet();
    content.value = draft.content;
    revision.value = draft.revision;
  } catch (loadError) {
    error.value =
      loadError instanceof Error ? loadError.message : "草稿加载失败";
  } finally {
    hydrated.value = true;
    await autosave.reset();
    textareaRef.value?.focus();
  }
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
    const cleared = await quickCaptureSubmit(trimmed);
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

async function copyContent() {
  await navigator.clipboard.writeText(content.value);
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
      ref="textareaRef"
      v-model="content"
      autofocus
      :disabled="!hydrated || submitting"
      placeholder="记下现在这件事"
      @input="onContentChange"
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
          :disabled="!hydrated"
          @click="copyContent"
        />
        <button
          type="button"
          class="primary-button"
          :disabled="!hydrated || !content.trim() || submitting"
          @click="submit"
        >
          <Send :size="16" />
          保存
        </button>
      </div>
    </footer>
  </main>
</template>
