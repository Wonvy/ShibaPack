// 与 crates/core/src/model.rs 对应的数据结构。

export type Platform = "windows" | "macos" | "linux" | "android" | "ios" | "cross";
export type Storage = "managed" | "indexed";
export type Theme = "system" | "dark" | "light";
/** 界面语言。内置 zh-CN 和 en-US，导入语言包后可以是别的代码。 */
export type Language = string;
export type SourceKind = "homebrew" | "github" | "winget";

export interface Category {
  id: string;
  name: string;
  parentId: string | null;
  color: string | null;
  icon: string | null;
}

export interface PackageFile {
  id: string;
  name: string;
  platform: Platform;
  arch: string;
  size: number;
  sha256: string;
  storage: Storage;
  path: string;
  addedAt: string;
}

export interface Version {
  id: string;
  version: string;
  note: string;
  addedAt: string;
  files: PackageFile[];
}

export interface Software {
  id: string;
  name: string;
  publisher: string;
  homepage: string;
  note: string;
  categoryId: string | null;
  starred: boolean;
  icon: string | null;
  iconData?: string | null;
  defaultVersionId: string | null;
  addedAt: string;
  versions: Version[];
  /** 关联的在线源。 */
  subscription?: Subscription | null;
}

/** 一个可以下载的安装包。 */
export interface Asset {
  name: string;
  url: string;
  platform: Platform;
  arch: string;
  size: number | null;
  sha256: string | null;
}

/** 在线源上的一个版本。 */
export interface Release {
  version: string;
  publishedAt: string;
  page: string;
  assets: Asset[];
}

export interface Subscription {
  source: SourceKind;
  reference: string;
  latest: Release | null;
  checkedAt: string;
}

/** 在 Homebrew 目录里搜到的一个软件。 */
export interface Candidate {
  reference: string;
  name: string;
  description: string;
  homepage: string;
  version: string;
}

/** 失败日志里的一条。 */
export interface FailureEntry {
  time: string;
  /** 哪一块：import、update、startup、library、installed…… */
  area: string;
  message: string;
  detail: string;
}

/** AI 对一个启动项的解释。`cached` 为真时是以前记下的，这次没有问 AI。 */
export interface Explanation {
  text: string;
  model: string;
  at: string;
  cached: boolean;
}

/** 一批启动项的 AI 解释：id → 解释。`asked` 是这次真的问了 AI 的项数，`missing` 是问了但没给出解释的。 */
export interface Explanations {
  explanations: Record<string, Explanation>;
  asked: number;
  missing: number;
}

/** AI 一批写简介的结果。 */
export interface DescribeResult {
  library: Library;
  written: number;
  unknown: number;
}

/** AI 分类的建议。`cached` 是其中有几个用的是上次 AI 的建议（这次没有问）。 */
export interface AiSuggestions {
  suggestions: Suggestion[];
  cached: number;
}

/** 顶栏搜索在 GitHub 上搜到的一个仓库。 */
export interface RepoHit {
  /** `owner/repo`，关联 GitHub Releases 时用的标识。 */
  reference: string;
  name: string;
  description: string;
  url: string;
  ownerAvatarUrl?: string;
  stars: number;
  /** 已经归档、不再维护。 */
  archived: boolean;
}

export interface SubscribeResult {
  library: Library;
  softwareId: string;
}

export interface CheckResult {
  library: Library;
  checked: number;
  failed: number;
  rateLimited: boolean;
}

export interface DownloadProgress {
  softwareId: string;
  url: string;
  name: string;
  received: number;
  total: number | null;
  done: boolean;
}

export interface CheckProgress {
  current: number;
  total: number;
  name: string;
}

/** 语言包：`_locale` 是语言代码，`_name` 是显示的名字，其余是“词条 → 文字”。 */
export type LangPack = Record<string, string>;

export interface Summary {
  text: string;
  source: string;
  fetchedAt: string;
  /** 原文的语言：zh、en、ja……旧索引里没有。 */
  lang?: string;
  /** 翻译成界面语言的译文：界面语言（如 zh-CN）→ 译文。 */
  translations?: Record<string, string>;
}

