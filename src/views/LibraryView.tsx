// 软件库：工具栏（数量、搜索、平台、安装状态、标签、视图）加软件列表、网格或文件夹树。

import {
  ArrowDown,
  ArrowUp,
  ChevronRight,
  CircleCheck,
  File as FileIcon,
  Folder,
  FolderOpen,
  FolderTree,
  LayoutGrid,
  LayoutList,
  PackageOpen,
  Rows3,
  Search,
  SearchX,
  Trash2,
  X,
} from "lucide-react";
import { type PointerEvent, useMemo, useState } from "react";
import { PlatformIcons } from "../components/PlatformIcon";
import { SoftwareIcon, useSoftwareIcon } from "../components/SoftwareIcon";
import { UpdateMark } from "../components/SourcePanel";
import { Button, ContextMenu, EmptyState, IconButton, Select, cx } from "../components/ui";
import { deleteSoftware, installSoftware, installerFor, moveSoftwareTo, uninstallApp } from "../lib/actions";
import { api } from "../lib/api";
import { beginDrag } from "../lib/drag";
import {
  PLATFORMS,
  categoryPath,
  fileCount,
  flattenTree,
  formatDate,
  formatSize,
  hasUpdate,
  primaryVersion,
  softwarePlatforms,
  softwareSize,
  sortedVersions,
  withDescendants,
} from "../lib/library";
import { type SortKey, usePrefs } from "../lib/prefs";
import { type InstallFilter, useApp, useLanguage, useT } from "../lib/store";
import type { InstalledApp, Platform, Software } from "../lib/types";

// 栏窄的时候先收起添加时间，再收起大小
const COLUMNS = "grid grid-cols-[minmax(0,1fr)_auto] @lg:grid-cols-[minmax(0,1fr)_104px_72px] @2xl:grid-cols-[minmax(0,1fr)_104px_76px_92px] items-center gap-x-3";

