// 安装监控窗口开着什么：开始监控一个安装包，或者看一条已有的记录。

import { create } from "zustand";
import type { TraceCategory, TraceChange } from "./types";

export type TraceTarget =
  | { mode: "start"; softwareId: string; versionId: string; fileId: string; fileName: string; softwareName: string; version: string }
  | { mode: "view"; softwareId: string; traceId: string }
  /** 上次没完成的监控（比如安装程序要求重启了电脑），ShibaPack 打开时接着来。 */
  | { mode: "resume" };

export const useTrace = create<{
  target: TraceTarget | null;
  /** 存下或删掉一条记录就加一，详情栏里的记录列表跟着刷新。 */
  revision: number;
}>(() => ({ target: null, revision: 0 }));

export const openTraceDialog = (target: TraceTarget) => useTrace.setState({ target });
export const closeTraceDialog = () => useTrace.setState({ target: null });
export const bumpTraces = () => useTrace.setState((state) => ({ revision: state.revision + 1 }));

/** 界面上类别的顺序：要紧的在前。 */
export const TRACE_CATEGORIES: TraceCategory[] = [
  "app",
  "startup",
  "service",
  "driver",
  "environment",
  "shortcut",
  "association",
  "uninstall",
  "data",
  "settings",
  "other",
];

/** 值得提醒的类别：开机就跑、常驻后台、进内核、改系统环境。 */
export const RISKY: TraceCategory[] = ["startup", "service", "driver", "environment"];

export interface TraceTotals {
  addedBytes: number;
  files: number;
  registry: number;
  byCategory: Partial<Record<TraceCategory, number>>;
  noise: number;
}

/** 一条记录的合计（不算噪音）。 */
export function traceTotals(changes: TraceChange[]): TraceTotals {
  const totals: TraceTotals = { addedBytes: 0, files: 0, registry: 0, byCategory: {}, noise: 0 };
  for (const change of changes) {
    if (change.noise) {
      totals.noise++;
      continue;
    }
    totals.byCategory[change.category] = (totals.byCategory[change.category] ?? 0) + 1;
    if (change.op === "added") totals.addedBytes += change.size ?? 0;
    if (change.kind === "file" || change.kind === "folder") totals.files += change.kind === "folder" && change.op !== "modified" ? (change.count ?? 0) + 1 : 1;
    else totals.registry += change.kind === "registryKey" ? (change.count ?? 0) + 1 : 1;
  }
  return totals;
}
