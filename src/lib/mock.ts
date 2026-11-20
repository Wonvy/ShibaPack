// 浏览器预览用的假后端：数据只在内存里，刷新就复原。
// 只有不在桌面窗口里（没有 Tauri）时才会加载这个文件。地址后加 ?setup 可以预览首次启动的引导页，
// 加 ?offline 预览上次的仓库没打开时的引导页。

import { newId } from "./library";
import { maskAiKey } from "./ai";
import { siNotion, siPostman } from "simple-icons";
import type {
  AiProgress,
  ProcessInfo,
  SystemSample,
  ImportHint,
  BackupInfo,
  Candidate,
  Category,
  CheckProgress,
  DownloadProgress,
  DropEntry,
  ImportCategoryTarget,
  ImportItem,
  InstalledApp,
  LangPack,
  Leftover,
  Library,
  PackageFile,
  Platform,
  Progress,
  Release,
  RepoHit,
  Settings,
  Software,
  SoftwareEdit,
  SourceKind,
  StartupItem,
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
// 地址后加 ?en：演示数据和界面都换成英文（给英文说明文档截图用）
const english = new URLSearchParams(location.search).has("en");
const tr = (zh: string, en: string) => (english ? en : zh);

const githubExamples: RepoHit[] = [
  {
    reference: "Molunerfinn/PicGo",
    name: "PicGo",
    description: tr("支持多种图床的图片上传工具", "Image uploader for multiple hosting services"),
    url: "https://github.com/Molunerfinn/PicGo",
    ownerAvatarUrl: "https://avatars.githubusercontent.com/u/12621342?v=4",
    stars: 27506,
    archived: false,
  },
  {
    reference: "LuckSiege/PictureSelector",
    name: "PictureSelector",
    description: tr("Android 图片选择器", "Image picker for Android"),
    url: "https://github.com/LuckSiege/PictureSelector",
    ownerAvatarUrl: "https://avatars.githubusercontent.com/u/24836954?v=4",
    stars: 13576,
    archived: false,
  },
];

const EXT_PLATFORM: Record<string, Platform> = {
  exe: "windows",
  msi: "windows",
  appinstaller: "windows",
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

const defaultCategories: Category[] = [
  { id: "dev", name: tr("开发工具", "Development"), parentId: null, color: null, icon: "code" },
  { id: "design", name: tr("设计创作", "Design"), parentId: null, color: null, icon: "palette" },
  { id: "office", name: tr("办公效率", "Productivity"), parentId: null, color: null, icon: "briefcase" },
  { id: "media", name: tr("影音媒体", "Media"), parentId: null, color: null, icon: "film" },
  { id: "internet", name: tr("网络工具", "Internet"), parentId: null, color: null, icon: "globe" },
  { id: "chat", name: tr("聊天社交", "Communication"), parentId: null, color: null, icon: "message" },
  { id: "system", name: tr("系统工具", "Utilities"), parentId: null, color: null, icon: "wrench" },
  { id: "games", name: tr("游戏娱乐", "Games"), parentId: null, color: null, icon: "gamepad" },
];

const lib: Library = {
  format: 1,
  categories: [
    ...defaultCategories,
    { id: "editor", name: tr("编辑器", "Editors"), parentId: "dev", color: null, icon: null },
    { id: "runtime", name: tr("运行环境", "Runtimes"), parentId: "dev", color: null, icon: "terminal" },
  ],
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
    // 只找到英文的简介：开着自动翻译时会去翻译
    Obsidian: {
      text: "Obsidian is a personal knowledge base and note-taking app that works on a folder of local Markdown files.",
      source: "Wikipedia (en)",
      lang: "en",
      fetchedAt: stamp(2),
    },
  },
  settings: { theme: "system", accent: "blue", language: "zh-CN", defaultStorage: "managed", moveOnImport: false, autoClassifyOnImport: false, fetchSummaries: true, theSvgIcons: false, translateSummaries: true, offlineCatalog: true, autoUpdate: true,
    // 预览里假装已经设好了 AI，界面上的 AI 按钮才会出来
    ai: { preset: "deepseek", baseUrl: "https://api.deepseek.com", model: "deepseek-chat" },
  },
};

if (english) {
  const wechat = lib.software.find((s) => s.name === "微信");
  if (wechat) Object.assign(wechat, { name: "WeChat", publisher: "Tencent" });
  lib.summaries = {
    "Visual Studio Code": {
      text: "Visual Studio Code is a cross-platform source-code editor from Microsoft, with syntax highlighting, code completion, debugging and built-in Git support. Extensions add more languages and tools.",
      source: "Wikipedia (en)",
      fetchedAt: stamp(2),
    },
    Git: { text: "Git is a distributed version control system, first written by Linus Torvalds and released in 2005 under the GPL.", source: "Wikipedia (en)", fetchedAt: stamp(2) },
    Blender: { text: "Blender is a free and open-source 3D graphics suite for modeling, animation, rendering, compositing and video editing.", source: "Wikipedia (en)", fetchedAt: stamp(2) },
  };
  lib.settings.language = "en-US";
}

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
  [/code|git|docker|node|python|sublime|postman/i, "dev", tr("开发工具", "Developer tools"), "code"],
  [/figma|blender|sketch|photoshop/i, "design", tr("设计创作", "Design"), "palette"],
  [/obsidian|notion|office|typora/i, "office", tr("办公效率", "Productivity"), "briefcase"],
  [/vlc|obs|player|spotify/i, "media", tr("影音媒体", "Media"), "film"],
  [/telegram|微信|wechat|slack|discord/i, "chat", tr("聊天社交", "Chat"), "message"],
  [/7-zip|everything|cleaner/i, "system", tr("系统工具", "Utilities"), "wrench"],
];

function suggest(id: string): Suggestion {
  return suggestFor(lib.software.find((s) => s.id === id)!);
}

