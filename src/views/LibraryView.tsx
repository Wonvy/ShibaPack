// 软件库。工具栏左边是标题和筛选，右边是视图相关的按钮；下面是列表、网格或文件夹树。

import * as DropdownPrimitive from "@radix-ui/react-dropdown-menu";
import {
  ArrowDown,
  ArrowUp,
  Boxes,
  Check,
  ChevronDown,
  ChevronRight,
  ChevronsDownUp,
  ChevronsUpDown,
  CircleCheck,
  CircleDashed,
  File as FileIcon,
  FilePlus,
  Folder,
  FolderOpen,
  FolderTree,
  Globe,
  LayoutGrid,
  LayoutList,
  ListFilter,
  PackageOpen,
  Pin,
  RefreshCw,
  ArrowUpCircle,
  ScanSearch,
  SearchX,
  Sparkles,
  Star,
  Trash2,
  WandSparkles,
} from "lucide-react";
import { type PointerEvent, type ReactNode, createContext, memo, useContext, useDeferredValue, useEffect, useMemo, useRef, useState } from "react";
import { DetailToggle } from "../components/chrome";
import { type ColumnDef, type Columns, ColumnsContext, TableHeader, useTableColumns, useWidth } from "../components/columns";
import { PlatformIcon, PlatformIcons } from "../components/PlatformIcon";
import { SoftwareIcon, useSoftwareIcon } from "../components/SoftwareIcon";
import {
  Button,
  ContextMenu,
  EmptyState,
  FilterMenu,
  IconButton,
  type MenuEntry,
  MiddleTruncate,
  ViewMenu,
  cx,
  itemClass,
  menuClass,
} from "../components/ui";
import {
  classify,
  deleteSoftware,
  deleteSoftwareMany,
  installOrUpgrade,
  installSoftware,
  installerFor,
  moveManyTo,
  moveSoftwareTo,
  revealSoftware,
  setDefaultVersion,
  starMany,
  toggleStar,
  uninstallApp,
  upgradeFor,
} from "../lib/actions";
import { api, pickFiles, pickFolder } from "../lib/api";
import { beginDrag } from "../lib/drag";
import {
  PLATFORMS,
  categoryPath,
  fileCount,
  filterSoftware,
  flattenTree,
  formatDate,
  formatSize,
  hasUpdate,
  primaryVersion,
  sameVersion,
  softwarePlatforms,
  softwareSize,
  sortedVersions,
  versionPaths,
} from "../lib/library";
import { type PackageView, type SortKey, usePrefs } from "../lib/prefs";
import { type InstallFilter, useAiReady, useApp, useLanguage, useT } from "../lib/store";
import type { InstalledApp, Platform, Software, Version } from "../lib/types";

// 列表视图的列：名称、版本、平台、大小、添加时间、安装/卸载。名称占满剩下的宽度；右边几列的宽度可以拖，
// 右键表头勾选显示哪几列（见 components/columns）。栏太窄时添加时间、大小、平台从右往左暂时收起。
// 版本列默认要放得下“1.96.2 ⌄ → 1.97.0”，下面一行是本机装的版本
type LibraryColumn = "version" | "platform" | "size" | "added" | "action";
const LIBRARY_COLUMNS: ColumnDef<LibraryColumn>[] = [
  { key: "version", initial: 136, min: 96, max: 320 },
  { key: "platform", initial: 88, min: 56, max: 220, autoHide: 3 },
  { key: "size", initial: 72, min: 56, max: 160, end: true, autoHide: 2 },
  { key: "added", initial: 92, min: 76, max: 200, end: true, autoHide: 1 },
  { key: "action", initial: 64, min: 64, max: 64, end: true },
];
/** 一行里除了各列之外占的宽度：列表的左右留白加上行自己的左右留白。 */
const ROW_CHROME = 16 + 22;

