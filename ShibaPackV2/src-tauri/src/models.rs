use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Windows,
    Macos,
    Linux,
    Crossplatform,
    Unknown,
}

impl Default for Platform {
    fn default() -> Self {
        Platform::Unknown
    }
}

impl Platform {
    pub fn as_str(&self) -> &'static str {
        match self {
            Platform::Windows => "windows",
            Platform::Macos => "macos",
            Platform::Linux => "linux",
            Platform::Crossplatform => "crossplatform",
            Platform::Unknown => "unknown",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "windows" | "win" => Platform::Windows,
            "macos" | "mac" | "darwin" => Platform::Macos,
            "linux" => Platform::Linux,
            "crossplatform" | "all" => Platform::Crossplatform,
            _ => Platform::Unknown,
        }
    }

    pub fn folder(&self) -> &'static str {
        match self {
            Platform::Windows => "win",
            Platform::Macos => "mac",
            Platform::Linux => "linux",
            Platform::Crossplatform | Platform::Unknown => "all",
        }
    }
}

/// 存储模式：managed 把文件复制进仓库，indexed 只记录外部文件位置
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StorageMode {
    #[default]
    Managed,
    Indexed,
}

impl StorageMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            StorageMode::Managed => "managed",
            StorageMode::Indexed => "indexed",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "indexed" | "index" | "external" => StorageMode::Indexed,
            _ => StorageMode::Managed,
        }
    }
}

/// 版本内的单个安装文件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftwareFile {
    pub id: String,
    pub name: String,
    /// 相对仓库根目录的路径（优先）；索引模式下可为绝对路径
    pub path: String,
    /// 扩展名，如 exe / dmg / zip
    pub format: String,
    /// 字节
    pub size: i64,
    #[serde(default)]
    pub sha256: String,
    /// installer / resource
    #[serde(default = "default_file_role")]
    pub role: String,
    #[serde(default)]
    pub platform: Platform,
    #[serde(default)]
    pub added_at: String,
}

fn default_file_role() -> String {
    "installer".to_string()
}

/// 软件的单个版本
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftwareVersion {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub is_default: bool,
    pub created_at: String,
    #[serde(default)]
    pub files: Vec<SoftwareFile>,
}

/// 仓库中的一个软件（含全部版本）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Software {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub category_id: Option<String>,
    /// 标签 id 列表
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub description: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub versions: Vec<SoftwareVersion>,
}

/// 标签
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: String,
    pub name: String,
    #[serde(default = "default_tag_color")]
    pub color: String,
}

fn default_tag_color() -> String {
    "#a25bff".to_string()
}

/// 仓库统计信息
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StorageInfo {
    pub repo_root: String,
    pub installer_directory: String,
    pub storage_mode: String,
    pub software_count: i64,
    pub version_count: i64,
    pub file_count: i64,
    pub total_size: i64,
    pub managed_size: i64,
    pub indexed_size: i64,
    pub missing_files: i64,
    pub backup_count: i64,
}

/// 备份文件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    pub name: String,
    pub path: String,
    pub size: i64,
    pub created_at: String,
}

// ===== 兼容视图：供现有界面与系统已安装程序功能使用 =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub id: String,
    pub file_name: String,
    pub full_path: String,
    pub software_name: String,
    pub version: String,
    pub platform: Platform,
    pub file_size: i64,
    pub added_time: String,
    pub modified_time: String,
    pub extension: String,
    pub category_id: Option<String>,
    pub product_name: Option<String>,
    pub publisher: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftwareGroup {
    pub name: String,
    pub packages: Vec<PackageInfo>,
    pub latest_version: String,
    pub platforms: Vec<Platform>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub id: String,
    pub name: String,
    pub color: String,
    pub icon: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    pub sort_order: i32,
    pub package_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryStats {
    pub software_count: i64,
    pub package_count: i64,
    pub total_size: i64,
    pub platform_counts: std::collections::HashMap<String, i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryData {
    pub packages: Vec<PackageInfo>,
    pub categories: Vec<Category>,
    pub groups: Vec<SoftwareGroup>,
    pub stats: LibraryStats,
    /// 新版三级结构数据
    #[serde(default)]
    pub software: Vec<Software>,
    #[serde(default)]
    pub tags: Vec<Tag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub package_root: String,
    /// managed / indexed
    #[serde(default = "default_storage_mode")]
    pub storage_mode: String,
    /// 相对仓库根目录的安装包目录
    #[serde(default = "default_installer_dir")]
    pub installer_directory: String,
    pub theme: String,
    pub accent: String,
    pub language: String,
    pub auto_scan: bool,
    pub copy_on_import: bool,
}

fn default_storage_mode() -> String {
    "managed".to_string()
}

fn default_installer_dir() -> String {
    "./Installers".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            package_root: String::new(),
            storage_mode: "managed".into(),
            installer_directory: "./Installers".into(),
            theme: "system".into(),
            accent: "#a25bff".into(),
            language: "zh-CN".into(),
            auto_scan: false,
            copy_on_import: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledProgram {
    pub id: String,
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub install_date: Option<String>,
    pub location: String,
    pub source: String,
    #[serde(default)]
    pub uninstall_string: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub size_bytes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppStatus {
    pub configured: bool,
    pub platform: &'static str,
    pub app_version: String,
    pub data_dir: String,
    /// 仓库根目录（软件.json 所在目录）
    #[serde(default)]
    pub repo_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanProgress {
    pub current: usize,
    pub total: usize,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScanResult {
    pub added: usize,
    pub updated: usize,
    pub skipped: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftwareSummary {
    pub name: String,
    pub summary: String,
    pub source: String,
    pub url: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupData {
    pub version: u32,
    pub exported_at: String,
    pub settings: Settings,
    pub categories: Vec<Category>,
    pub tags: Vec<Tag>,
    pub software: Vec<Software>,
    /// 兼容旧版备份
    #[serde(default)]
    pub packages: Vec<PackageInfo>,
}
