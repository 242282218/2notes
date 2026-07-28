<script setup lang="ts">
import { Filter, Search, SquarePen, Tags } from "lucide-vue-next";
import { nextTick, onMounted, onUnmounted, ref } from "vue";

import type { AppView } from "../../app/routes";
import type { EntryStatus, EntryType } from "../../types/generated";
import type { EntryDetailToolbarState } from "../entry/entryDetailToolbar";
import DetailActionBar from "./DetailActionBar.vue";
import EntryFilterPopover from "./EntryFilterPopover.vue";

const props = defineProps<{
  viewLabel: string;
  modelValue: string;
  view: AppView;
  currentTag: string;
  entryType: EntryType | "";
  status: EntryStatus | "";
  showDetailActions: boolean;
  toolbarState: EntryDetailToolbarState;
  tagPanelOpen: boolean;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: string];
  "update:tagPanelOpen": [open: boolean];
  typeChange: [value: EntryType | ""];
  statusChange: [value: EntryStatus | ""];
  quickCapture: [];
  retry: [];
  promote: [];
  demote: [];
  restore: [];
  deleteForever: [];
  moveToTrash: [];
}>();

const typeOptions: Array<{ value: EntryType | ""; label: string }> = [
  { value: "", label: "全部类型" },
  { value: "unclear", label: "未澄清" },
  { value: "idea", label: "想法" },
  { value: "task", label: "任务" },
  { value: "material", label: "素材" },
  { value: "question", label: "问题" },
];

const statusOptions: Array<{ value: EntryStatus | ""; label: string }> = [
  { value: "", label: "全部状态" },
  { value: "pending", label: "待处理" },
  { value: "done", label: "已完成" },
  { value: "archived", label: "已归档" },
];

const searchInputRef = ref<HTMLInputElement | null>(null);
const filterPanelOpen = ref(false);
const filterMenuRef = ref<HTMLElement | null>(null);
const filterButtonRef = ref<HTMLButtonElement | null>(null);
const tagButtonRef = ref<HTMLButtonElement | null>(null);

function focusSearch() {
  searchInputRef.value?.focus();
}

function closeFilterPanel(returnFocus = false) {
  if (!filterPanelOpen.value) return;
  filterPanelOpen.value = false;
  if (returnFocus) {
    void nextTick(() => filterButtonRef.value?.focus());
  }
}

function onFilterClose(returnFocus?: boolean) {
  closeFilterPanel(returnFocus ?? false);
}

function toggleTagPanel() {
  emit("update:tagPanelOpen", !props.tagPanelOpen);
}

function onDocumentPointerDown(event: globalThis.PointerEvent) {
  const target = event.target as globalThis.Node;
  if (filterPanelOpen.value && !filterMenuRef.value?.contains(target)) {
    closeFilterPanel();
  }
}

onMounted(() => {
  document.addEventListener("pointerdown", onDocumentPointerDown);
});

onUnmounted(() => {
  document.removeEventListener("pointerdown", onDocumentPointerDown);
});

defineExpose({
  focusSearch,
  tagButtonRef,
  filterButtonRef,
  closeFilterPanel,
});
</script>

<template>
  <header
    data-testid="topbar-layout"
    class="sticky top-0 z-20 grid h-[52px] min-w-0 shrink-0 grid-cols-[minmax(0,1fr)_minmax(120px,min(480px,calc(100%_-_960px)))_minmax(0,1fr)] items-center gap-2 border-b border-border bg-bg-elevated px-3 md:px-4"
  >
    <div
      data-testid="topbar-start"
      class="flex min-w-0 items-center justify-start"
    >
      <h1 class="hidden text-heading text-text-primary xl:block">
        {{ viewLabel }}
      </h1>
    </div>

    <div class="relative min-w-0 text-text-secondary">
      <div class="relative w-full text-text-secondary">
        <Search
          :size="16"
          class="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2"
          aria-hidden="true"
        />
        <input
          id="global-search"
          ref="searchInputRef"
          :value="modelValue"
          type="search"
          aria-label="搜索"
          placeholder="搜索标题与内容"
          class="input-base h-[36px] min-w-[120px] pl-9 pr-3 text-ui"
          @input="
            emit('update:modelValue', ($event.target as HTMLInputElement).value)
          "
        />
      </div>
    </div>

    <div
      data-testid="topbar-end"
      class="flex min-w-0 items-center justify-end gap-1"
    >
      <button
        v-if="view === 'tags'"
        ref="tagButtonRef"
        type="button"
        class="btn-secondary min-w-0 px-2.5 text-ui"
        aria-haspopup="true"
        :aria-expanded="tagPanelOpen ? 'true' : 'false'"
        aria-controls="tag-panel"
        @click="toggleTagPanel"
      >
        <Tags :size="15" aria-hidden="true" />
        <span class="hidden xl:inline">{{
          currentTag ? `#${currentTag}` : "选择标签"
        }}</span>
      </button>

      <select
        class="select-base hidden w-[112px] text-ui xl:block"
        aria-label="类型筛选"
        :value="entryType"
        @change="
          emit(
            'typeChange',
            ($event.target as HTMLSelectElement).value as EntryType | '',
          )
        "
      >
        <option
          v-for="option in typeOptions"
          :key="option.value || 'all'"
          :value="option.value"
        >
          {{ option.label }}
        </option>
      </select>
      <select
        class="select-base hidden w-[112px] text-ui xl:block"
        aria-label="状态筛选"
        :value="status"
        @change="
          emit(
            'statusChange',
            ($event.target as HTMLSelectElement).value as EntryStatus | '',
          )
        "
      >
        <option
          v-for="option in statusOptions"
          :key="option.value || 'all'"
          :value="option.value"
        >
          {{ option.label }}
        </option>
      </select>

      <div ref="filterMenuRef" class="relative xl:hidden">
        <button
          ref="filterButtonRef"
          type="button"
          class="btn-secondary min-w-0 px-2.5 text-ui"
          aria-haspopup="true"
          :aria-expanded="filterPanelOpen ? 'true' : 'false'"
          aria-controls="entry-filter-popover"
          @click="filterPanelOpen = !filterPanelOpen"
        >
          <Filter :size="15" aria-hidden="true" />
          <span class="hidden min-[1100px]:inline">筛选</span>
        </button>
        <EntryFilterPopover
          :open="filterPanelOpen"
          :entry-type="entryType"
          :status="status"
          @type-change="emit('typeChange', $event)"
          @status-change="emit('statusChange', $event)"
          @close="onFilterClose"
        />
      </div>

      <DetailActionBar
        v-if="showDetailActions"
        :toolbar-state="toolbarState"
        @retry="emit('retry')"
        @promote="emit('promote')"
        @demote="emit('demote')"
        @restore="emit('restore')"
        @delete-forever="emit('deleteForever')"
        @move-to-trash="emit('moveToTrash')"
      />

      <button
        type="button"
        class="btn-primary min-w-0 px-3 text-ui"
        @click="emit('quickCapture')"
      >
        <SquarePen :size="15" aria-hidden="true" />
        <span class="hidden min-[1100px]:inline">记录</span>
      </button>
    </div>
  </header>
</template>
