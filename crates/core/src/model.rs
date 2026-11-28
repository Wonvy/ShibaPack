//! `software.json` 的数据结构。字段名在文件里用 camelCase。

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};

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
    pub software: Vec<Software>,
    /// 联网取回的软件摘要，按软件名缓存，离线也能显示。
    #[serde(default)]
    pub summaries: BTreeMap<String, Summary>,
    /// 本机扫描到的图标，按软件显示名缓存，可以随索引一起迁移。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub installed_icons: BTreeMap<String, Arc<str>>,
    #[serde(default)]
    pub settings: Settings,
    /// 收藏的路径（软件的配置文件夹这类），可以备份到仓库里。见 `paths`。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<SavedPath>,
}

/// 收藏的一个文件或文件夹。`path` 是可移植的写法：家目录写成 `~`，Windows 上常见的位置写成
/// `%APPDATA%` 这样的环境变量，换一台电脑、换个用户名也能对上。用的时候再展开。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedPath {
    pub id: String,
    pub path: String,
    /// 备注，比如“VS Code 的设置和快捷键”。
    #[serde(default)]
    pub note: String,
    /// 收藏时是不是文件夹。路径暂时不在时靠它显示图标。
    #[serde(default)]
    pub is_dir: bool,
    pub added_at: String,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            format: FORMAT,
            categories: Vec::new(),
            software: Vec::new(),
            summaries: BTreeMap::new(),
            installed_icons: BTreeMap::new(),
            settings: Settings::default(),
            paths: Vec::new(),
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
    /// 收藏。旧版索引里的标签（tags、tagIds）读入时忽略，下次保存时不再写出。
    #[serde(default)]
    pub starred: bool,
    /// 图标，相对仓库根目录的路径。
    #[serde(default)]
    pub icon: Option<String>,
    /// 图标内容，供离线显示和跨系统迁移使用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_data: Option<Arc<str>>,
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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub text: String,
    pub source: String,
    pub fetched_at: String,
    /// 原文的语言：zh、en、ja……旧索引里没有，按文字猜（见 `summary::summary_language`）。
    #[serde(default)]
    pub lang: String,
    /// 翻译成界面语言的译文：界面语言（如 zh-CN）→ 译文。
    #[serde(default)]
    pub translations: BTreeMap<String, String>,
}

/// 跟着仓库走的设置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// system / dark / light
    #[serde(default = "legacy_default_theme")]
    pub theme: String,
    /// 纸张主题以浅色保存，旧版打开同一个仓库时也能正常显示。
    #[serde(default)]
    pub paper_theme: bool,
    #[serde(default = "legacy_default_accent")]
    pub accent: String,
    #[serde(default = "default_language")]
    pub language: String,
    /// 导入文件时默认用哪种存储模式。
    #[serde(default)]
    pub default_storage: Storage,
    /// 存储模式是“托管”时，导入后删掉原文件（移动进仓库），而不是复制一份。
    #[serde(default)]
    pub move_on_import: bool,
    /// 导入后自动给尚未分类的软件归类。
    #[serde(default)]
    pub auto_classify_on_import: bool,
    /// 是否联网取软件简介（会把软件名发给维基百科和 GitHub）。
    #[serde(default = "default_true")]
    pub fetch_summaries: bool,
    /// 官网没有合适的图标时，从 theSVG 查找品牌图标。
    #[serde(default)]
    pub the_svg_icons: bool,
    /// 简介不是界面语言时，是否联网翻译成界面语言（会把简介原文发给翻译服务）。
    #[serde(default = "default_true")]
    pub translate_summaries: bool,
    /// 是否在仓库根目录生成离线目录 index.html。
    #[serde(default = "default_true")]
    pub offline_catalog: bool,
    /// ShibaPack 发现自己有新版本时，是否自动下载并安装。
    #[serde(default = "default_true")]
    pub auto_update: bool,
    /// AI 模型：用来写软件简介、做智能分类。没设置时地址为空。
    #[serde(default)]
    pub ai: AiSettings,
}

/// 用哪个 AI 服务和模型。跟着仓库走；API Key 不在这里，只存在每台电脑的系统钥匙串里。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettings {
    /// 选的是哪个预设（deepseek、qwen、ollama……），自己填地址的是 `custom`。
    #[serde(default)]
    pub preset: String,
    /// OpenAI 兼容接口的地址，到 `/v1` 这一级，例如 `https://api.deepseek.com`。
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
}

fn default_true() -> bool {
    true
}

// 已有索引缺少外观字段时，沿用原来的默认值。
fn legacy_default_theme() -> String {
    "system".into()
}
fn legacy_default_accent() -> String {
    "blue".into()
}
fn default_language() -> String {
    "zh-CN".into()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "light".into(),
            paper_theme: true,
            accent: "sky".into(),
            language: default_language(),
            default_storage: Storage::Managed,
            move_on_import: false,
            auto_classify_on_import: false,
            fetch_summaries: true,
            the_svg_icons: false,
            translate_summaries: true,
            offline_catalog: true,
            auto_update: true,
            ai: AiSettings::default(),
        }
    }
}
