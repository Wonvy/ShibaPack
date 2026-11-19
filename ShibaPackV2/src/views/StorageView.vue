<script setup lang="ts">
import { computed, onMounted } from "vue";
import {
  Database,
  FolderOpen,
  HardDrive,
  Layers,
  Package,
  RefreshCw,
  ShieldCheck,
  TriangleAlert,
} from "lucide-vue-next";
import { formatSize } from "../lib/format";
import { t } from "../i18n";
import { useAppStore } from "../stores/app";
import { useUiStore } from "../stores/ui";
import { api } from "../lib/api";
import UiButton from "../components/UiButton.vue";

const app = useAppStore();
const ui = useUiStore();
const info = computed(() => app.storageInfo);

onMounted(() => {
  void app.loadStorageInfo();
  void app.loadBackups();
});

async function refresh() {
  await app.loadStorageInfo();
  await app.loadBackups();
  ui.toast(t("storage.refresh"), "success");
}

async function chooseRoot() {
  const path = await api.chooseDirectory();
  if (path) {
    const next = { ...app.settings, package_root: path };
    await app.saveSettings(next);
  }
}

function restore(path: string, name: string) {
  if (window.confirm(t("storage.restoreConfirm", { name }))) {
    void app.restoreBackupOp(path);
  }
}
</script>

