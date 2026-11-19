// 前端与后端之间唯一的通道。
//
// 在桌面应用里走 Tauri 的 invoke；直接用浏览器打开开发页面时没有后端，
// 改用 mock.ts 里的内存数据，这样界面可以脱离桌面窗口预览。

import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import type {
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
  importPaths: (paths: string[], mode: Storage, hint?: ImportHint) =>
    call<ImportResult>("import_paths", { paths, mode, hint: hint ?? null }),
  updateSoftware: (id: string, edit: SoftwareEdit) => call<Library>("update_software", { id, edit }),
  updateVersion: (softwareId: string, versionId: string, edit: VersionEdit) =>
    call<Library>("update_version", { softwareId, versionId, edit }),
  deleteSoftware: (id: string, deleteFiles: boolean) => call<Library>("delete_software", { id, deleteFiles }),
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
  fetchSummary: (id: string) => call<Library>("fetch_summary", { id }),
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

/** 把文件拖进窗口。`onHover` 在文件悬在窗口上方和离开时调用。 */
export async function onFileDrop(
  onDrop: (paths: string[]) => void,
  onHover: (over: boolean) => void,
): Promise<() => void> {
  if (!isTauri) return () => {};
  const { getCurrentWebview } = await import("@tauri-apps/api/webview");
  return getCurrentWebview().onDragDropEvent((event) => {
    const payload = event.payload;
    if (payload.type === "enter" || payload.type === "over") onHover(true);
    else if (payload.type === "leave") onHover(false);
    else if (payload.type === "drop") {
      onHover(false);
      if (payload.paths.length > 0) onDrop(payload.paths);
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
