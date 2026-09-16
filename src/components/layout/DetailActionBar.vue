<script setup lang="ts">
import {
  BookOpenCheck,
  BookX,
  MoreHorizontal,
  RotateCcw,
  Trash2,
} from "lucide-vue-next";
import { nextTick, onMounted, onUnmounted, ref } from "vue";

import type { EntryDetailToolbarState } from "../entry/entryDetailToolbar";
import IconButton from "../shared/IconButton.vue";
import SaveState from "../shared/SaveState.vue";

defineProps<{
  toolbarState: EntryDetailToolbarState;
}>();

const emit = defineEmits<{
  retry: [];
  promote: [];
  demote: [];
  restore: [];
  deleteForever: [];
  moveToTrash: [];
}>();

const detailMenuOpen = ref(false);
const detailMenuRef = ref<HTMLElement | null>(null);
const detailMenuButtonRef = ref<HTMLButtonElement | null>(null);

function closeDetailMenu(returnFocus = false) {
  if (!detailMenuOpen.value) return;
  detailMenuOpen.value = false;
  if (returnFocus) {
    void nextTick(() => detailMenuButtonRef.value?.focus());
  }
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape" && detailMenuOpen.value) {
    event.preventDefault();
    closeDetailMenu(true);
  }
}

function onPointerDown(event: globalThis.PointerEvent) {
  const target = event.target as globalThis.Node;
  if (detailMenuOpen.value && !detailMenuRef.value?.contains(target)) {
    closeDetailMenu();
  }
}

function runAndClose(action: () => void) {
  detailMenuOpen.value = false;
  action();
}

onMounted(() => {
  window.addEventListener("keydown", onKeydown);
  document.addEventListener("pointerdown", onPointerDown);
});

onUnmounted(() => {
  window.removeEventListener("keydown", onKeydown);
  document.removeEventListener("pointerdown", onPointerDown);
});
</script>

<template>
  <div
    data-testid="detail-actions"
    class="flex min-w-0 shrink-0 items-center gap-1"
  >
    <SaveState
      data-testid="save-state"
      :state="toolbarState.saveState"
      :error="toolbarState.saveError"
      @retry="emit('retry')"
    />
    <button
      v-if="toolbarState.showPromote"
      type="button"
      class="btn-primary hidden min-w-0 px-2.5 text-ui min-[1100px]:inline-flex"
      aria-label="沉淀为知识"
      :disabled="!toolbarState.canPromote"
      title="沉淀为知识"
      @click="emit('promote')"
    >
      <BookOpenCheck :size="15" aria-hidden="true" />
      <span class="hidden min-[1440px]:inline">沉淀为知识</span>
    </button>
    <button
      v-if="toolbarState.canDemote"
      type="button"
      class="btn-secondary hidden min-w-0 px-2.5 text-ui min-[1100px]:inline-flex"
      aria-label="移出知识库"
      title="移出知识库"
      @click="emit('demote')"
    >
      <BookX :size="15" aria-hidden="true" />
      <span class="hidden min-[1440px]:inline">移出知识库</span>
    </button>
    <IconButton
      v-if="toolbarState.deleted"
      class="hidden min-[1100px]:inline-flex"
      label="恢复"
      :icon="RotateCcw"
      @click="emit('restore')"
    />
    <IconButton
      v-if="toolbarState.deleted"
      class="hidden min-[1100px]:inline-flex"
      label="永久删除"
      :icon="Trash2"
      danger
      @click="emit('deleteForever')"
    />
    <IconButton
      v-else
      class="hidden min-[1100px]:inline-flex"
      label="移到回收站"
      :icon="Trash2"
      danger
      @click="emit('moveToTrash')"
    />

    <div ref="detailMenuRef" class="relative min-[1100px]:hidden">
      <button
        ref="detailMenuButtonRef"
        type="button"
        class="btn-icon"
        aria-label="更多详情操作"
        aria-haspopup="true"
        :aria-expanded="detailMenuOpen ? 'true' : 'false'"
        aria-controls="detail-actions-menu"
        @click="detailMenuOpen = !detailMenuOpen"
      >
        <MoreHorizontal :size="17" aria-hidden="true" />
      </button>
      <Transition name="filter-panel">
        <div
          v-if="detailMenuOpen"
          id="detail-actions-menu"
          class="elevation-2 absolute right-0 top-[40px] z-50 grid w-[180px] gap-1 rounded-lg bg-bg-elevated p-2"
          aria-label="详情操作"
        >
          <button
            v-if="toolbarState.showPromote"
            type="button"
            class="btn-secondary w-full justify-start"
            aria-label="沉淀为知识"
            :disabled="!toolbarState.canPromote"
            @click="runAndClose(() => emit('promote'))"
          >
            <BookOpenCheck :size="15" aria-hidden="true" />
            沉淀为知识
          </button>
          <button
            v-if="toolbarState.canDemote"
            type="button"
            class="btn-secondary w-full justify-start"
            aria-label="移出知识库"
            @click="runAndClose(() => emit('demote'))"
          >
            <BookX :size="15" aria-hidden="true" />
            移出知识库
          </button>
          <button
            v-if="toolbarState.deleted"
            type="button"
            class="btn-secondary w-full justify-start"
            aria-label="恢复"
            @click="runAndClose(() => emit('restore'))"
          >
            <RotateCcw :size="15" aria-hidden="true" />
            恢复
          </button>
          <button
            v-if="toolbarState.deleted"
            type="button"
            class="btn-secondary w-full justify-start text-danger"
            aria-label="永久删除"
            @click="runAndClose(() => emit('deleteForever'))"
          >
            <Trash2 :size="15" aria-hidden="true" />
            永久删除
          </button>
          <button
            v-else
            type="button"
            class="btn-secondary w-full justify-start text-danger"
            aria-label="移到回收站"
            @click="runAndClose(() => emit('moveToTrash'))"
          >
            <Trash2 :size="15" aria-hidden="true" />
            移到回收站
          </button>
        </div>
      </Transition>
    </div>
  </div>
</template>
