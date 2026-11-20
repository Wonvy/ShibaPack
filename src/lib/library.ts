// 从索引数据里推导界面要用的信息：最新版本、大小、平台、分类树等。

import type { Asset, Category, Language, PackageFile, Platform, Release, Software, Summary, Version } from "./types";

/** 猜一段文字是什么语言：有假名是日文，汉字占了两成以上是中文，否则当英文。和后端的 `detect_language` 一致。 */
export function detectLanguage(text: string): string {
  let han = 0;
  let kana = 0;
  let letters = 0;
  for (const char of text) {
    if (/[\u3040-\u30ff]/.test(char)) kana++;
    else if (/[\u3400-\u4dbf\u4e00-\u9fff]/.test(char)) han++;
    else if (/\p{L}/u.test(char)) letters++;
  }
  if (kana > 0) return "ja";
  return han * 5 >= Math.max(1, han + letters) ? "zh" : "en";
}

const baseLanguage = (code: string) => code.split(/[-_]/)[0].toLowerCase();

/** 简介要不要翻译成界面语言：有内容、和界面不是同一种语言、还没有译文。 */
export function needsTranslation(summary: Summary, language: string): boolean {
  const lang = summary.lang || detectLanguage(summary.text);
  return summary.text.trim() !== "" && baseLanguage(lang) !== baseLanguage(language) && !summary.translations?.[language];
}

/** 界面上显示的简介：开着自动翻译、又有译文时用译文。 */
export function shownSummary(summary: Summary | undefined, language: string, translate: boolean): { text: string; translated: boolean } | null {
  if (!summary?.text) return null;
  const translation = translate ? summary.translations?.[language] : undefined;
  return translation ? { text: translation, translated: true } : { text: summary.text, translated: false };
}

/** 一个安装包在磁盘上的完整路径。托管文件记的是仓库里的相对路径，接上仓库根目录；分隔符跟着根目录走。 */
export function fullPath(file: PackageFile, root: string | null | undefined): string {
  if (file.storage === "indexed" || !root) return file.path;
  const windows = root.includes("\\");
  const separator = windows ? "\\" : "/";
  const rel = windows ? file.path.replaceAll("/", "\\") : file.path;
  return root.endsWith(separator) ? root + rel : root + separator + rel;
}

/** 悬停在版本号上时的提示：这个版本里每个安装包的完整路径，一行一个。 */
export function versionPaths(version: Version, root: string | null | undefined): string {
  return version.files.map((file) => fullPath(file, root)).join("\n");
}

/**
 * 拿已安装软件的名字查简介时用的名字：去掉括号里的说明、版本号、架构这些安装程序加上的东西。
 * “7-Zip 24.09 (x64)” → “7-Zip”，“Python 3.13.1 (64-bit)” → “Python”。
 */
export function summaryName(name: string): string {
  const cleaned = name
    .replace(/\s*[（(][^（）()]*[）)]/g, " ")
    .replace(/\b(x64|x86|amd64|arm64|aarch64|64-bit|32-bit|win64|win32)\b/gi, " ")
    .replace(/\bv?\d+(?:\.\d+)+[\w.-]*/gi, " ")
    .replace(/\s+-\s*$/, "")
    .replace(/\s+/g, " ")
    .trim();
  return cleaned || name.trim();
}

/** 软件库列表的筛选条件。 */
export interface ListFilter {
  scope: { kind: "all" | "starred" | "updates" | "uncategorized" } | { kind: "category"; id: string };
  platform: Platform | "all";
  installFilter: "all" | "installed" | "missing";
  /** 仓库里的软件 id → 对应的已安装软件 id。 */
  links: Record<string, string>;
  search: string;
}

/** 按左侧入口、平台、安装状态和搜索词筛出列表里的软件（不排序）。 */
export function filterSoftware(software: Software[], categories: Category[], filter: ListFilter): Software[] {
  const { scope, platform, installFilter, links, search } = filter;
  let items = software;
  if (scope.kind === "starred") items = items.filter((s) => s.starred);
  if (scope.kind === "updates") items = items.filter(hasUpdate);
  if (scope.kind === "uncategorized") items = items.filter((s) => !s.categoryId);
  if (scope.kind === "category") {
    const ids = withDescendants(categories, scope.id);
    items = items.filter((s) => s.categoryId !== null && ids.has(s.categoryId));
  }
  if (platform !== "all") items = items.filter((s) => softwarePlatforms(s).includes(platform));
  if (installFilter !== "all") items = items.filter((s) => s.id in links === (installFilter === "installed"));
  const query = search.trim().toLowerCase();
  if (query) {
    items = items.filter(
      (s) =>
        s.name.toLowerCase().includes(query) ||
        s.publisher.toLowerCase().includes(query) ||
        s.versions.some((v) => v.files.some((f) => f.name.toLowerCase().includes(query))),
    );
  }
  return items;
}

/**
 * 只保留最新 `keep` 个版本时要删掉的版本（从新到旧）。
 * 手动固定的默认版本、本机装着的那个版本（`protect`）总是留着。
 */
export function prunable(software: Software, keep: number, protect?: string): Version[] {
  return sortedVersions(software)
    .slice(Math.max(1, keep))
    .filter((v) => v.id !== software.defaultVersionId && !(protect && sameVersion(v.version, protect)));
}

export const PLATFORMS: Platform[] = ["windows", "macos", "linux", "android", "ios", "cross"];

