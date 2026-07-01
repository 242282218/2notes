<script setup lang="ts">
defineProps<{
  open: boolean;
  title: string;
  message: string;
  confirmLabel: string;
  danger?: boolean;
}>();

defineEmits<{
  confirm: [];
  cancel: [];
}>();

const titleId = `modal-title-${crypto.randomUUID()}`;
</script>

<template>
  <div
    v-if="open"
    class="modal-backdrop"
    @click.self="$emit('cancel')"
  >
    <section
      class="modal"
      role="dialog"
      aria-modal="true"
      :aria-labelledby="titleId"
    >
      <h2 :id="titleId">
        {{ title }}
      </h2>
      <p>{{ message }}</p>
      <div class="modal-actions">
        <button
          type="button"
          class="secondary-button"
          @click="$emit('cancel')"
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
