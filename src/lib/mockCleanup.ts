// 浏览器预览里的“清理”：一台假的电脑上能清理的东西。地址后加 ?windows 看 Windows 的样子，加 ?en 换英文。

import type { BiText, CleanEntry, CleanItem, CleanOptions, CleanPick, CleanProgress, CleanReport, CleanRun, CookieSite, Explanation } from "./types";

const params = new URLSearchParams(location.search);
const windows = navigator.userAgent.includes("Windows") || params.has("windows");
const MB = 1024 * 1024;
const GB = 1024 * MB;
const DAY = 86_400_000;
const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
const bi = (zh: string, en: string): BiText => ({ zh, en });

const HOME = windows ? "C:\\Users\\demo" : "/Users/demo";
const LOCAL = `${HOME}\\AppData\\Local`;
const ROAMING = `${HOME}\\AppData\\Roaming`;
const LIBRARY = `${HOME}/Library`;
/** 预览里的仓库：受保护，不能加成清理文件夹。 */
const REPO = "/Users/demo/Installers";
const sep = windows ? "\\" : "/";
const join = (...parts: string[]) => parts.join(sep);

type Spec = Omit<CleanItem, "entries" | "entryCount" | "size" | "files" | "modified" | "running" | "canQuit" | "note" | "importable" | "app" | "evidence" | "admin" | "cookies" | "tool"> &
  Partial<Pick<CleanItem, "running" | "note" | "importable" | "app" | "evidence" | "size" | "admin" | "cookies" | "tool" | "files">> & {
    entries: [string, number, number?, CleanEntry["library"]?][];
    quit?: boolean;
  };

/** 一项的条目：[路径, 大小, 几天前改过, 仓库里对应的软件]。 */
function item(spec: Spec): CleanItem {
  const entries: CleanEntry[] = spec.entries
    .map(([path, size, days, library]) => ({
      path,
      size,
      // 散落的安装包一条就是一个文件
      files: library === undefined ? Math.max(1, Math.round(size / (180 * 1024))) : 1,
      modified: Date.now() - (days ?? 2) * DAY,
      library: library ?? null,
      index: 0,
    }))
    .sort((a, b) => b.size - a.size)
    .map((entry, index) => ({ ...entry, index }));
  return {
    ...spec,
    app: spec.app ?? null,
    evidence: spec.evidence ?? null,
    entries,
    entryCount: entries.length,
    size: spec.size === undefined ? entries.reduce((sum, e) => sum + e.size, 0) : spec.size,
    files: spec.files ?? entries.reduce((sum, e) => sum + e.files, 0),
    modified: entries.length ? Math.max(...entries.map((e) => e.modified ?? 0)) : null,
    running: spec.running ?? false,
    canQuit: spec.quit ?? false,
    note: spec.note ?? null,
    importable: spec.importable ?? false,
    admin: spec.admin ?? false,
    cookies: spec.cookies ?? false,
    tool: spec.tool ?? null,
  };
}

const CHROMIUM_DOC = "https://chromium.googlesource.com/chromium/src/+/HEAD/docs/user_data_dir.md";
const BROWSER_EFFECT = bi("网站第一次打开会慢一点。登录状态、历史、密码、书签都不受影响。", "Sites load a little slower the first time. Sign-ins, history, passwords and bookmarks are not affected.");

const ELECTRON_DOC = "https://www.electronjs.org/docs/latest/api/app";
const HISTORY_NAMES = ["History", "Visited Links", "Top Sites", "Shortcuts", "Network Action Predictor", "History-journal"];

/** 浏览记录：每个配置里的几个文件。 */
function history(id: string, short: string, app: string, profile: string, running = false): CleanItem {
  return item({
    id, category: "privacy", risk: "high", action: "delete", app, running, quit: true,
    name: bi(`${short} 浏览记录`, `${short} browsing history`),
    about: bi(`${short} 记下的访问过的网页、下载记录、常去的网站和地址栏的联想。`, `Pages you visited, download history, top sites and address bar suggestions ${short} keeps.`),
    effect: bi("删了就找不回来。书签、密码、Cookie 不受影响。", "Cannot be recovered. Bookmarks, passwords and cookies are not affected."),
    evidence: "https://support.google.com/chrome/answer/95589",
    roots: [profile],
    entries: HISTORY_NAMES.map((name, i) => [join(profile, name), [38, 6, 2, 1.5, 0.8, 0.2][i] * MB, 0] as [string, number, number]),
  });
}

/** Cookie：有保留名单时只删名单以外的网站。 */
function cookies(id: string, short: string, app: string, profile: string, running = false): CleanItem {
  return item({
    id, category: "privacy", risk: "high", action: "delete", app, running, quit: true, cookies: true,
    name: bi(`${short} Cookie`, `${short} cookies`),
    about: bi(`网站存在 ${short} 里的 Cookie，用来记住登录状态、偏好，也用来跟踪你。`, `Cookies websites store in ${short} to remember sign-ins and preferences, and to track you.`),
    effect: bi("保留名单以外的网站要重新登录。删了就找不回来。", "Sites not on your keep list will ask you to sign in again. Cannot be recovered."),
    evidence: "https://support.google.com/chrome/answer/95589",
    roots: [profile, join(profile, "Network")],
    entries: [[join(profile, "Network", "Cookies"), 3.2 * MB, 0], [join(profile, "Network", "Cookies-journal"), 0, 0]],
  });
}

function clipboard(): CleanItem {
  return item({
    id: "privacy.clipboard", category: "privacy", risk: "recoverable", action: "clearClipboard", size: null,
    name: bi("剪贴板", "Clipboard"),
    about: bi("最近一次复制的内容。可能是密码、验证码或者别的不想留下的东西。", "Whatever you copied last. It may be a password, a code or something else you'd rather not leave behind."),
    effect: bi("剪贴板清空，粘贴不出东西。", "The clipboard is emptied; there is nothing to paste."),
    roots: [], entries: [],
  });
}

