// 软件详情从上到下显示日期、简介、安装状态、版本和在线源；
// 分类、主页、备注收在“更多信息”里。

import * as DropdownPrimitive from "@radix-ui/react-dropdown-menu";
import {
  ArrowUpCircle,
  ChevronRight,
  CircleCheck,
  CircleHelp,
  CircleDashed,
  CloudDownload,
  Copy,
  Download,
  Ellipsis,
  ExternalLink,
  FolderOpen,
  MousePointerClick,
  Eraser,
  Pencil,
  Pin,
  PinOff,
  Play,
  Plus,
  RefreshCw,
  Sparkles,
  Star,
  StickyNote,
  Trash2,
  X,
  PackageMinus,
  WandSparkles,
} from "lucide-react";
import {
  bestAsset,
  classify,
  deleteFile,
  deleteSoftware,
  deleteSoftwareMany,
  deleteVersion,
  deleteVersions,
  editSummary,
  installSoftware,
  installerFor,
  installerIn,
  moveManyTo,
  setDefaultVersion,
  starMany,
  toggleStar,
  uninstallApp,
  upgradeFor,
} from "../lib/actions";
import { useRef, useState } from "react";
import { api, hostPlatform, isMac, pickFiles } from "../lib/api";
import {
  flattenTree,
  formatDate,
  formatSize,
  hasUpdate,
  primaryVersion,
  prunable,
  sameVersion,
  sortedVersions,
} from "../lib/library";
import { usePrefs } from "../lib/prefs";
import { useAiReady, useApp, useLanguage, useT } from "../lib/store";
import type { InstalledApp, PackageFile, Software, SoftwareEdit, Version } from "../lib/types";
import { SummaryText } from "./SummaryText";
import { useSoftwareHover } from "./HoverCard";
import { prompt } from "./dialogs";
import { PackageFileIcon } from "./PlatformIcon";
import { RepositorySoftwareIcon } from "./SoftwareIcon";
import { SourceSection } from "./SourcePanel";
import { Button, Checkbox, Chip, ContextMenu, Dropdown, IconButton, type MenuEntry, MiddleTruncate, Modal, Select, cx, menuClass } from "./ui";

export function PackageDetail() {
  const multi = useApp((state) => state.selectedIds.length > 1);
  const software = useApp((state) => state.library?.software.find((s) => s.id === state.selectedId));
  if (multi) return <BatchPanel />;
  if (!software) return <Overview />;
  // 换一个软件就整个重建，输入框里没保存的内容不会串到别的软件上
  return <Detail key={software.id} software={software} />;
}

/** 没选中软件时详情栏显示的概况：待更新、本机可升级、未分类、最近添加，点一行就选中那个软件。 */
function Overview() {
  const t = useT();
  const language = useLanguage();
  const library = useApp((state) => state.library)!;
  const links = useApp((state) => state.links);
  const installed = useApp((state) => state.installed);
  const setScope = useApp((state) => state.setScope);
  const checkUpdates = useApp((state) => state.checkUpdates);
  const checking = useApp((state) => state.checking !== null);

  const all = library.software;
  const linked = all.some((s) => s.subscription);
  const updates = all.filter(hasUpdate);
  const upgrades = all.flatMap((software) => {
    const app = installed?.find((item) => item.id === links[software.id]);
    const pick = upgradeFor(software, app);
    return pick && app ? [{ software, from: app.version, to: pick.version.version }] : [];
  });
  const uncategorized = all.filter((s) => !s.categoryId);
  const recent = [...all].sort((a, b) => b.addedAt.localeCompare(a.addedAt)).slice(0, 5);
  const files = all.reduce((sum, s) => sum + s.versions.reduce((n, v) => n + v.files.length, 0), 0);
  const size = all.reduce((sum, s) => sum + s.versions.reduce((n, v) => n + v.files.reduce((m, f) => m + f.size, 0), 0), 0);
  const LIMIT = 5;

  return (
    <div className="h-full overflow-y-auto px-4 pt-4 pb-8">
      <h3 className="m-0 text-[14px] font-semibold">{t("overview.title")}</h3>
      <div className="tabular mt-1 text-[12px] text-fg-3">
        {t("status.software", { n: all.length })} · {t("status.files", { n: files })} · {formatSize(size)}
      </div>

      <OverviewSection
        title={t("nav.updates")}
        count={updates.length}
        accent
        action={
          linked && (
            <Button variant="ghost" busy={checking} onClick={() => void checkUpdates()}>
              {!checking && <RefreshCw size={13} strokeWidth={1.75} aria-hidden />}
              {t("overview.check")}
            </Button>
          )
        }
      >
        {updates.length === 0 ? (
          <div className="py-1 text-[12px] text-fg-3">{linked ? t("overview.upToDate") : t("updates.none")}</div>
        ) : (
          <>
            {updates.slice(0, LIMIT).map((software) => (
              <OverviewRow key={software.id} software={software} detail={`→ ${software.subscription!.latest!.version}`} tone="text-accent" />
            ))}
            {updates.length > LIMIT && <MoreRow n={updates.length} onClick={() => setScope({ kind: "updates" })} />}
          </>
        )}
      </OverviewSection>

      {upgrades.length > 0 && (
        <OverviewSection title={t("overview.upgrades")} count={upgrades.length}>
          {upgrades.slice(0, LIMIT).map(({ software, from, to }) => (
            <OverviewRow key={software.id} software={software} detail={`${from} → ${to}`} tone="text-warning" />
          ))}
        </OverviewSection>
      )}

      {uncategorized.length > 0 && (
        <OverviewSection
          title={t("nav.uncategorized")}
          count={uncategorized.length}
          action={
            <Button variant="ghost" onClick={() => classify(uncategorized.map((s) => s.id))}>
              <Sparkles size={13} strokeWidth={1.75} aria-hidden />
              {t("classify.menu")}
            </Button>
          }
        >
          {uncategorized.slice(0, LIMIT).map((software) => (
            <OverviewRow key={software.id} software={software} detail={software.publisher} />
          ))}
          {uncategorized.length > LIMIT && <MoreRow n={uncategorized.length} onClick={() => setScope({ kind: "uncategorized" })} />}
        </OverviewSection>
      )}

      <OverviewSection title={t("overview.recent")}>
        {recent.map((software) => (
          <OverviewRow key={software.id} software={software} detail={formatDate(software.addedAt, language)} />
        ))}
      </OverviewSection>

      <div className="mt-6 flex items-center justify-center gap-1.5 text-[12px] text-fg-4">
        <MousePointerClick size={13} strokeWidth={1.75} aria-hidden />
        {t("detail.empty")}
        <ShortcutHelp />
      </div>
    </div>
  );
}

