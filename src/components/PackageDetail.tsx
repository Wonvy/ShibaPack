// 详情栏：只放常用的——简介、安装状态与新版本、各版本的安装包；
// 分类、在线源、主页、备注这些收在“更多信息”里，默认收起。

import {
  ArrowUp,
  ChevronRight,
  CircleCheck,
  Copy,
  Download,
  Ellipsis,
  ExternalLink,
  FolderOpen,
  MousePointerClick,
  Pencil,
  Play,
  Sparkles,
  Star,
  Trash2,
  X,
} from "lucide-react";
import {
  bestAsset,
  classify,
  deleteFile,
  deleteSoftware,
  deleteVersion,
  installSoftware,
  installerFor,
  toggleStar,
  uninstallApp,
} from "../lib/actions";
import { api, hostPlatform } from "../lib/api";
import { flattenTree, formatDate, formatSize, hasUpdate, primaryVersion, sortedVersions } from "../lib/library";
import { usePrefs } from "../lib/prefs";
import { useApp, useLanguage, useT } from "../lib/store";
import type { PackageFile, Software, SoftwareEdit, Version } from "../lib/types";
import { prompt } from "./dialogs";
import { PlatformChip } from "./PlatformIcon";
import { SoftwareIcon, useSoftwareIcon } from "./SoftwareIcon";
import { SourceSection } from "./SourcePanel";
import { Button, Chip, Dropdown, EmptyState, IconButton, Select, cx } from "./ui";

export function PackageDetail() {
  const t = useT();
  const software = useApp((state) => state.library?.software.find((s) => s.id === state.selectedId));
  if (!software) return <EmptyState icon={MousePointerClick} title={t("detail.empty")} />;
  // 换一个软件就整个重建，输入框里没保存的内容不会串到别的软件上
  return <Detail key={software.id} software={software} />;
}

