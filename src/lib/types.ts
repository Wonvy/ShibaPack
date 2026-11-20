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
  /** 是否联网取软件简介（会把软件名发给维基百科和 GitHub）。 */
  fetchSummaries: boolean;
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
  /** 软件看起来还在（卸载程序可能被取消了）。 */
  stillInstalled: boolean;
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
  settings: Settings;
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
}

export interface Progress {
  phase: "collect" | "import" | "done";
  current: number;
  total: number;
  file: string;
}

export interface ImportItem {
  path: string;
  status: "imported" | "duplicate" | "failed";
  softwareId: string | null;
  message: string | null;
}

export interface ImportReport {
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
