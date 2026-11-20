// 已安装软件：这台电脑上装着的程序，以及右侧的详情栏。

import {
  ArrowDown,
  ArrowUp,
  CheckSquare,
  FolderOpen,
  LayoutGrid,
  LayoutList,
  ListFilter,
  LoaderCircle,
  MonitorCheck,
  MousePointerClick,
  PackageCheck,
  PackageX,
  Pencil,
  RefreshCw,
  SearchX,
  Trash2,
  WandSparkles,
} from "lucide-react";
import { type ReactNode, useContext, useEffect, useMemo, useState } from "react";
import { DetailToggle } from "../components/chrome";
import { type ColumnDef, type Columns, ColumnsContext, TableHeader, useTableColumns, useWidth } from "../components/columns";
import { SoftwareIcon } from "../components/SoftwareIcon";
import { SummaryText } from "../components/SummaryText";
import { Button, Chip, ContextMenu, EmptyState, FilterMenu, IconButton, type MenuEntry, ViewMenu, cx } from "../components/ui";
import { editSummary, uninstallApp } from "../lib/actions";
import { api } from "../lib/api";
import { compareVersions, formatDate, formatSize, primaryVersion, shownSummary, summaryName } from "../lib/library";
import { type InstalledView as InstalledViewKind, usePrefs } from "../lib/prefs";
import { useAiReady, useApp, useLanguage, useT } from "../lib/store";
import type { InstalledApp, Software } from "../lib/types";

type Column = "name" | "version" | "date" | "size";
// 列表的列：名称、版本、仓库里有没有、安装日期、大小、卸载。宽度可以拖，右键表头勾选显示哪几列；
// 栏窄时安装日期、大小、仓库从右往左暂时收起（见 components/columns）
type InstalledColumn = "version" | "repo" | "date" | "size" | "action";
const INSTALLED_COLUMNS: ColumnDef<InstalledColumn>[] = [
  { key: "version", initial: 110, min: 64, max: 260 },
  { key: "repo", initial: 96, min: 72, max: 220, autoHide: 3 },
  { key: "date", initial: 92, min: 76, max: 200, end: true, autoHide: 1 },
  { key: "size", initial: 80, min: 60, max: 160, end: true, autoHide: 2 },
  { key: "action", initial: 64, min: 64, max: 96, end: true },
];
/** 一行里除了各列之外占的宽度：列表的左右留白加上行自己的左右留白。 */
const ROW_CHROME = 16 + 22;

type RepoFilter = "all" | "in" | "out";

