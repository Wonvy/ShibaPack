// 浏览器预览里的“磁盘分析”：一棵假的文件夹树，扫描时假装走一会儿。

import type { DiskLocation, DiskNode, DiskProgress, DiskSummary, LargeFile } from "./types";

const GB = 1024 ** 3;
const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
const listeners = new Set<(progress: DiskProgress) => void>();
let cancelled = false;

interface Fake {
  name: string;
  size?: number;
  files?: number;
  kind?: DiskNode["kind"];
  children?: Fake[];
}

/** 文件夹的大小是子项加起来；没写子项的是叶子。 */
const dir = (name: string, children: Fake[]): Fake => ({ name, children });
const file = (name: string, size: number): Fake => ({ name, size, files: 1, kind: "file" });
const leaf = (name: string, size: number, files: number): Fake => ({ name, size, files });
const small = (size: number, files: number): Fake => ({ name: "", size, files, kind: "small" });

const TREE: Fake = dir("demo", [
  dir("Library", [
    dir("Application Support", [
      dir("Google", [leaf("Chrome", 6.4 * GB, 41000), leaf("GoogleUpdater", 0.3 * GB, 900)]),
      dir("Code", [leaf("User", 1.9 * GB, 12000), leaf("CachedExtensionVSIXs", 0.8 * GB, 140), leaf("logs", 0.4 * GB, 3000)]),
      leaf("Figma", 2.2 * GB, 5200),
      leaf("Slack", 1.4 * GB, 8100),
      dir("Docker", [file("Docker.raw", 18 * GB)]),
      small(0.2 * GB, 2400),
    ]),
    dir("Caches", [leaf("com.spotify.client", 3.1 * GB, 900), leaf("Homebrew", 2.6 * GB, 310), leaf("com.apple.Safari", 0.9 * GB, 4000), leaf("pip", 1.2 * GB, 2100), small(0.3 * GB, 9000)]),
    dir("Developer", [leaf("Xcode", 24 * GB, 210000), dir("CoreSimulator", [leaf("Devices", 14 * GB, 98000), leaf("Caches", 3.2 * GB, 1200)])]),
    leaf("Containers", 4.8 * GB, 61000),
    leaf("Mail", 2.4 * GB, 33000),
    small(0.1 * GB, 1500),
  ]),
  dir("Downloads", [
    file("Xcode_16.dmg", 7.9 * GB),
    file("ubuntu-24.04-desktop-amd64.iso", 5.7 * GB),
    file("Figma-124.dmg", 0.4 * GB),
    file("Docker.dmg", 0.6 * GB),
    file("node-v22.pkg", 0.08 * GB),
    file("资料备份.zip", 3.4 * GB),
    file("播客合集.mp3", 0.3 * GB),
    file("设计稿.psd", 1.1 * GB),
    file("会议录像-2026-10.mp4", 2.3 * GB),
    leaf("旧照片备份", 9.6 * GB, 4200),
    small(0.6 * GB, 830),
  ]),
  dir("Movies", [file("家庭视频.mov", 14.2 * GB), file("旅行.mp4", 6.1 * GB), leaf("Final Cut Pro 资源库.fcpbundle", 21 * GB, 8400)]),
  dir("Pictures", [leaf("Photos Library.photoslibrary", 46 * GB, 52000), small(0.5 * GB, 900)]),
  dir("Documents", [leaf("项目资料", 12 * GB, 18000), leaf("扫描件", 1.3 * GB, 2400), file("虚拟机.vmdk", 32 * GB), small(0.9 * GB, 6100)]),
  dir("Desktop", [dir("ShibaPack", [leaf("target", 24.8 * GB, 140000), leaf("node_modules", 0.6 * GB, 52000), small(0.04 * GB, 900)]), leaf("Postflow", 18.3 * GB, 91000), small(0.2 * GB, 320)]),
  dir("Music", [leaf("Music", 8.4 * GB, 6100)]),
  leaf(".npm", 1.5 * GB, 61000),
  leaf(".cargo", 1.8 * GB, 21000),
  leaf(".rustup", 1.9 * GB, 66000),
  small(0.02 * GB, 120),
]);