export function LibraryView() {
  const t = useT();
  const library = useApp((state) => state.library)!;
  const scope = useApp((state) => state.scope);
  const search = useApp((state) => state.search);
  const platform = useApp((state) => state.platform);
  const tagFilter = useApp((state) => state.tagFilter);
  const installFilter = useApp((state) => state.installFilter);
  const updatesOnly = useApp((state) => state.updatesOnly);
  const links = useApp((state) => state.links);
  const installedKnown = useApp((state) => state.installed !== null);
  const selectedId = useApp((state) => state.selectedId);
  const select = useApp((state) => state.select);
  const patch = useApp((state) => state.patch);
  const setScope = useApp((state) => state.setScope);
  const prefs = usePrefs();

  const list = useMemo(() => {
    let items = library.software;
    if (scope.kind === "uncategorized") items = items.filter((s) => !s.categoryId);
    if (scope.kind === "category") {
      const ids = withDescendants(library.categories, scope.id);
      items = items.filter((s) => s.categoryId !== null && ids.has(s.categoryId));
    }
    if (platform !== "all") items = items.filter((s) => softwarePlatforms(s).includes(platform));
    if (tagFilter) items = items.filter((s) => s.tagIds.includes(tagFilter));
    if (installFilter !== "all") items = items.filter((s) => s.id in links === (installFilter === "installed"));
    if (updatesOnly) items = items.filter(hasUpdate);
    const query = search.trim().toLowerCase();
    if (query) {
      items = items.filter(
        (s) =>
          s.name.toLowerCase().includes(query) ||
          s.publisher.toLowerCase().includes(query) ||
          s.versions.some((v) => v.files.some((f) => f.name.toLowerCase().includes(query))),
      );
    }
    const direction = prefs.sortAsc ? 1 : -1;
    const byName = (a: Software, b: Software) => a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" });
    const compare: Record<SortKey, (a: Software, b: Software) => number> = {
      name: byName,
      platform: (a, b) => (softwarePlatforms(a)[0] ?? "").localeCompare(softwarePlatforms(b)[0] ?? "") || byName(a, b),
      size: (a, b) => softwareSize(a) - softwareSize(b),
      added: (a, b) => a.addedAt.localeCompare(b.addedAt),
    };
    return [...items].sort((a, b) => compare[prefs.sortKey](a, b) * direction);
  }, [library, scope, platform, tagFilter, installFilter, updatesOnly, links, search, prefs.sortKey, prefs.sortAsc]);
  const updateCount = useMemo(() => library.software.filter(hasUpdate).length, [library]);

  // 看“全部软件”时按分类分组；已经在某个分类里或正在搜索时就不分了
  const groups = useMemo(() => {
    const grouped = prefs.groupByCategory && scope.kind === "all" && !search.trim();
    if (!grouped) return [{ key: "all", title: null as string | null, items: list }];
    const out = flattenTree(library.categories)
      .map(({ category }) => ({
        key: category.id,
        title: categoryPath(library.categories, category.id),
        items: list.filter((s) => s.categoryId === category.id),
      }))
      .filter((group) => group.items.length > 0);
    const rest = list.filter((s) => !s.categoryId);
    if (rest.length > 0) out.push({ key: "none", title: t("nav.uncategorized"), items: rest });
    return out;
  }, [list, library.categories, prefs.groupByCategory, scope.kind, search, t]);

  const scopeName =
    scope.kind === "category"
      ? categoryPath(library.categories, scope.id)
      : scope.kind === "uncategorized"
        ? t("nav.uncategorized")
        : null;
  const filtering =
    search.trim() !== "" || platform !== "all" || tagFilter !== null || installFilter !== "all" || updatesOnly || scope.kind !== "all";
  const view = prefs.packageView;

  return (
    <div className="@container flex min-h-0 flex-1 flex-col">
      <div className="flex h-10 shrink-0 items-center gap-2 border-b border-line px-3">
        <span className="tabular shrink-0 text-[12px] text-fg-3">{t("lib.count", { n: list.length })}</span>
        <div className="relative min-w-[96px] flex-1 @2xl:max-w-[220px]">
          <Search size={14} strokeWidth={1.75} aria-hidden className="pointer-events-none absolute top-1/2 left-2 -translate-y-1/2 text-fg-3" />
          <input
            type="text"
            value={search}
            spellCheck={false}
            placeholder={t("lib.search")}
            onChange={(event) => patch({ search: event.target.value })}
            className="h-7 w-full rounded-md border border-transparent bg-hover pr-2 pl-7 text-[13px] outline-none placeholder:text-fg-3 focus:border-accent"
          />
        </div>
        <Select
          aria-label={t("lib.col.platform")}
          value={platform}
          onChange={(event) => patch({ platform: event.target.value as Platform | "all" })}
          compact
        >
          <option value="all">{t("lib.allPlatforms")}</option>
          {PLATFORMS.map((p) => (
            <option key={p} value={p}>
              {t(`platform.${p}`)}
            </option>
          ))}
        </Select>
        {installedKnown && (
          <Select
            aria-label={t("link.installed")}
            value={installFilter}
            onChange={(event) => patch({ installFilter: event.target.value as InstallFilter })}
            compact
            // 栏窄时收起；但正在筛选时必须看得见，否则不知道列表为什么变少了
            className={installFilter !== "all" ? undefined : "hidden @xl:block"}
          >
            <option value="all">{t("link.all")}</option>
            <option value="installed">{t("link.only")}</option>
            <option value="missing">{t("link.missing")}</option>
          </Select>
        )}
        {(updateCount > 0 || updatesOnly) && (
          <button
            type="button"
            aria-pressed={updatesOnly}
            onClick={() => patch({ updatesOnly: !updatesOnly })}
            className={cx(
              "tabular h-7 shrink-0 rounded-md px-2 text-[12px] font-medium whitespace-nowrap transition-colors",
              updatesOnly ? "bg-accent-strong text-white" : "bg-accent-soft text-accent hover:brightness-110",
            )}
          >
            {t("updates.chip", { n: updateCount })}
          </button>
        )}
        {library.tags.length > 0 && (
          <Select
            aria-label={t("nav.tags")}
            value={tagFilter ?? ""}
            onChange={(event) => patch({ tagFilter: event.target.value || null })}
            compact
            className={tagFilter ? undefined : "hidden @3xl:block"}
          >
            <option value="">{t("lib.allTags")}</option>
            {library.tags.map((tag) => (
              <option key={tag.id} value={tag.id}>
                {tag.name}
              </option>
            ))}
          </Select>
        )}
        {scopeName && (
          <span className="hidden h-6 shrink-0 items-center gap-1 rounded-md bg-accent-soft pr-1 pl-2 text-[12px] text-accent @4xl:inline-flex">
            <span className="max-w-[160px] truncate">{scopeName}</span>
            <button type="button" aria-label={t("common.close")} onClick={() => setScope({ kind: "all" })} className="rounded p-0.5 hover:bg-accent-soft">
              <X size={12} strokeWidth={2} />
            </button>
          </span>
        )}
        <div className="ml-auto flex shrink-0 items-center gap-0.5">
          <IconButton label={t("lib.group")} active={prefs.groupByCategory} onClick={() => prefs.set({ groupByCategory: !prefs.groupByCategory })}>
            <Rows3 size={16} strokeWidth={1.75} />
          </IconButton>
          <IconButton label={t("lib.view.list")} active={view === "list"} onClick={() => prefs.set({ packageView: "list" })}>
            <LayoutList size={16} strokeWidth={1.75} />
          </IconButton>
          <IconButton label={t("lib.view.grid")} active={view === "grid"} onClick={() => prefs.set({ packageView: "grid" })}>
            <LayoutGrid size={16} strokeWidth={1.75} />
          </IconButton>
          <IconButton label={t("lib.view.folder")} active={view === "folder"} onClick={() => prefs.set({ packageView: "folder" })}>
            <FolderTree size={16} strokeWidth={1.75} />
          </IconButton>
        </div>
      </div>

      {list.length === 0 ? (
        filtering ? (
          <EmptyState icon={SearchX} title={t("lib.noMatch")} />
        ) : (
          <EmptyState icon={PackageOpen} title={t("lib.empty.title")} body={t("lib.empty.body")} />
        )
      ) : (
        <>
          {view === "list" && (
            <div className={cx(COLUMNS, "h-7 shrink-0 border-b border-line pr-5 pl-[58px] text-[11px] font-medium text-fg-3")}>
              <SortHeader column="name" label={t("lib.col.name")} />
              <SortHeader column="platform" label={t("lib.col.platform")} />
              <SortHeader column="size" label={t("lib.col.size")} className="hidden justify-end @lg:flex" />
              <SortHeader column="added" label={t("lib.col.added")} className="hidden justify-end @2xl:flex" />
            </div>
          )}
          <div className="min-h-0 flex-1 overflow-y-auto pb-3" onClick={(event) => event.target === event.currentTarget && select(null)}>
            {groups.map((group) => (
              <section key={group.key}>
                {group.title !== null && (
                  <div className="flex items-center gap-2 px-3 pt-4 pb-1.5 text-[11px] font-medium text-fg-3">
                    <span>{group.title}</span>
                    <span className="tabular text-fg-4">{group.items.length}</span>
                    <span className="h-px flex-1 bg-line" />
                  </div>
                )}
                {view === "list" && (
                  <div className="px-1.5">
                    {group.items.map((software) => (
                      <Row key={software.id} software={software} selected={software.id === selectedId} />
                    ))}
                  </div>
                )}
                {view === "grid" && (
                  <div className="grid grid-cols-[repeat(auto-fill,minmax(150px,1fr))] gap-2 px-3 pt-1">
                    {group.items.map((software) => (
                      <Card key={software.id} software={software} selected={software.id === selectedId} />
                    ))}
                  </div>
                )}
                {view === "folder" && (
                  <div className="px-1.5">
                    {group.items.map((software) => (
                      <FolderNode key={software.id} software={software} selected={software.id === selectedId} />
                    ))}
                  </div>
                )}
              </section>
            ))}
          </div>
        </>
      )}
    </div>
  );
}