export function InstalledView() {
  const t = useT();
  const installed = useApp((state) => state.installed);
  const loading = useApp((state) => state.installedLoading);
  const sizes = useApp((state) => state.sizes);
  const selectedId = useApp((state) => state.selectedInstalledId);
  const selectedIds = useApp((state) => state.selectedInstalledIds);
  const selectInstalled = useApp((state) => state.selectInstalled);
  const setInstalledSelection = useApp((state) => state.setInstalledSelection);
  const reload = useApp((state) => state.loadInstalled);
  const search = useApp((state) => state.search);
  const links = useApp((state) => state.links);
  const software = useApp((state) => state.library?.software);
  const reveal = useApp((state) => state.reveal);
  const [repoFilter, setRepoFilter] = useState<RepoFilter>("all");
  const view = usePrefs((state) => state.installedView);
  const setPrefs = usePrefs((state) => state.set);
  const [frame, frameWidth] = useWidth<HTMLDivElement>();
  const columns = useTableColumns("installed", INSTALLED_COLUMNS, frameWidth - ROW_CHROME);
  const [sort, setSort] = useState<{ column: Column; asc: boolean }>({ column: "name", asc: true });

  // 统计出来的实际占用优先；还没统计完、或者统计不了时用安装程序登记的大小
  const sizeOf = (app: InstalledApp) => sizes[app.id] ?? app.size ?? null;

  // 已安装软件 id → 仓库里对应的软件
  const stored = useMemo(() => {
    const map = new Map<string, Software>();
    for (const [softwareId, appId] of Object.entries(links)) {
      const found = software?.find((s) => s.id === softwareId);
      if (found) map.set(appId, found);
    }
    return map;
  }, [links, software]);

  const list = useMemo(() => {
    const query = search.trim().toLowerCase();
    const items = (installed ?? []).filter(
      (app) =>
        (!query || app.name.toLowerCase().includes(query) || app.publisher.toLowerCase().includes(query)) &&
        (repoFilter === "all" || stored.has(app.id) === (repoFilter === "in")),
    );
    const byName = (a: InstalledApp, b: InstalledApp) => a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" });
    const compare: Record<Column, (a: InstalledApp, b: InstalledApp) => number> = {
      name: byName,
      version: (a, b) => compareVersions(a.version, b.version) || byName(a, b),
      date: (a, b) => a.installedAt.localeCompare(b.installedAt) || byName(a, b),
      size: (a, b) => (sizes[a.id] ?? a.size ?? -1) - (sizes[b.id] ?? b.size ?? -1) || byName(a, b),
    };
    return items.sort((a, b) => compare[sort.column](a, b) * (sort.asc ? 1 : -1));
  }, [installed, search, sort, sizes, stored, repoFilter]);

  // 选中的：多选时是 selectedIds，单选时就是 selectedId 那一个
  const chosen = selectedIds.length > 0 ? selectedIds : selectedId ? [selectedId] : [];

  /** 点一行：⌘ / Ctrl 加选或取消，Shift 从上次点的那一行连选到这一行，否则单选。 */
  const clickRow = (id: string, event: { metaKey: boolean; ctrlKey: boolean; shiftKey: boolean }) => {
    if (event.metaKey || event.ctrlKey) {
      const next = chosen.includes(id) ? chosen.filter((x) => x !== id) : [...chosen, id];
      setInstalledSelection(next, next.includes(id) ? id : (next[0] ?? null));
    } else if (event.shiftKey && selectedId && list.some((app) => app.id === selectedId)) {
      const from = list.findIndex((app) => app.id === selectedId);
      const to = list.findIndex((app) => app.id === id);
      setInstalledSelection(list.slice(Math.min(from, to), Math.max(from, to) + 1).map((app) => app.id), selectedId);
    } else {
      selectInstalled(id);
    }
  };

  // ⌘ / Ctrl + A 全选当前列出来的，Esc 取消选择。焦点在输入框、或者开着对话框和菜单时不管
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.closest("input, textarea, select, [contenteditable=true]")) return;
      if (document.querySelector('[role="dialog"],[role="menu"]')) return;
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "a") {
        event.preventDefault();
        setInstalledSelection(
          list.map((app) => app.id),
          selectedId ?? list[0]?.id ?? null,
        );
      } else if (event.key === "Escape" && chosen.length > 1) {
        selectInstalled(selectedId);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [list, selectedId, chosen.length, setInstalledSelection, selectInstalled]);

  const header = (column: Column, label: string, className?: string) => {
    const Arrow = sort.asc ? ArrowUp : ArrowDown;
    return (
      <button
        type="button"
        onClick={() => setSort({ column, asc: sort.column === column ? !sort.asc : column === "name" })}
        className={cx("flex items-center gap-1 text-left hover:text-fg-2", className)}
      >
        {label}
        {sort.column === column && <Arrow size={12} strokeWidth={2} aria-hidden />}
      </button>
    );
  };

  return (
    <div ref={frame} className="@container flex min-h-0 flex-1 flex-col">
      {/* 和软件库的工具栏一样分三段：标题、正中间的筛选、右边的按钮 */}
      <div className="grid h-11 shrink-0 grid-cols-[1fr_auto_1fr] items-center gap-2 border-b border-line pr-2 pl-4">
        <div className="flex min-w-0 items-center">
          <h2 className="m-0 min-w-0 truncate text-[14px] font-semibold">{t("installed.title")}</h2>
          <span className="tabular ml-1.5 shrink-0 text-[12px] text-fg-3">{list.length}</span>
        </div>
        <FilterMenu<RepoFilter>
          label={t("installed.repoFilter")}
          value={repoFilter}
          onChange={setRepoFilter}
          options={[
            { value: "all", label: t("installed.repoFilter"), icon: <ListFilter size={15} strokeWidth={1.75} /> },
            { value: "in", label: t("installed.repoIn"), icon: <PackageCheck size={15} strokeWidth={1.75} /> },
            { value: "out", label: t("installed.repoOut"), icon: <PackageX size={15} strokeWidth={1.75} /> },
          ]}
        />
        <div className="flex min-w-0 items-center justify-end gap-0.5">
          <IconButton label={t("installed.refresh")} disabled={loading} onClick={() => void reload()}>
            <RefreshCw size={15} strokeWidth={1.75} className={loading ? "animate-spin" : ""} />
          </IconButton>
          <ViewMenu<InstalledViewKind>
            label={t("lib.view")}
            value={view}
            onChange={(value) => setPrefs({ installedView: value })}
            options={[
              { value: "list", label: t("lib.view.list"), icon: <LayoutList size={15} strokeWidth={1.75} /> },
              { value: "grid", label: t("lib.view.grid"), icon: <LayoutGrid size={15} strokeWidth={1.75} /> },
            ]}
          />
          <DetailToggle />
        </div>
      </div>

      {installed === null ? (
        <EmptyState icon={LoaderCircle} title={t("installed.loading")} />
      ) : list.length === 0 ? (
        <EmptyState
          icon={search.trim() || repoFilter !== "all" ? SearchX : MonitorCheck}
          title={search.trim() || repoFilter !== "all" ? t("lib.noMatch") : t("installed.empty")}
        />
      ) : (
        <ColumnsContext.Provider value={columns}>
          {view === "list" && (
            <TableHeader
              table="installed"
              defs={INSTALLED_COLUMNS}
              columns={columns}
              className="pr-5 pl-[58px]"
              first={header("name", t("installed.col.name"))}
              firstLabel={t("installed.col.name")}
              labels={{
                version: t("installed.col.version"),
                repo: t("installed.col.repo"),
                date: t("installed.col.date"),
                size: t("installed.col.size"),
                action: t("lib.col.action"),
              }}
              cells={{
                // 这一列只放按钮，不用列名
                action: <span />,
                version: header("version", t("installed.col.version")),
                date: header("date", t("installed.col.date")),
                size: header("size", t("installed.col.size")),
              }}
            />
          )}
          <div
            className={cx(
              "min-h-0 flex-1 overflow-y-auto pt-1 pb-3",
              view === "grid" ? "grid auto-rows-min grid-cols-[repeat(auto-fill,minmax(112px,1fr))] content-start gap-1 px-3" : "px-2",
            )}
          >
            {list.map((app) =>
              view === "list" ? (
                <InstalledRow
                  key={app.id}
                  app={app}
                  selected={chosen.includes(app.id)}
                  group={chosen.length > 1 && chosen.includes(app.id) ? chosen : null}
                  size={sizeOf(app)}
                  software={stored.get(app.id)}
                  onSelect={(event) => clickRow(app.id, event)}
                  onOpen={(id) => reveal(id)}
                />
              ) : (
                <InstalledTile
                  key={app.id}
                  app={app}
                  selected={chosen.includes(app.id)}
                  group={chosen.length > 1 && chosen.includes(app.id) ? chosen : null}
                  software={stored.get(app.id)}
                  onSelect={(event) => clickRow(app.id, event)}
                />
              ),
            )}
          </div>
        </ColumnsContext.Provider>
      )}
    </div>
  );
}

