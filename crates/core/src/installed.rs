//! 这台电脑上已经安装的软件。
//!
//! - macOS：扫描“应用程序”文件夹里的 .app，读 Info.plist。
//! - Windows：读注册表的卸载列表。
//! - Linux：读各处的 .desktop 文件（拿不到版本号）。

use crate::icons;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledApp {
    /// 稳定的标识：.app 的路径、注册表键或 .desktop 文件的路径。
    pub id: String,
    pub name: String,
    pub version: String,
    pub publisher: String,
    /// 安装时间，ISO 8601；不知道时为空。
    pub installed_at: String,
    /// 安装程序登记的占用空间（Windows 注册表的 EstimatedSize）。实际占用另外统计，见 `size_sources`。
    pub size: Option<u64>,
    /// applications / user / system / registry / desktop / flatpak / snap
    pub source: String,
    /// 在文件管理器里定位用的路径。
    pub location: String,
    pub uninstallable: bool,
    /// 取好的图标文件的绝对路径。
    pub icon: Option<String>,
    #[serde(skip)]
    pub icon_source: Option<PathBuf>,
    /// Windows 上系统登记的卸载命令。
    #[serde(skip)]
    pub uninstall_command: Option<String>,
    /// 这个软件自己的文件夹，可以走一遍统计实际占用：macOS 是 .app，Windows 是登记的 InstallLocation。
    #[serde(skip)]
    pub install_dir: Option<PathBuf>,
    /// macOS 的 bundle id（com.tencent.xinWeChat），卸载后按它找残留。
    #[serde(skip)]
    pub bundle_id: Option<String>,
}

/// 列出已安装的软件，按名称排序。图标取出来放在 `icon_cache` 里，下次直接用。
pub fn list_installed(icon_cache: &Path) -> Vec<InstalledApp> {
    #[cfg(windows)]
    let mut apps = platform::list(icon_cache);
    #[cfg(not(windows))]
    let mut apps = platform::list();
    for app in apps.iter_mut().filter(|app| app.icon.is_none()) {
        app.icon = app.icon_source.as_deref().and_then(|source| icon_for(source, icon_cache));
    }
    apps.sort_by_key(|app| app.name.to_lowercase());
    apps.dedup_by(|a, b| a.id == b.id);
    apps
}

/// 图标按“来源路径 + 修改时间”缓存：软件升级后图标文件变了会重新取。
pub(crate) fn icon_for(source: &Path, cache: &Path) -> Option<String> {
    icon_for_index(source, cache, 0)
}

fn icon_for_index(source: &Path, cache: &Path, index: i32) -> Option<String> {
    let meta = std::fs::metadata(source).ok()?;
    if !meta.is_file() {
        return None;
    }
    let modified = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos();
    let key = hex::encode(Sha256::digest(format!("v2|{}|{index}|{modified}|{}", source.display(), meta.len())));
    let stem = &key[..20];
    let path = icons::cached_icon(cache, stem).or_else(|| icons::extract_icon_at(source, cache, stem, index))?;
    Some(path.to_string_lossy().to_string())
}

/// 统计大小时，一个文件夹里最多走这么多个文件。再多就不像是一个软件自己的文件夹了，改用登记的大小。
const MAX_FILES: usize = 300_000;

/// 一个软件的大小从哪来。
#[derive(Debug, Clone, PartialEq)]
pub enum SizeSource {
    /// 把这个文件夹走一遍，加起来。
    Scan(PathBuf),
    /// 用安装程序登记的大小。
    Registered(u64),
    Unknown,
}

/// 决定每个软件的大小怎么统计：有自己的文件夹就走一遍（实际占用），否则用登记的大小。
///
/// Windows 上登记的安装位置有时是公共文件夹（整个盘、Program Files、用户文件夹……），或者几个软件共用、
/// 互相套着，走一遍会把别的东西算进来，这些都改用登记的大小。
pub fn size_sources(apps: &[InstalledApp]) -> Vec<(String, SizeSource)> {
    size_sources_with(apps, &Protected::from_env(), cfg!(windows))
}

