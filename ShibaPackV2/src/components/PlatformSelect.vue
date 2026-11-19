<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { Apple, Check, ChevronDown, Globe, List, Monitor, Terminal } from "lucide-vue-next";
import { t } from "../i18n";
import type { Platform } from "../types";

const props = defineProps<{ modelValue: Platform | "all" }>();
const emit = defineEmits<{ "update:modelValue": [value: Platform | "all"] }>();

const open = ref(false);
const root = ref<HTMLElement | null>(null);

const options: { value: Platform | "all"; label: string; icon: typeof Monitor }[] = [
  { value: "all", label: "app.platform.all", icon: List },
  { value: "windows", label: "app.platform.windows", icon: Monitor },
  { value: "macos", label: "app.platform.macos", icon: Apple },
  { value: "linux", label: "app.platform.linux", icon: Terminal },
  { value: "crossplatform", label: "app.platform.crossplatform", icon: Globe },
];

function choose(value: Platform | "all") {
  emit("update:modelValue", value);
  open.value = false;
}

function onDocumentClick(event: MouseEvent) {
  if (root.value && !root.value.contains(event.target as Node)) open.value = false;
}

onMounted(() => document.addEventListener("click", onDocumentClick));
onUnmounted(() => document.removeEventListener("click", onDocumentClick));
</script>

<template>
  <div ref="root" class="relative shrink-0">
    <button
      type="button"
      class="flex h-8 w-[122px] items-center gap-1.5 rounded-md border border-line-strong bg-bg px-2 text-[12px] text-fg-2 transition-colors hover:border-accent hover:text-fg"
      :aria-label="t('app.platform.all')"
      @click="open = !open"
    >
      <component :is="options.find((item) => item.value === props.modelValue)?.icon ?? List" :size="13" :stroke-width="1.75" class="shrink-0 text-fg-3" />
      <span class="min-w-0 flex-1 truncate text-left">
        {{ t(options.find((item) => item.value === props.modelValue)?.label ?? "app.platform.all") }}
      </span>
      <ChevronDown :size="12" :stroke-width="1.75" class="shrink-0 text-fg-3" />
    </button>

    <div
      v-if="open"
      class="absolute right-0 top-[calc(100%+4px)] z-30 w-[150px] overflow-hidden rounded-lg border border-line-strong bg-surface py-1 shadow-[var(--shadow-pop)]"
    >
      <button
        v-for="option in options"
        :key="option.value"
        type="button"
        class="flex h-8 w-full items-center gap-2 px-2.5 text-left text-[12px] transition-colors"
        :class="props.modelValue === option.value ? 'bg-accent-soft text-accent' : 'text-fg-2 hover:bg-hover hover:text-fg'"
        @click="choose(option.value)"
      >
        <component :is="option.icon" :size="13" :stroke-width="1.75" class="shrink-0" />
        <span class="flex-1">{{ t(option.label) }}</span>
        <Check v-if="props.modelValue === option.value" :size="12" :stroke-width="2" />
      </button>
    </div>
  </div>
</template>