export function LibraryView() {
  const t = useT();
  const library = useApp((state) => state.library)!;
  const scope = useApp((state) => state.scope);
  const search = useApp((state) => state.search);
  const platform = useApp((state) => state.platform);
  const installFilter = useApp((state) => state.installFilter);
  const links = useApp((state) => state.links);
  const installedKnown = useApp((state) => state.installed !== null);
  const selectedIds = useApp((state) => state.selectedIds);
  const selectedId = useApp((state) => state.selectedId);
  const collapsedGroups = usePrefs((state) => state.collapsedGroups);
  const select = useApp((state) => state.select);
  const patch = useApp((state) => state.patch);
  const checkUpdates = useApp((state) => state.checkUpdates);
  const checking = useApp((state) => state.checking !== null);
  // 一项一项地取：拖动分栏宽度时偏好设置在不停地变，不能让整个列表跟着重画
  const view = usePrefs((state) => state.packageView);
  const sortKey = usePrefs((state) => state.sortKey);
  const sortAsc = usePrefs((state) => state.sortAsc);
  const groupByCategory = usePrefs((state) => state.groupByCategory);
  const gridGroupByCategory = usePrefs((state) => state.gridGroupByCategory);
  // 打字时先让输入框跟上，列表稍后再筛：软件多的时候打字不卡
  const deferredSearch = useDeferredValue(search);
  const [frame, frameWidth] = useWidth<HTMLDivElement>();
  const columns = useTableColumns("library", LIBRARY_COLUMNS, frameWidth - ROW_CHROME);

  const list = useMemo(() => {
    const items = filterSoftware(library.software, library.categories, { scope, platform, installFilter, links, search: deferredSearch });
    const direction = sortAsc ? 1 : -1;
    const byName = (a: Software, b: Software) => a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" });
    const compare: Record<SortKey, (a: Software, b: Software) => number> = {
      name: byName,
      platform: (a, b) => (softwarePlatforms(a)[0] ?? "").localeCompare(softwarePlatforms(b)[0] ?? "") || byName(a, b),
      size: (a, b) => softwareSize(a) - softwareSize(b),
      added: (a, b) => a.addedAt.localeCompare(b.addedAt),
    };
    return [...items].sort((a, b) => compare[sortKey](a, b) * direction);
    // 只跟着软件和分类变：后台补上简介、图标时索引的别处在变，不用重新筛选排序
  }, [library.software, library.categories, scope, platform, installFilter, links, deferredSearch, sortKey, sortAsc]);

  // 看“全部软件”时按分类分组；已经在某个分类里或正在搜索时就不分了
  const groups = useMemo(() => {
    const wanted = view === "grid" ? gridGroupByCategory : groupByCategory;
    const grouped = wanted && scope.kind === "all" && !search.trim();
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
  }, [list, library.categories, view, groupByCategory, gridGroupByCategory, scope.kind, search, t]);

  // 分组可以折叠（点分组标题；Alt 点或者右键可以全部折叠、全部展开）。只在分组显示时才算数
  const folded = useMemo(() => new Set(groups.length > 1 || groups[0]?.title !== null ? collapsedGroups : []), [groups, collapsedGroups]);
  const toggleGroup = (key: string, all: boolean) => {
    const keys = groups.filter((group) => group.title !== null).map((group) => group.key);
    const open = folded.has(key);
    const next = all
      ? open
        ? collapsedGroups.filter((k) => !keys.includes(k))
        : [...new Set([...collapsedGroups, ...keys])]
      : open
        ? collapsedGroups.filter((k) => k !== key)
        : [...collapsedGroups, key];
    usePrefs.getState().set({ collapsedGroups: next });
  };
  const groupMenu: MenuEntry[] = [
    { label: t("cat.expandAll"), icon: ChevronsUpDown, disabled: folded.size === 0, onSelect: () => usePrefs.getState().set({ collapsedGroups: [] }) },
    {
      label: t("cat.collapseAll"),
      icon: ChevronsDownUp,
      onSelect: () => usePrefs.getState().set({ collapsedGroups: [...new Set([...collapsedGroups, ...groups.map((group) => group.key)])] }),
    },
  ];

  // 屏幕上从上到下的顺序：键盘上下移动、Shift 连选、全选都按它来，折叠起来的分组不算
  const ordered = useMemo(() => groups.flatMap((group) => (folded.has(group.key) ? [] : group.items)), [groups, folded]);

  // 从别处选中了一个藏在折叠分组里的软件（概况里点的、刚导入的），就把那一组展开
  useEffect(() => {
    if (!selectedId) return;
    const group = groups.find((g) => g.title !== null && g.items.some((s) => s.id === selectedId));
    if (group && folded.has(group.key)) usePrefs.getState().set({ collapsedGroups: collapsedGroups.filter((k) => k !== group.key) });
    // 只在选中的软件变了时检查：手动把选中软件所在的组折起来时不要又弹开
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedId]);
  const selectedSet = useMemo(() => new Set(selectedIds), [selectedIds]);

  // 换了左侧入口或筛选条件后，选中的软件如果不在列表里了，就不再选着它（详情栏回到概况）
  const filterKey = `${scope.kind}:${scope.kind === "category" ? scope.id : ""}:${platform}:${installFilter}`;
  useEffect(() => {
    const state = useApp.getState();
    const visible = new Set(list.map((s) => s.id));
    const kept = state.selectedIds.filter((id) => visible.has(id));
    if (kept.length === state.selectedIds.length) return;
    state.setSelection(kept, state.selectedId && visible.has(state.selectedId) ? state.selectedId : (kept[0] ?? null));
    // 只在筛选条件变化时检查；搜索时不动选中
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filterKey]);

  // 键盘：↑↓ 换选中（网格里 ←→ 也行），Shift 加方向键连选，⌘/Ctrl+A 全选，
  // Enter 安装或升级，Delete / Backspace 删除，Esc 取消选择
  const cursor = useRef<string | null>(null);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target instanceof HTMLElement ? event.target : null;
      if (target?.closest("input, textarea, select, [contenteditable='true']")) return;
      if (document.querySelector('[role="dialog"],[role="menu"]')) return;
      const app = useApp.getState();
      if (app.route !== "library") return;
      const ids = ordered.map((s) => s.id);
      const mod = event.metaKey || event.ctrlKey;
      const back = event.key === "ArrowUp" || (view === "grid" && event.key === "ArrowLeft");
      const forward = event.key === "ArrowDown" || (view === "grid" && event.key === "ArrowRight");
      if ((back || forward) && !mod && !event.altKey) {
        if (ids.length === 0) return;
        event.preventDefault();
        const from = app.selectedIds.length > 1 && cursor.current && ids.includes(cursor.current) ? cursor.current : app.selectedId;
        const at = from ? ids.indexOf(from) : -1;
        const to = at < 0 ? (forward ? 0 : ids.length - 1) : Math.min(ids.length - 1, Math.max(0, at + (forward ? 1 : -1)));
        const id = ids[to];
        cursor.current = id;
        const anchor = app.selectedId && ids.includes(app.selectedId) ? ids.indexOf(app.selectedId) : -1;
        if (event.shiftKey && anchor >= 0) app.setSelection(ids.slice(Math.min(anchor, to), Math.max(anchor, to) + 1), app.selectedId);
        else app.select(id);
        document.querySelector(`[data-software-id="${id}"]`)?.scrollIntoView({ block: "nearest" });
        return;
      }
      if (mod && event.key.toLowerCase() === "a") {
        event.preventDefault();
        app.setSelection(ids, app.selectedId && ids.includes(app.selectedId) ? app.selectedId : (ids[0] ?? null));
        return;
      }
      if (event.key === "Escape") {
        if (app.selectedIds.length > 0) app.select(null);
        return;
      }
      // 删除作用于全部选中的软件，包括搜索后暂时不在列表里的：和右键菜单、详情栏的批量面板一致，
      // 确认框里写着一共几个
      if ((event.key === "Delete" || event.key === "Backspace") && app.selectedIds.length > 0) {
        event.preventDefault();
        void deleteSoftwareMany(selectedSoftware(app.selectedIds));
        return;
      }
      const chosen = ordered.filter((s) => app.selectedIds.includes(s.id));
      if (event.key === "Enter" && chosen.length === 1) {
        // 焦点停在另一行上时（用 Tab 移过去的），Enter 先选中那一行
        const row = target?.closest<HTMLElement>("[data-software-id]");
        if (row && row.dataset.softwareId && row.dataset.softwareId !== chosen[0].id) {
          app.select(row.dataset.softwareId);
          return;
        }
        if (target?.closest("button, a")) return;
        event.preventDefault();
        void installOrUpgrade(chosen[0]);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [ordered, view]);

  const title = {
    all: t("nav.all"),
    starred: t("nav.starred"),
    updates: t("nav.updates"),
    uncategorized: t("nav.uncategorized"),
    category: scope.kind === "category" ? categoryPath(library.categories, scope.id) : "",
  }[scope.kind];
  const filtering = search.trim() !== "" || platform !== "all" || installFilter !== "all" || scope.kind !== "all";

  return (
    <div ref={frame} className="@container flex min-h-0 flex-1 flex-col">
      {/* 工具栏三段：左边标题和数量，正中间是筛选，右边是视图菜单和详情栏开关 */}
      <div className="grid h-11 shrink-0 grid-cols-[1fr_auto_1fr] items-center gap-2 border-b border-line pr-2 pl-4">
        <div className="flex min-w-0 items-center">
          <h2 className="m-0 min-w-12 truncate text-[14px] font-semibold" title={title}>
            {title}
          </h2>
          <span className="tabular ml-1.5 shrink-0 text-[12px] text-fg-3">{list.length}</span>
        </div>
        <div className="flex items-center gap-1">
          <FilterMenu<Platform | "all">
            label={t("lib.col.platform")}
            value={platform}
            onChange={(value) => patch({ platform: value })}
            options={[
              { value: "all", label: t("lib.allPlatforms"), icon: <Boxes size={15} strokeWidth={1.75} /> },
              ...PLATFORMS.map((p) => ({ value: p, label: t(`platform.${p}`), icon: <PlatformIcon platform={p} size={14} /> })),
            ]}
          />
          {installedKnown && (
            <FilterMenu<InstallFilter>
              label={t("link.installed")}
              value={installFilter}
              onChange={(value) => patch({ installFilter: value })}
              options={[
                { value: "all", label: t("link.all"), icon: <ListFilter size={15} strokeWidth={1.75} /> },
                { value: "installed", label: t("link.only"), icon: <CircleCheck size={15} strokeWidth={1.75} /> },
                { value: "missing", label: t("link.missing"), icon: <CircleDashed size={15} strokeWidth={1.75} /> },
              ]}
            />
          )}
        </div>
        <div className="flex min-w-0 items-center justify-end gap-1">
          {scope.kind === "updates" && (
            <IconButton label={t("scan.updates")} disabled={checking} onClick={() => void checkUpdates()}>
              <RefreshCw size={15} strokeWidth={1.75} className={checking ? "animate-spin" : ""} />
            </IconButton>
          )}
          <DisplayMenu />
          <DetailToggle />
        </div>
      </div>

      {/* 未分类：直接给出智能分类的入口 */}
      {scope.kind === "uncategorized" && list.length > 0 && (
        <div className="flex shrink-0 items-center gap-2 border-b border-line bg-hover py-1.5 pr-2 pl-4 text-[12px] text-fg-2">
          <Sparkles size={14} strokeWidth={1.75} aria-hidden className="shrink-0 text-accent" />
          <span className="min-w-0 flex-1">{t("classify.hint")}</span>
          <Button onClick={() => classify(list.map((s) => s.id))}>{t("classify.menu")}</Button>
        </div>
      )}

      {list.length === 0 ? (
        filtering ? (
          <EmptyState icon={SearchX} title={t("lib.noMatch")} />
        ) : (
          <Welcome />
        )
      ) : (
        <>
          <ColumnsContext.Provider value={columns}>
          {view === "list" && <ListHeader />}
          <ListContext.Provider value={ordered}>
          <div className="min-h-0 flex-1 overflow-y-auto pb-3" onClick={(event) => event.target === event.currentTarget && select(null)}>
            {groups.map((group) => (
              <section key={group.key}>
                {group.title !== null && (
                  <ContextMenu items={groupMenu}>
                    <button
                      type="button"
                      aria-expanded={!folded.has(group.key)}
                      onClick={(event) => toggleGroup(group.key, event.altKey)}
                      // 箭头放在左边的留白里，分组名和下面的软件图标仍然对齐
                      className="group/heading flex w-full items-center pt-4 pr-4 pb-1.5 pl-[3px] text-left text-[12px] text-fg-3 hover:text-fg-2"
                    >
                      <ChevronRight
                        size={12}
                        strokeWidth={2}
                        aria-hidden
                        className={cx("mr-px shrink-0 text-fg-4 transition-transform group-hover/heading:text-fg-3", !folded.has(group.key) && "rotate-90")}
                      />
                      <span>{group.title}</span>
                      <span className="tabular ml-2 text-fg-4">{group.items.length}</span>
                      <span className="ml-2 h-px flex-1 bg-line" />
                    </button>
                  </ContextMenu>
                )}
                {!folded.has(group.key) && view === "list" && (
                  <div className="px-2">
                    {group.items.map((software) => (
                      <Row key={software.id} software={software} selected={selectedSet.has(software.id)} />
                    ))}
                  </div>
                )}
                {!folded.has(group.key) && view === "grid" && (
                  <div className="grid grid-cols-[repeat(auto-fill,minmax(112px,1fr))] gap-1 px-3 pt-1">
                    {group.items.map((software) => (
                      <Card key={software.id} software={software} selected={selectedSet.has(software.id)} />
                    ))}
                  </div>
                )}
                {!folded.has(group.key) && view === "folder" && (
                  <div className="px-2">
                    {group.items.map((software) => (
                      <FolderNode key={software.id} software={software} selected={selectedSet.has(software.id)} />
                    ))}
                  </div>
                )}
              </section>
            ))}
          </div>
          </ListContext.Provider>
          </ColumnsContext.Provider>
        </>
      )}
    </div>
  );
}

