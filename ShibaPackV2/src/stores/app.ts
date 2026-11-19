import { listen } from "@tauri-apps/api/event";
import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { api } from "../lib/api";
import { setLocale, t } from "../i18n";
import { compareVersions } from "../lib/version";
import type {
  AppStatus,
  BackupInfo,
  Category,
  InstalledProgram,
  LibraryData,
  PackageInfo,
  Platform,
  ScanProgress,
  Settings,
  Software,
  SoftwareGroup,
  SoftwareSummary,
  SoftwareVersion,
  StorageInfo,
  Tag,
} from "../types";
import { useUiStore } from "./ui";

export type Scope =
  | { type: "all" }
  | { type: "category"; id: string }
  | { type: "software"; name: string }
  | { type: "platform"; platform: Platform };

export type InstalledSort = "name" | "install_date" | "publisher" | "size" | "source";
export type PackageSort = "name" | "size" | "added" | "platform";
export type InstallFilter = "all" | "uninstalled" | "installed";

export interface CategorySection {
  category: Category | null;
  groups: SoftwareGroup[];
}

/** 归一化软件名：去空格/标点/大小写，便于安装包与系统已安装程序互相匹配 */
function normalizeName(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9\u4e00-\u9fa5]/g, "");
}

const emptyLibrary = (): LibraryData => ({
  packages: [],
  categories: [],
  groups: [],
  stats: { software_count: 0, package_count: 0, total_size: 0, platform_counts: {} },
  software: [],
  tags: [],
});

const defaultSettings = (): Settings => ({
  package_root: "",
  storage_mode: "managed",
  installer_directory: "./Installers",
  theme: "system",
  accent: "#a25bff",
  language: "zh-CN",
  auto_scan: false,
  copy_on_import: true,
});

let eventsBound = false;