/** “?”按钮：点开是软件列表能用的快捷键。 */
function ShortcutHelp() {
  const t = useT();
  const mod = isMac ? "⌘" : "Ctrl";
  const click = t("keys.click");
  const rows: [string, string[]][] = [
    [t("keys.move"), ["↑", "↓"]],
    [t("keys.toggle"), [mod, click]],
    [t("keys.range"), ["Shift", click]],
    [t("keys.all"), [mod, "A"]],
    [t("keys.install"), ["Enter"]],
    [t("keys.delete"), ["Delete"]],
    [t("keys.clear"), ["Esc"]],
    [t("keys.search"), [mod, "F"]],
  ];
  return (
    <DropdownPrimitive.Root>
      <DropdownPrimitive.Trigger asChild>
        <IconButton label={t("keys.title")} className="-my-1 size-6 text-fg-3 data-[state=open]:bg-active data-[state=open]:text-fg">
          <CircleHelp size={14} strokeWidth={1.75} />
        </IconButton>
      </DropdownPrimitive.Trigger>
      <DropdownPrimitive.Portal>
        <DropdownPrimitive.Content side="top" align="end" sideOffset={6} className={cx(menuClass, "w-[248px] p-2")}>
          <DropdownPrimitive.Label className="pb-1.5 text-[12px] font-medium text-fg-3">{t("keys.title")}</DropdownPrimitive.Label>
          <dl className="m-0 flex flex-col gap-1">
            {rows.map(([what, keys]) => (
              <div key={what} className="flex min-h-6 items-center justify-between gap-3">
                <dt className="text-[12px] text-fg-2">{what}</dt>
                <dd className="m-0 flex shrink-0 items-center gap-1">
                  {keys.map((key) => (
                    <kbd key={key} className="rounded border border-line-strong px-1.5 font-sans text-[12px] leading-5 text-fg-2">
                      {key}
                    </kbd>
                  ))}
                </dd>
              </div>
            ))}
          </dl>
        </DropdownPrimitive.Content>
      </DropdownPrimitive.Portal>
    </DropdownPrimitive.Root>
  );
}

function OverviewSection({
  title,
  count,
  accent,
  action,
  children,
}: {
  title: string;
  count?: number;
  accent?: boolean;
  action?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section className="mt-5">
      <div className="mb-1 flex min-h-7 items-center gap-1.5">
        <h4 className="m-0 text-[12px] font-medium text-fg-3">{title}</h4>
        {count !== undefined && count > 0 && <span className={cx("tabular text-[12px]", accent ? "font-medium text-accent" : "text-fg-3")}>{count}</span>}
        <span className="flex-1" />
        {action}
      </div>
      <div className="flex flex-col">{children}</div>
    </section>
  );
}

function OverviewRow({ software, detail, tone }: { software: Software; detail: string; tone?: string }) {
  // 当前列表被筛选挡住时，reveal 会先回到“全部软件”，免得选中了却在列表里找不到
  const reveal = useApp((state) => state.reveal);
  return (
    <button
      type="button"
      onClick={() => reveal(software.id)}
      className="-mx-2 flex h-8 items-center gap-2 rounded-md px-2 text-left text-[13px] transition-colors hover:bg-hover"
    >
      <RepositorySoftwareIcon software={software} size={20} />
      <span className="min-w-0 flex-1 truncate">{software.name}</span>
      <span className={cx("tabular max-w-[50%] shrink-0 truncate text-[12px]", tone ?? "text-fg-3")}>{detail}</span>
    </button>
  );
}

function MoreRow({ n, onClick }: { n: number; onClick: () => void }) {
  const t = useT();
  return (
    <button type="button" onClick={onClick} className="-mx-2 h-7 rounded-md px-2 text-left text-[12px] text-fg-3 hover:bg-hover hover:text-fg-2">
      {t("overview.more", { n })}
    </button>
  );
}

