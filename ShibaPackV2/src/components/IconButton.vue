<script setup lang="ts">
import { computed } from "vue";

const props = withDefaults(
  defineProps<{
    label: string;
    active?: boolean;
    muted?: boolean;
    danger?: boolean;
    disabled?: boolean;
  }>(),
  { active: false, muted: false, danger: false, disabled: false },
);

const classes = computed(() => [
  "inline-flex size-7 shrink-0 items-center justify-center rounded-md transition-colors",
  "disabled:pointer-events-none disabled:opacity-40",
  props.active
    ? "bg-accent-soft text-accent"
    : props.danger
      ? "text-danger hover:bg-hover"
      : props.muted
        ? "text-fg-3 hover:bg-hover hover:text-fg"
        : "text-fg-2 hover:bg-hover hover:text-fg",
]);
</script>

<template>
  <button type="button" :title="label" :aria-label="label" :aria-pressed="active" :disabled="disabled" :class="classes">
    <slot />
  </button>
</template>
