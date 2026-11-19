<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import {
  ArrowDown,
  ArrowUp,
  ChevronDown,
  FileArchive,
  FolderOpen,
  FolderTree,
  LayoutGrid,
  List,
  PackageOpen,
  Play,
  Search,
  Trash2,
  X,
} from "lucide-vue-next";
import { formatDate, formatSize } from "../lib/format";
import { t } from "../i18n";
import { useAppStore, type InstallFilter, type PackageSort } from "../stores/app";
import { useUiStore } from "../stores/ui";
import type { PackageInfo, SoftwareGroup } from "../types";
import AppIcon from "./AppIcon.vue";
import CategoryIcon from "./CategoryIcon.vue";
import PlatformBadge from "./PlatformBadge.vue";
import PlatformSelect from "./PlatformSelect.vue";
import TextField from "./TextField.vue";

const app = useAppStore();
const ui = useUiStore();
const openMenu = ref<string | null>(null);
const uninstalling = ref(false);
const installing = ref<string | null>(null);

// 列表列宽（表头与表格行共用，保证对齐）
const listGrid = { gridTemplateColumns: "minmax(0,1fr) 100px 84px 104px 136px" };

// 可排序的表头列：点击切换排序字段，同一列再次点击切换升降序
const columns: { key: PackageSort; labelKey: string; defaultDesc: boolean }[] = [
  { key: "name", labelKey: "library.sort.name", defaultDesc: false },
  { key: "platform", labelKey: "library.sort.platform", defaultDesc: false },
  { key: "size", labelKey: "library.sort.size", defaultDesc: true },
  { key: "added", labelKey: "library.sort.added", defaultDesc: true },
];

const filterOptions: { value: InstallFilter; labelKey: string }[] = [
  { value: "all", labelKey: "library.filter.all" },
  { value: "uninstalled", labelKey: "library.filter.uninstalled" },
  { value: "installed", labelKey: "library.filter.installed" },
];

const activeCategoryName = computed(() => {
  const scope = app.scope;
  if (scope.type !== "category") return "";
  return app.library.categories.find((item) => item.id === scope.id)?.name ?? t("common.all");
});

function toggleSort(col: { key: PackageSort; defaultDesc: boolean }) {
  if (app.packageSort === col.key) {
    app.packageSortDesc = !app.packageSortDesc;
  } else {
    app.packageSort = col.key;
    app.packageSortDesc = col.defaultDesc;
  }
}

// 当前可见软件的描述（联网摘要）按限流队列批量获取
watch(
  () => app.visibleGroups.map((g) => g.name).join("|"),
  () => app.ensureSummaries(app.visibleGroups.map((g) => g.name)),
  { immediate: true },
);

function description(group: SoftwareGroup): { text: string; loading: boolean } {
  const key = group.name.trim().toLowerCase();
  const summary = app.summaryOf(group.name);
  if (summary?.summary) return { text: summary.summary, loading: false };
  if (app.summaryLoading[key]) return { text: t("detail.summary.loading"), loading: true };
  return { text: `${group.latest_version ? `v${group.latest_version}` : t("library.unrecognized")} · ${formatSize(group.packages[0]?.file_size ?? 0)}`, loading: false };
}

function iconFor(group: SoftwareGroup): string {
  return app.softwareIcon(group.name);
}

function softwareIdFor(name: string): string | null {
  return app.library.software?.find((s) => s.name === name)?.id ?? null;
}

function select(pkg: PackageInfo) {
  app.selectedPackageId = pkg.id;
  const softwareId = softwareIdFor(pkg.software_name);
  if (softwareId) app.selectedSoftwareId = softwareId;
  openMenu.value = null;
}

function selectGroup(group: SoftwareGroup) {
  if (group.packages[0]) select(group.packages[0]);
}

function reveal(pkg: PackageInfo) {
  void app.revealPackage(pkg);
}

function installedProgram(group: SoftwareGroup) {
  return app.findInstalled(group.name);
}

function install(group: SoftwareGroup) {
  const pkg = group.packages[0];
  if (!pkg || installing.value) return;
  if (!window.confirm(t("library.installConfirm", { name: group.name }))) return;
  installing.value = group.name;
  void app.runPackage(pkg).finally(() => {
    installing.value = null;
  });
}

