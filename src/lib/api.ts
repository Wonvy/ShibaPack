// 前端与后端之间唯一的通道。
//
// 在桌面应用里走 Tauri 的 invoke；直接用浏览器打开开发页面时没有后端，
// 改用 mock.ts 里的内存数据，这样界面可以脱离桌面窗口预览。

import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import type {
  AiProgress,
  CleanItem,
  CleanOptions,
  CleanPick,
  CleanProgress,
  CleanReport,
  CleanRun,
  CookieSite,
  AiSuggestions,
  AiSettings,
  Assignment,
  BackupInfo,
  Candidate,
  Category,
  CheckProgress,
  CheckResult,
  DescribeResult,
  DownloadProgress,
  Explanations,
  FailureEntry,
  FillResult,
  DropEntry,
  ImportCategoryPreview,
  ImportCategoryTarget,
  ImportHint,
  ImportResult,
  InstalledApp,
  LangPack,
  LeftoverScan,
  Library,
  Progress,
  Release,
  RemoveReport,
  RepoHit,
  Settings,
  SoftwareEdit,
  SourceKind,
  StartupAction,
  StartupListing,
  Stats,
  Status,
  Storage,
  SubscribeResult,
  Suggestion,
  VerifyReport,
  VersionEdit,
} from "./types";

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const ua = typeof navigator === "undefined" ? "" : navigator.userAgent;
/** 当前这台电脑的系统，用来判断一个安装包能不能在本机运行。 */
export const hostPlatform = /Windows/i.test(ua) ? "windows" : /Mac/i.test(ua) ? "macos" : "linux";
export const isMac = hostPlatform === "macos";

async function call<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  if (isTauri) return invoke<T>(command, args);
  const { mockCall } = await import("./mock");
  return mockCall(command, args) as Promise<T>;
}

type FileRef = { softwareId: string; versionId: string; fileId: string };