function SortHeader({ column, label, className }: { column: SortKey; label: string; className?: string }) {
  const sortKey = usePrefs((state) => state.sortKey);
  const sortAsc = usePrefs((state) => state.sortAsc);
  const sortBy = usePrefs((state) => state.sortBy);
  const Arrow = sortAsc ? ArrowUp : ArrowDown;
  return (
    <button type="button" onClick={() => sortBy(column)} className={cx("flex items-center gap-1 text-left hover:text-fg-2", className)}>
      {label}
      {sortKey === column && <Arrow size={11} strokeWidth={2} aria-hidden />}
    </button>
  );
}

/** 一行或一张卡片共用的行为：选中、右键菜单、拖到分类上、和已安装软件的对应。 */
function useItem(software: Software) {
  const t = useT();
  const select = useApp((state) => state.select);
  const installedApp = useApp((state): InstalledApp | undefined => {
    const id = state.links[software.id];
    return id ? state.installed?.find((app) => app.id === id) : undefined;
  });
  return {
    installedApp,
    menu: [{ label: t("detail.deleteSoftware"), icon: Trash2, danger: true, onSelect: () => void deleteSoftware(software) }],
    handlers: {
      onClick: () => select(software.id),
      onContextMenu: () => select(software.id),
      onKeyDown: (event: { key: string }) => event.key === "Enter" && select(software.id),
      onPointerDown: (event: PointerEvent) =>
        beginDrag(event, { kind: "software", id: software.id, label: software.name }, (target) => void moveSoftwareTo(software.id, target.category)),
    },
  };
}