/** 简介按什么名字存：仓库里有它就用仓库里那个软件的名字，否则用去掉版本号、架构之后的名字。 */
function useSummaryKey(app: InstalledApp): string {
  return useApp((state) => {
    const softwareId = Object.keys(state.links).find((id) => state.links[id] === app.id);
    const linked = softwareId ? state.library?.software.find((s) => s.id === softwareId) : undefined;
    return linked?.name ?? summaryName(app.name);
  });
}

/** 一组已安装软件各自的简介名字和发布者（给“获取简介”“AI 写简介”用）。 */
function summaryTargets(ids: string[]): { name: string; publisher: string }[] {
  const state = useApp.getState();
  return ids.flatMap((id) => {
    const app = state.installedIndex[id];
    if (!app) return [];
    const softwareId = Object.keys(state.links).find((key) => state.links[key] === id);
    const linked = softwareId ? state.library?.software.find((s) => s.id === softwareId) : undefined;
    return [{ name: linked?.name ?? summaryName(app.name), publisher: app.publisher }];
  });
}

/**
 * 已安装软件的右键菜单：写描述、联网取简介、让 AI 写简介，打开所在文件夹，能卸载的可以卸载。
 * 右键的是多选里的一个时，作用于全部选中的软件（不能卸载、不能定位）。
 */