/** Electron 应用的缓存。 */
function electron(id: string, app: string, base: string, sizes: number[], running = false): CleanItem {
  const dirs = ["Cache", "Code Cache", "GPUCache"];
  return item({
    id, category: "apps", risk: "safe", action: "delete", app, running, quit: true,
    name: bi(`${app} 缓存`, `${app} cache`),
    about: bi(`${app} 存下的网页文件、编译好的代码和显卡缓存。`, `Web files, compiled code and GPU caches ${app} keeps.`),
    effect: bi("下次打开会慢一点。登录状态和内容不受影响。", "Opens a little slower next time. Sign-in and content are not affected."),
    evidence: ELECTRON_DOC,
    roots: dirs.map((d) => join(base, d)),
    entries: dirs.map((d, i) => [join(base, d, d === "Cache" ? "Cache_Data" : "data"), sizes[i] * MB, 0] as [string, number, number]),
  });
}

function macItems(): CleanItem[] {
  return [
    item({
      id: "system.temp", category: "system", risk: "safe", action: "delete",
      name: bi("临时文件", "Temporary files"),
      about: bi("程序运行时放在临时文件夹里的文件，用完后常常没有删掉。这里只算 7 天里都没动过的。", "Files that programs put in the temporary folder and often leave behind. Only items untouched for 7 days are counted."),
      effect: bi("不影响已经安装的软件。最近用过的、正在用的不会算进来。", "Installed software is not affected. Recently used or in-use items are left alone."),
      evidence: "https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-gettemppathw",
      roots: ["/var/folders/q_/fc76wr95/T"],
      entries: [["/var/folders/q_/fc76wr95/T/vscode-typescript501", 148 * MB, 12], ["/var/folders/q_/fc76wr95/T/node-compile-cache", 96 * MB, 21], ["/var/folders/q_/fc76wr95/T/zoom-update.pkg", 54 * MB, 30], ["/var/folders/q_/fc76wr95/T/TemporaryDownloads", 14 * MB, 9]],
    }),
    item({
      id: "system.logs", category: "system", risk: "recoverable", action: "trash",
      name: bi("应用日志", "App logs"),
      about: bi("应用写下的运行记录，排查问题时才用得到，时间长了会越积越多。", "Activity records apps write for troubleshooting. They pile up over time."),
      effect: bi("移到废纸篓。应用会重新开始记，以前的记录就看不到了。", "Moved to the Trash. Apps start new logs; older records are gone."),
      roots: [`${LIBRARY}/Logs`],
      entries: [[`${LIBRARY}/Logs/JetBrains`, 121 * MB, 1], [`${LIBRARY}/Logs/RustDesk`, 58 * MB, 3], [`${LIBRARY}/Logs/Claude`, 19 * MB, 0], [`${LIBRARY}/Logs/com.apple.CloudTelemetry`, 4 * MB, 6]],
    }),
    item({
      id: "system.crash-reports", category: "system", risk: "recoverable", action: "trash",
      name: bi("崩溃报告", "Crash reports"),
      about: bi("程序崩溃时系统记下的报告和内存转储，转储文件常常很大。", "Reports and memory dumps the system records when a program crashes. Dumps are often large."),
      effect: bi("移到回收站。以后没法再拿这些报告查崩溃原因。", "Moved to the Recycle Bin/Trash. You can no longer use these reports to investigate crashes."),
      roots: [`${LIBRARY}/Logs/DiagnosticReports`],
      entries: [[`${LIBRARY}/Logs/DiagnosticReports/RustDesk-2026-11-17-035501.ips`, 96 * 1024, 3], [`${LIBRARY}/Logs/DiagnosticReports/RustDesk-2026-11-14-082015.ips`, 88 * 1024, 6], [`${LIBRARY}/Logs/DiagnosticReports/Retired`, 2 * MB, 40]],
    }),
    item({
      id: "system.library-caches", category: "system", risk: "safe", action: "delete", admin: true,
      name: bi("系统级缓存", "System-wide caches"),
      about: bi("应用放在 /Library/Caches 里、所有用户共用的缓存。苹果自己的不算在内。", "Caches that apps keep in /Library/Caches for all users. Apple's own are left out."),
      effect: bi("应用下次用到时重新生成。要输入管理员密码。", "Apps regenerate them when needed. Requires an administrator password."),
      evidence: "https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/FileSystemProgrammingGuide/MacOSXDirectories/MacOSXDirectories.html",
      roots: ["/Library/Caches"],
      entries: [["/Library/Caches/com.microsoft.autoupdate.helper", 212 * MB, 20], ["/Library/Caches/com.adobe.acc", 64 * MB, 33]],
    }),
    item({
      id: "browser.chrome", category: "browser", risk: "safe", action: "delete", app: "Google Chrome", running: true, quit: true,
      name: bi("Chrome 缓存", "Chrome cache"),
      about: bi("Chrome 存下的网页文件、脚本编译结果和显卡着色器，用来让常去的网站打开得快一点。", "Web files, compiled scripts and GPU shaders Chrome keeps so sites you visit often load faster."),
      effect: BROWSER_EFFECT, evidence: CHROMIUM_DOC,
      roots: [`${LIBRARY}/Caches/Google/Chrome/Default/Cache`, `${LIBRARY}/Caches/Google/Chrome/Default/Code Cache`, `${LIBRARY}/Application Support/Google/Chrome/Default/GPUCache`],
      entries: [[`${LIBRARY}/Caches/Google/Chrome/Default/Cache/Cache_Data`, 812 * MB, 0], [`${LIBRARY}/Caches/Google/Chrome/Default/Code Cache/js`, 318 * MB, 0], [`${LIBRARY}/Caches/Google/Chrome/Default/Code Cache/wasm`, 64 * MB, 1], [`${LIBRARY}/Application Support/Google/Chrome/Default/GPUCache/data_1`, 18 * MB, 0]],
    }),
    item({
      id: "browser.edge", category: "browser", risk: "safe", action: "delete", app: "Microsoft Edge", quit: true,
      name: bi("Edge 缓存", "Edge cache"),
      about: bi("Edge 存下的网页文件、脚本编译结果和显卡着色器，用来让常去的网站打开得快一点。", "Web files, compiled scripts and GPU shaders Edge keeps so sites you visit often load faster."),
      effect: BROWSER_EFFECT, evidence: CHROMIUM_DOC,
      roots: [`${LIBRARY}/Caches/Microsoft Edge/Default/Cache`, `${LIBRARY}/Caches/Microsoft Edge/Default/Code Cache`],
      entries: [[`${LIBRARY}/Caches/Microsoft Edge/Default/Cache/Cache_Data`, 612 * MB, 1], [`${LIBRARY}/Caches/Microsoft Edge/Default/Code Cache/js`, 249 * MB, 1], [`${LIBRARY}/Caches/Microsoft Edge/Default/Code Cache/wasm`, 30 * MB, 4]],
    }),
    item({
      id: "browser.firefox", category: "browser", risk: "safe", action: "delete", app: "Firefox", quit: true,
      name: bi("Firefox 缓存", "Firefox cache"),
      about: bi("Firefox 存下的网页文件、启动缓存和网页缩略图。", "Web files, startup cache and page thumbnails Firefox keeps."),
      effect: BROWSER_EFFECT, evidence: "https://support.mozilla.org/kb/how-clear-firefox-cache",
      roots: [`${LIBRARY}/Caches/Firefox/Profiles/x8k2.default-release/cache2`],
      entries: [[`${LIBRARY}/Caches/Firefox/Profiles/x8k2.default-release/cache2/entries`, 341 * MB, 5], [`${LIBRARY}/Caches/Firefox/Profiles/x8k2.default-release/cache2/doomed`, 15 * MB, 5]],
    }),
    electron("apps.claude", "Claude", `${LIBRARY}/Application Support/Claude`, [412, 168, 15], true),
    electron("apps.slack", "Slack", `${LIBRARY}/Application Support/Slack`, [530, 96, 22]),
    history("privacy.edge-history", "Edge", "Microsoft Edge", `${LIBRARY}/Application Support/Microsoft Edge/Default`),
    cookies("privacy.edge-cookies", "Edge", "Microsoft Edge", `${LIBRARY}/Application Support/Microsoft Edge/Default`),
    cookies("privacy.chrome-cookies", "Chrome", "Google Chrome", `${LIBRARY}/Application Support/Google/Chrome/Default`, true),
    clipboard(),
    item({
      id: "development.xcode-derived-data", category: "development", risk: "safe", action: "delete", app: "Xcode", quit: true,
      name: bi("Xcode 编译数据", "Xcode derived data"),
      about: bi("Xcode 编译项目时生成的中间文件、索引和编译结果，每个项目一份，项目多了会很大。", "Intermediate files, indexes and build products Xcode creates for each project. Grows large with many projects."),
      effect: bi("下次打开项目会重新建索引，第一次编译要从头来。", "Projects re-index on next open and the first build starts from scratch."),
      evidence: "https://developer.apple.com/documentation/xcode/build-system",
      roots: [`${LIBRARY}/Developer/Xcode/DerivedData`],
      entries: [[`${LIBRARY}/Developer/Xcode/DerivedData/ShibaCast-dkqzhrwbgvlx`, 3.1 * GB, 4], [`${LIBRARY}/Developer/Xcode/DerivedData/Weather-aqpvbtusjfhy`, 1.9 * GB, 33], [`${LIBRARY}/Developer/Xcode/DerivedData/ModuleCache.noindex`, 1.2 * GB, 4], [`${LIBRARY}/Developer/Xcode/DerivedData/SymbolCache.noindex`, 0.4 * GB, 60]],
    }),
    item({
      id: "development.npm", category: "development", risk: "safe", action: "delete", app: "npm",
      name: bi("npm 缓存", "npm cache"),
      about: bi("npm 下载过的包。npm 会自己校验缓存，坏了也会重新下载。", "Packages npm has downloaded. npm verifies the cache and re-downloads anything missing."),
      effect: bi("下次安装依赖时重新下载。已经装在项目里的不受影响。", "Packages are downloaded again on next install. Dependencies already in projects are not affected."),
      evidence: "https://docs.npmjs.com/cli/commands/npm-cache",
      roots: [`${HOME}/.npm/_cacache`],
      entries: [[`${HOME}/.npm/_cacache/content-v2`, 1.31 * GB, 1], [`${HOME}/.npm/_cacache/index-v5`, 98 * MB, 1], [`${HOME}/.npm/_cacache/tmp`, 1 * MB, 1]],
    }),
    item({
      id: "development.go-build", category: "development", risk: "safe", action: "delete", app: "Go",
      name: bi("Go 编译缓存", "Go build cache"),
      about: bi("Go 编译和测试时记下的中间结果，用来加快下次编译。", "Intermediate results Go keeps from builds and tests to speed up the next build."),
      effect: bi("下次编译要从头来，会慢一些。", "The next build starts from scratch and takes longer."),
      evidence: "https://pkg.go.dev/cmd/go",
      roots: [`${LIBRARY}/Caches/go-build`],
      entries: Array.from({ length: 24 }, (_, i) => [`${LIBRARY}/Caches/go-build/${i.toString(16).padStart(2, "0")}`, (40 + ((i * 37) % 90)) * MB, i % 9] as [string, number, number]),
    }),
    item({
      id: "development.cargo", category: "development", risk: "recoverable", action: "trash", app: "Cargo",
      name: bi("Cargo 下载的 crate", "Cargo downloaded crates"),
      about: bi("Cargo 下载的 crate 压缩包和解开的源代码。", "Crate archives Cargo downloaded and their extracted sources."),
      effect: bi("移到回收站。下次编译 Rust 项目时要重新下载，没有网络时编译不了。", "Moved to the Recycle Bin/Trash. Crates are downloaded again on the next build, which needs a network connection."),
      evidence: "https://doc.rust-lang.org/cargo/guide/cargo-home.html",
      roots: [`${HOME}/.cargo/registry/cache`, `${HOME}/.cargo/registry/src`],
      entries: [[`${HOME}/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f`, 1.29 * GB, 1], [`${HOME}/.cargo/registry/cache/index.crates.io-1949cf8c6b5b557f`, 0.43 * GB, 1]],
    }),
    item({
      id: "development.vscode", category: "development", risk: "safe", action: "delete", app: "Visual Studio Code", running: true, quit: true,
      name: bi("VS Code 缓存", "VS Code cache"),
      about: bi("VS Code 的界面缓存、各版本编译好的代码、下载过的扩展安装包和日志。", "VS Code's interface cache, compiled code for each version, downloaded extension packages and logs."),
      effect: bi("下次打开会慢一点。设置、扩展、打开过的项目都不受影响。", "Opens a little slower next time. Settings, extensions and recent projects are not affected."),
      evidence: CHROMIUM_DOC,
      roots: [`${LIBRARY}/Application Support/Code/Cache`, `${LIBRARY}/Application Support/Code/CachedData`, `${LIBRARY}/Application Support/Code/logs`],
      entries: [[`${LIBRARY}/Application Support/Code/CachedData/1.106.0`, 21 * MB, 0], [`${LIBRARY}/Application Support/Code/logs/20261120T091102`, 6 * MB, 0], [`${LIBRARY}/Application Support/Code/Cache/Cache_Data`, 5 * MB, 0]],
    }),
    item({
      id: "installers.homebrew", category: "installers", risk: "safe", action: "delete", app: "Homebrew",
      name: bi("Homebrew 下载缓存", "Homebrew download cache"),
      about: bi("Homebrew 下载过的安装包和软件包信息，装完以后一般用不到。", "Installers and package metadata Homebrew downloaded. Usually not needed after installing."),
      effect: bi("重新安装或升级时要重新下载。已经装好的软件不受影响。", "Downloaded again when you reinstall or upgrade. Installed software is not affected."),
      evidence: "https://docs.brew.sh/FAQ",
      roots: [`${LIBRARY}/Caches/Homebrew`],
      entries: [[`${LIBRARY}/Caches/Homebrew/downloads`, 2.1 * GB, 3], [`${LIBRARY}/Caches/Homebrew/api`, 41 * MB, 0], [`${LIBRARY}/Caches/Homebrew/Cask`, 2 * MB, 3]],
    }),
    ...loose(),
    item({
      id: "installers.shibapack-partial", category: "installers", risk: "safe", action: "delete", app: "ShibaPack",
      name: bi("ShibaPack 没下完的文件", "ShibaPack unfinished downloads"),
      about: bi("在 ShibaPack 里下载安装包时中断或取消留下的半截文件，本来是留着从断点接着下的。这里只算 1 天里都没动过的。", "Partial files left when a download in ShibaPack was interrupted or cancelled, kept for resuming. Only files untouched for a day are counted."),
      effect: bi("再下载这个安装包时要从头下。", "Downloading that installer again starts from the beginning."),
      roots: [`${REPO}/.shibapack/downloads`],
      entries: [[`${REPO}/.shibapack/downloads/5f2a91c0d3e4b7a8.part`, 86 * MB, 4], [`${REPO}/.shibapack/downloads/5f2a91c0d3e4b7a8.part.etag`, 1024, 4]],
    }),
    item({
      id: "trash.empty", category: "trash", risk: "high", action: "emptyTrash", size: null, note: "full_disk_access", files: 27,
      name: bi("回收站", "Recycle Bin / Trash"),
      about: bi("已经删除、还放在回收站（废纸篓）里的文件。", "Files you deleted that are still in the Recycle Bin or Trash."),
      effect: bi("清空以后就找不回来了。", "Once emptied, these files cannot be recovered."),
      roots: [], entries: [],
    }),
  ];
}