export interface Settings {
  theme: Theme;
  accent: string;
  language: Language;
  defaultStorage: Storage;
  /** 托管时导入后删掉原文件（移动进仓库）。 */
  moveOnImport: boolean;
  autoClassifyOnImport: boolean;
  /** 是否联网取软件简介（会把软件名发给维基百科和 GitHub）。 */
  fetchSummaries: boolean;
  /** 官网没有合适的图标时，从 theSVG 查找品牌图标。 */
  theSvgIcons: boolean;
  /** 简介不是界面语言时，联网翻译成界面语言。 */
  translateSummaries: boolean;
  /** 是否在仓库根目录生成离线目录 index.html。 */
  offlineCatalog: boolean;
  /** ShibaPack 发现自己有新版本时，是否自动下载并安装。 */
  autoUpdate: boolean;
  /** AI 模型：写简介、智能分类用。没设置时 baseUrl 为空。 */
  ai: AiSettings;
}

/** 用哪个 AI 服务和模型。API Key 不在这里，只存在每台电脑的系统钥匙串里。 */
export interface AiSettings {
  /** 预设的 id（deepseek、ollama……），自己填地址的是 custom。 */
  preset: string;
  /** OpenAI 兼容接口的地址。 */
  baseUrl: string;
  model: string;
}

/** AI 分类的进度。 */
export interface AiProgress {
  done: number;
  total: number;
}

/** 这台电脑上已安装的一个软件。 */
export interface InstalledApp {
  id: string;
  name: string;
  version: string;
  publisher: string;
  installedAt: string;
  /** 安装程序登记的大小（Windows 注册表）。实际占用另外统计，在 store 的 `sizes` 里，优先用它。 */
  size: number | null;
  source: "applications" | "user" | "system" | "registry" | "desktop" | "flatpak" | "snap";
  location: string;
  uninstallable: boolean;
  /** 图标文件的绝对路径。 */
  icon: string | null;
}

/** 一个启动项或服务。 */
export interface StartupItem {
  /** 稳定的标识：plist 的路径、注册表位置、服务名、单元名。 */
  id: string;
  /** startup：登录时自动运行的程序；service：后台服务。 */
  kind: "startup" | "service";
  /** 登记在哪一类地方。 */
  source: "loginItem" | "launchAgent" | "launchDaemon" | "registry" | "startupFolder" | "service" | "autostart" | "systemd";
  name: string;
  /** 系统内部的名字：launchd 的 Label、Windows 的服务名、systemd 的单元名、注册表的值名。 */
  label: string;
  /** 系统或程序自带的说明，没有时为空。 */
  description: string;
  publisher: string;
  command: string;
  /** 登记在哪：plist 文件、注册表键、快捷方式、单元文件。 */
  location: string;
  /** user：只对当前用户；machine：所有用户。 */
  scope: "user" | "machine";
  /** 系统自带的（Apple、Microsoft、发行版的核心服务）。 */
  system: boolean;
  /** 会不会自动运行。 */
  enabled: boolean;
  canToggle: boolean;
  /** 服务的启动方式。Windows：auto / delayed / manual / disabled；systemd：enabled / disabled / static / masked … */
  startType: string | null;
  /** 可以改成哪几种启动方式（只有 Windows 的服务有，别的用开关）。 */
  startTypes: string[];
  /** 正在运行；不知道时为 null。 */
  running: boolean | null;
  pid: number | null;
  canControl: boolean;
  /** 改动它要管理员权限，系统会弹出授权提示。 */
  needsAdmin: boolean;
  /** 它属于哪个应用。 */
  app: string;
  icon: string | null;
  /** 什么时候运行：login、load、keepAlive、interval:<秒>、calendar、watch、demand、hidden。 */
  triggers: string[];
  /** 以哪个账户运行。 */
  account: string;
}

/** 列启动项的结果。`notes` 是列的时候遇到的问题，比如 `login_items_denied`。 */
export interface StartupListing {
  items: StartupItem[];
  notes: string[];
}

/** 对启动项做的事。 */
export type StartupAction = "enable" | "disable" | "start" | "stop" | "restart" | `type:${"auto" | "delayed" | "manual" | "disabled"}`;

