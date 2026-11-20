//! 卸载后的残留：卸载程序跑完以后，找出这个软件留下的文件夹、文件和注册表项，用户勾选后清理。
//!
//! 参考了两个开源工具的做法（只参考思路，代码是新写的）：
//! - Windows：Bulk Crap Uninstaller（Apache-2.0）。在 Program Files、AppData、ProgramData 里按软件名、
//!   “发布者\软件名”找文件夹；注册表看 Software\发布者\软件名、卸载项、App Paths、开机启动；
//!   别的软件还在用的、名字太泛的不算或者降低可信度；系统和公共文件夹一律不碰。
//! - macOS：Pearcleaner（Apache-2.0）。按 bundle id 精确匹配 ~/Library 下的各个文件夹；按名字匹配要求名字够长、
//!   不是 apple、google 这类泛泛的词；删除是移到废纸篓。
//!
//! 删除都能找回来：文件和文件夹移到回收站（废纸篓）；注册表项删除前先用 `reg export` 导出成 .reg 文件。
//! 删除前还会再按同一套规则检查一遍每一项（`allowed`），不在范围里的不删。

use crate::installed::{folder_size, path_key, InstalledApp, Protected};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// 有多大把握是这个软件留下的。界面上默认勾选高和中。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LeftoverKind {
    Folder,
    File,
    RegistryKey,
    RegistryValue,
}

/// 一项残留。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Leftover {
    pub kind: LeftoverKind,
    /// 文件或文件夹的完整路径；注册表项是 `HKCU\Software\Foo` 这样的键；注册表值是它所在的键。
    pub path: String,
    /// 注册表值的名字，只有 registryValue 有。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub confidence: Confidence,
    /// 为什么认为是残留：installDir、bundleId、publisherName、name、nameContains、shortcut、uninstallKey、appPath、startup。
    pub reason: String,
    /// 文件和文件夹占用的空间。
    #[serde(default)]
    pub size: Option<u64>,
}

/// 卸载前记下的这个软件的样子：卸载以后就查不到了。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub name: String,
    pub publisher: String,
    pub install_dir: Option<PathBuf>,
    pub bundle_id: Option<String>,
    /// Windows 上卸载信息所在的注册表键，如 `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Foo`。
    pub registry_key: Option<String>,
}

impl Target {
    pub fn of(app: &InstalledApp) -> Target {
        Target {
            name: app.name.clone(),
            publisher: app.publisher.clone(),
            install_dir: app.install_dir.clone(),
            bundle_id: app.bundle_id.clone(),
            registry_key: (app.source == "registry").then(|| app.id.clone()),
        }
    }
}

/// 清理的结果。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveReport {
    pub removed: usize,
    pub failed: Vec<Failure>,
    /// 有的项要管理员权限才删得掉（Program Files、HKLM……）。
    pub needs_admin: bool,
    /// 注册表备份放在哪个文件夹。
    pub backup: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    pub item: Leftover,
    /// access_denied、backup_failed、not_allowed、missing，或者系统给的原话。
    pub reason: String,
}

// ── 名字 ─────────────────────────────────────────────────────

/// 太泛的名字：单独拿来匹配会误伤别的软件和系统的东西。
const GENERIC: &[&str] = &[
    "microsoft", "windows", "google", "apple", "adobe", "mozilla", "intel", "nvidia", "amd", "oracle", "common", "commonfiles",
    "shared", "program", "programs", "programfiles", "programdata", "packages", "package", "temp", "tmp", "cache", "caches", "data",
    "appdata", "local", "locallow", "roaming", "users", "user", "default", "public", "system", "system32", "tools", "tool", "update",
    "updater", "updates", "installer", "installers", "install", "setup", "uninstall", "uninstaller", "app", "apps", "application",
    "applications", "software", "plugins", "plugin", "logs", "log", "config", "settings", "documents", "desktop", "downloads",
    "library", "preferences", "support", "containers", "service", "services", "helper", "launcher", "client", "runtime",
];

/// 发布者名字里不算数的后缀。
const COMPANY_WORDS: &[&str] = &[
    "corporation", "corp", "inc", "incorporated", "ltd", "limited", "llc", "gmbh", "co", "company", "ag", "sa", "srl", "sro", "bv",
    "technology", "technologies", "software", "group", "holdings", "international", "有限公司", "股份有限公司", "公司", "科技",
];

/// 只留字母和数字（包括汉字），转小写。
pub(crate) fn norm(text: &str) -> String {
    text.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// 名字够不够特别，能不能拿来匹配：不是泛泛的词；英文至少 4 个字符，汉字至少 2 个。
fn usable(name: &str) -> bool {
    if name.is_empty() || GENERIC.contains(&name) {
        return false;
    }
    if name.is_ascii() { name.len() >= 4 } else { name.chars().count() >= 2 }
}

/// 软件名里的版本号、位数、括号里的说明。
fn noise() -> &'static Regex {
    static NOISE: OnceLock<Regex> = OnceLock::new();
    NOISE.get_or_init(|| {
        Regex::new(r"(?i)\([^)]*\)|\[[^\]]*\]|\bv?\d+(?:\.\d+)+\S*|\b(?:x64|x86|x86_64|amd64|arm64|win64|win32|64-bit|32-bit)\b").unwrap()
    })
}

/// 发布者的核心名字：“Microsoft Corporation” → “microsoft”。
fn publisher_key(publisher: &str) -> Option<String> {
    let cleaned = noise().replace_all(publisher, " ");
    let words: Vec<String> = cleaned
        .split(|c: char| c.is_whitespace() || c == ',' || c == '.')
        .map(norm)
        .filter(|w| !w.is_empty() && !COMPANY_WORDS.contains(&w.as_str()))
        .collect();
    let key = words.concat();
    (key.chars().count() >= 3).then_some(key)
}

/// 软件名拿来匹配的几种写法：完整的；去掉版本号、位数和括号的；再去掉开头发布者名字的（“Microsoft Edge” → “edge”）。
pub(crate) fn name_variants(name: &str, publisher: &str) -> Vec<String> {
    let mut out = vec![norm(name)];
    let cleaned = norm(&noise().replace_all(name, " "));
    out.push(cleaned.clone());
    if let Some(publisher) = publisher_key(publisher) {
        if let Some(rest) = cleaned.strip_prefix(&publisher) {
            out.push(rest.to_string());
        }
    }
    let mut seen = HashSet::new();
    out.into_iter().filter(|v| usable(v) && seen.insert(v.clone())).collect()
}

