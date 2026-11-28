// 只和这台电脑有关的界面状态：栏宽、视图、排序、折叠的分类。存在 localStorage，不进仓库。

import { create } from "zustand";
import { persist } from "zustand/middleware";

export const SIDEBAR = { min: 140, max: 360 };
/** 侧栏收起后只剩一列图标的宽度。 */
export const SIDEBAR_RAIL = 48;
/** 顶栏的高度：拖动顶栏下沿可以调，双击回到默认。 */
export const TOPBAR = { min: 40, max: 72, initial: 52 };
export const DETAIL = { min: 280, max: 560, initial: 380 };
/** 磁盘分析右边列表的宽度：拖动中间的分隔线调，双击回到默认。 */
export const DISK_LIST = { min: 180, max: 520, initial: 240 };

export type SortKey = "name" | "platform" | "size" | "added";
export type PackageView = "list" | "grid" | "folder";

export type InstalledView = "list" | "grid";
export type InstalledSort = { column: "name" | "version" | "date" | "size"; asc: boolean };

/** 顶栏搜索的范围：下拉结果里列哪些。本机的几处之外，还可以在 GitHub、Homebrew 上找。 */
export const SEARCH_SCOPES = ["all", "library", "installed", "startup", "github", "homebrew"] as const;
export type SearchScope = typeof SEARCH_SCOPES[number];

/** 有可调列的表格：软件库、已安装软件、启动项、服务的列表。各记各的列宽和隐藏的列。 */
export type TableId = "library" | "installed" | "startup" | "service";
export interface TableColumns {
  /** 手动关掉的列。 */
  hidden: string[];
  /** 拖过的列宽，没拖过的用默认宽度。 */
  widths: Record<string, number>;
}
const NO_COLUMNS: TableColumns = { hidden: [], widths: {} };

interface Prefs {
  topBarHeight: number;
  /** 手动调整后的侧栏宽度。`null` 时按导航内容自动计算。 */
  sidebarWidth: number | null;
  /** 侧栏收起成一列图标。 */
  sidebarCollapsed: boolean;
  /** 毛玻璃：窗口背景透出桌面并模糊（macOS、Windows 11）。只和这台电脑有关，所以记在界面偏好里，不进仓库。 */
  glass: boolean;
  detailWidth: number;
  packageView: PackageView;
  sortKey: SortKey;
  sortAsc: boolean;
  /** 列表和文件夹视图在“全部软件”里按分类分组。 */
  groupByCategory: boolean;
  /** 网格视图单独记：卡片按组排时每组常常只有一两张，默认不分组。 */
  gridGroupByCategory: boolean;
  collapsed: string[];
  /** 软件列表里折叠起来的分组（分类 id，未分类是 `none`）。和侧栏分类树的折叠分开记。 */
  collapsedGroups: string[];
  /** 右边的详情栏是否展开。 */
  detailOpen: boolean;
  /** 详情栏里“更多信息”是否展开。 */
  detailMoreOpen: boolean;
  /** 已安装软件用列表还是网格。 */
  installedView: InstalledView;
  installedSort: InstalledSort;
  /** “启动与服务”页看的是启动项还是服务。 */
  startupTab: "startup" | "service";
  /** 收藏路径时把绝对路径自动换成变量的写法（`~`、`%APPDATA%`）。 */
  pathsConvert: boolean;
  /** 顶栏搜索的范围。 */
  searchScope: SearchScope;
  /** 固定在搜索框里的搜索范围。 */
  searchShortcuts: SearchScope[];
  /** 添加按钮在可拖动区域内的位置比例。`null` 表示默认的右下角。 */
  addButtonPosition: { x: number; y: number } | null;
  /** 各个表格的列宽和隐藏的列。 */
  tableColumns: Record<TableId, TableColumns>;
  /** 清理页里手动勾上或取消的项（项 id → 勾没勾）。没动过的按默认：安全档勾上，别的不勾。 */
  cleanupChecked: Record<string, boolean>;
  /** 清理页里手动展开或收起过的类别（类别 → 展开没有）。没动过的按默认：有勾选的展开，没有的收起。 */
  cleanupOpen: Record<string, boolean>;
  /** 磁盘分析用扇形图还是矩形图。 */
  /** list：当前文件夹里最大的文件，不分层级。 */
  diskChart: "sunburst" | "treemap" | "list";
  /** 磁盘分析右边列表的宽度。 */
  diskListWidth: number;
  /** 主页“最近安装”用列表还是图标。 */
  homeRecentView: "list" | "icons";
  /** 安装监控用深度范围。 */
  traceDeep: boolean;
  /** 点“安装”时默认勾上“监控这次安装”。 */
  traceInstalls: boolean;
  set: (patch: Partial<Omit<Prefs, "set" | "toggleCollapsed" | "sortBy" | "toggleColumn" | "setColumnWidth" | "resetColumns">>) => void;
  toggleCollapsed: (id: string) => void;
  sortBy: (key: SortKey) => void;
  toggleColumn: (table: TableId, key: string) => void;
  /** 宽度限制在 `range` 里；`null` 表示回到默认宽度。 */
  setColumnWidth: (table: TableId, key: string, width: number | null, range: { min: number; max: number }) => void;
  /** 所有列都显示，宽度都回到默认。 */
  resetColumns: (table: TableId) => void;
}

