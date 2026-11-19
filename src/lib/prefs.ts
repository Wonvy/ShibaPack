// 只和这台电脑有关的界面状态：栏宽、视图、排序、折叠的分类。存在 localStorage，不进仓库。

import { create } from "zustand";
import { persist } from "zustand/middleware";

export const SIDEBAR = { min: 140, max: 360, initial: 180 };
export const DETAIL = { min: 280, max: 560, initial: 380 };

export type SortKey = "name" | "platform" | "size" | "added";
export type PackageView = "list" | "grid" | "folder";

interface Prefs {
  sidebarWidth: number;
  detailWidth: number;
  packageView: PackageView;
  sortKey: SortKey;
  sortAsc: boolean;
  /** 列表和文件夹视图在“全部软件”里按分类分组。 */
  groupByCategory: boolean;
  /** 网格视图单独记：卡片按组排时每组常常只有一两张，默认不分组。 */
  gridGroupByCategory: boolean;
  collapsed: string[];
  /** 右边的详情栏是否展开。 */
  detailOpen: boolean;
  /** 详情栏里“更多信息”是否展开。 */
  detailMoreOpen: boolean;
  set: (patch: Partial<Omit<Prefs, "set" | "toggleCollapsed" | "sortBy">>) => void;
  toggleCollapsed: (id: string) => void;
  sortBy: (key: SortKey) => void;
}

const clamp = (value: number, range: { min: number; max: number }) => Math.min(range.max, Math.max(range.min, value));

export const usePrefs = create<Prefs>()(
  persist(
    (set) => ({
      sidebarWidth: SIDEBAR.initial,
      detailWidth: DETAIL.initial,
      packageView: "list",
      sortKey: "name",
      sortAsc: true,
      groupByCategory: true,
      gridGroupByCategory: false,
      collapsed: [],
      detailOpen: true,
      detailMoreOpen: false,
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
    }),
    { name: "shibapack.ui" },
  ),
);