/// 一个文件夹或文件名和软件名对不对得上：完全一样，或者（名字够长时）包含着它。
fn matches(entry: &str, variants: &[String]) -> Option<(String, bool)> {
    let entry = norm(entry);
    if entry.is_empty() {
        return None;
    }
    if let Some(v) = variants.iter().find(|v| **v == entry) {
        return Some((v.clone(), true));
    }
    variants
        .iter()
        .find(|v| v.chars().count() >= 6 && entry.contains(v.as_str()) && entry.chars().count() <= v.chars().count() + 12)
        .map(|v| (v.clone(), false))
}

// ── 扫描 ─────────────────────────────────────────────────────

/// 到哪些地方找。Windows 和 macOS 各有一套，测试里换成临时文件夹。
#[derive(Debug, Default, Clone)]
pub(crate) struct Roots {
    /// Program Files 这类：名字对得上可信度高。
    pub program: Vec<PathBuf>,
    /// AppData、ProgramData 这类：名字对得上可信度中。
    pub data: Vec<PathBuf>,
    /// 开始菜单：同名的文件夹和快捷方式。
    pub start_menu: Vec<PathBuf>,
    /// 桌面：同名的快捷方式。
    pub desktop: Vec<PathBuf>,
    /// macOS 的 ~/Library。
    pub library: Option<PathBuf>,
}

impl Roots {
    fn from_env() -> Roots {
        let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
        let mut roots = Roots::default();
        if cfg!(windows) {
            for name in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
                if let Some(dir) = var(name) {
                    if !roots.program.contains(&dir) {
                        roots.program.push(dir);
                    }
                }
            }
            roots.data.extend(var("APPDATA"));
            if let Some(local) = var("LOCALAPPDATA") {
                roots.data.push(local.join("Programs"));
                roots.data.push(local.clone());
            }
            if let Some(profile) = var("USERPROFILE") {
                roots.data.push(profile.join("AppData").join("LocalLow"));
                roots.desktop.push(profile.join("Desktop"));
            }
            roots.data.extend(var("ProgramData"));
            let start = |base: PathBuf| base.join("Microsoft").join("Windows").join("Start Menu").join("Programs");
            roots.start_menu.extend(var("APPDATA").map(start));
            roots.start_menu.extend(var("ProgramData").map(start));
            if let Some(public) = var("PUBLIC") {
                roots.desktop.push(public.join("Desktop"));
            }
        } else if cfg!(target_os = "macos") {
            roots.library = var("HOME").map(|home| home.join("Library"));
        }
        roots
    }
}

/// 卸载以后找这个软件的残留。`others` 是这台电脑上其余的已安装软件：它们在用的、和它们同名的都不算。
/// `still_installed` 为真（卸载程序被取消了，软件看起来还在）时，找到的都降成低可信度、默认不勾。
pub fn scan(target: &Target, others: &[InstalledApp], still_installed: bool) -> Vec<Leftover> {
    let mut found = scan_files(target, others, &Roots::from_env(), &Protected::from_env(), cfg!(windows));
    found.extend(registry::scan(target, others));
    if still_installed {
        for item in &mut found {
            item.confidence = Confidence::Low;
        }
    }
    found
}

/// 卸载之前先看一遍会删掉什么。软件还装着，所以不列它自己的程序：安装文件夹、Program Files 和
/// `%LOCALAPPDATA%\Programs` 里对得上名字的文件夹、卸载项、App Paths 都交给卸载程序（界面上是“App”那一行）。
/// 卸载程序跑完以后再用 [`scan`] 找一遍，它没删干净的那时候再列出来。
/// 这样只清数据、不卸载时，也不会动到软件本身。
pub fn scan_before(target: &Target, others: &[InstalledApp]) -> Vec<Leftover> {
    data_only(scan(target, others, false), &program_roots(), cfg!(windows))
}

/// 去掉属于软件本身的项，只留数据、快捷方式、设置和开机启动。`program` 是装软件的文件夹（`path_key` 过的）。
pub(crate) fn data_only(mut found: Vec<Leftover>, program: &[String], fold_case: bool) -> Vec<Leftover> {
    found.retain(|item| {
        if matches!(item.reason.as_str(), "installDir" | "uninstallKey" | "appPath") {
            return false;
        }
        if !matches!(item.kind, LeftoverKind::Folder | LeftoverKind::File) {
            return true;
        }
        let key = path_key(Path::new(&item.path), fold_case);
        !program.iter().any(|root| key.starts_with(&format!("{root}/")))
    });
    found
}

/// 装软件本身的地方：Program Files（三种写法）和 `%LOCALAPPDATA%\Programs`。
fn program_roots() -> Vec<String> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let mut roots: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"].iter().filter_map(std::env::var_os).map(PathBuf::from).collect();
    roots.extend(std::env::var_os("LOCALAPPDATA").map(|local| PathBuf::from(local).join("Programs")));
    roots.iter().filter(|root| !root.as_os_str().is_empty()).map(|root| path_key(root, true)).collect()
}

/// 别的软件和这个软件的关系：它们的名字、它们的文件夹。
struct Neighbours {
    names: HashSet<String>,
    dirs: Vec<String>,
}

impl Neighbours {
    fn of(target: &Target, others: &[InstalledApp], fold_case: bool) -> Neighbours {
        let own = target.registry_key.as_deref();
        let others = others.iter().filter(|app| Some(app.id.as_str()) != own && app.name != target.name);
        let mut names = HashSet::new();
        let mut dirs = Vec::new();
        for app in others {
            names.extend(name_variants(&app.name, &app.publisher));
            if let Some(dir) = &app.install_dir {
                dirs.push(path_key(dir, fold_case));
            }
        }
        Neighbours { names, dirs }
    }

    /// 这个文件夹是别的软件的、在别的软件里面、或者里面装着别的软件。
    fn uses(&self, key: &str) -> bool {
        let inside = |outer: &str, inner: &str| inner.len() > outer.len() && inner.starts_with(outer) && inner[outer.len()..].starts_with('/');
        self.dirs.iter().any(|dir| dir == key || inside(dir, key) || inside(key, dir))
    }
}

