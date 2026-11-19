import { compareVersions } from "./version";
import type {
  AppStatus,
  Category,
  InstalledProgram,
  LibraryData,
  PackageInfo,
  Settings,
  Software,
  SoftwareGroup,
  SoftwareSummary,
  Tag,
} from "../types";

export const mockStatus: AppStatus = {
  configured: true,
  platform: "browser-preview",
  app_version: "0.1.0-preview",
  data_dir: "~/Documents/ShibaPack",
  repo_root: "~/Documents/ShibaPack",
};

export const mockSettings: Settings = {
  package_root: "~/Documents/ShibaPack",
  storage_mode: "managed",
  installer_directory: "./Installers",
  theme: "system",
  accent: "#a25bff",
  language: "zh-CN",
  auto_scan: false,
  copy_on_import: true,
};

export const mockTags: Tag[] = [
  { id: "t1", name: "免费", color: "#2fb37b" },
  { id: "t2", name: "开源", color: "#5b8cff" },
  { id: "t3", name: "常用", color: "#a25bff" },
];

export const mockCategories: Category[] = [
  { id: "c1", name: "常用工具", color: "#a25bff", icon: "wrench", parent_id: null, sort_order: 0, package_count: 2 },
  { id: "c2", name: "开发环境", color: "#5b8cff", icon: "code", parent_id: null, sort_order: 1, package_count: 0 },
  { id: "c2a", name: "编辑器", color: "#5b8cff", icon: "file-code", parent_id: "c2", sort_order: 2, package_count: 2 },
  { id: "c2b", name: "运行时", color: "#5b8cff", icon: "terminal", parent_id: "c2", sort_order: 3, package_count: 1 },
  { id: "c3", name: "设计素材", color: "#e56a8f", icon: "palette", parent_id: null, sort_order: 4, package_count: 1 },
  { id: "c4", name: "系统增强", color: "#2fb37b", icon: "settings", parent_id: null, sort_order: 5, package_count: 0 },
];

const packages: PackageInfo[] = [
  {
    id: "p1", file_name: "GoogleChrome_131.0.6778.86.dmg", full_path: "~/Documents/ShibaPack/mac/GoogleChrome_131.0.6778.86.dmg",
    software_name: "Google Chrome", version: "131.0.6778.86", platform: "macos", file_size: 184_549_376,
    added_time: "2026-11-18T09:21:00Z", modified_time: "2026-11-17T14:02:00Z", extension: ".dmg",
    category_id: "c1", product_name: "Google Chrome", publisher: "Google LLC", description: "Web browser",
  },
  {
    id: "p2", file_name: "GoogleChrome_130.0.6723.91.dmg", full_path: "~/Documents/ShibaPack/mac/GoogleChrome_130.0.6723.91.dmg",
    software_name: "Google Chrome", version: "130.0.6723.91", platform: "macos", file_size: 181_403_648,
    added_time: "2026-10-22T08:10:00Z", modified_time: "2026-10-21T18:30:00Z", extension: ".dmg",
    category_id: "c1", product_name: "Google Chrome", publisher: "Google LLC", description: "Web browser",
  },
  {
    id: "p3", file_name: "VSCode-darwin-universal-1.96.4.dmg", full_path: "~/Documents/ShibaPack/mac/VSCode-darwin-universal-1.96.4.dmg",
    software_name: "Visual Studio Code", version: "1.96.4", platform: "macos", file_size: 112_721_920,
    added_time: "2026-11-19T02:15:00Z", modified_time: "2026-11-18T22:40:00Z", extension: ".dmg",
    category_id: "c2a", product_name: "Visual Studio Code", publisher: "Microsoft", description: "Code editor",
  },
  {
    id: "p4", file_name: "VSCodeSetup-x64-1.95.3.exe", full_path: "~/Documents/ShibaPack/win/VSCodeSetup-x64-1.95.3.exe",
    software_name: "Visual Studio Code", version: "1.95.3", platform: "windows", file_size: 98_255_872,
    added_time: "2026-11-05T11:30:00Z", modified_time: "2026-11-04T16:12:00Z", extension: ".exe",
    category_id: "c2a", product_name: "Visual Studio Code", publisher: "Microsoft", description: "Code editor",
  },
  {
    id: "p5", file_name: "node-v22.14.0-x64.msi", full_path: "~/Documents/ShibaPack/win/node-v22.14.0-x64.msi",
    software_name: "Node.js", version: "22.14.0", platform: "windows", file_size: 31_457_280,
    added_time: "2026-11-12T07:45:00Z", modified_time: "2026-11-11T20:18:00Z", extension: ".msi",
    category_id: "c2b", product_name: "Node.js", publisher: "OpenJS Foundation", description: "JavaScript runtime",
  },
  {
    id: "p6", file_name: "Figma-124.7.2.dmg", full_path: "~/Documents/ShibaPack/mac/Figma-124.7.2.dmg",
    software_name: "Figma", version: "124.7.2", platform: "macos", file_size: 158_335_000,
    added_time: "2026-11-16T05:00:00Z", modified_time: "2026-11-15T09:24:00Z", extension: ".dmg",
    category_id: "c3", product_name: "Figma", publisher: "Figma, Inc.", description: "Design tool",
  },
  {
    id: "p7", file_name: "obsidian-1.8.12.AppImage", full_path: "~/Documents/ShibaPack/linux/obsidian-1.8.12.AppImage",
    software_name: "Obsidian", version: "1.8.12", platform: "linux", file_size: 92_137_472,
    added_time: "2026-11-14T13:08:00Z", modified_time: "2026-11-13T17:50:00Z", extension: ".appimage",
    category_id: "c1", product_name: "Obsidian", publisher: "Dynalist Inc.", description: "Knowledge base",
  },
];

