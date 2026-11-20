// 多处共用的操作：带确认的删除、安装与卸载、拖拽后的归类和排序。

import { confirm, prompt } from "../components/dialogs";
import { api, hostPlatform } from "./api";
import type { DropTarget } from "./drag";
import { translate } from "./i18n";
import { newerThan, primaryVersion, shownSummary, sortedVersions, withDescendants } from "./library";
import { usePrefs } from "./prefs";
import { useApp } from "./store";
import { openTraceDialog } from "./trace";
import type { Asset, Category, InstalledApp, PackageFile, Software, SoftwareEdit, Version } from "./types";

function language() {
  return useApp.getState().library?.settings.language ?? "zh-CN";
}

/** 问一句“要不要连磁盘上的文件一起删”。返回 null 表示取消。 */
async function askDelete(title: string): Promise<boolean | null> {
  const lang = language();
  const { ok, checked } = await confirm({
    title,
    confirmLabel: translate(lang, "common.delete"),
    danger: true,
    checkbox: {
      label: translate(lang, "del.alsoDisk"),
      hintOn: translate(lang, "del.diskHint"),
      hintOff: translate(lang, "del.recordOnly"),
    },
  });
  return ok ? checked : null;
}

export async function deleteSoftware(software: Software) {
  const withFiles = await askDelete(translate(language(), "del.software.title", { name: software.name }));
  if (withFiles !== null) await useApp.getState().apply(() => api.deleteSoftware(software.id, withFiles));
}

/** 多选删除：问一次，一次删完。 */
export async function deleteSoftwareMany(list: Software[]) {
  if (list.length === 0) return;
  if (list.length === 1) return deleteSoftware(list[0]);
  const withFiles = await askDelete(translate(language(), "batch.deleteTitle", { n: list.length }));
  if (withFiles === null) return;
  const app = useApp.getState();
  if (await app.apply(() => api.deleteSoftwareMany(list.map((s) => s.id), withFiles))) app.select(null);
}

/**
 * 删掉一批旧版本（清理旧版本用）。`targets` 可以跨软件。
 * 返回 true 表示真的删了；用户取消或出错时返回 false。
 */
export async function deleteVersions(targets: { software: Software; version: Version }[], withFiles: boolean): Promise<boolean> {
  if (targets.length === 0) return false;
  const app = useApp.getState();
  const ok = await app.apply(() =>
    api.deleteVersions(
      targets.map(({ software, version }) => ({ softwareId: software.id, versionId: version.id })),
      withFiles,
    ),
  );
  if (ok) app.notify(translate(language(), "prune.done", { n: targets.length }));
  return ok;
}

export async function deleteVersion(software: Software, version: Version) {
  const label = version.version || translate(language(), "detail.unknownVersion");
  const withFiles = await askDelete(translate(language(), "del.version.title", { version: label }));
  if (withFiles !== null) await useApp.getState().apply(() => api.deleteVersion(software.id, version.id, withFiles));
}

export async function deleteFile(software: Software, version: Version, file: PackageFile) {
  const withFiles = await askDelete(translate(language(), "del.file.title", { name: file.name }));
  if (withFiles !== null) {
    const ref = { softwareId: software.id, versionId: version.id, fileId: file.id };
    await useApp.getState().apply(() => api.deleteFile(ref, withFiles));
  }
}

// ── 简介 ─────────────────────────────────────────────────────

/** 自己写或改一个软件的描述（按名字存，仓库里的软件和只装在本机的软件都用它）。清空就删掉。 */
export async function editSummary(name: string) {
  const app = useApp.getState();
  const lang = language();
  const summary = app.library?.summaries[name];
  const translateOn = app.library?.settings.translateSummaries ?? true;
  const current = summary && summary.source !== "none" ? (shownSummary(summary, lang, translateOn)?.text ?? summary.text) : "";
  const text = await prompt({
    title: translate(lang, "summary.editTitle", { name }),
    label: translate(lang, "summary.editLabel"),
    placeholder: translate(lang, "summary.editPlaceholder"),
    initial: current,
    multiline: true,
  });
  if (text === null || text === current) return;
  await app.writeSummary(name, text);
}

// ── 安装与卸载 ───────────────────────────────────────────────

