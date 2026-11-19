// 浏览器预览用的假后端：数据只在内存里，刷新就复原。
// 只有不在桌面窗口里（没有 Tauri）时才会加载这个文件。地址后加 ?setup 可以预览首次启动的引导页。

import { newId } from "./library";
import type {
  ImportHint,
  BackupInfo,
  Candidate,
  Category,
  CheckProgress,
  DownloadProgress,
  ImportItem,
  InstalledApp,
  LangPack,
  Library,
  PackageFile,
  Platform,
  Progress,
  Release,
  Settings,
  Software,
  SoftwareEdit,
  SourceKind,
  Stats,
  Status,
  Storage,
  Suggestion,
  VerifyReport,
  VersionEdit,
} from "./types";

const ROOT = "/Users/demo/Installers";
const MB = 1024 * 1024;
const stamp = (daysAgo: number) => new Date(Date.now() - daysAgo * 86400000).toISOString();
const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

const EXT_PLATFORM: Record<string, Platform> = {
  exe: "windows",
  msi: "windows",
  dmg: "macos",
  pkg: "macos",
  deb: "linux",
  rpm: "linux",
  appimage: "linux",
  apk: "android",
};

function file(name: string, sizeMb: number, folder: string, storage: Storage = "managed", daysAgo = 3): PackageFile {
  const ext = name.split(".").pop()!.toLowerCase();
  const arch = /arm64|aarch64/i.test(name) ? "arm64" : /universal/i.test(name) ? "universal" : /x64|amd64|64-bit/i.test(name) ? "x64" : "";
  return {
    id: newId(),
    name,
    platform: EXT_PLATFORM[ext] ?? "cross",
    arch,
    size: Math.round(sizeMb * MB),
    sha256: Array.from({ length: 64 }, () => "0123456789abcdef"[Math.floor(Math.random() * 16)]).join(""),
    storage,
    path: storage === "managed" ? `${folder}/${name}` : `/Volumes/NAS/Software/${name}`,
    addedAt: stamp(daysAgo),
  };
}

function software(
  name: string,
  publisher: string,
  categoryId: string | null,
  starred: boolean,
  versions: [string, [string, number, Storage?][]][],
  daysAgo = 5,
): Software {
  return {
    id: newId(),
    name,
    publisher,
    homepage: "",
    note: "",
    categoryId,
    starred,
    icon: null,
    defaultVersionId: null,
    addedAt: stamp(daysAgo),
    versions: versions.map(([version, files], i) => ({
      id: newId(),
      version,
      note: "",
      addedAt: stamp(daysAgo + i * 20),
      files: files.map(([fileName, size, storage]) => file(fileName, size, `${name}/${version}`, storage, daysAgo + i * 20)),
    })),
  };
}

const categories: Category[] = [
  { id: "dev", name: "开发工具", parentId: null, color: null, icon: "code" },
  { id: "editor", name: "编辑器", parentId: "dev", color: null, icon: null },
  { id: "runtime", name: "运行环境", parentId: "dev", color: null, icon: "terminal" },
  { id: "design", name: "设计创作", parentId: null, color: null, icon: "palette" },
  { id: "office", name: "办公效率", parentId: null, color: null, icon: "briefcase" },
  { id: "media", name: "影音媒体", parentId: null, color: null, icon: "film" },
  { id: "system", name: "系统工具", parentId: null, color: null, icon: "wrench" },
];

