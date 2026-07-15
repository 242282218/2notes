<script setup lang="ts">
import { Inbox, Search, Settings, Tags, Trash2 } from "lucide-vue-next";

import type { AppView } from "../../app/routes";

defineProps<{
  view: AppView;
}>();

defineEmits<{
  change: [view: AppView];
}>();

const navItems: Array<{ view: AppView; label: string; icon: unknown }> = [
  { view: "inbox", label: "收集箱", icon: Inbox },
  { view: "search", label: "搜索", icon: Search },
  { view: "tags", label: "标签", icon: Tags },
  { view: "trash", label: "回收站", icon: Trash2 },
  { view: "settings", label: "设置", icon: Settings },
];
</script>

<template>
  <nav class="sidebar-nav">
    <button
      v-for="item in navItems"
      :key="item.view"
      type="button"
      :class="{ active: item.view === view }"
      @click="$emit('change', item.view)"
    >
      <component :is="item.icon" :size="18" />
      <span>{{ item.label }}</span>
    </button>
  </nav>
</template>
