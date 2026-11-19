<script setup lang="ts">
import { computed } from "vue";
import {
  AppWindow,
  ArrowDown,
  ArrowUp,
  FolderOpen,
  LayoutGrid,
  List,
  Search,
} from "lucide-vue-next";
import AppIcon from "../components/AppIcon.vue";
import TextField from "../components/TextField.vue";
import { api } from "../lib/api";
import { formatDate, formatSize } from "../lib/format";
import { t } from "../i18n";
import { useAppStore, type InstalledSort } from "../stores/app";
import { useUiStore } from "../stores/ui";
import type { InstalledProgram } from "../types";

const app = useAppStore();
const ui = useUiStore();

// 列表列宽（表头与表格行共用）
const listGrid = { gridTemplateColumns: "minmax(0,1fr) 120px 84px 110px 92px 36px" };

const columns: { key: InstalledSort; labelKey: string; defaultDesc: boolean }[] = [
  { key: "name", labelKey: "installed.sort.name", defaultDesc: false },
  { key: "publisher", labelKey: "installed.sort.publisher", defaultDesc: false },
  { key: "size", labelKey: "installed.sort.size", defaultDesc: true },
  { key: "install_date", labelKey: "installed.sort.installDate", defaultDesc: true },
  { key: "source", labelKey: "installed.sort.source", defaultDesc: false },
];

const items = computed(() => app.visibleInstalled);

function toggleSort(col: { key: InstalledSort; defaultDesc: boolean }) {
  if (app.installedSort === col.key) {
    app.installedSortDesc = !app.installedSortDesc;
  } else {
    app.installedSort = col.key;
    app.installedSortDesc = col.defaultDesc;
  }
}

function reveal(item: InstalledProgram) {
  if (item.location) void api.revealPath(item.location);
}
</script>