pub(crate) fn scan_files(target: &Target, others: &[InstalledApp], roots: &Roots, protected: &Protected, fold_case: bool) -> Vec<Leftover> {
    let variants = name_variants(&target.name, &target.publisher);
    let publisher = publisher_key(&target.publisher);
    let neighbours = Neighbours::of(target, others, fold_case);
    let mut found: Vec<Leftover> = Vec::new();
    let add = |path: &Path, kind: LeftoverKind, confidence: Confidence, reason: &str, found: &mut Vec<Leftover>| {
        let key = path_key(path, fold_case);
        if protected.covers(&key) || neighbours.uses(&key) {
            return;
        }
        found.push(Leftover { kind, path: path.to_string_lossy().to_string(), value: None, confidence, reason: reason.into(), size: None });
    };
    // 名字对得上、又不是别的软件的名字
    let pick = |name: &str| matches(name, &variants).filter(|(variant, _)| !neighbours.names.contains(variant) && !neighbours.names.contains(&norm(name)));

    // 1. 卸载程序没删掉的安装文件夹
    if let Some(dir) = target.install_dir.as_deref().filter(|dir| dir.is_dir() && dir.extension().is_none_or(|ext| ext != "app")) {
        add(dir, LeftoverKind::Folder, Confidence::High, "installDir", &mut found);
    }

    // 2. Program Files、AppData、ProgramData 里同名的文件夹，或者“发布者\软件名”
    for (list, exact) in [(&roots.program, Confidence::High), (&roots.data, Confidence::Medium)] {
        for root in list {
            for entry in subdirs(root) {
                let name = file_name(&entry);
                match pick(&name) {
                    Some((_, true)) => add(&entry, LeftoverKind::Folder, exact, "name", &mut found),
                    Some((_, false)) => add(&entry, LeftoverKind::Folder, Confidence::Low, "nameContains", &mut found),
                    None if publisher.as_deref() == Some(norm(&name).as_str()) => {
                        for inner in subdirs(&entry) {
                            match pick(&file_name(&inner)) {
                                Some((_, true)) => add(&inner, LeftoverKind::Folder, Confidence::High, "publisherName", &mut found),
                                Some((_, false)) => add(&inner, LeftoverKind::Folder, Confidence::Low, "nameContains", &mut found),
                                None => {}
                            }
                        }
                    }
                    None => {}
                }
            }
        }
    }

    // 3. 开始菜单里同名的文件夹和快捷方式；桌面上同名的快捷方式
    for root in &roots.start_menu {
        for entry in read(root) {
            let is_shortcut = is_shortcut(&entry);
            if !(entry.is_dir() || is_shortcut) {
                continue;
            }
            let stem = if is_shortcut { stem(&entry) } else { file_name(&entry) };
            if let Some((_, true)) = pick(&stem) {
                let kind = if is_shortcut { LeftoverKind::File } else { LeftoverKind::Folder };
                add(&entry, kind, Confidence::Medium, "shortcut", &mut found);
            }
        }
    }
    for root in &roots.desktop {
        for entry in read(root).into_iter().filter(|e| is_shortcut(e)) {
            if let Some((_, true)) = pick(&stem(&entry)) {
                add(&entry, LeftoverKind::File, Confidence::Medium, "shortcut", &mut found);
            }
        }
    }

    // 4. macOS：~/Library 下按 bundle id 和名字找
    if let Some(library) = &roots.library {
        scan_library(library, target, &variants, publisher.as_deref(), &pick, &mut |path, kind, confidence, reason| {
            add(path, kind, confidence, reason, &mut found)
        });
    }

    tidy(found, fold_case)
}

/// macOS 的 ~/Library。bundle id 精确对得上的可信度高；名字对得上的只在 Application Support、Caches、Logs 里找，
/// 可信度中；那里面“发布者/软件名”（JetBrains/DataGrip2024.3）对得上的可信度高，只是包含软件名的低。
fn scan_library(
    library: &Path,
    target: &Target,
    variants: &[String],
    publisher: Option<&str>,
    pick: &dyn Fn(&str) -> Option<(String, bool)>,
    add: &mut dyn FnMut(&Path, LeftoverKind, Confidence, &str),
) {
    let kind_of = |path: &Path| if path.is_dir() { LeftoverKind::Folder } else { LeftoverKind::File };
    if let Some(id) = target.bundle_id.as_deref().filter(|id| id.len() >= 5 && !id.starts_with("com.apple.")) {
        let exact = [
            format!("Application Support/{id}"),
            format!("Caches/{id}"),
            format!("Containers/{id}"),
            format!("HTTPStorages/{id}"),
            format!("HTTPStorages/{id}.binarycookies"),
            format!("WebKit/{id}"),
            format!("Logs/{id}"),
            format!("Application Scripts/{id}"),
            format!("Saved Application State/{id}.savedState"),
            format!("Preferences/{id}.plist"),
            format!("Cookies/{id}.binarycookies"),
        ];
        for relative in exact {
            let path = library.join(relative);
            if path.exists() {
                add(&path, kind_of(&path), Confidence::High, "bundleId");
            }
        }
        // 同一个 bundle id 开头的：ByHost 偏好、辅助程序的偏好、开机启动项、App Group
        let prefixed = format!("{id}.");
        for (folder, confidence) in [("Preferences/ByHost", Confidence::High), ("Preferences", Confidence::Medium), ("LaunchAgents", Confidence::Medium)] {
            for entry in read(&library.join(folder)) {
                let name = file_name(&entry);
                if entry.is_file() && name.starts_with(&prefixed) && name != format!("{id}.plist") {
                    add(&entry, LeftoverKind::File, confidence, "bundleId");
                }
            }
        }
        for entry in subdirs(&library.join("Group Containers")) {
            let name = file_name(&entry);
            if name == format!("group.{id}") || name.ends_with(&format!(".{id}")) {
                add(&entry, LeftoverKind::Folder, Confidence::Medium, "bundleId");
            }
        }
    }
    if !variants.is_empty() {
        for folder in ["Application Support", "Caches", "Logs"] {
            for entry in subdirs(&library.join(folder)) {
                let name = file_name(&entry);
                if let Some((_, true)) = pick(&name) {
                    add(&entry, LeftoverKind::Folder, Confidence::Medium, "name");
                } else if publisher == Some(norm(&name).as_str()) {
                    for inner in subdirs(&entry) {
                        match pick(&file_name(&inner)) {
                            Some((_, true)) => add(&inner, LeftoverKind::Folder, Confidence::High, "publisherName"),
                            Some((_, false)) => add(&inner, LeftoverKind::Folder, Confidence::Low, "nameContains"),
                            None => {}
                        }
                    }
                }
            }
        }
    }
}

