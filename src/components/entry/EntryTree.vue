<script setup lang="ts">
import {
  ChevronDown,
  ChevronRight,
  FolderTree,
  Loader2,
  MoreHorizontal,
} from "lucide-vue-next";
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from "vue";

import { knowledgeTreeGet } from "../../services/knowledgeApi";
import type { EntryTreeNode } from "../../types/generated";
import EmptyState from "../shared/EmptyState.vue";

type VisibleNode = {
  id: string;
  title: string;
  depth: number;
  parentId: string | null;
  hasChildren: boolean;
};

const props = defineProps<{
  selectedId: string | null;
  refreshToken: number;
}>();

const emit = defineEmits<{
  select: [id: string];
  move: [id: string, parentId: string | null];
}>();

const tree = ref<EntryTreeNode[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);
const expandedIds = ref(new Set<string>());
const treeRef = ref<HTMLElement | null>(null);
const openMoveMenuId = ref<string | null>(null);
let requestId = 0;

function flatten(
  nodes: EntryTreeNode[],
  depth = 1,
  parentId: string | null = null,
  result: VisibleNode[] = [],
) {
  for (const node of nodes) {
    const hasChildren = node.children.length > 0;
    result.push({
      id: node.id,
      title: node.title,
      depth,
      parentId,
      hasChildren,
    });
    if (hasChildren && expandedIds.value.has(node.id)) {
      flatten(node.children, depth + 1, node.id, result);
    }
  }
  return result;
}

const visibleNodes = computed(() => flatten(tree.value));
const allNodes = computed(() => {
  const result: VisibleNode[] = [];
  const walk = (
    nodes: EntryTreeNode[],
    depth = 1,
    parentId: string | null = null,
  ) => {
    for (const node of nodes) {
      result.push({
        id: node.id,
        title: node.title,
        depth,
        parentId,
        hasChildren: node.children.length > 0,
      });
      walk(node.children, depth + 1, node.id);
    }
  };
  walk(tree.value);
  return result;
});

function descendantsOf(id: string) {
  const descendants = new Set<string>([id]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const node of allNodes.value) {
      if (
        node.parentId &&
        descendants.has(node.parentId) &&
        !descendants.has(node.id)
      ) {
        descendants.add(node.id);
        changed = true;
      }
    }
  }
  return descendants;
}

function moveTargets(id: string) {
  const excluded = descendantsOf(id);
  return allNodes.value.filter((node) => !excluded.has(node.id));
}

function nodeButtons() {
  return Array.from(
    treeRef.value?.querySelectorAll<HTMLButtonElement>("[data-tree-node]") ??
      [],
  );
}

function focusNode(id: string) {
  void nextTick(() => {
    treeRef.value
      ?.querySelector<HTMLButtonElement>(`[data-tree-node="${id}"]`)
      ?.focus();
  });
}

function toggle(node: VisibleNode) {
  if (!node.hasChildren) return;
  const next = new Set(expandedIds.value);
  if (next.has(node.id)) next.delete(node.id);
  else next.add(node.id);
  expandedIds.value = next;
}

function selectNode(id: string) {
  openMoveMenuId.value = null;
  emit("select", id);
}

function onKeydown(event: KeyboardEvent) {
  const nodes = visibleNodes.value;
  const buttons = nodeButtons();
  const currentIndex = buttons.findIndex(
    (button) => button === document.activeElement,
  );
  if (nodes.length === 0) return;
  const current = nodes[currentIndex] ?? nodes[0];

  if (event.key === "ArrowDown") {
    event.preventDefault();
    focusNode(
      nodes[Math.min(currentIndex + 1, nodes.length - 1)]?.id ?? nodes[0].id,
    );
  } else if (event.key === "ArrowUp") {
    event.preventDefault();
    focusNode(
      nodes[Math.max(currentIndex - 1, 0)]?.id ?? nodes[nodes.length - 1].id,
    );
  } else if (event.key === "ArrowRight" && current) {
    event.preventDefault();
    if (current.hasChildren && !expandedIds.value.has(current.id))
      toggle(current);
    else if (current.hasChildren)
      focusNode(nodes[currentIndex + 1]?.id ?? current.id);
  } else if (event.key === "ArrowLeft" && current) {
    event.preventDefault();
    if (current.hasChildren && expandedIds.value.has(current.id))
      toggle(current);
    else if (current.parentId) focusNode(current.parentId);
  } else if (event.key === "Enter" && current) {
    event.preventDefault();
    selectNode(current.id);
  } else if (event.key === "Home") {
    event.preventDefault();
    focusNode(nodes[0].id);
  } else if (event.key === "End") {
    event.preventDefault();
    focusNode(nodes[nodes.length - 1].id);
  } else if (event.key === "Escape") {
    openMoveMenuId.value = null;
  }
}