<template>
  <div class="flex h-full min-w-0 flex-col bg-bg">
    <div class="flex h-11 shrink-0 items-center gap-2 border-b border-line px-4">
      <HardDrive :size="15" :stroke-width="1.75" class="text-accent" />
      <h1 class="text-[13px] font-semibold">{{ t("storage.title") }}</h1>
      <UiButton class="ml-auto" @click="refresh">
        <RefreshCw :size="13" :stroke-width="1.75" />
        {{ t("storage.refresh") }}
      </UiButton>
    </div>

    <div class="min-h-0 flex-1 overflow-y-auto p-5">
      <!-- 存储模式 -->
      <section class="mb-6">
        <h2 class="mb-3 border-b border-line pb-2 text-[12px] font-semibold uppercase tracking-wide text-fg-3">
          {{ t("storage.mode") }}
        </h2>
        <div class="grid grid-cols-2 gap-3">
          <button
            type="button"
            class="rounded-lg border p-3 text-left transition-colors"
            :class="app.settings.storage_mode === 'managed' ? 'border-accent bg-accent-soft/30' : 'border-line bg-surface hover:border-line-strong'"
            @click="app.saveSettings({ ...app.settings, storage_mode: 'managed' })"
          >
            <div class="mb-1 flex items-center gap-1.5 text-[13px] font-medium">
              <Package :size="14" :stroke-width="1.75" class="text-accent" />
              {{ t("storage.mode.managed") }}
            </div>
            <p class="text-[11px] leading-relaxed text-fg-3">{{ t("storage.mode.managedHint") }}</p>
          </button>
          <button
            type="button"
            class="rounded-lg border p-3 text-left transition-colors"
            :class="app.settings.storage_mode === 'indexed' ? 'border-accent bg-accent-soft/30' : 'border-line bg-surface hover:border-line-strong'"
            @click="app.saveSettings({ ...app.settings, storage_mode: 'indexed' })"
          >
            <div class="mb-1 flex items-center gap-1.5 text-[13px] font-medium">
              <FolderOpen :size="14" :stroke-width="1.75" class="text-accent" />
              {{ t("storage.mode.indexed") }}
            </div>
            <p class="text-[11px] leading-relaxed text-fg-3">{{ t("storage.mode.indexedHint") }}</p>
          </button>
        </div>
      </section>

      <!-- 仓库路径 -->
      <section class="mb-6">
        <h2 class="mb-3 border-b border-line pb-2 text-[12px] font-semibold uppercase tracking-wide text-fg-3">
          {{ t("storage.repoRoot") }}
        </h2>
        <div class="flex items-center gap-2">
          <input
            :value="info?.repo_root || app.settings.package_root"
            readonly
            class="h-9 min-w-0 flex-1 rounded-md border border-line-strong bg-surface px-2.5 text-[12px] text-fg-2 outline-none"
          />
          <UiButton @click="chooseRoot">
            <FolderOpen :size="13" :stroke-width="1.75" />
            {{ t("settings.choose") }}
          </UiButton>
        </div>
        <p class="mt-1.5 text-[11px] text-fg-4">{{ t("storage.installerDir") }}: {{ info?.installer_directory || app.settings.installer_directory }}</p>
      </section>

      <!-- 统计 -->
      <section class="mb-6">
        <h2 class="mb-3 border-b border-line pb-2 text-[12px] font-semibold uppercase tracking-wide text-fg-3">
          {{ t("storage.stats") }}
        </h2>
        <div class="grid grid-cols-2 gap-2 md:grid-cols-4">
          <div class="rounded-lg border border-line bg-surface p-3">
            <div class="mb-1 text-[11px] text-fg-4">{{ t("storage.softwareCount") }}</div>
            <div class="tabular text-[18px] font-semibold">{{ info?.software_count ?? 0 }}</div>
          </div>
          <div class="rounded-lg border border-line bg-surface p-3">
            <div class="mb-1 text-[11px] text-fg-4">{{ t("storage.versionCount") }}</div>
            <div class="tabular text-[18px] font-semibold">{{ info?.version_count ?? 0 }}</div>
          </div>
          <div class="rounded-lg border border-line bg-surface p-3">
            <div class="mb-1 text-[11px] text-fg-4">{{ t("storage.fileCount") }}</div>
            <div class="tabular text-[18px] font-semibold">{{ info?.file_count ?? 0 }}</div>
          </div>
          <div class="rounded-lg border border-line bg-surface p-3">
            <div class="mb-1 text-[11px] text-fg-4">{{ t("storage.totalSize") }}</div>
            <div class="tabular text-[18px] font-semibold">{{ formatSize(info?.total_size ?? 0) }}</div>
          </div>
        </div>
        <div class="mt-2 grid grid-cols-3 gap-2">
          <div class="rounded-lg border border-line bg-surface px-3 py-2">
            <div class="text-[11px] text-fg-4">{{ t("storage.managedSize") }}</div>
            <div class="tabular text-[13px]">{{ formatSize(info?.managed_size ?? 0) }}</div>
          </div>
          <div class="rounded-lg border border-line bg-surface px-3 py-2">
            <div class="text-[11px] text-fg-4">{{ t("storage.indexedSize") }}</div>
            <div class="tabular text-[13px]">{{ formatSize(info?.indexed_size ?? 0) }}</div>
          </div>
          <div class="rounded-lg border border-line bg-surface px-3 py-2">
            <div class="flex items-center gap-1 text-[11px] text-fg-4">
              <TriangleAlert v-if="info?.missing_files" :size="11" class="text-warning" />
              {{ t("storage.missingFiles") }}
            </div>
            <div class="tabular text-[13px]">{{ info?.missing_files ?? 0 }}</div>
          </div>
        </div>
      </section>

      <!-- 备份与校验 -->
      <section>
        <h2 class="mb-3 flex items-center gap-2 border-b border-line pb-2 text-[12px] font-semibold uppercase tracking-wide text-fg-3">
          {{ t("storage.backups") }}
          <UiButton class="ml-auto" @click="app.verifyData">
            <ShieldCheck :size="12" :stroke-width="1.75" />
            {{ t("storage.verify") }}
          </UiButton>
        </h2>
        <div class="mb-2 flex items-center gap-1.5 text-[12px] text-fg-3">
          <Database :size="13" :stroke-width="1.75" />
          {{ t("storage.backupCount") }}: {{ info?.backup_count ?? 0 }}
        </div>
        <div v-if="app.backups.length === 0" class="text-[12px] text-fg-4">—</div>
        <div class="flex flex-col gap-1">
          <div
            v-for="backup in app.backups"
            :key="backup.name"
            class="flex items-center gap-2 rounded-md border border-line bg-surface px-3 py-2"
          >
            <Layers :size="13" :stroke-width="1.75" class="shrink-0 text-fg-4" />
            <span class="min-w-0 flex-1 truncate text-[12px]" :title="backup.path">{{ backup.name }}</span>
            <span class="tabular text-[11px] text-fg-4">{{ formatSize(backup.size) }}</span>
            <button
              type="button"
              class="text-[11px] text-accent hover:underline"
              :title="t('storage.restore')"
              @click="restore(backup.path, backup.name)"
            >
              {{ t("storage.restore") }}
            </button>
          </div>
        </div>
      </section>
    </div>
  </div>
</template>
