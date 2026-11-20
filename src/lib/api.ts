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
  SystemOverview,
  DiskLocation,
  DiskNode,
  DiskProgress,
  DiskSummary,
  LargeFile,
  SystemSample,
  Library,
  PathBackup,
  PathState,
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
  Trace,
  TraceProgress,
  TraceStatus,
  TraceSummary,
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
  /** 卸载前先分析：软件的数据、快捷方式、设置在哪、多大（不含软件本身）。只读。 */
  analyzeUninstall: (id: string) => call<LeftoverScan>("analyze_uninstall", { id }),
  uninstallInstalled: (id: string) => call<void>("uninstall_installed", { id }),
  /** 卸载程序还没跑完（软件还在）。 */
  uninstallStatus: (id: string) => call<boolean>("uninstall_status", { id }),
  /** 找刚卸载的软件的残留。 */
  scanLeftovers: (id: string) => call<LeftoverScan>("scan_leftovers", { id }),
  /** 清理残留：`indices` 是上次找到的列表里的序号；`elevated` 为真时以管理员身份（Windows）。 */
  removeLeftovers: (id: string, indices: number[], elevated = false) => call<RemoveReport>("remove_leftovers", { id, indices, elevated }),
  /** 在文件管理器里显示分析或查找到的某一项。 */
  revealLeftover: (id: string, index: number) => call<void>("reveal_leftover", { id, index }),
  /** 打开清理时备份注册表的文件夹。 */
  openUninstallBackup: (path: string) => call<void>("open_uninstall_backup", { path }),
  revealInstalled: (id: string) => call<void>("reveal_installed", { id }),
  /** 主页：电脑名、系统、处理器、内存总量。 */
  systemOverview: () => call<SystemOverview>("system_overview"),
  /** 主页：此刻的 CPU、内存、系统盘和所有进程。 */
  systemSample: () => call<SystemSample>("system_sample"),
  /** 打开或关掉毛玻璃。系统不支持时返回 false。 */
  setWindowGlass: (enabled: boolean) => call<boolean>("set_window_glass", { enabled }),
  /** 磁盘分析：可以分析的地方（用户文件夹和每一块盘）。 */
  diskLocations: () => call<DiskLocation[]>("disk_locations"),
  /** 扫一个文件夹或一块盘（只读）。进度见 `onDiskProgress`；停下了返回 `cancelled`。 */
  diskScan: (root: string) => call<DiskSummary>("disk_scan", { root }),
  diskCancel: () => call<void>("disk_cancel"),
  /** 扫描中的样子（没在扫时是 null）。`path` 只能是根或第一层的文件夹；没扫完的项带 `pending`。 */
  diskLiveView: (path: string[], minRatio: number) => call<DiskNode | null>("disk_live_view", { path, minRatio }),
  /** 扫描中到现在为止最大的文件。 */
  diskLiveLargest: () => call<LargeFile[]>("disk_live_largest"),
  /** `path` 那个文件夹（连同下面所有层）里最大的 `limit` 个文件，从大到小。小于 1 MB 的不列。 */
  diskLargest: (path: string[], limit: number) => call<LargeFile[]>("disk_largest", { path, limit }),
  /** 安装监控：先拍基线（进度见 `onTraceProgress`），拍完运行安装包。取消了返回 `cancelled`。 */
  traceStart: (ref: FileRef, deep: boolean) => call<void>("trace_start", { ...ref, deep }),
  /** 正在监控的安装，没有时是 null。 */
  traceStatus: () => call<TraceStatus | null>("trace_status"),
  /** 装完了：再拍一次、对比、存下记录。 */
  traceFinish: () => call<Trace>("trace_finish"),
  traceCancel: () => call<void>("trace_cancel"),
  /** 重新打开安装时的浮动小窗（开始监控时后端自己会开，结束时自己会关）。 */
  traceMini: () => call<void>("trace_mini"),
  /** 小窗上点了按钮：`finish` 完成监控，`open` 回到主窗口。主窗口会收到 `onTraceMiniAction`。 */
  traceMiniAction: (action: "finish" | "open") => call<void>("trace_mini_action", { action }),
  listTraces: (softwareId: string | null) => call<TraceSummary[]>("list_traces", { softwareId }),
  getTrace: (softwareId: string, id: string) => call<Trace>("get_trace", { softwareId, id }),
  deleteTrace: (softwareId: string, id: string) => call<void>("delete_trace", { softwareId, id }),
  /** 在文件管理器里定位记录里的第 `index` 项。 */
  traceReveal: (softwareId: string, id: string, index: number) => call<void>("trace_reveal", { softwareId, id, index }),
  /** 从扫完的结果里取一块：相对扫描根的路径段，往下几层，占比太小的合成“其他”。 */
  diskView: (path: string[], depth: number, minRatio: number) => call<DiskNode>("disk_view", { path, depth, minRatio }),
  /** 在文件管理器里显示；`open` 为真时用默认程序打开。 */
  diskReveal: (path: string[], open: boolean) => call<void>("disk_reveal", { path, open }),
  diskFullPath: (path: string[]) => call<string>("disk_full_path", { path }),
  /** 移到回收站，返回腾出的大小。受保护的返回 `disk_protected:原因`。 */
  diskTrash: (path: string[]) => call<number>("disk_trash", { path }),
  /** 磁盘分析里文件夹是做什么的：`cached` 只看缓存，`ask` 没记下的问 AI。返回 路径段用 / 连起来 → 解释。 */
  aiExplainFolders: (paths: string[][], mode: "cached" | "ask") => call<Record<string, string>>("ai_explain_folders", { paths, mode }),
  /** 进程的图标（按可执行文件）：取不到的不在结果里。 */
  processIcons: (exes: string[]) => call<Record<string, string>>("process_icons", { exes }),
  /** 结束进程：`force` 为假时请它正常退出。 */
  endProcess: (pid: number, force: boolean) => call<void>("end_process", { pid, force }),
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
  /** 收藏路径（可以带 `~`、`%APPDATA%`、`$HOME` 这类变量；绝对路径会自动换成变量的写法）。已收藏过的跳过。 */
  pathsAdd: (paths: string[]) => call<{ library: Library; added: string[] }>("paths_add", { paths }),
  pathEdit: (id: string, edit: { path?: string; note?: string }) => call<Library>("path_edit", { id, ...edit }),
  pathsRemove: (ids: string[], deleteBackups: boolean) => call<Library>("paths_remove", { ids, deleteBackups }),
  pathStates: () => call<Record<string, PathState>>("path_states"),
  pathBackup: (id: string) => call<PathBackup>("path_backup", { id }),
  pathBackups: (id: string) => call<PathBackup[]>("path_backups", { id }),
  /** 用一份备份替换现在的；现在的先自动备份一份再送进回收站。返回那份自动备份。 */
  pathRestore: (id: string, name: string) => call<PathBackup | null>("path_restore", { id, name }),
  pathDeleteBackup: (id: string, name: string) => call<PathBackup[]>("path_delete_backup", { id, name }),
  pathReveal: (id: string, open: boolean) => call<void>("path_reveal", { id, open }),
  pathRevealBackup: (id: string, name: string) => call<void>("path_reveal_backup", { id, name }),
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
  /** 问访达废纸篓里有几项（macOS）。第一次会弹授权；允许了以后扫描时自动问。返回更新后的全部项。 */
  cleanupCountTrash: () => call<CleanItem[]>("cleanup_count_trash"),
  /** 打开要在别处清的那一项：系统的存储设置，或者那个应用。 */
  cleanupOpenTool: (id: string) => call<void>("cleanup_open_tool", { id }),
  /** 停下正在进行的扫描：已经量完的照样作为结果。 */
  cleanupCancelScan: () => call<void>("cleanup_cancel_scan"),
  /** 只重新量这几项，返回更新后的全部项。 */
  cleanupRescan: (ids: string[]) => call<CleanItem[]>("cleanup_rescan", { ids }),
  /** 让 AI 解释清理项：这些文件是什么、删了会怎样。`mode` 和解释启动项一样。 */
  aiExplainCleanup: (ids: string[], mode: "cached" | "auto" | "refresh") => call<Explanations>("ai_explain_cleanup", { ids, mode }),
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