function windowsItems(): CleanItem[] {
  return [
    item({
      id: "system.temp", category: "system", risk: "safe", action: "delete",
      name: bi("临时文件", "Temporary files"),
      about: bi("程序运行时放在临时文件夹里的文件，用完后常常没有删掉。这里只算 7 天里都没动过的。", "Files that programs put in the temporary folder and often leave behind. Only items untouched for 7 days are counted."),
      effect: bi("不影响已经安装的软件。最近用过的、正在用的不会算进来。", "Installed software is not affected. Recently used or in-use items are left alone."),
      evidence: "https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-gettemppathw",
      roots: [join(LOCAL, "Temp")],
      entries: [[join(LOCAL, "Temp", "vcredist_x64.exe"), 24 * MB, 40], [join(LOCAL, "Temp", "chocolatey"), 312 * MB, 22], [join(LOCAL, "Temp", "NVIDIA Corporation"), 188 * MB, 15], [join(LOCAL, "Temp", "msedge_installer.log"), 2 * MB, 9]],
    }),
    item({
      id: "system.windows-temp", category: "system", risk: "safe", action: "delete", admin: true, size: null, note: "needs_admin",
      name: bi("系统临时文件", "System temporary files"),
      about: bi("Windows 和以系统身份运行的程序放在 Windows\\Temp 里的临时文件。这里只算 7 天里都没动过的。", "Temporary files Windows and system services leave in Windows\\Temp. Only items untouched for 7 days are counted."),
      effect: bi("不影响已经装好的软件。要管理员权限。", "Installed software is not affected. Requires administrator rights."),
      evidence: "https://support.microsoft.com/en-us/windows/how-to-use-disk-cleanup-in-windows-8a96ff42-5751-39ad-23d6-434b4d5b9a68",
      roots: ["C:\\Windows\\Temp"], entries: [],
    }),
    item({
      id: "system.windows-update-downloads", category: "system", risk: "safe", action: "delete", admin: true,
      name: bi("Windows 更新下载的文件", "Windows Update downloads"),
      about: bi("Windows 更新下载下来、已经装完的更新包。", "Update packages Windows Update downloaded and has already installed."),
      effect: bi("已经装好的更新不受影响；还没装的会重新下载。正在用的文件会跳过。要管理员权限。", "Installed updates are not affected; pending ones are downloaded again. Files in use are skipped. Requires administrator rights."),
      evidence: "https://learn.microsoft.com/en-us/windows/deployment/update/windows-update-resources",
      roots: ["C:\\Windows\\SoftwareDistribution\\Download"],
      entries: [["C:\\Windows\\SoftwareDistribution\\Download\\4f1a8e0b2c", 1.1 * GB, 12], ["C:\\Windows\\SoftwareDistribution\\Download\\9c3d71aa0e", 380 * MB, 40]],
    }),
    item({
      id: "system.delivery-optimization", category: "system", risk: "safe", action: "delete", admin: true, size: null, note: "needs_admin",
      name: bi("传递优化缓存", "Delivery Optimization cache"),
      about: bi("Windows 更新和应用商店下载时留下的分片，用来分享给局域网里的其他电脑。", "Pieces of Windows Update and Store downloads kept to share with other PCs on your network."),
      effect: bi("不影响已经装好的更新和应用。用系统自带的命令清理，要管理员权限。", "Installed updates and apps are not affected. Cleaned with the built-in cmdlet; requires administrator rights."),
      evidence: "https://learn.microsoft.com/windows/deployment/do/waas-delivery-optimization-monitor",
      roots: ["C:\\Windows\\ServiceProfiles\\NetworkService\\AppData\\Local\\Microsoft\\Windows\\DeliveryOptimization\\Cache"], entries: [],
    }),
    item({
      id: "system.windows-old", category: "system", risk: "high", action: "delete", size: null, note: "system_tool", tool: "storage",
      name: bi("以前的 Windows 安装", "Previous Windows installation"),
      about: bi("升级 Windows 以后留下的旧系统（Windows.old），常常有十几 GB。删了就不能退回到以前的版本。", "The old system (Windows.old) left after a Windows upgrade, often over 10 GB. Once removed, you can't go back to the previous version."),
      effect: bi("要在系统设置的“临时文件”里勾选“以前的 Windows 安装”来删，ShibaPack 不直接删它。", "Remove it in Settings under Temporary files (Previous Windows installation). ShibaPack doesn't delete it directly."),
      evidence: "https://support.microsoft.com/en-us/windows/manage-drive-space-with-storage-sense-654f6ada-7bfc-45e5-966b-e24aded96ad5",
      roots: ["C:\\Windows.old"], entries: [],
    }),
    item({
      id: "system.thumbnail-db", category: "system", risk: "safe", action: "delete",
      name: bi("缩略图缓存", "Thumbnail cache"),
      about: bi("资源管理器为图片、视频生成的预览小图，存在 thumbcache 数据库里。", "Preview images File Explorer generates for pictures and videos, stored in the thumbcache databases."),
      effect: bi("下次打开文件夹时重新生成。资源管理器正在用的会跳过，通常要注销后再清才清得掉。", "Regenerated the next time you open a folder. Files in use by Explorer are skipped; signing out first usually frees them."),
      evidence: "https://learn.microsoft.com/en-us/windows/win32/shell/thumbnail-providers",
      roots: [join(LOCAL, "Microsoft", "Windows", "Explorer")],
      entries: [[join(LOCAL, "Microsoft", "Windows", "Explorer", "thumbcache_256.db"), 210 * MB, 0], [join(LOCAL, "Microsoft", "Windows", "Explorer", "thumbcache_1280.db"), 96 * MB, 0], [join(LOCAL, "Microsoft", "Windows", "Explorer", "thumbcache_96.db"), 24 * MB, 0]],
    }),
    item({
      id: "system.crash-reports", category: "system", risk: "recoverable", action: "trash",
      name: bi("崩溃报告", "Crash reports"),
      about: bi("程序崩溃时系统记下的报告和内存转储，转储文件常常很大。", "Reports and memory dumps the system records when a program crashes. Dumps are often large."),
      effect: bi("移到回收站。以后没法再拿这些报告查崩溃原因。", "Moved to the Recycle Bin/Trash. You can no longer use these reports to investigate crashes."),
      evidence: "https://learn.microsoft.com/en-us/windows/win32/wer/collecting-user-mode-dumps",
      roots: [join(LOCAL, "CrashDumps"), join(LOCAL, "Microsoft", "Windows", "WER", "ReportArchive")],
      entries: [[join(LOCAL, "CrashDumps", "explorer.exe.10236.dmp"), 412 * MB, 12], [join(LOCAL, "CrashDumps", "Steam.exe.8812.dmp"), 98 * MB, 30], [join(LOCAL, "Microsoft", "Windows", "WER", "ReportArchive", "AppCrash_steam.exe_3f2a"), 1 * MB, 30]],
    }),
    item({
      id: "browser.chrome", category: "browser", risk: "safe", action: "delete", app: "Google Chrome", running: true, quit: true,
      name: bi("Chrome 缓存", "Chrome cache"),
      about: bi("Chrome 存下的网页文件、脚本编译结果和显卡着色器，用来让常去的网站打开得快一点。", "Web files, compiled scripts and GPU shaders Chrome keeps so sites you visit often load faster."),
      effect: BROWSER_EFFECT, evidence: CHROMIUM_DOC,
      roots: [join(LOCAL, "Google", "Chrome", "User Data", "Default", "Cache"), join(LOCAL, "Google", "Chrome", "User Data", "Default", "Code Cache")],
      entries: [[join(LOCAL, "Google", "Chrome", "User Data", "Default", "Cache", "Cache_Data"), 1.02 * GB, 0], [join(LOCAL, "Google", "Chrome", "User Data", "Default", "Code Cache", "js"), 280 * MB, 0]],
    }),
    item({
      id: "browser.edge", category: "browser", risk: "safe", action: "delete", app: "Microsoft Edge", quit: true,
      name: bi("Edge 缓存", "Edge cache"),
      about: bi("Edge 存下的网页文件、脚本编译结果和显卡着色器，用来让常去的网站打开得快一点。", "Web files, compiled scripts and GPU shaders Edge keeps so sites you visit often load faster."),
      effect: BROWSER_EFFECT, evidence: CHROMIUM_DOC,
      roots: [join(LOCAL, "Microsoft", "Edge", "User Data", "Default", "Cache")],
      entries: [[join(LOCAL, "Microsoft", "Edge", "User Data", "Default", "Cache", "Cache_Data"), 455 * MB, 2]],
    }),
    item({
      id: "development.npm", category: "development", risk: "safe", action: "delete", app: "npm",
      name: bi("npm 缓存", "npm cache"),
      about: bi("npm 下载过的包。npm 会自己校验缓存，坏了也会重新下载。", "Packages npm has downloaded. npm verifies the cache and re-downloads anything missing."),
      effect: bi("下次安装依赖时重新下载。已经装在项目里的不受影响。", "Packages are downloaded again on next install. Dependencies already in projects are not affected."),
      evidence: "https://docs.npmjs.com/cli/commands/npm-cache",
      roots: [join(LOCAL, "npm-cache", "_cacache")],
      entries: [[join(LOCAL, "npm-cache", "_cacache", "content-v2"), 0.92 * GB, 1], [join(LOCAL, "npm-cache", "_cacache", "index-v5"), 71 * MB, 1]],
    }),
    item({
      id: "development.gradle", category: "development", risk: "recoverable", action: "trash", app: "Gradle",
      name: bi("Gradle 缓存", "Gradle caches"),
      about: bi("Gradle 下载的依赖、插件和编译缓存。Android 项目多的话会很大。", "Dependencies, plugins and build caches Gradle downloaded. Large with many Android projects."),
      effect: bi("移到回收站。下次编译要重新下载依赖。Gradle 后台进程正在用的会跳过。", "Moved to the Recycle Bin/Trash. Dependencies are downloaded again on the next build. Files held by a running Gradle daemon are skipped."),
      evidence: "https://docs.gradle.org/current/userguide/directory_layout.html",
      roots: [join(HOME, ".gradle", "caches")],
      entries: [[join(HOME, ".gradle", "caches", "modules-2"), 2.4 * GB, 3], [join(HOME, ".gradle", "caches", "transforms-4"), 1.1 * GB, 3], [join(HOME, ".gradle", "caches", "8.11.1"), 0.6 * GB, 3]],
    }),
    item({
      id: "development.vscode", category: "development", risk: "safe", action: "delete", app: "Visual Studio Code", quit: true,
      name: bi("VS Code 缓存", "VS Code cache"),
      about: bi("VS Code 的界面缓存、各版本编译好的代码、下载过的扩展安装包和日志。", "VS Code's interface cache, compiled code for each version, downloaded extension packages and logs."),
      effect: bi("下次打开会慢一点。设置、扩展、打开过的项目都不受影响。", "Opens a little slower next time. Settings, extensions and recent projects are not affected."),
      evidence: CHROMIUM_DOC,
      roots: [join(ROAMING, "Code", "CachedData"), join(ROAMING, "Code", "CachedExtensionVSIXs")],
      entries: [[join(ROAMING, "Code", "CachedExtensionVSIXs", "ms-python.python-2026.20.0"), 64 * MB, 8], [join(ROAMING, "Code", "CachedData", "1.106.0"), 23 * MB, 1]],
    }),
    item({
      id: "installers.winget", category: "installers", risk: "safe", action: "delete", app: "WinGet",
      name: bi("WinGet 下载的安装包", "WinGet downloads"),
      about: bi("WinGet 安装软件时下载的安装程序，装完常常留在临时文件夹里。", "Installers WinGet downloaded while installing software. They often stay in the temporary folder."),
      effect: bi("重新安装时要重新下载。已经装好的软件不受影响。", "Downloaded again when you reinstall. Installed software is not affected."),
      evidence: "https://learn.microsoft.com/en-us/windows/package-manager/winget/",
      roots: [join(LOCAL, "Temp", "WinGet")],
      entries: [[join(LOCAL, "Temp", "WinGet", "Git.Git.2.47.1"), 68 * MB, 11], [join(LOCAL, "Temp", "WinGet", "7zip.7zip.24.09"), 2 * MB, 20]],
    }),
    ...loose(),
    electron("apps.discord", "Discord", join(ROAMING, "discord"), [640, 120, 18]),
    history("privacy.edge-history", "Edge", "Microsoft Edge", join(LOCAL, "Microsoft", "Edge", "User Data", "Default")),
    cookies("privacy.edge-cookies", "Edge", "Microsoft Edge", join(LOCAL, "Microsoft", "Edge", "User Data", "Default")),
    item({
      id: "privacy.recent-files", category: "privacy", risk: "high", action: "delete",
      name: bi("最近使用的文件和跳转列表", "Recent files and jump lists"),
      about: bi("资源管理器“最近使用的文件”和任务栏右键菜单里的跳转列表。只是指向文件的快捷方式，文件本身不在这里。", "File Explorer's recent files and the jump lists on taskbar right-click menus. These are shortcuts; the files themselves are elsewhere."),
      effect: bi("最近使用的列表会清空，文件本身不受影响。资源管理器正在用的会跳过。", "The recent lists are cleared; the files themselves are not affected. Items in use by Explorer are skipped."),
      evidence: "https://learn.microsoft.com/en-us/windows/win32/shell/taskbar-extensions",
      roots: [join(ROAMING, "Microsoft", "Windows", "Recent")],
      entries: Array.from({ length: 36 }, (_, i) => [join(ROAMING, "Microsoft", "Windows", "Recent", `文档 ${i + 1}.lnk`), 1.5 * 1024, i % 20] as [string, number, number]),
    }),
    clipboard(),
    item({
      id: "trash.empty", category: "trash", risk: "high", action: "emptyTrash", size: 2.3 * GB,
      name: bi("回收站", "Recycle Bin / Trash"),
      about: bi("已经删除、还放在回收站（废纸篓）里的文件。", "Files you deleted that are still in the Recycle Bin or Trash."),
      effect: bi("清空以后就找不回来了。", "Once emptied, these files cannot be recovered."),
      roots: [], entries: [],
    }),
  ].map((it) => (it.id === "trash.empty" ? { ...it, files: 148, entryCount: 148 } : it));
}