/** 列表里屏幕上的顺序，Shift 点击连选时用。 */
const ListContext = createContext<Software[]>([]);

/** 点击选中：普通点击单选，⌘/Ctrl 点击加选或去掉，Shift 点击从起点连选到这里。 */
function clickSelect(id: string, event: { metaKey: boolean; ctrlKey: boolean; shiftKey: boolean }, ordered: Software[]) {
  const { selectedId, selectedIds, select, setSelection } = useApp.getState();
  if (event.metaKey || event.ctrlKey) {
    const next = selectedIds.includes(id) ? selectedIds.filter((x) => x !== id) : [...selectedIds, id];
    setSelection(next, next.includes(id) ? id : selectedId && next.includes(selectedId) ? selectedId : (next[0] ?? null));
    return;
  }
  const ids = ordered.map((s) => s.id);
  if (event.shiftKey && selectedId && ids.includes(selectedId) && ids.includes(id)) {
    const [a, b] = [ids.indexOf(selectedId), ids.indexOf(id)];
    setSelection(ids.slice(Math.min(a, b), Math.max(a, b) + 1), selectedId);
    return;
  }
  select(id);
}

const SORT_KEYS: SortKey[] = ["name", "platform", "size", "added"];

/** “显示选项”菜单：排序方式、顺序、是否按分类分组。网格视图的分组单独记。 */
/** 视图菜单：换列表、网格、文件夹视图，下面是排序和分组。 */
function DisplayMenu() {
  const t = useT();
  const prefs = usePrefs();
  const grid = prefs.packageView === "grid";
  const grouped = grid ? prefs.gridGroupByCategory : prefs.groupByCategory;
  const label = (text: string) => <DropdownPrimitive.Label className="px-2 pt-1.5 pb-1 text-[12px] text-fg-3">{text}</DropdownPrimitive.Label>;
  const indicator = (
    <DropdownPrimitive.ItemIndicator>
      <Check size={14} strokeWidth={2} className="text-accent" />
    </DropdownPrimitive.ItemIndicator>
  );
  const item = cx(itemClass, "justify-between gap-6 text-fg");
  return (
    <ViewMenu<PackageView>
      label={t("lib.view")}
      value={prefs.packageView}
      onChange={(value) => prefs.set({ packageView: value })}
      options={[
        { value: "list", label: t("lib.view.list"), icon: <LayoutList size={15} strokeWidth={1.75} /> },
        { value: "grid", label: t("lib.view.grid"), icon: <LayoutGrid size={15} strokeWidth={1.75} /> },
        { value: "folder", label: t("lib.view.folder"), icon: <FolderTree size={15} strokeWidth={1.75} /> },
      ]}
    >
          {label(t("lib.sortBy"))}
          <DropdownPrimitive.RadioGroup value={prefs.sortKey} onValueChange={(value) => prefs.set({ sortKey: value as SortKey })}>
            {SORT_KEYS.map((key) => (
              <DropdownPrimitive.RadioItem key={key} value={key} className={item}>
                {t(`lib.col.${key}`)}
                {indicator}
              </DropdownPrimitive.RadioItem>
            ))}
          </DropdownPrimitive.RadioGroup>
          <DropdownPrimitive.Separator className="mx-1 my-1 h-px bg-line" />
          <DropdownPrimitive.RadioGroup value={prefs.sortAsc ? "asc" : "desc"} onValueChange={(value) => prefs.set({ sortAsc: value === "asc" })}>
            <DropdownPrimitive.RadioItem value="asc" className={item}>
              {t("lib.sortAsc")}
              {indicator}
            </DropdownPrimitive.RadioItem>
            <DropdownPrimitive.RadioItem value="desc" className={item}>
              {t("lib.sortDesc")}
              {indicator}
            </DropdownPrimitive.RadioItem>
          </DropdownPrimitive.RadioGroup>
          <DropdownPrimitive.Separator className="mx-1 my-1 h-px bg-line" />
          <DropdownPrimitive.CheckboxItem
            checked={grouped}
            onCheckedChange={(checked) => prefs.set(grid ? { gridGroupByCategory: checked } : { groupByCategory: checked })}
            className={item}
          >
            {t("lib.group")}
            {indicator}
          </DropdownPrimitive.CheckboxItem>
    </ViewMenu>
  );
}

