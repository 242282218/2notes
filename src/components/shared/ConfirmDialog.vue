<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, watch } from "vue";

const props = defineProps<{
  open: boolean;
  title: string;
  message: string;
  confirmLabel: string;
  danger?: boolean;
}>();

const emit = defineEmits<{
  confirm: [];
  cancel: [];
}>();

const titleId = `modal-title-${crypto.randomUUID()}`;
const dialogRef = ref<HTMLElement | null>(null);
const cancelButtonRef = ref<HTMLButtonElement | null>(null);
let previouslyFocused: HTMLElement | null = null;

watch(
  () => props.open,
  async (open) => {
    if (!open) {
      restoreFocus();
      return;
    }
    previouslyFocused = document.activeElement as HTMLElement | null;
    await nextTick();
    cancelButtonRef.value?.focus();
  },
);

function cancel() {
  emit("cancel");
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    cancel();
    return;
  }
  if (event.key !== "Tab") {
    return;
  }
  const focusable = dialogRef.value?.querySelectorAll<HTMLElement>(
    'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
  );
  if (!focusable?.length) {
    return;
  }
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}

function restoreFocus() {
  previouslyFocused?.focus?.();
  previouslyFocused = null;
}

onBeforeUnmount(restoreFocus);
</script>

<template>
  <div v-if="open" class="modal-backdrop" @click.self="cancel">
    <section
      ref="dialogRef"
      class="modal"
      role="dialog"
      aria-modal="true"
      :aria-labelledby="titleId"
      @keydown="onKeydown"
    >
      <h2 :id="titleId">
        {{ title }}
      </h2>
      <p>{{ message }}</p>
      <div class="modal-actions">
        <button
          ref="cancelButtonRef"
          type="button"
          class="secondary-button"
          @click="cancel"
        >
          取消
        </button>
        <button
          type="button"
          class="primary-button"
          :class="{ danger }"
          @click="$emit('confirm')"
        >
          {{ confirmLabel }}
        </button>
      </div>
    </section>
  </div>
</template>