/// 去掉重复的；一个文件夹已经在列表里了，它里面的就不再单列。算出每项的大小。按可信度、路径排好。
fn tidy(mut found: Vec<Leftover>, fold_case: bool) -> Vec<Leftover> {
    found.sort_by(|a, b| path_key(Path::new(&a.path), fold_case).cmp(&path_key(Path::new(&b.path), fold_case)).then(b.confidence.cmp(&a.confidence)));
    let mut kept: Vec<Leftover> = Vec::new();
    let mut kept_keys: Vec<String> = Vec::new();
    for item in found {
        let key = path_key(Path::new(&item.path), fold_case);
        let covered = kept_keys.iter().any(|k| *k == key || (key.starts_with(k.as_str()) && key[k.len()..].starts_with('/')));
        if covered {
            continue;
        }
        kept_keys.push(key);
        kept.push(item);
    }
    for item in &mut kept {
        let path = Path::new(&item.path);
        item.size = match item.kind {
            LeftoverKind::Folder => folder_size(path),
            LeftoverKind::File => std::fs::metadata(path).ok().map(|m| m.len()),
            _ => None,
        };
    }
    kept.sort_by(|a, b| b.confidence.cmp(&a.confidence).then_with(|| a.path.to_lowercase().cmp(&b.path.to_lowercase())));
    kept
}

fn read(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).collect()
}

