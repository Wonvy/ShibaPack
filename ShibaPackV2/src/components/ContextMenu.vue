<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import type { Component } from "vue";

export interface MenuItem {
  key: string;
  label: string;
  icon?: Component;
  danger?: boolean;
  disabled?: boolean;
  separator?: boolean;
}

const props = defineProps<{ items: MenuItem[]; x: number; y: number }>();
const emit = defineEmits<{ select: [key: string]; close: [] }>();

const menu = ref<HTMLElement | null>(null);
const position = ref({ left: props.x, top: props.y });

function onDocumentClick(event: MouseEvent) {
  if (menu.value && !menu.value.contains(event.target as Node)) emit("close");
}
function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") emit("close");
}

onMounted(() => {
  document.addEventListener("click", onDocumentClick);
  document.addEventListener("keydown", onKeydown);
  requestAnimationFrame(() => {
    const el = menu.value;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    position.value = {
      left: Math.min(props.x, window.innerWidth - rect.width - 8),
      top: Math.min(props.y, window.innerHeight - rect.height - 8),
    };
  });
});
onUnmounted(() => {
  document.removeEventListener("click", onDocumentClick);
  document.removeEventListener("keydown", onKeydown);
});
</script>

<template>
  <Teleport to="body">
    <div
      ref="menu"
      class="fixed z-50 min-w-[170px] overflow-hidden rounded-lg border border-line-strong bg-surface py-1 shadow-[var(--shadow-pop)]"
      :style="{ left: `${position.left}px`, top: `${position.top}px` }"
    >
      <template v-for="item in items" :key="item.key">
        <div v-if="item.separator" class="my-1 border-t border-line" />
        <button
          v-else
          type="button"
          class="flex h-8 w-full items-center gap-2 px-2.5 text-left text-[12px] transition-colors disabled:cursor-not-allowed disabled:opacity-40"
          :class="item.danger ? 'text-danger hover:bg-danger/10' : 'text-fg-2 hover:bg-hover hover:text-fg'"
          :disabled="item.disabled"
          @click="emit('select', item.key); emit('close')"
        >
          <component :is="item.icon" v-if="item.icon" :size="13" :stroke-width="1.75" class="shrink-0" />
          <span>{{ item.label }}</span>
        </button>
      </template>
    </div>
  </Teleport>
</template>
