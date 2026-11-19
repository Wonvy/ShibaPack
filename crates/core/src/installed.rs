//! 这台电脑上已经安装的软件。
//!
//! - macOS：扫描“应用程序”文件夹里的 .app，读 Info.plist。
//! - Windows：读注册表的卸载列表。
//! - Linux：读各处的 .desktop 文件（拿不到版本号）。

use crate::icons;
use serde::Serialize;
use sha2::{Digest, Sha256};
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
    /// 占用空间。macOS 上要另外统计（见 `measure`），列出来时先是 `None`。
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
}

/// 列出已安装的软件，按名称排序。图标取出来放在 `icon_cache` 里，下次直接用。
pub fn list_installed(icon_cache: &Path) -> Vec<InstalledApp> {
    let mut apps = platform::list();
    for app in &mut apps {
        app.icon = app.icon_source.as_deref().and_then(|source| icon_for(source, icon_cache));
    }
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps.dedup_by(|a, b| a.id == b.id);
    apps
}

/// 图标按“来源路径 + 修改时间”缓存：软件升级后图标文件变了会重新取。
fn icon_for(source: &Path, cache: &Path) -> Option<String> {
    let meta = std::fs::metadata(source).ok()?;
    let modified = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    let key = hex::encode(Sha256::digest(format!("{}|{modified}|{}", source.display(), meta.len())));
    let stem = &key[..20];
    let path = icons::cached_icon(cache, stem).or_else(|| icons::extract_icon(source, cache, stem))?;
    Some(path.to_string_lossy().to_string())
}

/// 统计一个软件占用的空间。只对 macOS 的 .app 有意义，别的系统直接用登记的大小。
pub fn measure(app: &InstalledApp) -> Option<u64> {
    if app.size.is_some() {
        return app.size;
    }
    let path = Path::new(&app.location);
    if !path.is_dir() || path.extension().is_none_or(|ext| ext != "app") {
        return None;
    }
    let total = walkdir::WalkDir::new(path)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.metadata().ok())
        .filter(|meta| meta.is_file())
        .map(|meta| meta.len())
        .sum();
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

        let resources = contents.join("Resources");
        let declared = text("CFBundleIconFile").map(|file| {
            let path = resources.join(file);
            if path.extension().is_some() { path } else { path.with_extension("icns") }
        });
        let icon_source = declared.filter(|path| path.is_file()).or_else(|| {
            // 没写图标文件名的，在资源目录里找一个 .icns
            std::fs::read_dir(&resources).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|ext| ext == "icns"))
        });

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
        })
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use winreg::{enums::*, RegKey};

    const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
    const UNINSTALL_32: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";

    pub fn list() -> Vec<InstalledApp> {
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
                if let Some(app) = read_key(&key, &format!(r"{hive_name}\{path}\{key_name}")) {
                    apps.push(app);
                }
            }
        }
        // 同一个软件可能在 64 位和 32 位两处都登记了
        apps.sort_by(|a, b| (a.name.to_lowercase(), &a.version).cmp(&(b.name.to_lowercase(), &b.version)));
        apps.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name) && a.version == b.version);
        apps
    }

    fn read_key(key: &RegKey, id: &str) -> Option<InstalledApp> {
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
        let install_dir = text("InstallLocation").map(|dir| PathBuf::from(dir.trim_matches('"')));
        // DisplayIcon 形如 "C:\App\app.exe,0"：去掉引号和后面的图标序号
        let display_icon = text("DisplayIcon").map(|raw| {
            let raw = raw.trim_matches('"');
            let path = match raw.rsplit_once(',') {
                Some((path, index)) if index.trim().trim_start_matches('-').chars().all(|c| c.is_ascii_digit()) => path,
                _ => raw,
            };
            PathBuf::from(path.trim().trim_matches('"'))
        });
        let icon_source = display_icon
            .clone()
            .filter(|path| path.is_file())
            .or_else(|| install_dir.as_deref().and_then(|dir| main_exe(dir, &name)))
            .or_else(|| uninstall_command.as_deref().and_then(exe_in_command).filter(|path| path.is_file()));

        let location = install_dir
            .clone()
            .filter(|dir| dir.is_dir())
            .or_else(|| display_icon.as_deref().and_then(Path::parent).map(Path::to_path_buf))
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
            icon: None,
            icon_source,
            uninstall_command,
        })
    }

    /// 安装目录里最像主程序的 exe：名字和软件名对得上的优先，否则取最大的。
    fn main_exe(dir: &Path, name: &str) -> Option<PathBuf> {
        let wanted = crate::identify::normalize_name(name);
        let mut exes: Vec<(PathBuf, u64)> = std::fs::read_dir(dir)
            .ok()?
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("exe")))
            .filter(|path| {
                let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_lowercase();
                !stem.starts_with("unins") && !stem.contains("uninstall") && !stem.contains("update")
            })
            .filter_map(|path| std::fs::metadata(&path).ok().map(|meta| (path, meta.len())))
            .collect();
        exes.sort_by(|a, b| b.1.cmp(&a.1));
        let by_name = exes.iter().find(|(path, _)| {
            let stem = crate::identify::normalize_name(&path.file_stem().unwrap_or_default().to_string_lossy());
            !stem.is_empty() && (wanted.contains(&stem) || stem.contains(&wanted))
        });
        by_name.or(exes.first()).map(|(path, _)| path.clone())
    }

    /// 卸载命令里的程序路径：`"C:\App\unins000.exe" /SILENT` → `C:\App\unins000.exe`。
    fn exe_in_command(command: &str) -> Option<PathBuf> {
        let command = command.trim();
        let path = match command.strip_prefix('"') {
            Some(rest) => rest.split('"').next()?,
            None => {
                let end = command.to_lowercase().find(".exe")? + 4;
                &command[..end]
            }
        };
        Some(PathBuf::from(path))
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
            });
        }
    }
    apps
}

/// Icon= 可能是绝对路径，也可能只是图标主题里的名字。
/// `icon_dirs` 里是图标主题的根目录（下面有 256x256/apps 这样的子目录）和直接放图标的目录（pixmaps）。
fn find_desktop_icon(icon: &str, icon_dirs: &[PathBuf]) -> Option<PathBuf> {
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
