<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { useRoute } from "vue-router";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { Check, PackageOpen, Plus, RefreshCw, Settings } from "lucide-vue-next";
import AppLogo from "./components/AppLogo.vue";
import AppSidebar from "./components/AppSidebar.vue";
import IconButton from "./components/IconButton.vue";
import InstalledDetail from "./components/InstalledDetail.vue";
import Modal from "./components/Modal.vue";
import PackageDetail from "./components/PackageDetail.vue";
import SettingsView from "./views/SettingsView.vue";
import SetupGuide from "./components/SetupGuide.vue";
import ToastHost from "./components/ToastHost.vue";
import WindowControls from "./components/WindowControls.vue";
import { api, isDesktop } from "./lib/api";
import { formatSize } from "./lib/format";
import { t } from "./i18n";
import { useAppStore } from "./stores/app";
import { useUiStore } from "./stores/ui";

const app = useAppStore();
const ui = useUiStore();
const route = useRoute();

const dragHover = ref(false);
let unlistenDrag: (() => void) | null = null;

const importPercent = computed(() => {
  const p = app.importProgress;
  if (!p || p.total <= 0) return 0;
  return Math.min(100, Math.round((p.current / p.total) * 100));
});

function fileName(path: string): string {
  if (!path) return "";
  return path.split(/[\\/]/).pop() ?? path;
}

onMounted(() => {
  void app.initialize();
  // 原生文件拖放：Tauri 的 drag-drop 事件不受 DOM 层叠影响
  if (isDesktop) {
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        const payload = event.payload;
        if (payload.type === "enter") {
          dragHover.value = true;
        } else if (payload.type === "leave") {
          dragHover.value = false;
        } else if (payload.type === "drop") {
          dragHover.value = false;
          const paths = (payload.paths ?? []).filter(Boolean);
          if (paths.length > 0) void app.addFiles(paths);
        }
      })
      .then((fn) => {
        unlistenDrag = fn;
      });
  }
});

watch(
  () => route.name,
  () => {
    app.search = "";
  },
);

async function addFiles() {
  const paths = await api.chooseFiles();
  if (paths && paths.length > 0) await app.addFiles(paths);
}

const resizing = ref<"sidebar" | "detail" | null>(null);

function startResize(target: "sidebar" | "detail", event: MouseEvent) {
  event.preventDefault();
  resizing.value = target;
  document.body.style.cursor = "col-resize";
  document.body.style.userSelect = "none";
  window.addEventListener("mousemove", onResize);
  window.addEventListener("mouseup", stopResize);
}

function onResize(event: MouseEvent) {
  if (resizing.value === "sidebar") {
    ui.sidebarWidth = Math.min(360, Math.max(140, event.clientX));
  } else if (resizing.value === "detail") {
    const width = window.innerWidth - event.clientX;
    ui.detailWidth = Math.min(560, Math.max(280, width));
  }
}

function stopResize() {
  resizing.value = null;
  document.body.style.cursor = "";
  document.body.style.userSelect = "";
  window.removeEventListener("mousemove", onResize);
  window.removeEventListener("mouseup", stopResize);
}

onUnmounted(() => {
  unlistenDrag?.();
  stopResize();
});
</script>