/** 卸载后找到的一项残留。 */
export interface Leftover {
  kind: "folder" | "file" | "registryKey" | "registryValue";
  /** 文件或文件夹的完整路径；注册表项是 `HKCU\Software\Foo` 这样的键；注册表值是它所在的键。 */
  path: string;
  /** 注册表值的名字。 */
  value?: string;
  confidence: "high" | "medium" | "low";
  /** installDir、bundleId、publisherName、name、nameContains、shortcut、uninstallKey、appPath、startup */
  reason: string;
  size: number | null;
}

export interface LeftoverScan {
  items: Leftover[];
  /** 软件看起来还在（卸载程序可能被取消了）。卸载前的分析里总是 false。 */
  stillInstalled: boolean;
  /** 卸载前的分析里“App”那一行显示的：Windows 是卸载命令，macOS 是 .app 的位置。 */
  appPath?: string;
}

export interface RemoveReport {
  removed: number;
  failed: { item: Leftover; reason: string }[];
  /** 有的项要管理员权限才删得掉。 */
  needsAdmin: boolean;
  /** 注册表备份所在的文件夹。 */
  backup: string | null;
}

export interface FillResult {
  library: Library | null;
  more: boolean;
}

export interface Library {
  format: number;
  categories: Category[];
  software: Software[];
  summaries: Record<string, Summary>;
  installedIcons?: Record<string, string>;
  settings: Settings;
  /** 收藏的路径（软件的配置文件夹这类）。旧索引里没有。 */
  paths?: SavedPath[];
}

/** 收藏的一个文件或文件夹。`path` 是可移植的写法（`~`、`%APPDATA%`），用的时候才展开。 */
export interface SavedPath {
  id: string;
  path: string;
  note: string;
  isDir: boolean;
  addedAt: string;
}

/** 一条收藏在这台电脑上的情况。 */
export interface PathState {
  /** 展开后的完整路径；有不认识的环境变量时是空的。 */
  full: string;
  exists: boolean;
  isDir: boolean;
  /** 展开不了的原因，比如 `path_unknown_var: XDG_CONFIG_HOME`。 */
  error: string | null;
  backups: number;
  lastBackup: string | null;
  backupSize: number;
}

/** 一次备份。 */
export interface PathBackup {
  name: string;
  createdAt: string;
  source: string;
  size: number;
  files: number;
  skipped: string[];
  skippedCount: number;
  beforeRestore: boolean;
}

export interface Status {
  configured: boolean;
  repoRoot: string | null;
  /** 启动时索引损坏并做了恢复：所用的备份文件名；空字符串表示没有可用备份。 */
  recovered: string | null;
  /** 上次用的仓库没打开时：它的路径，和原因（`missing` 表示文件夹不在，否则是错误代码）。 */
  lastRoot?: string | null;
  openError?: string | null;
  version: string;
  /** 这台电脑的处理器架构：x64 或 arm64。 */
  arch: string;
  /** 开发用的自检模式：值是要导入的一个文件的路径。平时是 null。 */
  selftest?: string | null;
}

export interface SoftwareEdit {
  name: string;
  publisher: string;
  homepage: string;
  note: string;
  categoryId: string | null;
  starred: boolean;
  defaultVersionId: string | null;
}

/** 智能分类推荐新建的分类。 */
export interface NewCategory {
  name: string;
  icon: string;
}

/** 智能分类对一个软件的建议。`categoryId` 和 `newCategory` 至多有一个，都没有表示判断不出来。 */
export interface Suggestion {
  softwareId: string;
  categoryId: string | null;
  newCategory: NewCategory | null;
  unchanged: boolean;
  known: boolean;
  evidence: string[];
  /** AI 给的依据：它认为这是个什么软件。内置规则的建议没有。 */
  reason?: string;
}

/** 预览里确认的一条分类。 */
export interface Assignment {
  softwareId: string;
  categoryId?: string;
  newCategory?: NewCategory;
}

export interface VersionEdit {
  version: string;
  note: string;
}

export interface ImportHint {
  name?: string;
  version?: string;
  platform?: Platform;
  /** 放进哪个分类：新建的软件放进去，已有的软件还没分类时也放进去。 */
  categoryId?: string;
  autoClassify?: boolean;
  /** 整个当作一个安装包导入的文件夹（绿色版软件等）。其余文件夹展开成里面的安装包。 */
  packages?: string[];
  entries?: Record<string, ImportEntryEdit>;
}

export interface ImportEntryEdit {
  name: string;
  version: string;
}