/** 一个版本里能在本机运行的安装包：本机平台的优先，其次是跨平台的。 */
export function installerIn(version: Version): PackageFile | null {
  return version.files.find((f) => f.platform === hostPlatform) ?? version.files.find((f) => f.platform === "cross") ?? null;
}

/** 在文件管理器里打开这个软件的文件夹：定位到它的默认版本（没设就是最新版本）里的安装包，本机能用的优先。 */
export async function revealSoftware(software: Software) {
  const version = primaryVersion(software);
  const file = version && (installerIn(version) ?? version.files[0]);
  if (!version || !file) return;
  await useApp.getState().attempt(() => api.revealFile({ softwareId: software.id, versionId: version.id, fileId: file.id }));
}

/** 这个软件里适合在本机运行的安装包：默认版本优先，其次从新到旧。 */
export function installerFor(software: Software): { version: Version; file: PackageFile } | null {
  const chosen = software.versions.find((v) => v.id === software.defaultVersionId);
  const versions = chosen ? [chosen, ...sortedVersions(software).filter((v) => v !== chosen)] : sortedVersions(software);
  for (const version of versions) {
    const file = installerIn(version);
    if (file) return { version, file };
  }
  return null;
}

/** 本机装的版本比仓库里能装的旧时，返回用来升级的安装包；否则 null。 */
export function upgradeFor(software: Software, installed: InstalledApp | undefined): { version: Version; file: PackageFile } | null {
  if (!installed?.version) return null;
  const pick = installerFor(software);
  return pick && newerThan(pick.version.version, installed.version) ? pick : null;
}

/** 运行安装包来安装或升级（先确认）。给了 `version` 就装那个版本，否则装默认版本。 */
export async function installSoftware(software: Software, upgrade = false, version?: Version) {
  const lang = language();
  const app = useApp.getState();
  const file = version ? installerIn(version) : null;
  const pick = version ? (file ? { version, file } : null) : installerFor(software);
  if (!pick) {
    app.notify(translate(lang, "link.noInstaller"), "error");
    return;
  }
  const action = translate(lang, upgrade ? "link.upgrade" : "link.install");
  const { ok, checked } = await confirm({
    title: action,
    body: upgrade
      ? translate(lang, "link.upgradeConfirm", { file: pick.file.name, name: software.name, version: pick.version.version })
      : translate(lang, "link.installConfirm", { file: pick.file.name, name: software.name }),
    confirmLabel: action,
    checkbox: {
      label: translate(lang, "link.traceThis"),
      hintOn: translate(lang, "link.traceHintOn"),
      hintOff: translate(lang, "link.traceHintOff"),
      initial: usePrefs.getState().traceInstalls,
    },
  });
  if (!ok) return;
  usePrefs.getState().set({ traceInstalls: checked });
  const ref = { softwareId: software.id, versionId: pick.version.id, fileId: pick.file.id };
  if (checked) openTraceDialog({ mode: "start", ...ref, fileName: pick.file.name, softwareName: software.name, version: pick.version.version });
  else await app.attempt(() => api.runFile(ref));
}

/**
 * 换默认版本：列表里显示、点“安装”时装的就是它。`null` 表示不固定，始终用最新的版本。
 */
export async function setDefaultVersion(software: Software, versionId: string | null) {
  if (software.defaultVersionId === versionId) return;
  const edit: SoftwareEdit = {
    name: software.name,
    publisher: software.publisher,
    homepage: software.homepage,
    note: software.note,
    categoryId: software.categoryId,
    starred: software.starred,
    defaultVersionId: versionId,
  };
  await useApp.getState().apply(() => api.updateSoftware(software.id, edit));
}

/**
 * 卸载这台电脑上的软件：打开卸载窗口。窗口先分析每个软件的数据、快捷方式和设置，勾选确认后再卸载、清理
 * （见 components/UninstallDialog）。可以一次卸载好几个。
 */
export async function uninstallApp(target: InstalledApp | InstalledApp[]) {
  const apps = (Array.isArray(target) ? target : [target]).filter((app) => app.uninstallable);
  if (apps.length > 0) useApp.getState().patch({ uninstallTarget: { apps, analyze: false } });
}

/** 只分析一个软件占了哪些地方，不卸载；窗口里可以再勾上软件本身去卸载，或者只清理数据。 */
export async function analyzeApp(target: InstalledApp) {
  useApp.getState().patch({ uninstallTarget: { apps: [target], analyze: true } });
}