/** 预览里只写了总大小的文件夹：编几个子文件夹出来，点进去也有东西看（真的扫描里大于 1 MB 的文件夹都能往里点）。 */
const PARTS = ["data", "cache", "resources", "logs", "lib"];
const SHARES = [0.42, 0.24, 0.16, 0.1, 0.06];

function invent(node: Fake, depth: number): Fake[] {
  const size = node.size ?? 0;
  const files = node.files ?? 0;
  const parts: Fake[] = PARTS.map((name, i) => {
    const part = { name, size: size * SHARES[i]!, files: Math.max(1, Math.round(files * SHARES[i]!)) };
    return depth < 2 && part.size > 200 * 1024 ** 2 ? { ...part, children: invent(part, depth + 1) } : part;
  });
  return [...parts, small(size * 0.02, Math.max(1, Math.round(files * 0.02)))];
}

/** 算出每个文件夹的大小、文件数，按大小排好。 */
function settle(node: Fake): Fake {
  if (!node.children && !node.kind && (node.size ?? 0) > 50 * 1024 ** 2) return settle({ ...node, children: invent(node, 0) });
  if (!node.children) return { ...node, kind: node.kind ?? "dir", size: node.size ?? 0, files: node.files ?? 0 };
  const children = node.children.map(settle).sort((a, b) => (b.size ?? 0) - (a.size ?? 0));
  return {
    ...node,
    kind: "dir",
    children,
    size: children.reduce((sum, c) => sum + (c.size ?? 0), 0),
    files: children.reduce((sum, c) => sum + (c.files ?? 0), 0),
  };
}

let tree: Fake | null = null;

function find(path: string[]): Fake | null {
  let node = tree;
  for (const segment of path) node = node?.children?.find((c) => c.kind !== "small" && c.name === segment) ?? null;
  return node;
}

function build(node: Fake, path: string[], depth: number, threshold: number): DiskNode {
  const children: DiskNode[] = [];
  if (depth > 0 && node.children) {
    let other = { size: 0, files: 0, count: 0 };
    for (const child of node.children) {
      if ((child.size ?? 0) < Math.max(threshold, 1)) {
        other = { size: other.size + (child.size ?? 0), files: other.files + (child.files ?? 0), count: other.count + 1 };
        continue;
      }
      children.push(build(child, child.kind === "small" ? [] : [...path, child.name], depth - 1, threshold));
    }
    if (other.count > 0) children.push({ name: String(other.count), path: [], kind: "other", size: other.size, files: other.files, hasChildren: false, children: [] });
  }
  return {
    name: node.name,
    path,
    kind: node.kind ?? "dir",
    size: node.size ?? 0,
    files: node.files ?? 0,
    hasChildren: (node.kind ?? "dir") === "dir" && (node.children?.length ?? 0) > 0,
    children,
  };
}

function remove(node: Fake, path: string[]): number {
  const [first, ...rest] = path;
  const index = node.children?.findIndex((c) => c.kind !== "small" && c.name === first) ?? -1;
  if (index < 0 || !node.children) return 0;
  const child = node.children[index]!;
  let size: number;
  let files: number;
  if (rest.length === 0) {
    node.children.splice(index, 1);
    size = child.size ?? 0;
    files = child.files ?? 0;
  } else {
    const before = child.files ?? 0;
    size = remove(child, rest);
    files = before - (child.files ?? 0);
  }
  node.size = (node.size ?? 0) - size;
  node.files = (node.files ?? 0) - files;
  return size;
}

export function mockOnDiskProgress(handler: (progress: DiskProgress) => void): () => void {
  listeners.add(handler);
  return () => listeners.delete(handler);
}