/** 多选时的详情栏：一起收藏、移动分类、智能分类、删除。 */
function BatchPanel() {
  const t = useT();
  const library = useApp((state) => state.library)!;
  const ids = useApp((state) => state.selectedIds);
  const select = useApp((state) => state.select);
  const chosen = library.software
    .filter((s) => ids.includes(s.id))
    .sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" }));
  const versions = chosen.reduce((sum, s) => sum + s.versions.length, 0);
  const files = chosen.reduce((sum, s) => sum + s.versions.reduce((n, v) => n + v.files.length, 0), 0);
  const size = chosen.reduce((sum, s) => sum + s.versions.reduce((n, v) => n + v.files.reduce((m, f) => m + f.size, 0), 0), 0);
  const allStarred = chosen.every((s) => s.starred);
  const aiReady = useAiReady();
  const describeWithAi = useApp((state) => state.describeWithAi);
  const writing = useApp((state) => chosen.some((s) => state.aiBusy[s.name]));

  return (
    <div className="flex h-full flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto px-4 pt-4 pb-4">
        <div className="flex items-start gap-2">
          <div className="min-w-0 flex-1">
            <h3 className="m-0 text-[15px] font-semibold">{t("batch.title", { n: chosen.length })}</h3>
            <div className="tabular mt-1 text-[12px] text-fg-3">{t("batch.summary", { versions, files, size: formatSize(size) })}</div>
          </div>
          <Button variant="ghost" size="sm" onClick={() => select(null)}>
            {t("batch.clear")}
          </Button>
        </div>

        <div className="mt-4 flex flex-col gap-2">
          <Button className="w-full" onClick={() => void starMany(chosen, !allStarred)}>
            <Star size={14} strokeWidth={1.75} fill={allStarred ? "currentColor" : "none"} aria-hidden />
            {allStarred ? t("batch.unstar") : t("batch.star")}
          </Button>
          <Select
            aria-label={t("batch.move")}
            value=""
            onChange={(event) => {
              const value = event.target.value;
              if (value) void moveManyTo(ids, value === "__none" ? null : value);
            }}
          >
            <option value="" disabled>
              {t("batch.move")}
            </option>
            <option value="__none">{t("nav.uncategorized")}</option>
            {flattenTree(library.categories).map(({ category, depth }) => (
              <option key={category.id} value={category.id}>
                {"　".repeat(depth) + category.name}
              </option>
            ))}
          </Select>
          <Button className="w-full" onClick={() => classify(ids)}>
            <Sparkles size={14} strokeWidth={1.75} aria-hidden />
            {t("classify.menu")}
          </Button>
          {aiReady && (
            <Button className="w-full" busy={writing} onClick={() => void describeWithAi(chosen.map((s) => ({ name: s.name })))}>
              {!writing && <WandSparkles size={14} strokeWidth={1.75} aria-hidden />}
              {t("ai.describe")}
            </Button>
          )}
        </div>

        <section className="mt-5">
          <h4 className="m-0 mb-1 text-[12px] font-medium text-fg-3">{t("batch.list")}</h4>
          <div className="flex flex-col">
            {chosen.map((software) => (
              <OverviewRow key={software.id} software={software} detail={primaryVersion(software)?.version ?? ""} />
            ))}
          </div>
        </section>
      </div>
      <div className="flex shrink-0 flex-col gap-2 border-t border-line p-3">
        <Button variant="caution" className="w-full" onClick={() => void deleteSoftwareMany(chosen)}>
          <Trash2 size={14} strokeWidth={1.75} aria-hidden />
          {t("batch.delete", { n: chosen.length })}
        </Button>
        <div className="text-center text-[12px] text-fg-4">{t("batch.keys")}</div>
      </div>
    </div>
  );
}