/** 下载文件夹和桌面上的安装包。 */
function loose(): CleanItem[] {
  const downloads = join(HOME, "Downloads");
  const desktop = join(HOME, "Desktop");
  const roots = [downloads, desktop];
  const ext = windows ? "exe" : "dmg";
  return [
    item({
      id: "installers.in-library", category: "installers", risk: "recoverable", action: "trash",
      name: bi("仓库里已有的安装包", "Installers already in your library"),
      about: bi("下载文件夹和桌面上的这些安装包，和仓库里的文件一模一样（SHA-256 相同）。", "These installers in Downloads and on the Desktop are identical to files in your library (same SHA-256)."),
      effect: bi("移到回收站。仓库里的那一份不受影响。", "Moved to the Recycle Bin/Trash. The copy in your library is not affected."),
      roots,
      entries: [
        [join(downloads, `Blender-4.3.2.${windows ? "msi" : "dmg"}`), 302 * MB, 20, { software: "Blender", versions: ["4.3.2"], same: true }],
        [join(downloads, `Obsidian-1.7.7.${ext}`), 186 * MB, 35, { software: "Obsidian", versions: ["1.7.7"], same: true }],
        [join(desktop, `Telegram-5.9.0.${ext}`), 98 * MB, 12, { software: "Telegram", versions: ["5.9.0"], same: true }],
      ],
    }),
    item({
      id: "installers.not-in-library", category: "installers", risk: "recoverable", action: "trash", importable: true,
      name: bi("没收进仓库的安装包", "Installers not in your library"),
      about: bi("下载文件夹和桌面上还没收进仓库的安装包。可以先导入仓库，再清理这里的原件。", "Installers in Downloads and on the Desktop that are not in your library yet. You can import them first, then clean up the originals."),
      effect: bi("移到回收站。没导入仓库的话，以后要用得重新下载。", "Moved to the Recycle Bin/Trash. If you don't import them, you'll need to download them again later."),
      roots,
      entries: [
        [join(downloads, `Figma-126.1.${ext}`), 241 * MB, 3, { software: "Figma", versions: ["125.9.4"], same: false }],
        [join(downloads, `RustDesk-1.3.6.${ext}`), 28 * MB, 8, null],
        [join(downloads, "tools", `Wireshark-4.4.2.${ext}`), 92 * MB, 16, null],
        [join(desktop, `Node.js-22.12.0.${windows ? "msi" : "pkg"}`), 79 * MB, 2, { software: "Node.js", versions: ["22.12.0"], same: false }],
      ],
    }),
  ];
}