/// 下一层的文件夹（不跟着符号链接走）。
fn subdirs(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect()
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

fn stem(path: &Path) -> String {
    path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

fn is_shortcut(path: &Path) -> bool {
    path.is_file() && path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("lnk") || ext.eq_ignore_ascii_case("url"))
}

// ── 删除前的检查 ─────────────────────────────────────────────

/// 删之前再查一遍：只删扫描规则允许的范围（公共文件夹、太浅的路径、注册表的要紧位置一律不删）。
/// 删除的清单来自界面，管理员身份运行时来自一个文件，所以不能只信扫描时的判断。
pub fn allowed(item: &Leftover) -> bool {
    match item.kind {
        LeftoverKind::Folder | LeftoverKind::File => {
            let fold_case = cfg!(windows);
            let key = path_key(Path::new(&item.path), fold_case);
            let depth = key.split('/').filter(|part| !part.is_empty()).count();
            depth >= 3 && !Protected::from_env().covers(&key) && !library_root(&key, fold_case) && Path::new(&item.path).exists()
        }
        LeftoverKind::RegistryKey => registry::key_allowed(&item.path),
        LeftoverKind::RegistryValue => item.value.as_deref().is_some_and(|v| !v.is_empty()) && registry::run_key(&item.path),
    }
}

/// ~/Library 本身和它下面那些标准的文件夹（Caches、Preferences……）不能删。
fn library_root(key: &str, fold_case: bool) -> bool {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else { return false };
    let library = path_key(&home.join("Library"), fold_case);
    if key == library {
        return true;
    }
    match key.strip_prefix(&format!("{library}/")) {
        Some(rest) => !rest.contains('/') || rest.split('/').count() == 2 && rest.starts_with("Preferences/ByHost"),
        None => false,
    }
}

// ── 删除 ─────────────────────────────────────────────────────

/// 删掉这些残留：文件和文件夹移到回收站；注册表项先导出到 `backup_dir` 再删。没通过 `allowed` 的不删。
pub fn remove(items: &[Leftover], backup_dir: &Path) -> RemoveReport {
    let mut report = RemoveReport::default();
    let mut backup: Option<PathBuf> = None;
    for item in items {
        if !allowed(item) {
            report.failed.push(Failure { item: item.clone(), reason: "not_allowed".into() });
            continue;
        }
        let result = match item.kind {
            LeftoverKind::Folder | LeftoverKind::File => trash::delete(&item.path).map_err(|e| {
                let text = e.to_string();
                if machine_wide(&item.path) || text.to_lowercase().contains("denied") || text.contains("0x80070005") {
                    "access_denied".to_string()
                } else {
                    text
                }
            }),
            LeftoverKind::RegistryKey | LeftoverKind::RegistryValue => {
                let folder = backup.get_or_insert_with(|| backup_dir.join(chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()));
                registry::backup_and_delete(item, folder)
            }
        };
        match result {
            Ok(()) => report.removed += 1,
            Err(reason) => {
                report.needs_admin |= reason == "access_denied";
                report.failed.push(Failure { item: item.clone(), reason });
            }
        }
    }
    report.backup = backup.filter(|dir| dir.is_dir()).map(|dir| dir.to_string_lossy().to_string());
    report
}

/// Windows 上所有用户共用的位置，普通权限一般删不掉。
fn machine_wide(path: &str) -> bool {
    if !cfg!(windows) {
        return false;
    }
    let key = path_key(Path::new(path), true);
    ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432", "ProgramData", "PUBLIC"]
        .iter()
        .filter_map(|name| std::env::var_os(name))
        .any(|root| key.starts_with(&format!("{}/", path_key(Path::new(&root), true))))
}

/// 管理员身份清理时交给另一个进程的清单。
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Plan {
    items: Vec<Leftover>,
    backup_dir: PathBuf,
}

const ELEVATED_FLAG: &str = "--shibapack-cleanup";

/// 以管理员身份删：把清单写进临时文件，用 UAC 提权再启动一次 ShibaPack（只做清理，不开窗口），等它做完读回结果。
/// 用户在 UAC 提示里点了“否”时返回 `cancelled`。只有 Windows 有。
#[cfg(windows)]
pub fn remove_elevated(items: &[Leftover], backup_dir: &Path) -> Result<RemoveReport, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = std::env::temp_dir().join(format!("shibapack-cleanup-{}", crate::new_id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let plan = dir.join("plan.json");
    let result = dir.join("result.json");
    let body = serde_json::to_vec(&Plan { items: items.to_vec(), backup_dir: backup_dir.to_path_buf() }).map_err(|e| e.to_string())?;
    std::fs::write(&plan, body).map_err(|e| e.to_string())?;
    // PowerShell 的单引号字符串里，单引号写两遍；路径可能有空格，参数再包一层双引号
    let quote = |path: &Path| path.to_string_lossy().replace('\'', "''");
    let script = format!(
        "Start-Process -FilePath '{}' -ArgumentList @('{ELEVATED_FLAG}', '\"{}\"') -Verb RunAs -Wait",
        quote(&exe),
        quote(&plan)
    );
    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
    let report = std::fs::read(&result).ok().and_then(|bytes| serde_json::from_slice::<RemoveReport>(&bytes).ok());
    let _ = std::fs::remove_dir_all(&dir);
    match (report, status) {
        (Some(report), _) => Ok(report),
        (None, Err(e)) => Err(e.to_string()),
        // 没有结果文件：多半是在 UAC 提示里点了“否”
        (None, Ok(_)) => Err("cancelled".into()),
    }
}

#[cfg(not(windows))]
pub fn remove_elevated(_items: &[Leftover], _backup_dir: &Path) -> Result<RemoveReport, String> {
    Err("not_supported".into())
}

/// 提权启动的那个进程：命令行是 `--shibapack-cleanup <清单>` 时只做清理，写好结果就退出。
/// 在应用启动的最开头调用；不是这种启动方式时返回 `None`，照常启动。
pub fn elevated_entry() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    if args.next()?.to_string_lossy() != ELEVATED_FLAG {
        return None;
    }
    let plan_path = PathBuf::from(args.next()?.to_string_lossy().trim_matches('"'));
    let Some(plan) = std::fs::read(&plan_path).ok().and_then(|bytes| serde_json::from_slice::<Plan>(&bytes).ok()) else {
        return Some(2);
    };
    let report = remove(&plan.items, &plan.backup_dir);
    let written = serde_json::to_vec(&report).ok().and_then(|body| std::fs::write(plan_path.with_file_name("result.json"), body).ok());
    Some(if written.is_some() { 0 } else { 1 })
}

/// 软件还在不在：Windows 看卸载信息的注册表键还在不在；macOS 看 .app 还在不在。
pub fn still_installed(target: &Target) -> bool {
    match &target.registry_key {
        Some(key) => registry::exists(key),
        None => target.install_dir.as_deref().is_some_and(Path::exists),
    }
}

// ── 注册表（只有 Windows）────────────────────────────────────

mod registry {
    use super::*;

    /// 这些键本身和下面的要紧位置不能删。
    const BLOCKED_TOP: &[&str] = &["classes", "policies", "clients", "registeredapplications", "wow6432node"];

    /// 注册表路径拆成段，第一段统一成 HKCU / HKLM。
    pub(super) fn parts(path: &str) -> Option<Vec<String>> {
        let mut parts: Vec<String> = path.split('\\').filter(|p| !p.is_empty()).map(str::to_string).collect();
        let hive = match parts.first()?.to_ascii_uppercase().as_str() {
            "HKCU" | "HKEY_CURRENT_USER" => "HKCU",
            "HKLM" | "HKEY_LOCAL_MACHINE" => "HKLM",
            _ => return None,
        };
        parts[0] = hive.into();
        Some(parts)
    }

    /// 允许删的注册表键：Software\软件、Software\发布者\软件（含 WOW6432Node）、某一个卸载项、某一个 App Paths 项。
    pub(super) fn key_allowed(path: &str) -> bool {
        let Some(parts) = parts(path) else { return false };
        let lower: Vec<String> = parts.iter().map(|p| p.to_lowercase()).collect();
        if lower.get(1).map(String::as_str) != Some("software") {
            return false;
        }
        let mut rest: Vec<&str> = lower[2..].iter().map(String::as_str).collect();
        if rest.first() == Some(&"wow6432node") {
            rest.remove(0);
        }
        // 某一个卸载项、某一个 App Paths 项
        if rest.len() == 5 && rest[..4] == ["microsoft", "windows", "currentversion", "uninstall"] {
            return true;
        }
        if rest.len() == 5 && rest[..4] == ["microsoft", "windows", "currentversion", "app paths"] {
            return true;
        }
        match rest.as_slice() {
            [app] => !BLOCKED_TOP.contains(app) && !GENERIC.contains(&norm(app).as_str()),
            [publisher, app] => {
                !BLOCKED_TOP.contains(publisher) && !(*publisher == "microsoft" && matches!(*app, "windows" | "windows nt" | "cryptography" | "ole"))
                    && !GENERIC.contains(&norm(app).as_str())
            }
            _ => false,
        }
    }

    /// 开机启动的 Run 键（只允许删这里面的值）。
    pub(super) fn run_key(path: &str) -> bool {
        let Some(parts) = parts(path) else { return false };
        let lower = parts[1..].iter().map(|p| p.to_lowercase()).collect::<Vec<_>>().join("\\");
        lower == r"software\microsoft\windows\currentversion\run" || lower == r"software\wow6432node\microsoft\windows\currentversion\run"
    }

    #[cfg(not(windows))]
    pub(super) fn scan(_target: &Target, _others: &[InstalledApp]) -> Vec<Leftover> {
        Vec::new()
    }

    #[cfg(not(windows))]
    pub(super) fn exists(_path: &str) -> bool {
        false
    }

    #[cfg(not(windows))]
    pub(super) fn backup_and_delete(_item: &Leftover, _folder: &Path) -> Result<(), String> {
        Err("not_supported".into())
    }

    #[cfg(windows)]
    use winreg::{enums::*, RegKey};

    #[cfg(windows)]
    fn open(path: &str) -> Option<(RegKey, String)> {
        let parts = parts(path)?;
        let hive = RegKey::predef(if parts[0] == "HKCU" { HKEY_CURRENT_USER } else { HKEY_LOCAL_MACHINE });
        Some((hive, parts[1..].join("\\")))
    }

    #[cfg(windows)]
    pub(super) fn exists(path: &str) -> bool {
        open(path).is_some_and(|(hive, sub)| hive.open_subkey(sub).is_ok())
    }

    #[cfg(windows)]
    pub(super) fn scan(target: &Target, others: &[InstalledApp]) -> Vec<Leftover> {
        let variants = name_variants(&target.name, &target.publisher);
        let publisher = publisher_key(&target.publisher);
        let neighbours = Neighbours::of(target, others, true);
        let taken = |name: &str, variant: &str| neighbours.names.contains(variant) || neighbours.names.contains(&norm(name));
        let mut found = Vec::new();
        let mut push = |kind: LeftoverKind, path: String, value: Option<String>, confidence: Confidence, reason: &str| {
            if found.iter().any(|f: &Leftover| f.path.eq_ignore_ascii_case(&path) && f.value == value) {
                return;
            }
            found.push(Leftover { kind, path, value, confidence, reason: reason.into(), size: None });
        };

        // 1. 卸载程序没删掉的卸载项
        if let Some(key) = &target.registry_key {
            if exists(key) && key_allowed(key) {
                push(LeftoverKind::RegistryKey, key.clone(), None, Confidence::High, "uninstallKey");
            }
        }

        // 2. Software\软件名、Software\发布者\软件名
        let bases = [("HKCU", r"Software"), ("HKLM", r"SOFTWARE"), ("HKLM", r"SOFTWARE\WOW6432Node")];
        for (hive, base) in bases {
            let Some((root, sub)) = open(&format!(r"{hive}\{base}")) else { continue };
            let Ok(base_key) = root.open_subkey(&sub) else { continue };
            for name in base_key.enum_keys().flatten() {
                if BLOCKED_TOP.contains(&name.to_lowercase().as_str()) {
                    continue;
                }
                if let Some((variant, true)) = matches(&name, &variants) {
                    if !taken(&name, &variant) {
                        let path = format!(r"{hive}\{base}\{name}");
                        if key_allowed(&path) {
                            push(LeftoverKind::RegistryKey, path, None, Confidence::Medium, "name");
                        }
                    }
                } else if publisher.as_deref() == Some(norm(&name).as_str()) {
                    let Ok(vendor) = base_key.open_subkey(&name) else { continue };
                    for inner in vendor.enum_keys().flatten() {
                        if let Some((variant, true)) = matches(&inner, &variants) {
                            if !taken(&inner, &variant) {
                                let path = format!(r"{hive}\{base}\{name}\{inner}");
                                if key_allowed(&path) {
                                    push(LeftoverKind::RegistryKey, path, None, Confidence::High, "publisherName");
                                }
                            }
                        }
                    }
                }
            }
        }

        // 3. 指向原来安装文件夹的 App Paths 项和开机启动项
        let protected = Protected::from_env();
        let dir = target
            .install_dir
            .as_deref()
            .map(|dir| path_key(dir, true))
            .filter(|key| !protected.covers(key) && !neighbours.uses(key));
        if let Some(dir) = dir {
            let points_into = |text: &str| {
                let text = text.trim().trim_start_matches('"').replace('\\', "/").to_lowercase();
                text.starts_with(&format!("{dir}/")) || text.contains(&format!("\"{dir}/"))
            };
            for (hive, base) in [("HKCU", r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths"), ("HKLM", r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths")] {
                let Some((root, sub)) = open(&format!(r"{hive}\{base}")) else { continue };
                let Ok(paths) = root.open_subkey(&sub) else { continue };
                for name in paths.enum_keys().flatten() {
                    let Ok(key) = paths.open_subkey(&name) else { continue };
                    let default: String = key.get_value("").unwrap_or_default();
                    if points_into(&default) {
                        push(LeftoverKind::RegistryKey, format!(r"{hive}\{base}\{name}"), None, Confidence::High, "appPath");
                    }
                }
            }
            for (hive, base) in [
                ("HKCU", r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run"),
                ("HKLM", r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run"),
                ("HKLM", r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run"),
            ] {
                let Some((root, sub)) = open(&format!(r"{hive}\{base}")) else { continue };
                let Ok(run) = root.open_subkey(&sub) else { continue };
                for (name, _) in run.enum_values().flatten() {
                    let command: String = run.get_value(&name).unwrap_or_default();
                    if !name.is_empty() && points_into(&command) {
                        push(LeftoverKind::RegistryValue, format!(r"{hive}\{base}"), Some(name), Confidence::High, "startup");
                    }
                }
            }
        }
        found
    }

    /// 先用 reg.exe 把键（值的话是它所在的键）导出到 `folder`，导出成功了才删。
    #[cfg(windows)]
    pub(super) fn backup_and_delete(item: &Leftover, folder: &Path) -> Result<(), String> {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let Some((root, sub)) = open(&item.path) else { return Err("not_allowed".into()) };
        if root.open_subkey(&sub).is_err() {
            // 已经没有了，算删掉
            return Ok(());
        }
        std::fs::create_dir_all(folder).map_err(|_| "backup_failed".to_string())?;
        let count = std::fs::read_dir(folder).map(|d| d.count()).unwrap_or(0);
        let file = folder.join(format!("{:03}.reg", count + 1));
        let exported = std::process::Command::new("reg")
            .arg("export")
            .arg(&item.path)
            .arg(&file)
            .arg("/y")
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .is_ok_and(|status| status.success());
        if !exported {
            return Err("backup_failed".into());
        }
        let denied = |e: std::io::Error| if e.kind() == std::io::ErrorKind::PermissionDenied { "access_denied".to_string() } else { e.to_string() };
        match (&item.kind, &item.value) {
            (LeftoverKind::RegistryValue, Some(value)) => {
                let key = root.open_subkey_with_flags(&sub, KEY_SET_VALUE).map_err(denied)?;
                key.delete_value(value).map_err(denied)
            }
            _ => root.delete_subkey_all(&sub).map_err(denied),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed(id: &str, name: &str, publisher: &str, dir: Option<&Path>) -> InstalledApp {
        InstalledApp {
            id: id.into(),
            name: name.into(),
            version: String::new(),
            publisher: publisher.into(),
            installed_at: String::new(),
            size: None,
            source: "registry".into(),
            location: String::new(),
            uninstallable: true,
            icon: None,
            icon_source: None,
            uninstall_command: None,
            install_dir: dir.map(Path::to_path_buf),
            bundle_id: None,
        }
    }

    /// 卸载前的分析只留数据：软件本身（安装文件夹、Program Files 里的、卸载项、App Paths）交给卸载程序。
    #[test]
    fn analysis_before_uninstall_leaves_the_program_alone() {
        let item = |kind, path: &str, reason: &str| Leftover { kind, path: path.into(), value: None, confidence: Confidence::High, reason: reason.into(), size: None };
        let found = vec![
            item(LeftoverKind::Folder, "C:/Program Files/Foo", "installDir"),
            item(LeftoverKind::Folder, "C:/Program Files (x86)/Foo", "name"),
            item(LeftoverKind::Folder, "C:/Users/me/AppData/Local/Programs/Foo", "name"),
            item(LeftoverKind::Folder, "C:/Users/me/AppData/Roaming/Foo", "name"),
            item(LeftoverKind::File, "C:/Users/me/Desktop/Foo.lnk", "shortcut"),
            item(LeftoverKind::RegistryKey, r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Foo", "uninstallKey"),
            item(LeftoverKind::RegistryKey, r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\foo.exe", "appPath"),
            item(LeftoverKind::RegistryKey, r"HKCU\Software\Foo", "name"),
        ];
        let program = ["c:/program files", "c:/program files (x86)", "c:/users/me/appdata/local/programs"].map(String::from);
        let kept: Vec<String> = data_only(found, &program, true).into_iter().map(|item| item.path).collect();
        assert_eq!(kept, ["C:/Users/me/AppData/Roaming/Foo", "C:/Users/me/Desktop/Foo.lnk", r"HKCU\Software\Foo"]);
    }

    fn no_protection() -> Protected {
        Protected { exact: Vec::new(), under: Vec::new() }
    }

    fn dirs(base: &Path, list: &[&str]) {
        for path in list {
            std::fs::create_dir_all(base.join(path)).unwrap();
            std::fs::write(base.join(path).join("x.dat"), b"12345").unwrap();
        }
    }

    /// 在这台电脑上对一个装着的软件试找一遍残留，看看会找出什么（只读，什么都不删）：
    /// `SHIBAPACK_LEFTOVER_APP="软件名" cargo test -p shibapack-core scans_real_app -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn scans_real_app() {
        let name = std::env::var("SHIBAPACK_LEFTOVER_APP").expect("先设置 SHIBAPACK_LEFTOVER_APP=软件名");
        let dir = tempfile::tempdir().unwrap();
        let apps = crate::installed::list_installed(dir.path());
        let app = apps.iter().find(|a| a.name.eq_ignore_ascii_case(&name)).expect("这台电脑上没有这个软件");
        let others: Vec<InstalledApp> = apps.iter().filter(|a| a.id != app.id).cloned().collect();
        let found = scan(&Target::of(app), &others, false);
        for item in &found {
            let size = item.size.map(|s| format!("{:.1} MB", s as f64 / 1e6)).unwrap_or_default();
            eprintln!("{:<6} {:<13} {:<14} {:>10}  {}{}", format!("{:?}", item.confidence), format!("{:?}", item.kind), item.reason, size, item.path, item.value.as_deref().map(|v| format!(" → {v}")).unwrap_or_default());
        }
        eprintln!("{}：找到 {} 项", app.name, found.len());
    }

    #[test]
    fn name_variants_drop_versions_and_publisher() {
        assert_eq!(name_variants("Microsoft Edge", "Microsoft Corporation"), ["microsoftedge", "edge"]);
        assert_eq!(name_variants("7-Zip 24.09 (x64)", "Igor Pavlov"), ["7zip2409x64", "7zip"]);
        assert_eq!(name_variants("微信", "腾讯科技(深圳)有限公司"), ["微信"]);
        assert!(name_variants("Update", "").is_empty(), "太泛的名字不拿来匹配");
        assert!(name_variants("Foo", "").is_empty(), "太短");
        assert_eq!(publisher_key("Tencent Technology(Shenzhen) Company Limited").as_deref(), Some("tencent"));
    }

    #[test]
    fn finds_windows_style_leftovers() {
        let base = tempfile::tempdir().unwrap();
        let b = base.path();
        dirs(
            b,
            &[
                "pf/Notepad++",
                "pf/Notepad++ Plugins Extra Long Name",
                "pf/Common Files",
                "pfx86/Acme/Widget Studio",
                "pfx86/Acme/Other Tool",
                "roaming/Notepad++",
                "roaming/notepad++backup",
                "local/Widget Studio",
                "local/Python",
                "start/Notepad++",
            ],
        );
        std::fs::write(b.join("desktop-Notepad++.lnk"), b"").unwrap();
        std::fs::create_dir_all(b.join("desktop")).unwrap();
        std::fs::write(b.join("desktop/Notepad++.lnk"), b"").unwrap();
        std::fs::write(b.join("desktop/Notes.lnk"), b"").unwrap();
        let roots = Roots {
            program: vec![b.join("pf"), b.join("pfx86")],
            data: vec![b.join("roaming"), b.join("local")],
            start_menu: vec![b.join("start")],
            desktop: vec![b.join("desktop")],
            library: None,
        };
        let others = vec![installed("other", "Other Tool", "Acme", Some(&b.join("pfx86/Acme/Other Tool"))), installed("py", "Python 3.12", "", None)];

        let npp = Target { name: "Notepad++ (64-bit x64)".into(), publisher: "Notepad++ Team".into(), ..Target::default() };
        let found = scan_files(&npp, &others, &roots, &no_protection(), false);
        let at = |rel: &str| found.iter().find(|f| f.path == b.join(rel).to_string_lossy());
        assert_eq!(at("pf/Notepad++").unwrap().confidence, Confidence::High);
        assert_eq!(at("roaming/Notepad++").unwrap().confidence, Confidence::Medium);
        assert_eq!(at("roaming/notepad++backup").unwrap().confidence, Confidence::Low);
        assert_eq!(at("start/Notepad++").unwrap().reason, "shortcut");
        assert_eq!(at("desktop/Notepad++.lnk").unwrap().kind, LeftoverKind::File);
        assert!(at("desktop/Notes.lnk").is_none());
        assert!(at("pf/Common Files").is_none());
        assert_eq!(at("pf/Notepad++").unwrap().size, Some(5));
        // 高可信度排在前面
        assert_eq!(found[0].confidence, Confidence::High);

        let widget = Target {
            name: "Widget Studio".into(),
            publisher: "Acme Inc.".into(),
            install_dir: Some(b.join("pfx86/Acme/Widget Studio")),
            ..Target::default()
        };
        let found = scan_files(&widget, &others, &roots, &no_protection(), false);
        let at = |rel: &str| found.iter().find(|f| f.path == b.join(rel).to_string_lossy());
        assert_eq!(at("pfx86/Acme/Widget Studio").unwrap().reason, "installDir", "安装文件夹只列一次");
        assert_eq!(found.iter().filter(|f| f.path.contains("Widget Studio") && f.path.contains("pfx86")).count(), 1);
        assert!(at("pfx86/Acme").is_none(), "发布者文件夹本身不删");
        assert!(at("pfx86/Acme/Other Tool").is_none(), "别的软件的文件夹");
        assert_eq!(at("local/Widget Studio").unwrap().confidence, Confidence::Medium);

        // 和别的已安装软件同名（Python 3.11 卸了，Python 3.12 还在）：不算
        let py = Target { name: "Python 3.11".into(), ..Target::default() };
        let found = scan_files(&py, &others, &roots, &no_protection(), false);
        assert!(found.iter().all(|f| !f.path.ends_with("Python")));

        // 公共文件夹不算
        let protected = Protected { exact: vec![path_key(&b.join("pf/Notepad++"), false)], under: Vec::new() };
        let found = scan_files(&npp, &others, &roots, &protected, false);
        assert!(found.iter().all(|f| f.path != b.join("pf/Notepad++").to_string_lossy()));
    }

    #[test]
    fn finds_mac_library_leftovers() {
        let base = tempfile::tempdir().unwrap();
        let lib = base.path().join("Library");
        dirs(
            &lib,
            &[
                "Application Support/com.acme.widget",
                "Application Support/Widget Studio",
                "Application Support/Unrelated",
                "Caches/com.acme.widget",
                "Containers/com.acme.widget",
                "Group Containers/ABCDE12345.com.acme.widget",
                "Saved Application State/com.acme.widget.savedState",
                "Logs/Other",
                "Application Support/Acme/WidgetStudio2024.3",
                "Application Support/Acme/Other Thing",
            ],
        );
        std::fs::create_dir_all(lib.join("Preferences/ByHost")).unwrap();
        std::fs::create_dir_all(lib.join("LaunchAgents")).unwrap();
        for file in [
            "Preferences/com.acme.widget.plist",
            "Preferences/com.acme.widget.helper.plist",
            "Preferences/com.acme.widgetpro.plist",
            "Preferences/ByHost/com.acme.widget.1234.plist",
            "LaunchAgents/com.acme.widget.updater.plist",
        ] {
            std::fs::write(lib.join(file), b"x").unwrap();
        }
        let roots = Roots { library: Some(lib.clone()), ..Roots::default() };
        let target = Target {
            name: "Widget Studio".into(),
            publisher: "Acme".into(),
            bundle_id: Some("com.acme.widget".into()),
            ..Target::default()
        };
        let found = scan_files(&target, &[], &roots, &no_protection(), false);
        let names: Vec<String> = found.iter().map(|f| f.path.strip_prefix(&*lib.to_string_lossy()).unwrap().to_string()).collect();
        for expected in [
            "/Application Support/com.acme.widget",
            "/Application Support/Widget Studio",
            "/Caches/com.acme.widget",
            "/Containers/com.acme.widget",
            "/Group Containers/ABCDE12345.com.acme.widget",
            "/Saved Application State/com.acme.widget.savedState",
            "/Preferences/com.acme.widget.plist",
            "/Preferences/com.acme.widget.helper.plist",
            "/Preferences/ByHost/com.acme.widget.1234.plist",
            "/LaunchAgents/com.acme.widget.updater.plist",
        ] {
            assert!(names.iter().any(|n| n == expected), "缺少 {expected}：{names:?}");
        }
        assert!(!names.iter().any(|n| n.contains("widgetpro") || n.contains("Unrelated") || n.contains("Other")));
        let versioned = found.iter().find(|f| f.path.ends_with("Acme/WidgetStudio2024.3")).expect("发布者文件夹里带版本号的");
        assert_eq!((versioned.confidence, versioned.reason.as_str()), (Confidence::Low, "nameContains"));
        assert!(!names.iter().any(|n| n.ends_with("/Acme")), "发布者文件夹本身不删");
        let pref = found.iter().find(|f| f.path.ends_with("Preferences/com.acme.widget.plist")).unwrap();
        assert_eq!((pref.confidence, pref.reason.as_str(), pref.kind), (Confidence::High, "bundleId", LeftoverKind::File));

        // 苹果自己的应用不找
        let apple = Target { name: "Safari".into(), bundle_id: Some("com.apple.Safari".into()), ..Target::default() };
        assert!(scan_files(&apple, &[], &roots, &no_protection(), false).iter().all(|f| !f.path.contains("com.apple")));
    }

    #[test]
    fn registry_paths_are_checked_before_deleting() {
        assert!(registry::key_allowed(r"HKCU\Software\Notepad++"));
        assert!(registry::key_allowed(r"HKLM\SOFTWARE\WOW6432Node\Acme\Widget Studio"));
        assert!(registry::key_allowed(r"HKCU\Software\Microsoft\Edge"));
        assert!(registry::key_allowed(r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Foo"));
        assert!(registry::key_allowed(r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\foo.exe"));
        for blocked in [
            r"HKCU\Software",
            r"HKCU\Software\Microsoft",
            r"HKCU\Software\Classes",
            r"HKCU\Software\Classes\foo",
            r"HKLM\SOFTWARE\Microsoft\Windows",
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            r"HKCU\Software\Acme\Widget\Deep",
            r"HKCR\foo",
            r"HKLM\SYSTEM\CurrentControlSet\Services\foo",
        ] {
            assert!(!registry::key_allowed(blocked), "{blocked}");
        }
        assert!(registry::run_key(r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run"));
        assert!(!registry::run_key(r"HKCU\Software\Microsoft\Windows\CurrentVersion"));
        let value = |path: &str, value: Option<&str>| Leftover {
            kind: LeftoverKind::RegistryValue,
            path: path.into(),
            value: value.map(String::from),
            confidence: Confidence::High,
            reason: "startup".into(),
            size: None,
        };
        assert!(allowed(&value(r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", Some("Widget"))));
        assert!(!allowed(&value(r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", None)));
        assert!(!allowed(&value(r"HKCU\Software\Acme", Some("x"))));
    }

    #[test]
    fn shallow_or_missing_paths_are_never_deleted() {
        let folder = |path: &str| Leftover {
            kind: LeftoverKind::Folder,
            path: path.into(),
            value: None,
            confidence: Confidence::High,
            reason: "name".into(),
            size: None,
        };
        assert!(!allowed(&folder("/")));
        assert!(!allowed(&folder("/tmp")));
        assert!(!allowed(&folder("/definitely/not/here/at-all")));
        if let Some(home) = std::env::var_os("HOME") {
            let library = PathBuf::from(home).join("Library");
            assert!(!allowed(&folder(&library.to_string_lossy())));
            assert!(!allowed(&folder(&library.join("Caches").to_string_lossy())));
        }
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("a/b/c");
        std::fs::create_dir_all(&target).unwrap();
        assert!(allowed(&folder(&target.to_string_lossy())));

        // 不允许的不删，报 not_allowed；允许的移到废纸篓（这里不真的调用废纸篓，只看拒绝的那条）
        let report = remove(&[folder("/")], dir.path());
        assert_eq!(report.removed, 0);
        assert_eq!(report.failed[0].reason, "not_allowed");
    }
}