const lib: Library = {
  format: 1,
  categories,
  software: [
    software("Visual Studio Code", "Microsoft", "editor", true, [
      ["1.96.2", [["VSCodeUserSetup-x64-1.96.2.exe", 104], ["VSCodeUserSetup-arm64-1.96.2.exe", 101], ["VSCode-darwin-universal.dmg", 221], ["code_1.96.2_amd64.deb", 98]]],
      ["1.95.3", [["VSCodeUserSetup-x64-1.95.3.exe", 103], ["VSCode-darwin-universal-1.95.3.dmg", 219]]],
      ["1.94.2", [["VSCodeUserSetup-x64-1.94.2.exe", 101]]],
    ], 2),
    software("Sublime Text", "Sublime HQ", "editor", false, [["4189", [["sublime_text_build_4189_x64_setup.exe", 18], ["sublime_text_build_4189_mac.dmg", 41]]]], 40),
    software("Git", "Git SCM", "dev", false, [["2.47.1", [["Git-2.47.1-64-bit.exe", 66], ["git-2.47.1-intel-universal.dmg", 44]]]], 12),
    software("Docker Desktop", "Docker Inc.", "dev", false, [["4.37.0", [["Docker Desktop Installer.exe", 512, "indexed"], ["Docker.dmg", 498, "indexed"]]]], 9),
    software("Node.js", "OpenJS Foundation", "runtime", false, [
      ["22.12.0", [["node-v22.12.0-x64.msi", 29], ["node-v22.12.0.pkg", 71]]],
      ["20.18.1", [["node-v20.18.1-x64.msi", 27]]],
    ], 20),
    software("Python", "Python Software Foundation", "runtime", false, [["3.13.1", [["python-3.13.1-amd64.exe", 27], ["python-3.13.1-macos11.pkg", 66]]]], 25),
    software("Figma", "Figma", "design", true, [["125.4.9", [["FigmaSetup.exe", 112], ["Figma.dmg", 131]]]], 15),
    software("Blender", "Blender Foundation", "design", false, [["4.3.2", [["blender-4.3.2-windows-x64.msi", 348], ["blender-4.3.2-macos-arm64.dmg", 302], ["blender-4.3.2-linux-x64.tar.xz", 371]]]], 31),
    software("Obsidian", "Dynalist Inc.", "office", true, [["1.7.7", [["Obsidian-1.7.7.exe", 96], ["Obsidian-1.7.7.dmg", 187], ["Obsidian-1.7.7.AppImage", 113]]]], 6),
    software("VLC", "VideoLAN", "media", false, [["3.0.21", [["vlc-3.0.21-win64.exe", 43], ["vlc-3.0.21-universal.dmg", 78], ["VLC-Android-3.6.0-arm64.apk", 38]]]], 60),
    software("OBS Studio", "OBS Project", "media", false, [["31.0.0", [["OBS-Studio-31.0.0-Windows-Installer.exe", 138], ["OBS-Studio-31.0.0-macOS-Apple.dmg", 181]]]], 18),
    software("7-Zip", "Igor Pavlov", "system", false, [["24.09", [["7z2409-x64.exe", 1.6], ["7z2409-arm64.exe", 1.5]]]], 90),
    software("Everything", "voidtools", "system", true, [["1.4.1.1026", [["Everything-1.4.1.1026.x64-Setup.exe", 1.8]]]], 75),
    software("Telegram", "Telegram FZ-LLC", null, false, [["5.9.0", [["tsetup-x64.5.9.0.exe", 46], ["Telegram-11.5.3-arm64.apk", 72]]]], 4),
    software("微信", "腾讯", null, true, [["3.9.12", [["WeChatSetup.exe", 228, "indexed"]]]], 1),
  ],
  summaries: {
    "Visual Studio Code": {
      text: "Visual Studio Code 是微软开发的跨平台源代码编辑器，支持语法高亮、代码自动补全、调试和内置的 Git 版本控制，可以通过扩展增加功能。",
      source: "Wikipedia (zh)",
      fetchedAt: stamp(2),
    },
    Git: { text: "git 是一个分布式版本控制软件，最初由林纳斯·托瓦兹创作，于 2005 年以 GPL 许可协议发布。", source: "Wikipedia (zh)", fetchedAt: stamp(2) },
    Blender: { text: "Blender 是一款自由开源的三维计算机图形软件，用于建模、动画、渲染、合成和视频剪辑。", source: "Wikipedia (zh)", fetchedAt: stamp(2) },
  },
  settings: { theme: "system", accent: "blue", language: "zh-CN", defaultStorage: "managed", fetchSummaries: true, offlineCatalog: true, autoUpdate: true },
};