export interface ImportCategoryTarget {
  name: string;
  publisher: string;
}

export type ImportCategoryPreview = Pick<Suggestion, "categoryId" | "newCategory">;

/** 拖进来的一项，导入前在确认窗口里显示。 */
export interface DropEntry {
  path: string;
  name: string;
  isDir: boolean;
  size: number | null;
  /** 文件夹看起来是一个软件，建议整个导入。 */
  package: boolean;
  /** 文件夹里能找到的安装包个数。 */
  installers: number;
  /** 文件是认得的安装包。 */
  installer: boolean;
  guessName: string;
  guessVersion: string;
  filenameVersion: string;
  metadataVersion: string;
  matchedSoftwareId: string | null;
  publisher: string;
  iconData: string | null;
  children: DropEntry[];
}

export interface Progress {
  phase: "collect" | "import" | "classify" | "done";
  current: number;
  total: number;
  file: string;
}

export interface ImportItem {
  path: string;
  status: "imported" | "duplicate" | "failed";
  softwareId: string | null;
  /** 失败时系统或程序给的原话。 */
  message: string | null;
  /** 失败在哪一步：read 读取、copy 放进仓库、save 登记进索引。 */
  step?: string | null;
  /** 失败的原因代码（permission_denied、disk_full、in_use……），归不了类的是 io。 */
  reason?: string | null;
}

export interface ImportReport {
  classified?: number;
  classificationError?: string | null;
  imported: number;
  duplicates: number;
  failed: number;
  items: ImportItem[];
  /** 用户中途取消了，后面的文件没有处理。 */
  cancelled?: boolean;
}

export interface ImportResult {
  report: ImportReport;
  library: Library;
}

export interface Stats {
  software: number;
  versions: number;
  files: number;
  totalSize: number;
  managedSize: number;
  indexedSize: number;
  backups: number;
  trashSize: number;
}

export interface BackupInfo {
  name: string;
  createdAt: string;
  size: number;
}

export interface MissingFile {
  softwareId: string;
  versionId: string;
  fileId: string;
  software: string;
  version: string;
  name: string;
  path: string;
  reason: "missing" | "size";
}

export interface VerifyReport {
  checked: number;
  problems: MissingFile[];
}

// ── 清理 ─────────────────────────────────────────────────────

export type CleanCategory = "system" | "browser" | "apps" | "development" | "installers" | "privacy" | "trash" | "custom";
/** safe：删了会自己重新生成，默认勾选、直接删除；recoverable、high：默认不勾、移到回收站（high 确认时要再勾一次）。 */
export type CleanRisk = "safe" | "recoverable" | "high";
export type CleanAction = "delete" | "trash" | "emptyTrash" | "clearClipboard";

/** 清理规则自带的两种语言的文字，别的语言用英文。 */
export interface BiText {
  zh: string;
  en: string;
}

export interface CleanEntry {
  /** 在这一项里的序号。不想清某一条时把序号传回去。 */
  index: number;
  path: string;
  size: number;
  files: number;
  modified: number | null;
  /** 散落的安装包：仓库里对应的软件。`same` 表示和仓库里的文件一模一样。 */
  library: { software: string; versions: string[]; same: boolean } | null;
}

export interface CleanItem {
  id: string;
  category: CleanCategory;
  risk: CleanRisk;
  action: CleanAction;
  name: BiText;
  about: BiText;
  effect: BiText;
  app: string | null;
  evidence: string | null;
  roots: string[];
  /** 读不到时（macOS 的废纸篓要完全磁盘访问权限）是 null。 */
  size: number | null;
  files: number;
  /** 最大的在前，最多 200 条。 */
  entries: CleanEntry[];
  entryCount: number;
  modified: number | null;
  /** 要先关掉的程序正在运行。 */
  running: boolean;
  canQuit: boolean;
  /** `unreadable`、`full_disk_access`。 */
  note: string | null;
  /** 还没收进仓库的安装包，可以导入。 */
  importable: boolean;
  /** 要管理员权限：清理时一起交给提权的进程（Windows 弹 UAC，macOS 输入密码）。 */
  admin: boolean;
  /** 浏览器的 Cookie：有保留名单时只删名单以外的网站。 */
  cookies: boolean;
  /** 这里不清，要到系统设置里清（`storage`：存储设置）。 */
  tool: string | null;
}

