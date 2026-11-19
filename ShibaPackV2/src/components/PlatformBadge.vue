<script setup lang="ts">
import { computed } from "vue";
import type { Platform } from "../types";

const props = defineProps<{ platform: Platform }>();

const label = computed(() => {
  switch (props.platform) {
    case "windows":
      return "Windows";
    case "macos":
      return "macOS";
    case "linux":
      return "Linux";
    case "crossplatform":
      return "通用";
    default:
      return "未知";
  }
});
</script>

<template>
  <span class="inline-flex items-center rounded-[4px] bg-hover px-1.5 py-0.5 text-[10px] text-fg-3">
    {{ label }}
  </span>
</template>
