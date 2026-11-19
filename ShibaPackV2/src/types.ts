export type Platform = "windows" | "macos" | "linux" | "crossplatform" | "unknown";
export type StorageMode = "managed" | "indexed";

export type ThemeMode = "system" | "dark" | "light";

// ===== 新版三级结构 =====

export interface SoftwareFile {
  id: string;
  name: string;
  /** 相对仓库根目录的路径（托管），或绝对路径（索引） */
  path: string;
  format: string;
  size: number;
  sha256: string;
  role: "installer" | "resource" | string;
  platform: Platform;
  added_at: string;
}

export interface SoftwareVersion {
  id: string;
  version: string;
  release_date: string | null;
  is_default: boolean;
  created_at: string;
  files: SoftwareFile[];
}

export interface Software {
  id: string;
  name: string;
  icon: string;
  category_id: string | null;
  tags: string[];
  description: string;
  created_at: string;
  updated_at: string;
  versions: SoftwareVersion[];
}

export interface Tag {
  id: string;
  name: string;
  color: string;
}

export interface StorageInfo {
  repo_root: string;
  installer_directory: string;
  storage_mode: StorageMode | string;
  software_count: number;
  version_count: number;
  file_count: number;
  total_size: number;
  managed_size: number;
  indexed_size: number;
  missing_files: number;
  backup_count: number;
}

export interface BackupInfo {
  name: string;
  path: string;
  size: number;
  created_at: string;
}

// ===== 兼容结构 =====

export interface PackageInfo {
  id: string;
  file_name: string;
  full_path: string;
  software_name: string;
  version: string;
  platform: Platform;
  file_size: number;
  added_time: string;
  modified_time: string;
  extension: string;
  category_id: string | null;
  product_name?: string | null;
  publisher?: string | null;
  description?: string | null;
}

export interface SoftwareGroup {
  name: string;
  packages: PackageInfo[];
  latest_version: string;
  platforms: Platform[];
}

export interface Category {
  id: string;
  name: string;
  color: string;
  icon: string;
  parent_id: string | null;
  sort_order: number;
  package_count: number;
}

export interface LibraryStats {
  software_count: number;
  package_count: number;
  total_size: number;
  platform_counts: Record<string, number>;
}

export interface LibraryData {
  packages: PackageInfo[];
  categories: Category[];
  groups: SoftwareGroup[];
  stats: LibraryStats;
  software: Software[];
  tags: Tag[];
}

export interface Settings {
  package_root: string;
  storage_mode: StorageMode;
  installer_directory: string;
  theme: ThemeMode;
  accent: string;
  language: "zh-CN" | "en-US";
  auto_scan: boolean;
  copy_on_import: boolean;
}

export interface InstalledProgram {
  id: string;
  name: string;
  version: string;
  publisher: string;
  install_date: string | null;
  location: string;
  source: string;
  uninstall_string: string;
  icon: string;
  size_bytes: number | null;
}

export interface AppStatus {
  configured: boolean;
  platform: string;
  app_version: string;
  data_dir: string;
  repo_root: string;
}

export interface ScanProgress {
  current: number;
  total: number;
  path: string;
}

export interface SoftwareSummary {
  name: string;
  summary: string;
  source: string;
  url: string;
  updated_at: string;
}

export interface ScanResult {
  added: number;
  updated: number;
  skipped: number;
  total: number;
}
