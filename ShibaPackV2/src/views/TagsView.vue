<script setup lang="ts">
import { computed, reactive, ref } from "vue";
import { Plus, Tag as TagIcon, Trash2 } from "lucide-vue-next";
import { t } from "../i18n";
import { useAppStore } from "../stores/app";
import type { Tag } from "../types";
import UiButton from "../components/UiButton.vue";
import Modal from "../components/Modal.vue";

const app = useAppStore();
const editorOpen = ref(false);
const form = reactive({ name: "", color: "#a25bff" });

const colors = ["#a25bff", "#5b8cff", "#2fb37b", "#e0a133", "#e56a8f", "#38bdf8"];

const usageCount = computed(() => {
  const map = new Map<string, number>();
  for (const software of app.library.software ?? []) {
    for (const id of software.tags ?? []) {
      map.set(id, (map.get(id) ?? 0) + 1);
    }
  }
  return map;
});

function openEditor() {
  form.name = "";
  form.color = colors[0];
  editorOpen.value = true;
}

async function save() {
  if (!form.name.trim()) return;
  await app.createTag(form.name.trim(), form.color);
  editorOpen.value = false;
}

function confirmDelete(tag: Tag) {
  if (window.confirm(t("tags.confirmDelete", { name: tag.name }))) {
    void app.deleteTag(tag.id);
  }
}
</script>

<template>
  <div class="flex h-full min-w-0 flex-col bg-bg">
    <div class="flex h-11 shrink-0 items-center gap-2 border-b border-line px-4">
      <TagIcon :size="15" :stroke-width="1.75" class="text-accent" />
      <h1 class="text-[13px] font-semibold">{{ t("tags.title") }}</h1>
      <UiButton variant="primary" class="ml-auto" @click="openEditor">
        <Plus :size="13" :stroke-width="1.75" />
        {{ t("tags.new") }}
      </UiButton>
    </div>

    <div class="min-h-0 flex-1 overflow-y-auto p-5">
      <p class="mb-4 text-[12px] text-fg-3">{{ t("tags.desc") }}</p>

      <div v-if="app.tags.length === 0" class="flex flex-col items-center justify-center gap-2 py-20 text-center">
        <TagIcon :size="28" :stroke-width="1.25" class="text-fg-4" />
        <p class="text-[12px] text-fg-3">{{ t("tags.empty") }}</p>
      </div>

      <div v-else class="grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-3">
        <div
          v-for="tag in app.tags"
          :key="tag.id"
          class="flex items-center gap-2 rounded-lg border border-line bg-surface px-3 py-2.5"
        >
          <span class="size-3 shrink-0 rounded-full" :style="{ background: tag.color }" />
          <div class="min-w-0 flex-1">
            <div class="truncate text-[13px]">{{ tag.name }}</div>
            <div class="text-[11px] text-fg-4">{{ usageCount.get(tag.id) ?? 0 }} {{ t("tags.count") }}</div>
          </div>
          <button
            type="button"
            class="flex size-7 items-center justify-center rounded text-fg-4 hover:bg-hover hover:text-danger"
            :title="t('common.delete')"
            @click="confirmDelete(tag)"
          >
            <Trash2 :size="13" :stroke-width="1.75" />
          </button>
        </div>
      </div>
    </div>

    <Modal :open="editorOpen" :title="t('tags.new')" :width="380" @close="editorOpen = false">
      <div class="flex flex-col gap-4 p-4">
        <label class="block">
          <span class="mb-1.5 block text-[12px] font-medium text-fg-2">{{ t("tags.name") }}</span>
          <input
            v-model="form.name"
            class="h-9 w-full rounded-md border border-line-strong bg-bg px-2.5 text-[13px] outline-none focus:border-accent"
            @keydown.enter="save"
          />
        </label>
        <div>
          <span class="mb-1.5 block text-[12px] font-medium text-fg-2">{{ t("tags.color") }}</span>
          <div class="flex items-center gap-2">
            <button
              v-for="color in colors"
              :key="color"
              type="button"
              class="size-6 rounded-full border-2 transition-transform hover:scale-110"
              :class="form.color === color ? 'border-fg' : 'border-transparent'"
              :style="{ background: color }"
              @click="form.color = color"
            />
          </div>
        </div>
        <div class="flex justify-end gap-2 pt-2">
          <UiButton @click="editorOpen = false">{{ t("common.cancel") }}</UiButton>
          <UiButton variant="primary" @click="save">{{ t("common.save") }}</UiButton>
        </div>
      </div>
    </Modal>
  </div>
</template>