function useInstalledMenu(app: InstalledApp, group: string[] | null): MenuEntry[] {
  const t = useT();
  const attempt = useApp((state) => state.attempt);
  const requestSummary = useApp((state) => state.requestSummary);
  const describeWithAi = useApp((state) => state.describeWithAi);
  const aiReady = useAiReady();
  const key = useSummaryKey(app);
  if (group) {
    return [
      { label: t("installed.fetchSummaries", { n: group.length }), icon: RefreshCw, onSelect: () => summaryTargets(group).forEach((target) => requestSummary(target.name, true)) },
      ...(aiReady ? [{ label: t("installed.aiSummaries", { n: group.length }), icon: WandSparkles, onSelect: () => void describeWithAi(summaryTargets(group)) }] : []),
    ];
  }
  return [
    { label: t("summary.editMenu"), icon: Pencil, onSelect: () => void editSummary(key) },
    { label: t("summary.fetch"), icon: RefreshCw, onSelect: () => requestSummary(key, true) },
    ...(aiReady ? [{ label: t("ai.describe"), icon: WandSparkles, onSelect: () => void describeWithAi([{ name: key, publisher: app.publisher }]) }] : []),
    { label: t("menu.openFolder"), icon: FolderOpen, separated: true, disabled: !app.location, onSelect: () => void attempt(() => api.revealInstalled(app.id)) },
    ...(app.uninstallable ? [{ label: t("installed.uninstall"), icon: Trash2, danger: true, separated: true, onSelect: () => void uninstallApp(app) }] : []),
  ];
}

/** 列表里名字下面那一行：有简介时写简介（看得出它是干什么的），没有时写发布者。 */
function useSubtitle(app: InstalledApp): { text: string; described: boolean } {
  const t = useT();
  const language = useLanguage();
  const key = useSummaryKey(app);
  const summary = useApp((state) => state.library?.summaries[key]);
  const translate = useApp((state) => state.library?.settings.translateSummaries ?? true);
  const shown = shownSummary(summary, language, translate);
  if (shown) return { text: shown.text, described: true };
  return { text: app.publisher || t(`source.${app.source}`), described: false };
}

/** 卸载按钮：鼠标停上去或选中时才出现，一直占着位置，列不会跳。 */
function UninstallButton({ app, selected }: { app: InstalledApp; selected: boolean }) {
  const t = useT();
  if (!app.uninstallable) return null;
  return (
    <Button
      variant="quiet"
      size="sm"
      className={selected ? "" : "invisible group-hover:visible group-focus-within:visible"}
      onClick={(event) => {
        event.stopPropagation();
        void uninstallApp(app);
      }}
    >
      {t("installed.uninstall")}
    </Button>
  );
}