export interface CleanPick {
  id: string;
  skip: number[];
}

export interface CleanSkipped {
  item: string;
  path: string | null;
  reason: string;
}

export interface CleanReport {
  freed: number;
  trashed: number;
  removed: number;
  items: { id: string; freed: number; trashed: number; removed: number; skipped: number }[];
  skipped: CleanSkipped[];
  skippedCount: number;
  history: string | null;
  /** 清理前后系统报告的可用空间（用户文件夹所在的盘）。量不出来时是 null。 */
  diskBefore: number | null;
  diskAfter: number | null;
}

export interface CleanOptions {
  custom: string[];
  exclude: string[];
  /** 清 Cookie 时保留这些网站（域名，包括子域名）。 */
  keepCookies: string[];
  /** 用户允许过控制访达：扫描时问访达废纸篓里有几项。 */
  askFinder?: boolean;
  /** 自己加的清理规则。旧版本里只有文件夹的 `custom` 存的时候会并进来。 */
  rules: CustomCleanRule[];
}

/** 自己加的一条清理规则：清某个文件夹下第一层的东西。 */
export interface CustomCleanRule {
  name: string;
  path: string;
  /** 只清名字符合这些通配的（`*.log`）。空的话文件夹里的都清。 */
  names: string[];
  /** 只清这么多天里都没动过的。 */
  olderThanDays: number | null;
  /** 清理前要先关掉的程序（进程名）。 */
  processes: string[];
  /** 直接删除，不进回收站。 */
  delete: boolean;
}

/** 浏览器里有 Cookie 的网站：给保留名单出主意。 */
export interface CookieSite {
  site: string;
  count: number;
}

export interface CleanRun {
  id: string;
  at: string;
  freed: number;
  trashed: number;
  items: { id: string; name: BiText; freed: number; trashed: number; removed: number; paths: string[]; more: number }[];
  skipped: number;
}

export interface CleanProgress {
  done: number;
  total: number;
  /** 正在等用户在系统的授权提示里输入密码或点“是”。 */
  waitingAdmin: boolean;
}

/** 主页：电脑名、系统、处理器、内存总量、开机时间（不会变的那些）。 */
export interface SystemOverview {
  hostName: string;
  /** 型号：“MacBook Pro (Mac16,1)”、主板登记的厂商和产品名。不知道时为空。 */
  model: string;
  /** Apple 芯片的性能核、能效核个数。 */
  coreSplit: [number, number] | null;
  /** 处理器频率（MHz），读不到时为 0。 */
  cpuMhz: number;
  gpus: { model: string; memory: number | null }[];
  /** 本机的硬盘，系统盘在前。 */
  disks: { name: string; mount: string; total: number; available: number; removable: boolean; kind: string }[];
  os: string;
  kernel: string;
  arch: string;
  cpuBrand: string;
  cpuThreads: number;
  cpuCores: number | null;
  totalMemory: number;
  totalSwap: number;
  /** Unix 秒。 */
  bootTime: number;
}

/** 某一刻的系统状态。 */
export interface SystemSample {
  /** 0–100 */
  cpu: number;
  memoryUsed: number;
  memoryTotal: number;
  swapUsed: number;
  swapTotal: number;
  disk: { mount: string; total: number; available: number } | null;
  uptime: number;
  processes: ProcessInfo[];
}

export interface ProcessInfo {
  pid: number;
  name: string;
  /** 占整台电脑 CPU 的百分比（已除以核心数）。 */
  cpu: number;
  memory: number;
  exe: string | null;
  /** 不能在这里结束：ShibaPack 自己、系统进程、别的用户的。 */
  protected: boolean;
}

/** 磁盘分析：可以分析的地方。 */
export interface DiskLocation {
  path: string;
  name: string;
  /** home：用户文件夹；disk：一块盘。 */
  kind: "home" | "disk";
  total: number;
  available: number;
}

export interface DiskSummary {
  root: string;
  size: number;
  files: number;
  /** 没有权限进去的文件夹个数。 */
  denied: number;
  deniedExamples: string[];
  elapsedMs: number;
}

export interface DiskProgress {
  files: number;
  dirs: number;
  bytes: number;
  current: string;
}

