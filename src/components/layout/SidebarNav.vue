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
import type { Component } from "vue";

import type { AppView } from "../../app/routes";

defineProps<{
  view: AppView;
}>();

defineEmits<{
  change: [view: AppView];
}>();

const primaryItems: Array<{ view: AppView; label: string; icon: Component }> = [
  { view: "inbox", label: "收集箱", icon: Inbox },
  { view: "knowledge", label: "知识库", icon: BookOpen },
  { view: "search", label: "搜索", icon: Search },
  { view: "tags", label: "标签", icon: Tags },
  { view: "trash", label: "回收站", icon: Trash2 },
];

const settingsItem = {
  view: "settings" as const,
  label: "设置",
  icon: Settings,
};
</script>

<template>
  <nav
    aria-label="主导航"
    class="z-10 flex flex-col gap-1 bg-bg-base p-2 md:p-3"
  >
    <header
      class="mb-2 flex h-[42px] items-center justify-center gap-2 px-2 text-title text-text-primary md:justify-start"
    >
      <span
        class="flex size-7 items-center justify-center rounded-md bg-brand text-white shadow-sm"
      >
        <Feather :size="16" aria-hidden="true" />
      </span>
      <span class="hidden md:inline">2notes</span>
    </header>
    <button
      v-for="item in primaryItems"
      :key="item.view"
      type="button"
      class="group relative flex h-[38px] items-center justify-center gap-2 rounded-md border-none bg-transparent text-text-secondary transition-[color,background-color] duration-fast ease-token ring-focus hover:bg-selected hover:text-text-primary active:bg-bg-active md:grid md:grid-cols-[20px_1fr] md:justify-items-start md:px-3 md:text-left"
      :class="{
        'bg-selected font-medium text-text-primary hover:bg-selected':
          item.view === view,
      }"
      :aria-label="item.label"
      :aria-current="item.view === view ? 'page' : undefined"
      :title="item.label"
      @click="$emit('change', item.view)"
    >
      <span
        class="absolute bottom-2 left-0 top-2 hidden w-0.5 rounded-r-full bg-brand opacity-0 transition-opacity duration-fast ease-token md:block"
        :class="{ 'opacity-100': item.view === view }"
        aria-hidden="true"
      ></span>
      <component :is="item.icon" :size="17" aria-hidden="true" />
      <span class="hidden md:inline">{{ item.label }}</span>
    </button>

    <button
      type="button"
      class="group relative mt-auto flex h-[38px] items-center justify-center gap-2 rounded-md border-none bg-transparent text-text-secondary transition-[color,background-color] duration-fast ease-token ring-focus hover:bg-selected hover:text-text-primary active:bg-bg-active md:grid md:grid-cols-[20px_1fr] md:justify-items-start md:px-3 md:text-left"
      :class="{
        'bg-selected font-medium text-text-primary hover:bg-selected':
          settingsItem.view === view,
      }"
      :aria-label="settingsItem.label"
      :aria-current="settingsItem.view === view ? 'page' : undefined"
      :title="settingsItem.label"
      @click="$emit('change', settingsItem.view)"
    >
      <span
        class="absolute bottom-2 left-0 top-2 hidden w-0.5 rounded-r-full bg-brand opacity-0 transition-opacity duration-fast ease-token md:block"
        :class="{ 'opacity-100': settingsItem.view === view }"
        aria-hidden="true"
      ></span>
      <Settings :size="17" aria-hidden="true" />
      <span class="hidden md:inline">{{ settingsItem.label }}</span>
    </button>
  </nav>
</template>
