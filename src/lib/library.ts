// 从索引数据里推导界面要用的信息：最新版本、大小、平台、分类树等。

import type { Asset, Category, Language, Platform, Release, Software, Version } from "./types";

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

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value >= 100 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`;
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

/** 给没有图标的软件用的底色，同一个名字总是同一个颜色。 */
export function colorFor(name: string): string {
  const palette = ["#2f7fd6", "#7a5af5", "#1d9e75", "#d9542b", "#c9503f", "#3d6fa8", "#b0621b", "#8b5fbf", "#4a8f3c"];
  let hash = 0;
  for (const ch of name) hash = (hash * 31 + ch.codePointAt(0)!) >>> 0;
  return palette[hash % palette.length];
}