fn size_sources_with(apps: &[InstalledApp], protected: &Protected, fold_case: bool) -> Vec<(String, SizeSource)> {
    let keys: Vec<Option<String>> = apps.iter().map(|app| app.install_dir.as_deref().map(|dir| path_key(dir, fold_case))).collect();
    let mut users: HashMap<&str, usize> = HashMap::new();
    for key in keys.iter().flatten() {
        *users.entry(key.as_str()).or_default() += 1;
    }
    apps.iter()
        .zip(&keys)
        .map(|(app, key)| {
            let own = key.as_deref().filter(|key| {
                !protected.covers(key)
                    // 只有这一个软件登记在这里
                    && users.get(key) == Some(&1)
                    // 里面没有套着别的软件的文件夹
                    && !users.keys().any(|other| other.len() > key.len() && other.starts_with(key) && other[key.len()..].starts_with('/'))
            });
            let source = match (own, &app.install_dir, app.size) {
                (Some(_), Some(dir), _) => SizeSource::Scan(dir.clone()),
                (_, _, Some(size)) => SizeSource::Registered(size),
                _ => SizeSource::Unknown,
            };
            (app.id.clone(), source)
        })
        .collect()
}

/// 比较路径用的样子：统一用 `/`，去掉末尾的分隔符；Windows 上不分大小写。
pub(crate) fn path_key(path: &Path, fold_case: bool) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let text = text.trim_end_matches('/');
    let text = if text.is_empty() { "/" } else { text };
    if fold_case { text.to_lowercase() } else { text.to_string() }
}

/// 不拿来统计大小的公共文件夹。
pub(crate) struct Protected {
    /// 这些文件夹本身（它们下面的子文件夹可以）。
    pub(crate) exact: Vec<String>,
    /// 这些文件夹和它们下面的一切。
    pub(crate) under: Vec<String>,
}

impl Protected {
    pub(crate) fn from_env() -> Protected {
        let fold_case = cfg!(windows);
        let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
        let mut exact: Vec<PathBuf> = [
            "ProgramFiles",
            "ProgramFiles(x86)",
            "ProgramW6432",
            "CommonProgramFiles",
            "CommonProgramFiles(x86)",
            "CommonProgramW6432",
            "ProgramData",
            "ALLUSERSPROFILE",
            "PUBLIC",
            "USERPROFILE",
            "APPDATA",
            "LOCALAPPDATA",
            "HOME",
        ]
        .iter()
        .filter_map(|name| var(name))
        .collect();
        if let Some(local) = var("LOCALAPPDATA") {
            exact.push(local.join("Programs"));
        }
        if let Some(drive) = var("SystemDrive") {
            exact.push(PathBuf::from(format!("{}\\Users", drive.to_string_lossy())));
        }
        let mut under: Vec<PathBuf> = ["SystemRoot", "windir"].iter().filter_map(|name| var(name)).collect();
        if let Some(data) = var("ProgramData") {
            under.push(data.join("Package Cache"));
        }
        Protected {
            exact: exact.iter().map(|p| path_key(p, fold_case)).collect(),
            under: under.iter().map(|p| path_key(p, fold_case)).collect(),
        }
    }

    pub(crate) fn covers(&self, key: &str) -> bool {
        // 整个盘（“c:”“/”）
        let depth = key.split('/').filter(|part| !part.is_empty()).count();
        depth <= 1
            || self.exact.iter().any(|p| p == key)
            || self.under.iter().any(|p| key == p || (key.starts_with(p.as_str()) && key[p.len()..].starts_with('/')))
    }
}

/// 一个文件夹里所有文件的大小加起来。不跟着符号链接走；文件太多（见 `MAX_FILES`）或者打不开时返回 `None`。
pub fn folder_size(path: &Path) -> Option<u64> {
    if !path.is_dir() {
        return None;
    }
    let (mut total, mut files) = (0u64, 0usize);
    for entry in walkdir::WalkDir::new(path).follow_links(false).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        files += 1;
        if files > MAX_FILES {
            return None;
        }
        total += entry.metadata().map(|meta| meta.len()).unwrap_or(0);
    }
    Some(total)
}

/// 运行系统登记的卸载程序。只有 Windows 有这个概念；别的系统返回 `Ok(false)`，由调用方另行处理。
#[cfg(windows)]
pub fn run_uninstaller(app: &InstalledApp) -> std::io::Result<bool> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let Some(command) = app.uninstall_command.as_deref() else { return Ok(false) };
    // 卸载命令本身带引号和参数。用 /S /C 加一层外引号原样交给 cmd，不让标准库再转义一遍
    std::process::Command::new("cmd")
        .args(["/S", "/C"])
        .raw_arg(format!("\"{command}\""))
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()?;
    Ok(true)
}

#[cfg(not(windows))]
pub fn run_uninstaller(_app: &InstalledApp) -> std::io::Result<bool> {
    Ok(false)
}

