<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { FolderOpen, RefreshCw, Sparkles, Trash2, X } from "lucide-vue-next";
import AppIcon from "./AppIcon.vue";
import { api } from "../lib/api";
import { formatDate, formatSize } from "../lib/format";
import { t } from "../i18n";
import { useAppStore } from "../stores/app";

const app = useAppStore();
const uninstalling = ref(false);

const selected = computed(() => app.selectedInstalled);

const summary = computed(() => {
  const key = selected.value?.name.trim().toLowerCase() ?? "";
  return app.summaries[key] ?? null;
});
const summaryBusy = computed(() => {
  const key = selected.value?.name.trim().toLowerCase() ?? "";
  return Boolean(app.summaryLoading[key]);
});

const uninstallHint = computed(() => {
  const platform = app.status?.platform;
  const key =
    platform === "macos" ? "installed.uninstallHint.macos"
      : platform === "windows" ? "installed.uninstallHint.windows"
        : "installed.uninstallHint.linux";
  return t(key);
});

watch(
  () => selected.value?.name,
  (name) => {
    if (name) void app.fetchSummary(name);
  },
  { immediate: true },
);

async function confirmUninstall() {
  const program = selected.value;
  if (!program || uninstalling.value) return;
  if (!window.confirm(t("installed.uninstallConfirm", { name: program.name }))) return;
  uninstalling.value = true;
  await app.uninstall(program);
  uninstalling.value = false;
}

function reveal() {
  if (selected.value?.location) void api.revealPath(selected.value.location);
}
</script>

<template>
  <div v-if="selected" class="flex h-full w-full flex-col bg-surface">
    <header class="flex h-11 shrink-0 items-center gap-2 border-b border-line px-3">
      <AppIcon :src="selected.icon" :name="selected.name" :size="24" rounded="6px" />
      <h1 class="min-w-0 flex-1 truncate text-[13px] font-semibold">{{ selected.name }}</h1>
      <button
        type="button"
        class="flex size-6 shrink-0 items-center justify-center rounded text-fg-3 transition-colors hover:bg-hover hover:text-fg"
        :aria-label="t('detail.close')"
        @click="app.selectedInstalledId = null"
      >
        <X :size="15" :stroke-width="1.75" />
      </button>
    </header>

    <div class="flex min-h-0 flex-1 flex-col overflow-y-auto p-4">
      <div class="rounded-xl border border-line bg-bg p-4">
        <div class="mb-4 flex items-center gap-3">
          <AppIcon :src="selected.icon" :name="selected.name" :size="48" rounded="12px" />
          <div class="min-w-0">
            <div class="truncate text-[15px] font-medium">{{ selected.name }}</div>
            <div class="text-[12px] text-fg-3">{{ t("common.version") }} {{ selected.version || "—" }}</div>
          </div>
        </div>
        <dl class="flex flex-col gap-3">
          <div>
            <dt class="mb-1 text-[12px] text-fg-3">{{ t("installed.publisher") }}</dt>
            <dd class="text-[13px]">{{ selected.publisher || "—" }}</dd>
          </div>
          <div>
            <dt class="mb-1 text-[12px] text-fg-3">{{ t("installed.installDate") }}</dt>
            <dd class="text-[13px]">{{ formatDate(selected.install_date) }}</dd>
          </div>
          <div v-if="selected.size_bytes">
            <dt class="mb-1 text-[12px] text-fg-3">{{ t("installed.size") }}</dt>
            <dd class="tabular text-[13px]">{{ formatSize(selected.size_bytes) }}</dd>
          </div>
          <div>
            <dt class="mb-1 text-[12px] text-fg-3">{{ t("installed.source") }}</dt>
            <dd class="text-[13px]">{{ selected.source }}</dd>
          </div>
          <div class="min-w-0">
            <dt class="mb-1 text-[12px] text-fg-3">{{ t("installed.location") }}</dt>
            <dd class="flex items-start gap-1.5 break-all text-[13px] text-fg-2" :title="selected.location">
              <FolderOpen :size="13" :stroke-width="1.75" class="mt-0.5 shrink-0 text-fg-3" />
              <span>{{ selected.location || "—" }}</span>
            </dd>
          </div>
        </dl>

        <div class="mt-4 flex flex-wrap items-center gap-2 border-t border-line pt-3">
          <button
            type="button"
            class="flex h-8 items-center gap-1.5 rounded-md border border-danger/40 px-2.5 text-[12px] text-danger transition-colors hover:bg-danger/10 disabled:opacity-50"
            :disabled="uninstalling"
            @click="confirmUninstall"
          >
            <Trash2 :size="13" :stroke-width="1.75" />
            {{ uninstalling ? t("installed.uninstalling") : t("installed.uninstall") }}
          </button>
          <button
            type="button"
            class="flex h-8 items-center gap-1.5 rounded-md border border-line-strong px-2.5 text-[12px] text-fg-2 transition-colors hover:bg-hover hover:text-fg"
            @click="reveal"
          >
            <FolderOpen :size="13" :stroke-width="1.75" />
            {{ t("installed.reveal") }}
          </button>
          <span class="ml-auto max-w-[180px] text-right text-[11px] leading-tight text-fg-4">{{ uninstallHint }}</span>
        </div>
      </div>

      <div class="mb-4 rounded-xl border border-line bg-bg p-4" style="order: -1">
        <div class="mb-2 flex items-center gap-1.5">
          <Sparkles :size="13" :stroke-width="1.75" class="text-accent" />
          <h2 class="text-[12px] font-medium text-fg-2">{{ t("detail.summary") }}</h2>
          <button
            type="button"
            class="ml-auto flex size-6 items-center justify-center rounded text-fg-3 transition-colors hover:bg-hover hover:text-fg"
            :aria-label="t('detail.summary.refresh')"
            @click="app.fetchSummary(selected.name, true)"
          >
            <RefreshCw :size="12" :stroke-width="1.75" :class="summaryBusy && 'animate-spin'" />
          </button>
        </div>
        <p v-if="summaryBusy" class="text-[12px] text-fg-3">{{ t("detail.summary.loading") }}</p>
        <template v-else-if="summary && summary.summary">
          <p class="text-[13px] leading-relaxed text-fg-2">{{ summary.summary }}</p>
          <div class="mt-2 text-[11px] text-fg-4">{{ summary.source }}</div>
        </template>
        <div v-else class="flex items-center justify-between gap-2 text-[12px] text-fg-3">
          <span>{{ t("detail.summary.empty") }}</span>
          <button type="button" class="text-accent hover:underline" @click="app.fetchSummary(selected.name, true)">
            {{ t("common.retry") }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
