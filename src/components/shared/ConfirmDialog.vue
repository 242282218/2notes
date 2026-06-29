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
    >
      <h2>{{ title }}</h2>
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