async function loadTree() {
  const currentRequest = ++requestId;
  loading.value = true;
  error.value = null;
  try {
    const next = await knowledgeTreeGet();
    if (currentRequest === requestId) tree.value = next;
  } catch (cause) {
    if (currentRequest === requestId) {
      error.value = cause instanceof Error ? cause.message : "知识树加载失败";
    }
  } finally {
    if (currentRequest === requestId) loading.value = false;
  }
}

function onDocumentPointerDown(event: globalThis.PointerEvent) {
  if (!treeRef.value?.contains(event.target as globalThis.Node))
    openMoveMenuId.value = null;
}

watch(
  () => props.refreshToken,
  () => void loadTree(),
  { immediate: true },
);
onMounted(() =>
  document.addEventListener("pointerdown", onDocumentPointerDown),
);
onBeforeUnmount(() =>
  document.removeEventListener("pointerdown", onDocumentPointerDown),
);
</script>

<template>
  <section
    ref="treeRef"
    class="entry-list flex min-h-0 flex-col bg-bg-elevated ring-focus ring-inset"
    aria-label="知识树"
    @keydown="onKeydown"
  >
    <header class="flex h-[40px] shrink-0 items-center justify-between px-4">
      <strong
        class="text-micro font-semibold uppercase tracking-wide text-text-secondary"
        >知识库</strong
      >
      <button
        class="icon-button text-text-secondary"
        type="button"
        aria-label="刷新知识树"
        :disabled="loading"
        @click="loadTree"
      >
        <Loader2
          v-if="loading"
          :size="14"
          class="animate-spin"
          aria-hidden="true"
        />
        <FolderTree v-else :size="14" aria-hidden="true" />
      </button>
    </header>
    <p v-if="error" class="m-0 px-4 py-2 text-caption text-danger" role="alert">
      {{ error }}
    </p>
    <div
      class="min-h-0 flex-1 overflow-auto px-2 pb-2"
      role="tree"
      aria-label="知识条目"
    >
      <div
        v-if="loading && !visibleNodes.length"
        class="px-3 py-4 text-ui text-text-secondary"
        role="status"
      >
        加载中
      </div>
      <EmptyState
        v-else-if="!visibleNodes.length"
        :icon="FolderTree"
        title="暂无知识条目"
        description="将记录沉淀为知识后会显示在这里"
        size="fill"
      />
      <div
        v-for="node in visibleNodes"
        :key="node.id"
        class="relative flex min-w-0 items-center gap-1"
        :style="{ paddingLeft: `${(node.depth - 1) * 16}px` }"
        role="none"
      >
        <button
          v-if="node.hasChildren"
          class="icon-button shrink-0 text-text-secondary"
          type="button"
          :aria-label="expandedIds.has(node.id) ? '折叠' : '展开'"
          :aria-expanded="expandedIds.has(node.id)"
          :data-tree-toggle="node.id"
          @click="toggle(node)"
        >
          <ChevronDown
            v-if="expandedIds.has(node.id)"
            :size="14"
            aria-hidden="true"
          />
          <ChevronRight v-else :size="14" aria-hidden="true" />
        </button>
        <span v-else class="w-[30px] shrink-0" aria-hidden="true" />
        <button
          class="min-w-0 flex-1 truncate rounded-sm px-2 py-2 text-left text-ui outline-none ring-focus hover:bg-bg-secondary"
          :class="
            node.id === selectedId
              ? 'bg-brand/10 text-text-primary'
              : 'text-text-secondary'
          "
          type="button"
          role="treeitem"
          :data-tree-node="node.id"
          :aria-level="node.depth"
          :aria-selected="node.id === selectedId"
          :aria-expanded="
            node.hasChildren ? expandedIds.has(node.id) : undefined
          "
          :tabindex="
            node.id === selectedId || (!selectedId && node === visibleNodes[0])
              ? 0
              : -1
          "
          @click="selectNode(node.id)"
        >
          {{ node.title || "无标题" }}
        </button>
        <button
          v-if="node.id === selectedId"
          class="icon-button shrink-0 text-text-secondary"
          type="button"
          aria-label="移动到"
          :aria-expanded="openMoveMenuId === node.id"
          @click="openMoveMenuId = openMoveMenuId === node.id ? null : node.id"
        >
          <MoreHorizontal :size="16" aria-hidden="true" />
        </button>
        <div
          v-if="openMoveMenuId === node.id"
          class="elevation-2 absolute right-0 top-full z-20 grid min-w-44 rounded-md border border-border bg-bg-elevated p-1"
          role="menu"
        >
          <button
            class="menu-item"
            type="button"
            role="menuitem"
            @click="emit('move', node.id, null)"
          >
            根级
          </button>
          <button
            v-for="target in moveTargets(node.id)"
            :key="target.id"
            class="menu-item max-w-56 truncate text-left"
            type="button"
            role="menuitem"
            :data-tree-move-target="target.id"
            @click="emit('move', node.id, target.id)"
          >
            {{ target.title || "无标题" }}
          </button>
        </div>
      </div>
    </div>
  </section>
</template>
