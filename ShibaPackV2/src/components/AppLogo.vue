<script setup lang="ts">
withDefaults(defineProps<{ size?: number }>(), { size: 20 });
</script>

<template>
  <span
    class="inline-flex shrink-0 items-center justify-center rounded-[6px] bg-accent-strong text-white"
    :style="{ width: `${size}px`, height: `${size}px` }"
    aria-hidden="true"
  >
    <svg :width="size * 0.66" :height="size * 0.66" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      <path d="M10 5.5 8 2l3 2h2l3-2-2 3.5" />
      <path d="M12 8c-3.3 0-6 2.5-6 5.8V19a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2v-5.2C18 10.5 15.3 8 12 8Z" />
      <path d="M9.5 13h.01M14.5 13h.01M10 16.5c.6.6 1.3.9 2 .9s1.4-.3 2-.9" />
    </svg>
  </span>
</template>
