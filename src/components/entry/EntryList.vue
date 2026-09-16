<script setup lang="ts">
import { Inbox, Loader2 } from "lucide-vue-next";
import {
  computed,
  nextTick,
  onMounted,
  ref,
  type ComponentPublicInstance,
} from "vue";
import { useVirtualizer } from "@tanstack/vue-virtual";

import type { EntryListItem as EntryListItemType } from "../../types/generated";
import EmptyState from "../shared/EmptyState.vue";
import SkeletonText from "../shared/SkeletonText.vue";
import EntryListItem from "./EntryListItem.vue";

const props = defineProps<{
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
const scrollRef = ref<HTMLElement | null>(null);

// Virtualize the row list so thousands of "load more" results stay cheap to
// render. Row heights vary (1-2 line summaries, search snippets), so each row
// is measured after mount and the load-more button is the final virtual row.
// The options must stay reactive: vue-virtual only forwards them to the
// instance when its own computed re-evaluates, so a plain object would freeze
// `count` at its setup-time value and rows would never appear.
const virtualizer = useVirtualizer(
  computed(() => ({
    count: props.items.length + (props.hasMore ? 1 : 0),
    getScrollElement: () => scrollRef.value,
    estimateSize: () => 96,
    overscan: 8,
    getItemKey: (index: number) =>
      index < props.items.length ? props.items[index].id : "__more__",
  })),
);

/** Inside a double-quoted attribute selector only the quote and backslash are
 * special. Hand-rolled because jsdom provides no CSS.escape. */
function quoteAttrValue(value: string) {
  return value.replace(/["\\]/g, "\\$&");
}

/** Targeted lookup by id. Avoids materialising every rendered row, which made
 * each arrow keypress cost O(list length) DOM work. */
function entryButton(id: string) {
  return (
    listRef.value?.querySelector<HTMLButtonElement>(
      `.entry-list-item[data-entry-id="${quoteAttrValue(id)}"]`,
    ) ?? null
  );
}

function focusItemAt(index: number) {
  const item = props.items[index];
  if (!item) {
    return;
  }
  const target = entryButton(item.id);
  if (target) {
    target.focus();
    emit("select", item.id);
    return;
  }
  // Not yet rendered (scrolled out of the virtual window): scroll it into view,
  // then focus once the newly visible rows have been mounted.
  void scrollAndFocus(item.id, index);
}

async function scrollAndFocus(id: string, index: number) {
  virtualizer.value.scrollToIndex(index, { align: "auto" });
  await nextTick();
  const target = entryButton(id);
  if (target) {
    target.focus();
  }
  emit("select", id);
}

function measureRow(node: Element | ComponentPublicInstance | null) {
  virtualizer.value.measureElement(node as Element | null);
}

onMounted(() => {
  // Some environments (jsdom) have no ResizeObserver, so the virtualizer never
  // learns the scroll container size on its own; measure once explicitly.
  virtualizer.value.measure();
});

/** Resolve the focused row from the active element's own id rather than by
 * comparing against every row. */
function activeIndex() {
  const active = document.activeElement;
  if (!(active instanceof HTMLElement)) {
    return -1;
  }
  const id = active.dataset.entryId;
  if (!id || !active.classList.contains("entry-list-item")) {
    return -1;
  }
  return props.items.findIndex((item) => item.id === id);
}

function focusRelative(offset: number) {
  const current = props.items.findIndex((item) => item.id === props.selectedId);
  const next = Math.max(0, Math.min(props.items.length - 1, current + offset));
  focusItemAt(current < 0 ? 0 : next);
}

defineExpose({ focusRelative });

function onKeydown(event: KeyboardEvent) {
  const total = props.items.length;
  if (total === 0) {
    return;
  }

  const currentIndex = activeIndex();

  if (event.key === "ArrowDown") {
    event.preventDefault();
    if (currentIndex === -1) {
      focusItemAt(0);
    } else {
      focusItemAt(Math.min(currentIndex + 1, total - 1));
    }
  } else if (event.key === "ArrowUp") {
    event.preventDefault();
    if (currentIndex === -1) {
      focusItemAt(total - 1);
    } else {
      focusItemAt(Math.max(currentIndex - 1, 0));
    }
  } else if (event.key === "Home") {
    event.preventDefault();
    focusItemAt(0);
  } else if (event.key === "End") {
    event.preventDefault();
    focusItemAt(total - 1);
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
      ref="scrollRef"
      data-testid="entry-items"
      class="min-h-0 flex-1 overflow-auto px-2 pb-2"
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
      <div
        v-else
        class="relative w-full"
        :style="{ height: `${virtualizer.getTotalSize()}px` }"
      >
        <div
          v-for="virtualRow in virtualizer.getVirtualItems()"
          :ref="measureRow"
          :key="
            virtualRow.index < items.length
              ? items[virtualRow.index].id
              : '__more__'
          "
          :data-index="virtualRow.index"
          class="absolute left-0 top-0 w-full pb-1"
          :style="{ transform: `translateY(${virtualRow.start}px)` }"
        >
          <EntryListItem
            v-if="virtualRow.index < items.length"
            :item="items[virtualRow.index]"
            :active="items[virtualRow.index].id === selectedId"
            @select="$emit('select', $event)"
          />
          <div v-else class="p-3">
            <button
              type="button"
              class="btn-secondary w-full text-ui"
              :disabled="loading"
              @click="$emit('more')"
            >
              <Loader2 v-if="loading" :size="14" aria-hidden="true" />
              {{ loading ? "加载中" : "加载更多" }}
            </button>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>
