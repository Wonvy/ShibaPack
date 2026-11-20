// 前端与后端之间唯一的通道。
//
// 在桌面应用里走 Tauri 的 invoke；直接用浏览器打开开发页面时没有后端，
// 改用 mock.ts 里的内存数据，这样界面可以脱离桌面窗口预览。

import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import type {
  AiProgress,
  AiSettings,
  Assignment,
  BackupInfo,
  Candidate,
  Category,
  CheckProgress,
  CheckResult,
  DownloadProgress,
  FillResult,
  ImportHint,
  ImportResult,
  InstalledApp,
  LangPack,
  Library,
  Progress,
  Release,
  Settings,
  SoftwareEdit,
  SourceKind,
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
  listInstalled: () => call<InstalledApp[]>("list_installed"),
  measureInstalled: () => call<Record<string, number>>("measure_installed"),
  matchInstalled: () => call<Record<string, string>>("match_installed"),
  uninstallInstalled: (id: string) => call<void>("uninstall_installed", { id }),
  revealInstalled: (id: string) => call<void>("reveal_installed", { id }),
  fillIcons: () => call<FillResult>("fill_icons"),
  /** 联网找图标（主页的网站图标或 GitHub 头像）。返回图标的相对路径，没找到时是 null。 */
  fetchIcon: (id: string) => call<string | null>("fetch_icon", { id }),
  /** 按名字取简介（需要时顺带翻译）。`refresh` 为真时不管有没有都重新取。 */
  fetchSummary: (name: string, refresh = false) => call<Library>("fetch_summary", { name, refresh }),
  selftestReport: (line: string, done = false) => call<void>("selftest_report", { line, done }),
  sourceLookup: (kind: SourceKind, reference: string) => call<Release>("source_lookup", { kind, reference }),
  sourceSearch: (query: string) => call<Candidate[]>("source_search", { query }),
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
  /** 存 API Key（只存在这台电脑的系统钥匙串里）；`key` 为空时删掉。 */
  aiSetKey: (baseUrl: string, key: string) => call<void>("ai_set_key", { baseUrl, key }),
  aiModels: (settings: AiSettings, key?: string) => call<string[]>("ai_models", { settings, key: key || null }),
  /** 和模型说一句话，返回它的回答。 */
  aiTest: (settings: AiSettings, key?: string) => call<string>("ai_test", { settings, key: key || null }),
  /** 让 AI 写简介并存进索引（按名字存）。`publisher` 给只装在本机的软件用。 */
  aiDescribe: (name: string, publisher?: string) => call<Library>("ai_describe", { name, publisher: publisher || null }),
  /** 用 AI 推荐分类，只读。进度见 `onAiProgress`。 */
  aiSuggestCategories: (softwareIds: string[]) => call<Suggestion[]>("ai_suggest_categories", { softwareIds }),
  /** 停下正在进行的 AI 分类。 */
  cancelAi: () => call<void>("cancel_ai"),
};

/** 把磁盘上的图标文件变成界面里 <img> 能用的地址。 */
export function assetUrl(path: string): string {
  return isTauri ? convertFileSrc(path) : path;
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

export const windowControls = {
  minimize: async () => (await currentWindow()).minimize(),
  toggleMaximize: async () => (await currentWindow()).toggleMaximize(),
  close: async () => (await currentWindow()).close(),
};