<template>
  <div class="relative flex h-full flex-col overflow-hidden bg-bg text-fg">
    <!-- 顶部栏：品牌 · 操作（搜索与筛选已整合到列表工具栏） -->
    <header class="drag-region flex h-11 shrink-0 items-center gap-3 border-b border-line bg-sidebar px-3">
      <div class="flex shrink-0 items-center gap-2" :style="{ width: `${ui.sidebarWidth - 24}px` }">
        <AppLogo :size="18" />
        <span class="text-[13px] font-semibold tracking-tight">ShibaPack</span>
      </div>

      <div class="no-drag ml-auto flex shrink-0 items-center gap-0.5">
        <IconButton label="添加安装包" @click="addFiles">
          <Plus :size="16" :stroke-width="1.75" />
        </IconButton>
        <IconButton :label="t('app.action.scan')" @click="app.scan()">
          <RefreshCw :size="15" :stroke-width="1.75" :class="app.scanning && 'animate-spin'" />
        </IconButton>
        <IconButton :label="t('app.action.settings')" :active="ui.settingsOpen" @click="ui.settingsOpen = true">
          <Settings :size="15" :stroke-width="1.75" />
        </IconButton>
      </div>

      <WindowControls />
    </header>

    <!-- 主体：三栏（分类 | 列表 | 详情），两处可拖拽调整宽度 -->
    <div class="flex min-h-0 flex-1">
      <AppSidebar />
      <div
        class="w-1 shrink-0 cursor-col-resize border-r border-line transition-colors"
        :class="resizing === 'sidebar' ? 'bg-accent' : 'bg-transparent hover:bg-accent-soft'"
        title="拖拽调整宽度"
        @mousedown="startResize('sidebar', $event)"
      />

      <!-- 中间栏：软件列表 -->
      <main class="relative min-w-0 flex-1">
        <RouterView />

        <!-- 导入进度条：拖入/添加安装包时显示 -->
        <div
          v-if="app.importing"
          class="pointer-events-none absolute inset-x-0 top-0 z-30 border-b border-line bg-surface/95 shadow-[var(--shadow-pop)]"
        >
          <div class="h-[3px] w-full bg-line">
            <div
              class="h-full bg-accent transition-[width] duration-200 ease-out"
              :style="{ width: `${importPercent}%` }"
            />
          </div>
          <div class="flex h-7 items-center gap-2 px-3 text-[11px]">
            <RefreshCw :size="11" :stroke-width="2" class="animate-spin text-fg-3" />
            <span class="tabular shrink-0 text-fg-2">
              {{ t("app.status.importProgress", { current: app.importProgress?.current ?? 0, total: app.importProgress?.total ?? 0 }) }}
            </span>
            <span class="min-w-0 truncate text-fg-4">{{ fileName(app.importProgress?.path ?? "") }}</span>
          </div>
        </div>
      </main>

      <!-- 第三栏：详情（宽度可拖拽），标签/存储页占满中间栏 -->
      <template v-if="route.name === 'library' || route.name === 'installed'">
      <div
        class="w-1 shrink-0 cursor-col-resize border-l border-line transition-colors"
        :class="resizing === 'detail' ? 'bg-accent' : 'bg-transparent hover:bg-accent-soft'"
        title="拖拽调整宽度"
        @mousedown="startResize('detail', $event)"
      />
      <aside
        class="flex shrink-0 flex-col overflow-hidden bg-surface"
        :style="{ width: `${ui.detailWidth}px` }"
      >
        <template v-if="route.name === 'installed'">
          <InstalledDetail v-if="app.selectedInstalled" />
          <div v-else class="flex h-full flex-col items-center justify-center gap-2 px-6 text-center">
            <PackageOpen :size="28" :stroke-width="1.25" class="text-fg-4" />
            <p class="text-[13px] text-fg-3">{{ t("detail.placeholder") }}</p>
          </div>
        </template>
        <template v-else>
          <PackageDetail v-if="app.selectedSoftware" />
          <div v-else class="flex h-full flex-col items-center justify-center gap-2 px-6 text-center">
            <PackageOpen :size="28" :stroke-width="1.25" class="text-fg-4" />
            <p class="text-[13px] text-fg-3">{{ t("detail.noSoftware") }}</p>
          </div>
        </template>
      </aside>
      </template>
    </div>

    <!-- 底部状态栏：分栏展示 -->
    <footer class="flex h-7 shrink-0 items-stretch border-t border-line bg-sidebar text-[11px] text-fg-3">
      <div class="flex items-center gap-1.5 border-r border-line px-3">
        <span
          class="size-1.5 rounded-full"
          :class="app.scanning || app.importing || app.libraryLoading || app.installedLoading ? 'bg-warning' : 'bg-success'"
        />
        <span v-if="app.importing">{{ t("app.status.importing") }}</span>
        <span v-else-if="app.scanning">{{ t("app.status.scanningProgress", { current: app.progress?.current ?? 0, total: app.progress?.total ?? 0 }) }}</span>
        <span v-else-if="app.libraryLoading || app.installedLoading">{{ t("app.status.loading") }}</span>
        <span v-else>{{ t("app.status.ready") }}</span>
      </div>
      <div class="flex items-center gap-1 border-r border-line px-3">
        <span class="tabular">{{ app.library.stats.software_count }}</span>
        <span>{{ t("app.status.software") }}</span>
      </div>
      <div class="flex items-center gap-1 border-r border-line px-3">
        <span class="tabular">{{ app.library.stats.package_count }}</span>
        <span>{{ t("app.status.packages") }}</span>
      </div>
      <div class="flex items-center border-r border-line px-3">
        <span class="tabular">{{ app.library.stats.total_size ? formatSize(app.library.stats.total_size) : "0 B" }}</span>
      </div>
      <div class="ml-auto flex items-center gap-1.5 border-l border-line px-3">
        <Check :size="11" :stroke-width="2" class="text-success" />
        <span>{{ app.status?.platform ?? t("app.status.unknownPlatform") }}</span>
        <span class="text-fg-4">·</span>
        <span class="tabular">v{{ app.status?.app_version ?? "0.1.0" }}</span>
      </div>
    </footer>

    <Modal :open="ui.settingsOpen" :title="t('settings.title')" :width="680" @close="ui.settingsOpen = false">
      <SettingsView />
    </Modal>
    <SetupGuide v-if="app.initialized && app.status && !app.status.configured" />
    <ToastHost />

    <!-- 拖入文件时的提示遮罩 -->
    <div v-if="dragHover" class="pointer-events-none absolute inset-0 z-50 flex items-center justify-center bg-overlay/50">
      <div class="anim-pop flex flex-col items-center gap-2 rounded-xl border-2 border-dashed border-accent bg-surface px-10 py-7 shadow-[var(--shadow-pop)]">
        <PackageOpen :size="30" :stroke-width="1.5" class="text-accent" />
        <p class="text-[14px] font-medium">{{ t("app.drop.title") }}</p>
        <p class="text-[12px] text-fg-3">{{ t("app.drop.hint") }}</p>
      </div>
    </div>
  </div>
</template>
