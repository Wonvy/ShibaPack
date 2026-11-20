// 磁盘分析：选哪里、扫描进度、扫完的结果、现在看到哪一层。扫完的树只在后端内存里，这里只拿要画的那一块。

import { create } from "zustand";
import { api, onDiskProgress } from "./api";
import type { DiskLocation, DiskNode, DiskProgress, DiskSummary } from "./types";

/** 往下取几层：扇形图画 4 圈，矩形图画 3 层。 */
export const VIEW_DEPTH = 4;
/** 占当前这一层不到这么多的合成“其他”（扇形图上已经看不清了）。 */
const MIN_RATIO = 0.004;

interface DiskState {
  locations: DiskLocation[] | null;
  /** 选中的地方（路径）。 */
  root: string | null;
  scanning: boolean;
  progress: DiskProgress | null;
  summary: DiskSummary | null;
  /** 现在看的那一层，相对扫描根的路径段。 */
  path: string[];
  view: DiskNode | null;
  error: unknown;
  /** 鼠标停着的那一项（路径段连起来；“其他”“小文件”用父路径加名字）。 */
  hover: string | null;
  /** AI 写的文件夹作用：路径段用 / 连起来 → 解释。缓存在仓库的 ai-folders.json 里，这里是这次看过的。 */
  explanations: Record<string, string>;
  /** 正在问 AI 的项。 */
  asking: Record<string, true>;
  /** 问过但没答上来的项，不再反复问。 */
  unanswered: Record<string, true>;
  /** 打开一层时先看缓存里有没有这一层的解释（不问 AI）。 */
  lookUp: (nodes: DiskNode[]) => Promise<void>;
  /** 问 AI：鼠标停着的那项，顺带这一层里还没解释过的（一次最多 10 个，一次请求）。 */
  ask: (node: DiskNode) => Promise<void>;
  loadLocations: () => Promise<void>;
  scan: (root?: string) => Promise<void>;
  cancel: () => void;
  open: (path: string[]) => Promise<void>;
  /** 重新取当前这一层（删了东西以后）。 */
  refresh: () => Promise<void>;
}

export const useDisk = create<DiskState>()((set, get) => ({
  locations: null,
  root: null,
  scanning: false,
  progress: null,
  summary: null,
  path: [],
  view: null,
  error: null,
  hover: null,
  explanations: {},
  asking: {},
  unanswered: {},
  lookUp: async (nodes) => {
    const known = get().explanations;
    const paths = nodes.filter((n) => explainable(n) && known[n.path.join("/")] === undefined).map((n) => n.path);
    if (paths.length === 0) return;
    try {
      const found = await api.aiExplainFolders(paths, "cached");
      if (Object.keys(found).length > 0) set((state) => ({ explanations: { ...state.explanations, ...found } }));
    } catch {
      // 没有缓存不要紧
    }
  },
  ask: async (node) => {
    const { explanations, asking, unanswered, view } = get();
    const key = node.path.join("/");
    if (!explainable(node) || explanations[key] !== undefined || asking[key] || unanswered[key]) return;
    const waiting = (n: DiskNode) => {
      const k = n.path.join("/");
      return explainable(n) && explanations[k] === undefined && !asking[k] && !unanswered[k];
    };
    const batch = [node, ...(view?.children ?? []).filter((n) => n !== node && waiting(n))].slice(0, 10);
    const keys = batch.map((n) => n.path.join("/"));
    set((state) => ({ asking: { ...state.asking, ...Object.fromEntries(keys.map((k) => [k, true as const])) } }));
    let found: Record<string, string> = {};
    try {
      found = await api.aiExplainFolders(batch.map((n) => n.path), "ask");
    } catch {
      // AI 出错时这一批先不再问；切换位置或重新扫描后可以再试
    }
    set((state) => {
      const asking = { ...state.asking };
      const unanswered = { ...state.unanswered };
      for (const k of keys) {
        delete asking[k];
        if (found[k] === undefined) unanswered[k] = true;
      }
      return { asking, unanswered, explanations: { ...state.explanations, ...found } };
    });
  },
  loadLocations: async () => {
    try {
      const locations = await api.diskLocations();
      set((state) => ({ locations, root: state.root ?? locations[0]?.path ?? null }));
    } catch (error) {
      set({ error });
    }
  },
  scan: async (root) => {
    const target = root ?? get().root;
    if (!target || get().scanning) return;
    set({ root: target, scanning: true, progress: null, error: null, summary: null, view: null, path: [], unanswered: {} });
    const unlisten = await onDiskProgress((progress) => set({ progress }));
    try {
      const summary = await api.diskScan(target);
      set({ summary });
      await get().open([]);
    } catch (error) {
      // 停下了不算错
      if (String(error) !== "cancelled") set({ error });
    } finally {
      unlisten();
      set({ scanning: false });
    }
  },
  cancel: () => void api.diskCancel(),
  open: async (path) => {
    try {
      const view = await api.diskView(path, VIEW_DEPTH, MIN_RATIO);
      set({ path, view, hover: null });
    } catch (error) {
      set({ error });
    }
  },
  refresh: async () => {
    const { path } = get();
    // 当前这一层被删掉了就往上退
    for (let n = path.length; n >= 0; n--) {
      try {
        const view = await api.diskView(path.slice(0, n), VIEW_DEPTH, MIN_RATIO);
        set({ path: path.slice(0, n), view });
        return;
      } catch {
        // 再往上一层
      }
    }
  },
}));

/** 一项的标识：路径段连起来；“其他”“小文件”没有路径，用父路径加类型。 */
export function nodeKey(node: DiskNode, parent: string[]): string {
  return node.path.length > 0 ? node.path.join("/") : `${parent.join("/")}/:${node.kind}`;
}

/** 能问作用的：真的文件夹或文件（不是“小文件”“其他”，也不是扫描的位置本身）。 */
export function explainable(node: DiskNode): boolean {
  return node.path.length > 0 && (node.kind === "dir" || node.kind === "file");
}
