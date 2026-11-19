<script setup lang="ts">
import { ref } from "vue";
import { FolderOpen, PackageOpen } from "lucide-vue-next";
import UiButton from "./UiButton.vue";
import { t } from "../i18n";
import { api } from "../lib/api";
import { useAppStore } from "../stores/app";

const app = useAppStore();
const path = ref("");
const busy = ref(false);
const error = ref("");

async function choose() {
  const selected = await api.chooseDirectory();
  if (selected) {
    path.value = selected;
    error.value = "";
  }
}

async function start() {
  if (!path.value.trim()) {
    error.value = t("setup.required");
    return;
  }
  busy.value = true;
  await app.saveSettings({ ...app.settings, package_root: path.value.trim() });
  busy.value = false;
  await app.scan(path.value.trim());
}
</script>

<template>
  <Teleport to="body">
    <div class="fixed inset-0 z-50 flex items-center justify-center bg-bg px-4">
      <div class="anim-pop w-full max-w-[460px] rounded-xl border border-line bg-surface p-6 shadow-pop">
        <div class="mb-4 flex size-11 items-center justify-center rounded-lg border border-line bg-active text-accent">
          <PackageOpen :size="22" :stroke-width="1.6" />
        </div>
        <h1 class="mb-2 text-[17px] font-semibold">{{ t("setup.title") }}</h1>
        <p class="mb-6 text-[13px] leading-relaxed text-fg-3">{{ t("setup.desc") }}</p>

        <label class="mb-2 block text-[12px] font-medium text-fg-2">{{ t("setup.path") }}</label>
        <div class="flex items-center gap-2">
          <input
            :value="path"
            readonly
            :placeholder="t('setup.path')"
            class="h-9 min-w-0 flex-1 rounded-md border bg-bg px-2.5 text-[13px] outline-none"
            :class="error ? 'border-danger' : 'border-line-strong'"
          />
          <UiButton class="h-9" @click="choose">
            <FolderOpen :size="14" :stroke-width="1.75" />
            {{ t("setup.choose") }}
          </UiButton>
        </div>
        <p v-if="error" class="mt-1.5 text-[12px] text-danger">{{ error }}</p>
        <p class="mt-2 text-[12px] leading-relaxed text-fg-4">{{ t("setup.hint") }}</p>

        <div class="mt-6 flex justify-end">
          <UiButton variant="primary" class="h-9" :busy="busy" @click="start">
            {{ t("setup.start") }}
          </UiButton>
        </div>
      </div>
    </div>
  </Teleport>
</template>
