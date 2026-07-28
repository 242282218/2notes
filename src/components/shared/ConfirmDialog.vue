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
const descriptionId = `modal-description-${crypto.randomUUID()}`;
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
  { immediate: true },
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
  const focusIsOutside = !dialogRef.value?.contains(document.activeElement);
  if (event.shiftKey && (document.activeElement === first || focusIsOutside)) {
    event.preventDefault();
    last.focus();
  } else if (
    !event.shiftKey &&
    (document.activeElement === last || focusIsOutside)
  ) {
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
  <Teleport to="body">
    <div
      v-if="open"
      class="modal-backdrop fixed inset-0 z-[100] flex items-center justify-center bg-backdrop p-4 backdrop-blur-[2px]"
      @click.self="cancel"
    >
      <section
        ref="dialogRef"
        class="modal elevation-3 w-full max-w-[420px] rounded-lg bg-bg-elevated p-5 text-text-primary"
        role="dialog"
        aria-modal="true"
        :aria-labelledby="titleId"
        :aria-describedby="descriptionId"
        @keydown="onKeydown"
      >
        <h2 :id="titleId" class="m-0 text-title">
          {{ title }}
        </h2>
        <p :id="descriptionId" class="mb-0 mt-2 text-body text-text-secondary">
          {{ message }}
        </p>
        <div class="modal-actions mt-5 flex justify-end gap-2">
          <button
            ref="cancelButtonRef"
            type="button"
            class="btn-secondary"
            @click="cancel"
          >
            取消
          </button>
          <button
            type="button"
            class="btn-primary"
            :class="{ 'btn-primary-danger': danger }"
            @click="$emit('confirm')"
          >
            {{ confirmLabel }}
          </button>
        </div>
      </section>
    </div>
  </Teleport>
</template>