function Detail({ software }: { software: Software }) {
  const t = useT();
  const language = useLanguage();
  const library = useApp((state) => state.library)!;
  const apply = useApp((state) => state.apply);
  const attempt = useApp((state) => state.attempt);
  const moreOpen = usePrefs((state) => state.detailMoreOpen);
  const setPrefs = usePrefs((state) => state.set);
  const icon = useSoftwareIcon(software);
  const summary = library.summaries[software.name];

  const save = (patch: Partial<SoftwareEdit>) => {
    const edit: SoftwareEdit = {
      name: software.name,
      publisher: software.publisher,
      homepage: software.homepage,
      note: software.note,
      categoryId: software.categoryId,
      starred: software.starred,
      defaultVersionId: software.defaultVersionId,
      ...patch,
    };
    return apply(() => api.updateSoftware(software.id, edit));
  };

  /** 失焦时保存；内容没变就不动。保存失败（比如重名）时恢复原值。 */
  const commit = (field: "name" | "publisher" | "homepage" | "note") => async (event: { currentTarget: HTMLInputElement | HTMLTextAreaElement }) => {
    const input = event.currentTarget;
    const value = field === "note" ? input.value : input.value.trim();
    if (value === software[field]) return;
    if (!(await save({ [field]: value }))) input.value = software[field];
  };
  const blurOnEnter = (event: React.KeyboardEvent<HTMLInputElement>) => event.key === "Enter" && event.currentTarget.blur();

  const main = primaryVersion(software);
  const homepage = /^https?:\/\//i.test(software.homepage) ? software.homepage : null;
  const plain = "w-full rounded-md border border-transparent bg-transparent px-1.5 outline-none transition-colors hover:border-line-strong focus:border-accent focus:bg-bg";
  const field = "selectable w-full rounded-md border border-line-strong bg-bg px-2.5 text-[13px] outline-none placeholder:text-fg-4 focus:border-accent";

  return (
    <div className="h-full overflow-y-auto px-4 pt-4 pb-8">
      {/* 头部：图标、名字、发布者；右边是收藏和更多操作 */}
      <div className="flex items-start gap-3">
        <SoftwareIcon name={software.name} icon={icon} size={44} />
        <div className="min-w-0 flex-1">
          <input
            aria-label={t("lib.col.name")}
            defaultValue={software.name}
            key={software.name}
            spellCheck={false}
            onBlur={commit("name")}
            onKeyDown={blurOnEnter}
            className={cx(plain, "-ml-1.5 h-7 text-[15px] font-semibold")}
          />
          <input
            aria-label={t("detail.publisher")}
            defaultValue={software.publisher}
            key={`p:${software.publisher}`}
            placeholder={t("detail.publisher")}
            spellCheck={false}
            onBlur={commit("publisher")}
            onKeyDown={blurOnEnter}
            className={cx(plain, "-ml-1.5 h-6 text-[12px] text-fg-2 placeholder:text-fg-4")}
          />
        </div>
        <div className="flex shrink-0 items-center">
          <IconButton
            label={software.starred ? t("star.remove") : t("star.add")}
            className={software.starred ? "text-warning hover:text-warning" : ""}
            onClick={() => void toggleStar(software)}
          >
            <Star size={16} strokeWidth={1.75} fill={software.starred ? "currentColor" : "none"} />
          </IconButton>
          <Dropdown
            items={[
              { label: t("classify.menu"), icon: Sparkles, onSelect: () => classify([software.id]) },
              { label: t("detail.openHomepage"), icon: ExternalLink, disabled: !homepage, onSelect: () => homepage && void attempt(() => api.openUrl(homepage)) },
              { label: t("detail.deleteSoftware"), icon: Trash2, danger: true, separated: true, onSelect: () => void deleteSoftware(software) },
            ]}
          >
            <IconButton label={t("common.edit")}>
              <Ellipsis size={16} strokeWidth={1.75} />
            </IconButton>
          </Dropdown>
        </div>
      </div>

      {summary?.text && (
        <p className="selectable m-0 mt-3 line-clamp-4 text-[13px] leading-relaxed text-fg-2" title={`${summary.text}\n\n${t("summary.source", { source: summary.source })}`}>
          {summary.text}
        </p>
      )}

      <StatusBlock software={software} />

      <Section title={`${t("detail.versions")} · ${software.versions.length}`}>
        {software.versions.length === 0 && <div className="text-[12px] text-fg-4">{t("detail.noFiles")}</div>}
        <div className="flex flex-col gap-3">
          {sortedVersions(software).map((version) => (
            <VersionBlock key={version.id} software={software} version={version} isMain={version.id === main?.id} onSave={save} />
          ))}
        </div>
      </Section>

      {/* 更多信息：不常改的都在这里 */}
      <button
        type="button"
        aria-expanded={moreOpen}
        onClick={() => setPrefs({ detailMoreOpen: !moreOpen })}
        className="mt-5 flex h-7 w-full items-center gap-1 rounded-md text-[12px] font-medium text-fg-3 hover:text-fg-2"
      >
        <ChevronRight size={14} strokeWidth={2} aria-hidden className={cx("transition-transform", moreOpen && "rotate-90")} />
        {t("detail.more")}
        <span className="ml-2 h-px flex-1 bg-line" />
      </button>
      {moreOpen && (
        <div className="flex flex-col gap-4 pt-2">
          <Labeled label={t("detail.category")}>
            <Select value={software.categoryId ?? ""} onChange={(event) => void save({ categoryId: event.target.value || null })}>
              <option value="">{t("nav.uncategorized")}</option>
              {flattenTree(library.categories).map(({ category, depth }) => (
                <option key={category.id} value={category.id}>
                  {"　".repeat(depth) + category.name}
                </option>
              ))}
            </Select>
          </Labeled>
          <Labeled label={t("source.title")}>
            <SourceSection software={software} />
          </Labeled>
          <Labeled label={t("detail.homepage")}>
            <div className="flex gap-1">
              <input
                aria-label={t("detail.homepage")}
                defaultValue={software.homepage}
                key={`h:${software.homepage}`}
                placeholder="https://"
                spellCheck={false}
                onBlur={commit("homepage")}
                onKeyDown={blurOnEnter}
                className={cx(field, "h-7")}
              />
              {homepage && (
                <IconButton label={t("detail.openHomepage")} onClick={() => void attempt(() => api.openUrl(homepage))}>
                  <ExternalLink size={14} strokeWidth={1.75} />
                </IconButton>
              )}
            </div>
          </Labeled>
          <Labeled label={t("detail.note")}>
            <textarea
              aria-label={t("detail.note")}
              defaultValue={software.note}
              key={`n:${software.note}`}
              rows={3}
              placeholder={t("detail.notePlaceholder")}
              onBlur={commit("note")}
              className={cx(field, "resize-y py-1.5 leading-relaxed")}
            />
          </Labeled>
          <div className="flex items-center justify-between text-[12px] text-fg-3">
            <span>{t("detail.addedAt")}</span>
            <span className="tabular">{formatDate(software.addedAt, language)}</span>
          </div>
        </div>
      )}
    </div>
  );
}