/** 列表视图的表头：点列名排序，拖列之间的竖线改宽度，右键勾选显示哪几列。 */
function ListHeader() {
  const t = useT();
  const columns = useContext(ColumnsContext) as Columns<LibraryColumn>;
  return (
    <TableHeader
      table="library"
      defs={LIBRARY_COLUMNS}
      columns={columns}
      className="pr-5 pl-[58px]"
      first={<SortHeader column="name" label={t("lib.col.name")} />}
      firstLabel={t("lib.col.name")}
      labels={{
        version: t("lib.col.version"),
        platform: t("lib.col.platform"),
        size: t("lib.col.size"),
        added: t("lib.col.added"),
        action: t("lib.col.action"),
      }}
      cells={{
        // 这一列只放按钮，不用列名
        action: <span />,
        platform: <SortHeader column="platform" label={t("lib.col.platform")} />,
        size: <SortHeader column="size" label={t("lib.col.size")} />,
        added: <SortHeader column="added" label={t("lib.col.added")} />,
      }}
    />
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
      {sortKey === column && <Arrow size={12} strokeWidth={2} aria-hidden />}
    </button>
  );
}

/** 一行、一张卡片或一个文件夹节点共用的行为：选中、右键菜单、拖到分类上、和已安装软件的对应。 */
function useItem(software: Software) {
  const t = useT();
  const ordered = useContext(ListContext);
  const select = useApp((state) => state.select);
  const installedApp = useApp((state): InstalledApp | undefined => {
    const id = state.links[software.id];
    return id ? state.installedIndex[id] : undefined;
  });
  // 这一行是多选里的一个时，右键菜单和拖拽都作用于全部选中的软件
  const group = useApp((state) =>
    state.selectedIds.length > 1 && selectionOf(state.selectedIds).has(software.id) ? state.selectedIds : null,
  );
  const aiReady = useAiReady();
  // 多选时作用于全部选中的软件，包括搜索后暂时不在列表里的（和详情栏的批量面板一致）
  const chosen = () => selectedSoftware(group ?? []);
  const describe = (list: Software[]): MenuEntry[] =>
    aiReady ? [{ label: t("ai.describe"), icon: WandSparkles, onSelect: () => void useApp.getState().describeWithAi(list.map((s) => ({ name: s.name }))) }] : [];
  const menu: MenuEntry[] = group
    ? [
        {
          label: chosen().every((s) => s.starred) ? t("batch.unstar") : t("batch.star"),
          icon: Star,
          onSelect: () => void starMany(chosen(), !chosen().every((s) => s.starred)),
        },
        { label: t("classify.menu"), icon: Sparkles, onSelect: () => classify(group) },
        ...describe(chosen()),
        { label: t("batch.delete", { n: group.length }), icon: Trash2, danger: true, separated: true, onSelect: () => void deleteSoftwareMany(chosen()) },
      ]
    : [
        {
          label: software.starred ? t("star.remove") : t("star.add"),
          icon: Star,
          onSelect: () => void toggleStar(software),
        },
        { label: t("classify.menu"), icon: Sparkles, onSelect: () => classify([software.id]) },
        ...describe([software]),
        { label: t("menu.openFolder"), icon: FolderOpen, separated: true, onSelect: () => void revealSoftware(software) },
        { label: t("detail.deleteSoftware"), icon: Trash2, danger: true, separated: true, onSelect: () => void deleteSoftware(software) },
      ];
  return {
    installedApp,
    upgrade: upgradeFor(software, installedApp),
    menu,
    handlers: {
      "data-software-id": software.id,
      onClick: (event: { metaKey: boolean; ctrlKey: boolean; shiftKey: boolean }) => clickSelect(software.id, event, ordered),
      onContextMenu: () => !group && select(software.id),
      onPointerDown: (event: PointerEvent) => {
        // 按着 ⌘/Ctrl/Shift 是在多选，不开始拖拽
        if (event.metaKey || event.ctrlKey || event.shiftKey) return;
        const ids = group ?? [software.id];
        beginDrag(
          event,
          { kind: "software", id: software.id, label: group ? t("drag.many", { n: group.length }) : software.name },
          (target) => void (group ? moveManyTo(ids, target.category) : moveSoftwareTo(software.id, target.category)),
        );
      },
    },
  };
}