function uninstall(group: SoftwareGroup) {
  const program = installedProgram(group);
  if (!program || uninstalling.value) return;
  if (!window.confirm(t("installed.uninstallConfirm", { name: program.name }))) return;
  uninstalling.value = true;
  void app.uninstall(program).finally(() => {
    uninstalling.value = false;
  });
}

function closeMenus() {
  openMenu.value = null;
}

onMounted(() => document.addEventListener("click", closeMenus));
onUnmounted(() => document.removeEventListener("click", closeMenus));
</script>

<template>
  <div class="flex h-full min-w-0 flex-col">
    <!-- 工具栏：搜索 / 平台 / 状态 / 分类 / 数量 / 视图，全部收拢到一行 -->
    <div class="flex h-10 shrink-0 items-center gap-2 border-b border-line px-3">
      <span class="tabular shrink-0 text-[11px] text-fg-4">{{ app.visibleGroups.length }}</span>
      <div class="relative min-w-0 flex-1">
        <Search
          :size="13"
          :stroke-width="1.75"
          class="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-fg-3"
        />
        <TextField v-model="app.search" :placeholder="t('app.search.library')" class="pl-7" />
      </div>
      <PlatformSelect v-model="app.platformFilter" />
      <div class="relative shrink-0">
        <select
          v-model="app.installFilter"
          class="h-8 appearance-none rounded-md border border-line-strong bg-active pl-2 pr-6 text-[11px] text-fg-2 outline-none transition-colors hover:text-fg"
          :title="t('library.filter.label')"
        >
          <option v-for="opt in filterOptions" :key="opt.value" :value="opt.value">
            {{ t(opt.labelKey) }}
          </option>
        </select>
        <ChevronDown :size="11" :stroke-width="1.75" class="pointer-events-none absolute right-1.5 top-1/2 -translate-y-1/2 text-fg-3" />
      </div>
      <button
        v-if="activeCategoryName"
        type="button"
        class="flex h-8 shrink-0 items-center gap-1 rounded-md border border-line-strong bg-active px-2 text-[12px] text-fg-2"
        @click="app.chooseScope({ type: 'all' })"
      >
        {{ activeCategoryName }}
        <X :size="12" :stroke-width="1.75" />
      </button>
      <span class="mx-0.5 h-4 w-px shrink-0 bg-line" />
      <div class="flex shrink-0 items-center gap-0.5">
        <button
          type="button"
          class="flex size-7 items-center justify-center rounded-md transition-colors"
          :class="ui.packageView === 'list' ? 'bg-active text-fg' : 'text-fg-4 hover:bg-hover hover:text-fg-2'"
          :aria-label="t('library.view.list')"
          :title="t('library.view.list')"
          @click="ui.packageView = 'list'"
        >
          <List :size="14" :stroke-width="1.75" />
        </button>
        <button
          type="button"
          class="flex size-7 items-center justify-center rounded-md transition-colors"
          :class="ui.packageView === 'grid' ? 'bg-active text-fg' : 'text-fg-4 hover:bg-hover hover:text-fg-2'"
          :aria-label="t('library.view.grid')"
          :title="t('library.view.grid')"
          @click="ui.packageView = 'grid'"
        >
          <LayoutGrid :size="14" :stroke-width="1.75" />
        </button>
        <button
          type="button"
          class="flex size-7 items-center justify-center rounded-md transition-colors"
          :class="ui.packageView === 'folder' ? 'bg-active text-fg' : 'text-fg-4 hover:bg-hover hover:text-fg-2'"
          :aria-label="t('library.view.folder')"
          :title="t('library.view.folder')"
          @click="ui.packageView = 'folder'"
        >
          <FolderTree :size="14" :stroke-width="1.75" />
        </button>
      </div>
    </div>

    <!-- 可点击排序的表头（仅列表视图） -->
    <div
      v-if="ui.packageView === 'list'"
      class="grid h-8 shrink-0 items-center gap-2 border-b border-line bg-surface px-3 text-[11px] text-fg-4"
      :style="listGrid"
    >
      <button
        v-for="col in columns"
        :key="col.key"
        type="button"
        class="flex items-center gap-1 text-left transition-colors hover:text-fg"
        :class="app.packageSort === col.key ? 'font-medium text-fg-2' : ''"
        @click="toggleSort(col)"
      >
        {{ t(col.labelKey) }}
        <ArrowDown v-if="app.packageSort === col.key && app.packageSortDesc" :size="11" :stroke-width="2" />
        <ArrowUp v-else-if="app.packageSort === col.key" :size="11" :stroke-width="2" />
      </button>
      <span />
    </div>

    <div class="min-h-0 flex-1 overflow-y-auto px-3 pb-3 pt-1">
      <template v-if="ui.packageView !== 'folder'">
      <template v-for="section in app.categorizedGroups" :key="section.category?.id ?? 'uncategorized'">
        <!-- 分类标题：中性色 + 细分隔线，分组间距统一 -->
        <div class="mb-1.5 mt-5 flex items-center gap-2 px-1 first:mt-1">
          <CategoryIcon :name="section.category?.icon" :size="13" class="text-fg-4" />
          <span class="shrink-0 text-[11px] font-medium uppercase tracking-wide text-fg-3">
            {{ section.category?.name ?? t("detail.noCategory") }}
          </span>
          <span class="tabular text-[10px] text-fg-4">{{ section.groups.length }}</span>
          <span class="h-px min-w-0 flex-1 bg-line" />
        </div>

        <!-- 列表视图 -->
        <template v-if="ui.packageView === 'list'">
          <div
            v-for="group in section.groups"
            :key="group.name"
            role="button"
            tabindex="0"
            class="group relative grid min-h-[52px] items-center gap-2 rounded-lg px-2.5 py-1.5 text-left transition-colors"
            :style="listGrid"
            :class="softwareIdFor(group.name) && app.selectedSoftwareId === softwareIdFor(group.name) ? 'bg-active text-fg' : 'text-fg-2 hover:bg-hover hover:text-fg'"
            :title="group.packages[0]?.full_path"
            @click="selectGroup(group)"
            @keydown.enter="selectGroup(group)"
          >
            <!-- 软件：真实图标 + 名称 + 描述 -->
            <div class="flex min-w-0 items-center gap-2.5">
              <AppIcon :src="iconFor(group)" :name="group.name" :size="36" rounded="9px" />
              <span class="min-w-0 flex-1">
                <span class="flex min-w-0 items-center gap-1.5">
                  <span class="truncate text-[13px] font-medium">{{ group.name }}</span>
                  <button
                    v-if="group.packages.length > 1"
                    type="button"
                    class="flex h-5 shrink-0 items-center gap-0.5 rounded border border-line-strong px-1 text-[10px] text-fg-3 transition-colors hover:bg-active hover:text-fg"
                    :aria-label="t('library.versions', { count: group.packages.length })"
                    @click.stop="openMenu = openMenu === group.name ? null : group.name"
                  >
                    {{ group.packages.length }} <ChevronDown :size="10" :stroke-width="1.75" />
                  </button>
                </span>
                <span class="mt-0.5 block truncate text-[11px] text-fg-4">
                  {{ description(group).text }}
                </span>
              </span>
            </div>

            <!-- 平台 -->
            <div class="flex flex-wrap items-center gap-1">
              <PlatformBadge v-for="platform in group.platforms" :key="platform" :platform="platform" />
            </div>

            <!-- 大小 -->
            <span class="tabular truncate text-[11px] text-fg-3">{{ formatSize(group.packages[0]?.file_size ?? 0) }}</span>

            <!-- 添加时间 -->
            <span class="tabular truncate text-[11px] text-fg-3">{{ formatDate(group.packages[0]?.added_time) }}</span>

            <!-- 操作：悬停显示安装/卸载、打开文件夹（中性配色） -->
            <div class="flex items-center justify-end gap-1 opacity-0 transition-opacity group-hover:opacity-100">
              <button
                v-if="installedProgram(group)"
                type="button"
                class="flex h-7 shrink-0 items-center gap-1 rounded-md border border-line-strong px-2 text-[11px] text-fg-3 transition-colors hover:border-danger/40 hover:text-danger disabled:opacity-40"
                :disabled="uninstalling"
                :aria-label="t('library.uninstall')"
                @click.stop="uninstall(group)"
              >
                <Trash2 :size="13" :stroke-width="1.75" />
                {{ t("library.uninstall") }}
              </button>
              <button
                v-else
                type="button"
                class="flex h-7 shrink-0 items-center gap-1 rounded-md border border-line-strong px-2 text-[11px] text-fg-2 transition-colors hover:bg-active hover:text-fg disabled:opacity-40"
                :disabled="installing === group.name"
                :aria-label="t('library.install')"
                @click.stop="install(group)"
              >
                <Play :size="12" fill="currentColor" :stroke-width="1.5" />
                {{ installing === group.name ? t("library.installing") : t("library.install") }}
              </button>
              <button
                v-if="group.packages[0]"
                type="button"
                class="flex size-7 shrink-0 items-center justify-center rounded-md text-fg-3 transition-colors hover:bg-active hover:text-fg"
                :aria-label="t('library.reveal')"
                :title="group.packages[0].full_path"
                @click.stop="reveal(group.packages[0])"
              >
                <FolderOpen :size="14" :stroke-width="1.75" />
              </button>
            </div>

            <!-- 版本选择浮层 -->
            <div
              v-if="openMenu === group.name"
              class="absolute right-2 top-[calc(100%-4px)] z-20 w-[280px] overflow-hidden rounded-lg border border-line-strong bg-surface py-1 shadow-[var(--shadow-pop)]"
              @click.stop
            >
              <button
                v-for="pkg in group.packages"
                :key="pkg.id"
                type="button"
                class="flex h-9 w-full items-center gap-2 px-3 text-left text-[12px] transition-colors"
                :class="app.selectedPackageId === pkg.id ? 'bg-active text-fg' : 'text-fg-2 hover:bg-hover hover:text-fg'"
                @click="select(pkg)"
              >
                <span class="font-medium">v{{ pkg.version || "—" }}</span>
                <PlatformBadge :platform="pkg.platform" />
                <span class="truncate text-fg-3">{{ pkg.extension.toUpperCase() }}</span>
                <span class="tabular ml-auto shrink-0 text-[11px] text-fg-3">{{ formatSize(pkg.file_size) }}</span>
              </button>
            </div>
          </div>
        </template>

        <!-- 图标视图 -->
        <div v-else-if="ui.packageView === 'grid'" class="grid grid-cols-[repeat(auto-fill,minmax(148px,1fr))] gap-2">
          <div
            v-for="group in section.groups"
            :key="group.name"
            role="button"
            tabindex="0"
            class="group relative flex flex-col items-center rounded-lg border border-line bg-surface px-2 pb-2.5 pt-4 text-center transition-colors"
            :class="softwareIdFor(group.name) && app.selectedSoftwareId === softwareIdFor(group.name) ? 'border-line-strong bg-active' : 'hover:border-line-strong hover:bg-hover'"
            :title="group.packages[0]?.full_path"
            @click="selectGroup(group)"
            @keydown.enter="selectGroup(group)"
          >
            <div class="absolute left-1.5 top-1.5 flex gap-0.5">
              <button
                v-if="installedProgram(group)"
                type="button"
                class="flex size-6 items-center justify-center rounded-md text-fg-3 opacity-0 transition-all hover:bg-hover hover:text-danger group-hover:opacity-100 disabled:opacity-40"
                :disabled="uninstalling"
                :aria-label="t('library.uninstall')"
                @click.stop="uninstall(group)"
              >
                <Trash2 :size="13" :stroke-width="1.75" />
              </button>
              <button
                v-else
                type="button"
                class="flex size-6 items-center justify-center rounded-md text-fg-3 opacity-0 transition-all hover:bg-active hover:text-fg group-hover:opacity-100 disabled:opacity-40"
                :disabled="installing === group.name"
                :aria-label="t('library.install')"
                @click.stop="install(group)"
              >
                <Play :size="12" fill="currentColor" :stroke-width="1.5" />
              </button>
            </div>
            <button
              v-if="group.packages[0]"
              type="button"
              class="absolute right-1.5 top-1.5 flex size-6 items-center justify-center rounded-md text-fg-3 opacity-0 transition-all hover:bg-active hover:text-fg group-hover:opacity-100"
              :aria-label="t('library.reveal')"
              :title="group.packages[0].full_path"
              @click.stop="reveal(group.packages[0])"
            >
              <FolderOpen :size="13" :stroke-width="1.75" />
            </button>

            <AppIcon :src="iconFor(group)" :name="group.name" :size="48" rounded="12px" />
            <span class="mt-2 w-full truncate text-[12px] font-medium text-fg">{{ group.name }}</span>
            <span class="mt-0.5 w-full truncate text-[11px] text-fg-4" :title="description(group).text">
              {{ description(group).text }}
            </span>

            <span class="mt-1.5 flex flex-wrap items-center justify-center gap-1">
              <PlatformBadge v-for="platform in group.platforms" :key="platform" :platform="platform" />
            </span>

            <button
              v-if="group.packages.length > 1"
              type="button"
              class="mt-1.5 flex h-6 items-center gap-1 rounded-md border border-line-strong px-1.5 text-[10px] text-fg-3 transition-colors hover:bg-active hover:text-fg"
              @click.stop="openMenu = openMenu === `grid-${group.name}` ? null : `grid-${group.name}`"
            >
              {{ t("library.versions", { count: group.packages.length }) }}
              <ChevronDown :size="10" :stroke-width="1.75" />
            </button>

            <div
              v-if="openMenu === `grid-${group.name}`"
              class="absolute left-1 right-1 top-[calc(100%+2px)] z-20 overflow-hidden rounded-lg border border-line-strong bg-surface py-1 shadow-[var(--shadow-pop)]"
              @click.stop
            >
              <button
                v-for="pkg in group.packages"
                :key="pkg.id"
                type="button"
                class="flex h-8 w-full items-center gap-1.5 px-2 text-left text-[11px] transition-colors"
                :class="app.selectedPackageId === pkg.id ? 'bg-active text-fg' : 'text-fg-2 hover:bg-hover hover:text-fg'"
                @click="select(pkg)"
              >
                <span class="font-medium">v{{ pkg.version || "—" }}</span>
                <span class="truncate text-fg-3">{{ pkg.extension.toUpperCase() }}</span>
                <span class="tabular ml-auto shrink-0 text-[10px] text-fg-3">{{ formatSize(pkg.file_size) }}</span>
              </button>
            </div>
          </div>
        </div>
      </template>

      <div v-if="app.libraryLoading && app.visibleGroups.length === 0" class="flex h-full flex-col items-center justify-center gap-2 px-6 text-center">
        <PackageOpen :size="28" :stroke-width="1.25" class="animate-pulse text-fg-4" />
        <p class="text-[13px] text-fg-3">{{ t("app.status.loading") }}</p>
      </div>
      <div v-else-if="app.visibleGroups.length === 0" class="flex h-full flex-col items-center justify-center gap-2 px-6 text-center">
        <PackageOpen :size="28" :stroke-width="1.25" class="text-fg-4" />
        <p class="text-[13px] text-fg-2">{{ t("library.empty.title") }}</p>
        <p class="text-[12px] leading-relaxed text-fg-3">{{ t("library.empty.desc") }}</p>
      </div>
      </template>

      <!-- 文件夹视图：按 软件 / 版本 / 文件 的物理层级浏览 -->
      <template v-else>
        <div v-for="software in app.library.software" :key="software.id" class="mb-1">
          <div class="flex items-center gap-2 rounded-md px-2 py-1.5 text-[13px] font-medium">
            <FolderTree :size="14" :stroke-width="1.75" class="text-fg-4" />
            <AppIcon :src="software.icon" :name="software.name" :size="18" rounded="5px" />
            <span class="truncate">{{ software.name }}</span>
            <span class="tabular ml-auto text-[11px] text-fg-4">{{ software.versions.length }}</span>
          </div>
          <div v-for="version in software.versions" :key="version.id" class="ml-5 border-l border-line pl-3">
            <div class="flex items-center gap-2 py-1 text-[12px] text-fg-3">
              <FolderOpen :size="12" :stroke-width="1.75" />
              <span>v{{ version.version || "—" }}</span>
              <span v-if="version.is_default" class="rounded-full bg-hover px-1.5 text-[10px] text-fg-3">{{ t("detail.default") }}</span>
            </div>
            <button
              v-for="file in version.files"
              :key="file.id"
              type="button"
              class="ml-5 flex w-[calc(100%-1.25rem)] items-center gap-2 rounded-md px-2 py-1 text-left text-[12px] transition-colors hover:bg-hover"
              @click="app.selectedSoftwareId = software.id"
            >
              <FileArchive :size="12" :stroke-width="1.75" class="shrink-0 text-fg-4" />
              <span class="min-w-0 flex-1 truncate text-fg-2">{{ file.name }}</span>
              <span class="tabular shrink-0 text-[11px] text-fg-4">{{ formatSize(file.size) }}</span>
            </button>
          </div>
        </div>
        <div v-if="!app.library.software?.length" class="py-20 text-center text-[12px] text-fg-4">
          {{ t("library.empty.title") }}
        </div>
      </template>
    </div>
  </div>
</template>