const groups: SoftwareGroup[] = Array.from(
  packages.reduce((map, pkg) => {
    const list = map.get(pkg.software_name) ?? [];
    list.push(pkg);
    map.set(pkg.software_name, list);
    return map;
  }, new Map<string, PackageInfo[]>()),
)
  .map(([name, items]) => ({
    name,
    packages: items.sort(
      (a, b) => compareVersions(b.version, a.version) || b.added_time.localeCompare(a.added_time),
    ),
    latest_version: items[0]?.version ?? "",
    platforms: Array.from(new Set(items.map((item) => item.platform))),
  }))
  .sort((a, b) => a.name.localeCompare(b.name));

const platformCounts = packages.reduce<Record<string, number>>((acc, pkg) => {
  acc[pkg.platform] = (acc[pkg.platform] ?? 0) + 1;
  return acc;
}, {});

// 由扁平 packages 构造新三级结构，供预览模式展示
const mockSoftware: Software[] = Array.from(
  packages.reduce((map, pkg) => {
    const list = map.get(pkg.software_name) ?? [];
    list.push(pkg);
    map.set(pkg.software_name, list);
    return map;
  }, new Map<string, PackageInfo[]>()),
)
  .map(([name, items]) => {
    items.sort((a, b) => compareVersions(b.version, a.version));
    return {
      id: `s-${name}`,
      name,
      icon: "",
      category_id: items[0]?.category_id ?? null,
      tags: name === "Google Chrome" || name === "Obsidian" ? ["t1", "t3"] : [],
      description: items[0]?.description ?? "",
      created_at: items[items.length - 1]?.added_time ?? "",
      updated_at: items[0]?.added_time ?? "",
      versions: items.map((pkg, index) => ({
        id: `v-${pkg.id}`,
        version: pkg.version,
        release_date: null,
        is_default: index === 0,
        created_at: pkg.added_time,
        files: [
          {
            id: pkg.id,
            name: pkg.file_name,
            path: pkg.full_path,
            format: pkg.extension.replace(".", ""),
            size: pkg.file_size,
            sha256: "",
            role: "installer",
            platform: pkg.platform,
            added_at: pkg.added_time,
          },
        ],
      })),
    };
  })
  .sort((a, b) => a.name.localeCompare(b.name));

export const mockLibrary: LibraryData = {
  packages,
  categories: mockCategories,
  groups,
  stats: {
    software_count: groups.length,
    package_count: packages.length,
    total_size: packages.reduce((sum, pkg) => sum + pkg.file_size, 0),
    platform_counts: platformCounts,
  },
  software: mockSoftware,
  tags: mockTags,
};

const mockSummaryText: Record<string, string> = {
  "google chrome": "Google Chrome 是由 Google 开发的网页浏览器，基于 Chromium 开源项目构建，以速度快、安全性高和 extensions 生态丰富著称。",
  "visual studio code": "Visual Studio Code 是微软推出的轻量级代码编辑器，支持语法高亮、智能补全、调试和 Git 集成，可通过扩展支持几乎所有主流编程语言。",
  "node.js": "Node.js 是基于 Chrome V8 引擎的 JavaScript 运行时，让 JavaScript 可以在服务器端运行，广泛用于构建 Web 服务、命令行工具和前端工程化链路。",
  figma: "Figma 是一款基于浏览器的协作式界面设计工具，支持多人实时编辑、组件复用和原型交互，是产品和 UI 设计团队的主流选择。",
  obsidian: "Obsidian 是一款基于本地 Markdown 文件的知识管理工具，通过双向链接和图谱视图帮助用户建立个人知识网络。",
};

export function mockSummary(name: string): SoftwareSummary {
  const key = name.trim().toLowerCase();
  return {
    name: key,
    summary: mockSummaryText[key] ?? `预览模式暂未收录「${name}」的摘要。运行 Tauri 应用后会自动从 Wikipedia 与 GitHub 获取。`,
    source: "预览示例",
    url: "",
    updated_at: new Date().toISOString(),
  };
}

export const mockInstalled: InstalledProgram[] = [
  {
    id: "i1", name: "Google Chrome", version: "131.0.6778.86", publisher: "Google LLC",
    install_date: "2026-11-18", location: "/Applications/Google Chrome.app", source: "Applications", uninstall_string: "",
    icon: "", size_bytes: 512_000_000,
  },
  {
    id: "i2", name: "Visual Studio Code", version: "1.96.4", publisher: "Microsoft",
    install_date: "2026-11-19", location: "/Applications/Visual Studio Code.app", source: "Applications", uninstall_string: "",
    icon: "", size_bytes: 380_000_000,
  },
  {
    id: "i3", name: "Figma", version: "124.7.2", publisher: "Figma, Inc.",
    install_date: "2026-11-16", location: "/Applications/Figma.app", source: "Applications", uninstall_string: "",
    icon: "", size_bytes: 245_000_000,
  },
  {
    id: "i4", name: "Obsidian", version: "1.8.12", publisher: "Dynalist Inc.",
    install_date: "2026-11-14", location: "/Applications/Obsidian.app", source: "Applications", uninstall_string: "",
    icon: "", size_bytes: 168_000_000,
  },
];