export const useAppStore = defineStore("app", () => {
  const ui = useUiStore();

  const status = ref<AppStatus | null>(null);
  const settings = ref<Settings>(defaultSettings());
  const library = ref<LibraryData>(emptyLibrary());
  const installed = ref<InstalledProgram[]>([]);
  const summaries = ref<Record<string, SoftwareSummary>>({});
  const summaryLoading = ref<Record<string, boolean>>({});
  const libraryLoading = ref(false);
  const installedLoading = ref(false);
  const scanning = ref(false);
  const progress = ref<ScanProgress | null>(null);
  const importing = ref(false);
  const importProgress = ref<ScanProgress | null>(null);
  const initialized = ref(false);

  const scope = ref<Scope>({ type: "all" });
  const search = ref("");
  const platformFilter = ref<Platform | "all">("all");
  const selectedPackageId = ref<string | null>(null);
  const selectedInstalledId = ref<string | null>(null);
  const installedSort = ref<InstalledSort>("install_date");
  const installedSortDesc = ref(true);
  const packageSort = ref<PackageSort>("name");
  const packageSortDesc = ref(false);
  const installFilter = ref<InstallFilter>("all");

  // ===== 新版三级结构相关状态 =====
  const tags = ref<Tag[]>([]);
  const selectedSoftwareId = ref<string | null>(null);
  const storageInfo = ref<StorageInfo | null>(null);
  const backups = ref<BackupInfo[]>([]);
  const tagFilter = ref<string[]>([]);

  /** 按名称匹配系统已安装程序（精确匹配优先，再退化为包含匹配） */
  const installedByName = computed(() => {
    const map = new Map<string, InstalledProgram>();
    for (const item of installed.value) {
      map.set(normalizeName(item.name), item);
    }
    return map;
  });

  function findInstalled(name: string): InstalledProgram | null {
    const key = normalizeName(name);
    if (!key) return null;
    const hit = installedByName.value.get(key);
    if (hit) return hit;
    // 安装包名常带 Setup/版本等后缀，允许安全的包含匹配
    for (const [installedKey, program] of installedByName.value) {
      if (installedKey.length >= 3 && (key.includes(installedKey) || installedKey.includes(key))) {
        return program;
      }
    }
    return null;
  }

  function isInstalled(name: string): boolean {
    return findInstalled(name) !== null;
  }

  const visibleInstalled = computed(() => {
    const q = search.value.trim().toLowerCase();
    const list = q
      ? installed.value.filter((item) =>
          [item.name, item.publisher, item.version, item.install_date ?? ""].some((value) =>
            value.toLowerCase().includes(q),
          ),
        )
      : [...installed.value];
    const key = installedSort.value;
    const dir = installedSortDesc.value ? -1 : 1;
    list.sort((a, b) => {
      let result = 0;
      if (key === "size") {
        result = (a.size_bytes ?? 0) - (b.size_bytes ?? 0);
      } else if (key === "install_date") {
        // 安装日期格式可能为 YYYYMMDD 或 ISO，统一成可比较的字符串
        result = (a.install_date ?? "").localeCompare(b.install_date ?? "");
      } else if (key === "publisher" || key === "source") {
        result = a[key].localeCompare(b[key], "zh-CN");
      } else {
        result = a.name.localeCompare(b.name, "zh-CN");
      }
      if (result === 0) result = a.name.localeCompare(b.name, "zh-CN");
      return result * dir;
    });
    return list;
  });

  const visiblePackages = computed(() => {
    const q = search.value.trim().toLowerCase();
    return library.value.packages.filter((pkg) => {
      if (platformFilter.value !== "all" && pkg.platform !== platformFilter.value) return false;
      if (scope.value.type === "category" && pkg.category_id !== scope.value.id) return false;
      if (scope.value.type === "software" && pkg.software_name !== scope.value.name) return false;
      if (scope.value.type === "platform" && pkg.platform !== scope.value.platform) return false;
      if (!q) return true;
      return (
        pkg.software_name.toLowerCase().includes(q) ||
        pkg.file_name.toLowerCase().includes(q) ||
        pkg.version.toLowerCase().includes(q) ||
        (pkg.publisher ?? "").toLowerCase().includes(q)
      );
    });
  });

  const visibleGroups = computed<SoftwareGroup[]>(() => {
    const map = new Map<string, PackageInfo[]>();
    for (const pkg of visiblePackages.value) {
      const list = map.get(pkg.software_name) ?? [];
      list.push(pkg);
      map.set(pkg.software_name, list);
    }
    const groups = Array.from(map.entries())
      .map(([name, packages]) => ({
        name,
        packages: packages.sort(
          (a, b) => compareVersions(b.version, a.version) || b.added_time.localeCompare(a.added_time),
        ),
        latest_version: packages[0]?.version ?? "",
        platforms: Array.from(new Set(packages.map((p) => p.platform))),
      }));

    const filtered =
      installFilter.value === "all"
        ? groups
        : groups.filter((group) => isInstalled(group.name) === (installFilter.value === "installed"));

    const key = packageSort.value;
    const dir = packageSortDesc.value ? -1 : 1;
    return filtered.sort((a, b) => {
      let result = 0;
      if (key === "size") {
        result = (a.packages[0]?.file_size ?? 0) - (b.packages[0]?.file_size ?? 0);
      } else if (key === "added") {
        result = (a.packages[0]?.added_time ?? "").localeCompare(b.packages[0]?.added_time ?? "");
      } else if (key === "platform") {
        result = (a.platforms[0] ?? "").localeCompare(b.platforms[0] ?? "");
      } else {
        result = a.name.localeCompare(b.name, "zh-CN");
      }
      if (result === 0) result = a.name.localeCompare(b.name, "zh-CN");
      return result * dir;
    });
  });

  function categoryOf(group: SoftwareGroup): Category | null {
    const id = group.packages[0]?.category_id;
    if (!id) return null;
    return library.value.categories.find((item) => item.id === id) ?? null;
  }

  /** 按分类把可见软件分组：有分类的按分类排序顺序排列，未分类放最后 */
  const categorizedGroups = computed<CategorySection[]>(() => {
    const categories = library.value.categories;
    const order = new Map(categories.map((c) => [c.id, c.sort_order]));
    const buckets = new Map<string, SoftwareGroup[]>();
    for (const group of visibleGroups.value) {
      const cid = group.packages[0]?.category_id ?? "";
      const list = buckets.get(cid) ?? [];
      list.push(group);
      buckets.set(cid, list);
    }
    const sections: CategorySection[] = Array.from(buckets.entries()).map(([cid, groups]) => ({
      category: cid ? categories.find((c) => c.id === cid) ?? null : null,
      groups,
    }));
    sections.sort((a, b) => {
      if (a.category && b.category) {
        return (order.get(a.category.id) ?? 0) - (order.get(b.category.id) ?? 0)
          || a.category.name.localeCompare(b.category.name, "zh-CN");
      }
      if (a.category) return -1;
      if (b.category) return 1;
      return 0;
    });
    return sections;
  });

  const selectedPackage = computed(
    () => visiblePackages.value.find((pkg) => pkg.id === selectedPackageId.value) ?? null,
  );

  const selectedInstalled = computed(
    () => installed.value.find((item) => item.id === selectedInstalledId.value) ?? null,
  );

  function bindEvents() {
    if (eventsBound) return;
    eventsBound = true;
    void listen<ScanProgress>("scan://progress", (event) => {
      progress.value = event.payload;
    });
    void listen<ScanProgress>("import://progress", (event) => {
      importProgress.value = event.payload;
    });
  }

  async function initialize() {
    if (initialized.value) return;
    bindEvents();
    try {
      // 阶段 1：只读取状态与设置（本地 JSON，毫秒级），让主界面框架立即渲染
      status.value = await api.status();
      settings.value = await api.settings();
      ui.theme = settings.value.theme;
      ui.accent = settings.value.accent;
      setLocale(settings.value.language);
      initialized.value = true;

      // 阶段 2：资料库与已安装软件放到后台加载，不阻塞窗口显示
      // 用 setTimeout 让浏览器/WebView 先完成一帧绘制再开始重活
      if (status.value?.configured) {
        window.setTimeout(() => {
          void refreshLibrary(false);
          void loadInstalled(false);
        }, 0);
      }
    } catch (error) {
      initialized.value = true;
      ui.toast(t("toast.initFailed", { error: String(error) }), "error");
    }
  }

  async function refreshLibrary(showError = true) {
    libraryLoading.value = true;
    try {
      library.value = await api.library();
      tags.value = library.value.tags ?? [];
      if (selectedPackageId.value && !library.value.packages.some((p) => p.id === selectedPackageId.value)) {
        selectedPackageId.value = null;
      }
      if (selectedSoftwareId.value && !library.value.software?.some((s) => s.id === selectedSoftwareId.value)) {
        selectedSoftwareId.value = null;
      }
      void loadStorageInfo();
    } catch (error) {
      if (showError) ui.toast(t("toast.libraryFailed", { error: String(error) }), "error");
    } finally {
      libraryLoading.value = false;
    }
  }

  async function loadStorageInfo() {
    try {
      storageInfo.value = await api.getStorageInfo();
    } catch {
      // 统计信息加载失败不影响主流程
    }
  }

  async function loadBackups() {
    try {
      backups.value = await api.listBackups();
    } catch {
      // ignore
    }
  }

  async function loadInstalled(showError = true) {
    installedLoading.value = true;
    try {
      installed.value = await api.installed();
    } catch (error) {
      if (showError) ui.toast(t("toast.installedFailed", { error: String(error) }), "error");
    } finally {
      installedLoading.value = false;
    }
  }

  async function uninstall(program: InstalledProgram) {
    try {
      await api.uninstallApp(program);
      if (selectedInstalledId.value === program.id) selectedInstalledId.value = null;
      await loadInstalled(false);
      ui.toast(t("toast.uninstallDone", { name: program.name }), "success");
    } catch (error) {
      ui.toast(t("toast.uninstallFailed", { error: String(error) }), "error");
    }
  }

  async function fetchSummary(name: string, force = false) {
    const key = name.trim().toLowerCase();
    if (!key || summaryLoading.value[key]) return;
    if (!force && summaries.value[key]) return;
    summaryLoading.value[key] = true;
    try {
      summaries.value[key] = await api.fetchSummary(name, force);
    } catch {
      // 摘要获取失败不影响主流程
    } finally {
      summaryLoading.value[key] = false;
    }
  }

  // 列表描述自动获取：去重 + 并发限制，避免一次性发起大量网络请求
  const summaryQueue: string[] = [];
  const summaryAttempted = new Set<string>();
  let summaryActive = 0;
  const SUMMARY_CONCURRENCY = 3;

  function pumpSummaryQueue() {
    while (summaryActive < SUMMARY_CONCURRENCY && summaryQueue.length > 0) {
      const name = summaryQueue.shift()!;
      summaryActive++;
      void fetchSummary(name).finally(() => {
        summaryActive--;
        pumpSummaryQueue();
      });
    }
  }

  function ensureSummaries(names: string[]) {
    for (const name of names) {
      const key = name.trim().toLowerCase();
      if (!key || summaryAttempted.has(key) || summaries.value[key]) continue;
      summaryAttempted.add(key);
      summaryQueue.push(name);
    }
    pumpSummaryQueue();
  }

  function summaryOf(name: string): SoftwareSummary | null {
    return summaries.value[name.trim().toLowerCase()] ?? null;
  }

  async function scan(path?: string | null) {
    if (scanning.value) return;
    scanning.value = true;
    progress.value = null;
    try {
      const result = await api.scan(path);
      await refreshLibrary(false);
      ui.toast(
        t("toast.scanDone", { added: result.added, updated: result.updated, skipped: result.skipped }),
        "success",
      );
    } catch (error) {
      ui.toast(t("toast.scanFailed", { error: String(error) }), "error");
    } finally {
      scanning.value = false;
      progress.value = null;
    }
  }

  async function addFiles(paths: string[]) {
    if (paths.length === 0) return;
    importing.value = true;
    importProgress.value = { current: 0, total: paths.length, path: "" };
    try {
      const result = await api.addPackages(paths, null);
      await refreshLibrary(false);
      ui.toast(t("toast.importDone", { count: result.added }), "success");
    } catch (error) {
      ui.toast(t("toast.importFailed", { error: String(error) }), "error");
    } finally {
      importing.value = false;
      importProgress.value = null;
    }
  }

  async function removePackage(pkg: PackageInfo, deleteFile = false) {
    try {
      await api.deletePackage(pkg.id, deleteFile);
      if (selectedPackageId.value === pkg.id) selectedPackageId.value = null;
      await refreshLibrary(false);
      ui.toast(t("toast.removed"), "success");
    } catch (error) {
      ui.toast(t("toast.deleteFailed", { error: String(error) }), "error");
    }
  }

  async function runPackage(pkg: PackageInfo) {
    try {
      await api.runPackage(pkg.id);
      ui.toast(`已启动 ${pkg.software_name}`, "success");
    } catch (error) {
      ui.toast(`启动失败：${String(error)}`, "error");
    }
  }

  async function revealPackage(pkg: PackageInfo) {
    try {
      await api.revealPackage(pkg.id);
    } catch (error) {
      ui.toast(`定位失败：${String(error)}`, "error");
    }
  }

  async function saveSettings(next: Settings) {
    try {
      settings.value = await api.saveSettings(next);
      status.value = await api.status();
      ui.theme = settings.value.theme;
      ui.accent = settings.value.accent;
      setLocale(settings.value.language);
      ui.toast(t("settings.saved"), "success");
    } catch (error) {
      ui.toast(`保存设置失败：${String(error)}`, "error");
    }
  }

  async function createCategory(name: string, color: string, icon: string, parentId: string | null = null) {
    try {
      const category = await api.createCategory(name, color, icon, parentId);
      await refreshLibrary(false);
      ui.toast(t("toast.categoryCreated", { name: category.name }), "success");
    } catch (error) {
      ui.toast(t("toast.categoryFailed", { error: String(error) }), "error");
    }
  }

  async function reorderCategories(ids: string[]) {
    try {
      await api.reorderCategories(ids);
      await refreshLibrary(false);
    } catch (error) {
      ui.toast(t("toast.categoryFailed", { error: String(error) }), "error");
    }
  }

  async function updateCategory(category: Category) {
    try {
      await api.updateCategory(category);
      await refreshLibrary(false);
    } catch (error) {
      ui.toast(`更新分类失败：${String(error)}`, "error");
    }
  }

  async function deleteCategory(id: string) {
    try {
      await api.deleteCategory(id);
      if (scope.value.type === "category" && scope.value.id === id) scope.value = { type: "all" };
      await refreshLibrary(false);
    } catch (error) {
      ui.toast(`删除分类失败：${String(error)}`, "error");
    }
  }

  async function assignCategory(packageId: string, categoryId: string | null) {
    try {
      await api.assignCategory(packageId, categoryId);
      await refreshLibrary(false);
    } catch (error) {
      ui.toast(`设置分类失败：${String(error)}`, "error");
    }
  }

  async function exportBackup(path: string) {
    try {
      await api.exportBackup(path);
      ui.toast(t("toast.backupExported"), "success");
    } catch (error) {
      ui.toast(t("toast.importFailed", { error: String(error) }), "error");
    }
  }

  async function importBackup(path: string) {
    try {
      const result = await api.importBackup(path);
      await refreshLibrary(false);
      ui.toast(t("toast.backupImported", { count: result.added }), "success");
    } catch (error) {
      ui.toast(t("toast.importFailed", { error: String(error) }), "error");
    }
  }

  // ===== 新版三级结构 actions =====

  const selectedSoftware = computed(
    () => library.value.software?.find((s) => s.id === selectedSoftwareId.value) ?? null,
  );

  function setSoftwareFilter(next: string[]) {
    tagFilter.value = next;
  }

  async function createSoftware(name: string, categoryId: string | null = null) {
    try {
      const software = await api.createSoftware(name, categoryId);
      await refreshLibrary(false);
      selectedSoftwareId.value = software.id;
      ui.toast(t("toast.softwareCreated", { name: software.name }), "success");
    } catch (error) {
      ui.toast(`创建软件失败：${String(error)}`, "error");
    }
  }

  async function updateSoftware(software: Software) {
    try {
      await api.updateSoftware(software);
      await refreshLibrary(false);
      ui.toast(t("toast.saved"), "success");
    } catch (error) {
      ui.toast(`更新软件失败：${String(error)}`, "error");
    }
  }

  async function deleteSoftware(id: string) {
    try {
      await api.deleteSoftware(id);
      if (selectedSoftwareId.value === id) selectedSoftwareId.value = null;
      await refreshLibrary(false);
      ui.toast(t("toast.removed"), "success");
    } catch (error) {
      ui.toast(t("toast.deleteFailed", { error: String(error) }), "error");
    }
  }

  async function addVersion(softwareId: string, version: SoftwareVersion) {
    try {
      await api.addVersion(softwareId, version);
      await refreshLibrary(false);
      ui.toast(t("toast.saved"), "success");
    } catch (error) {
      ui.toast(`添加版本失败：${String(error)}`, "error");
    }
  }

  async function deleteVersion(softwareId: string, versionId: string) {
    try {
      await api.deleteVersion(softwareId, versionId);
      await refreshLibrary(false);
      ui.toast(t("toast.removed"), "success");
    } catch (error) {
      ui.toast(`删除版本失败：${String(error)}`, "error");
    }
  }

  async function setDefaultVersion(softwareId: string, versionId: string) {
    try {
      await api.setDefaultVersion(softwareId, versionId);
      await refreshLibrary(false);
    } catch (error) {
      ui.toast(`设置默认版本失败：${String(error)}`, "error");
    }
  }

  async function deleteFileOp(
    softwareId: string,
    versionId: string,
    fileId: string,
    deleteFromDisk = false,
  ) {
    try {
      await api.deleteFile(softwareId, versionId, fileId, deleteFromDisk);
      await refreshLibrary(false);
      ui.toast(t("toast.removed"), "success");
    } catch (error) {
      ui.toast(`删除文件失败：${String(error)}`, "error");
    }
  }

  async function runFile(softwareId: string, fileId: string) {
    try {
      await api.runFile(softwareId, fileId);
    } catch (error) {
      ui.toast(`启动失败：${String(error)}`, "error");
    }
  }

  async function revealFile(softwareId: string, fileId: string) {
    try {
      await api.revealFile(softwareId, fileId);
    } catch (error) {
      ui.toast(`定位失败：${String(error)}`, "error");
    }
  }

  async function createTag(name: string, color: string) {
    try {
      await api.createTag(name, color);
      await refreshLibrary(false);
      ui.toast(t("toast.saved"), "success");
    } catch (error) {
      ui.toast(`创建标签失败：${String(error)}`, "error");
    }
  }

  async function updateTag(tag: Tag) {
    try {
      await api.updateTag(tag);
      await refreshLibrary(false);
    } catch (error) {
      ui.toast(`更新标签失败：${String(error)}`, "error");
    }
  }

  async function deleteTag(id: string) {
    try {
      await api.deleteTag(id);
      tagFilter.value = tagFilter.value.filter((t) => t !== id);
      await refreshLibrary(false);
      ui.toast(t("toast.removed"), "success");
    } catch (error) {
      ui.toast(`删除标签失败：${String(error)}`, "error");
    }
  }

  async function assignTags(softwareId: string, nextTags: string[]) {
    try {
      await api.assignTags(softwareId, nextTags);
      await refreshLibrary(false);
    } catch (error) {
      ui.toast(`设置标签失败：${String(error)}`, "error");
    }
  }

  async function restoreBackupOp(path: string) {
    try {
      await api.restoreBackup(path);
      await refreshLibrary(false);
      ui.toast(t("toast.backupImported", { count: 0 }), "success");
    } catch (error) {
      ui.toast(`恢复失败：${String(error)}`, "error");
    }
  }

  async function verifyData() {
    try {
      const ok = await api.verifyData();
      ui.toast(ok ? t("settings.verifyOk") : t("settings.verifyBad"), ok ? "success" : "error");
    } catch (error) {
      ui.toast(`校验失败：${String(error)}`, "error");
    }
  }

  /** 从三级结构里取软件图标（data URL），供列表/详情复用 */
  function softwareIcon(name: string): string {
    return library.value.software?.find((s) => s.name === name)?.icon ?? "";
  }

  function chooseScope(next: Scope) {
    scope.value = next;
    selectedPackageId.value = null;
    selectedSoftwareId.value = null;
  }

  return {
    status,
    settings,
    library,
    installed,
    summaries,
    summaryLoading,
    libraryLoading,
    installedLoading,
    scanning,
    progress,
    importing,
    importProgress,
    initialized,
    scope,
    search,
    platformFilter,
    selectedPackageId,
    selectedInstalledId,
    installedSort,
    installedSortDesc,
    packageSort,
    packageSortDesc,
    installFilter,
    tags,
    selectedSoftwareId,
    selectedSoftware,
    storageInfo,
    backups,
    tagFilter,
    findInstalled,
    isInstalled,
    categoryOf,
    categorizedGroups,
    visibleInstalled,
    visiblePackages,
    visibleGroups,
    selectedPackage,
    selectedInstalled,
    setSoftwareFilter,
    ensureSummaries,
    summaryOf,
    initialize,
    refreshLibrary,
    loadInstalled,
    uninstall,
    fetchSummary,
    scan,
    addFiles,
    removePackage,
    runPackage,
    revealPackage,
    saveSettings,
    createCategory,
    reorderCategories,
    updateCategory,
    deleteCategory,
    assignCategory,
    exportBackup,
    importBackup,
    loadStorageInfo,
    loadBackups,
    createSoftware,
    updateSoftware,
    deleteSoftware,
    addVersion,
    deleteVersion,
    setDefaultVersion,
    deleteFileOp,
    runFile,
    revealFile,
    createTag,
    updateTag,
    deleteTag,
    assignTags,
    restoreBackupOp,
    verifyData,
    softwareIcon,
    chooseScope,
  };
});
