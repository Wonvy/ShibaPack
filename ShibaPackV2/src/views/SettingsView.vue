<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { Download, FolderOpen, Info, ShieldCheck, Upload } from "lucide-vue-next";
import UiButton from "../components/UiButton.vue";
import { isDesktop, api } from "../lib/api";
import {
  exportLanguage,
  importLanguage,
  languages,
  setLocale,
  t,
} from "../i18n";
import { useAppStore } from "../stores/app";
import { useUiStore } from "../stores/ui";
import type { Settings, ThemeMode } from "../types";

const app = useAppStore();
const ui = useUiStore();
const form = reactive<Settings>({ ...app.settings });
const saving = ref(false);
const fileInput = ref<HTMLInputElement | null>(null);
const dataFile = computed(() => {
  const root = app.status?.repo_root?.replace(/[\\/]+$/, "");
  return root ? `${root}/software.json` : "软件仓库目录 / software.json";
});

watch(
  () => app.settings,
  (value) => Object.assign(form, value),
);

const accents = ["#a25bff", "#5b8cff", "#2fb37b", "#e0a133", "#e56a8f"];
const themes: { value: ThemeMode; label: string }[] = [
  { value: "system", label: "settings.theme.system" },
  { value: "dark", label: "settings.theme.dark" },
  { value: "light", label: "settings.theme.light" },
];

async function save() {
  saving.value = true;
  await app.saveSettings({ ...form });
  saving.value = false;
}

async function chooseRoot() {
  const path = await api.chooseDirectory();
  if (path) form.package_root = path;
}

async function exportData() {
  const path = await api.chooseSaveFile("shibapack-backup.json");
  if (path) await app.exportBackup(path);
}

async function importData() {
  const path = await open({
    multiple: false,
    filters: [{ name: "JSON", extensions: ["json"] }],
  });
  if (typeof path === "string") await app.importBackup(path);
}

function applyLanguage(content: string) {
  const result = importLanguage(content);
  if (result.ok && result.code) {
    form.language = result.code;
    ui.toast(t("settings.languageImported"), "success");
  } else {
    ui.toast(t("settings.languageInvalid"), "error");
  }
}