// ── 状态 ─────────────────────────────────────────────────────

let items: CleanItem[] = windows ? windowsItems() : macItems();
let options: CleanOptions = { custom: [], exclude: [], keepCookies: [], rules: [] };
const scanListeners = new Set<(progress: CleanProgress) => void>();
let scanCancelled = false;
const explained: Record<string, Explanation> = {};

export function mockOnCleanupScan(handler: (progress: CleanProgress) => void): () => void {
  scanListeners.add(handler);
  return () => scanListeners.delete(handler);
}
const COOKIE_SITES: CookieSite[] = [
  { site: "google.com", count: 186 },
  { site: "github.com", count: 64 },
  { site: "microsoft.com", count: 51 },
  { site: "bilibili.com", count: 47 },
  { site: "doubleclick.net", count: 33 },
  { site: "apple.com", count: 29 },
  { site: "zhihu.com", count: 22 },
  { site: "notion.so", count: 18 },
];
const runs: CleanRun[] = [];
let diskFree = 86.4 * GB;
const itemListeners = new Set<(item: CleanItem) => void>();
const progressListeners = new Set<(progress: CleanProgress) => void>();

export function mockOnCleanupItem(handler: (item: CleanItem) => void): () => void {
  itemListeners.add(handler);
  return () => itemListeners.delete(handler);
}