/** 安装状态和在线源上的新版本，各占一行；都没有就不显示。 */
function StatusBlock({ software }: { software: Software }) {
  const t = useT();
  const installedApp = useApp((state) => {
    const id = state.links[software.id];
    return id ? state.installed?.find((app) => app.id === id) : undefined;
  });
  const startDownload = useApp((state) => state.downloadAsset);
  const latest = software.subscription?.latest;
  const update = latest && hasUpdate(software) ? latest : null;
  const asset = update ? bestAsset(software) : null;
  const download = useApp((state) => (asset ? state.downloads[asset.url] : undefined));
  const installer = installerFor(software);
  if (!installedApp && !installer && !update) return null;

  const percent = download?.total ? Math.min(100, Math.round((download.received / download.total) * 100)) : null;
  const row = "flex min-h-8 items-center gap-2";
  return (
    <div className="mt-4 flex flex-col gap-1.5 rounded-lg bg-hover px-3 py-2">
      {(installedApp || installer) && (
        <div className={row}>
          {installedApp ? (
            <>
              <span className="flex min-w-0 flex-1 items-center gap-1.5 text-[13px] text-success">
                <CircleCheck size={15} strokeWidth={2} aria-hidden className="shrink-0" />
                <span className="truncate">
                  {installedApp.version ? t("link.installedVersion", { version: installedApp.version }) : t("link.installed")}
                </span>
              </span>
              {installedApp.uninstallable && (
                <Button variant="ghost" className="text-danger hover:text-danger" onClick={() => void uninstallApp(installedApp)}>
                  {t("link.uninstall")}
                </Button>
              )}
            </>
          ) : (
            <>
              <span className="min-w-0 flex-1 text-[13px] text-fg-3">{t("link.missing")}</span>
              <Button variant="primary" onClick={() => void installSoftware(software)}>
                <Download size={14} strokeWidth={2} aria-hidden />
                {t("link.install")}
              </Button>
            </>
          )}
        </div>
      )}
      {update && (
        <div className={row}>
          <span className="flex min-w-0 flex-1 items-center gap-1.5 text-[13px] text-accent">
            <ArrowUp size={15} strokeWidth={2.25} aria-hidden className="shrink-0" />
            <span className="truncate">{t("source.updateTo", { version: update.version })}</span>
          </span>
          {download ? (
            <>
              <span className="tabular text-[12px] text-accent">{percent !== null ? `${percent}%` : t("source.queued")}</span>
              <IconButton label={t("source.cancel")} onClick={() => void api.cancelDownload(asset!.url)}>
                <X size={15} strokeWidth={1.75} />
              </IconButton>
            </>
          ) : asset ? (
            <Button title={asset.name} onClick={() => void startDownload(software.id, asset)}>
              {t("source.download")}
            </Button>
          ) : null}
        </div>
      )}
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="mt-5">
      <h3 className="m-0 mb-2 text-[12px] font-medium text-fg-3">{title}</h3>
      {children}
    </section>
  );
}

