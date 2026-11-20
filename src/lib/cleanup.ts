// 清理：扫描结果、勾选、正在做的事。都只和这台电脑有关，不进仓库。
//
// 扫描只读，结果留在后端；清理时只把项的 id 和不要的条目序号传回去，后端在扫描结果里找路径、删之前再查一遍。
// 勾选记在本机的界面偏好里（没动过的按默认：安全档勾上，别的不勾）；不要的条目只在这次扫描里有效。

import { create } from "zustand";
import { api, onCleanupItem, onCleanupProgress } from "./api";
import { type Key, errorText, translate } from "./i18n";
import { usePrefs } from "./prefs";
import { useApp } from "./store";
import type { BiText, CleanCategory, CleanItem, CleanProgress, CleanReport, CleanRun, Language } from "./types";

/** 类别在列表里的顺序。 */
export const CATEGORIES: CleanCategory[] = ["system", "browser", "apps", "development", "installers", "privacy", "trash", "custom"];

interface CleanupState {
  /** 还没扫过时是 null。第一次扫描时一项一项加进来。 */
  items: CleanItem[] | null;
  scanning: boolean;
  /** 上次扫完的时间（毫秒）。 */
  scannedAt: number;
  error: string | null;
  selectedId: string | null;
  /** 不想清的条目：项 id → 条目序号。只在这次扫描里有效。 */
  skips: Record<string, number[]>;
  cleaning: boolean;
  progress: CleanProgress | null;
  /** 正在帮忙退出程序的项。 */
  quitting: Record<string, boolean>;
  /** 确认窗口里要清的项。 */
  confirming: string[] | null;
  /** 刚做完的清理结果，结果窗口显示它。 */
  report: CleanReport | null;
  /** 结果窗口要用到的：清理前那些项的名字。 */
  reportItems: CleanItem[];
  historyOpen: boolean;
  optionsOpen: boolean;
  /** 清理记录（算“上次清理后又长了多少”用）。还没读过时是 null。 */
  history: CleanRun[] | null;
  /** Cookie 保留名单（清理设置里的），确认窗口和详情栏要显示。 */
  keepCookies: string[];
  select: (id: string | null) => void;
  loadHistory: () => Promise<void>;
  scan: () => Promise<void>;
  toggleEntry: (id: string, index: number) => void;
  setEntries: (id: string, skip: number[]) => void;
}

export const useCleanup = create<CleanupState>()((set, get) => ({
  items: null,
  scanning: false,
  scannedAt: 0,
  error: null,
  selectedId: null,
  skips: {},
  cleaning: false,
  progress: null,
  quitting: {},
  confirming: null,
  report: null,
  reportItems: [],
  historyOpen: false,
  optionsOpen: false,
  history: null,
  keepCookies: [],
  select: (id) => set({ selectedId: id }),
  loadHistory: async () => {
    try {
      set({ history: await api.cleanupHistory() });
    } catch {
      set({ history: [] });
    }
  },
  scan: async () => {
    if (get().scanning || get().cleaning) return;
    const first = get().items === null;
    set({ scanning: true, error: null, ...(first ? { items: [] } : {}) });
    // 第一次扫描时量完一项就先显示一项；重新扫描时先留着旧的，扫完一起换
    const stop = first
      ? await onCleanupItem((item) => {
          const items = get().items ?? [];
          if (!items.some((x) => x.id === item.id)) set({ items: [...items, item] });
        })
      : null;
    try {
      const items = await api.cleanupScan();
      const ids = new Set(items.map((item) => item.id));
      const selectedId = get().selectedId;
      set({ items, skips: {}, selectedId: selectedId && ids.has(selectedId) ? selectedId : null });
    } catch (error) {
      set({ error: errorText(language(), error) });
      if (get().items === null) set({ items: [] });
    } finally {
      stop?.();
      set({ scanning: false, scannedAt: Date.now() });
    }
    void get().loadHistory();
    api.cleanupOptions().then((options) => set({ keepCookies: options.keepCookies ?? [] }), () => {});
  },
  toggleEntry: (id, index) => {
    const current = get().skips[id] ?? [];
    const next = current.includes(index) ? current.filter((i) => i !== index) : [...current, index];
    set({ skips: { ...get().skips, [id]: next } });
  },
  setEntries: (id, skip) => set({ skips: { ...get().skips, [id]: skip } }),
}));

function language(): Language {
  return useApp.getState().library?.settings.language ?? "zh-CN";
}

const tr = (key: Key, vars?: Record<string, string | number>) => translate(language(), key, vars);

/** 规则自带的文字：中文界面用中文，别的用英文。 */
export function textOf(text: BiText, lang: Language): string {
  return lang.startsWith("zh") ? text.zh : text.en;
}

