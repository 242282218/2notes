<script setup lang="ts">
import { Inbox, Loader2 } from "lucide-vue-next";

import { ref } from "vue";

import type { EntryListItem as EntryListItemType } from "../../types/generated";
import EmptyState from "../shared/EmptyState.vue";
import SkeletonText from "../shared/SkeletonText.vue";
import EntryListItem from "./EntryListItem.vue";

defineProps<{
  items: EntryListItemType[];
  selectedId: string | null;
  loading: boolean;
  hasMore: boolean;
}>();

const emit = defineEmits<{
  select: [id: string];
  more: [];
}>();

const listRef = ref<HTMLElement | null>(null);

function itemButtons() {
  return Array.from(
    listRef.value?.querySelectorAll<HTMLButtonElement>(".entry-list-item") ??
      [],
  );
}

function focusItemAt(index: number) {
  const target = itemButtons()[index];
  if (target) {
    target.focus();
    emit("select", target.dataset.entryId!);
  }
}

function onKeydown(event: KeyboardEvent) {
  const buttons = itemButtons();
  if (buttons.length === 0) {
    return;
  }

  const currentIndex = buttons.findIndex(
    (button) => button === document.activeElement,
  );

  if (event.key === "ArrowDown") {
    event.preventDefault();
    if (currentIndex === -1) {
      focusItemAt(0);
    } else {
      focusItemAt(Math.min(currentIndex + 1, buttons.length - 1));
    }
  } else if (event.key === "ArrowUp") {
    event.preventDefault();
    if (currentIndex === -1) {
      focusItemAt(buttons.length - 1);
    } else {
      focusItemAt(Math.max(currentIndex - 1, 0));
    }
  } else if (event.key === "Home") {
    event.preventDefault();
    focusItemAt(0);
  } else if (event.key === "End") {
    event.preventDefault();
    focusItemAt(buttons.length - 1);
  }
}
</script>

<template>
  <section
    ref="listRef"
    class="entry-list flex min-h-0 flex-col bg-bg-elevated ring-focus ring-inset"
    aria-label="条目列表"
    @keydown="onKeydown"
  >
    <header class="flex h-[40px] shrink-0 items-center justify-between px-4">
      <strong
        class="text-micro font-semibold uppercase tracking-wide text-text-secondary"
        >记录</strong
      >
      <span class="text-caption tabular-nums text-text-tertiary">{{
        items.length
      }}</span>
    </header>
    <div
      data-testid="entry-items"
      class="flex min-h-0 flex-1 flex-col gap-1 overflow-auto px-2 pb-2"
    >
      <div
        v-if="loading && !items.length"
        class="grid gap-1"
        role="status"
        aria-label="正在加载条目"
        aria-busy="true"
      >
        <div
          v-for="index in 6"
          :key="index"
          data-entry-skeleton
          class="min-h-24 rounded-md bg-bg-secondary px-3 py-3"
        >
          <SkeletonText :lines="3" />
        </div>
      </div>
      <EmptyState
        v-else-if="!items.length"
        :icon="Inbox"
        title="暂无条目"
        description="这里会显示符合条件的记录"
        size="fill"
      />
      <EntryListItem
        v-for="item in items"
        :key="item.id"
        :item="item"
        :active="item.id === selectedId"
        @select="$emit('select', $event)"
      />
      <button
        v-if="hasMore"
        type="button"
        class="btn-secondary m-3 w-[calc(100%_-_24px)] text-ui"
        :disabled="loading"
        @click="$emit('more')"
      >
        <Loader2 v-if="loading" :size="14" aria-hidden="true" />
        {{ loading ? "加载中" : "加载更多" }}
      </button>
    </div>
  </section>
</template>