/** 列表视图的一行。 */
function InstalledRow({
  app,
  selected,
  group,
  size,
  software,
  onSelect,
  onOpen,
}: {
  app: InstalledApp;
  selected: boolean;
  group: string[] | null;
  size: number | null;
  software?: Software;
  onSelect: (event: { metaKey: boolean; ctrlKey: boolean; shiftKey: boolean }) => void;
  onOpen: (id: string) => void;
}) {
  const language = useLanguage();
  const menu = useInstalledMenu(app, group);
  const subtitle = useSubtitle(app);
  const selectInstalled = useApp((state) => state.selectInstalled);
  const columns = useContext(ColumnsContext) as Columns<InstalledColumn>;
  const show = (key: InstalledColumn) => columns.visible.includes(key);
  return (
    <ContextMenu items={menu}>
      <div
        role="button"
        tabIndex={0}
        aria-pressed={selected}
        onClick={onSelect}
        // 右键的不在多选里时，先单选它，菜单作用于它自己
        onContextMenu={() => !group && !selected && selectInstalled(app.id)}
        onKeyDown={(event) => event.key === "Enter" && onSelect(event)}
        style={{ gridTemplateColumns: columns.template }}
        className={cx("group grid min-h-[52px] items-center gap-x-3 rounded-md py-1.5 pr-3 pl-2.5 transition-colors", selected ? "bg-active" : "hover:bg-hover")}
      >
        <div className="flex min-w-0 items-center gap-2.5">
          <SoftwareIcon name={app.name} icon={app.icon} />
          <div className="min-w-0">
            <div className="truncate text-[13px] font-medium">{app.name}</div>
            <div className={cx("truncate text-[12px]", subtitle.described ? "text-fg-2" : "text-fg-3")} title={subtitle.described ? `${subtitle.text}\n${app.publisher}` : undefined}>
              {subtitle.text}
            </div>
          </div>
        </div>
        {show("version") && <div className="tabular truncate text-[12px] text-fg-2">{app.version}</div>}
        {show("repo") && (
          <div className="flex min-w-0">
            <RepoCell software={software} onOpen={onOpen} />
          </div>
        )}
        {show("date") && <div className="tabular truncate text-right text-[12px] text-fg-3">{app.installedAt && formatDate(app.installedAt, language)}</div>}
        {show("size") && <div className="tabular truncate text-right text-[12px] text-fg-2">{size === null ? "" : formatSize(size)}</div>}
        {show("action") && (
          <div className="flex justify-end">
            <UninstallButton app={app} selected={selected} />
          </div>
        )}
      </div>
    </ContextMenu>
  );
}

/** 网格视图里的一个：居中的大图标，下面是名字和版本。仓库里有它的安装包时，图标角上有个小标记。 */
function InstalledTile({
  app,
  selected,
  group,
  software,
  onSelect,
}: {
  app: InstalledApp;
  selected: boolean;
  group: string[] | null;
  software?: Software;
  onSelect: (event: { metaKey: boolean; ctrlKey: boolean; shiftKey: boolean }) => void;
}) {
  const t = useT();
  const menu = useInstalledMenu(app, group);
  const subtitle = useSubtitle(app);
  const selectInstalled = useApp((state) => state.selectInstalled);
  return (
    <ContextMenu items={menu}>
      <div
        role="button"
        tabIndex={0}
        aria-pressed={selected}
        title={subtitle.described ? subtitle.text : undefined}
        onClick={onSelect}
        onContextMenu={() => !group && !selected && selectInstalled(app.id)}
        onKeyDown={(event) => event.key === "Enter" && onSelect(event)}
        className={cx(
          "group flex min-w-0 flex-col items-center rounded-lg px-2 pt-4 pb-2 text-center transition-colors",
          selected ? "bg-accent-soft" : "hover:bg-hover",
        )}
      >
        <span className="relative">
          <SoftwareIcon name={app.name} icon={app.icon} size={48} />
          {software && (
            <span
              title={t("installed.inLibrary")}
              aria-label={t("installed.inLibrary")}
              className="absolute -right-1.5 -bottom-1 flex size-[18px] items-center justify-center rounded-full bg-bg text-accent"
            >
              <PackageCheck size={13} strokeWidth={2} />
            </span>
          )}
        </span>
        <span className="mt-2.5 line-clamp-2 w-full text-[13px] leading-snug font-medium break-words">{app.name}</span>
        <span className="tabular mt-0.5 max-w-full truncate text-[12px] text-fg-3">{app.version || "—"}</span>
        <div className="mt-1.5 flex h-6 items-center justify-center">
          <UninstallButton app={app} selected={selected} />
        </div>
      </div>
    </ContextMenu>
  );
}