function Detail({ software }: { software: Software }) {
  const t = useT();
  const language = useLanguage();
  const library = useApp((state) => state.library)!;
  const apply = useApp((state) => state.apply);
  const attempt = useApp((state) => state.attempt);
  const summary = library.summaries[software.name];
  const summaryBusy = useApp((state) => Boolean(state.summaryBusy[software.name]));
  const requestSummary = useApp((state) => state.requestSummary);
  const installedApp = useApp((state) => {
    const id = state.links[software.id];
    return id ? state.installedIndex[id] : undefined;
  });

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

  const homepage = /^https?:\/\//i.test(software.homepage) ? software.homepage : null;
  const plain = "w-full rounded-md border border-transparent bg-transparent px-1.5 outline-none transition-colors hover:border-line-strong focus:border-accent focus:bg-bg";

  return (
    <div className="h-full overflow-y-auto px-4 pt-4 pb-8">
      {/* 头部：图标、名字、发布者；右边是收藏和更多操作 */}
      <div className="flex items-start gap-3">
        <RepositorySoftwareIcon software={software} size={44} />
        <div className="min-w-0 flex-1">
          <EditableTitle value={software.name} label={t("lib.col.name")} onCommit={(name) => save({ name })} />
          <input
            aria-label={t("detail.publisher")}
            defaultValue={software.publisher}
            key={`p:${software.publisher}`}
            placeholder={t("detail.publisher")}
            spellCheck={false}
            onBlur={(event) => {
              const value = event.currentTarget.value.trim();
              if (value !== software.publisher) void save({ publisher: value });
            }}
            onKeyDown={(event) => event.key === "Enter" && event.currentTarget.blur()}
            className={cx(plain, "-ml-1.5 h-6 text-[12px] text-fg-3 placeholder:text-fg-4")}
          />
        </div>
        <div className="-mr-1 flex shrink-0 items-center">
          <IconButton
            label={software.starred ? t("star.remove") : t("star.add")}
            className={software.starred ? "text-warning hover:text-warning" : ""}
            onClick={() => void toggleStar(software)}
          >
            <Star size={16} strokeWidth={1.75} fill={software.starred ? "currentColor" : "none"} />
          </IconButton>
          <Dropdown
            items={[
              { label: t("summary.editMenu"), icon: Pencil, onSelect: () => void editSummary(software.name) },
              { label: t("classify.menu"), icon: Sparkles, onSelect: () => classify([software.id]) },
              { label: t("detail.openHomepage"), icon: ExternalLink, disabled: !homepage, onSelect: () => homepage && void attempt(() => api.openUrl(homepage)) },
              ...(installedApp?.uninstallable
                ? [{ label: t("link.uninstall"), icon: PackageMinus, separated: true, onSelect: () => void uninstallApp(installedApp) }]
                : []),
              { label: t("detail.deleteSoftware"), icon: Trash2, danger: true, separated: !installedApp?.uninstallable, onSelect: () => void deleteSoftware(software) },
            ]}
          >
            <IconButton label={t("common.edit")}>
              <Ellipsis size={16} strokeWidth={1.75} />
            </IconButton>
          </Dropdown>
        </div>
      </div>

      <dl className="m-0 mt-2 flex flex-wrap gap-x-4 gap-y-1 text-[12px]">
        {installedApp && (
          <div className="flex items-baseline gap-1.5">
            <dt className="text-fg-3">{t("installed.date")}</dt>
            <dd className="tabular m-0 text-fg-2">{formatDate(installedApp.installedAt, language) || t("installed.unknown")}</dd>
          </div>
        )}
        <div className="flex items-baseline gap-1.5">
          <dt className="text-fg-3">{t("detail.addedAt")}</dt>
          <dd className="tabular m-0 text-fg-2">{formatDate(software.addedAt, language) || t("installed.unknown")}</dd>
        </div>
      </dl>

      <section className="mt-4">
        <h3 className="m-0 text-[12px] font-medium text-fg-3">{t("summary.title")}</h3>
        <SummaryText
          summary={summary}
          busy={summaryBusy}
          onFetch={() => requestSummary(software.name, true)}
          ai={{ name: software.name }}
          name={software.name}
          showWriteButton={false}
        />
      </section>

      <StatusBlock software={software} installedApp={installedApp} />

      <Versions software={software} />

      <section className="mt-6">
        <SectionHeader title={t("source.title")} />
        <SourceSection software={software} />
      </section>

      <InfoSection software={software} onSave={save} />
    </div>
  );
}

/** 每一块的标题：小号灰字，右边放这一块的按钮。块与块之间一样的间距。 */
function SectionHeader({ title, count, children }: { title: string; count?: number; children?: React.ReactNode }) {
  return (
    <div className="mb-1.5 flex h-7 items-center gap-0.5">
      <h3 className="m-0 flex-1 text-[12px] font-medium text-fg-3">
        {title}
        {count !== undefined && <span className="tabular ml-1.5 font-normal text-fg-4">{count}</span>}
      </h3>
      {children}
    </div>
  );
}

/**
 * 安装状态：至多两行，都没有就不显示。
 * 第一行是这台电脑上的情况（没装给“安装”，比仓库旧给“升级”，装着的是最新就只写版本）；第二行是在线源上的新版本。
 * 每行左边一个图标（只有图标带颜色），中间两行字，右边至多一个按钮，按钮一样大。卸载在头部的 ⋯ 菜单里。
 */
