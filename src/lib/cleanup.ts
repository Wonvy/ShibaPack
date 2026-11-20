// 清理：扫描结果、勾选、正在做的事。都只和这台电脑有关，不进仓库。
//
// 扫描只读，结果留在后端；清理时只把项的 id 和不要的条目序号传回去，后端在扫描结果里找路径、删之前再查一遍。
// 勾选记在本机的界面偏好里（没动过的按默认：安全档勾上，别的不勾）；不要的条目只在这次扫描里有效。

import { create } from "zustand";
import { api, onAiProgress, onCleanupItem, onCleanupProgress, onCleanupScan } from "./api";
import { type Key, errorText, translate } from "./i18n";
import { usePrefs } from "./prefs";
import { useApp } from "./store";
import type { BiText, CleanCategory, CleanItem, CleanProgress, CleanReport, CleanRun, Explanation, Language } from "./types";

/** 扫描结果放多久算旧：回到清理页时超过这么久就自动重新扫。 */
export const STALE_MS = 10 * 60_000;

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
  /** 扫描进度：量完几处、一共几处。 */
  scanProgress: { done: number; total: number } | null;
  /** 上一次扫描被取消了，结果不完整。 */
  partial: boolean;
  /** AI 的解释，按项 id。 */
  explanations: Record<string, Explanation>;
  explaining: Record<string, boolean>;
  /** 一批让 AI 解释时的进度。 */
  explainProgress: { done: number; total: number } | null;
  /** 底部状态栏显示的选择情况（清理页写进来）。 */
  summary: string;
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
  scanProgress: null,
  partial: false,
  explanations: {},
  explaining: {},
  explainProgress: null,
  summary: "",
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
    cancelRequested = false;
    set({ scanning: true, error: null, scanProgress: null, ...(first ? { items: [] } : {}) });
    // 第一次扫描时量完一项就先显示一项；重新扫描时先留着旧的，扫完一起换
    const stop = first
      ? await onCleanupItem((item) => {
          const items = get().items ?? [];
          if (!items.some((x) => x.id === item.id)) set({ items: [...items, item] });
        })
      : null;
    const stopProgress = await onCleanupScan((progress) => set({ scanProgress: { done: progress.done, total: progress.total } }));
    try {
      const items = await api.cleanupScan();
      const ids = new Set(items.map((item) => item.id));
      const selectedId = get().selectedId;
      // 取消了的扫描结果不完整：照样显示，上面提示一句
      set({ items, skips: {}, partial: cancelRequested, selectedId: selectedId && ids.has(selectedId) ? selectedId : null });
    } catch (error) {
      set({ error: errorText(language(), error) });
      if (get().items === null) set({ items: [] });
    } finally {
      stop?.();
      stopProgress();
      set({ scanning: false, scanProgress: null, scannedAt: Date.now() });
    }
    void get().loadHistory();
    api.cleanupOptions().then((options) => set({ keepCookies: options.keepCookies ?? [] }), () => {});
    // 以前让 AI 解释过的拿出来（只读缓存，不问 AI）
    const known = get().explanations;
    const missing = (get().items ?? []).filter((item) => !known[item.id]).map((item) => item.id);
    if (missing.length > 0) {
      api.aiExplainCleanup(missing, "cached").then(
        (result) => set({ explanations: { ...get().explanations, ...result.explanations } }),
        () => {},
      );
    }
  },
  toggleEntry: (id, index) => {
    const current = get().skips[id] ?? [];
    const next = current.includes(index) ? current.filter((i) => i !== index) : [...current, index];
    set({ skips: { ...get().skips, [id]: next } });
  },
  setEntries: (id, skip) => set({ skips: { ...get().skips, [id]: skip } }),
}));

/** 用户点了取消扫描。 */
let cancelRequested = false;

/** 停下正在进行的扫描。已经量完的照样显示。 */
export function cancelScan() {
  if (!useCleanup.getState().scanning) return;
  cancelRequested = true;
  void api.cleanupCancelScan().catch(() => {});
}