/** 名称下面那行：发布者，有多个版本时接着写“N 个版本”；没填发布者时写文件数。简介只在详情栏里显示。 */
function useDescription(software: Software): string {
  const t = useT();
  const parts = [software.publisher || t("lib.files", { n: fileCount(software) })];
  if (software.versions.length > 1) parts.push(t("lib.versions", { n: software.versions.length }));
  return parts.join(" · ");
}

function StarMark({ software }: { software: Software }) {
  const t = useT();
  if (!software.starred) return null;
  return (
    <span title={t("nav.starred")} aria-label={t("nav.starred")} className="flex shrink-0 text-warning">
      <Star size={12} strokeWidth={2} fill="currentColor" />
    </span>
  );
}

/**
 * 仓库里的版本号（默认版本）；在线源上有更新的版本时，后面接着“→ 新版本号”。
 * 有多个版本时版本号是个下拉按钮，点开可以换默认版本；固定在旧版本上时带一个图钉。
 */
function VersionLabel({ software, installed, className }: { software: Software; installed?: string; className?: string }) {
  const t = useT();
  const root = useApp((state) => state.status?.repoRoot);
  const main = primaryVersion(software);
  const newest = sortedVersions(software)[0];
  const version = main?.version || "—";
  const latest = software.subscription?.latest;
  const online = latest && hasUpdate(software) ? latest.version : null;
  const pinnedOld = software.defaultVersionId !== null && main !== null && newest !== undefined && main.id !== newest.id;
  return (
    <span className={cx("tabular flex min-w-0 items-center gap-1 text-[12px] text-fg-2", className)}>
      {software.versions.length > 1 ? (
        <VersionMenu software={software} installed={installed} online={online}>
          <button
            type="button"
            aria-label={t("version.switch")}
            // 按下时不开始拖拽；点击照常选中这一行
            onPointerDown={(event) => event.stopPropagation()}
            className="-ml-1 flex min-w-0 items-center gap-0.5 rounded px-1 outline-none hover:bg-active hover:text-fg data-[state=open]:bg-active"
          >
            <span className="truncate">{version}</span>
            {pinnedOld && (
              <span title={t("version.pinned", { version, latest: newest.version })} className="flex shrink-0 text-fg-3">
                <Pin size={11} strokeWidth={2} aria-hidden />
              </span>
            )}
            <ChevronDown size={12} strokeWidth={2} aria-hidden className="shrink-0 opacity-70" />
          </button>
        </VersionMenu>
      ) : (
        // 只有一个版本：悬停时提示本机版本、在线的新版本和安装包的完整路径
        <span className="truncate" title={versionTitle(t, main, installed, online, root)}>
          {version}
        </span>
      )}
      <UpdateArrow version={online} />
    </span>
  );
}

/** 在线源上有新版本时版本号右边的小箭头。新版本号在悬停提示里，不占列表的位置。 */
function UpdateArrow({ version }: { version: string | null | undefined }) {
  const t = useT();
  if (!version) return null;
  const text = t("source.updateTo", { version });
  return (
    <span title={text} aria-label={text} className="flex shrink-0 text-accent">
      <ArrowUp size={12} strokeWidth={2.5} />
    </span>
  );
}

/** 版本号的悬停提示：本机装的版本（和仓库不一样时）、在线源上的新版本，再空一行列出安装包的完整路径。 */
function versionTitle(t: ReturnType<typeof useT>, main: Version | null, installed: string | undefined, online: string | null | undefined, root: string | null | undefined) {
  const lines = [
    installed && (!main || !sameVersion(main.version, installed)) ? t("local.version", { version: installed }) : null,
    online ? t("source.updateTo", { version: online }) : null,
  ].filter(Boolean);
  const paths = main ? versionPaths(main, root) : "";
  return [...lines, ...(lines.length > 0 && paths ? [""] : []), paths].join("\n") || undefined;
}