function StatusBlock({ software, installedApp }: { software: Software; installedApp?: InstalledApp }) {
  const t = useT();
  const startDownload = useApp((state) => state.downloadAsset);
  const latest = software.subscription?.latest;
  const update = latest && hasUpdate(software) ? latest : null;
  const asset = update ? bestAsset(software) : null;
  const download = useApp((state) => (asset ? state.downloads[asset.url] : undefined));
  const installer = installerFor(software);
  const upgrade = upgradeFor(software, installedApp);
  if (!installedApp && !installer && !update) return null;

  const percent = download?.total ? Math.min(100, Math.round((download.received / download.total) * 100)) : null;
  const unknown = t("detail.unknownVersion");
  const action = "min-w-16 shrink-0";
  return (
    <div className="mt-4 grid gap-1 rounded-lg bg-hover px-3 py-2">
      {installedApp && upgrade ? (
        <StatusRow
          icon={<ArrowUpCircle size={16} strokeWidth={1.75} className="text-warning" />}
          title={t("status.upgradeTo", { version: upgrade.version.version || unknown })}
          detail={t("status.installedNow", { version: installedApp.version || unknown })}
        >
          <Button variant="primary" size="sm" className={action} title={upgrade.file.name} onClick={() => void installSoftware(software, true)}>
            {t("link.upgrade")}
          </Button>
        </StatusRow>
      ) : installedApp ? (
        <StatusRow
          icon={<CircleCheck size={16} strokeWidth={1.75} className="text-success" />}
          title={installedApp.version ? t("link.installedVersion", { version: installedApp.version }) : t("link.installed")}
          detail={t("status.onThisComputer")}
        />
      ) : (
        installer && (
          <StatusRow icon={<CircleDashed size={16} strokeWidth={1.75} className="text-fg-3" />} title={t("link.missing")} detail={t("status.notOnThisComputer")}>
            <Button variant="primary" size="sm" className={action} title={installer.file.name} onClick={() => void installSoftware(software)}>
              {t("link.installVersion", { version: installer.version.version || unknown })}
            </Button>
          </StatusRow>
        )
      )}
      {update && (
        <StatusRow
          icon={<CloudDownload size={16} strokeWidth={1.75} className="text-accent" />}
          title={t("status.onlineVersion", { version: update.version })}
          detail={software.subscription ? `${t(`source.${software.subscription.source}`)} · ${software.subscription.reference}` : ""}
        >
          {download ? (
            <span className="flex shrink-0 items-center gap-1">
              <span className="tabular text-[12px] text-accent">{percent !== null ? `${percent}%` : t("source.queued")}</span>
              <IconButton label={t("source.cancel")} className="size-6" onClick={() => void api.cancelDownload(asset!.url).catch(() => {})}>
                <X size={14} strokeWidth={1.75} />
              </IconButton>
            </span>
          ) : asset ? (
            <Button size="sm" className={action} title={asset.name} onClick={() => void startDownload(software.id, asset)}>
              {t("source.download")}
            </Button>
          ) : null}
        </StatusRow>
      )}
    </div>
  );
}

function StatusRow({ icon, title, detail, children }: { icon: React.ReactNode; title: string; detail?: string; children?: React.ReactNode }) {
  return (
    <div className="flex min-h-10 items-center gap-2.5 py-1">
      <span aria-hidden className="flex shrink-0">
        {icon}
      </span>
      <div className="min-w-0 flex-1">
        <div className="text-[13px] leading-snug break-words">{title}</div>
        {detail && <div className="text-[12px] leading-snug break-words text-fg-3">{detail}</div>}
      </div>
      {children}
    </div>
  );
}

/** 软件名：平时是能折成两行的标题，点一下变成输入框；Enter 或失焦保存，Esc 放弃。保存失败时退回原名。 */
function EditableTitle({ value, label, onCommit }: { value: string; label: string; onCommit: (value: string) => Promise<boolean> }) {
  const [editing, setEditing] = useState(false);
  if (!editing) {
    return (
      <button
        type="button"
        title={value}
        aria-label={label}
        onClick={() => setEditing(true)}
        className="-ml-1.5 block w-full rounded-md border border-transparent px-1.5 py-0.5 text-left text-[15px] leading-snug font-semibold break-words hover:border-line-strong"
      >
        <span className="line-clamp-2">{value}</span>
      </button>
    );
  }
  const finish = async (input: HTMLInputElement, keep: boolean) => {
    const next = input.value.trim();
    setEditing(false);
    if (keep && next && next !== value) await onCommit(next);
  };
  return (
    <input
      autoFocus
      aria-label={label}
      defaultValue={value}
      spellCheck={false}
      onFocus={(event) => event.currentTarget.select()}
      onBlur={(event) => void finish(event.currentTarget, true)}
      onKeyDown={(event) => {
        if (event.key === "Enter") event.currentTarget.blur();
        if (event.key === "Escape") {
          event.stopPropagation();
          void finish(event.currentTarget, false);
        }
      }}
      className="-ml-1.5 h-[30px] w-full rounded-md border border-accent bg-bg px-1.5 text-[15px] font-semibold outline-none"
    />
  );
}

/**
 * 版本：一张表，每个版本一行（版本号、默认、本机、文件数和大小），点一行展开它的安装包。
 * 一开始只展开默认版本。设为默认、安装这个版本、备注、改版本号、删除都在版本行的右键菜单里。
 */
function Versions({ software }: { software: Software }) {
  const t = useT();
  const installed = useApp((state) => {
    const id = state.links[software.id];
    return id ? state.installedIndex[id]?.version : undefined;
  });
  const patch = useApp((state) => state.patch);
  const [pruning, setPruning] = useState(false);
  const versions = sortedVersions(software);
  const main = primaryVersion(software);
  const [open, setOpen] = useState<Set<string>>(() => new Set(main ? [main.id] : []));
  const toggle = (id: string) => {
    const next = new Set(open);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setOpen(next);
  };

  // 选的文件都归到这个软件下，版本号在导入窗口确认。
  const addFiles = async () => {
    const paths = await pickFiles();
    if (paths.length > 0) patch({ importDialog: { paths, softwareName: software.name } });
  };

  return (
    <section className="mt-6">
      <SectionHeader title={t("detail.versions")} count={versions.length}>
        <IconButton label={t("version.addHint", { name: software.name })} className="size-6" onClick={() => void addFiles()}>
          <Plus size={15} strokeWidth={1.75} />
        </IconButton>
        {versions.length > 1 && (
          <IconButton label={t("version.prune")} className="size-6" onClick={() => setPruning(true)}>
            <Eraser size={14} strokeWidth={1.75} />
          </IconButton>
        )}
      </SectionHeader>
      {versions.length === 0 ? (
        <p className="m-0 px-2 py-2 text-[12px] text-fg-3">{t("detail.noFiles")}</p>
      ) : (
        <div className="flex flex-col gap-1">
          {versions.map((version) => (
            <VersionItem
              key={version.id}
              software={software}
              version={version}
              isMain={version.id === main?.id}
              installed={installed}
              open={open.has(version.id)}
              onToggle={() => toggle(version.id)}
            />
          ))}
        </div>
      )}
      {pruning && <PruneDialog software={software} installed={installed} onClose={() => setPruning(false)} />}
    </section>
  );
}