/** 磁盘分析画出来的一个节点。 */
export interface DiskNode {
  name: string;
  /** 相对扫描根的路径段；“小文件”“其他”为空。 */
  path: string[];
  /** small：一个文件夹里小于 1 MB 的文件合起来；other：太小、合起来显示的。 */
  kind: "dir" | "file" | "small" | "other";
  size: number;
  files: number;
  /** 还能往里点。 */
  hasChildren: boolean;
  /** 边扫边看时：这一项还没扫完，大小还会变。 */
  pending?: boolean;
  children: DiskNode[];
}

/** 磁盘分析里的一个大文件。 */
export interface LargeFile {
  /** 相对扫描根的路径段。 */
  path: string[];
  name: string;
  size: number;
  /** 所在文件夹（用户文件夹写成 ~）。 */
  folder: string;
  modifiedMs: number | null;
}

// ── 安装监控 ─────────────────────────────────────────────────

export type TraceDepth = "standard" | "deep";
export type TraceOp = "added" | "modified" | "removed";
export type TraceItemKind = "file" | "folder" | "registryKey" | "registryValue";
export type TraceCategory =
  | "app"
  | "data"
  | "settings"
  | "startup"
  | "service"
  | "driver"
  | "shortcut"
  | "association"
  | "environment"
  | "uninstall"
  | "other";

/** 安装程序做的一项改动。 */
export interface TraceChange {
  op: TraceOp;
  kind: TraceItemKind;
  /** 文件或文件夹的完整路径；注册表是键的路径。 */
  path: string;
  /** 注册表值的名字。 */
  value?: string;
  /** 注册表值现在的内容（截短的）。 */
  data?: string;
  size?: number;
  /** 新增或删除的文件夹、注册表键里一共有几项。 */
  count?: number;
  category: TraceCategory;
  /** 多半不是安装程序改的（缓存、日志、浏览器资料……）。 */
  noise: boolean;
  /** 安装程序自己登记过这一项（.pkg 安装收据、MSI 组件清单）。 */
  confirmed?: boolean;
}

/** 安装程序自己登记的产品。 */
export interface TraceProduct {
  kind: "pkg" | "msi";
  id: string;
  name: string;
  version: string;
  files: number;
}

/** 安装期间出现过、装完已经不在的文件。 */
export interface TraceTransient {
  path: string;
  category: TraceCategory;
  noise: boolean;
}

export interface LiveEvent {
  op: TraceOp;
  path: string;
  category: TraceCategory;
  atMs: number;
}

/** 安装时实时看到的文件改动。 */
export interface LiveSummary {
  files: number;
  counts: Partial<Record<TraceCategory, number>>;
  noise: number;
  /** 最近的几次改动，新的在前。 */
  recent: LiveEvent[];
}

/** 一次安装监控的记录。 */
export interface Trace {
  id: string;
  softwareId: string;
  versionId: string;
  softwareName: string;
  version: string;
  fileName: string;
  platform: string;
  depth: TraceDepth;
  startedAt: string;
  finishedAt: string;
  baselineMs: number;
  compareMs: number;
  entries: number;
  changes: TraceChange[];
  truncated: boolean;
  products?: TraceProduct[];
  transient?: TraceTransient[];
  transientCount?: number;
  /** 安装程序要求重启电脑。 */
  rebootRequired?: boolean;
  /** 重启电脑后接着完成的监控。 */
  resumed?: boolean;
}

export interface TraceSummary {
  id: string;
  softwareId: string;
  versionId: string;
  version: string;
  fileName: string;
  platform: string;
  startedAt: string;
  finishedAt: string;
  /** 不算噪音的改动有几项。 */
  changes: number;
  addedBytes: number;
}

/** 正在监控的安装。 */
export interface TraceStatus {
  softwareId: string;
  versionId: string;
  softwareName: string;
  version: string;
  fileName: string;
  elapsedMs: number;
  /** 安装程序还在不在跑；看不出来时是 null。 */
  installerRunning: boolean | null;
  /** 实时看到的文件改动；监听不了时是 null。 */
  live: LiveSummary | null;
  rebootRequired: boolean;
  /** ShibaPack 重新打开后接着的监控。 */
  resumed: boolean;
}

export interface TraceProgress {
  stage: "baseline" | "compare";
  entries: number;
}