/** 假装在线源上有这些版本。 */
function fakeRelease(kind: SourceKind, reference: string): Release {
  const short = reference.split(/[/.]/).pop() || reference;
  const version = kind === "homebrew" ? "1.97.0" : kind === "github" ? "1.8.4" : "26.06";
  const asset = (name: string, platform: Release["assets"][number]["platform"], arch: string, mb: number) => ({
    name,
    url: `https://example.com/${reference}/${name}`,
    platform,
    arch,
    size: Math.round(mb * MB),
    sha256: "0".repeat(64),
  });
  const assets =
    kind === "homebrew"
      ? [asset(`${short}-${version}-arm64.dmg`, "macos", "arm64", 221), asset(`${short}-${version}-x64.dmg`, "macos", "x64", 228)]
      : kind === "github"
        ? [asset(`${short}-${version}.exe`, "windows", "x64", 98), asset(`${short}-${version}.dmg`, "macos", "universal", 189), asset(`${short}-${version}.AppImage`, "linux", "x64", 114)]
        : [asset(`${short}-${version}-x64.exe`, "windows", "x64", 1.6), asset(`${short}-${version}-arm64.exe`, "windows", "arm64", 1.5)];
  return { version, publishedAt: stamp(1), page: "https://example.com/release", assets };
}

const subscribeTo = (name: string, source: SourceKind, reference: string) => {
  const target = lib.software.find((s) => s.name === name);
  if (target) target.subscription = { source, reference, latest: fakeRelease(source, reference), checkedAt: stamp(0) };
};
subscribeTo("Visual Studio Code", "homebrew", "visual-studio-code");
subscribeTo("Obsidian", "github", "obsidianmd/obsidian-releases");
subscribeTo("7-Zip", "winget", "7zip.7zip");

const catalog: Candidate[] = [
  { reference: "visual-studio-code", name: "Microsoft Visual Studio Code", description: "Open-source code editor", homepage: "https://code.visualstudio.com/", version: "1.97.0" },
  { reference: "obsidian", name: "Obsidian", description: "Knowledge base that works on top of a local folder of plain text Markdown files", homepage: "https://obsidian.md/", version: "1.8.4" },
  { reference: "figma", name: "Figma", description: "Collaborative team software", homepage: "https://www.figma.com/", version: "125.4.9" },
  { reference: "docker-desktop", name: "Docker Desktop", description: "App to build and share containerised applications and microservices", homepage: "https://www.docker.com/", version: "4.37.0" },
  { reference: "sublime-text", name: "Sublime Text", description: "Text editor for code, markup and prose", homepage: "https://www.sublimetext.com/", version: "4189" },
  { reference: "vlc", name: "VLC media player", description: "Multimedia player", homepage: "https://www.videolan.org/vlc/", version: "3.0.21" },
];

const GUESSES: [RegExp, string, string, string][] = [
  [/code|git|docker|node|python|sublime|postman/i, "dev", "开发工具", "code"],
  [/figma|blender|sketch|photoshop/i, "design", "设计创作", "palette"],
  [/obsidian|notion|office|typora/i, "office", "办公效率", "briefcase"],
  [/vlc|obs|player|spotify/i, "media", "影音媒体", "film"],
  [/telegram|微信|wechat|slack|discord/i, "chat", "聊天社交", "message"],
  [/7-zip|everything|cleaner/i, "system", "系统工具", "wrench"],
];