export const api = {
  getStatus: () => call<Status>("get_status"),
  setRepoRoot: (path: string) => call<Status>("set_repo_root", { path }),
  getLibrary: () => call<Library>("get_library"),
  /** 多选收藏或取消收藏，只存一次索引。 */
  setStarredMany: (ids: string[], starred: boolean) => call<Library>("set_starred_many", { ids, starred }),
  /** 把多个软件移到一个分类（null 是未分类），只存一次索引。 */
  moveSoftware: (ids: string[], categoryId: string | null) => call<Library>("move_software", { ids, categoryId }),
  /** 索引被别处改过时返回新的索引，没改过返回 null。 */
  refreshLibrary: () => call<Library | null>("refresh_library"),
  /** `moveFiles` 只对托管模式有效：导入后不留原文件。 */
  importPaths: (paths: string[], mode: Storage, hint?: ImportHint, moveFiles = false) =>
    call<ImportResult>("import_paths", { paths, mode, moveFiles, hint: hint ?? null }),
  /** 在两个文件之间停下正在进行的导入。 */
  cancelImport: () => call<void>("cancel_import"),
  updateSoftware: (id: string, edit: SoftwareEdit) => call<Library>("update_software", { id, edit }),
  updateVersion: (softwareId: string, versionId: string, edit: VersionEdit) =>
    call<Library>("update_version", { softwareId, versionId, edit }),
  deleteSoftware: (id: string, deleteFiles: boolean) => call<Library>("delete_software", { id, deleteFiles }),
  /** 多选删除，只存一次索引。 */
  deleteSoftwareMany: (ids: string[], deleteFiles: boolean) => call<Library>("delete_software_many", { ids, deleteFiles }),
  /** 清理旧版本：一次删掉多个版本。 */
  deleteVersions: (targets: { softwareId: string; versionId: string }[], deleteFiles: boolean) =>
    call<Library>("delete_versions", { targets, deleteFiles }),
  deleteVersion: (softwareId: string, versionId: string, deleteFiles: boolean) =>
    call<Library>("delete_version", { softwareId, versionId, deleteFiles }),
  deleteFile: (ref: FileRef, deleteFromDisk: boolean) => call<Library>("delete_file", { ...ref, deleteFromDisk }),
  runFile: (ref: FileRef) => call<void>("run_file", ref),
  revealFile: (ref: FileRef) => call<void>("reveal_file", ref),
  openRepoFolder: () => call<void>("open_repo_folder"),
  setCategories: (categories: Category[]) => call<Library>("set_categories", { categories }),
  setStarred: (softwareId: string, starred: boolean) => call<Library>("set_starred", { softwareId, starred }),
  suggestCategories: (softwareIds: string[]) => call<Suggestion[]>("suggest_categories", { softwareIds }),
  applyClassification: (assignments: Assignment[]) => call<Library>("apply_classification", { assignments }),
  setSettings: (settings: Settings) => call<Library>("set_settings", { settings }),
  getStats: () => call<Stats>("get_stats"),
  listBackups: () => call<BackupInfo[]>("list_backups"),
  createBackup: () => call<BackupInfo[]>("create_backup"),
  restoreBackup: (name: string) => call<Library>("restore_backup", { name }),
  verifyData: () => call<VerifyReport>("verify_data"),
  exportBackup: (dest: string) => call<void>("export_backup", { dest }),
  importBackup: (src: string) => call<Library>("import_backup", { src }),
  emptyTrash: () => call<Stats>("empty_trash"),
  listInstalled: () => call<{ apps: InstalledApp[]; cachedIcons: Library["installedIcons"] | null }>("list_installed"),
  measureInstalled: () => call<Record<string, number>>("measure_installed"),
  matchInstalled: () => call<Record<string, string>>("match_installed"),
  uninstallInstalled: (id: string) => call<void>("uninstall_installed", { id }),
  /** 卸载程序还没跑完（软件还在）。 */
  uninstallStatus: (id: string) => call<boolean>("uninstall_status", { id }),
  /** 找刚卸载的软件的残留。 */
  scanLeftovers: (id: string) => call<LeftoverScan>("scan_leftovers", { id }),
  /** 清理残留：`indices` 是上次找到的列表里的序号；`elevated` 为真时以管理员身份（Windows）。 */
  removeLeftovers: (id: string, indices: number[], elevated = false) => call<RemoveReport>("remove_leftovers", { id, indices, elevated }),
  /** 打开清理时备份注册表的文件夹。 */
  openUninstallBackup: (path: string) => call<void>("open_uninstall_backup", { path }),
  revealInstalled: (id: string) => call<void>("reveal_installed", { id }),
  /** 列出启动项和服务。 */
  listStartup: () => call<StartupListing>("list_startup"),
  /** 启用、停用、启动、停止、重启，或改服务的启动类型。返回改完后重新列出来的结果。 */
  startupAction: (id: string, action: StartupAction) => call<StartupListing>("startup_action", { id, action }),
  revealStartup: (id: string) => call<void>("reveal_startup", { id }),
  /**
   * 打开系统自己的设置页：`loginItems` 启动项，macOS 上允许控制“系统事件”的 `automation`，
   * 允许没有签名的应用更新自己的 `appManagement`（App 管理）。
   */
  openStartupSettings: (page: "loginItems" | "automation" | "appManagement" | "fullDiskAccess" | "storage") =>
    call<void>("open_startup_settings", { page }),
  /** 只读地找一遍能清理的东西。每量完一项通过 `onCleanupItem` 先送来。 */
  cleanupScan: () => call<CleanItem[]>("cleanup_scan"),
  /** 清理勾选的项（id 和不要的条目序号）。进度见 `onCleanupProgress`。 */
  cleanupRun: (picks: CleanPick[]) => call<CleanReport>("cleanup_run", { picks }),
  /** 让这一项要先关掉的程序正常退出。返回更新后的这一项。 */
  cleanupQuitApp: (id: string) => call<CleanItem>("cleanup_quit_app", { id }),
  cleanupReveal: (id: string, index: number | null) => call<void>("cleanup_reveal", { id, index }),
  cleanupOptions: () => call<CleanOptions>("cleanup_options"),
  /** 保存自定义的清理文件夹和排除的文件夹。受保护的文件夹返回 `cleanup_protected:路径` 这样的错误。 */
  cleanupSetOptions: (options: CleanOptions) => call<CleanOptions>("cleanup_set_options", { options }),
  cleanupHistory: () => call<CleanRun[]>("cleanup_history"),
  /** 浏览器里有 Cookie 的网站（按主域名合并，多的在前）。 */
  cleanupCookieSites: () => call<CookieSite[]>("cleanup_cookie_sites"),
  /** 记一条失败日志。 */
  logFailure: (area: string, message: string, detail?: string) => call<void>("log_failure", { area, message, detail: detail ?? null }),
  /** 最近的失败日志，新的在前。 */
  readFailureLog: (limit = 500) => call<FailureEntry[]>("read_failure_log", { limit }),
  clearFailureLog: () => call<void>("clear_failure_log"),
  openLogFolder: () => call<void>("open_log_folder"),
  /** 自己更新时用 latest.json 里的哪一项（Windows 按当初怎么装的来），其他系统是 null。 */
  updaterTarget: () => call<string | null>("updater_target"),
  /** 把这个版本的安装包下载到“下载”文件夹并打开，返回文件路径。 */
  downloadInstaller: (version: string) => call<string>("download_installer", { version }),
  /**
   * 让 AI 解释一批启动项是做什么的、能不能停用：没解释过的凑成一批问，不是一项一项问。解释记在仓库的缓存里：
   * `cached` 只看缓存、不问 AI；`auto` 有就用、没有才问；`refresh` 全部重新问。进度见 `onAiProgress`。
   */
  aiExplainStartup: (ids: string[], mode: "cached" | "auto" | "refresh") => call<Explanations>("ai_explain_startup_many", { ids, mode }),
  /** 让 AI 一批写简介（每次请求放 15 个），写好的一次存进索引。进度见 `onAiProgress`。 */
  aiDescribeMany: (items: { name: string; publisher?: string }[]) => call<DescribeResult>("ai_describe_many", { items }),
  fillIcons: () => call<FillResult>("fill_icons"),
  fillFileInfo: () => call<FillResult>("fill_file_info"),
  inspectDrop: (paths: string[], includeChildren = false) => call<DropEntry[]>("inspect_drop", { paths, includeChildren }),
  previewImportCategories: (targets: ImportCategoryTarget[]) => call<ImportCategoryPreview[]>("preview_import_categories", { targets }),
  /** 联网找图标。返回图标的相对路径和缓存内容，没找到时是 null。 */
  fetchIcon: (id: string) => call<{ icon: string; iconData: string | null } | null>("fetch_icon", { id }),
  /** 按名字取简介（需要时顺带翻译）。`refresh` 为真时不管有没有都重新取。 */
  fetchSummary: (name: string, refresh = false) => call<Library>("fetch_summary", { name, refresh }),
  selftestReport: (line: string, done = false) => call<void>("selftest_report", { line, done }),
  sourceLookup: (kind: SourceKind, reference: string) => call<Release>("source_lookup", { kind, reference }),
  sourceSearch: (query: string) => call<Candidate[]>("source_search", { query }),
  /** ShibaPack 某个版本的更新说明（GitHub Release 的说明原文）。 */
  releaseNotes: (version: string) => call<string>("release_notes", { version }),
  /** 在 GitHub 上搜仓库（不登录每分钟只能搜 10 次）。 */
  githubSearch: (query: string) => call<RepoHit[]>("github_search", { query }),
  subscribe: (softwareId: string | null, name: string | null, kind: SourceKind, reference: string) =>
    call<SubscribeResult>("subscribe", { softwareId, name, kind, reference }),
  unsubscribe: (softwareId: string) => call<Library>("unsubscribe", { softwareId }),
  checkUpdates: () => call<CheckResult>("check_updates"),
  downloadAsset: (softwareId: string, url: string) => call<Library>("download_asset", { softwareId, url }),
  cancelDownload: (url: string) => call<void>("cancel_download", { url }),
  tidyRoot: () => call<ImportResult>("tidy_root"),
  listLangPacks: () => call<LangPack[]>("list_lang_packs"),
  importLangPack: (src: string) => call<LangPack[]>("import_lang_pack", { src }),
  removeLangPack: (locale: string) => call<LangPack[]>("remove_lang_pack", { locale }),
  exportLangPack: (dest: string, pack: LangPack) => call<void>("export_lang_pack", { dest, pack }),
  /** 用系统的浏览器打开一个网址。 */
  openUrl: (url: string) => call<void>("open_url", { url }),

  // AI 模型。`key` 是设置对话框里刚填、还没存的密钥；不给时用这台电脑钥匙串里存的
  /** 这台电脑的钥匙串里有没有这个地址的 API Key。 */
  aiKeySaved: (baseUrl: string) => call<boolean>("ai_key_saved", { baseUrl }),
  /** 已存密钥的遮蔽预览。前端只收到可显示的部分。 */
  aiKeyPreview: (baseUrl: string) => call<string | null>("ai_key_preview", { baseUrl }),
  /** 存 API Key（只存在这台电脑的系统钥匙串里）；`key` 为空时删掉。 */
  aiSetKey: (baseUrl: string, key: string) => call<void>("ai_set_key", { baseUrl, key }),
  aiModels: (settings: AiSettings, key?: string) => call<string[]>("ai_models", { settings, key: key || null }),
  /** 和模型说一句话，返回它的回答。 */
  aiTest: (settings: AiSettings, key?: string) => call<string>("ai_test", { settings, key: key || null }),
  /** 让 AI 写简介并存进索引（按名字存）。`publisher` 给只装在本机的软件用。 */
  aiDescribe: (name: string, publisher?: string) => call<Library>("ai_describe", { name, publisher: publisher || null }),
  /** 用 AI 推荐分类，只读。软件和分类都没变的直接用上次的建议，`refresh` 为真时全部重新问。进度见 `onAiProgress`。 */
  aiSuggestCategories: (softwareIds: string[], refresh = false) => call<AiSuggestions>("ai_suggest_categories", { softwareIds, refresh }),
  /** 自己写的简介（按名字存）；空字符串表示删掉。 */
  setSummary: (name: string, text: string) => call<Library>("set_summary", { name, text }),
  /** 停下正在进行的 AI 分类。 */
  cancelAi: () => call<void>("cancel_ai"),
};

