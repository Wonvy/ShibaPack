<script setup lang="ts">
import { computed } from "vue";
import { LoaderCircle } from "lucide-vue-next";

const props = withDefaults(
  defineProps<{
    variant?: "primary" | "secondary" | "ghost" | "danger";
    busy?: boolean;
    disabled?: boolean;
  }>(),
  { variant: "secondary", busy: false, disabled: false },
);

const classes = computed(() => [
  "inline-flex h-7 shrink-0 items-center justify-center gap-1.5 rounded-md px-2.5 text-[13px] font-medium transition-colors",
  "disabled:pointer-events-none disabled:opacity-50",
  props.variant === "primary" && "bg-accent-strong text-white hover:bg-accent",
  props.variant === "secondary" && "border border-line-strong bg-hover text-fg hover:bg-active",
  props.variant === "ghost" && "text-fg-2 hover:bg-hover hover:text-fg",
  props.variant === "danger" && "bg-danger text-white hover:opacity-90",
]);
</script>

<template>
  <button type="button" :class="classes" :disabled="disabled || busy">
    <LoaderCircle v-if="busy" :size="13" :stroke-width="2" class="animate-spin" />
    <slot />
  </button>
</template>