const asked = new Set<string>();
const EXPLAINED: Record<string, string> = {
  Library: "macOS 给每个用户的资源库，放应用的设置、缓存和数据。不要整个删除，里面的 Caches 可以清。",
  Developer: "Xcode 的编译数据、模拟器和设备调试符号，可以在 Xcode 里或用清理功能删掉，用到时会重新生成。",
  Caches: "各个应用的缓存，删了会自动重新生成，第一次打开会慢一点。",
  "Photos Library.photoslibrary": "“照片”应用的图库，里面是你的照片和视频，不要删除。",
  target: "Rust 项目的编译结果，删了下次编译会重新生成，只是要等久一点。",
  "Docker.raw": "Docker 虚拟机的磁盘映像，删了会丢失所有容器和镜像。可以在 Docker 设置里调小。",
  Downloads: "下载文件夹。里面是你下载的文件，看过不要的可以删。",
};

const PROTECTED = new Set(["Library", "Downloads", "Movies", "Pictures", "Documents", "Desktop", "Music"]);

export async function mockDiskCall(command: string, args: Record<string, unknown>): Promise<unknown> {
  switch (command) {
    case "disk_locations":
      return [
        { path: "/Users/demo", name: "demo", kind: "home", total: 0, available: 0 },
        { path: "/", name: "Macintosh HD", kind: "disk", total: 994 * GB, available: 182 * GB },
        { path: "/Volumes/Backup", name: "Backup", kind: "disk", total: 2000 * GB, available: 1210 * GB },
      ] satisfies DiskLocation[];
    case "disk_scan": {
      cancelled = false;
      const settled = settle(TREE);
      const total = settled.size ?? 0;
      const files = settled.files ?? 0;
      const steps = 16;
      for (let i = 1; i <= steps; i++) {
        await sleep(110);
        if (cancelled) throw "cancelled";
        const share = i / steps;
        const places = ["Library/Caches", "Library/Developer/Xcode", "Pictures/Photos Library.photoslibrary", "Desktop/ShibaPack/target", "Documents/项目资料"];
        listeners.forEach((l) =>
          l({ files: Math.round(files * share), dirs: Math.round(files * share * 0.08), bytes: Math.round(total * share), current: `${args.root}/${places[i % places.length]}` }),
        );
      }
      tree = settled;
      return { root: String(args.root), size: total, files, denied: 3, deniedExamples: ["/Users/demo/Library/Mail", "/Users/demo/Library/Safari"], elapsedMs: 1840 } satisfies DiskSummary;
    }
    case "disk_cancel":
      cancelled = true;
      return null;
    case "disk_largest": {
      const start = find(args.path as string[]);
      if (!start) throw "not_found";
      const out: LargeFile[] = [];
      const walk = (node: Fake, at: string[]) => {
        for (const child of node.children ?? []) {
          if (child.kind === "file") out.push({ path: [...at, child.name], name: child.name, size: child.size ?? 0, folder: `~/${at.join("/")}`, modifiedMs: Date.now() - (child.size ?? 0) / 4000 });
          else if (child.children) walk(child, [...at, child.name]);
        }
      };
      walk(start, args.path as string[]);
      return out.sort((a, b) => b.size - a.size).slice(0, args.limit as number);
    }
    case "disk_view": {
      const node = find(args.path as string[]);
      if (!node) throw "not_found";
      return build(node, args.path as string[], args.depth as number, (node.size ?? 0) * (args.minRatio as number));
    }
    case "disk_reveal":
      return null;
    case "disk_full_path":
      return `/Users/demo/${(args.path as string[]).join("/")}`;
    case "disk_trash": {
      const path = args.path as string[];
      if (path.length === 1 && PROTECTED.has(path[0]!)) throw "disk_protected:personal";
      if (!tree || !find(path)) throw "not_found";
      await sleep(300);
      return remove(tree, path);
    }
    case "ai_explain_folders": {
      const paths = args.paths as string[][];
      if (args.mode === "ask") await sleep(1200);
      const out: Record<string, string> = {};
      for (const path of paths) {
        const key = path.join("/");
        const known = EXPLAINED[path[path.length - 1] ?? ""];
        // 预览里先问过（ask）才算缓存过
        if (args.mode === "ask") asked.add(key);
        if (asked.has(key)) out[key] = known ?? "预览里的示例解释：这个文件夹的作用要连上 AI 服务才看得到。";
      }
      return out;
    }
    default:
      throw `mock: unknown ${command}`;
  }
}