/** “仓库”一列：仓库里有它的安装包时写默认版本号，点了到软件库里看；没有时写“未收录”。 */
function RepoCell({ software, onOpen }: { software?: Software; onOpen: (id: string) => void }) {
  const t = useT();
  if (!software) return <span className="truncate text-[12px] text-fg-4">{t("installed.repoNone")}</span>;
  const version = primaryVersion(software)?.version ?? "";
  return (
    <button
      type="button"
      title={t("installed.showInLibrary")}
      onClick={(event) => {
        event.stopPropagation();
        onOpen(software.id);
      }}
      className="tabular -ml-1 min-w-0 truncate rounded px-1 text-left text-[12px] text-fg-2 hover:bg-active hover:text-accent"
    >
      {version ? t("installed.repoHas", { version }) : software.name}
    </button>
  );
}

export function InstalledDetail() {
  const many = useApp((state) => state.selectedInstalledIds.length > 1);
  return many ? <InstalledBatch /> : <InstalledSingle />;
}

/** 多选时的详情栏：一起取简介、让 AI 写简介。 */
function InstalledBatch() {
  const t = useT();
  const ids = useApp((state) => state.selectedInstalledIds);
  const index = useApp((state) => state.installedIndex);
  // 不能在选择器里拼新数组：每次都是新的，界面会一直重画
  const apps = useMemo(() => ids.map((id) => index[id]).filter(Boolean), [ids, index]);
  const requestSummary = useApp((state) => state.requestSummary);
  const describeWithAi = useApp((state) => state.describeWithAi);
  const selectInstalled = useApp((state) => state.selectInstalled);
  const aiReady = useAiReady();
  const writing = useApp((state) => summaryTargets(ids).some((target) => state.aiBusy[target.name]));
  const fetching = useApp((state) => summaryTargets(ids).some((target) => state.summaryBusy[target.name]));
  return (
    <div className="flex h-full flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto px-4 pt-4 pb-4">
        <div className="flex items-center gap-2">
          <CheckSquare size={18} strokeWidth={1.75} aria-hidden className="text-accent" />
          <span className="text-[15px] font-semibold">{t("batch.title", { n: apps.length })}</span>
        </div>
        <p className="mt-1.5 mb-0 text-[12px] leading-relaxed text-fg-3">{t("batch.keys")}</p>
        <div className="mt-4 flex flex-col gap-2">
          <Button className="w-full" busy={fetching} onClick={() => summaryTargets(ids).forEach((target) => requestSummary(target.name, true))}>
            <RefreshCw size={14} strokeWidth={1.75} aria-hidden />
            {t("installed.fetchSummaries", { n: apps.length })}
          </Button>
          {aiReady && (
            <Button className="w-full" busy={writing} onClick={() => void describeWithAi(summaryTargets(ids))}>
              <WandSparkles size={14} strokeWidth={1.75} aria-hidden />
              {t("installed.aiSummaries", { n: apps.length })}
            </Button>
          )}
        </div>
        <div className="mt-4 rounded-lg border border-line">
          {apps.map((app) => (
            <div key={app.id} className="flex h-9 items-center gap-2 border-b border-line px-2.5 last:border-b-0">
              <SoftwareIcon name={app.name} icon={app.icon} size={20} />
              <span className="min-w-0 flex-1 truncate text-[13px]">{app.name}</span>
            </div>
          ))}
        </div>
      </div>
      <div className="shrink-0 border-t border-line p-3">
        <Button variant="ghost" className="w-full" onClick={() => selectInstalled(null)}>
          {t("batch.clear")}
        </Button>
      </div>
    </div>
  );
}