function suggest(id: string): Suggestion {
  const item = lib.software.find((s) => s.id === id)!;
  const hit = GUESSES.find(([pattern]) => pattern.test(item.name));
  if (!hit) return { softwareId: id, categoryId: null, newCategory: null, unchanged: false, known: false, evidence: [] };
  const [, key, name, icon] = hit;
  const existing = lib.categories.find((c) => c.id === key || c.name === name);
  // 已经在这个分类的下级里：保持原样
  const parentOf = (cid: string | null) => lib.categories.find((c) => c.id === cid)?.parentId ?? null;
  const inside = existing !== undefined && (item.categoryId === existing.id || parentOf(item.categoryId) === existing.id);
  return {
    softwareId: id,
    categoryId: inside ? item.categoryId : (existing?.id ?? null),
    newCategory: existing ? null : { name, icon },
    unchanged: inside,
    known: true,
    evidence: [],
  };
}

let langPacks: LangPack[] = [];
const downloadListeners = new Set<(progress: DownloadProgress) => void>();
const checkListeners = new Set<(progress: CheckProgress) => void>();
const cancelled = new Set<string>();

export function mockOnDownload(handler: (progress: DownloadProgress) => void): () => void {
  downloadListeners.add(handler);
  return () => downloadListeners.delete(handler);
}

export function mockOnCheck(handler: (progress: CheckProgress) => void): () => void {
  checkListeners.add(handler);
  return () => checkListeners.delete(handler);
}

const installedSizes: Record<string, number> = {};

function app(name: string, version: string, publisher: string, sizeMb: number, daysAgo: number, source: InstalledApp["source"] = "applications"): InstalledApp {
  installedSizes[`/Applications/${name}.app`] = sizeMb * MB;
  return {
    id: `/Applications/${name}.app`,
    name,
    version,
    publisher,
    installedAt: stamp(daysAgo),
    size: null,
    source,
    location: `/Applications/${name}.app`,
    uninstallable: source !== "system",
    icon: null,
  };
}

let installedApps: InstalledApp[] = [
  app("Visual Studio Code", "1.95.3", "Microsoft Corporation", 612, 200),
  app("Docker", "4.37.0", "Docker Inc", 1840, 90),
  app("Figma", "125.4.9", "Figma, Inc", 402, 60),
  app("Obsidian", "1.7.4", "Dynalist Inc", 398, 150),
  app("Safari", "18.2", "Apple", 14, 400, "system"),
  app("Calculator", "11.0", "Apple", 6, 400, "system"),
  app("Microsoft Edge", "131.0.2903.86", "Microsoft Corporation", 1310, 30),
  app("DataGrip", "2024.3", "JetBrains s.r.o", 2260, 45),
  app("VLC", "3.0.21", "VideoLAN", 148, 320),
];
let configured = !location.search.includes("setup");
// 地址后加 ?empty：仓库是空的，用来看欢迎页
if (location.search.includes("empty")) {
  lib.software = [];
  lib.categories = [];
}
let backups: BackupInfo[] = [3, 26, 50].map((hours) => {
  const date = new Date(Date.now() - hours * 3600000);
  const pad = (n: number) => String(n).padStart(2, "0");
  const name = `software-${date.getFullYear()}${pad(date.getMonth() + 1)}${pad(date.getDate())}-${pad(date.getHours())}${pad(date.getMinutes())}00-000.json`;
  return { name, createdAt: date.toISOString(), size: 18400 };
});

const listeners = new Set<(progress: Progress) => void>();

export function mockOnProgress(handler: (progress: Progress) => void): () => void {
  listeners.add(handler);
  return () => listeners.delete(handler);
}

export function mockPickFiles(): string[] {
  return ["/Users/demo/Downloads/Notion-4.2.0-x64.exe", "/Users/demo/Downloads/Notion-4.2.0-arm64.dmg"];
}

const snapshot = () => structuredClone(lib);
const find = (id: unknown) => {
  const found = lib.software.find((s) => s.id === id);
  if (!found) throw "not_found";
  return found;
};