/** 换默认版本的菜单：“始终用最新版本”，或者固定在某一个版本上。本机装着的那个版本带“本机”标记。 */
/**
 * 换默认版本的下拉菜单。鼠标停在版本号上一会儿就展开，移开（又没移进菜单）就收起；点一下也能开关。
 * 焦点在输入框里（正在搜索）时不靠悬停展开，免得打字被打断。菜单里悬停某个版本，提示它的安装包的完整路径。
 */
function VersionMenu({
  software,
  installed,
  online,
  children,
}: {
  software: Software;
  installed?: string;
  online?: string | null;
  children: ReactNode;
}) {
  const t = useT();
  const root = useApp((state) => state.status?.repoRoot);
  const [open, setOpen] = useState(false);
  const byHover = useRef(false);
  const timer = useRef<number | undefined>(undefined);
  const later = (next: boolean, delay: number) => {
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      if (next) byHover.current = true;
      setOpen(next);
    }, delay);
  };
  const stay = () => window.clearTimeout(timer.current);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const item = cx(itemClass, "gap-3 pr-2 text-fg");
  const indicator = (
    <span className="flex w-3.5 shrink-0 justify-end">
      <DropdownPrimitive.ItemIndicator>
        <Check size={14} strokeWidth={2} className="text-accent" />
      </DropdownPrimitive.ItemIndicator>
    </span>
  );
  return (
    <DropdownPrimitive.Root
      open={open}
      modal={false}
      onOpenChange={(next) => {
        stay();
        byHover.current = false;
        setOpen(next);
      }}
    >
      <DropdownPrimitive.Trigger
        asChild
        onPointerEnter={() => {
          // 正在输入框里打字时不靠悬停展开：搜索时列表在鼠标底下滚动，不能把焦点抢走
          const typing = document.activeElement?.closest("input, textarea, [contenteditable='true']");
          if (!open && !typing) later(true, 180);
        }}
        onPointerLeave={() => later(false, 220)}
      >
        {children}
      </DropdownPrimitive.Trigger>
      <DropdownPrimitive.Portal>
        <DropdownPrimitive.Content
          align="start"
          sideOffset={4}
          className={cx(menuClass, "min-w-[220px]")}
          onClick={(event) => event.stopPropagation()}
          onPointerEnter={stay}
          onPointerLeave={() => later(false, 220)}
          onCloseAutoFocus={(event) => byHover.current && event.preventDefault()}
        >
          <DropdownPrimitive.Label className="px-2 pt-1.5 pb-1 text-[12px] text-fg-3">{t("version.pickTitle")}</DropdownPrimitive.Label>
          <DropdownPrimitive.RadioGroup
            value={software.defaultVersionId ?? "auto"}
            onValueChange={(value) => void setDefaultVersion(software, value === "auto" ? null : value)}
          >
            <DropdownPrimitive.RadioItem value="auto" className={item}>
              <span className="flex-1">{t("version.auto")}</span>
              {indicator}
            </DropdownPrimitive.RadioItem>
            <DropdownPrimitive.Separator className="mx-1 my-1 h-px bg-line" />
            {sortedVersions(software).map((version) => (
              <DropdownPrimitive.RadioItem key={version.id} value={version.id} className={item} title={versionPaths(version, root)}>
                <span className="tabular flex-1 truncate">{version.version || t("detail.unknownVersion")}</span>
                {installed && sameVersion(version.version, installed) && <span className="text-[12px] text-success">{t("version.local")}</span>}
                <span className="tabular text-[12px] text-fg-3">{t("lib.files", { n: version.files.length })}</span>
                {indicator}
              </DropdownPrimitive.RadioItem>
            ))}
          </DropdownPrimitive.RadioGroup>
          {/* 列表里只留一个版本号，别的版本信息在这里说 */}
          {(online || (installed && !software.versions.some((v) => sameVersion(v.version, installed)))) && (
            <>
              <DropdownPrimitive.Separator className="mx-1 my-1 h-px bg-line" />
              {installed && !software.versions.some((v) => sameVersion(v.version, installed)) && (
                <div className="tabular px-2 py-1 text-[12px] text-fg-3">{t("local.version", { version: installed })}</div>
              )}
              {online && (
                <div className="tabular flex items-center gap-1 px-2 py-1 text-[12px] text-accent">
                  <ArrowUp size={12} strokeWidth={2.5} aria-hidden />
                  {t("source.updateTo", { version: online })}
                </div>
              )}
            </>
          )}
        </DropdownPrimitive.Content>
      </DropdownPrimitive.Portal>
    </DropdownPrimitive.Root>
  );
}

