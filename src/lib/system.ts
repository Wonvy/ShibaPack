// 主页的系统状态：配置、此刻的 CPU / 内存 / 显卡 / 网络 / 磁盘、进程，和最近一分钟的走势。
//
// 只在主页（或任务管理器）开着、窗口在前面时每 2 秒向后端要一次；切走、最小化就停。什么都不存。

import { useEffect } from "react";
import { create } from "zustand";
import { api } from "./api";
import type { SystemOverview, SystemSample } from "./types";

/** 走势线保留多少个点：2 秒一个，一分钟。 */
const HISTORY = 30;
const INTERVAL = 2000;

interface SystemState {
  overview: SystemOverview | null;
  sample: SystemSample | null;
  /** 最近的 CPU 占用（0–100），旧的在前。 */
  cpuHistory: number[];
  /** 最近的内存占用（0–100）。 */
  memoryHistory: number[];
  /** 最近的显卡占用（0–100）。 */
  gpuHistory: number[];
  /** 最近每秒下载、上传的字节数。 */
  downHistory: number[];
  upHistory: number[];
  /** 最近每秒读、写硬盘的字节数。 */
  readHistory: number[];
  writeHistory: number[];
  /** 读不到时后端给的原因，界面上用 errorText 翻译。 */
  error: unknown;
  /** 主页里打开了任务管理器（全部进程）。 */
  processesOpen: boolean;
  /** 进程的图标：可执行文件 → 图标文件。 */
  icons: Record<string, string>;
  /** 开着进程的右键菜单：先不刷新，免得列表重新排序、那一行从鼠标下面跑开。 */
  paused: boolean;
  refresh: () => Promise<void>;
}

export const useSystem = create<SystemState>()((set, get) => ({
  overview: null,
  sample: null,
  cpuHistory: [],
  memoryHistory: [],
  gpuHistory: [],
  downHistory: [],
  upHistory: [],
  readHistory: [],
  writeHistory: [],
  error: null,
  processesOpen: false,
  paused: false,
  icons: {},
  refresh: async () => {
    if (get().paused) return;
    try {
      if (!get().overview) set({ overview: await api.systemOverview() });
      const sample = await api.systemSample();
      const memory = sample.memoryTotal > 0 ? (sample.memoryUsed / sample.memoryTotal) * 100 : 0;
      set((state) => ({
        sample,
        error: null,
        cpuHistory: [...state.cpuHistory, sample.cpu].slice(-HISTORY),
        memoryHistory: [...state.memoryHistory, memory].slice(-HISTORY),
        gpuHistory: sample.gpu ? [...state.gpuHistory, sample.gpu.usage].slice(-HISTORY) : state.gpuHistory,
        downHistory: sample.network ? [...state.downHistory, sample.network.received].slice(-HISTORY) : state.downHistory,
        upHistory: sample.network ? [...state.upHistory, sample.network.sent].slice(-HISTORY) : state.upHistory,
        readHistory: sample.diskIo ? [...state.readHistory, sample.diskIo.read].slice(-HISTORY) : state.readHistory,
        writeHistory: sample.diskIo ? [...state.writeHistory, sample.diskIo.written].slice(-HISTORY) : state.writeHistory,
      }));
      void loadIcons(sample.processes.map((p) => p.exe).filter((exe): exe is string => Boolean(exe)));
    } catch (reason) {
      set({ error: reason ?? "internal" });
    }
  },
}));

/** 问过图标的可执行文件（取不到的也不再问）。 */
const asked = new Set<string>();
let loading = false;

/** 新出现的进程的图标：一次最多问 40 个，问完一批再问下一批，不和刷新抢。 */
async function loadIcons(exes: string[]) {
  if (loading) return;
  loading = true;
  try {
    for (;;) {
      const batch = [...new Set(exes)].filter((exe) => !asked.has(exe)).slice(0, 40);
      if (batch.length === 0) break;
      batch.forEach((exe) => asked.add(exe));
      const found = await api.processIcons(batch);
      if (Object.keys(found).length > 0) useSystem.setState((state) => ({ icons: { ...state.icons, ...found } }));
    }
  } catch {
    // 取不到图标不要紧，用默认的
  } finally {
    loading = false;
  }
}

/** 页面开着时调用：窗口在前面、页面看得见时每 2 秒刷新一次。 */
export function useSampling() {
  useEffect(() => {
    let timer: ReturnType<typeof setInterval> | null = null;
    let busy = false;
    const tick = async () => {
      if (busy) return;
      busy = true;
      try {
        await useSystem.getState().refresh();
      } finally {
        busy = false;
      }
    };
    const active = () => document.visibilityState === "visible" && document.hasFocus();
    const update = () => {
      if (active() && timer === null) {
        void tick();
        timer = setInterval(() => void tick(), INTERVAL);
      } else if (!active() && timer !== null) {
        clearInterval(timer);
        timer = null;
      }
    };
    // 第一次打开时就算窗口不在前面也先读一次，页面不至于空着
    void tick();
    update();
    window.addEventListener("focus", update);
    window.addEventListener("blur", update);
    document.addEventListener("visibilitychange", update);
    return () => {
      if (timer !== null) clearInterval(timer);
      window.removeEventListener("focus", update);
      window.removeEventListener("blur", update);
      document.removeEventListener("visibilitychange", update);
    };
  }, []);
}