/** 默认勾不勾：安全档勾上；要管理员权限的、隐私痕迹、别的档都不勾。 */
export function defaultChecked(item: CleanItem): boolean {
  return item.risk === "safe" && !item.admin && item.category !== "privacy";
}

/** 勾没勾（不看程序在不在运行）。 */
export function isChecked(item: CleanItem, overrides: Record<string, boolean>): boolean {
  return overrides[item.id] ?? defaultChecked(item);
}

/** 能不能清：程序没在运行、不是要到系统设置里清的、还有东西（要管理员权限的读不到也算，提权以后再量）。 */
export function canClean(item: CleanItem): boolean {
  if (item.running || item.tool) return false;
  return item.entryCount > 0 || item.action === "emptyTrash" || item.action === "clearClipboard" || item.admin;
}

/** 勾上或取消一批。和默认一样时不记，免得默认改了以后记着旧的。 */
export function setChecked(items: CleanItem[], checked: boolean) {
  const next = { ...usePrefs.getState().cleanupChecked };
  for (const item of items) {
    if (checked === defaultChecked(item)) delete next[item.id];
    else next[item.id] = checked;
  }
  usePrefs.getState().set({ cleanupChecked: next });
}

/** 这一项去掉不要的条目以后还有多大。读不到大小的是 0。 */
export function pickedSize(item: CleanItem, skips: Record<string, number[]>): number {
  const skip = skips[item.id] ?? [];
  if (skip.length === 0) return item.size ?? 0;
  const removed = item.entries.filter((e) => skip.includes(e.index)).reduce((sum, e) => sum + e.size, 0);
  return Math.max(0, (item.size ?? 0) - removed);
}

/** 勾着、能清的项。 */
export function pickedItems(items: CleanItem[], overrides: Record<string, boolean>): CleanItem[] {
  return items.filter((item) => isChecked(item, overrides) && canClean(item));
}

/** 打开确认窗口。 */
export function askClean(ids: string[]) {
  const items = useCleanup.getState().items ?? [];
  const ready = ids.filter((id) => items.some((item) => item.id === id && canClean(item)));
  if (ready.length === 0) return;
  useCleanup.setState({ confirming: ready });
}

/** 确认以后真的清理：做完显示结果，再重新扫一遍。 */
export async function runClean(ids: string[]) {
  const state = useCleanup.getState();
  if (state.cleaning) return;
  const items = (state.items ?? []).filter((item) => ids.includes(item.id));
  const picks = items.map((item) => ({ id: item.id, skip: state.skips[item.id] ?? [] }));
  useCleanup.setState({ cleaning: true, confirming: null, progress: { done: 0, total: 0, waitingAdmin: false } });
  const stop = await onCleanupProgress((progress) => useCleanup.setState({ progress }));
  try {
    const report = await api.cleanupRun(picks);
    useCleanup.setState({ report, reportItems: items });
  } catch (error) {
    useApp.getState().notify(errorText(language(), error), "error");
  } finally {
    stop();
    useCleanup.setState({ cleaning: false, progress: null });
  }
  await useCleanup.getState().scan();
}

/** 让这一项要先关掉的程序正常退出。 */
export async function quitApp(item: CleanItem) {
  if (useCleanup.getState().quitting[item.id]) return;
  useCleanup.setState({ quitting: { ...useCleanup.getState().quitting, [item.id]: true } });
  try {
    const updated = await api.cleanupQuitApp(item.id);
    useCleanup.setState({ items: (useCleanup.getState().items ?? []).map((x) => (x.id === updated.id ? updated : x)) });
    if (updated.running) useApp.getState().notify(tr("cleanup.quitFailed", { app: item.app ?? "" }), "error");
  } catch (error) {
    useApp.getState().notify(errorText(language(), error), "error");
  } finally {
    const { [item.id]: _done, ...rest } = useCleanup.getState().quitting;
    useCleanup.setState({ quitting: rest });
  }
}

/** 把还没收进仓库的安装包交给导入窗口（不要的条目除外）。 */
export function importLoose(item: CleanItem) {
  const skip = useCleanup.getState().skips[item.id] ?? [];
  const paths = item.entries.filter((e) => !skip.includes(e.index)).map((e) => e.path);
  if (paths.length > 0) useApp.getState().patch({ importDialog: { paths } });
}

/** 跳过的原因写成一句话。认不出的原样显示。 */
export function skipReason(reason: string): string {
  const key = `cleanup.reason.${reason}` as Key;
  const text = tr(key);
  return text === key ? reason : text;
}

/** 上次清理这一项是什么时候、清掉了多少。没清过时是 null。 */
export function lastCleaned(id: string, history: CleanRun[] | null): { at: string; size: number } | null {
  for (const run of history ?? []) {
    const item = run.items.find((x) => x.id === id);
    if (item) return { at: run.at, size: item.freed + item.trashed };
  }
  return null;
}