<template>
  <div class="flex h-full min-w-0 flex-col">
    <!-- 工具栏：数量 / 搜索 / 视图切换，收拢到一行 -->
    <div class="flex h-10 shrink-0 items-center gap-2 border-b border-line px-3">
      <span class="tabular shrink-0 text-[11px] text-fg-4">{{ items.length }}</span>
      <div class="relative min-w-0 flex-1">
        <Search
          :size="13"
          :stroke-width="1.75"
          class="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-fg-3"
        />
        <TextField v-model="app.search" :placeholder="t('app.search.installed')" class="pl-7" />
      </div>
      <div class="flex shrink-0 items-center gap-0.5">
        <button
          type="button"
          class="flex size-7 items-center justify-center rounded-md transition-colors"
          :class="ui.installedView === 'list' ? 'bg-active text-fg' : 'text-fg-4 hover:bg-hover hover:text-fg-2'"
          :aria-label="t('library.view.list')"
          :title="t('library.view.list')"
          @click="ui.installedView = 'list'"
        >
          <List :size="14" :stroke-width="1.75" />
        </button>
        <button
          type="button"
          class="flex size-7 items-center justify-center rounded-md transition-colors"
          :class="ui.installedView === 'grid' ? 'bg-active text-fg' : 'text-fg-4 hover:bg-hover hover:text-fg-2'"
          :aria-label="t('library.view.grid')"
          :title="t('library.view.grid')"
          @click="ui.installedView = 'grid'"
        >
          <LayoutGrid :size="14" :stroke-width="1.75" />
        </button>
      </div>
    </div>

    <!-- 可点击排序的表头（仅列表视图） -->
    <div
      v-if="ui.installedView === 'list'"
      class="grid h-8 shrink-0 items-center gap-2 border-b border-line bg-surface px-3 text-[11px] text-fg-4"
      :style="listGrid"
    >
      <button
        v-for="col in columns"
        :key="col.key"
        type="button"
        class="flex items-center gap-1 text-left transition-colors hover:text-fg"
        :class="app.installedSort === col.key ? 'font-medium text-fg' : ''"
        @click="toggleSort(col)"
      >
        {{ t(col.labelKey) }}
        <ArrowDown v-if="app.installedSort === col.key && app.installedSortDesc" :size="11" :stroke-width="2" />
        <ArrowUp v-else-if="app.installedSort === col.key" :size="11" :stroke-width="2" />
      </button>
      <span />
    </div>

    <div class="min-h-0 flex-1 overflow-y-auto px-3 py-2">
      <!-- 列表视图 -->
      <template v-if="ui.installedView === 'list'">
        <div
          v-for="item in items"
          :key="item.id"
          role="button"
          tabindex="0"
          class="group relative grid min-h-[60px] items-center gap-2 rounded-lg px-3 py-2 text-left transition-colors"
          :style="listGrid"
          :class="app.selectedInstalledId === item.id ? 'bg-active text-fg' : 'text-fg-2 hover:bg-hover hover:text-fg'"
          :title="item.location"
          @click="app.selectedInstalledId = item.id"
          @keydown.enter="app.selectedInstalledId = item.id"
        >
          <!-- 软件 -->
          <div class="flex min-w-0 items-center gap-2.5">
            <AppIcon :src="item.icon" :name="item.name" :size="40" />
            <span class="min-w-0 flex-1">
              <span class="block truncate text-[13px] font-medium">{{ item.name }}</span>
              <span class="mt-0.5 block truncate text-[11px] text-fg-3">
                {{ item.version ? `v${item.version}` : t("library.unrecognized") }}
              </span>
            </span>
          </div>

          <!-- 发布者 -->
          <span class="truncate text-[11px] text-fg-3">{{ item.publisher || "—" }}</span>

          <!-- 大小 -->
          <span class="tabular truncate text-[11px] text-fg-3">{{ item.size_bytes ? formatSize(item.size_bytes) : "—" }}</span>

          <!-- 安装时间 -->
          <span class="tabular truncate text-[11px] text-fg-3">{{ formatDate(item.install_date) }}</span>

          <!-- 来源 -->
          <span class="truncate text-[11px] text-fg-3">{{ item.source || "—" }}</span>

          <!-- 操作：悬停显示打开文件夹 -->
          <button
            v-if="item.location"
            type="button"
            class="flex size-7 items-center justify-center rounded-md text-fg-3 opacity-0 transition-opacity hover:bg-active hover:text-fg group-hover:opacity-100"
            :aria-label="t('library.reveal')"
            :title="item.location"
            @click.stop="reveal(item)"
          >
            <FolderOpen :size="14" :stroke-width="1.75" />
          </button>
        </div>
      </template>

      <!-- 图标视图 -->
      <div v-else class="grid grid-cols-[repeat(auto-fill,minmax(148px,1fr))] gap-2">
        <div
          v-for="item in items"
          :key="item.id"
          role="button"
          tabindex="0"
          class="group relative flex flex-col items-center rounded-lg border border-line bg-surface px-2 pb-2.5 pt-4 text-center transition-colors"
          :class="app.selectedInstalledId === item.id ? 'border-accent bg-accent-soft/40' : 'hover:border-line-strong hover:bg-hover'"
          :title="item.location"
          @click="app.selectedInstalledId = item.id"
          @keydown.enter="app.selectedInstalledId = item.id"
        >
          <button
            v-if="item.location"
            type="button"
            class="absolute right-1.5 top-1.5 flex size-6 items-center justify-center rounded-md text-fg-3 opacity-0 transition-all hover:bg-active hover:text-fg group-hover:opacity-100"
            :aria-label="t('library.reveal')"
            :title="item.location"
            @click.stop="reveal(item)"
          >
            <FolderOpen :size="13" :stroke-width="1.75" />
          </button>

          <AppIcon :src="item.icon" :name="item.name" :size="48" rounded="12px" />
          <span class="mt-2 w-full truncate text-[12px] font-medium text-fg">{{ item.name }}</span>
          <span class="mt-0.5 w-full truncate text-[11px] text-fg-3">
            {{ item.version ? `v${item.version}` : t("library.unrecognized") }}
          </span>
          <span class="mt-0.5 w-full truncate text-[11px] text-fg-4">
            {{ item.size_bytes ? formatSize(item.size_bytes) : item.publisher }}
          </span>
        </div>
      </div>

      <div v-if="app.installedLoading && items.length === 0" class="flex h-full flex-col items-center justify-center gap-2 px-6 text-center">
        <AppWindow :size="28" :stroke-width="1.25" class="animate-pulse text-fg-4" />
        <p class="text-[13px] text-fg-3">{{ t("app.status.loading") }}</p>
      </div>
      <div v-else-if="items.length === 0" class="flex h-full flex-col items-center justify-center gap-2 px-6 text-center">
        <AppWindow :size="28" :stroke-width="1.25" class="text-fg-4" />
        <p class="text-[13px] text-fg-2">{{ t("installed.empty.title") }}</p>
        <p class="max-w-[320px] text-[12px] leading-relaxed text-fg-3">
          {{ t("installed.empty.desc") }}
        </p>
      </div>
    </div>
  </div>
</template>
