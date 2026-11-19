<script setup lang="ts">
withDefaults(defineProps<{ open: boolean; title: string; width?: number }>(), { width: 440 });
defineEmits<{ close: [] }>();
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="fixed inset-0 z-40 flex items-start justify-center">
      <div class="anim-fade absolute inset-0 bg-overlay" @click="$emit('close')" />
      <section
        class="anim-pop relative mt-[14vh] flex max-h-[76vh] flex-col overflow-hidden rounded-xl bg-surface shadow-pop"
        :style="{ width: `${width}px`, maxWidth: 'calc(100vw - 32px)' }"
      >
        <header class="flex h-11 shrink-0 items-center justify-between border-b border-line pl-4 pr-2">
          <h2 class="text-[13px] font-semibold">{{ title }}</h2>
          <button
            type="button"
            aria-label="关闭"
            class="inline-flex size-7 items-center justify-center rounded-md text-fg-3 hover:bg-hover hover:text-fg"
            @click="$emit('close')"
          >
            <span class="text-[16px] leading-none">×</span>
          </button>
        </header>
        <div class="min-h-0 flex-1 overflow-y-auto">
          <slot />
        </div>
      </section>
    </div>
  </Teleport>
</template>