/// 从版权声明里取出公司或作者的名字："Copyright © 2015–2024 Microsoft Corporation. All rights reserved." → "Microsoft Corporation"。
pub fn publisher_from_copyright(text: &str) -> String {
    use regex::Regex;
    use std::sync::OnceLock;
    static NOISE: OnceLock<Regex> = OnceLock::new();
    let noise = NOISE.get_or_init(|| {
        Regex::new(r"(?i)copyright|\(c\)|©|all rights reserved\.?|版权所有|保留所有权利。?|\d{4}(\s*[-–—~]\s*\d{4})?").unwrap()
    });
    let cleaned = noise.replace_all(text, " ");
    let first_line = cleaned.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or("");
    first_line
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|c: char| ",.;:-–—".contains(c) || c.is_whitespace())
        .to_string()
}

/// 没有版权声明时，从 "com.tencent.xinWeChat" 这样的标识里猜："Tencent"。
pub fn publisher_from_bundle_id(id: &str) -> String {
    let mut parts = id.split('.');
    let (Some(_tld), Some(company)) = (parts.next(), parts.next()) else { return String::new() };
    let mut chars = company.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// .desktop 文件里用得到的字段。
#[derive(Debug, Default, PartialEq)]
pub struct DesktopEntry {
    pub name: String,
    pub icon: String,
    pub exec: String,
}

/// 解析 .desktop 文件。不是应用、被隐藏或没有名字的返回 `None`。
pub fn parse_desktop(content: &str) -> Option<DesktopEntry> {
    let mut entry = DesktopEntry::default();
    let mut in_main = false;
    let mut is_app = false;
    for line in content.lines().map(str::trim) {
        if line.starts_with('[') {
            in_main = line == "[Desktop Entry]";
            continue;
        }
        if !in_main || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let value = value.trim();
        match key.trim() {
            "Type" => is_app = value == "Application",
            "Name" => entry.name = value.to_string(),
            "Icon" => entry.icon = value.to_string(),
            "Exec" => entry.exec = value.to_string(),
            "NoDisplay" | "Hidden" if value.eq_ignore_ascii_case("true") => return None,
            _ => {}
        }
    }
    (is_app && !entry.name.is_empty()).then_some(entry)
}

/// .app 的图标文件：Info.plist 里写的 CFBundleIconFile，没写的在资源目录里找一个 .icns。
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn bundle_icon(bundle: &Path, declared: Option<&str>) -> Option<PathBuf> {
    let resources = bundle.join("Contents/Resources");
    let declared = declared.map(|file| {
        let path = resources.join(file);
        if path.extension().is_some() { path } else { path.with_extension("icns") }
    });
    declared.filter(|path| path.is_file()).or_else(|| {
        std::fs::read_dir(&resources).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|ext| ext == "icns"))
    })
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;

    pub fn list() -> Vec<InstalledApp> {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let mut roots = vec![
            (PathBuf::from("/Applications"), "applications"),
            (PathBuf::from("/System/Applications"), "system"),
        ];
        if let Some(home) = home {
            roots.push((home.join("Applications"), "user"));
        }
        let mut apps = Vec::new();
        for (root, source) in roots {
            // 应用常被收在子文件夹里（Utilities、厂商文件夹），往下多看两层；.app 里面不再深入
            let mut walker = walkdir::WalkDir::new(&root).max_depth(3).into_iter();
            while let Some(entry) = walker.next() {
                let Ok(entry) = entry else { continue };
                // 用 path().is_dir() 而不是 file_type()：Safari 这类应用在 /Applications 里只是个符号链接
                if entry.path().extension().is_some_and(|ext| ext == "app") && entry.path().is_dir() {
                    if entry.file_type().is_dir() {
                        walker.skip_current_dir();
                    }
                    if let Some(app) = read_bundle(entry.path(), source) {
                        apps.push(app);
                    }
                }
            }
        }
        apps
    }

    fn read_bundle(bundle: &Path, source: &str) -> Option<InstalledApp> {
        let contents = bundle.join("Contents");
        let info = plist::Value::from_file(contents.join("Info.plist")).ok()?.into_dictionary()?;
        let text = |key: &str| info.get(key).and_then(|v| v.as_string()).map(str::trim).filter(|s| !s.is_empty());

        let name = bundle.file_stem()?.to_string_lossy().to_string();
        let version = text("CFBundleShortVersionString").or(text("CFBundleVersion")).unwrap_or("").to_string();
        let mut publisher = text("NSHumanReadableCopyright").map(publisher_from_copyright).unwrap_or_default();
        if publisher.is_empty() {
            publisher = text("CFBundleIdentifier").map(publisher_from_bundle_id).unwrap_or_default();
        }
        let installed_at = std::fs::metadata(bundle)
            .and_then(|meta| meta.created())
            .map(|time| chrono::DateTime::<chrono::Local>::from(time).to_rfc3339_opts(chrono::SecondsFormat::Secs, false))
            .unwrap_or_default();

        let icon_source = super::bundle_icon(bundle, text("CFBundleIconFile"));

        // 系统自带的应用受系统保护，删不掉。有的看起来在 /Applications，实际链接到系统分区
        let protected = std::fs::canonicalize(bundle).unwrap_or_else(|_| bundle.to_path_buf()).starts_with("/System");

        Some(InstalledApp {
            id: bundle.to_string_lossy().to_string(),
            name,
            version,
            publisher,
            installed_at,
            size: None,
            source: if protected { "system" } else { source }.to_string(),
            location: bundle.to_string_lossy().to_string(),
            uninstallable: !protected,
            icon: None,
            icon_source,
            uninstall_command: None,
            install_dir: Some(bundle.to_path_buf()),
            bundle_id: text("CFBundleIdentifier").map(str::to_string),
        })
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::{
        core::PCWSTR,
        Win32::System::{
            ApplicationInstallationAndServicing::{MsiGetProductInfoW, INSTALLPROPERTY_LOCALPACKAGE, INSTALLPROPERTY_PRODUCTICON},
            Environment::ExpandEnvironmentStringsW,
        },
    };
    use winreg::{enums::*, RegKey};

    const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
    const UNINSTALL_32: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";

    pub fn list(icon_cache: &Path) -> Vec<InstalledApp> {
        let mut apps = Vec::new();
        let hives = [
            (HKEY_LOCAL_MACHINE, "HKLM", UNINSTALL),
            (HKEY_LOCAL_MACHINE, "HKLM", UNINSTALL_32),
            (HKEY_CURRENT_USER, "HKCU", UNINSTALL),
        ];
        for (hive, hive_name, path) in hives {
            let Ok(root) = RegKey::predef(hive).open_subkey(path) else { continue };
            for key_name in root.enum_keys().flatten() {
                let Ok(key) = root.open_subkey(&key_name) else { continue };
                if let Some(app) = read_key(&key, &format!(r"{hive_name}\{path}\{key_name}"), &key_name, icon_cache) {
                    apps.push(app);
                }
            }
        }
        // 同一个软件可能在 64 位和 32 位两处都登记了
        apps.sort_by(|a, b| (a.name.to_lowercase(), &a.version).cmp(&(b.name.to_lowercase(), &b.version)));
        apps.dedup_by(|duplicate, first| {
            let same = duplicate.name.eq_ignore_ascii_case(&first.name) && duplicate.version == first.version;
            if same && first.icon.is_none() {
                first.icon = duplicate.icon.take();
            }
            same
        });
        apps
    }

    fn read_key(key: &RegKey, id: &str, key_name: &str, icon_cache: &Path) -> Option<InstalledApp> {
        let text = |name: &str| key.get_value::<String, _>(name).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let number = |name: &str| key.get_value::<u32, _>(name).ok();

        let name = text("DisplayName")?;
        // 系统组件、补丁和子组件不算软件
        if number("SystemComponent") == Some(1) || text("ParentKeyName").is_some() {
            return None;
        }
        if text("ReleaseType").is_some_and(|kind| kind.contains("Update") || kind.contains("Hotfix")) {
            return None;
        }

        let uninstall_command = text("UninstallString");
        let install_dir = text("InstallLocation").and_then(|dir| expand_path(dir.trim_matches('"')));
        let display_icon = text("DisplayIcon").and_then(|raw| display_icon_path(&raw)).map(|(path, index)| {
            let path = match &install_dir {
                Some(dir) if path.is_relative() => dir.join(path),
                _ => path,
            };
            (path, index)
        });
        let msi_product = (key_name.starts_with('{') && key_name.ends_with('}') && uuid::Uuid::parse_str(key_name).is_ok())
            .then(|| key_name.encode_utf16().chain(Some(0)).collect::<Vec<_>>());
        let icon = display_icon.as_ref().and_then(|(path, index)| icon_for_index(path, icon_cache, *index))
            .or_else(|| msi_product_path(msi_product.as_deref()?, INSTALLPROPERTY_PRODUCTICON)
                .and_then(|path| icon_for(&path, icon_cache)))
            .or_else(|| msi_product_path(msi_product.as_deref()?, INSTALLPROPERTY_LOCALPACKAGE)
                .and_then(|path| icon_for(&path, icon_cache)))
            .or_else(|| main_exes(install_dir.as_deref()?, &name).into_iter().find_map(|path| icon_for(&path, icon_cache)))
            .or_else(|| {
                let path = uninstall_command.as_deref().and_then(exe_in_command)?;
                if path.file_name()?.eq_ignore_ascii_case("msiexec.exe") {
                    return None;
                }
                icon_for(&path, icon_cache)
            });

        let location = install_dir
            .clone()
            .filter(|dir| dir.is_dir())
            .or_else(|| display_icon.as_ref().and_then(|(path, _)| path.parent()).map(Path::to_path_buf))
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default();

        // InstallDate 是 "20241130" 这样的八位数字
        let installed_at = text("InstallDate")
            .filter(|date| date.len() == 8 && date.chars().all(|c| c.is_ascii_digit()))
            .map(|date| format!("{}-{}-{}T00:00:00", &date[..4], &date[4..6], &date[6..]))
            .unwrap_or_default();

        Some(InstalledApp {
            id: id.to_string(),
            name,
            version: text("DisplayVersion").unwrap_or_default(),
            publisher: text("Publisher").unwrap_or_default(),
            installed_at,
            // EstimatedSize 的单位是 KB
            size: number("EstimatedSize").map(|kb| u64::from(kb) * 1024),
            source: "registry".to_string(),
            location,
            uninstallable: uninstall_command.is_some(),
            icon,
            icon_source: None,
            uninstall_command,
            // 只认登记的安装位置；从图标路径推出来的文件夹可能是安装程序的缓存，不拿来统计
            install_dir: install_dir.filter(|dir| dir.is_dir()),
            bundle_id: None,
        })
    }

    fn display_icon_path(raw: &str) -> Option<(PathBuf, i32)> {
        let raw = raw.trim().trim_start_matches('@');
        let (path, index) = raw.rsplit_once(',')
            .and_then(|(path, index)| Some((path, index.trim().trim_end_matches('"').parse::<i32>().ok()?)))
            .unwrap_or((raw, 0));
        Some((expand_path(path.trim().trim_matches('"'))?, index))
    }

    fn expand_path(path: &str) -> Option<PathBuf> {
        if !path.contains('%') {
            return Some(PathBuf::from(path));
        }
        let input: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
        // input 以 NUL 结尾。先查询所需长度，再写入足够长的缓冲区。
        let size = unsafe { ExpandEnvironmentStringsW(input.as_ptr(), std::ptr::null_mut(), 0) };
        if size == 0 {
            return None;
        }
        let mut expanded = vec![0; size as usize];
        let used = unsafe { ExpandEnvironmentStringsW(input.as_ptr(), expanded.as_mut_ptr(), size) };
        (used > 0 && used <= size).then(|| std::ffi::OsString::from_wide(&expanded[..used as usize - 1]).into())
    }

    fn msi_product_path(product: &[u16], property: PCWSTR) -> Option<PathBuf> {
        let mut value = vec![0u16; 260];
        let mut size = value.len() as u32;
        // 产品代码与属性都是 NUL 结尾的 UTF-16，size 是输出缓冲区的字符数。
        let result = unsafe { MsiGetProductInfoW(product.as_ptr(), property, value.as_mut_ptr(), &mut size) };
        let result = if result == 234 {
            size += 1;
            value.resize(size as usize, 0);
            unsafe { MsiGetProductInfoW(product.as_ptr(), property, value.as_mut_ptr(), &mut size) }
        } else {
            result
        };
        (result == 0 && size > 0).then(|| std::ffi::OsString::from_wide(&value[..size as usize]).into())
    }

    /// 安装目录里最像主程序的 exe：名字和软件名对得上的优先，然后按大小排列。
    fn main_exes(dir: &Path, name: &str) -> Vec<PathBuf> {
        let wanted = crate::identify::normalize_name(name);
        let mut exes: Vec<_> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("exe")))
            .filter_map(|path| {
                let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_lowercase();
                if stem.starts_with("unins") || stem.contains("uninstall") || stem.contains("update") {
                    return None;
                }
                let stem = crate::identify::normalize_name(&stem);
                let matches = !stem.is_empty() && (wanted.contains(&stem) || stem.contains(&wanted));
                Some((!matches, std::cmp::Reverse(std::fs::metadata(&path).ok()?.len()), path))
            })
            .collect();
        exes.sort();
        exes.into_iter().map(|(_, _, path)| path).collect()
    }

    /// 卸载命令里的程序路径：`"C:\App\unins000.exe" /SILENT` → `C:\App\unins000.exe`。
    fn exe_in_command(command: &str) -> Option<PathBuf> {
        let command = command.trim();
        let path = match command.strip_prefix('"') {
            Some(rest) => rest.split('"').next()?,
            None => {
                let end = command.to_ascii_lowercase().find(".exe")? + 4;
                &command[..end]
            }
        };
        expand_path(path)
    }

    #[cfg(test)]
    #[test]
    fn parses_display_icon_paths_and_resource_ids() {
        assert_eq!(display_icon_path(r#""C:\Apps\name, tool.dll", -123"#), Some((PathBuf::from(r"C:\Apps\name, tool.dll"), -123)));
        assert_eq!(display_icon_path(r#""C:\Apps\tool.exe,2""#), Some((PathBuf::from(r"C:\Apps\tool.exe"), 2)));
        assert_eq!(display_icon_path(r"@%SystemRoot%\explorer.exe,0"), Some((PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("explorer.exe"), 0)));
    }
}

/// Linux：从一组放 .desktop 文件的文件夹里列出应用。
///
/// 这段只用到标准库，所以不按系统区分编译，在任何系统上都能测试。
pub fn desktop_apps(roots: &[(PathBuf, &str)], icon_dirs: &[PathBuf]) -> Vec<InstalledApp> {
    let mut apps = Vec::new();
    for (root, source) in roots {
        for entry in std::fs::read_dir(root).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "desktop") {
                continue;
            }
            let Some(desktop) = std::fs::read_to_string(&path).ok().and_then(|text| parse_desktop(&text)) else {
                continue;
            };
            let installed_at = entry
                .metadata()
                .and_then(|meta| meta.modified())
                .map(|time| chrono::DateTime::<chrono::Local>::from(time).to_rfc3339_opts(chrono::SecondsFormat::Secs, false))
                .unwrap_or_default();
            apps.push(InstalledApp {
                id: path.to_string_lossy().to_string(),
                name: desktop.name,
                version: String::new(),
                publisher: String::new(),
                installed_at,
                size: None,
                source: source.to_string(),
                location: path.to_string_lossy().to_string(),
                // 卸载要走各发行版自己的包管理器，这里不代劳
                uninstallable: false,
                icon: None,
                icon_source: find_desktop_icon(&desktop.icon, icon_dirs),
                uninstall_command: None,
                install_dir: None,
                bundle_id: None,
            });
        }
    }
    apps
}

/// Icon= 可能是绝对路径，也可能只是图标主题里的名字。
/// `icon_dirs` 里是图标主题的根目录（下面有 256x256/apps 这样的子目录）和直接放图标的目录（pixmaps）。
pub(crate) fn find_desktop_icon(icon: &str, icon_dirs: &[PathBuf]) -> Option<PathBuf> {
    if icon.is_empty() {
        return None;
    }
    let direct = PathBuf::from(icon);
    if direct.is_absolute() {
        return direct.is_file().then_some(direct);
    }
    for dir in icon_dirs {
        for size in ["256x256", "128x128", "scalable", "96x96", "64x64", "48x48"] {
            for ext in ["png", "svg"] {
                let path = dir.join(size).join("apps").join(format!("{icon}.{ext}"));
                if path.is_file() {
                    return Some(path);
                }
            }
        }
        if let Some(found) = ["png", "svg"].iter().map(|ext| dir.join(format!("{icon}.{ext}"))).find(|p| p.is_file()) {
            return Some(found);
        }
    }
    None
}

#[cfg(all(unix, not(target_os = "macos")))]
mod platform {
    use super::*;

    pub fn list() -> Vec<InstalledApp> {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let roots = [
            (PathBuf::from("/usr/share/applications"), "desktop"),
            (PathBuf::from("/usr/local/share/applications"), "desktop"),
            (home.join(".local/share/applications"), "desktop"),
            (PathBuf::from("/var/lib/flatpak/exports/share/applications"), "flatpak"),
            (home.join(".local/share/flatpak/exports/share/applications"), "flatpak"),
            (PathBuf::from("/var/lib/snapd/desktop/applications"), "snap"),
        ];
        let icon_dirs = [
            PathBuf::from("/usr/share/icons/hicolor"),
            home.join(".local/share/icons/hicolor"),
            PathBuf::from("/var/lib/flatpak/exports/share/icons/hicolor"),
            home.join(".local/share/flatpak/exports/share/icons/hicolor"),
            PathBuf::from("/usr/share/pixmaps"),
        ];
        desktop_apps(&roots, &icon_dirs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, dir: Option<&str>, size: Option<u64>) -> InstalledApp {
        InstalledApp {
            id: id.into(),
            name: id.into(),
            version: String::new(),
            publisher: String::new(),
            installed_at: String::new(),
            size,
            source: "registry".into(),
            location: dir.unwrap_or_default().into(),
            uninstallable: true,
            icon: None,
            icon_source: None,
            uninstall_command: None,
            install_dir: dir.map(PathBuf::from),
            bundle_id: None,
        }
    }

    fn windows_like() -> Protected {
        let keys = |list: &[&str]| list.iter().map(|p| path_key(Path::new(p), true)).collect();
        Protected {
            exact: keys(&[r"C:\Program Files", r"C:\Program Files (x86)", r"C:\ProgramData", r"C:\Users\me", r"C:\Users\me\AppData\Local\Programs"]),
            under: keys(&[r"C:\Windows", r"C:\ProgramData\Package Cache"]),
        }
    }

    #[test]
    fn only_own_folders_are_scanned() {
        let apps = vec![
            app("edge", Some(r"C:\Program Files (x86)\Microsoft\Edge\Application\"), Some(3_500)),
            app("root", Some("C:\\"), Some(10)),
            app("pf", Some(r"c:\program files"), Some(20)),
            app("win", Some(r"C:\Windows\System32\Foo"), Some(30)),
            app("cache", Some(r"C:\ProgramData\Package Cache\{abc}"), Some(40)),
            app("twin-a", Some(r"D:\Tools\Shared"), Some(50)),
            app("twin-b", Some(r"d:\tools\shared\"), Some(60)),
            app("vendor", Some(r"C:\Program Files\Adobe"), Some(70)),
            app("photoshop", Some(r"C:\Program Files\Adobe\Photoshop"), None),
            app("user", Some(r"C:\Users\me\AppData\Local\Programs\Code"), None),
            app("no-dir", None, Some(80)),
            app("nothing", None, None),
        ];
        let got: HashMap<String, SizeSource> = size_sources_with(&apps, &windows_like(), true).into_iter().collect();
        let scanned = |id: &str| matches!(got[id], SizeSource::Scan(_));
        assert!(scanned("edge") && scanned("photoshop") && scanned("user"));
        assert_eq!(got["root"], SizeSource::Registered(10), "整个盘");
        assert_eq!(got["pf"], SizeSource::Registered(20), "Program Files 本身，大小写不同也认得");
        assert_eq!(got["win"], SizeSource::Registered(30), "Windows 文件夹下面的一切");
        assert_eq!(got["cache"], SizeSource::Registered(40), "安装程序缓存");
        assert_eq!(got["twin-a"], SizeSource::Registered(50), "两个软件登记在同一个文件夹");
        assert_eq!(got["twin-b"], SizeSource::Registered(60));
        assert_eq!(got["vendor"], SizeSource::Registered(70), "里面套着别的软件的文件夹");
        assert_eq!(got["no-dir"], SizeSource::Registered(80));
        assert_eq!(got["nothing"], SizeSource::Unknown);
    }

    /// 在这台电脑上实际统计一遍，和登记的大小对照：
    /// `cargo test -p shibapack-core measures_real_applications -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn measures_real_applications() {
        let dir = tempfile::tempdir().unwrap();
        let apps = list_installed(dir.path());
        let mut scanned = 0;
        for (id, source) in size_sources(&apps) {
            let app = apps.iter().find(|a| a.id == id).unwrap();
            let registered = app.size.map(|s| format!("{:.1} MB", s as f64 / 1e6)).unwrap_or_else(|| "-".into());
            match source {
                SizeSource::Scan(path) => {
                    scanned += 1;
                    let real = folder_size(&path).map(|s| format!("{:.1} MB", s as f64 / 1e6)).unwrap_or_else(|| "（文件太多）".into());
                    eprintln!("{:<40} 登记 {registered:>12}  实际 {real:>12}  {}", app.name, path.display());
                }
                other => eprintln!("{:<40} 登记 {registered:>12}  （不统计：{other:?}）", app.name),
            }
        }
        eprintln!("共 {} 个软件，{scanned} 个按文件夹统计", apps.len());
    }

    #[test]
    fn folder_size_adds_up_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("a/b")).unwrap();
        std::fs::write(dir.path().join("x.bin"), vec![0u8; 1000]).unwrap();
        std::fs::write(dir.path().join("a/b/y.bin"), vec![0u8; 234]).unwrap();
        assert_eq!(folder_size(dir.path()), Some(1234));
        assert_eq!(folder_size(&dir.path().join("missing")), None);
        #[cfg(unix)]
        {
            // 符号链接指向的东西不算进来
            let outside = tempfile::tempdir().unwrap();
            std::fs::write(outside.path().join("big.bin"), vec![0u8; 5000]).unwrap();
            std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
            assert_eq!(folder_size(dir.path()), Some(1234));
        }
    }

    #[test]
    fn publisher_guessing() {
        assert_eq!(
            publisher_from_copyright("Copyright © 2015–2024 Microsoft Corporation. All rights reserved."),
            "Microsoft Corporation"
        );
        assert_eq!(publisher_from_copyright("© 2024 Anthropic, PBC"), "Anthropic, PBC");
        assert_eq!(publisher_from_copyright("Copyright (C) 2010-2025 JetBrains s.r.o."), "JetBrains s.r.o");
        assert_eq!(publisher_from_copyright("版权所有 © 2024 腾讯公司。保留所有权利。"), "腾讯公司。");
        assert_eq!(publisher_from_copyright("2024"), "");
        assert_eq!(publisher_from_bundle_id("com.tencent.xinWeChat"), "Tencent");
        assert_eq!(publisher_from_bundle_id("single"), "");
    }

    #[test]
    fn desktop_entries() {
        let text = "[Desktop Entry]\nType=Application\nName=Firefox\nName[zh_CN]=火狐\nExec=firefox %u\nIcon=firefox\n\n[Desktop Action new-window]\nName=New Window\n";
        assert_eq!(
            parse_desktop(text),
            Some(DesktopEntry { name: "Firefox".into(), icon: "firefox".into(), exec: "firefox %u".into() })
        );
        assert!(parse_desktop("[Desktop Entry]\nType=Application\nName=Hidden\nNoDisplay=true\n").is_none());
        assert!(parse_desktop("[Desktop Entry]\nType=Link\nName=A link\n").is_none());
        assert!(parse_desktop("[Desktop Entry]\nType=Application\n").is_none());
    }

    #[test]
    fn linux_desktop_folder() {
        let dir = tempfile::tempdir().unwrap();
        let apps_dir = dir.path().join("applications");
        let theme = dir.path().join("hicolor");
        let pixmaps = dir.path().join("pixmaps");
        std::fs::create_dir_all(&apps_dir).unwrap();
        std::fs::create_dir_all(theme.join("128x128/apps")).unwrap();
        std::fs::create_dir_all(&pixmaps).unwrap();
        std::fs::write(theme.join("128x128/apps/firefox.png"), b"\x89PNG\r\n\x1a\n").unwrap();
        std::fs::write(pixmaps.join("gimp.svg"), b"<svg/>").unwrap();
        let entry = |name: &str, icon: &str| format!("[Desktop Entry]\nType=Application\nName={name}\nIcon={icon}\n");
        std::fs::write(apps_dir.join("firefox.desktop"), entry("Firefox", "firefox")).unwrap();
        std::fs::write(apps_dir.join("gimp.desktop"), entry("GIMP", "gimp")).unwrap();
        std::fs::write(apps_dir.join("plain.desktop"), entry("No Icon", "missing-icon")).unwrap();
        std::fs::write(apps_dir.join("hidden.desktop"), "[Desktop Entry]\nType=Application\nName=X\nNoDisplay=true\n").unwrap();
        std::fs::write(apps_dir.join("readme.txt"), "not a desktop file").unwrap();

        let mut apps = desktop_apps(&[(apps_dir, "flatpak")], &[theme.clone(), pixmaps.clone()]);
        apps.sort_by(|a, b| a.name.cmp(&b.name));
        let names: Vec<_> = apps.iter().map(|app| app.name.as_str()).collect();
        assert_eq!(names, ["Firefox", "GIMP", "No Icon"]);
        assert_eq!(apps[0].icon_source, Some(theme.join("128x128/apps/firefox.png")));
        assert_eq!(apps[1].icon_source, Some(pixmaps.join("gimp.svg")));
        assert_eq!(apps[2].icon_source, None);
        assert!(apps.iter().all(|app| app.source == "flatpak" && !app.uninstallable));
    }

    /// 在真实系统上跑一遍：能列出东西、字段齐全、图标能取出来。
    #[test]
    #[cfg(target_os = "macos")]
    fn lists_real_applications() {
        let cache = tempfile::tempdir().unwrap();
        let apps = list_installed(cache.path());
        assert!(!apps.is_empty(), "这台 Mac 上应当至少有系统自带的应用");
        let safari = apps.iter().find(|app| app.name == "Safari").expect("应当能找到 Safari");
        assert!(!safari.version.is_empty());
        assert!(safari.publisher.starts_with("Apple"), "{}", safari.publisher);
        assert!(!safari.uninstallable, "系统自带的应用不应当可卸载");
        assert!(!apps.iter().any(|app| app.name.is_empty() || app.location.is_empty()));

        let with_icon = apps.iter().filter(|app| app.icon.is_some()).count();
        assert!(with_icon * 2 > apps.len(), "多数应用应当能取到图标：{with_icon}/{}", apps.len());
        for app in apps.iter().filter_map(|app| app.icon.as_deref()).take(5) {
            assert!(std::fs::read(app).unwrap().starts_with(b"\x89PNG"));
        }
        eprintln!("共 {} 个应用，{with_icon} 个有图标", apps.len());
    }
}
