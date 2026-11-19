// 详情栏：一个软件的信息、分类与标签、各版本的安装包。

import { CircleCheck, Copy, Download, Ellipsis, ExternalLink, FolderOpen, MousePointerClick, Pencil, Play, Star, Trash2 } from "lucide-react";
import { deleteFile, deleteSoftware, deleteVersion, installSoftware, installerFor, uninstallApp } from "../lib/actions";
import { api, hostPlatform } from "../lib/api";
import { flattenTree, formatDate, formatSize, primaryVersion, sortedVersions } from "../lib/library";
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
  const icon = useSoftwareIcon(software);
  const summary = library.summaries[software.name];
  // 这台电脑上装着的同一个软件
  const installedApp = useApp((state) => {
    const id = state.links[software.id];
    return id ? state.installed?.find((app) => app.id === id) : undefined;
  });
  const installer = installerFor(software);

  const save = (patch: Partial<SoftwareEdit>) => {
    const edit: SoftwareEdit = {
      name: software.name,
      publisher: software.publisher,
      homepage: software.homepage,
      note: software.note,
      categoryId: software.categoryId,
      tagIds: software.tagIds,
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

  const toggleTag = (id: string) =>
    void save({ tagIds: software.tagIds.includes(id) ? software.tagIds.filter((x) => x !== id) : [...software.tagIds, id] });

  const main = primaryVersion(software);
  const plain = "w-full rounded-md border border-transparent bg-transparent px-1.5 outline-none transition-colors hover:border-line-strong focus:border-accent focus:bg-bg";

  return (
    <div className="flex h-full flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto px-4 pt-4 pb-4">
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
        </div>

        {summary?.text && (
          <div className="mt-4">
            <p className="selectable m-0 text-[13px] leading-relaxed text-fg-2">{summary.text}</p>
            <p className="m-0 mt-1 text-[11px] text-fg-4">{t("summary.source", { source: summary.source })}</p>
          </div>
        )}

        {(installedApp || installer) && (
          <div className="mt-4 flex items-center gap-2">
            {installedApp ? (
              <>
                <span className="flex min-w-0 flex-1 items-center gap-1.5 text-[12px] text-success">
                  <CircleCheck size={14} strokeWidth={2} aria-hidden className="shrink-0" />
                  <span className="truncate">
                    {installedApp.version ? t("link.installedVersion", { version: installedApp.version }) : t("link.installed")}
                  </span>
                </span>
                {installedApp.uninstallable && (
                  <Button className="text-danger" onClick={() => void uninstallApp(installedApp)}>
                    {t("link.uninstall")}
                  </Button>
                )}
              </>
            ) : (
              <Button variant="primary" onClick={() => void installSoftware(software)}>
                <Download size={14} strokeWidth={2} aria-hidden />
                {t("link.install")}
              </Button>
            )}
          </div>
        )}

        <Section title={t("detail.category")}>
          <Select value={software.categoryId ?? ""} onChange={(event) => void save({ categoryId: event.target.value || null })}>
            <option value="">{t("nav.uncategorized")}</option>
            {flattenTree(library.categories).map(({ category, depth }) => (
              <option key={category.id} value={category.id}>
                {"　".repeat(depth) + category.name}
              </option>
            ))}
          </Select>
        </Section>

        <Section title={t("detail.tags")}>
          {library.tags.length === 0 ? (
            <div className="text-[12px] text-fg-4">{t("detail.noTags")}</div>
          ) : (
            <div className="flex flex-wrap gap-1.5">
              {library.tags.map((tag) => {
                const on = software.tagIds.includes(tag.id);
                return (
                  <button
                    key={tag.id}
                    type="button"
                    aria-pressed={on}
                    onClick={() => toggleTag(tag.id)}
                    className={cx(
                      "inline-flex h-6 items-center gap-1.5 rounded-full border px-2 text-[12px] transition-colors",
                      on ? "border-transparent bg-active text-fg" : "border-line text-fg-3 hover:text-fg-2",
                    )}
                  >
                    <span className="size-2 rounded-full" style={{ background: tag.color ?? "var(--fg-3)", opacity: on ? 1 : 0.45 }} />
                    {tag.name}
                  </button>
                );
              })}
            </div>
          )}
        </Section>

        <Section title={`${t("detail.versions")} · ${software.versions.length}`}>
          {software.versions.length === 0 && <div className="text-[12px] text-fg-4">{t("detail.noFiles")}</div>}
          <div className="flex flex-col gap-3">
            {sortedVersions(software).map((version) => (
              <VersionBlock key={version.id} software={software} version={version} isMain={version.id === main?.id} onSave={save} />
            ))}
          </div>
        </Section>

        <Section title={t("source.title")}>
          <SourceSection software={software} />
        </Section>

        <Section title={t("detail.homepage")}>
          <div className="flex gap-1">
          <input
            aria-label={t("detail.homepage")}
            defaultValue={software.homepage}
            key={`h:${software.homepage}`}
            placeholder="https://"
            spellCheck={false}
            onBlur={commit("homepage")}
            onKeyDown={blurOnEnter}
            className="selectable h-7 w-full rounded-md border border-line-strong bg-bg px-2.5 text-[13px] outline-none placeholder:text-fg-4 focus:border-accent"
          />
          {/^https?:\/\//i.test(software.homepage) && (
            <IconButton label={t("detail.homepage")} onClick={() => void attempt(() => api.openUrl(software.homepage))}>
              <ExternalLink size={14} strokeWidth={1.75} />
            </IconButton>
          )}
          </div>
        </Section>

        <Section title={t("detail.note")}>
          <textarea
            aria-label={t("detail.note")}
            defaultValue={software.note}
            key={`n:${software.note}`}
            rows={3}
            placeholder={t("detail.notePlaceholder")}
            onBlur={commit("note")}
            className="w-full resize-y rounded-md border border-line-strong bg-bg px-2.5 py-1.5 text-[13px] leading-relaxed outline-none placeholder:text-fg-4 focus:border-accent"
          />
        </Section>
      </div>

      <div className="flex h-10 shrink-0 items-center justify-between border-t border-line pr-2 pl-4">
        <span className="text-[12px] text-fg-3">{t("detail.added", { date: formatDate(software.addedAt, language) })}</span>
        <Button variant="ghost" className="text-danger hover:text-danger" onClick={() => void deleteSoftware(software)}>
          <Trash2 size={14} strokeWidth={1.75} aria-hidden />
          {t("detail.deleteSoftware")}
        </Button>
      </div>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="mt-5">
      <h3 className="m-0 mb-2 text-[11px] font-medium text-fg-3">{title}</h3>
      {children}
    </section>
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
        <div className="mt-0.5 flex flex-wrap items-center gap-1 text-[11px] text-fg-3">
          <PlatformChip platform={file.platform} />
          {file.arch && <Chip>{file.arch}</Chip>}
          <Chip>{file.storage === "managed" ? t("detail.managed") : t("detail.indexed")}</Chip>
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
