// 多处共用的操作：带确认的删除、安装与卸载、拖拽后的归类和排序。

import { confirm } from "../components/dialogs";
import { api, hostPlatform } from "./api";
import type { DropTarget } from "./drag";
import { translate } from "./i18n";
import { sortedVersions, withDescendants } from "./library";
import { useApp } from "./store";
import type { Asset, Category, InstalledApp, PackageFile, Software, Version } from "./types";

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

// ── 安装与卸载 ───────────────────────────────────────────────

/** 这个软件里适合在本机运行的安装包：默认版本优先，其次从新到旧。 */
export function installerFor(software: Software): { version: Version; file: PackageFile } | null {
  const chosen = software.versions.find((v) => v.id === software.defaultVersionId);
  const versions = chosen ? [chosen, ...sortedVersions(software).filter((v) => v !== chosen)] : sortedVersions(software);
  for (const version of versions) {
    const file = version.files.find((f) => f.platform === hostPlatform) ?? version.files.find((f) => f.platform === "cross");
    if (file) return { version, file };
  }
  return null;
}

/** 运行安装包来安装（先确认）。 */
export async function installSoftware(software: Software) {
  const lang = language();
  const app = useApp.getState();
  const pick = installerFor(software);
  if (!pick) {
    app.notify(translate(lang, "link.noInstaller"), "error");
    return;
  }
  const { ok } = await confirm({
    title: translate(lang, "link.install"),
    body: translate(lang, "link.installConfirm", { file: pick.file.name, name: software.name }),
    confirmLabel: translate(lang, "link.install"),
  });
  if (ok) await app.attempt(() => api.runFile({ softwareId: software.id, versionId: pick.version.id, fileId: pick.file.id }));
}

/** 卸载一个已安装的软件（先确认）。 */
export async function uninstallApp(target: InstalledApp) {
  const lang = language();
  const app = useApp.getState();
  // Windows 上是启动卸载程序，别的系统上是移到废纸篓
  const viaUninstaller = target.source === "registry";
  const { ok } = await confirm({
    title: translate(lang, "installed.uninstall"),
    body: translate(lang, viaUninstaller ? "installed.uninstallConfirm.run" : "installed.uninstallConfirm.trash", { name: target.name }),
    confirmLabel: translate(lang, "installed.uninstall"),
    danger: true,
  });
  if (!ok) return;
  const done = await app.attempt(() => api.uninstallInstalled(target.id).then(() => true));
  if (!done) return;
  app.notify(translate(lang, viaUninstaller ? "installed.uninstallStarted" : "installed.uninstalled"));
  await app.loadInstalled();
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
  await app.apply(() =>
    api.updateSoftware(software.id, {
      name: software.name,
      publisher: software.publisher,
      homepage: software.homepage,
      note: software.note,
      categoryId,
      starred: software.starred,
      defaultVersionId: software.defaultVersionId,
    }),
  );
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