export function isImageDataUrl(value: string | null | undefined): value is string {
  return typeof value === "string" && /^data:image\/(?:png|svg\+xml|jpeg|gif|webp);base64,/.test(value);
}

/** 把磁盘上的图标文件变成界面里 <img> 能用的地址，已内嵌的图标直接使用。 */
export function assetUrl(path: string): string {
  return isTauri && !isImageDataUrl(path) ? convertFileSrc(path) : path;
}

// ── 系统对话框 ───────────────────────────────────────────────

export async function pickFiles(): Promise<string[]> {
  if (!isTauri) return (await import("./mock")).mockPickFiles();
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({ multiple: true, directory: false });
  return picked ? (Array.isArray(picked) ? picked : [picked]) : [];
}

export async function pickFolder(): Promise<string | null> {
  if (!isTauri) return "/Users/demo/Installers";
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({ multiple: false, directory: true });
  return typeof picked === "string" ? picked : null;
}

export async function pickJsonFile(): Promise<string | null> {
  if (!isTauri) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({ multiple: false, filters: [{ name: "JSON", extensions: ["json"] }] });
  return typeof picked === "string" ? picked : null;
}

export async function pickSavePath(defaultName: string): Promise<string | null> {
  if (!isTauri) return null;
  const { save } = await import("@tauri-apps/plugin-dialog");
  return save({ defaultPath: defaultName, filters: [{ name: "JSON", extensions: ["json"] }] });
}

