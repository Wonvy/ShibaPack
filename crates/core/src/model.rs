//! `software.json` 的数据结构。字段名在文件里用 camelCase。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const FORMAT: u32 = 1;

fn default_format() -> u32 {
    FORMAT
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Windows,
    Macos,
    Linux,
    Android,
    Ios,
    /// 跨平台，或者看不出平台（压缩包、jar、iso）。
    Cross,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Storage {
    /// 文件复制进仓库，`path` 是相对仓库根目录的路径。
    #[default]
    Managed,
    /// 只记录外部文件，`path` 是绝对路径。
    Indexed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    #[serde(default = "default_format")]
    pub format: u32,
    #[serde(default)]
    pub categories: Vec<Category>,
    #[serde(default)]
    pub tags: Vec<Tag>,
    #[serde(default)]
    pub software: Vec<Software>,
    /// 联网取回的软件摘要，按软件名缓存，离线也能显示。
    #[serde(default)]
    pub summaries: BTreeMap<String, Summary>,
    #[serde(default)]
    pub settings: Settings,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            format: FORMAT,
            categories: Vec::new(),
            tags: Vec::new(),
            software: Vec::new(),
            summaries: BTreeMap::new(),
            settings: Settings::default(),
        }
    }
}

/// 分类。顺序就是数组里的顺序；`parent_id` 为空的是顶层分类。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Software {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub publisher: String,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub category_id: Option<String>,
    #[serde(default)]
    pub tag_ids: Vec<String>,
    /// 图标，相对仓库根目录的路径。
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub default_version_id: Option<String>,
    pub added_at: String,
    #[serde(default)]
    pub versions: Vec<Version>,
    /// 关联的在线源。有了它才能检查更新和下载新版本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscription: Option<Subscription>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    /// Homebrew Cask，`reference` 是 cask 的名字，如 `visual-studio-code`。
    Homebrew,
    /// GitHub Releases，`reference` 是 `owner/repo`。
    Github,
    /// winget 社区清单，`reference` 是包标识，如 `Microsoft.VisualStudioCode`。
    Winget,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    pub source: SourceKind,
    pub reference: String,
    /// 上次检查到的最新版本。
    #[serde(default)]
    pub latest: Option<Release>,
    #[serde(default)]
    pub checked_at: String,
}

/// 在线源上的一个版本。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub version: String,
    #[serde(default)]
    pub published_at: String,
    /// 这个版本的说明页。
    #[serde(default)]
    pub page: String,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

/// 一个可以下载的安装包。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub platform: Platform,
    #[serde(default)]
    pub arch: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub id: String,
    /// 版本号。识别不出时为空字符串。
    pub version: String,
    #[serde(default)]
    pub note: String,
    pub added_at: String,
    #[serde(default)]
    pub files: Vec<PackageFile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageFile {
    pub id: String,
    /// 文件名。
    pub name: String,
    pub platform: Platform,
    /// 架构：x64、x86、arm64、universal，识别不出时为空。
    #[serde(default)]
    pub arch: String,
    pub size: u64,
    pub sha256: String,
    pub storage: Storage,
    pub path: String,
    pub added_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub text: String,
    pub source: String,
    pub fetched_at: String,
}

/// 跟着仓库走的设置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// system / dark / light
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_accent")]
    pub accent: String,
    #[serde(default = "default_language")]
    pub language: String,
    /// 导入文件时默认用哪种存储模式。
    #[serde(default)]
    pub default_storage: Storage,
    /// 是否联网取软件简介（会把软件名发给维基百科和 GitHub）。
    #[serde(default = "default_true")]
    pub fetch_summaries: bool,
    /// 是否在仓库根目录生成离线目录 index.html。
    #[serde(default = "default_true")]
    pub offline_catalog: bool,
    /// ShibaPack 发现自己有新版本时，是否自动下载并安装。
    #[serde(default = "default_true")]
    pub auto_update: bool,
}

fn default_true() -> bool {
    true
}

fn default_theme() -> String {
    "system".into()
}
fn default_accent() -> String {
    "blue".into()
}
fn default_language() -> String {
    "zh-CN".into()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            accent: default_accent(),
            language: default_language(),
            default_storage: Storage::Managed,
            fetch_summaries: true,
            offline_catalog: true,
            auto_update: true,
        }
    }
}
