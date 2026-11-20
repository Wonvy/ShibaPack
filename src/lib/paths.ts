// 收藏的路径：软件的配置文件夹这类，写上备注，可以备份进仓库、从备份恢复。
// 收藏本身记在仓库的索引里（换电脑也在）；在不在、有几份备份是这台电脑上现看的。

import { create } from "zustand";
import { confirm } from "../components/dialogs";
import { api } from "./api";
import { type Key, errorText, translate } from "./i18n";
import { formatSize } from "./library";
import { useApp } from "./store";
import type { PathBackup, PathState } from "./types";

interface PathsState {
  /** 每条收藏在这台电脑上的情况，按 id。还没看过时是 null。 */
  states: Record<string, PathState> | null;
  loading: boolean;
  selectedId: string | null;
  selectedIds: string[];
  /** 详情栏里列出来的备份，按 id。 */
  backups: Record<string, PathBackup[]>;
  /** 正在备份或恢复的：id → 在做什么。 */
  busy: Record<string, "backup" | "restore">;
  select: (id: string | null) => void;
  setSelection: (ids: string[], primary: string | null) => void;
  load: () => Promise<void>;
  loadBackups: (id: string) => Promise<void>;
}

export const usePaths = create<PathsState>()((set, get) => ({
  states: null,
  loading: false,
  selectedId: null,
  selectedIds: [],
  backups: {},
  busy: {},
  select: (id) => set({ selectedId: id, selectedIds: id ? [id] : [] }),
  setSelection: (ids, primary) => set({ selectedIds: ids, selectedId: primary ?? ids[0] ?? null }),
  load: async () => {
    if (get().loading) return;
    set({ loading: true });
    try {
      set({ states: await api.pathStates() });
    } catch (error) {
      useApp.getState().notify(errorText(language(), error), "error");
      if (get().states === null) set({ states: {} });
    } finally {
      set({ loading: false });
    }
  },
  loadBackups: async (id) => {
    try {
      const list = await api.pathBackups(id);
      set({ backups: { ...get().backups, [id]: list } });
    } catch {
      // 看不到就当没有
    }
  },
}));

function language() {
  return useApp.getState().library?.settings.language ?? "zh-CN";
}

const tr = (key: Key, vars?: Record<string, string | number>) => translate(language(), key, vars);

function itemOf(id: string) {
  return useApp.getState().library?.paths?.find((p) => p.id === id);
}

/** 显示用的名字：有备注用备注的第一行，没有就用路径最后一段。 */
export function pathTitle(item: { path: string; note: string }): string {
  const note = item.note.trim().split("\n")[0]?.trim();
  if (note) return note;
  return baseName(item.path);
}

export function baseName(path: string): string {
  const parts = path.split(/[\\/]+/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

/** 收藏一批路径。返回新加的 id，并选中最后一个。 */
export async function addPaths(paths: string[]): Promise<string[]> {
  const app = useApp.getState();
  let added: string[] = [];
  const ok = await app.apply(async () => {
    const result = await api.pathsAdd(paths);
    added = result.added;
    return result.library;
  });
  if (!ok) return [];
  if (added.length > 0) {
    app.notify(tr("paths.added", { n: added.length }));
    usePaths.getState().select(added[added.length - 1]);
  } else {
    app.notify(tr("paths.alreadyThere"));
  }
  await usePaths.getState().load();
  return added;
}

/** 备份一条或几条，一条接一条地做。 */
export async function backupPaths(ids: string[]) {
  const app = useApp.getState();
  let done = 0;
  let skipped = 0;
  let size = 0;
  for (const id of ids) {
    if (usePaths.getState().busy[id]) continue;
    usePaths.setState({ busy: { ...usePaths.getState().busy, [id]: "backup" } });
    try {
      const backup = await api.pathBackup(id);
      done += 1;
      skipped += backup.skippedCount;
      size += backup.size;
      await usePaths.getState().loadBackups(id);
    } catch (error) {
      const item = itemOf(id);
      app.notify(tr("paths.failedOne", { name: item ? pathTitle(item) : "", reason: errorText(language(), error) }), "error");
    } finally {
      const { [id]: _done, ...rest } = usePaths.getState().busy;
      usePaths.setState({ busy: rest });
    }
  }
  if (done > 0) {
    const text = done === 1 ? tr("paths.backedUp", { size: formatSize(size) }) : tr("paths.backedUpMany", { n: done, size: formatSize(size) });
    app.notify(skipped > 0 ? `${text} ${tr("paths.skipped", { n: skipped })}` : text, skipped > 0 ? "error" : "info");
  }
  await usePaths.getState().load();
}

/** 用一份备份替换现在的。先确认；现在的会先自动备份一份再送进回收站。 */
export async function restoreBackup(id: string, backup: PathBackup) {
  const item = itemOf(id);
  if (!item || usePaths.getState().busy[id]) return;
  const { ok } = await confirm({
    title: tr("paths.restoreConfirm.title", { name: pathTitle(item) }),
    body: tr("paths.restoreConfirm.body", { path: backup.source }),
    confirmLabel: tr("paths.restore"),
    danger: true,
  });
  if (!ok) return;
  usePaths.setState({ busy: { ...usePaths.getState().busy, [id]: "restore" } });
  try {
    const before = await api.pathRestore(id, backup.name);
    useApp.getState().notify(before ? tr("paths.restoredWithBackup") : tr("paths.restored"));
  } catch (error) {
    useApp.getState().notify(errorText(language(), error), "error");
  } finally {
    const { [id]: _done, ...rest } = usePaths.getState().busy;
    usePaths.setState({ busy: rest });
    await usePaths.getState().loadBackups(id);
    await usePaths.getState().load();
  }
}

export async function deleteBackup(id: string, backup: PathBackup) {
  const { ok } = await confirm({
    title: tr("paths.deleteBackupConfirm"),
    body: tr("paths.deleteBackupBody"),
    confirmLabel: tr("paths.deleteBackup"),
    danger: true,
  });
  if (!ok) return;
  const list = await useApp.getState().attempt(() => api.pathDeleteBackup(id, backup.name));
  if (list) usePaths.setState({ backups: { ...usePaths.getState().backups, [id]: list } });
  await usePaths.getState().load();
}

/** 取消收藏。有备份时问要不要一起删掉。 */
export async function removePaths(ids: string[]) {
  const library = useApp.getState().library;
  const items = (library?.paths ?? []).filter((p) => ids.includes(p.id));
  if (items.length === 0) return;
  const states = usePaths.getState().states ?? {};
  const withBackups = items.some((p) => (states[p.id]?.backups ?? 0) > 0);
  const { ok, checked } = await confirm({
    title: items.length === 1 ? tr("paths.removeConfirm", { name: pathTitle(items[0]) }) : tr("paths.removeConfirmMany", { n: items.length }),
    body: tr("paths.removeBody"),
    confirmLabel: tr("paths.remove"),
    danger: true,
    checkbox: withBackups
      ? { label: tr("paths.removeBackups"), hintOn: tr("paths.removeBackupsOn"), hintOff: tr("paths.removeBackupsOff") }
      : undefined,
  });
  if (!ok) return;
  const done = await useApp.getState().apply(() => api.pathsRemove(ids, checked));
  if (done) {
    const state = usePaths.getState();
    state.setSelection(state.selectedIds.filter((id) => !ids.includes(id)), null);
    await state.load();
  }
}
