<script setup lang="ts">
import { ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Square, X } from "lucide-vue-next";
import { isDesktop } from "../lib/api";

const maximized = ref(false);
const appWindow = isDesktop ? getCurrentWindow() : null;

if (appWindow) {
  void appWindow.isMaximized().then((value) => (maximized.value = value));
}

async function minimize() {
  await appWindow?.minimize();
}

async function toggleMaximize() {
  if (!appWindow) return;
  await appWindow.toggleMaximize();
  maximized.value = await appWindow.isMaximized();
}

async function close() {
  await appWindow?.close();
}
</script>

<template>
  <div v-if="isDesktop" class="no-drag flex h-full shrink-0 items-center">
    <button
      type="button"
      class="flex h-full w-10 items-center justify-center text-fg-3 transition-colors hover:bg-hover hover:text-fg"
      aria-label="最小化"
      @click="minimize"
    >
      <Minus :size="14" :stroke-width="1.75" />
    </button>
    <button
      type="button"
      class="flex h-full w-10 items-center justify-center text-fg-3 transition-colors hover:bg-hover hover:text-fg"
      :aria-label="maximized ? '还原' : '最大化'"
      @click="toggleMaximize"
    >
      <Square :size="11" :stroke-width="1.75" />
    </button>
    <button
      type="button"
      class="flex h-full w-10 items-center justify-center text-fg-3 transition-colors hover:bg-danger hover:text-white"
      aria-label="关闭"
      @click="close"
    >
      <X :size="14" :stroke-width="1.75" />
    </button>
  </div>
</template>