/** 名称下面那行说明：联网取到的简介优先，没有就显示版本和大小。 */
function useDescription(software: Software): string {
  const t = useT();
  const summary = useApp((state) => state.library?.summaries[software.name]?.text);
  if (summary) return summary;
  const version = primaryVersion(software);
  const parts = [version?.version ? `v${version.version}` : null, formatSize(softwareSize(software)), software.publisher || null];
  return parts.filter(Boolean).join(" · ") || t("detail.noFiles");
}

function InstalledMark({ app }: { app: InstalledApp }) {
  const t = useT();
  const label = app.version ? t("link.installedVersion", { version: app.version }) : t("link.installed");
  return (
    <span title={label} aria-label={label} className="flex shrink-0 text-success">
      <CircleCheck size={13} strokeWidth={2} />
    </span>
  );
}

function Row({ software, selected }: { software: Software; selected: boolean }) {
  const t = useT();
  const language = useLanguage();
  const { installedApp, menu, handlers } = useItem(software);
  const icon = useSoftwareIcon(software);
  const description = useDescription(software);
  const platforms = softwarePlatforms(software);
  const canInstall = !installedApp && installerFor(software) !== null;
  const canUninstall = installedApp?.uninstallable ?? false;
  return (
    <ContextMenu items={menu}>
      <div
        role="button"
        tabIndex={0}
        aria-pressed={selected}
        {...handlers}
        className={cx(COLUMNS, "group relative min-h-[52px] rounded-md py-1.5 pr-3.5 pl-2.5 transition-colors", selected ? "bg-active" : "hover:bg-hover")}
      >
        <div className="flex min-w-0 items-center gap-2.5">
          <SoftwareIcon name={software.name} icon={icon} />
          <div className="min-w-0">
            <div className="flex items-center gap-1.5">
              <span className="truncate text-[13px] font-medium">{software.name}</span>
              {installedApp && <InstalledMark app={installedApp} />}
              <UpdateMark software={software} />
            </div>
            <div className="truncate text-[12px] text-fg-3">{description}</div>
          </div>
        </div>
        <PlatformIcons platforms={platforms} className="justify-end @lg:justify-start" />
        <div className="tabular hidden text-right text-[12px] text-fg-2 @lg:block">{formatSize(softwareSize(software))}</div>
        <div className="tabular hidden text-right text-[12px] text-fg-3 @2xl:block">{formatDate(software.addedAt, language)}</div>
        {(canInstall || canUninstall) && (
          // 悬停时盖在这一行右侧的按钮。按下时不触发拖拽，也不改变选中
          <div
            className="absolute inset-y-1 right-2 hidden items-center rounded-md bg-panel px-1 shadow-pop group-hover:flex"
            onPointerDown={(event) => event.stopPropagation()}
            onClick={(event) => event.stopPropagation()}
          >
            {canInstall ? (
              <Button variant="ghost" className="text-accent hover:text-accent" onClick={() => void installSoftware(software)}>
                {t("link.install")}
              </Button>
            ) : (
              <Button variant="ghost" className="text-danger hover:text-danger" onClick={() => installedApp && void uninstallApp(installedApp)}>
                {t("link.uninstall")}
              </Button>
            )}
          </div>
        )}
      </div>
    </ContextMenu>
  );
}