/** 仓库还是空的：一块拖放区，加上三种添加方式。 */
function Welcome() {
  const t = useT();
  const importPaths = useApp((state) => state.importPaths);
  const patch = useApp((state) => state.patch);
  const dropping = useApp((state) => state.dropping);
  const importFiles = async () => importPaths(await pickFiles());
  const scanFolder = async () => {
    const folder = await pickFolder();
    if (folder) await importPaths([folder], "indexed");
  };
  const ways = [
    { icon: FilePlus, title: t("welcome.files"), hint: t("welcome.filesHint"), run: () => void importFiles() },
    { icon: ScanSearch, title: t("welcome.scan"), hint: t("welcome.scanHint"), run: () => void scanFolder() },
    { icon: Globe, title: t("welcome.online"), hint: t("welcome.onlineHint"), run: () => patch({ sourceDialog: { softwareId: null } }) },
  ];
  return (
    <div className="flex min-h-0 flex-1 items-center justify-center overflow-y-auto p-6">
      <div className="w-full max-w-[560px]">
        <div
          className={cx(
            "flex flex-col items-center rounded-xl border-2 border-dashed px-6 py-10 text-center transition-colors",
            dropping ? "border-accent bg-accent-soft" : "border-line-strong",
          )}
        >
          <PackageOpen size={36} strokeWidth={1.5} aria-hidden className={dropping ? "text-accent" : "text-fg-3"} />
          <h2 className="m-0 mt-4 text-[17px] font-semibold">{t("welcome.title")}</h2>
          <p className="m-0 mt-2 max-w-[400px] text-[13px] leading-relaxed text-fg-2">{t("welcome.body")}</p>
        </div>
        <div className="mt-4 grid grid-cols-1 gap-2 @xl:grid-cols-3">
          {ways.map((way) => (
            <button
              key={way.title}
              type="button"
              onClick={way.run}
              className="flex items-start gap-3 rounded-lg border border-line p-3 text-left transition-colors hover:border-line-strong hover:bg-hover @xl:flex-col @xl:gap-2"
            >
              <way.icon size={18} strokeWidth={1.75} aria-hidden className="mt-0.5 shrink-0 text-accent" />
              <span className="min-w-0">
                <span className="block text-[13px] font-medium">{way.title}</span>
                <span className="mt-0.5 block text-[12px] leading-relaxed text-fg-3">{way.hint}</span>
              </span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

/**
 * 安装 / 升级 / 卸载按钮，列表、卡片、文件夹视图共用。
 * “升级”是需要处理的新情况，一直显示；安装、卸载只在鼠标停在这一行上、或这一行被选中时出现，
 * 免得一整列按钮盖过软件名，也不容易误点卸载。所在的行要带 `group` 类。
 * 没有适合本机的安装包时显示一条短横线；装着但不能卸载（系统自带）时什么都不显示。
 */
function ActionButton({
  software,
  installedApp,
  upgrade,
  selected = false,
  dash = true,
}: {
  software: Software;
  installedApp?: InstalledApp;
  upgrade: boolean;
  /** 这一行被选中时，安装、卸载也一直显示。 */
  selected?: boolean;
  /** 没有适合本机的安装包时是否画一条短横线（列表里画，卡片里不画）。 */
  dash?: boolean;
}) {
  const t = useT();
  const installer = installerFor(software);
  // 不占位置地藏起来：列对齐不变，鼠标一停上去就出现
  const onHover = selected ? "" : "invisible group-hover:visible group-focus-within:visible";
  // 按下时不开始拖拽，点击时不改变选中
  const guard = { onPointerDown: (event: PointerEvent) => event.stopPropagation() };
  const run = (task: () => void) => (event: { stopPropagation: () => void }) => {
    event.stopPropagation();
    task();
  };
  if (upgrade) {
    // 能升级时平常只是一个小图标，鼠标停上去（或选中、键盘移过来）才换成“升级”按钮。按钮一直占着位置，列不会跳
    return (
      <span className="relative flex items-center justify-end">
        <Button variant="primary" size="sm" className={onHover} {...guard} onClick={run(() => void installSoftware(software, true))}>
          {t("link.upgrade")}
        </Button>
        {!selected && (
          <span
            title={t("link.canUpgrade")}
            aria-label={t("link.canUpgrade")}
            className="pointer-events-none absolute inset-y-0 right-0 flex items-center pr-1.5 text-accent group-focus-within:invisible group-hover:invisible"
          >
            <ArrowUpCircle size={16} strokeWidth={2} />
          </span>
        )}
      </span>
    );
  }
  if (!installedApp && installer) {
    return (
      <Button variant="soft" size="sm" className={onHover} title={installer.file.name} {...guard} onClick={run(() => void installSoftware(software))}>
        {t("link.install")}
      </Button>
    );
  }
  if (installedApp?.uninstallable) {
    return (
      <Button variant="quiet" size="sm" className={onHover} {...guard} onClick={run(() => void uninstallApp(installedApp))}>
        {t("link.uninstall")}
      </Button>
    );
  }
  if (installedApp || !dash) return null;
  return (
    <span title={t("link.noInstaller")} className={cx("px-2 text-[12px] text-fg-4", onHover)}>
      —
    </span>
  );
}

/** 名字旁的“已安装”对勾。鼠标停上去显示本机装的版本。 */
function InstalledMark({ app }: { app: InstalledApp | undefined }) {
  const t = useT();
  if (!app) return null;
  const text = app.version ? t("link.installedVersion", { version: app.version }) : t("link.installed");
  return (
    <span title={text} aria-label={text} className="flex shrink-0 text-success">
      <CircleCheck size={12} strokeWidth={2.25} />
    </span>
  );
}

/**
 * 本机装的版本，只在和仓库的默认版本不一样时显示（一样时名字旁的对勾已经说明了）。
 * 比仓库里能装的旧时用警示色，鼠标停上去说明可以升级。
 */
function LocalLabel({
  software,
  app,
  upgradeTo,
  className,
}: {
  software: Software;
  app: InstalledApp | undefined;
  upgradeTo: string | null;
  className?: string;
}) {
  const t = useT();
  const main = primaryVersion(software);
  if (!app?.version || (main && sameVersion(main.version, app.version))) return null;
  const text = t("local.version", { version: app.version });
  const title = upgradeTo ? t("local.older", { installed: app.version, version: upgradeTo }) : text;
  return (
    <span title={title} className={cx("tabular truncate text-[12px]", upgradeTo ? "text-warning" : "text-fg-3", className)}>
      {text}
    </span>
  );
}

// 每一行只在自己的软件或选中状态变了时重画
const Row = memo(function Row({ software, selected }: { software: Software; selected: boolean }) {
  const language = useLanguage();
  const columns = useContext(ColumnsContext);
  const show = (key: LibraryColumn) => columns.visible.includes(key);
  const { installedApp, upgrade, menu, handlers } = useItem(software);
  const icon = useSoftwareIcon(software);
  const description = useDescription(software);
  return (
    <ContextMenu items={menu}>
      <div
        role="button"
        tabIndex={0}
        aria-pressed={selected}
        {...handlers}
        style={{ gridTemplateColumns: columns.template }}
        className={cx(
          "group grid min-h-[52px] items-center gap-x-3 rounded-md py-1.5 pr-3 pl-2.5 transition-colors",
          selected ? "bg-active" : "hover:bg-hover",
        )}
      >
        <div className="flex min-w-0 items-center gap-2.5">
          <SoftwareIcon name={software.name} icon={icon} stack={software.versions.length} />
          <div className="min-w-0">
            <div className="flex items-center gap-1.5">
              <span className="truncate text-[13px] font-medium">{software.name}</span>
              <InstalledMark app={installedApp} />
              <StarMark software={software} />
            </div>
            <div className="truncate text-[12px] text-fg-3">{description}</div>
          </div>
        </div>
        {show("version") && (
          <div className="flex min-w-0">
            <VersionLabel software={software} installed={installedApp?.version} />
          </div>
        )}
        {show("platform") && <PlatformIcons platforms={softwarePlatforms(software)} className="min-w-0 overflow-hidden" />}
        {show("size") && <div className="tabular truncate text-right text-[12px] text-fg-3">{formatSize(softwareSize(software))}</div>}
        {show("added") && <div className="tabular truncate text-right text-[12px] text-fg-3">{formatDate(software.addedAt, language)}</div>}
        {show("action") && (
          <div className="flex justify-end">
            <ActionButton software={software} installedApp={installedApp} upgrade={upgrade !== null} selected={selected} />
          </div>
        )}
      </div>
    </ContextMenu>
  );
});

/**
 * 网格视图里的一个软件：居中的大图标，下面是名字和版本，别的都收起来。
 * 收藏和已安装是图标角上的小标记；安装、卸载按钮鼠标停上去或选中时才出现，能升级时一直显示。
 */
const Card = memo(function Card({ software, selected }: { software: Software; selected: boolean }) {
  const t = useT();
  const { installedApp, upgrade, menu, handlers } = useItem(software);
  const icon = useSoftwareIcon(software);
  const root = useApp((state) => state.status?.repoRoot);
  const main = primaryVersion(software);
  const version = main?.version || "—";
  const online = hasUpdate(software) ? software.subscription?.latest?.version : null;
  const installedText = installedApp && (installedApp.version ? t("link.installedVersion", { version: installedApp.version }) : t("link.installed"));
  return (
    <ContextMenu items={menu}>
      <div
        role="button"
        tabIndex={0}
        aria-pressed={selected}
        {...handlers}
        className={cx(
          "group flex min-w-0 flex-col items-center rounded-lg px-2 pt-4 pb-2 text-center transition-colors",
          selected ? "bg-accent-soft" : "hover:bg-hover",
        )}
      >
        <span className="relative">
          <SoftwareIcon name={software.name} icon={icon} size={48} stack={software.versions.length} />
          {software.starred && (
            <span title={t("nav.starred")} aria-label={t("nav.starred")} className="absolute -top-1.5 -right-1.5 flex size-[18px] items-center justify-center rounded-full bg-bg text-warning">
              <Star size={11} strokeWidth={2} fill="currentColor" />
            </span>
          )}
          {installedText && (
            <span title={installedText} aria-label={installedText} className="absolute -right-1.5 -bottom-1 flex size-[18px] items-center justify-center rounded-full bg-bg text-success">
              <CircleCheck size={14} strokeWidth={2.25} />
            </span>
          )}
        </span>
        <span className="mt-2.5 line-clamp-2 w-full text-[13px] leading-snug font-medium break-words">{software.name}</span>
        <span className="tabular mt-0.5 flex max-w-full min-w-0 items-center justify-center gap-1 text-[12px] text-fg-3">
          <span className="truncate" title={versionTitle(t, main, installedApp?.version, online, root)}>
            {version}
          </span>
          <UpdateArrow version={online} />
        </span>
        <div className="mt-1.5 flex h-6 items-center justify-center">
          <ActionButton software={software} installedApp={installedApp} upgrade={upgrade !== null} selected={selected} dash={false} />
        </div>
      </div>
    </ContextMenu>
  );
});

/** 文件夹视图里的一个软件：展开看版本，再展开看文件，和磁盘上的目录一一对应。 */
function FolderNode({ software, selected }: { software: Software; selected: boolean }) {
  const t = useT();
  const attempt = useApp((state) => state.attempt);
  const { installedApp, upgrade, menu, handlers } = useItem(software);
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
          onClick={(event) => {
            handlers.onClick(event);
            // 多选时只改选中，不展开
            if (!event.metaKey && !event.ctrlKey && !event.shiftKey) setOpen(!open);
          }}
          className={cx(line, "group pl-1.5", selected ? "bg-active" : "hover:bg-hover")}
        >
          <ChevronRight size={13} strokeWidth={2} aria-hidden className={cx("shrink-0 text-fg-3 transition-transform", open && "rotate-90")} />
          <SoftwareIcon name={software.name} icon={icon} size={20} />
          <span className="truncate font-medium">{software.name}</span>
          <InstalledMark app={installedApp} />
          <StarMark software={software} />
          <LocalLabel software={software} app={installedApp} upgradeTo={upgrade?.version.version ?? null} className="ml-1 shrink-0" />
          <span className="tabular ml-auto shrink-0 text-[12px] text-fg-3">
            {t("lib.versions", { n: software.versions.length })} · {formatSize(softwareSize(software))}
          </span>
          <span className="flex w-14 shrink-0 justify-end">
            <ActionButton software={software} installedApp={installedApp} upgrade={upgrade !== null} selected={selected} />
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
                    <MiddleTruncate text={file.name} title={file.path} className="selectable text-[12px]" />
                    <span className="ml-auto flex shrink-0 items-center gap-1.5">
                      <PlatformIcons platforms={[file.platform]} size={13} />
                      <span className="tabular w-[64px] text-right text-[12px] text-fg-3">{formatSize(file.size)}</span>
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

/** 按 id 取出选中的软件（保持选中的顺序），已经不在库里的跳过。 */
function selectedSoftware(ids: string[]): Software[] {
  const byId = new Map(useApp.getState().library?.software.map((s) => [s.id, s]));
  return ids.flatMap((id) => byId.get(id) ?? []);
}

/** 选中的 id 做成 Set，同一次选择只做一次：每一行都要问“我被选中没有”，不能每次都从头找。 */
const selectionSets = new WeakMap<string[], Set<string>>();
function selectionOf(ids: string[]): Set<string> {
  let set = selectionSets.get(ids);
  if (!set) {
    set = new Set(ids);
    selectionSets.set(ids, set);
  }
  return set;
}
