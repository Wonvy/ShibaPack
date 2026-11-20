// 只和这台电脑有关的界面状态：栏宽、视图、排序、折叠的分类。存在 localStorage，不进仓库。

import { create } from "zustand";
import { persist } from "zustand/middleware";

export const SIDEBAR = { min: 140, max: 360, initial: 180 };
/** 侧栏收起后只剩一列图标时的宽度。 */
export const RAIL_WIDTH = 52;
export const DETAIL = { min: 280, max: 560, initial: 380 };

export type SortKey = "name" | "platform" | "size" | "added";
export type PackageView = "list" | "grid" | "folder";

export type InstalledView = "list" | "grid";

/** 有可调列的表格：软件库的列表、已安装软件的列表。各记各的列宽和隐藏的列。 */
export type TableId = "library" | "installed";
export interface TableColumns {
  /** 手动关掉的列。 */
  hidden: string[];
  /** 拖过的列宽，没拖过的用默认宽度。 */
  widths: Record<string, number>;
}
const NO_COLUMNS: TableColumns = { hidden: [], widths: {} };

interface Prefs {
  sidebarWidth: number;
  /** 侧栏收起成一列图标。展开时回到原来的宽度。 */
  sidebarCollapsed: boolean;
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
  /** 各个表格的列宽和隐藏的列。 */
  tableColumns: Record<TableId, TableColumns>;
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
      sidebarWidth: SIDEBAR.initial,
      sidebarCollapsed: false,
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
      tableColumns: { library: NO_COLUMNS, installed: NO_COLUMNS },
      set: (patch) =>
        set((state) => ({
          ...patch,
          sidebarWidth: clamp(patch.sidebarWidth ?? state.sidebarWidth, SIDEBAR),
          detailWidth: clamp(patch.detailWidth ?? state.detailWidth, DETAIL),
        })),
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
    { name: "shibapack.ui" },
  ),
);