const clamp = (value: number, range: { min: number; max: number }) => Math.min(range.max, Math.max(range.min, value));

export const usePrefs = create<Prefs>()(
  persist(
    (set) => ({
      topBarHeight: TOPBAR.initial,
      sidebarWidth: null,
      sidebarCollapsed: false,
      glass: false,
      detailWidth: DETAIL.initial,
      packageView: "list",
      sortKey: "name",
      sortAsc: true,
      groupByCategory: true,
      gridGroupByCategory: false,
      collapsed: [],
      collapsedGroups: [],
      detailOpen: true,
      detailMoreOpen: false,
      installedView: "list",
      installedSort: { column: "name", asc: true },
      startupTab: "startup",
      pathsConvert: true,
      searchScope: "all",
      searchShortcuts: ["github", "homebrew"],
      addButtonPosition: null,
      tableColumns: { library: NO_COLUMNS, installed: NO_COLUMNS, startup: NO_COLUMNS, service: NO_COLUMNS },
      cleanupChecked: {},
      cleanupOpen: {},
      diskChart: "sunburst",
      diskListWidth: DISK_LIST.initial,
      homeRecentView: "list",
      traceDeep: false,
      traceInstalls: false,
      set: (patch) =>
        set((state) => {
          const { sidebarWidth = state.sidebarWidth } = patch;
          return {
            ...patch,
            topBarHeight: Math.round(clamp(patch.topBarHeight ?? state.topBarHeight, TOPBAR)),
            sidebarWidth: sidebarWidth === null ? null : clamp(sidebarWidth, SIDEBAR),
            detailWidth: clamp(patch.detailWidth ?? state.detailWidth, DETAIL),
            diskListWidth: Math.round(clamp(patch.diskListWidth ?? state.diskListWidth ?? DISK_LIST.initial, DISK_LIST)),
          };
        }),
      toggleCollapsed: (id) =>
        set((state) => ({
          collapsed: state.collapsed.includes(id) ? state.collapsed.filter((x) => x !== id) : [...state.collapsed, id],
        })),
      // 再点一次同一列就反过来排
      sortBy: (key) => set((state) => ({ sortKey: key, sortAsc: state.sortKey === key ? !state.sortAsc : true })),
      toggleColumn: (table, key) =>
        set((state) => {
          const current = state.tableColumns[table] ?? NO_COLUMNS;
          const hidden = current.hidden.includes(key) ? current.hidden.filter((k) => k !== key) : [...current.hidden, key];
          return { tableColumns: { ...state.tableColumns, [table]: { ...current, hidden } } };
        }),
      setColumnWidth: (table, key, width, range) =>
        set((state) => {
          const current = state.tableColumns[table] ?? NO_COLUMNS;
          const widths = { ...current.widths };
          if (width === null) delete widths[key];
          else widths[key] = Math.round(clamp(width, range));
          return { tableColumns: { ...state.tableColumns, [table]: { ...current, widths } } };
        }),
      resetColumns: (table) => set((state) => ({ tableColumns: { ...state.tableColumns, [table]: NO_COLUMNS } })),
    }),
    {
      name: "shibapack.ui",
      version: 3,
      migrate: (persisted, version) => {
        const saved = persisted as Partial<Prefs>;
        // 旧版会把默认的 180px 和其他界面偏好一起保存。
        if (version < 1 && saved.sidebarWidth === 180) saved.sidebarWidth = null;
        // 旧版按搜索范围菜单的顺序显示快捷图标。
        if (version < 2 && Array.isArray(saved.searchShortcuts)) {
          const shortcuts = saved.searchShortcuts;
          saved.searchShortcuts = SEARCH_SCOPES.filter((scope) => shortcuts.includes(scope));
        }
        return saved;
      },
    },
  ),
);