// ── 事件 ─────────────────────────────────────────────────────

/** 导入进度。返回取消监听的函数。 */
export async function onImportProgress(handler: (progress: Progress) => void): Promise<() => void> {
  if (!isTauri) return (await import("./mock")).mockOnProgress(handler);
  const { listen } = await import("@tauri-apps/api/event");
  return listen<Progress>("import://progress", (event) => handler(event.payload));
}

/** 下载进度。 */
export async function onDownloadProgress(handler: (progress: DownloadProgress) => void): Promise<() => void> {
  if (!isTauri) return (await import("./mock")).mockOnDownload(handler);
  const { listen } = await import("@tauri-apps/api/event");
  return listen<DownloadProgress>("download://progress", (event) => handler(event.payload));
}

/** 检查更新的进度。`total` 为 0 表示检查结束。 */
export async function onCheckProgress(handler: (progress: CheckProgress) => void): Promise<() => void> {
  if (!isTauri) return (await import("./mock")).mockOnCheck(handler);
  const { listen } = await import("@tauri-apps/api/event");
  return listen<CheckProgress>("sources://progress", (event) => handler(event.payload));
}

/** AI 分类的进度。 */
export async function onAiProgress(handler: (progress: AiProgress) => void): Promise<() => void> {
  if (!isTauri) return (await import("./mock")).mockOnAi(handler);
  const { listen } = await import("@tauri-apps/api/event");
  return listen<AiProgress>("ai://progress", (event) => handler(event.payload));
}