function Card({ software, selected }: { software: Software; selected: boolean }) {
  const t = useT();
  const { installedApp, menu, handlers } = useItem(software);
  const icon = useSoftwareIcon(software);
  const version = primaryVersion(software);
  return (
    <ContextMenu items={menu}>
      <div
        role="button"
        tabIndex={0}
        aria-pressed={selected}
        {...handlers}
        className={cx(
          "flex flex-col gap-2 rounded-lg border p-3 transition-colors",
          selected ? "border-accent bg-accent-soft" : "border-line hover:bg-hover",
        )}
      >
        <div className="flex items-start gap-1.5">
          <SoftwareIcon name={software.name} icon={icon} size={40} />
          <span className="flex-1" />
          <UpdateMark software={software} />
          {installedApp && <InstalledMark app={installedApp} />}
        </div>
        <div className="min-w-0">
          <div className="truncate text-[13px] font-medium">{software.name}</div>
          <div className="truncate text-[12px] text-fg-3">
            {version?.version ? `v${version.version}` : t("lib.files", { n: fileCount(software) })}
          </div>
        </div>
        <PlatformIcons platforms={softwarePlatforms(software)} />
      </div>
    </ContextMenu>
  );
}

/** 文件夹视图里的一个软件：展开看版本，再展开看文件，和磁盘上的目录一一对应。 */
function FolderNode({ software, selected }: { software: Software; selected: boolean }) {
  const t = useT();
  const attempt = useApp((state) => state.attempt);
  const { installedApp, menu, handlers } = useItem(software);
  const icon = useSoftwareIcon(software);
  const [open, setOpen] = useState(false);
  const [closedVersions, setClosedVersions] = useState<string[]>([]);
  const toggleVersion = (id: string) => setClosedVersions((list) => (list.includes(id) ? list.filter((x) => x !== id) : [...list, id]));
  const line = "flex h-8 items-center gap-1.5 rounded-md pr-2.5 text-[13px] transition-colors";

  return (
    <div>
      <ContextMenu items={menu}>
        <div
          role="button"
          tabIndex={0}
          aria-expanded={open}
          {...handlers}
          onClick={() => {
            handlers.onClick();
            setOpen(!open);
          }}
          className={cx(line, "pl-1.5", selected ? "bg-active" : "hover:bg-hover")}
        >
          <ChevronRight size={13} strokeWidth={2} aria-hidden className={cx("shrink-0 text-fg-3 transition-transform", open && "rotate-90")} />
          <SoftwareIcon name={software.name} icon={icon} size={20} />
          <span className="truncate font-medium">{software.name}</span>
          {installedApp && <InstalledMark app={installedApp} />}
          <UpdateMark software={software} />
          <span className="tabular ml-auto shrink-0 text-[12px] text-fg-3">
            {t("lib.versions", { n: software.versions.length })} · {formatSize(softwareSize(software))}
          </span>
        </div>
      </ContextMenu>
      {open &&
        sortedVersions(software).map((version) => {
          const shown = !closedVersions.includes(version.id);
          return (
            <div key={version.id}>
              <button type="button" aria-expanded={shown} onClick={() => toggleVersion(version.id)} className={cx(line, "w-full pl-7 text-left hover:bg-hover")}>
                <ChevronRight size={13} strokeWidth={2} aria-hidden className={cx("shrink-0 text-fg-3 transition-transform", shown && "rotate-90")} />
                <Folder size={15} strokeWidth={1.75} aria-hidden className="shrink-0 text-fg-3" />
                <span className="tabular truncate">{version.version || t("detail.unknownVersion")}</span>
                <span className="tabular ml-auto shrink-0 text-[12px] text-fg-3">{t("lib.files", { n: version.files.length })}</span>
              </button>
              {shown &&
                version.files.map((file) => (
                  <div key={file.id} className={cx(line, "group pl-[54px] hover:bg-hover")}>
                    <FileIcon size={14} strokeWidth={1.75} aria-hidden className="shrink-0 text-fg-3" />
                    <span className="selectable truncate text-[12px]" title={file.path}>
                      {file.name}
                    </span>
                    <span className="ml-auto flex shrink-0 items-center gap-1.5">
                      <PlatformIcons platforms={[file.platform]} size={13} />
                      <span className="tabular w-[62px] text-right text-[12px] text-fg-3">{formatSize(file.size)}</span>
                      <IconButton
                        label={t("detail.reveal")}
                        className="size-6 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
                        onClick={() => void attempt(() => api.revealFile({ softwareId: software.id, versionId: version.id, fileId: file.id }))}
                      >
                        <FolderOpen size={14} strokeWidth={1.75} />
                      </IconButton>
                    </span>
                  </div>
                ))}
            </div>
          );
        })}
    </div>
  );
}
