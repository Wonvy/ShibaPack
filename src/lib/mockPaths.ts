// 浏览器预览里的“路径”：收藏和备份都放在内存里。

import type { Library, PathBackup, PathState, SavedPath } from "./types";

const HOME = "/Users/demo";
let seeded = false;
const backups: Record<string, PathBackup[]> = {};
const missing = new Set(["/Users/demo/.config/nvim"]);

function expand(path: string): string {
  return path.replace(/^~/, HOME).replace(/\$HOME|\$\{HOME\}|%HOME%/g, HOME);
}

function portable(path: string): string {
  return path.startsWith(HOME) ? `~${path.slice(HOME.length)}` : path;
}

function backup(source: string, beforeRestore = false, hoursAgo = 0): PathBackup {
  const at = new Date(Date.now() - hoursAgo * 3600_000);
  return {
    name: at.toISOString().replace(/[-:.TZ]/g, ""),
    createdAt: at.toISOString(),
    source,
    size: 180_000 + Math.round(Math.random() * 4_000_000),
    files: 12 + Math.round(Math.random() * 200),
    skipped: [],
    skippedCount: 0,
    beforeRestore,
  };
}

export function seedPaths(lib: Library) {
  if (seeded) return;
  seeded = true;
  const now = new Date().toISOString();
  const items: SavedPath[] = [
    { id: "p1", path: "~/Library/Application Support/Code/User", note: "VS Code 的设置和快捷键", isDir: true, addedAt: now },
    { id: "p2", path: "~/.zshrc", note: "", isDir: false, addedAt: now },
    { id: "p3", path: "~/.config/nvim", note: "Neovim 配置", isDir: true, addedAt: now },
    { id: "p4", path: "%APPDATA%\\Typora\\themes", note: "Typora 主题（Windows 上用）", isDir: true, addedAt: now },
  ];
  lib.paths = items;
  backups.p1 = [backup(expand(items[0].path), false, 2), backup(expand(items[0].path), true, 30), backup(expand(items[0].path), false, 200)];
  backups.p3 = [backup(expand(items[2].path), false, 50)];
}

function state(item: SavedPath): PathState {
  const list = backups[item.id] ?? [];
  const unknown = item.path.includes("%APPDATA%");
  const full = unknown ? "" : expand(item.path);
  return {
    full,
    exists: !unknown && !missing.has(full),
    isDir: item.isDir,
    error: unknown ? "path_unknown_var: APPDATA" : null,
    backups: list.length,
    lastBackup: list[0]?.createdAt ?? null,
    backupSize: list.reduce((sum, b) => sum + b.size, 0),
    forms: unknown ? [] : full.startsWith(HOME) ? [portable(full), full] : [full],
  };
}

export async function mockPathsCall(command: string, args: Record<string, unknown>, lib: Library, snapshot: () => Library): Promise<unknown> {
  seedPaths(lib);
  const items = (lib.paths ??= []);
  const find = (id: unknown) => {
    const item = items.find((p) => p.id === id);
    if (!item) throw "not_found";
    return item;
  };
  switch (command) {
    case "paths_add": {
      const added: string[] = [];
      for (const raw of args.paths as string[]) {
        const path = args.convert === false ? raw.trim() : portable(raw.trim());
        if (!path.startsWith("~") && !path.startsWith("/") && !path.includes("%") && !path.includes("$")) throw "path_not_absolute";
        if (items.some((p) => expand(p.path) === expand(path))) continue;
        const id = `p${Date.now()}${added.length}`;
        items.push({ id, path, note: "", isDir: !/\.[a-z0-9]+$/i.test(path), addedAt: new Date().toISOString() });
        added.push(id);
      }
      return { library: snapshot(), added };
    }
    case "path_edit": {
      const item = find(args.id);
      if (typeof args.path === "string") item.path = args.convert === false ? args.path.trim() : portable(args.path);
      if (typeof args.note === "string") item.note = args.note.trim();
      return snapshot();
    }
    case "paths_remove":
      lib.paths = items.filter((p) => !(args.ids as string[]).includes(p.id));
      return snapshot();
    case "path_states":
      return Object.fromEntries(items.map((item) => [item.id, state(item)]));
    case "path_backup": {
      const item = find(args.id);
      if (!state(item).exists) throw "file_missing";
      await new Promise((r) => setTimeout(r, 700));
      const made = backup(expand(item.path));
      backups[item.id] = [made, ...(backups[item.id] ?? [])];
      return made;
    }
    case "path_backups":
      return backups[String(args.id)] ?? [];
    case "path_restore": {
      const item = find(args.id);
      await new Promise((r) => setTimeout(r, 600));
      const exists = state(item).exists;
      missing.delete(expand(item.path));
      if (!exists) return null;
      const before = backup(expand(item.path), true);
      backups[item.id] = [before, ...(backups[item.id] ?? [])];
      return before;
    }
    case "path_delete_backup": {
      const id = String(args.id);
      backups[id] = (backups[id] ?? []).filter((b) => b.name !== args.name);
      return backups[id];
    }
    default:
      return null;
  }
}