/** 清理扫描时每量完一项送来一次。 */
export async function onCleanupItem(handler: (item: CleanItem) => void): Promise<() => void> {
  if (!isTauri) return (await import("./mockCleanup")).mockOnCleanupItem(handler);
  const { listen } = await import("@tauri-apps/api/event");
  return listen<CleanItem>("cleanup://item", (event) => handler(event.payload));
}

/** 清理进度。 */
export async function onCleanupProgress(handler: (progress: CleanProgress) => void): Promise<() => void> {
  if (!isTauri) return (await import("./mockCleanup")).mockOnCleanupProgress(handler);
  const { listen } = await import("@tauri-apps/api/event");
  return listen<CleanProgress>("cleanup://progress", (event) => handler(event.payload));
}

/** 把文件拖进窗口。`onHover` 在文件悬在窗口上方和离开时调用。 */
/** 拖放时指针在网页里的位置（CSS 像素）。 */
export type DropPoint = { x: number; y: number };

/**
 * 从系统拖文件进窗口。`onHover` 在指针移动时反复调用，带着指针位置，
 * 好让界面判断文件正停在哪个软件上；离开窗口时 `over` 为 false。
 */
export async function onFileDrop(
  onDrop: (paths: string[], point: DropPoint) => void,
  onHover: (over: boolean, point: DropPoint | null) => void,
): Promise<() => void> {
  if (!isTauri) return () => {};
  const { getCurrentWebview } = await import("@tauri-apps/api/webview");
  // 事件里给的是物理像素，换成网页里的 CSS 像素
  const toPoint = (position: { x: number; y: number }): DropPoint => ({
    x: position.x / window.devicePixelRatio,
    y: position.y / window.devicePixelRatio,
  });
  return getCurrentWebview().onDragDropEvent((event) => {
    const payload = event.payload;
    if (payload.type === "enter" || payload.type === "over") onHover(true, toPoint(payload.position));
    else if (payload.type === "leave") onHover(false, null);
    else if (payload.type === "drop") {
      onHover(false, null);
      if (payload.paths.length > 0) onDrop(payload.paths, toPoint(payload.position));
    }
  });
}

// ── 窗口按钮（无边框窗口用）─────────────────────────────────

async function currentWindow() {
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow();
}

type DetailWindowResize = { open: boolean; detailWidth: number; before: number; after: number };
let detailResize = Promise.resolve<DetailWindowResize | null>(null);

export const windowControls = {
  minimize: async () => (await currentWindow()).minimize(),
  toggleMaximize: async () => (await currentWindow()).toggleMaximize(),
  close: async () => (await currentWindow()).close(),
  resizeForDetail(open: boolean, detailWidth: number, dockMinWidth: number) {
    if (!isTauri) return;
    // 快速开关时，按点击顺序完成窗口调整，再以实际的窗口宽度计算下一步。
    detailResize = detailResize.then((previous) =>
      call<DetailWindowResize | null>("resize_detail_window", { open, detailWidth, dockMinWidth, previous }),
    ).catch(() => null);
    return detailResize;
  },
};
