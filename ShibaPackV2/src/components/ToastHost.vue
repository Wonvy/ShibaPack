<script setup lang="ts">
import { useUiStore } from "../stores/ui";

const ui = useUiStore();
</script>

<template>
  <div class="pointer-events-none fixed bottom-4 right-4 z-50 flex flex-col items-end gap-2">
    <div
      v-for="item in ui.toasts"
      :key="item.id"
      class="anim-pop pointer-events-auto max-w-[360px] rounded-lg border border-line-strong bg-surface px-3 py-2 text-[12px] shadow-pop"
      :class="{
        'text-success': item.tone === 'success',
        'text-danger': item.tone === 'error',
        'text-fg': item.tone === 'default',
      }"
    >
      {{ item.text }}
    </div>
  </div>
</template>