/** 选要收藏的文件或文件夹。`directory` 为真时选文件夹（可以多选）。 */
export async function pickPaths(directory: boolean): Promise<string[]> {
  if (!isTauri) return [directory ? "/Users/demo/Library/Application Support/Code/User" : "/Users/demo/.zshrc"];
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({ multiple: true, directory });
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

/** 安装监控拍快照的进度。 */
export async function onTraceProgress(handler: (progress: TraceProgress) => void): Promise<() => void> {
  if (!isTauri) return (await import("./mockTrace")).mockOnTrace(handler);
  const { listen } = await import("@tauri-apps/api/event");
  return listen<TraceProgress>("trace://progress", (event) => handler(event.payload));
}

/** 浮动小窗上点了“完成”或“打开”。 */
export async function onTraceMiniAction(handler: (action: "finish" | "open") => void): Promise<() => void> {
  if (!isTauri) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen<"finish" | "open">("trace://mini-action", (event) => handler(event.payload));
}

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

/** 扫描进度：量完几处、一共几处。 */
export async function onCleanupScan(handler: (progress: CleanProgress) => void): Promise<() => void> {
  if (!isTauri) return (await import("./mockCleanup")).mockOnCleanupScan(handler);
  const { listen } = await import("@tauri-apps/api/event");
  return listen<CleanProgress>("cleanup://scan", (event) => handler(event.payload));
}

/** 清理进度。 */
export async function onDiskProgress(handler: (progress: DiskProgress) => void): Promise<() => void> {
  if (!isTauri) return (await import("./mockDisk")).mockOnDiskProgress(handler);
  const { listen } = await import("@tauri-apps/api/event");
  return listen<DiskProgress>("disk://progress", (event) => handler(event.payload));
}

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
