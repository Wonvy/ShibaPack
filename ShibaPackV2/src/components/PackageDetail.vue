<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import {
  CalendarDays,
  FileArchive,
  FolderOpen,
  Play,
  Plus,
  RefreshCw,
  Sparkles,
  Star,
  Tag,
  Trash2,
  X,
} from "lucide-vue-next";
import { formatDate, formatSize } from "../lib/format";
import { api } from "../lib/api";
import { t } from "../i18n";
import { useAppStore } from "../stores/app";
import AppIcon from "./AppIcon.vue";
import UiButton from "./UiButton.vue";

const app = useAppStore();
const software = computed(() => app.selectedSoftware);
const expandedVersions = ref<Set<string>>(new Set());
const tagPickerOpen = ref(false);

watch(
  () => software.value?.id,
  () => {
    expandedVersions.value = new Set(
      software.value?.versions.filter((v) => v.is_default).map((v) => v.id) ?? [],
    );
    tagPickerOpen.value = false;
  },
  { immediate: true },
);

const summary = computed(() => {
  const key = software.value?.name.trim().toLowerCase() ?? "";
  return app.summaries[key] ?? null;
});
const summaryBusy = computed(() => {
  const key = software.value?.name.trim().toLowerCase() ?? "";
  return Boolean(app.summaryLoading[key]);
});

watch(
  () => software.value?.name,
  (name) => {
    if (name) void app.fetchSummary(name);
  },
  { immediate: true },
);

function toggleVersion(id: string) {
  const next = new Set(expandedVersions.value);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  expandedVersions.value = next;
}

function tagName(id: string): string {
  return app.tags.find((tag) => tag.id === id)?.name ?? id;
}

function toggleTag(id: string) {
  if (!software.value) return;
  const current = software.value.tags ?? [];
  const next = current.includes(id) ? current.filter((t) => t !== id) : [...current, id];
  void app.assignTags(software.value.id, next);
}

async function addVersionFiles() {
  const paths = await api.chooseFiles();
  if (paths && paths.length > 0) await app.addFiles(paths);
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") app.selectedSoftwareId = null;
}

onMounted(() => window.addEventListener("keydown", onKeydown));
onUnmounted(() => window.removeEventListener("keydown", onKeydown));
</script>

