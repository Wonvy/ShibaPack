<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { Trash2 } from "lucide-vue-next";
import { t } from "../i18n";
import { useAppStore } from "../stores/app";
import type { Category } from "../types";
import CategoryIcon from "./CategoryIcon.vue";
import Modal from "./Modal.vue";
import UiButton from "./UiButton.vue";

const props = withDefaults(
  defineProps<{
    open: boolean;
    category: Category | null;
    parentId?: string | null;
  }>(),
  { parentId: null },
);
const emit = defineEmits<{ close: [] }>();

const app = useAppStore();
const isNew = computed(() => !props.category);
const title = computed(() => (isNew.value ? t("category.new") : t("category.edit")));

const iconNames = [
  "wrench", "code", "file-code", "terminal", "palette", "image", "music",
  "game", "book", "work", "globe", "shield", "zap", "settings", "tag",
  "folder", "cpu", "star", "heart", "layers", "box",
];
const colorOptions = ["#a25bff", "#5b8cff", "#2fb37b", "#e0a133", "#e56a8f", "#e56a5b"];

const form = reactive({ name: "", color: "#a25bff", icon: "tag", parent_id: "" });
const saving = ref(false);

watch(
  () => [props.open, props.category?.id, props.parentId] as const,
  () => {
    if (!props.open) return;
    const c = props.category;
    form.name = c?.name ?? "";
    form.color = c?.color ?? "#a25bff";
    form.icon = c?.icon ?? "tag";
    form.parent_id = c?.parent_id ?? props.parentId ?? "";
  },
  { immediate: true },
);

const parentOptions = computed(() =>
  app.library.categories.filter(
    (item) => !item.parent_id && item.id !== props.category?.id,
  ),
);

async function save() {
  const name = form.name.trim();
  if (!name) return;
  saving.value = true;
  if (isNew.value) {
    await app.createCategory(name, form.color, form.icon, form.parent_id || null);
  } else if (props.category) {
    await app.updateCategory({
      ...props.category,
      name,
      color: form.color,
      icon: form.icon,
      parent_id: form.parent_id || null,
    });
  }
  saving.value = false;
  emit("close");
}

async function remove() {
  if (!props.category) return;
  if (!window.confirm(t("sidebar.confirmDelete", { name: props.category.name }))) return;
  await app.deleteCategory(props.category.id);
  emit("close");
}
</script>

<template>
  <Modal :open="open" :title="title" :width="440" @close="emit('close')">
    <div class="flex flex-col gap-4 px-4 py-4">
      <div class="flex items-center gap-3">
        <span
          class="flex size-11 shrink-0 items-center justify-center rounded-lg border border-line"
          :style="{ color: form.color, background: `${form.color}18` }"
        >
          <CategoryIcon :name="form.icon" :size="22" />
        </span>
        <label class="block min-w-0 flex-1">
          <span class="mb-1 block text-[12px] text-fg-3">{{ t("category.name") }}</span>
          <input
            v-model="form.name"
            autofocus
            :placeholder="t('category.name')"
            class="h-8 w-full rounded-md border border-line-strong bg-bg px-2.5 text-[13px] outline-none focus:border-accent"
            @keydown.enter="save"
          />
        </label>
      </div>

      <div>
        <div class="mb-1.5 text-[12px] text-fg-3">{{ t("category.color") }}</div>
        <div class="flex flex-wrap items-center gap-2">
          <button
            v-for="color in colorOptions"
            :key="color"
            type="button"
            class="size-6 rounded-full border-2 transition-transform hover:scale-110"
            :class="form.color === color ? 'border-fg' : 'border-transparent'"
            :style="{ background: color }"
            :aria-label="color"
            @click="form.color = color"
          />
          <input
            v-model="form.color"
            type="color"
            class="size-6 cursor-pointer rounded border border-line-strong bg-transparent p-0"
          />
        </div>
      </div>

      <div>
        <div class="mb-1.5 text-[12px] text-fg-3">{{ t("category.icon") }}</div>
        <div class="grid grid-cols-7 gap-1.5 rounded-lg border border-line bg-bg p-2">
          <button
            v-for="name in iconNames"
            :key="name"
            type="button"
            class="flex h-8 items-center justify-center rounded-md border transition-colors"
            :class="form.icon === name ? 'border-accent bg-active text-accent' : 'border-transparent text-fg-2 hover:bg-hover'"
            :title="t(`settings.icons.${name}`)"
            @click="form.icon = name"
          >
            <CategoryIcon :name="name" :size="16" />
          </button>
        </div>
      </div>

      <label class="block">
        <span class="mb-1 block text-[12px] text-fg-3">{{ t("category.parent") }}</span>
        <select
          v-model="form.parent_id"
          class="h-8 w-full rounded-md border border-line-strong bg-bg px-2 text-[13px] outline-none focus:border-accent"
        >
          <option value="">{{ t("settings.rootLevel") }}</option>
          <option v-for="parent in parentOptions" :key="parent.id" :value="parent.id">
            {{ parent.name }}
          </option>
        </select>
      </label>
    </div>

    <div class="flex items-center gap-2 border-t border-line px-4 py-3">
      <button
        v-if="!isNew"
        type="button"
        class="inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-[13px] text-fg-3 hover:bg-hover hover:text-danger"
        @click="remove"
      >
        <Trash2 :size="13" :stroke-width="1.75" />
        {{ t("common.delete") }}
      </button>
      <div class="ml-auto flex items-center gap-2">
        <UiButton @click="emit('close')">{{ t("common.cancel") }}</UiButton>
        <UiButton variant="primary" :disabled="!form.name.trim()" :busy="saving" @click="save">
          {{ isNew ? t("category.create") : t("common.save") }}
        </UiButton>
      </div>
    </div>
  </Modal>
</template>