function Labeled({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <div className="mb-1.5 text-[12px] text-fg-3">{label}</div>
      {children}
    </div>
  );
}

function VersionBlock({
  software,
  version,
  isMain,
  onSave,
}: {
  software: Software;
  version: Version;
  isMain: boolean;
  onSave: (patch: Partial<SoftwareEdit>) => Promise<boolean>;
}) {
  const t = useT();
  const apply = useApp((state) => state.apply);
  const pinned = software.defaultVersionId === version.id;

  const rename = async () => {
    const value = await prompt({ title: t("detail.editVersion"), label: t("detail.versionPrompt"), initial: version.version });
    if (value !== null && value !== version.version) {
      await apply(() => api.updateVersion(software.id, version.id, { version: value, note: version.note }));
    }
  };

  return (
    <div className="rounded-lg border border-line">
      <div className="flex h-8 items-center gap-2 border-b border-line pr-1 pl-2.5">
        <span className="tabular truncate text-[13px] font-medium">{version.version || t("detail.unknownVersion")}</span>
        {isMain && <Chip className={pinned ? "bg-accent-soft text-accent" : ""}>{t("detail.default")}</Chip>}
        <div className="ml-auto">
          <Dropdown
            items={[
              { label: t("detail.setDefault"), icon: Star, disabled: pinned, onSelect: () => void onSave({ defaultVersionId: version.id }) },
              { label: t("detail.editVersion"), icon: Pencil, onSelect: () => void rename() },
              { label: t("detail.deleteVersion"), icon: Trash2, danger: true, separated: true, onSelect: () => void deleteVersion(software, version) },
            ]}
          >
            <IconButton label={t("common.edit")} className="size-6">
              <Ellipsis size={15} strokeWidth={1.75} />
            </IconButton>
          </Dropdown>
        </div>
      </div>
      {version.files.map((file) => (
        <FileRow key={file.id} software={software} version={version} file={file} />
      ))}
    </div>
  );
}

function FileRow({ software, version, file }: { software: Software; version: Version; file: PackageFile }) {
  const t = useT();
  const attempt = useApp((state) => state.attempt);
  const notify = useApp((state) => state.notify);
  const ref = { softwareId: software.id, versionId: version.id, fileId: file.id };
  const runnable = file.platform === hostPlatform || file.platform === "cross";
  const platform = t(`platform.${file.platform}`);

  const copyHash = async () => {
    await navigator.clipboard.writeText(file.sha256);
    notify(t("detail.copied"));
  };

  return (
    <div className="flex items-center gap-1 border-b border-line py-1.5 pr-1 pl-2.5 last:border-b-0">
      <div className="min-w-0 flex-1">
        <div className="selectable truncate text-[12px]" title={file.path}>
          {file.name}
        </div>
        <div className="mt-1 flex flex-wrap items-center gap-1 text-[12px] text-fg-3">
          <PlatformChip platform={file.platform} />
          {file.arch && <Chip>{file.arch}</Chip>}
          <span className="tabular ml-0.5">{formatSize(file.size)}</span>
        </div>
      </div>
      <IconButton
        label={runnable ? t("detail.run") : t("detail.runOther", { platform })}
        disabled={!runnable}
        onClick={() => void attempt(() => api.runFile(ref))}
      >
        <Play size={15} strokeWidth={1.75} />
      </IconButton>
      <IconButton label={t("detail.reveal")} onClick={() => void attempt(() => api.revealFile(ref))}>
        <FolderOpen size={15} strokeWidth={1.75} />
      </IconButton>
      <Dropdown
        items={[
          { label: t("detail.copyHash"), icon: Copy, onSelect: () => void copyHash() },
          { label: t("detail.deleteFile"), icon: Trash2, danger: true, separated: true, onSelect: () => void deleteFile(software, version, file) },
        ]}
      >
        <IconButton label={t("common.edit")}>
          <Ellipsis size={15} strokeWidth={1.75} />
        </IconButton>
      </Dropdown>
    </div>
  );
}