export function mockOnCleanupProgress(handler: (progress: CleanProgress) => void): () => void {
  progressListeners.add(handler);
  return () => progressListeners.delete(handler);
}

function customItems(): CleanItem[] {
  return options.rules.map((rule, i) => {
    const whole = rule.names.length === 0 && !rule.olderThanDays;
    const ext = rule.names[0]?.replace(/^\*/, "") ?? "";
    return item({
      id: `custom.${i}`, category: "custom", risk: "recoverable", action: rule.delete ? "delete" : "trash", quit: rule.processes.length > 0,
      name: bi(rule.name, rule.name),
      about: whole
        ? bi("你自己加的清理规则，这个文件夹里的东西都会清掉。", "A rule you added. Everything inside this folder is cleaned.")
        : bi("你自己加的清理规则，只清这个文件夹里符合条件的。", "A rule you added. Only matching items in this folder are cleaned."),
      effect: rule.delete
        ? bi("直接删除，不进回收站，删了找不回来。", "Deleted directly, skipping the Recycle Bin/Trash. It cannot be recovered.")
        : bi("移到回收站，需要时可以从回收站找回来。", "Moved to the Recycle Bin/Trash, where you can restore it if needed."),
      roots: [rule.path],
      entries: whole
        ? [[join(rule.path, "build"), 420 * MB, 10], [join(rule.path, "out.log"), 3 * MB, 10]]
        : [[join(rule.path, `app-2026-10${ext || ".log"}`), 180 * MB, 30], [join(rule.path, `app-2026-09${ext || ".log"}`), 96 * MB, 60]],
    });
  });
}