/** 清理旧版本：选保留最新几个，列出会删掉哪些、能省多少空间。 */
function PruneDialog({ software, installed, onClose }: { software: Software; installed?: string; onClose: () => void }) {
  const t = useT();
  const versions = sortedVersions(software);
  const [keep, setKeep] = useState(Math.min(2, versions.length - 1));
  const [keepLocal, setKeepLocal] = useState(true);
  const [withFiles, setWithFiles] = useState(true);
  const [busy, setBusy] = useState(false);
  // 本机装着的版本在仓库里有，才需要问要不要留
  const local = installed !== undefined && versions.some((v) => sameVersion(v.version, installed)) ? installed : undefined;
  const doomed = prunable(software, keep, keepLocal ? local : undefined);
  const doomedIds = new Set(doomed.map((v) => v.id));
  const freed = doomed.reduce((sum, v) => sum + v.files.reduce((n, f) => n + f.size, 0), 0);
  const pinned = versions.find((v) => v.id === software.defaultVersionId);
  const pinnedOld = pinned !== undefined && versions.indexOf(pinned) >= keep;

  const run = async () => {
    setBusy(true);
    const done = await deleteVersions(
      doomed.map((version) => ({ software, version })),
      withFiles,
    );
    setBusy(false);
    if (done) onClose();
  };

  return (
    <Modal
      open
      onClose={onClose}
      title={t("prune.title", { name: software.name })}
      width={420}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="danger" busy={busy} disabled={doomed.length === 0} onClick={() => void run()}>
            {t("prune.confirm", { n: doomed.length })}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <label className="flex items-center gap-2 text-[13px]">
          {t("prune.keep")}
          <Select compact className="w-auto" value={String(keep)} onChange={(event) => setKeep(Number(event.target.value))}>
            {Array.from({ length: versions.length - 1 }, (_, i) => i + 1).map((n) => (
              <option key={n} value={n}>
                {t("lib.versions", { n })}
              </option>
            ))}
          </Select>
        </label>
        {local && (
          <Checkbox checked={keepLocal} onChange={setKeepLocal}>
            {t("prune.keepLocal", { version: local })}
          </Checkbox>
        )}
        {pinnedOld && <p className="m-0 text-[12px] text-fg-3">{t("prune.pinnedKept", { version: pinned.version })}</p>}
        <div className="rounded-lg border border-line">
          {versions.map((version) => {
            const gone = doomedIds.has(version.id);
            return (
              <div key={version.id} className="flex h-8 items-center gap-2 border-b border-line px-2.5 text-[13px] last:border-b-0">
                <span className={cx("tabular min-w-0 flex-1 truncate", gone ? "text-fg-3 line-through" : "")}>
                  {version.version || t("detail.unknownVersion")}
                </span>
                <span className="tabular text-[12px] text-fg-3">{formatSize(version.files.reduce((n, f) => n + f.size, 0))}</span>
                <span className={cx("w-10 text-right text-[12px]", gone ? "text-danger" : "text-fg-3")}>
                  {gone ? t("common.delete") : t("version.keep")}
                </span>
              </div>
            );
          })}
        </div>
        <div className="text-[13px] font-medium">
          {doomed.length > 0 ? t("prune.summary", { n: doomed.length, size: formatSize(freed) }) : t("prune.nothing")}
        </div>
        <div>
          <Checkbox checked={withFiles} onChange={setWithFiles}>
            {t("del.alsoDisk")}
          </Checkbox>
          <p className="m-0 mt-1 pl-6 text-[12px] leading-relaxed text-fg-3">{withFiles ? t("del.diskHint") : t("del.recordOnly")}</p>
        </div>
      </div>
    </Modal>
  );
}