/** 按数字段比较版本号："1.10" 比 "1.9" 新。 */
export function compareVersions(a: string, b: string): number {
  const split = (v: string) => v.split(/[.\-_+]/).map((part) => (/^\d+$/.test(part) ? Number(part) : part));
  const [pa, pb] = [split(a), split(b)];
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const [x, y] = [pa[i], pb[i]];
    if (x === y) continue;
    if (x === undefined) return -1;
    if (y === undefined) return 1;
    if (typeof x === "number" && typeof y === "number") return x - y;
    // 数字段比 beta、rc 这类词新
    if (typeof x === "number") return 1;
    if (typeof y === "number") return -1;
    return x < y ? -1 : 1;
  }
  return 0;
}

/**
 * `candidate` 是否比 `current` 新。两边可能带前后缀（“Build 4189”、“v1.2.3”），
 * 只比较里面第一段数字版本号；任何一边取不出数字就当作不新。
 */
export function newerThan(candidate: string, current: string): boolean {
  const core = (value: string) => value.match(/\d+(?:[._-]\d+)*/)?.[0] ?? null;
  const [a, b] = [core(candidate), core(current)];
  return a !== null && b !== null && compareVersions(a, b) > 0;
}

/** 两个版本号的数字部分是否相同（“1.95.3”和“v1.95.3”算同一个）。 */
export function sameVersion(a: string, b: string): boolean {
  const core = (value: string) => value.match(/\d+(?:[._-]\d+)*/)?.[0] ?? null;
  const [x, y] = [core(a), core(b)];
  return x !== null && y !== null && compareVersions(x, y) === 0;
}

/** 版本从新到旧。 */
export function sortedVersions(software: Software): Version[] {
  return [...software.versions].sort((a, b) => compareVersions(b.version, a.version));
}

/** 列表里代表这个软件的版本：手动设的默认版本，否则最新的。 */
export function primaryVersion(software: Software): Version | null {
  const chosen = software.versions.find((v) => v.id === software.defaultVersionId);
  return chosen ?? sortedVersions(software)[0] ?? null;
}

export function softwareSize(software: Software): number {
  return software.versions.reduce((sum, v) => sum + v.files.reduce((s, f) => s + f.size, 0), 0);
}

export function fileCount(software: Software): number {
  return software.versions.reduce((sum, v) => sum + v.files.length, 0);
}

export function softwarePlatforms(software: Software): Platform[] {
  const found = new Set(software.versions.flatMap((v) => v.files.map((f) => f.platform)));
  return PLATFORMS.filter((p) => found.has(p));
}

/**
 * 文件大小。按 1024 进位，但数字到了 1000 就换下一个单位：“1010 MB”读起来别扭，写成“1.0 GB”。
 * 100 以上不带小数，以下带一位。
 */
export function formatSize(bytes: number): string {
  // 四舍五入后会显示成 1000 的也算到了
  if (bytes < 999.5) return `${Math.round(bytes)} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 999.5 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value >= 99.95 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`;
}

export function formatDate(iso: string, language: Language, withTime = false): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  const options: Intl.DateTimeFormatOptions = withTime
    ? { year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" }
    : { year: "numeric", month: "2-digit", day: "2-digit" };
  try {
    return new Intl.DateTimeFormat(language, options).format(date);
  } catch {
    // 导入的语言包可能用了系统不认识的语言代码
    return new Intl.DateTimeFormat("en-US", options).format(date);
  }
}

/** 在线源上的最新版本是不是比仓库里的都新。 */
export function hasUpdate(software: Software): boolean {
  const latest = software.subscription?.latest;
  if (!latest) return false;
  return software.versions.every((v) => compareVersions(latest.version, v.version) > 0);
}

/** 在线源上的这个安装包，仓库里是不是已经有了。 */
export function assetInLibrary(software: Software, release: Release, asset: Asset): boolean {
  const local = software.versions.find((v) => v.version === release.version);
  if (!local) return false;
  return local.files.some((f) => f.name === asset.name || (f.platform === asset.platform && f.arch === asset.arch));
}

export function childrenOf(categories: Category[], parentId: string | null): Category[] {
  return categories.filter((c) => (c.parentId ?? null) === parentId);
}

/** 一个分类自己和它所有下级分类的 id。 */
export function withDescendants(categories: Category[], id: string): Set<string> {
  const ids = new Set([id]);
  let grew = true;
  while (grew) {
    grew = false;
    for (const category of categories) {
      if (category.parentId && ids.has(category.parentId) && !ids.has(category.id)) {
        ids.add(category.id);
        grew = true;
      }
    }
  }
  return ids;
}

/** "开发工具 / 编辑器" 这样的完整路径。 */
export function categoryPath(categories: Category[], id: string | null): string {
  const names: string[] = [];
  let current = categories.find((c) => c.id === id);
  while (current && names.length < 16) {
    names.unshift(current.name);
    const parentId = current.parentId;
    current = parentId ? categories.find((c) => c.id === parentId) : undefined;
  }
  return names.join(" / ");
}

/** 分类按树的顺序展开成一列，带缩进层级。用在下拉框里。 */
export function flattenTree(categories: Category[]): { category: Category; depth: number }[] {
  const out: { category: Category; depth: number }[] = [];
  const walk = (parentId: string | null, depth: number) => {
    for (const category of childrenOf(categories, parentId)) {
      out.push({ category, depth });
      if (depth < 16) walk(category.id, depth + 1);
    }
  };
  walk(null, 0);
  return out;
}

export function newId(): string {
  return crypto.randomUUID().replaceAll("-", "").slice(0, 12);
}