function excluded(path: string): boolean {
  return options.exclude.some((ex) => path === ex || path.startsWith(ex + sep));
}

function current(): CleanItem[] {
  return [...items, ...customItems()]
    .map((it) => {
      if (it.entries.length === 0) return it;
      const entries = it.entries.filter((e) => !excluded(e.path)).map((e, index) => ({ ...e, index }));
      return { ...it, entries, entryCount: entries.length, size: entries.reduce((sum, e) => sum + e.size, 0) };
    })
    // 读不到大小的（要管理员权限、要到系统设置里清的）没有条目也列出来
    .filter((it) => it.entryCount > 0 || it.id === "trash.empty" || it.action === "clearClipboard" || it.size === null);
}

export async function mockCleanupCall(command: string, args: Record<string, unknown>): Promise<unknown> {
  switch (command) {
    case "cleanup_scan": {
      const list = current();
      scanCancelled = false;
      // 一项一项量出来；取消了就停下，量完的照样返回
      const found: CleanItem[] = [];
      const total = list.length + 30;
      for (const [i, it] of list.entries()) {
        if (scanCancelled) break;
        await sleep(70 + Math.random() * 90);
        found.push(it);
        for (const handler of itemListeners) handler(it);
        for (const handler of scanListeners) handler({ done: Math.round(((i + 1) / list.length) * total), total, waitingAdmin: false });
      }
      return found;
    }
    case "cleanup_cancel_scan":
      scanCancelled = true;
      return null;
    case "cleanup_rescan":
      await sleep(250);
      return current();
    case "ai_explain_cleanup": {
      const ids = args.ids as string[];
      const out: Record<string, Explanation> = {};
      const todo = ids.filter((id) => args.mode === "refresh" || !explained[id]);
      for (const id of ids) if (explained[id] && args.mode !== "refresh") out[id] = { ...explained[id], cached: true };
      if (args.mode === "cached" || todo.length === 0) return { explanations: out, asked: 0, missing: 0 };
      await sleep(1100);
      for (const id of todo) {
        const it = current().find((x) => x.id === id);
        if (!it) continue;
        const zh = it.category === "custom"
          ? `看名字像是程序运行时写下的日志和构建产物，不是文档或数据库。删掉以后程序会重新生成，以前的日志就看不到了。`
          : `这是 ${it.app ?? "系统"} 为了加快速度存下的缓存，删掉以后会自己重新生成，第一次用会慢一点。`;
        const en = it.category === "custom"
          ? "These look like logs and build output a program writes while running, not documents or databases. They are regenerated; older logs are lost."
          : `This is cache ${it.app ?? "the system"} keeps for speed. It is regenerated after deletion; the first use is a little slower.`;
        explained[id] = { text: params.has("en") ? en : zh, model: "demo-model", at: new Date().toISOString(), cached: false };
        out[id] = explained[id];
      }
      return { explanations: out, asked: todo.length, missing: 0 };
    }
    case "cleanup_run": {
      const picks = args.picks as CleanPick[];
      const list = current();
      const report: CleanReport = { freed: 0, trashed: 0, removed: 0, items: [], skipped: [], skippedCount: 0, history: null, diskBefore: diskFree, diskAfter: diskFree };
      const total = picks.reduce((sum, p) => sum + Math.max(1, list.find((it) => it.id === p.id)?.entryCount ?? 0), 0);
      let done = 0;
      const run: CleanRun = { id: "", at: new Date().toISOString(), freed: 0, trashed: 0, items: [], skipped: 0 };
      for (const pick of picks) {
        const it = list.find((x) => x.id === pick.id);
        if (!it) continue;
        const result = { id: it.id, freed: 0, trashed: 0, removed: 0, skipped: 0 };
        if (it.admin) {
          // 预览里假装弹出了授权提示，等一会儿
          for (const handler of progressListeners) handler({ done, total, waitingAdmin: true });
          await sleep(1500);
        }
        if (it.tool) {
          report.skipped.push({ item: it.id, path: null, reason: "not_supported" });
          result.skipped++;
          done++;
        } else if (it.action === "clearClipboard") {
          result.removed = 1;
          done++;
        } else if (it.cookies && options.keepCookies.length > 0 && !it.running) {
          // 有保留名单：只删名单以外的，文件留着
          result.freed = Math.round((it.size ?? 0) * 0.7);
          result.removed = 1;
          done += it.entryCount;
        } else if (it.admin && it.size === null) {
          // 提权以后才量得到：预览里随便给个数
          result.freed = 640 * MB;
          result.removed = 7;
          done++;
          items = items.filter((x) => x.id !== it.id);
        } else if (it.running) {
          report.skipped.push({ item: it.id, path: null, reason: "app_running" });
          result.skipped++;
          done += Math.max(1, it.entryCount);
        } else if (it.action === "emptyTrash") {
          await sleep(500);
          result.freed = it.size ?? 0;
          result.removed = it.entryCount;
          done++;
          items = items.filter((x) => x.id !== it.id);
        } else {
          const gone = new Set<string>();
          for (const entry of it.entries) {
            done++;
            await sleep(25);
            for (const handler of progressListeners) handler({ done, total, waitingAdmin: false });
            if (pick.skip.includes(entry.index)) continue;
            // 预览里假装缩略图数据库被资源管理器占着
            if (it.id === "system.thumbnail-db" && entry.size < 100 * MB) {
              report.skipped.push({ item: it.id, path: entry.path, reason: "in_use" });
              result.skipped++;
              continue;
            }
            if (it.action === "delete") result.freed += entry.size;
            else result.trashed += entry.size;
            result.removed++;
            gone.add(entry.path);
          }
          items = items.map((x) => (x.id === it.id ? item({ ...x, size: undefined, entries: x.entries.filter((e) => !gone.has(e.path)).map((e) => [e.path, e.size, 1, x.id.startsWith("installers.") && x.id.endsWith("library") ? e.library : undefined]) }) : x));
          if (it.category === "custom") options = { ...options, rules: options.rules.filter((_, i) => `custom.${i}` !== it.id || it.entries.some((e) => !gone.has(e.path))) };
        }
        for (const handler of progressListeners) handler({ done, total, waitingAdmin: false });
        report.freed += result.freed;
        report.trashed += result.trashed;
        report.removed += result.removed;
        report.items.push(result);
        if (result.removed > 0) run.items.push({ id: it.id, name: it.name, freed: result.freed, trashed: result.trashed, removed: result.removed, paths: [], more: result.removed });
      }
      report.skippedCount = report.skipped.length;
      // 预览里假装系统报告的可用空间比我们算的少一点（别的程序同时在写）
      diskFree += Math.max(0, report.freed - 180 * MB);
      report.diskAfter = diskFree;
      run.freed = report.freed;
      run.trashed = report.trashed;
      run.skipped = report.skippedCount;
      if (run.items.length > 0 || run.skipped > 0) {
        run.id = new Date().toISOString().replace(/\D/g, "").slice(0, 14);
        runs.unshift(run);
        report.history = run.id;
      }
      return report;
    }
    case "cleanup_quit_app": {
      await sleep(1200);
      items = items.map((it) => (it.id === args.id ? { ...it, running: false } : it));
      const found = current().find((it) => it.id === args.id);
      if (!found) throw "not_found";
      return found;
    }
    case "cleanup_reveal":
      return null;
    case "cleanup_options":
      return options;
    case "cleanup_set_options": {
      const next = args.options as CleanOptions;
      const rules = [...next.custom.map((path) => ({ name: path.split(/[\\/]/).pop() || path, path, names: [], olderThanDays: null, processes: [], delete: false })), ...(next.rules ?? [])];
      for (const rule of rules) {
        const folder = rule.path;
        if (folder === REPO || folder === HOME || folder.endsWith(`${sep}Downloads`) || folder.endsWith(`${sep}Desktop`)) throw `cleanup_protected:${folder}`;
        for (const pattern of rule.names) if (/^[*?]+$/.test(pattern) || /[\\/]/.test(pattern)) throw `cleanup_bad_pattern:${pattern}`;
      }
      const keep: string[] = [];
      for (const raw of next.keepCookies ?? []) {
        const domain = raw.trim().toLowerCase().replace(/^[a-z]+:\/\//, "").split(/[/?#:]/)[0].replace(/^(\*\.|\.)/, "").replace(/^www\./, "");
        if (!/^[a-z0-9-]+(\.[a-z0-9-]+)+$/.test(domain)) throw `cleanup_bad_domain:${raw}`;
        if (!keep.includes(domain)) keep.push(domain);
      }
      options = { custom: [], exclude: [...new Set(next.exclude)], keepCookies: keep, rules: rules.map((rule) => ({ ...rule, name: rule.name.trim() || rule.path })) };
      return options;
    }
    case "cleanup_cookie_sites":
      await sleep(300);
      return COOKIE_SITES;
    case "cleanup_history":
      return runs;
    default:
      return null;
  }
}
