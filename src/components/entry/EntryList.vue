<script setup lang="ts">
import { Inbox, Loader2 } from "lucide-vue-next";

import type { EntryListItem as EntryListItemType } from "../../types/generated";
import EmptyState from "../shared/EmptyState.vue";
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

function focusItemAt(index: number) {
  const buttons = document.querySelectorAll<HTMLButtonElement>(
    ".entry-list .entry-list-item",
  );
  const target = buttons[index];
  if (target) {
    target.focus();
    emit("select", target.dataset.entryId!);
  }
}

function onKeydown(event: KeyboardEvent) {
  const buttons = Array.from(
    document.querySelectorAll<HTMLButtonElement>(
      ".entry-list .entry-list-item",
    ),
  );
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
    class="entry-list flex min-h-0 flex-col overflow-auto border-r border-border bg-bg-elevated focus-visible:outline-none focus-visible:shadow-[inset_0_0_0_2px_var(--color-focus-ring)]"
    aria-label="条目列表"
    @keydown="onKeydown"
  >
    <EmptyState
      v-if="loading && !items.length"
      :icon="Loader2"
      title="加载中"
    />
    <EmptyState
      v-else-if="!items.length"
      :icon="Inbox"
      title="暂无条目"
      description="这里会显示符合条件的记录"
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
      class="mx-3 my-3 h-[36px] w-[calc(100%-24px)] rounded-md border border-border-strong bg-bg-elevated text-text-secondary transition-colors duration-150 hover:border-border-hover hover:bg-bg-hover active:bg-bg-active disabled:opacity-55"
      :disabled="loading"
      @click="$emit('more')"
    >
      加载更多
    </button>
  </section>
</template>