function stats(): Stats {
  const files = lib.software.flatMap((s) => s.versions.flatMap((v) => v.files));
  const sum = (list: PackageFile[]) => list.reduce((total, f) => total + f.size, 0);
  return {
    software: lib.software.length,
    versions: lib.software.reduce((n, s) => n + s.versions.length, 0),
    files: files.length,
    totalSize: sum(files),
    managedSize: sum(files.filter((f) => f.storage === "managed")),
    indexedSize: sum(files.filter((f) => f.storage === "indexed")),
    backups: backups.length,
    trashSize: 96 * MB,
  };
}

async function importPaths(paths: string[], mode: Storage, hint?: ImportHint) {
  const emit = (progress: Progress) => listeners.forEach((handler) => handler(progress));
  // 选的是文件夹时，假装在里面找到了几个安装包
  const files = paths.flatMap((path) =>
    /\.\w{2,8}$/.test(path) ? [path] : ["Postman-win64-11.23.0-Setup.exe", "Postman-11.23.0-arm64.dmg", "Postman-11.23.0-x64.deb"].map((n) => `${path}/${n}`),
  );
  emit({ phase: "collect", current: 0, total: 0, file: "" });
  await sleep(350);
  const items: ImportItem[] = [];
  for (const [i, path] of files.entries()) {
    const base = path.split(/[\\/]/).pop()!;
    emit({ phase: "import", current: i + 1, total: files.length, file: base });
    await sleep(450);
    const match = base.match(/^(.*?)[-_ ](?:win64[-_ ])?v?(\d+(?:\.\d+)+)/);
    // 拖到某个软件上导入时，名字以那个软件为准
    const name = hint?.name ?? (match?.[1] ?? base.replace(/\.\w+$/, "")).replace(/[-_]/g, " ").trim();
    const version = match?.[2] ?? "";
    let target = lib.software.find((s) => s.name.toLowerCase() === name.toLowerCase());
    if (target?.versions.some((v) => v.version === version && v.files.some((f) => f.name === base))) {
      items.push({ path, status: "duplicate" as const, softwareId: target.id, message: null });
      continue;
    }
    if (!target) {
      target = software(name, "", null, false, [], 0);
      lib.software.push(target);
    }
    let slot = target.versions.find((v) => v.version === version);
    if (!slot) {
      slot = { id: newId(), version, note: "", addedAt: stamp(0), files: [] };
      target.versions.push(slot);
    }
    slot.files.push(file(base, 80 + Math.random() * 120, `${name}/${version || "unknown"}`, mode, 0));
    items.push({ path, status: "imported" as const, softwareId: target.id, message: null });
  }
  emit({ phase: "done", current: files.length, total: files.length, file: "" });
  const count = (status: string) => items.filter((item) => item.status === status).length;
  return {
    report: { imported: count("imported"), duplicates: count("duplicate"), failed: 0, items },
    library: snapshot(),
  };
}