/** 在线源上最新版本里，最适合这台电脑的安装包：系统对得上，架构一致或通用的优先。 */
export function bestAsset(software: Software): Asset | null {
  const assets = software.subscription?.latest?.assets ?? [];
  const arch = useApp.getState().status?.arch ?? "";
  const mine = assets.filter((asset) => asset.platform === hostPlatform);
  return (
    mine.find((asset) => asset.arch === arch) ??
    mine.find((asset) => asset.arch === "universal" || asset.arch === "") ??
    mine[0] ??
    assets.find((asset) => asset.platform === "cross") ??
    null
  );
}

export async function toggleStar(software: Software) {
  await useApp.getState().apply(() => api.setStarred(software.id, !software.starred));
}

/** 多选收藏或取消收藏。 */
/** 多选收藏或取消收藏：一次改完、只存一次，不会改到一半停下。 */
export async function starMany(list: Software[], starred: boolean) {
  const ids = list.filter((software) => software.starred !== starred).map((software) => software.id);
  if (ids.length > 0) await useApp.getState().apply(() => api.setStarredMany(ids, starred));
}

/** 多选移到一个分类（null 为未分类）：一次改完、只存一次，再说一声移到了哪里。 */
export async function moveManyTo(ids: string[], categoryId: string | null) {
  const app = useApp.getState();
  if (ids.length === 0) return;
  if (await app.apply(() => api.moveSoftware(ids, categoryId))) notifyMoved(ids.length, categoryId);
}

/** 拖动、批量移动之后的提示：移了几个、移到了哪里。 */
function notifyMoved(count: number, categoryId: string | null) {
  const app = useApp.getState();
  const name =
    categoryId === null
      ? translate(language(), "nav.uncategorized")
      : (app.library?.categories.find((c) => c.id === categoryId)?.name ?? "");
  app.notify(translate(language(), "move.done", { n: count, name }));
}

/** 键盘上的 Enter：本机比仓库旧就升级，没装就安装，已是最新就什么都不做。 */
export async function installOrUpgrade(software: Software) {
  const app = useApp.getState();
  const linked = app.links[software.id];
  const installed = linked ? app.installed?.find((item) => item.id === linked) : undefined;
  if (upgradeFor(software, installed)) await installSoftware(software, true);
  else if (!installed && installerFor(software)) await installSoftware(software);
}

/** 打开智能分类的预览窗口。 */
export function classify(softwareIds: string[]) {
  if (softwareIds.length > 0) useApp.getState().patch({ classifyIds: softwareIds });
}

// ── 拖拽落下之后 ─────────────────────────────────────────────

/** 把软件归到一个分类下（null 为未分类）。 */
export async function moveSoftwareTo(softwareId: string, categoryId: string | null) {
  const app = useApp.getState();
  const software = app.library?.software.find((s) => s.id === softwareId);
  if (!software || software.categoryId === categoryId) return;
  if (await app.apply(() => api.moveSoftware([software.id], categoryId))) notifyMoved(1, categoryId);
}

/** 分类能不能落在这个位置：不能落在自己或自己的下级上。 */
export function canMoveCategory(categories: Category[], id: string, target: DropTarget): boolean {
  return target.category !== null && !withDescendants(categories, id).has(target.category);
}

/** 拖动分类：放进另一个分类做下级，或者排到它的前面、后面。 */
export async function moveCategory(id: string, target: DropTarget) {
  const app = useApp.getState();
  const categories = app.library?.categories ?? [];
  const moving = categories.find((c) => c.id === id);
  const anchor = categories.find((c) => c.id === target.category);
  if (!moving || !anchor || !canMoveCategory(categories, id, target)) return;

  const rest = categories.filter((c) => c.id !== id);
  if (target.zone === "inside") {
    rest.push({ ...moving, parentId: anchor.id });
  } else {
    // 同级分类的先后就是它们在数组里的先后
    const at = rest.findIndex((c) => c.id === anchor.id) + (target.zone === "after" ? 1 : 0);
    rest.splice(at, 0, { ...moving, parentId: anchor.parentId });
  }
  await app.apply(() => api.setCategories(rest));
}
