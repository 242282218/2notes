<script setup lang="ts">
import EntryListItem from "./EntryListItem.vue";
import type { EntryListItem as EntryListItemType } from "../../types/generated";

defineProps<{
  items: EntryListItemType[];
  selectedId: string | null;
  loading: boolean;
  hasMore: boolean;
}>();

defineEmits<{
  select: [id: string];
  more: [];
}>();
</script>

<template>
  <section class="entry-list">
    <div v-if="loading && !items.length" class="empty-state">加载中</div>
    <div v-else-if="!items.length" class="empty-state">暂无条目</div>
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
      class="load-more"
      :disabled="loading"
      @click="$emit('more')"
    >
      加载更多
    </button>
  </section>
</template>