export async function mockCall(command: string, args: Record<string, unknown>): Promise<unknown> {
  await sleep(40);
  switch (command) {
    case "get_status":
    case "set_repo_root": {
      if (command === "set_repo_root") configured = true;
      const status: Status = { configured, repoRoot: configured ? ROOT : null, recovered: null, version: "0.1.0", arch: "arm64", selftest: null };
      return status;
    }
    case "get_library":
      return snapshot();
    case "import_paths":
      return importPaths(args.paths as string[], args.mode as Storage, (args.hint as ImportHint | null) ?? undefined);
    case "update_software": {
      const edit = args.edit as SoftwareEdit;
      if (!edit.name.trim()) throw "invalid_name";
      Object.assign(find(args.id), edit, { name: edit.name.trim() });
      return snapshot();
    }
    case "update_version": {
      const target = find(args.softwareId).versions.find((v) => v.id === args.versionId);
      if (!target) throw "not_found";
      Object.assign(target, args.edit as VersionEdit);
      return snapshot();
    }
    case "delete_software":
      lib.software = lib.software.filter((s) => s.id !== args.id);
      return snapshot();
    case "delete_software_many": {
      const ids = new Set(args.ids as string[]);
      lib.software = lib.software.filter((s) => !ids.has(s.id));
      return snapshot();
    }
    case "delete_versions": {
      for (const { softwareId, versionId } of args.targets as { softwareId: string; versionId: string }[]) {
        const owner = lib.software.find((s) => s.id === softwareId);
        if (!owner) continue;
        owner.versions = owner.versions.filter((v) => v.id !== versionId);
        if (owner.defaultVersionId === versionId) owner.defaultVersionId = null;
      }
      return snapshot();
    }
    case "delete_version": {
      const owner = find(args.softwareId);
      owner.versions = owner.versions.filter((v) => v.id !== args.versionId);
      return snapshot();
    }
    case "delete_file": {
      const owner = find(args.softwareId);
      for (const version of owner.versions) version.files = version.files.filter((f) => f.id !== args.fileId);
      owner.versions = owner.versions.filter((v) => v.files.length > 0);
      return snapshot();
    }
    case "set_categories": {
      lib.categories = args.categories as Category[];
      const ids = new Set(lib.categories.map((c) => c.id));
      for (const category of lib.categories) if (category.parentId && !ids.has(category.parentId)) category.parentId = null;
      for (const item of lib.software) if (item.categoryId && !ids.has(item.categoryId)) item.categoryId = null;
      return snapshot();
    }
    case "set_starred":
      find(args.softwareId).starred = args.starred as boolean;
      return snapshot();
    case "suggest_categories":
      await sleep(300);
      return (args.softwareIds as string[]).map(suggest);
    case "apply_classification": {
      for (const item of args.assignments as { softwareId: string; categoryId?: string; newCategory?: { name: string; icon: string } }[]) {
        const target = lib.software.find((s) => s.id === item.softwareId);
        if (!target) continue;
        let categoryId = item.categoryId ?? null;
        if (!categoryId && item.newCategory) {
          const existing = lib.categories.find((c) => c.name === item.newCategory!.name);
          categoryId = existing?.id ?? newId();
          if (!existing) lib.categories.push({ id: categoryId, name: item.newCategory.name, parentId: null, color: null, icon: item.newCategory.icon });
        }
        if (categoryId) target.categoryId = categoryId;
      }
      return snapshot();
    }
    case "set_settings":
      lib.settings = args.settings as Settings;
      return snapshot();
    case "get_stats":
    case "empty_trash":
      return command === "empty_trash" ? { ...stats(), trashSize: 0 } : stats();
    case "list_backups":
      return backups;
    case "create_backup":
      backups = [{ name: `software-${Date.now()}.json`, createdAt: new Date().toISOString(), size: 18400 }, ...backups].slice(0, 8);
      return backups;
    case "restore_backup":
    case "import_backup":
      return snapshot();
    case "verify_data": {
      await sleep(500);
      const owner = lib.software.find((s) => s.versions.some((v) => v.files.some((f) => f.storage === "indexed")));
      const version = owner?.versions[0];
      const missing = version?.files.find((f) => f.storage === "indexed");
      const report: VerifyReport = {
        checked: stats().files,
        problems:
          owner && version && missing
            ? [{ softwareId: owner.id, versionId: version.id, fileId: missing.id, software: owner.name, version: version.version, name: missing.name, path: missing.path, reason: "missing" }]
            : [],
      };
      return report;
    }
    case "source_lookup":
      await sleep(500);
      if ((args.reference as string).includes("nothing")) throw "source_not_found";
      return fakeRelease(args.kind as SourceKind, args.reference as string);
    case "source_search": {
      await sleep(350);
      const query = (args.query as string).toLowerCase().replace(/[^a-z0-9]/g, "");
      return catalog.filter((c) => (c.name + c.reference).toLowerCase().replace(/[^a-z0-9]/g, "").includes(query));
    }
    case "subscribe": {
      await sleep(500);
      let target = lib.software.find((s) => s.id === args.softwareId);
      if (!target) {
        target = software((args.name as string) || (args.reference as string), "", null, false, [], 0);
        lib.software.push(target);
      }
      const kind = args.kind as SourceKind;
      target.subscription = { source: kind, reference: args.reference as string, latest: fakeRelease(kind, args.reference as string), checkedAt: new Date().toISOString() };
      return { library: snapshot(), softwareId: target.id };
    }
    case "unsubscribe":
      find(args.softwareId).subscription = null;
      return snapshot();
    case "check_updates": {
      const subscribed = lib.software.filter((s) => s.subscription);
      for (const [i, item] of subscribed.entries()) {
        checkListeners.forEach((handler) => handler({ current: i + 1, total: subscribed.length, name: item.name }));
        await sleep(500);
        item.subscription!.checkedAt = new Date().toISOString();
      }
      checkListeners.forEach((handler) => handler({ current: 0, total: 0, name: "" }));
      return { library: snapshot(), checked: subscribed.length, failed: 0, rateLimited: false };
    }
    case "download_asset": {
      const owner = find(args.softwareId);
      const release = owner.subscription?.latest;
      const asset = release?.assets.find((a) => a.url === args.url);
      if (!release || !asset) throw "not_found";
      const total = asset.size ?? 100 * MB;
      for (let step = 0; step <= 10; step++) {
        if (cancelled.delete(asset.url)) {
          downloadListeners.forEach((handler) => handler({ softwareId: owner.id, url: asset.url, name: asset.name, received: 0, total: null, done: true }));
          throw "cancelled";
        }
        downloadListeners.forEach((handler) => handler({ softwareId: owner.id, url: asset.url, name: asset.name, received: (total * step) / 10, total, done: false }));
        await sleep(280);
      }
      downloadListeners.forEach((handler) => handler({ softwareId: owner.id, url: asset.url, name: asset.name, received: 0, total: null, done: true }));
      let slot = owner.versions.find((v) => v.version === release.version);
      if (!slot) {
        slot = { id: newId(), version: release.version, note: "", addedAt: stamp(0), files: [] };
        owner.versions.push(slot);
      }
      slot.files.push({ ...file(asset.name, total / MB, `${owner.name}/${release.version}`, "managed", 0), platform: asset.platform, arch: asset.arch });
      return snapshot();
    }
    case "open_url":
      window.open(args.url as string, "_blank", "noreferrer");
      return null;
    case "cancel_download":
      cancelled.add(args.url as string);
      return null;
    case "tidy_root":
      await sleep(400);
      return { report: { imported: 0, duplicates: 0, failed: 0, items: [] }, library: snapshot() };
    case "list_lang_packs":
      return langPacks;
    case "import_lang_pack":
    case "remove_lang_pack":
      if (command === "remove_lang_pack") langPacks = langPacks.filter((pack) => pack._locale !== args.locale);
      return langPacks;
    case "list_installed":
      await sleep(400);
      return installedApps;
    case "measure_installed":
      await sleep(900);
      return installedSizes;
    case "match_installed": {
      const words = (name: string) => name.toLowerCase().replace(/[^a-z0-9\u4e00-\u9fff]+/g, " ").trim();
      const same = (a: string, b: string) => words(a) === words(b) || words(b) === `${words(a)} desktop` || words(a) === `${words(b)} desktop`;
      const links: Record<string, string> = {};
      for (const item of lib.software) {
        const found = installedApps.find((candidate) => same(candidate.name, item.name));
        if (found) links[item.id] = found.id;
      }
      return links;
    }
    case "uninstall_installed":
      installedApps = installedApps.filter((candidate) => candidate.id !== args.id);
      return null;
    case "fill_icons":
      return { library: null, more: false };
    case "fetch_summary": {
      await sleep(600);
      const target = find(args.id);
      lib.summaries[target.name] = { text: "", source: "none", fetchedAt: new Date().toISOString() };
      return snapshot();
    }
    default:
      // 运行、定位、导出这类动作在预览里什么都不做
      return null;
  }
}