async function exportLangFile() {
  const content = exportLanguage(form.language);
  if (isDesktop) {
    const path = await api.chooseSaveFile(`shibapack-${form.language}.json`);
    if (path) await api.writeTextFile(path, content);
    return;
  }
  const url = URL.createObjectURL(new Blob([content], { type: "application/json" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = `shibapack-${form.language}.json`;
  link.click();
  URL.revokeObjectURL(url);
}

async function importLangFile() {
  if (isDesktop) {
    const path = await open({
      multiple: false,
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (typeof path !== "string") return;
    applyLanguage(await api.readTextFile(path));
    return;
  }
  fileInput.value?.click();
}

async function onFileChange(event: Event) {
  const target = event.target as HTMLInputElement;
  const file = target.files?.[0];
  if (file) applyLanguage(await file.text());
  target.value = "";
}
</script>

<template>
  <section class="h-full overflow-y-auto bg-bg">
    <div class="px-5 py-5">
      <input ref="fileInput" type="file" accept=".json,application/json" class="hidden" @change="onFileChange" />

      <!-- 存储 -->
      <div class="mb-6">
        <h2 class="mb-3 border-b border-line pb-2 text-[12px] font-semibold uppercase tracking-wide text-fg-3">
          {{ t("settings.storage") }}
        </h2>
        <div class="flex flex-col gap-3">
          <label class="block">
            <span class="mb-1.5 block text-[12px] font-medium text-fg-2">{{ t("settings.packageRoot") }}</span>
            <span class="flex items-center gap-2">
              <input
                :value="form.package_root"
                readonly
                class="h-8 min-w-0 flex-1 rounded-md border border-line-strong bg-bg px-2.5 text-[13px] text-fg-2 outline-none"
              />
              <UiButton @click="chooseRoot">
                <FolderOpen :size="13" :stroke-width="1.75" />
                {{ t("settings.choose") }}
              </UiButton>
            </span>
            <span class="mt-1.5 block text-[12px] text-fg-3">{{ t("settings.packageRootHint") }}</span>
          </label>

          <div>
            <span class="mb-1.5 block text-[12px] font-medium text-fg-2">{{ t("settings.storageMode") }}</span>
            <div class="inline-flex rounded-md border border-line-strong bg-bg p-0.5">
              <button
                v-for="mode in [{ v: 'managed', l: 'storage.mode.managed' }, { v: 'indexed', l: 'storage.mode.indexed' }]"
                :key="mode.v"
                type="button"
                class="h-6 rounded-[5px] px-2.5 text-[12px] font-medium transition-colors"
                :class="form.storage_mode === mode.v ? 'bg-active text-fg' : 'text-fg-3 hover:text-fg-2'"
                @click="form.storage_mode = mode.v as 'managed' | 'indexed'"
              >
                {{ t(mode.l) }}
              </button>
            </div>
          </div>

          <label class="block">
            <span class="mb-1.5 block text-[12px] font-medium text-fg-2">{{ t("settings.installerDir") }}</span>
            <input
              v-model="form.installer_directory"
              class="h-8 w-full rounded-md border border-line-strong bg-bg px-2.5 text-[13px] text-fg-2 outline-none focus:border-accent"
            />
          </label>

          <div class="flex items-center justify-between gap-3">
            <div class="min-w-0">
              <button type="button" class="flex items-center gap-1 text-[12px] font-medium text-fg-2 hover:text-accent" @click="api.revealDataDir()">
                {{ t("settings.dataFile") }}
                <FolderOpen :size="12" :stroke-width="1.75" />
              </button>
              <div class="mt-0.5 truncate text-[12px] text-fg-3" :title="dataFile">{{ dataFile }}</div>
            </div>
          </div>

          <label class="flex items-center justify-between">
            <span>
              <span class="block text-[13px]">{{ t("settings.autoScan") }}</span>
              <span class="block text-[12px] text-fg-3">{{ t("settings.autoScanHint") }}</span>
            </span>
            <input v-model="form.auto_scan" type="checkbox" class="size-4 accent-[var(--accent)]" />
          </label>

          <label class="flex items-center justify-between">
            <span>
              <span class="block text-[13px]">{{ t("settings.copyOnImport") }}</span>
              <span class="block text-[12px] text-fg-3">{{ t("settings.copyOnImportHint") }}</span>
            </span>
            <input v-model="form.copy_on_import" type="checkbox" class="size-4 accent-[var(--accent)]" />
          </label>
        </div>
      </div>

      <!-- 外观与语言 -->
      <div class="mb-6">
        <h2 class="mb-3 border-b border-line pb-2 text-[12px] font-semibold uppercase tracking-wide text-fg-3">
          {{ t("settings.appearance") }}
        </h2>
        <div class="flex flex-col gap-4">
          <div>
            <div class="mb-1.5 text-[12px] font-medium text-fg-2">{{ t("settings.theme") }}</div>
            <div class="inline-flex rounded-md border border-line-strong bg-bg p-0.5">
              <button
                v-for="item in themes"
                :key="item.value"
                type="button"
                class="h-6 rounded-[5px] px-2.5 text-[12px] font-medium transition-colors"
                :class="form.theme === item.value ? 'bg-active text-fg' : 'text-fg-3 hover:text-fg-2'"
                @click="form.theme = item.value"
              >
                {{ t(item.label) }}
              </button>
            </div>
          </div>

          <div>
            <div class="mb-1.5 text-[12px] font-medium text-fg-2">{{ t("settings.accent") }}</div>
            <div class="flex items-center gap-2">
              <button
                v-for="color in accents"
                :key="color"
                type="button"
                class="size-6 rounded-full border-2 transition-transform hover:scale-110"
                :class="form.accent === color ? 'border-fg' : 'border-transparent'"
                :style="{ background: color }"
                :aria-label="color"
                @click="form.accent = color"
              />
            </div>
          </div>

          <div>
            <div class="mb-1.5 text-[12px] font-medium text-fg-2">{{ t("settings.language") }}</div>
            <div class="flex flex-wrap items-center gap-2">
              <select
                v-model="form.language"
                class="h-8 w-[130px] rounded-md border border-line-strong bg-bg px-2 text-[12px] text-fg-2 outline-none focus:border-accent"
                @change="setLocale(form.language)"
              >
                <option v-for="lang in languages" :key="lang.code" :value="lang.code">{{ lang.name }}</option>
              </select>
              <UiButton @click="exportLangFile">
                <Download :size="13" :stroke-width="1.75" />
                {{ t("settings.exportLanguage") }}
              </UiButton>
              <UiButton @click="importLangFile">
                <Upload :size="13" :stroke-width="1.75" />
                {{ t("settings.importLanguage") }}
              </UiButton>
            </div>
            <p class="mt-1.5 text-[12px] text-fg-3">{{ t("settings.languageHint") }}</p>
          </div>
        </div>
      </div>

      <!-- 数据 -->
      <div class="mb-6">
        <h2 class="mb-3 border-b border-line pb-2 text-[12px] font-semibold uppercase tracking-wide text-fg-3">
          {{ t("settings.data") }}
        </h2>
        <div class="flex flex-wrap items-center gap-2">
          <UiButton @click="exportData">
            <Download :size="13" :stroke-width="1.75" />
            {{ t("settings.exportBackup") }}
          </UiButton>
          <UiButton @click="importData">
            <Upload :size="13" :stroke-width="1.75" />
            {{ t("settings.importBackup") }}
          </UiButton>
          <UiButton @click="app.verifyData">
            <ShieldCheck :size="13" :stroke-width="1.75" />
            {{ t("settings.verifyData") }}
          </UiButton>
          <span class="ml-auto text-[12px] text-fg-3">{{ t("settings.backupHint") }}</span>
        </div>
      </div>

      <div class="flex items-center justify-between border-t border-line pt-4">
        <div class="flex items-center gap-2 text-[12px] text-fg-4">
          <Info :size="14" :stroke-width="1.75" />
          ShibaPack 0.1.0 · {{ app.status?.platform ?? t("app.status.unknownPlatform") }}
        </div>
        <UiButton variant="primary" :busy="saving" @click="save">{{ t("settings.save") }}</UiButton>
      </div>
    </div>
  </section>
</template>
