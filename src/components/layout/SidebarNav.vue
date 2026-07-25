<script setup lang="ts">
import {
  BookOpen,
  Feather,
  Inbox,
  Search,
  Settings,
  Tags,
  Trash2,
} from "lucide-vue-next";

import type { AppView } from "../../app/routes";

defineProps<{
  view: AppView;
}>();

defineEmits<{
  change: [view: AppView];
}>();

const navItems: Array<{ view: AppView; label: string; icon: unknown }> = [
  { view: "inbox", label: "收集箱", icon: Inbox },
  { view: "knowledge", label: "知识库", icon: BookOpen },
  { view: "search", label: "搜索", icon: Search },
  { view: "tags", label: "标签", icon: Tags },
  { view: "trash", label: "回收站", icon: Trash2 },
  { view: "settings", label: "设置", icon: Settings },
];
</script>

<template>
  <nav class="flex flex-col gap-1 border-r border-border-subtle p-2 glass-panel z-10 md:p-3">
    <header class="flex h-[44px] items-center justify-center gap-2 mb-2 text-base font-bold tracking-tight text-text-primary md:justify-start md:px-3">
      <Feather :size="20" class="text-brand drop-shadow-[0_0_12px_rgba(15,105,120,0.25)] dark:drop-shadow-[0_0_16px_rgba(63,179,195,0.35)]" />
      <span class="hidden md:inline">2notes</span>
    </header>
    <button
      v-for="item in navItems"
      :key="item.view"
      type="button"
      class="group relative flex h-[38px] items-center justify-center gap-2 rounded-md border-none bg-transparent text-text-secondary transition-all duration-150 ring-focus hover:bg-bg-hover hover:text-text-primary hover:translate-x-0.5 active:scale-97 active:translate-x-[1px] md:grid md:grid-cols-[22px_1fr] md:justify-items-start md:px-3 md:text-left"
      :class="{ 'font-semibold text-brand bg-brand-subtle hover:bg-brand-subtle hover:text-brand': item.view === view }"
      :aria-current="item.view === view ? 'page' : undefined"
      @click="$emit('change', item.view)"
    >
      <!-- Active Indicator (Left bar) - only on desktop -->
      <div
        class="absolute left-0 top-2 bottom-2 w-[3px] rounded-r-full bg-brand opacity-0 transition-opacity duration-150 shadow-glow hidden md:block"
        :class="{ 'opacity-100': item.view === view }"
      ></div>
      <component :is="item.icon" :size="18" />
      <span class="hidden md:inline">{{ item.label }}</span>
    </button>
  </nav>
</template>