function suggestFor(item: Pick<Software, "id" | "name" | "categoryId">): Suggestion {
  const id = item.id;
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

/** 预览里“AI”给的分类依据。 */
const AI_REASONS: Record<string, [string, string]> = {
  dev: ["开发工具", "Developer tool"],
  editor: ["代码编辑器", "Code editor"],
  design: ["设计软件", "Design app"],
  office: ["笔记与文档", "Notes and docs"],
  media: ["影音播放", "Media player"],
};

/** 预览里的 AI 分类：在内置猜测的基础上，认不出的也给一个新分类，并带上一句依据。 */
function aiSuggest(id: string): Suggestion {
  const guess = suggest(id);
  const item = lib.software.find((s) => s.id === id)!;
  if (!guess.categoryId && !guess.newCategory) {
    return { ...guess, known: false, newCategory: { name: tr("实用工具", "Utilities"), icon: "wrench" }, reason: tr(`${item.name} 是一款工具`, "A utility") };
  }
  const category = lib.categories.find((c) => c.id === (guess.categoryId ?? ""));
  const reason = AI_REASONS[category?.id ?? ""] ?? [category?.name ?? guess.newCategory?.name ?? "", category?.name ?? guess.newCategory?.name ?? ""];
  return { ...guess, known: false, reason: tr(reason[0], reason[1]) };
}

const aiKeys = new Map([["https://api.deepseek.com", "••••••demo01"]]);
let aiCancelled = false;
const aiListeners = new Set<(progress: AiProgress) => void>();

export function mockOnAi(handler: (progress: AiProgress) => void): () => void {
  aiListeners.add(handler);
  return () => aiListeners.delete(handler);
}

/** 预览里分析过、正在卸载的软件：按 id 分开，可以同时卸载好几个。 */
const uninstalling = new Map<string, { windows: boolean; name: string; startedAt: number; found: Leftover[] }>();

function mockLeftovers(name: string, windows: boolean): Leftover[] {
  const MB = 1024 * 1024;
  const item = (kind: Leftover["kind"], path: string, confidence: Leftover["confidence"], reason: string, size: number | null, value?: string): Leftover =>
    ({ kind, path, confidence, reason, size, ...(value ? { value } : {}) });
  if (windows) {
    const short = name.replace(/^Microsoft /, "");
    return [
      item("folder", `C:\\Program Files (x86)\\Microsoft\\${short}\\Application`, "high", "installDir", 220 * MB),
      item("file", `C:\\Users\\demo\\AppData\\Roaming\\Microsoft\\Windows\\Start Menu\\Programs\\${name}.lnk`, "medium", "shortcut", 4096),
      item("folder", `C:\\Users\\demo\\AppData\\Local\\Microsoft\\${short}`, "high", "publisherName", 1840 * MB),
      item("file", `C:\\Users\\demo\\Desktop\\${name}.lnk`, "medium", "shortcut", 2048),
      item("folder", `C:\\Users\\demo\\AppData\\Roaming\\${short}Backup`, "low", "nameContains", 3 * MB),
      item("registryKey", `HKCU\\Software\\Microsoft\\${short}`, "high", "publisherName", null),
      item("registryKey", `HKLM\\SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\${name}`, "high", "uninstallKey", null),
      item("registryValue", "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run", "high", "startup", null, `${short}AutoLaunch`),
    ];
  }
  const id = `com.example.${name.toLowerCase().replace(/[^a-z0-9]/g, "")}`;
  return [
    item("folder", `/Users/demo/Library/Caches/${id}`, "high", "bundleId", 80 * MB),
    item("file", `/Users/demo/Library/Preferences/${id}.plist`, "high", "bundleId", 4096),
    item("folder", `/Users/demo/Library/Saved Application State/${id}.savedState`, "high", "bundleId", 20480),
    item("folder", `/Users/demo/Library/Application Support/${name}`, "medium", "name", 320 * MB),
    item("folder", `/Users/demo/Library/Logs/${name} Helper`, "low", "nameContains", 6 * MB),
  ];
}

/** 卸载前的分析：不含软件本身（安装文件夹、卸载项、Program Files 里的），和 leftovers::scan_before 一样。 */
function mockAnalysis(name: string, windows: boolean): Leftover[] {
  return mockLeftovers(name, windows).filter(
    (item) => !["installDir", "uninstallKey", "appPath"].includes(item.reason) && !item.path.startsWith("C:\\Program Files"),
  );
}

/** 预览里的进程：名字、平时的 CPU 和内存，每次取样在附近晃一晃。 */
const GB = 1024 ** 3;
let processes: (ProcessInfo & { base: number })[] = [
  ["Google Chrome", 6, 1.2 * GB, "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"],
  ["Google Chrome Helper (Renderer)", 4, 640 * MB, null],
  ["Google Chrome Helper (GPU)", 3, 380 * MB, null],
  ["Figma", 5, 980 * MB, "/Applications/Figma.app/Contents/MacOS/Figma"],
  ["Visual Studio Code", 2.5, 820 * MB, "/Applications/Visual Studio Code.app/Contents/MacOS/Electron"],
  ["Docker", 1.8, 2.1 * GB, "/Applications/Docker.app/Contents/MacOS/Docker"],
  ["DataGrip", 1.2, 1.6 * GB, "/Applications/DataGrip.app/Contents/MacOS/datagrip"],
  ["ShibaPack", 0.8, 160 * MB, "/Applications/ShibaPack.app/Contents/MacOS/shibapack"],
  ["WindowServer", 3.5, 410 * MB, "/System/Library/PrivateFrameworks/SkyLight.framework/Resources/WindowServer"],
  ["kernel_task", 2.2, 18 * MB, null],
  ["Finder", 0.3, 210 * MB, "/System/Library/CoreServices/Finder.app/Contents/MacOS/Finder"],
  ["Safari", 0.6, 450 * MB, "/Applications/Safari.app/Contents/MacOS/Safari"],
  ["mds_stores", 1.1, 120 * MB, null],
  ["node", 1.4, 300 * MB, "/opt/homebrew/bin/node"],
  ["zsh", 0, 6 * MB, "/bin/zsh"],
  ["Obsidian", 0.4, 360 * MB, "/Applications/Obsidian.app/Contents/MacOS/Obsidian"],
  ["VLC", 0.2, 140 * MB, "/Applications/VLC.app/Contents/MacOS/VLC"],
  ["launchd", 0.1, 12 * MB, "/sbin/launchd"],
  ["coreaudiod", 0.3, 24 * MB, null],
  ["Spotlight", 0.1, 90 * MB, null],
].map(([name, base, memory, exe], i) => ({
  pid: name === "launchd" ? 1 : name === "kernel_task" ? 0 : 400 + i * 37,
  name: name as string,
  base: base as number,
  cpu: base as number,
  memory: memory as number,
  exe: exe as string | null,
  protected: ["ShibaPack", "WindowServer", "kernel_task", "launchd", "coreaudiod", "mds_stores", "Spotlight"].includes(name as string),
}));

function mockSample(): SystemSample {
  const wobble = (value: number, spread: number) => Math.max(0, value + (Math.random() - 0.5) * spread);
  processes = processes.map((p) => ({ ...p, cpu: Math.min(100, wobble(p.base, p.base * 0.8 + 0.2)), memory: wobble(p.memory, p.memory * 0.02) }));
  const list = processes.map(({ base: _base, ...p }) => p).sort((a, b) => b.cpu - a.cpu);
  const memoryTotal = 36 * GB;
  return {
    cpu: Math.min(100, list.reduce((sum, p) => sum + p.cpu, 0)),
    memoryUsed: Math.min(memoryTotal, list.reduce((sum, p) => sum + p.memory, 0) + 9 * GB),
    memoryTotal,
    swapUsed: 1.2 * GB,
    swapTotal: 4 * GB,
    disk: { mount: "/", total: 994 * GB, available: 182 * GB },
    uptime: Math.round(performance.now() / 1000) + 3 * 86400 + 4 * 3600,
    processes: list,
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
  // 预览里假装 Edge 是 Windows 上用卸载程序卸的那种，好看到“等待卸载程序”和注册表残留
  { ...app("Microsoft Edge", "131.0.2903.86", "Microsoft Corporation", 1310, 30), source: "registry", location: "C:\\Program Files (x86)\\Microsoft\\Edge\\Application" },
  app("DataGrip", "2024.3", "JetBrains s.r.o", 2260, 45),
  app("VLC", "3.0.21", "VideoLAN", 148, 320),
];
// ── 启动项与服务 ─────────────────────────────────────────────

/** 预览里的启动项和服务。Windows 上（或地址后加 ?windows）看到的是 Windows 的样子，别的系统看到 macOS 的样子。 */
function startupItem(partial: Partial<StartupItem> & Pick<StartupItem, "id" | "kind" | "source" | "name">): StartupItem {
  return {
    label: partial.name,
    description: "",
    publisher: "",
    command: "",
    location: "",
    scope: "user",
    system: false,
    enabled: true,
    canToggle: true,
    startType: null,
    startTypes: [],
    running: null,
    pid: null,
    canControl: false,
    needsAdmin: false,
    app: "",
    icon: null,
    triggers: [],
    account: "",
    ...partial,
  };
}

const WIN_TYPES = ["auto", "delayed", "manual", "disabled"];

function winService(name: string, display: string, description: string, start: string, running: boolean, system: boolean, path: string): StartupItem {
  return startupItem({
    id: `service:${name}`,
    kind: "service",
    source: "service",
    name: display,
    label: name,
    description,
    publisher: system ? "Microsoft" : "",
    command: path,
    location: `HKLM\\SYSTEM\\CurrentControlSet\\Services\\${name}`,
    scope: "machine",
    system,
    enabled: start !== "disabled",
    startType: start,
    startTypes: WIN_TYPES,
    running,
    pid: running ? 1000 + name.length * 37 : null,
    canControl: true,
    needsAdmin: true,
    account: "LocalSystem",
  });
}

let startupItems: StartupItem[] = navigator.userAgent.includes("Windows") || new URLSearchParams(location.search).has("windows")
  ? [
      startupItem({ id: "run:HKCU:Run:Steam", kind: "startup", source: "registry", name: "Steam", description: "Steam Client Bootstrapper", publisher: "Valve Corporation", command: '"C:\\Program Files (x86)\\Steam\\steam.exe" -silent', location: "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run", triggers: ["login"] }),
      startupItem({ id: "run:HKCU:Run:Discord", kind: "startup", source: "registry", name: "Discord", description: "Discord", publisher: "Discord Inc.", command: '"C:\\Users\\demo\\AppData\\Local\\Discord\\Update.exe" --processStart Discord.exe', location: "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run", enabled: false, triggers: ["login"] }),
      startupItem({ id: "run:HKLM:Run:SecurityHealth", kind: "startup", source: "registry", name: "SecurityHealth", description: "Windows Security notification icon", publisher: "Microsoft Corporation", command: "%windir%\\system32\\SecurityHealthSystray.exe", location: "HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Run", scope: "machine", system: true, needsAdmin: true, triggers: ["login"] }),
      startupItem({ id: "folder:user:Everything.lnk", kind: "startup", source: "startupFolder", name: "Everything", label: "Everything.lnk", description: "Everything", publisher: "voidtools", command: "C:\\Program Files\\Everything\\Everything.exe", location: "C:\\Users\\demo\\AppData\\Roaming\\Microsoft\\Windows\\Start Menu\\Programs\\Startup\\Everything.lnk", triggers: ["login"] }),
      winService("wuauserv", "Windows Update", "启用检测、下载和安装 Windows 和其他程序的更新。如果此服务被禁用，这台计算机的用户将无法使用 Windows 更新或其自动更新功能。", "manual", true, true, "C:\\WINDOWS\\system32\\svchost.exe -k netsvcs -p"),
      winService("Spooler", "Print Spooler", "该服务在后台执行打印作业并处理与打印机的交互。如果关闭该服务，则无法进行打印或查看打印机。", "auto", true, true, "C:\\WINDOWS\\System32\\spoolsv.exe"),
      winService("AnyDesk", "AnyDesk Service", "", "delayed", true, false, '"C:\\Program Files (x86)\\AnyDesk\\AnyDesk.exe" --service'),
      winService("MozillaMaintenance", "Mozilla Maintenance Service", "Mozilla 维护服务确保您的计算机上安装了最新、最安全的 Mozilla Firefox 版本。", "manual", false, false, '"C:\\Program Files (x86)\\Mozilla Maintenance Service\\maintenanceservice.exe"'),
      winService("AdobeARMservice", "Adobe Acrobat Update Service", "Adobe Acrobat Updater 使您的 Adobe 软件保持最新。", "disabled", false, false, '"C:\\Program Files (x86)\\Common Files\\Adobe\\ARM\\1.0\\armsvc.exe"'),
    ]
  : [
      startupItem({ id: "login:/Applications/Docker.app", kind: "startup", source: "loginItem", name: "Docker", publisher: "Docker Inc", app: "Docker", command: "/Applications/Docker.app", location: "/Applications/Docker.app", triggers: ["login"] }),
      startupItem({ id: "login:/Applications/Obsidian.app", kind: "startup", source: "loginItem", name: "Obsidian", publisher: "Dynalist Inc", app: "Obsidian", command: "/Applications/Obsidian.app", location: "/Applications/Obsidian.app", enabled: false, triggers: ["login", "hidden"] }),
      startupItem({ id: "/Users/demo/Library/LaunchAgents/com.microsoft.EdgeUpdater.wake.plist", kind: "startup", source: "launchAgent", name: "com.microsoft.EdgeUpdater.wake", publisher: "Microsoft", app: "EdgeUpdater", command: '"/Users/demo/Library/Application Support/Microsoft/EdgeUpdater/Current/EdgeUpdater.app/Contents/MacOS/EdgeUpdater" --wake-all', location: "/Users/demo/Library/LaunchAgents/com.microsoft.EdgeUpdater.wake.plist", running: false, canControl: true, triggers: ["interval:3600"] }),
      startupItem({ id: "/Library/LaunchAgents/com.carriez.RustDesk_server.plist", kind: "startup", source: "launchAgent", name: "com.carriez.RustDesk_server", publisher: "Purslane Ltd", app: "RustDesk", command: "/Applications/RustDesk.app/Contents/MacOS/RustDesk --server", location: "/Library/LaunchAgents/com.carriez.RustDesk_server.plist", scope: "machine", running: true, pid: 1107, canControl: true, triggers: ["load", "keepAlive"] }),
      startupItem({ id: "/Library/LaunchAgents/com.google.keystone.agent.plist", kind: "startup", source: "launchAgent", name: "com.google.keystone.agent", publisher: "Google", command: "/Library/Google/GoogleSoftwareUpdate/GoogleSoftwareUpdate.bundle/Contents/Resources/GoogleSoftwareUpdateAgent.app/Contents/MacOS/GoogleSoftwareUpdateAgent -runMode ifneeded", location: "/Library/LaunchAgents/com.google.keystone.agent.plist", scope: "machine", enabled: false, running: false, canControl: true, triggers: ["load", "interval:3623"] }),
      startupItem({ id: "/System/Library/LaunchAgents/com.apple.Siri.agent.plist", kind: "startup", source: "launchAgent", name: "com.apple.Siri.agent", publisher: "Apple", command: "/System/Library/PrivateFrameworks/AssistantServices.framework/Versions/A/Support/siriactionsd", location: "/System/Library/LaunchAgents/com.apple.Siri.agent.plist", scope: "machine", system: true, enabled: false, running: false, canControl: true, triggers: ["demand"] }),
      startupItem({ id: "/Library/LaunchDaemons/com.carriez.RustDesk_service.plist", kind: "service", source: "launchDaemon", name: "com.carriez.RustDesk_service", publisher: "Purslane Ltd", app: "RustDesk", command: "/bin/sh -c /Applications/RustDesk.app/Contents/MacOS/service", location: "/Library/LaunchDaemons/com.carriez.RustDesk_service.plist", scope: "machine", running: true, pid: 345, canControl: true, needsAdmin: true, triggers: ["load", "keepAlive"], account: "root" }),
      startupItem({ id: "/Library/LaunchDaemons/com.cloudflare.cloudflared.plist", kind: "service", source: "launchDaemon", name: "com.cloudflare.cloudflared", publisher: "Cloudflare", command: '/usr/local/bin/cloudflared tunnel run --token-file "/Library/Application Support/com.cloudflare.cloudflared/token"', location: "/Library/LaunchDaemons/com.cloudflare.cloudflared.plist", scope: "machine", running: true, pid: 399, canControl: true, needsAdmin: true, account: "root" }),
      startupItem({ id: "/Library/LaunchDaemons/com.docker.vmnetd.plist", kind: "service", source: "launchDaemon", name: "com.docker.vmnetd", publisher: "Docker Inc", app: "Docker", command: "/Library/PrivilegedHelperTools/com.docker.vmnetd", location: "/Library/LaunchDaemons/com.docker.vmnetd.plist", scope: "machine", running: false, canControl: true, needsAdmin: true, triggers: ["demand"], account: "root" }),
      startupItem({ id: "/System/Library/LaunchDaemons/com.openssh.sshd.plist", kind: "service", source: "launchDaemon", name: "com.openssh.sshd", publisher: "Apple", command: "/usr/libexec/sshd-keygen-wrapper", location: "/System/Library/LaunchDaemons/com.openssh.sshd.plist", scope: "machine", system: true, running: false, canControl: true, needsAdmin: true, triggers: ["demand"], account: "root" }),
      startupItem({ id: "/System/Library/LaunchDaemons/com.apple.ftpd.plist", kind: "service", source: "launchDaemon", name: "com.apple.ftpd", publisher: "Apple", command: "/usr/libexec/ftpd -l", location: "/System/Library/LaunchDaemons/com.apple.ftpd.plist", scope: "machine", system: true, enabled: false, running: false, canControl: true, needsAdmin: true, triggers: ["demand"], account: "root" }),
    ];

/** 预览里按操作改一下假数据：停用的同时停下，启用的同时跑起来（launchd、systemd 就是这样）。 */
function mockStartupAction(id: string, action: string) {
  startupItems = startupItems.map((item) => {
    if (item.id !== id) return item;
    const launchd = item.source === "launchAgent" || item.source === "launchDaemon";
    switch (action) {
      case "enable":
        return { ...item, enabled: true, running: launchd ? true : item.running, pid: launchd ? 4242 : item.pid };
      case "disable":
        return { ...item, enabled: false, running: launchd ? false : item.running, pid: launchd ? null : item.pid };
      case "start":
      case "restart":
        return { ...item, running: true, pid: 4000 + Math.floor(Math.random() * 900) };
      case "stop":
        return { ...item, running: false, pid: null };
      default: {
        const type = action.replace("type:", "");
        return { ...item, startType: type, enabled: type !== "disabled" };
      }
    }
  });
}

let configured = !location.search.includes("setup") && !location.search.includes("offline");
// 新仓库有默认分类，软件和简介从空白开始。
if (location.search.includes("empty") || location.search.includes("setup")) {
  lib.software = [];
  lib.summaries = {};
  lib.categories = structuredClone(defaultCategories);
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

function sampleFolderFiles(path: string): string[] {
  return ["Postman-win64-11.23.0-Setup.exe", "Postman-11.23.0-arm64.dmg", "Postman-11.23.0-x64.deb"].map((name) => `${path}/${name}`);
}

function inspectEntry(path: string, includeChildren: boolean): DropEntry {
  const name = path.split(/[\\/]/).pop()!;
  const isDir = !/\.\w{2,8}$/.test(name) || /\.app$/i.test(name);
  const portable = /portable|\.app$/i.test(name);
  const match = name.match(/^(.*?)[-_ ](?:win64[-_ ])?v?(\d+(?:\.\d+)+)/);
  const guessName = (match?.[1] ?? name.replace(/\.\w+$/, "")).replace(/[-_]/g, " ").trim();
  const filenameVersion = match?.[2] ?? "";
  const metadataVersion = /Notion-4\.2\.0-x64\.exe/i.test(name) ? "4.2.1" : portable ? "2.4.1" : "";
  const installers = isDir && !/\.app$/i.test(name) ? sampleFolderFiles(path) : [];
  const logo = [siNotion, siPostman].find((icon) => icon.title.toLowerCase() === guessName.toLowerCase());
  const iconData = logo ? `data:image/svg+xml,${encodeURIComponent(logo.svg.replace("<svg ", `<svg fill="#${logo.hex === "000000" ? "93959A" : logo.hex}" `))}` : null;
  return {
    path, name, isDir, size: isDir ? null : 48 * MB,
    package: isDir && portable,
    installers: installers.length,
    installer: !isDir && /\.(exe|msi|appinstaller|dmg|pkg|deb|rpm|zip|appimage)$/i.test(name),
    guessName, guessVersion: filenameVersion || metadataVersion, filenameVersion, metadataVersion,
    matchedSoftwareId: lib.software.find((software) => software.name.toLowerCase() === guessName.toLowerCase())?.id ?? null,
    publisher: "",
    iconData,
    children: includeChildren ? installers.map((file) => inspectEntry(file, false)) : [],
  };
}

async function importPaths(paths: string[], mode: Storage, hint?: ImportHint) {
  const emit = (progress: Progress) => listeners.forEach((handler) => handler(progress));
  // 选的是文件夹时，假装在里面找到了几个安装包；标成“整个导入”的文件夹和 .app 当作一个安装包
  const packages = new Set(hint?.packages ?? []);
  const files = [...new Set(paths.flatMap((path) =>
    /\.\w{2,8}$/.test(path) || packages.has(path)
      ? [path]
      : sampleFolderFiles(path),
  ))];
  emit({ phase: "collect", current: 0, total: 0, file: "" });
  await sleep(350);
  const items: ImportItem[] = [];
  for (const [i, path] of files.entries()) {
    const base = path.split(/[\\/]/).pop()!;
    emit({ phase: "import", current: i + 1, total: files.length, file: base });
    await sleep(450);
    // 预览里演示失败：名字带 broken 的没有权限，带 locked 的被占用
    if (/broken/i.test(base)) {
      items.push({ path, status: "failed", softwareId: null, message: "Permission denied (os error 13)", step: "copy", reason: "permission_denied" });
      continue;
    }
    if (/locked/i.test(base)) {
      items.push({ path, status: "failed", softwareId: null, message: "Resource busy (os error 16)", step: "read", reason: "in_use" });
      continue;
    }
    const entry = inspectEntry(path, false);
    const edit = hint?.entries?.[path];
    // 在详情栏里给某个软件添加版本时，名字以那个软件为准
    const name = edit?.name.trim() || hint?.name || entry.guessName;
    const version = edit?.version.trim() ?? hint?.version ?? entry.guessVersion;
    let target = lib.software.find((s) => s.name.toLowerCase() === name.toLowerCase());
    if (target?.versions.some((v) => v.version === version && v.files.some((f) => f.name === base))) {
      items.push({ path, status: "duplicate" as const, softwareId: target.id, message: null });
      continue;
    }
    if (!target) {
      target = software(name, "", hint?.categoryId ?? null, false, [], 0);
      lib.software.push(target);
    } else if (!target.categoryId && hint?.categoryId) {
      target.categoryId = hint.categoryId;
    }
    let slot = target.versions.find((v) => v.version === version);
    if (!slot) {
      slot = { id: newId(), version, note: "", addedAt: stamp(0), files: [] };
      target.versions.push(slot);
    }
    slot.files.push(file(base, 80 + Math.random() * 120, `${name}/${version || "unknown"}`, mode, 0));
    items.push({ path, status: "imported" as const, softwareId: target.id, message: null });
  }
  let classified = 0;
  if (!hint?.categoryId && (hint?.autoClassify ?? lib.settings.autoClassifyOnImport)) {
    const ids = [...new Set(items.flatMap((item) => item.status === "imported" && item.softwareId ? [item.softwareId] : []))];
    const assignments = ids.filter((id) => !find(id).categoryId).map(suggest).filter((s) => s.categoryId || s.newCategory);
    if (assignments.length) {
      emit({ phase: "classify", current: 0, total: 0, file: "" });
      await mockCall("apply_classification", { assignments });
      classified = assignments.length;
    }
  }
  emit({ phase: "done", current: files.length, total: files.length, file: "" });
  const count = (status: string) => items.filter((item) => item.status === status).length;
  return {
    report: { imported: count("imported"), duplicates: count("duplicate"), failed: count("failed"), items, classified },
    library: snapshot(),
  };
}

/** 预览里的失败日志。 */
const mockLog: { time: string; area: string; message: string; detail: string }[] = [
  { time: new Date(Date.now() - 3600_000).toISOString(), area: "import", message: "not_a_file", detail: "/Users/demo/Downloads/broken.exe" },
  { time: new Date(Date.now() - 7200_000).toISOString(), area: "update", message: "Failed to move the new app into place", detail: "0.4.0" },
];

/** 预览里的 AI 缓存：启动项的解释、分类建议。 */
const explainCache: Record<string, { text: string; model: string; at: string; cached: boolean }> = {};
const classifyCache: Record<string, Suggestion> = {};

export async function mockCall(command: string, args: Record<string, unknown>): Promise<unknown> {
  await sleep(40);
  if (command.startsWith("disk_") || command === "ai_explain_folders") return (await import("./mockDisk")).mockDiskCall(command, args);
  if (command.startsWith("trace_") || command.endsWith("_trace") || command === "list_traces") return (await import("./mockTrace")).mockTraceCall(command, args);
  if (command.startsWith("cleanup_") || command === "ai_explain_cleanup") return (await import("./mockCleanup")).mockCleanupCall(command, args);
  switch (command) {
    case "get_status":
    case "set_repo_root": {
      if (command === "set_repo_root") configured = true;
      // ?offline：上次的仓库在 NAS 上、这会儿没连上
      const offline = !configured && location.search.includes("offline");
      const status: Status = {
        configured,
        repoRoot: configured ? ROOT : null,
        recovered: null,
        lastRoot: offline ? "/Volumes/NAS/Installers" : null,
        openError: offline ? "missing" : null,
        version: "0.1.0",
        arch: "arm64",
        selftest: null,
      };
      return status;
    }
    case "refresh_library":
      return null;
    case "get_library":
      (await import("./mockPaths")).seedPaths(lib);
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
    case "set_starred_many":
      for (const id of args.ids as string[]) find(id).starred = args.starred as boolean;
      return snapshot();
    case "move_software":
      for (const id of args.ids as string[]) find(id).categoryId = (args.categoryId as string | null) ?? null;
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
    case "log_failure":
      mockLog.unshift({ time: new Date().toISOString(), area: String(args.area), message: String(args.message), detail: String(args.detail ?? "") });
      return null;
    case "read_failure_log":
      return mockLog.slice(0, Number(args.limit ?? 500));
    case "clear_failure_log":
      mockLog.length = 0;
      return null;
    case "open_log_folder":
      return null;
    case "updater_target":
      return null;
    case "download_installer":
      await sleep(800);
      return `/Users/demo/Downloads/ShibaPack_${args.version}_universal.dmg`;
    case "release_notes":
      await sleep(400);
      return "## 0.4.0\n\n### 中文\n- 新增：启动与服务\n- 新增：顶栏搜索可以在 GitHub、Homebrew 上找\n\n### English\n- New: startup items and services\n- New: search GitHub and Homebrew from the top bar\n\n---\n\nDownload the installer for your system from the assets below.";
    case "github_search": {
      await sleep(700);
      const query = String(args.query).trim();
      const examples = githubExamples.filter((repo) => repo.reference.toLowerCase().includes(query.toLowerCase()));
      if (query && examples.length > 0) return examples;
      const slug = query.toLowerCase().replace(/[^a-z0-9]+/g, "-");
      if (!slug) return [];
      return [
        { reference: `${slug}/${slug}`, name: slug, description: tr(`${query} 的官方仓库（预览里的假数据）`, `Official ${query} repository (preview data)`), url: `https://github.com/${slug}/${slug}`, stars: 48213, archived: false },
        { reference: `${slug}-community/${slug}-desktop`, name: `${slug}-desktop`, description: tr(`${query} 桌面客户端`, `${query} desktop client`), url: `https://github.com/${slug}-community/${slug}-desktop`, stars: 2310, archived: false },
        { reference: `someone/awesome-${slug}`, name: `awesome-${slug}`, description: tr(`${query} 相关资源合集`, `A list of ${query} resources`), url: `https://github.com/someone/awesome-${slug}`, stars: 815, archived: true },
      ];
    }
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
      return { apps: installedApps, cachedIcons: null };
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
    case "analyze_uninstall": {
      await sleep(600);
      const target = installedApps.find((candidate) => candidate.id === args.id);
      if (!target) throw "not_found";
      const windows = target.source === "registry";
      const found = mockAnalysis(target.name, windows);
      uninstalling.set(target.id, { windows, name: target.name, startedAt: 0, found });
      const appPath = windows ? `"C:\\Program Files\\${target.name}\\Uninstall ${target.name}.exe" /currentuser` : target.location;
      return { items: found, stillInstalled: false, appPath };
    }
    case "uninstall_installed": {
      const target = installedApps.find((candidate) => candidate.id === args.id);
      const session = uninstalling.get(args.id as string);
      uninstalling.set(args.id as string, {
        windows: target?.source === "registry",
        name: target?.name ?? "",
        startedAt: Date.now(),
        found: session?.found ?? [],
      });
      installedApps = installedApps.filter((candidate) => candidate.id !== args.id);
      return null;
    }
    case "uninstall_status": {
      // Windows 那种要等卸载程序跑完，预览里等 3 秒
      const session = uninstalling.get(args.id as string);
      return Boolean(session?.windows && Date.now() - session.startedAt < 3000);
    }
    case "scan_leftovers": {
      await sleep(800);
      const session = uninstalling.get(args.id as string);
      if (!session) throw "not_found";
      // 卸载程序删掉了桌面快捷方式，没删安装文件夹
      session.found = mockLeftovers(session.name, session.windows).filter((item) => !item.path.endsWith(".lnk") || item.path.includes("Start Menu"));
      return { items: session.found, stillInstalled: session.windows && Date.now() - session.startedAt < 3000 };
    }
    case "set_window_glass":
      // 预览是浏览器，做不了窗口效果
      return false;
    case "system_overview":
      return {
        hostName: "Demo 的 MacBook Pro",
        os: "macOS 15.6 Sequoia",
        kernel: "24.6.0",
        arch: "arm64",
        cpuBrand: "Apple M3 Pro",
        cpuThreads: 12,
        cpuCores: 12,
        model: "MacBook Pro (Mac15,6)",
        coreSplit: [6, 6],
        cpuMhz: 0,
        gpus: [{ model: "Apple M3 Pro", memory: null }],
        disks: [
          { name: "Macintosh HD", mount: "/", total: 994 * GB, available: 182 * GB, removable: false, kind: "SSD" },
          { name: "Backup", mount: "/Volumes/Backup", total: 2000 * GB, available: 1210 * GB, removable: true, kind: "HDD" },
        ],
        totalMemory: 36 * GB,
        totalSwap: 4 * GB,
        bootTime: Math.round(Date.now() / 1000) - 3 * 86400,
      };
    case "system_sample":
      await sleep(80);
      return mockSample();
    case "process_icons":
      // 预览里没有真的图标文件
      return {};
    case "end_process": {
      const target = processes.find((p) => p.pid === args.pid);
      if (!target) throw "not_found";
      if (target.protected) throw "protected";
      // 正常退出时 Docker 不理会，要强制结束
      if (!args.force && target.name === "Docker") return null;
      processes = processes.filter((p) => p.pid !== args.pid);
      return null;
    }
    case "reveal_leftover":
      return null;
    case "remove_leftovers": {
      await sleep(700);
      const found = uninstalling.get(args.id as string)?.found ?? [];
      const chosen = (args.indices as number[]).map((i) => found[i]).filter(Boolean);
      // 普通权限删不掉 Program Files 和 HKLM 下的；以管理员身份重试就都删掉
      const denied = args.elevated ? [] : chosen.filter((item) => item.path.startsWith("C:\\Program Files") || item.path.startsWith("HKLM"));
      const registry = chosen.some((item) => item.kind.startsWith("registry"));
      return {
        removed: chosen.length - denied.length,
        failed: denied.map((item) => ({ item, reason: "access_denied" })),
        needsAdmin: denied.length > 0,
        backup: registry ? "C:\\Users\\demo\\AppData\\Local\\ShibaPack\\uninstall-backups\\20261120-101500" : null,
      };
    }
    case "open_uninstall_backup":
      return null;
    case "paths_add":
    case "path_edit":
    case "paths_remove":
    case "path_states":
    case "path_backup":
    case "path_backups":
    case "path_restore":
    case "path_delete_backup":
    case "path_reveal":
    case "path_reveal_backup":
      return (await import("./mockPaths")).mockPathsCall(command, args, lib, snapshot);
    case "list_startup":
      await sleep(500);
      return { items: startupItems, notes: [] };
    case "startup_action": {
      // 预览里假装要输入管理员密码，等一会儿
      const target = startupItems.find((item) => item.id === args.id);
      await sleep(target?.needsAdmin ? 1200 : 450);
      mockStartupAction(args.id as string, args.action as string);
      return { items: startupItems, notes: [] };
    }
    case "ai_explain_startup_many": {
      // 和真的一样：记下来的直接给；没解释过的凑成一批“问 AI”（预览里每 10 项等一会儿）
      const ids = args.ids as string[];
      const mode = args.mode as string;
      const explanations: Record<string, { text: string; model: string; at: string; cached: boolean }> = {};
      const todo: string[] = [];
      for (const id of ids) {
        if (mode !== "refresh" && explainCache[id]) explanations[id] = { ...explainCache[id], cached: true };
        else todo.push(id);
      }
      if (mode === "cached" || todo.length === 0) return { explanations, asked: 0, missing: 0 };
      const emit = (done: number) => aiListeners.forEach((handler) => handler({ done, total: todo.length }));
      emit(0);
      for (let done = 0; done < todo.length; done = Math.min(todo.length, done + 10)) {
        await sleep(900);
        emit(Math.min(todo.length, done + 10));
      }
      for (const id of todo) {
        const target = startupItems.find((item) => item.id === id);
        const name = target?.app || target?.name || "";
        explainCache[id] = {
          text: tr(
            `这是 ${name} 的后台组件，用来在后台检查更新或保持连接。停用后 ${name} 本身还能正常打开，只是不会在后台自动做这些事。（预览里的 AI 解释）`,
            `This is a background component of ${name} that checks for updates or keeps a connection open. If you disable it, ${name} still opens normally but stops doing this in the background. (AI explanation in preview)`,
          ),
          model: lib.settings.ai.model || "deepseek-chat",
          at: new Date().toISOString(),
          cached: false,
        };
        explanations[id] = explainCache[id];
      }
      return { explanations, asked: todo.length, missing: 0 };
    }
    case "ai_describe_many": {
      const items = args.items as { name: string }[];
      await sleep(1200);
      for (const item of items) {
        lib.summaries[item.name] = {
          text: tr(`${item.name} 是一款常用的桌面软件。（预览里的 AI 简介，一批写的）`, `${item.name} is a popular desktop app. (AI summary in preview, written in a batch)`),
          source: `AI · ${lib.settings.ai.model}`,
          fetchedAt: new Date().toISOString(),
          lang: english ? "en" : "zh",
        };
      }
      return { library: snapshot(), written: items.length, unknown: 0 };
    }
    case "set_summary": {
      const name = args.name as string;
      const text = String(args.text).trim();
      if (!text) delete lib.summaries[name];
      else lib.summaries[name] = { text, source: "manual", fetchedAt: new Date().toISOString(), lang: english ? "en" : "zh" };
      return snapshot();
    }
    case "fill_icons":
    case "fill_file_info":
      return { library: null, more: false };
    case "inspect_drop":
      return (args.paths as string[]).map((path) => inspectEntry(path, Boolean(args.includeChildren)));
    case "preview_import_categories":
      return (args.targets as ImportCategoryTarget[]).map(({ name }) => {
        const existing = lib.software.find((item) => item.name.toLowerCase() === name.toLowerCase());
        if (existing?.categoryId) return { categoryId: existing.categoryId, newCategory: null };
        const { categoryId, newCategory } = suggestFor(existing ?? { id: "", name, categoryId: null });
        return { categoryId, newCategory };
      });
    case "fetch_icon":
      // 预览里不联网，图标都用首字方块
      return null;
    case "fetch_summary": {
      await sleep(600);
      const name = args.name as string;
      const existing = args.refresh ? undefined : lib.summaries[name];
      if (!existing) {
        // 预览里不联网：有写好的简介就用，没有就记成“没找到”
        lib.summaries[name] = MOCK_SUMMARIES[name] ?? { text: "", source: "none", fetchedAt: new Date().toISOString() };
      } else if (lib.settings.translateSummaries && existing.text) {
        const text = MOCK_TRANSLATIONS[name] ?? existing.text;
        existing.translations = { ...existing.translations, [lib.settings.language]: text };
      }
      return snapshot();
    }
    case "ai_key_saved":
      return aiKeys.has(String(args.baseUrl).replace(/\/+$/, "").toLowerCase());
    case "ai_key_preview":
      return aiKeys.get(String(args.baseUrl).replace(/\/+$/, "").toLowerCase()) ?? null;
    case "ai_set_key": {
      const url = String(args.baseUrl).replace(/\/+$/, "").toLowerCase();
      const key = String(args.key).trim();
      if (key) aiKeys.set(url, maskAiKey(key));
      else aiKeys.delete(url);
      return null;
    }
    case "ai_models":
      await sleep(500);
      return ["deepseek-chat", "deepseek-reasoner"];
    case "ai_test":
      await sleep(700);
      return "OK";
    case "ai_describe": {
      await sleep(1200);
      const name = args.name as string;
      lib.summaries[name] = {
        text: tr(`${name} 是一款常用的桌面软件。（预览里的 AI 简介）`, `${name} is a popular desktop app. (AI summary in preview)`),
        source: `AI · ${lib.settings.ai.model}`,
        fetchedAt: new Date().toISOString(),
        lang: english ? "en" : "zh",
      };
      return snapshot();
    }
    case "ai_suggest_categories": {
      // 问过的直接用上次的建议（预览里不比较指纹），点“重新分析”才重新问
      const all = args.softwareIds as string[];
      const ids = args.refresh ? all : all.filter((id) => !(id in classifyCache));
      aiCancelled = false;
      const emit = (done: number) => aiListeners.forEach((handler) => handler({ done, total: ids.length }));
      if (ids.length > 0) emit(0);
      for (let done = 0; done < ids.length; done = Math.min(ids.length, done + 4)) {
        await sleep(500);
        if (aiCancelled) throw "cancelled";
        emit(Math.min(ids.length, done + 4));
      }
      for (const id of ids) classifyCache[id] = aiSuggest(id);
      return { suggestions: all.map((id) => classifyCache[id]), cached: all.length - ids.length };
    }
    case "cancel_ai":
      aiCancelled = true;
      return null;
    default:
      // 运行、定位、导出这类动作在预览里什么都不做
      return null;
  }
}

/** 预览里的“机器翻译”结果。 */
const MOCK_TRANSLATIONS: Record<string, string> = {
  Obsidian: "Obsidian 是一款个人知识库和笔记应用，直接使用本地文件夹里的 Markdown 文件。",
};

/** 预览里“联网”取回的简介：只装在本机、仓库里没有的软件用。 */
const MOCK_SUMMARIES: Record<string, { text: string; source: string; fetchedAt: string; lang?: string }> = {
  Calculator: { text: "计算器是 macOS 自带的计算器程序，支持基本运算、科学计算和程序员模式。", source: "Wikipedia (zh)", fetchedAt: "2026-11-19T12:00:00+08:00", lang: "zh" },
  Safari: { text: "Safari 是苹果公司开发的网页浏览器，是 macOS 和 iOS 的默认浏览器。", source: "Wikipedia (zh)", fetchedAt: "2026-11-19T12:00:00+08:00", lang: "zh" },
};