function InstalledSingle() {
  const t = useT();
  const language = useLanguage();
  const app = useApp((state) => (state.selectedInstalledId ? state.installedIndex[state.selectedInstalledId] : undefined));
  const measured = useApp((state) => (app ? state.sizes[app.id] : undefined));
  const measuring = useApp((state) => state.measuring);
  // 仓库里和它对应的软件
  const linked = useApp((state) => {
    const softwareId = app && Object.keys(state.links).find((id) => state.links[id] === app.id);
    return softwareId ? state.library?.software.find((s) => s.id === softwareId) : undefined;
  });
  const attempt = useApp((state) => state.attempt);
  const reveal = useApp((state) => state.reveal);
  // 简介按名字存：仓库里有它就用仓库里那个软件的名字，否则用去掉版本号、架构之后的名字
  const summaryKey = linked?.name ?? (app ? summaryName(app.name) : "");
  const summary = useApp((state) => state.library?.summaries[summaryKey]);
  const summaryBusy = useApp((state) => Boolean(state.summaryBusy[summaryKey]));
  const requestSummary = useApp((state) => state.requestSummary);
  // 选中哪个就去取哪个的简介（开着“联网获取简介”时；取过的不再取）
  useEffect(() => {
    if (summaryKey) requestSummary(summaryKey);
  }, [summaryKey, requestSummary]);

  if (!app) return <EmptyState icon={MousePointerClick} title={t("installed.select")} />;

  const size = measured ?? app.size ?? null;
  const showInLibrary = () => linked && reveal(linked.id);

  return (
    <div className="flex h-full flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto px-4 pt-4 pb-4">
        <div className="flex items-center gap-3">
          <SoftwareIcon name={app.name} icon={app.icon} size={44} />
          <div className="min-w-0">
            <div className="selectable truncate text-[15px] font-semibold">{app.name}</div>
            <div className="truncate text-[12px] text-fg-2">{app.publisher}</div>
          </div>
        </div>

        <SummaryText
          key={summaryKey}
          summary={summary}
          busy={summaryBusy}
          onFetch={() => requestSummary(summaryKey, true)}
          ai={summaryKey ? { name: summaryKey, publisher: app.publisher } : undefined}
          name={summaryKey || undefined}
        />

        <div
          className={cx(
            "mt-4 flex items-center gap-2 rounded-lg px-2.5 py-2 text-[12px]",
            linked ? "bg-accent-soft text-accent" : "bg-hover text-fg-3",
          )}
        >
          {linked ? <PackageCheck size={15} strokeWidth={1.75} aria-hidden /> : <PackageX size={15} strokeWidth={1.75} aria-hidden />}
          <span className="min-w-0 flex-1">{linked ? t("installed.inLibrary") : t("installed.notInLibrary")}</span>
          {linked && (
            <button type="button" onClick={showInLibrary} className="shrink-0 font-medium underline-offset-2 hover:underline">
              {t("installed.showInLibrary")}
            </button>
          )}
        </div>

        <dl className="mt-4 mb-0">
          <Fact label={t("installed.version")}>{app.version || t("installed.unknown")}</Fact>
          <Fact label={t("installed.source")}>
            <Chip>{t(`source.${app.source}`)}</Chip>
          </Fact>
          <Fact label={t("installed.date")}>{app.installedAt ? formatDate(app.installedAt, language) : t("installed.unknown")}</Fact>
          <Fact label={t("installed.size")}>
            {size !== null ? formatSize(size) : measuring ? t("installed.measuring") : t("installed.unknown")}
          </Fact>
          <Fact label={t("installed.location")}>
            <span className="selectable font-mono text-[12px] break-all text-fg-2">{app.location || t("installed.unknown")}</span>
          </Fact>
        </dl>
      </div>

      <div className="flex shrink-0 flex-col gap-2 border-t border-line p-3">
        <div className="flex gap-2">
          <Button className="flex-1" disabled={!app.location} onClick={() => void attempt(() => api.revealInstalled(app.id))}>
            <FolderOpen size={14} strokeWidth={1.75} aria-hidden />
            {t("installed.reveal")}
          </Button>
          <Button variant="caution" className="flex-1" disabled={!app.uninstallable} onClick={() => void uninstallApp(app)}>
            <Trash2 size={14} strokeWidth={1.75} aria-hidden />
            {t("installed.uninstall")}
          </Button>
        </div>
        {!app.uninstallable && (
          <div className="text-center text-[12px] text-fg-3">{app.source === "system" ? t("installed.protected") : t("installed.noUninstall")}</div>
        )}
      </div>
    </div>
  );
}

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-start gap-3 border-b border-line py-2 text-[13px] last:border-b-0">
      <dt className="w-[72px] shrink-0 text-[12px] leading-[19px] text-fg-3">{label}</dt>
      <dd className="tabular m-0 min-w-0 flex-1">{children}</dd>
    </div>
  );
}