<template>
  <div v-if="software" class="flex h-full w-full flex-col bg-surface">
    <header class="flex h-11 shrink-0 items-center gap-2 border-b border-line px-3">
      <AppIcon :src="software.icon" :name="software.name" :size="24" rounded="6px" />
      <h1 class="min-w-0 truncate text-[13px] font-semibold">{{ software.name }}</h1>
      <button
        type="button"
        class="ml-auto flex size-6 shrink-0 items-center justify-center rounded text-fg-3 transition-colors hover:bg-hover hover:text-fg"
        :aria-label="t('detail.close')"
        @click="app.selectedSoftwareId = null"
      >
        <X :size="15" :stroke-width="1.75" />
      </button>
    </header>

    <div class="flex min-h-0 flex-1 flex-col overflow-y-auto p-4">
      <!-- 软件信息 -->
      <div class="rounded-xl border border-line bg-bg p-4">
        <div class="mb-4 flex items-center gap-3">
          <AppIcon :src="software.icon" :name="software.name" :size="44" rounded="12px" />
          <div class="min-w-0">
            <div class="truncate text-[15px] font-medium">{{ software.name }}</div>
            <div class="text-[12px] text-fg-3">
              {{ software.versions.length }} {{ t("storage.versionCount") }} · {{ formatDate(software.created_at) }}
            </div>
          </div>
        </div>

        <div class="flex flex-col gap-3">
          <div>
            <div class="mb-1 text-[12px] text-fg-3">{{ t("detail.category") }}</div>
            <select
              :value="software.category_id ?? ''"
              class="h-7 w-full rounded-md border border-line-strong bg-surface px-2 text-[13px] outline-none focus:border-accent"
              @change="app.assignCategory(software.id, ($event.target as HTMLSelectElement).value || null)"
            >
              <option value="">{{ t("detail.noCategory") }}</option>
              <option v-for="category in app.library.categories" :key="category.id" :value="category.id">
                {{ category.name }}
              </option>
            </select>
          </div>

          <!-- 标签 -->
          <div>
            <div class="mb-1 flex items-center gap-1.5 text-[12px] text-fg-3">
              <Tag :size="12" :stroke-width="1.75" />
              {{ t("detail.tags") }}
              <button type="button" class="ml-auto text-[11px] text-accent hover:underline" @click="tagPickerOpen = !tagPickerOpen">
                {{ t("detail.addTag") }}
              </button>
            </div>
            <div class="flex flex-wrap gap-1.5">
              <span
                v-for="id in software.tags"
                :key="id"
                class="inline-flex items-center gap-1 rounded-full border border-line-strong px-2 py-0.5 text-[11px]"
                :style="{ color: app.tags.find((t) => t.id === id)?.color }"
              >
                {{ tagName(id) }}
                <button type="button" class="text-fg-4 hover:text-fg" @click="toggleTag(id)">
                  <X :size="10" :stroke-width="2" />
                </button>
              </span>
              <span v-if="!software.tags?.length" class="text-[12px] text-fg-4">—</span>
            </div>
            <div v-if="tagPickerOpen" class="mt-2 flex flex-wrap gap-1.5 rounded-md border border-line bg-bg p-2">
              <button
                v-for="tag in app.tags"
                :key="tag.id"
                type="button"
                class="rounded-full border px-2 py-0.5 text-[11px] transition-colors"
                :class="software.tags?.includes(tag.id) ? 'border-accent bg-accent-soft' : 'border-line-strong hover:border-accent'"
                :style="{ color: tag.color }"
                @click="toggleTag(tag.id)"
              >
                {{ tag.name }}
              </button>
              <span v-if="app.tags.length === 0" class="text-[11px] text-fg-4">{{ t("tags.empty") }}</span>
            </div>
          </div>

          <div>
            <div class="mb-1 text-[12px] text-fg-3">{{ t("detail.updated") }}</div>
            <div class="flex items-center gap-1.5 text-[13px]">
              <CalendarDays :size="13" :stroke-width="1.75" class="text-fg-3" />
              {{ formatDate(software.updated_at) }}
            </div>
          </div>
        </div>
      </div>

      <!-- 摘要（置顶） -->
      <div class="mb-4 rounded-xl border border-line bg-bg p-4" style="order: -1">
        <div class="mb-2 flex items-center gap-1.5">
          <Sparkles :size="13" :stroke-width="1.75" class="text-accent" />
          <h2 class="text-[12px] font-medium text-fg-2">{{ t("detail.summary") }}</h2>
          <button
            type="button"
            class="ml-auto flex size-6 items-center justify-center rounded text-fg-3 transition-colors hover:bg-hover hover:text-fg"
            :aria-label="t('detail.summary.refresh')"
            @click="app.fetchSummary(software.name, true)"
          >
            <RefreshCw :size="12" :stroke-width="1.75" :class="summaryBusy && 'animate-spin'" />
          </button>
        </div>
        <p v-if="summaryBusy" class="text-[12px] text-fg-3">{{ t("detail.summary.loading") }}</p>
        <template v-else-if="summary && summary.summary">
          <p class="text-[13px] leading-relaxed text-fg-2">{{ summary.summary }}</p>
          <div class="mt-2 flex items-center gap-1.5 text-[11px] text-fg-4">
            <span>{{ summary.source }}</span>
            <span v-if="summary.url" class="max-w-[240px] truncate" :title="summary.url">· {{ summary.url }}</span>
          </div>
        </template>
        <div v-else class="flex items-center justify-between gap-2 text-[12px] text-fg-3">
          <span>{{ t("detail.summary.empty") }}</span>
          <button type="button" class="text-accent hover:underline" @click="app.fetchSummary(software.name, true)">{{ t("common.retry") }}</button>
        </div>
      </div>

      <!-- 全部版本 -->
      <div class="mt-5">
        <h2 class="mb-2 flex items-center gap-1.5 text-[12px] font-medium text-fg-3">
          {{ t("detail.versions") }} · {{ software.versions.length }}
        </h2>
        <div class="flex flex-col gap-2">
          <div
            v-for="version in software.versions"
            :key="version.id"
            class="overflow-hidden rounded-lg border border-line bg-bg"
          >
            <div class="flex h-10 items-center gap-2 px-3">
              <button type="button" class="flex min-w-0 flex-1 items-center gap-2 text-left" @click="toggleVersion(version.id)">
                <span class="text-[13px] font-medium">v{{ version.version || "—" }}</span>
                <span
                  v-if="version.is_default"
                  class="inline-flex items-center gap-1 rounded-full bg-accent-soft px-1.5 py-0.5 text-[10px] text-accent"
                >
                  <Star :size="9" fill="currentColor" /> {{ t("detail.default") }}
                </span>
                <span class="tabular ml-auto text-[11px] text-fg-4">{{ version.files.length }} {{ t("storage.fileCount") }}</span>
              </button>
              <button
                v-if="!version.is_default"
                type="button"
                class="text-[11px] text-fg-3 hover:text-accent"
                :title="t('detail.setDefault')"
                @click="app.setDefaultVersion(software.id, version.id)"
              >
                {{ t("detail.setDefault") }}
              </button>
              <button
                type="button"
                class="flex size-6 items-center justify-center rounded text-fg-4 hover:bg-hover hover:text-danger"
                :title="t('detail.deleteVersion')"
                @click="app.deleteVersion(software.id, version.id)"
              >
                <Trash2 :size="12" :stroke-width="1.75" />
              </button>
            </div>

            <div v-if="expandedVersions.has(version.id)" class="border-t border-line px-3 py-2">
              <div v-for="file in version.files" :key="file.id" class="flex items-start gap-2 py-1.5">
                <FileArchive :size="13" :stroke-width="1.75" class="mt-0.5 shrink-0 text-fg-4" />
                <div class="min-w-0 flex-1">
                  <div class="truncate text-[12px]" :title="file.name">{{ file.name }}</div>
                  <div class="tabular truncate text-[10px] text-fg-4" :title="file.path">{{ file.path }}</div>
                  <div v-if="file.sha256" class="tabular truncate text-[10px] text-fg-4" :title="file.sha256">
                    {{ t("detail.sha256") }}: {{ file.sha256 }}
                  </div>
                  <div class="text-[10px] text-fg-4">{{ formatSize(file.size) }} · {{ file.format.toUpperCase() }} · {{ file.role }}</div>
                </div>
                <div class="flex shrink-0 items-center gap-0.5">
                  <button
                    type="button"
                    class="flex size-6 items-center justify-center rounded text-fg-3 hover:bg-active hover:text-fg"
                    :title="t('detail.run')"
                    @click="app.runFile(software.id, file.id)"
                  >
                    <Play :size="12" fill="currentColor" :stroke-width="1.5" />
                  </button>
                  <button
                    type="button"
                    class="flex size-6 items-center justify-center rounded text-fg-3 hover:bg-active hover:text-fg"
                    :title="t('detail.reveal')"
                    @click="app.revealFile(software.id, file.id)"
                  >
                    <FolderOpen :size="12" :stroke-width="1.75" />
                  </button>
                  <button
                    type="button"
                    class="flex size-6 items-center justify-center rounded text-fg-3 hover:bg-hover hover:text-danger"
                    :title="t('detail.deleteFileBtn')"
                    @click="app.deleteFileOp(software.id, version.id, file.id, false)"
                  >
                    <X :size="12" :stroke-width="1.75" />
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <footer class="flex h-12 shrink-0 items-center gap-1.5 border-t border-line px-3">
      <UiButton @click="addVersionFiles">
        <Plus :size="13" :stroke-width="1.75" />
        {{ t("detail.addVersion") }}
      </UiButton>
      <UiButton variant="ghost" class="ml-auto text-danger" @click="app.deleteSoftware(software.id)">
        <Trash2 :size="13" :stroke-width="1.75" />
      </UiButton>
    </footer>
  </div>
</template>