function VersionItem({
  software,
  version,
  isMain,
  installed,
  open,
  onToggle,
}: {
  software: Software;
  version: Version;
  isMain: boolean;
  installed?: string;
  open: boolean;
  onToggle: () => void;
}) {
  const t = useT();
  const language = useLanguage();
  const apply = useApp((state) => state.apply);
  const hover = useSoftwareHover(software, { files: version.files, meta: t("version.addedAt", { date: formatDate(version.addedAt, language) }) });
  const pinned = software.defaultVersionId === version.id;
  const local = installed !== undefined && sameVersion(version.version, installed);
  const installer = installerIn(version);
  const size = version.files.reduce((sum, file) => sum + file.size, 0);
  const [editingNote, setEditingNote] = useState(false);
  const editNoteAfterMenu = useRef(false);

  const saveNote = async (value: string) => {
    setEditingNote(false);
    const note = value.trim();
    if (note !== version.note) await apply(() => api.updateVersion(software.id, version.id, { version: version.version, note }));
  };

  const rename = async () => {
    const value = await prompt({ title: t("detail.editVersion"), label: t("detail.versionPrompt"), initial: version.version });
    if (value !== null && value !== version.version) {
      await apply(() => api.updateVersion(software.id, version.id, { version: value, note: version.note }));
    }
  };

  const menu: MenuEntry[] = [
    ...(pinned
      ? [{ label: t("version.auto"), icon: PinOff, onSelect: () => void setDefaultVersion(software, null) }]
      : isMain
        ? []
        : [{ label: t("detail.setDefault"), icon: Pin, onSelect: () => void setDefaultVersion(software, version.id) }]),
    // 默认版本的安装和升级在上面的状态表里，这里只给别的版本
    ...(!isMain && installer && !local
      ? [{ label: t("version.installThis"), icon: Download, onSelect: () => void installSoftware(software, false, version) }]
      : []),
    {
      label: version.note ? t("version.editNote") : t("version.addNote"),
      icon: StickyNote,
      separated: pinned || !isMain,
      onSelect: () => {
        editNoteAfterMenu.current = true;
      },
    },
    { label: t("detail.editVersion"), icon: Pencil, onSelect: () => void rename() },
    { label: t("detail.deleteVersion"), icon: Trash2, danger: true, separated: true, onSelect: () => void deleteVersion(software, version) },
  ];

  return (
    <div>
      <ContextMenu
        items={menu}
        onCloseAutoFocus={(event) => {
          if (!editNoteAfterMenu.current) return;
          editNoteAfterMenu.current = false;
          event.preventDefault();
          setEditingNote(true);
        }}
      >
        <button
          type="button"
          aria-expanded={open}
          onClick={onToggle}
          {...hover.handlers}
          className="flex h-9 w-full cursor-default items-center gap-1.5 rounded-md pr-3 pl-2 text-left transition-colors hover:bg-hover data-[state=open]:bg-hover"
        >
          <ChevronRight size={14} strokeWidth={2} aria-hidden className={cx("shrink-0 text-fg-3 transition-transform", open && "rotate-90")} />
          <span className="tabular max-w-[55%] shrink-0 truncate text-[13px] font-medium">{version.version || t("detail.unknownVersion")}</span>
          {isMain && <Chip className={cx("h-[18px] px-1", pinned && "bg-accent-soft text-accent")}>{t("detail.default")}</Chip>}
          {local && <Chip className="h-[18px] px-1 text-success">{t("version.local")}</Chip>}
          <span className="tabular ml-auto min-w-0 truncate pl-2 text-[12px] text-fg-3">
            {t("lib.files", { n: version.files.length })} · {formatSize(size)}
          </span>
        </button>
      </ContextMenu>
      {hover.card}
      {/* 版本备注：点一下就能改，失焦保存，Esc 放弃 */}
      {editingNote ? (
        <div className="px-3 pb-2">
          <textarea
            autoFocus
            rows={2}
            defaultValue={version.note}
            aria-label={t("version.editNote")}
            placeholder={t("version.notePlaceholder")}
            onBlur={(event) => void saveNote(event.currentTarget.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                event.stopPropagation();
                setEditingNote(false);
              }
            }}
            className="selectable block w-full resize-y rounded-md border border-line-strong bg-bg px-2 py-1 text-[12px] leading-relaxed outline-none placeholder:text-fg-4 focus:border-accent"
          />
        </div>
      ) : (
        version.note && (
          <button
            type="button"
            title={t("version.editNote")}
            onClick={() => setEditingNote(true)}
            className="mx-2 mb-1.5 flex w-[calc(100%-16px)] items-start gap-1.5 rounded-md px-[22px] py-0.5 text-left text-[12px] leading-relaxed whitespace-pre-wrap text-fg-2 hover:bg-hover"
          >
            <span className="min-w-0">{version.note}</span>
          </button>
        )
      )}
      {open && (
        <div className="pb-1">
          {version.files.map((file) => (
            <FileRow key={file.id} software={software} version={version} file={file} />
          ))}
        </div>
      )}
    </div>
  );
}

