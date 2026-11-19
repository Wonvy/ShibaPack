import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  mockCategories,
  mockInstalled,
  mockLibrary,
  mockSettings,
  mockSummary,
  mockStatus,
} from "./mock";
import type {
  AppStatus,
  BackupInfo,
  Category,
  InstalledProgram,
  LibraryData,
  ScanResult,
  Settings,
  Software,
  SoftwareFile,
  SoftwareVersion,
  SoftwareSummary,
  StorageInfo,
  Tag,
} from "../types";

const isTauriApp =
  typeof window !== "undefined" &&
  ("__TAURI_INTERNALS__" in window || "__TAURI__" in window);

const ok = <T>(value: T): Promise<T> => Promise.resolve(value);

export const isDesktop = isTauriApp;

export const api = {
  status: (): Promise<AppStatus> =>
    isTauriApp ? invoke<AppStatus>("get_status") : ok(mockStatus),

  library: (): Promise<LibraryData> =>
    isTauriApp ? invoke<LibraryData>("get_library") : ok(mockLibrary),

  settings: (): Promise<Settings> =>
    isTauriApp ? invoke<Settings>("get_settings") : ok(mockSettings),

  saveSettings: (settings: Settings): Promise<Settings> =>
    isTauriApp ? invoke<Settings>("update_settings", { settings }) : ok(settings),

  scan: (path?: string | null): Promise<ScanResult> =>
    isTauriApp
      ? invoke<ScanResult>("scan_directory", { path: path ?? null })
      : ok({ added: 0, updated: 0, skipped: 0, total: 0 }),

  addPackages: (paths: string[], platformHint?: string | null): Promise<ScanResult> =>
    isTauriApp
      ? invoke<ScanResult>("add_packages", { paths, platformHint: platformHint ?? null })
      : ok({ added: 0, updated: 0, skipped: 0, total: paths.length }),

  deletePackage: (id: string, deleteFile: boolean): Promise<void> => {
    if (!isTauriApp) {
      const index = mockLibrary.packages.findIndex((item) => item.id === id);
      if (index >= 0) mockLibrary.packages.splice(index, 1);
      return ok(undefined);
    }
    return invoke<void>("delete_package", { id, deleteFile });
  },

  runPackage: (id: string): Promise<void> =>
    isTauriApp ? invoke<void>("run_package", { id }) : ok(undefined),

  revealPackage: (id: string): Promise<void> =>
    isTauriApp ? invoke<void>("reveal_package", { id }) : ok(undefined),

  revealDataDir: (): Promise<void> =>
    isTauriApp ? invoke<void>("reveal_data_dir") : ok(undefined),

  revealPath: (path: string): Promise<void> =>
    isTauriApp ? invoke<void>("reveal_path", { path }) : ok(undefined),

  fetchSummary: (name: string, force = false): Promise<SoftwareSummary> =>
    isTauriApp
      ? invoke<SoftwareSummary>("fetch_summary", { name, force })
      : ok(mockSummary(name)),

  installed: (): Promise<InstalledProgram[]> =>
    isTauriApp ? invoke<InstalledProgram[]>("get_installed_apps") : ok(mockInstalled),

  uninstallApp: (program: InstalledProgram): Promise<void> =>
    isTauriApp ? invoke<void>("uninstall_app", { program }) : ok(undefined),

  reorderCategories: (ids: string[]): Promise<void> => {
    if (!isTauriApp) {
      const order = new Map(ids.map((id, index) => [id, index]));
      mockCategories.sort((a, b) => (order.get(a.id) ?? 0) - (order.get(b.id) ?? 0));
      mockLibrary.categories = [...mockCategories];
      return ok(undefined);
    }
    return invoke<void>("reorder_categories", { ids });
  },

  createCategory: (
    name: string,
    color: string,
    icon: string,
    parentId: string | null = null,
  ): Promise<Category> => {
    if (!isTauriApp) {
      const category: Category = {
        id: `mock-${Date.now()}`,
        name,
        color,
        icon,
        parent_id: parentId,
        sort_order: mockCategories.length,
        package_count: 0,
      };
      mockCategories.push(category);
      mockLibrary.categories = [...mockCategories];
      return ok(category);
    }
    return invoke<Category>("create_category", { name, color, icon, parentId });
  },

  updateCategory: (category: Category): Promise<void> => {
    if (!isTauriApp) {
      const index = mockCategories.findIndex((item) => item.id === category.id);
      if (index >= 0) mockCategories[index] = category;
      mockLibrary.categories = [...mockCategories];
      return ok(undefined);
    }
    return invoke<void>("update_category", { category });
  },

  deleteCategory: (id: string): Promise<void> => {
    if (!isTauriApp) {
      const index = mockCategories.findIndex((item) => item.id === id);
      if (index >= 0) mockCategories.splice(index, 1);
      mockLibrary.categories = [...mockCategories];
      return ok(undefined);
    }
    return invoke<void>("delete_category", { id });
  },

  assignCategory: (packageId: string, categoryId: string | null): Promise<void> => {
    if (!isTauriApp) {
      const pkg = mockLibrary.packages.find((item) => item.id === packageId);
      if (pkg) pkg.category_id = categoryId;
      return ok(undefined);
    }
    return invoke<void>("assign_category", { packageId, categoryId });
  },

  exportBackup: (path: string): Promise<void> =>
    isTauriApp ? invoke<void>("export_backup", { path }) : ok(undefined),

  importBackup: (path: string): Promise<ScanResult> =>
    isTauriApp
      ? invoke<ScanResult>("import_backup", { path })
      : ok({ added: 0, updated: 0, skipped: 0, total: 0 }),

  chooseDirectory: (): Promise<string | null> =>
    isTauriApp ? open({ directory: true, multiple: false }) : ok(null),

  chooseFiles: (): Promise<string[] | null> =>
    isTauriApp
      ? open({
          multiple: true,
          filters: [
            {
              name: "安装包",
              extensions: [
                "exe", "msi", "msix", "dmg", "pkg", "app", "deb", "rpm",
                "AppImage", "snap", "zip", "7z", "rar", "tar", "gz", "xz", "iso",
              ],
            },
          ],
        })
      : ok(null),

  chooseSaveFile: (defaultPath?: string): Promise<string | null> =>
    isTauriApp ? save({ defaultPath }) : ok(null),

  writeTextFile: (path: string, content: string): Promise<void> =>
    isTauriApp ? invoke<void>("write_text_file", { path, content }) : ok(undefined),

  readTextFile: (path: string): Promise<string> =>
    isTauriApp ? invoke<string>("read_text_file", { path }) : ok(""),

  // ===== 标签 =====
  getTags: (): Promise<Tag[]> =>
    isTauriApp ? invoke<Tag[]>("get_tags") : ok(mockLibrary.tags),

  createTag: (name: string, color: string): Promise<Tag> =>
    isTauriApp ? invoke<Tag>("create_tag", { name, color }) : ok({ id: `mock-${Date.now()}`, name, color }),

  updateTag: (tag: Tag): Promise<void> =>
    isTauriApp ? invoke<void>("update_tag", { tag }) : ok(undefined),

  deleteTag: (id: string): Promise<void> =>
    isTauriApp ? invoke<void>("delete_tag", { id }) : ok(undefined),

  assignTags: (softwareId: string, tags: string[]): Promise<void> =>
    isTauriApp ? invoke<void>("assign_tags", { softwareId, tags }) : ok(undefined),

  // ===== 软件 / 版本 / 文件 =====
  createSoftware: (name: string, categoryId: string | null = null): Promise<Software> =>
    isTauriApp
      ? invoke<Software>("create_software", { name, categoryId })
      : ok({ id: `mock-${Date.now()}`, name, icon: "", category_id: categoryId, tags: [], description: "", created_at: "", updated_at: "", versions: [] }),

  updateSoftware: (software: Software): Promise<void> =>
    isTauriApp ? invoke<void>("update_software", { software }) : ok(undefined),

  deleteSoftware: (id: string): Promise<void> =>
    isTauriApp ? invoke<void>("delete_software", { id }) : ok(undefined),

  getSoftware: (id: string): Promise<Software | null> =>
    isTauriApp
      ? invoke<Software | null>("get_software", { id })
      : ok(mockLibrary.software.find((s) => s.id === id) ?? null),

  getFiles: (softwareId: string): Promise<SoftwareFile[]> =>
    isTauriApp
      ? invoke<SoftwareFile[]>("get_files", { softwareId })
      : ok(mockLibrary.software.find((s) => s.id === softwareId)?.versions.flatMap((v) => v.files) ?? []),

  addVersion: (softwareId: string, version: SoftwareVersion): Promise<void> =>
    isTauriApp ? invoke<void>("add_version", { softwareId, version }) : ok(undefined),

  deleteVersion: (softwareId: string, versionId: string): Promise<void> =>
    isTauriApp ? invoke<void>("delete_version", { softwareId, versionId }) : ok(undefined),

  setDefaultVersion: (softwareId: string, versionId: string): Promise<void> =>
    isTauriApp ? invoke<void>("set_default_version", { softwareId, versionId }) : ok(undefined),

  deleteFile: (softwareId: string, versionId: string, fileId: string, deleteFromDisk = false): Promise<void> =>
    isTauriApp
      ? invoke<void>("delete_file", { softwareId, versionId, fileId, deleteFromDisk })
      : ok(undefined),

  runFile: (softwareId: string, fileId: string): Promise<void> =>
    isTauriApp ? invoke<void>("run_file", { softwareId, fileId }) : ok(undefined),

  revealFile: (softwareId: string, fileId: string): Promise<void> =>
    isTauriApp ? invoke<void>("reveal_file", { softwareId, fileId }) : ok(undefined),

  checkFile: (path: string): Promise<boolean> =>
    isTauriApp ? invoke<boolean>("check_file", { path }) : ok(true),

  // ===== 存储与备份 =====
  getStorageInfo: (): Promise<StorageInfo> =>
    isTauriApp
      ? invoke<StorageInfo>("get_storage_info")
      : ok({
          repo_root: "~/Documents/ShibaPack",
          installer_directory: "./Installers",
          storage_mode: "managed",
          software_count: mockLibrary.software.length,
          version_count: mockLibrary.software.reduce((n, s) => n + s.versions.length, 0),
          file_count: mockLibrary.packages.length,
          total_size: mockLibrary.stats.total_size,
          managed_size: mockLibrary.stats.total_size,
          indexed_size: 0,
          missing_files: 0,
          backup_count: 3,
        }),

  listBackups: (): Promise<BackupInfo[]> => isTauriApp ? invoke<BackupInfo[]>("list_backups") : ok([]),
  restoreBackup: (path: string): Promise<void> =>
    isTauriApp ? invoke<void>("restore_backup", { path }) : ok(undefined),
  verifyData: (): Promise<boolean> => isTauriApp ? invoke<boolean>("verify_data") : ok(true),
};