/** 回到清理页或窗口回到前台时：结果放得太久了就重新扫一遍。 */
export function rescanIfStale() {
  const state = useCleanup.getState();
  if (state.items === null || state.scanning || state.cleaning || state.confirming || state.report) return;
  if (Date.now() - state.scannedAt > STALE_MS) void state.scan();
}

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
  // 读不到大小、项数也不知道的回收站：照样能清空（交给访达）
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
  // 只重新量清过的这几项，别的不动
  try {
    const ids = items.map((item) => item.id);
    const updated = await api.cleanupRescan(ids);
    const left = new Set(updated.map((item) => item.id));
    const state = useCleanup.getState();
    const skips = Object.fromEntries(Object.entries(state.skips).filter(([id]) => !ids.includes(id)));
    useCleanup.setState({ items: updated, skips, selectedId: state.selectedId && left.has(state.selectedId) ? state.selectedId : null });
    void state.loadHistory();
  } catch {
    // 单项重扫不行（比如后端的扫描结果没了）就整个重扫
    await useCleanup.getState().scan();
  }
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

/** 让 AI 解释这一项：有记下的解释就直接用；`refresh` 为真（点了“重新解释”）才重新问。 */
export async function explainCleanup(item: CleanItem, refresh = false) {
  return explainMany([item], refresh);
}

/**
 * 让 AI 解释一批：以前解释过的直接用，没解释过的凑成一批问（每次请求 10 项），不是一项一项问。
 * `refresh` 为真时全部重新问。一批的时候最后提示解释了几项。
 */
export async function explainMany(items: CleanItem[], refresh = false) {
  const state = useCleanup.getState();
  const todo = items.filter((item) => !state.explaining[item.id]);
  if (todo.length === 0) return;
  useCleanup.setState({ explaining: { ...state.explaining, ...Object.fromEntries(todo.map((item) => [item.id, true])) } });
  const stop = todo.length > 1 ? await onAiProgress((progress) => useCleanup.setState({ explainProgress: progress })) : null;
  try {
    const result = await api.aiExplainCleanup(
      todo.map((item) => item.id),
      refresh ? "refresh" : "auto",
    );
    useCleanup.setState({ explanations: { ...useCleanup.getState().explanations, ...result.explanations } });
    if (todo.length > 1) {
      const n = Object.keys(result.explanations).length;
      const key = result.asked === 0 ? "startup.explainedCached" : result.missing > 0 ? "startup.explainedMissing" : "startup.explainedMany";
      useApp.getState().notify(tr(key, { n, asked: result.asked, missing: result.missing }));
    } else if (result.missing > 0) {
      useApp.getState().notify(errorText(language(), "ai_unknown"), "error");
    }
  } catch (error) {
    useApp.getState().notify(errorText(language(), error), "error");
  } finally {
    stop?.();
    const explaining = { ...useCleanup.getState().explaining };
    for (const item of todo) delete explaining[item.id];
    useCleanup.setState({ explaining, explainProgress: null });
  }
}

/** 问访达废纸篓里有几项。第一次会弹授权；不允许时提示怎么打开。 */
export async function countTrash() {
  try {
    const items = await api.cleanupCountTrash();
    useCleanup.setState({ items });
  } catch (error) {
    useApp.getState().notify(errorText(language(), error), "error");
  }
}

/**
 * 这一项大概多快长回来（字节每天）：看清理记录里相邻两次清理，后一次清掉的就是这段时间里长出来的。
 * 至少要清过两次、隔了一天以上才算得出，算不出时是 null。
 */
export function regrowthPerDay(id: string, history: CleanRun[] | null): number | null {
  const cleaned = (history ?? [])
    .map((run) => ({ at: new Date(run.at).getTime(), item: run.items.find((x) => x.id === id) }))
    .filter((x): x is { at: number; item: NonNullable<typeof x.item> } => Boolean(x.item) && Number.isFinite(x.at))
    .sort((a, b) => a.at - b.at);
  let bytes = 0;
  let days = 0;
  for (let i = 1; i < cleaned.length; i++) {
    const gap = (cleaned[i].at - cleaned[i - 1].at) / 86_400_000;
    if (gap < 1) continue;
    bytes += cleaned[i].item.freed + cleaned[i].item.trashed;
    days += gap;
  }
  return days > 0 ? bytes / days : null;
}