/** 安装包行显示平台、文件名和大小，文件操作在右键菜单里。 */
function FileRow({ software, version, file }: { software: Software; version: Version; file: PackageFile }) {
  const t = useT();
  const attempt = useApp((state) => state.attempt);
  const notify = useApp((state) => state.notify);
  const ref = { softwareId: software.id, versionId: version.id, fileId: file.id };
  const runnable = file.platform === hostPlatform || file.platform === "cross";
  const platform = t(`platform.${file.platform}`);
  const hover = useSoftwareHover(software, { files: [file], name: file.name, meta: [platform, file.arch].filter(Boolean).join(" · ") });

  const copyHash = async () => {
    try {
      await navigator.clipboard.writeText(file.sha256);
      notify(t("detail.copied"));
    } catch {
      notify(t("detail.copyFailed"), "error");
    }
  };

  const menu: MenuEntry[] = [
    {
      label: runnable ? t("detail.run") : t("detail.runOther", { platform }),
      icon: Play,
      disabled: !runnable,
      onSelect: () => void attempt(() => api.runFile(ref)),
    },
    { label: t("detail.reveal"), icon: FolderOpen, onSelect: () => void attempt(() => api.revealFile(ref)) },
    { label: t("detail.copyHash"), icon: Copy, onSelect: () => void copyHash() },
    { label: t("detail.deleteFile"), icon: Trash2, danger: true, separated: true, onSelect: () => void deleteFile(software, version, file) },
  ];

  return (
    <ContextMenu items={menu}>
      <div tabIndex={0} className="flex h-8 items-center gap-2 overflow-hidden rounded-md pr-3 pl-[30px] hover:bg-hover data-[state=open]:bg-hover" {...hover.handlers}>
        {hover.card}
        <span className="flex shrink-0 text-fg-3">
          <PackageFileIcon name={file.name} platform={file.platform} size={13} />
        </span>
        <MiddleTruncate text={file.name} tail={12} title={null} className="selectable min-w-0 flex-1 text-[12px]" />
        <span className="tabular shrink-0 text-[12px] text-fg-3">{formatSize(file.size)}</span>
      </div>
    </ContextMenu>
  );
}

/**
 * 更多信息（默认收起）：分类、主页、备注，点值就能改。
 */
function InfoSection({ software, onSave }: { software: Software; onSave: (patch: Partial<SoftwareEdit>) => Promise<boolean> }) {
  const t = useT();
  const categories = useApp((state) => state.library!.categories);
  const attempt = useApp((state) => state.attempt);
  const moreOpen = usePrefs((state) => state.detailMoreOpen);
  const setPrefs = usePrefs((state) => state.set);
  const homepage = /^https?:\/\//i.test(software.homepage) ? software.homepage : null;
  const field =
    "selectable w-full rounded-md border border-transparent bg-transparent px-1.5 text-[13px] outline-none transition-colors placeholder:text-fg-4 hover:border-line-strong focus:border-accent focus:bg-bg";

  /** 失焦时保存；内容没变就不动。保存失败时恢复原值。 */
  const commit = (key: "homepage" | "note") => async (event: { currentTarget: HTMLInputElement | HTMLTextAreaElement }) => {
    const input = event.currentTarget;
    const value = key === "note" ? input.value : input.value.trim();
    if (value === software[key]) return;
    if (!(await onSave({ [key]: value }))) input.value = software[key];
  };

  return (
    <section className="mt-6">
      <button
        type="button"
        aria-expanded={moreOpen}
        onClick={() => setPrefs({ detailMoreOpen: !moreOpen })}
        className="mb-1.5 flex h-7 w-full items-center gap-1 text-[12px] font-medium text-fg-3 hover:text-fg-2"
      >
        {t("detail.more")}
        <ChevronRight size={13} strokeWidth={2} aria-hidden className={cx("transition-transform", moreOpen && "rotate-90")} />
      </button>
      {moreOpen && (
        <dl className="m-0">
          <InfoRow label={t("detail.category")}>
            <Select compact className="w-full" value={software.categoryId ?? ""} onChange={(event) => void onSave({ categoryId: event.target.value || null })}>
              <option value="">{t("nav.uncategorized")}</option>
              {flattenTree(categories).map(({ category, depth }) => (
                <option key={category.id} value={category.id}>
                  {"　".repeat(depth) + category.name}
                </option>
              ))}
            </Select>
          </InfoRow>
          <InfoRow label={t("detail.homepage")}>
            <div className="flex items-center gap-0.5">
              <input
                aria-label={t("detail.homepage")}
                defaultValue={software.homepage}
                key={`h:${software.homepage}`}
                placeholder="https://"
                spellCheck={false}
                onBlur={commit("homepage")}
                onKeyDown={(event) => event.key === "Enter" && event.currentTarget.blur()}
                className={cx(field, "h-7")}
              />
              {homepage && (
                <IconButton label={t("detail.openHomepage")} className="size-6" onClick={() => void attempt(() => api.openUrl(homepage))}>
                  <ExternalLink size={13} strokeWidth={1.75} />
                </IconButton>
              )}
            </div>
          </InfoRow>
          <InfoRow label={t("detail.note")} top>
            <textarea
              aria-label={t("detail.note")}
              defaultValue={software.note}
              key={`n:${software.note}`}
              rows={2}
              placeholder={t("detail.notePlaceholder")}
              onBlur={commit("note")}
              className={cx(field, "block resize-y py-1 leading-relaxed")}
            />
          </InfoRow>
        </dl>
      )}
    </section>
  );
}

function InfoRow({ label, top, children }: { label: string; top?: boolean; children: React.ReactNode }) {
  return (
    <div className={cx("flex gap-2 py-1", top ? "items-start" : "items-center")}>
      <dt className={cx("w-14 shrink-0 text-[12px] text-fg-3", top && "pt-1.5")}>{label}</dt>
      <dd className="m-0 min-w-0 flex-1">{children}</dd>
    </div>
  );
}
