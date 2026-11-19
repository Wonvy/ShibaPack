//! 从文件名识别软件名、版本、平台和架构。
//!
//! 只看文件名，不读文件内容，所以结果是“猜”的，界面上都能手动改。

use crate::model::Platform;
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq)]
pub struct Identity {
    pub name: String,
    pub version: String,
    pub platform: Platform,
    pub arch: String,
}

/// 由两段组成的扩展名。长的排前面。
const MULTI_EXTS: &[&str] = &["pkg.tar.zst", "tar.gz", "tar.xz", "tar.bz2", "tar.zst"];

/// 扫描文件夹时认作安装包的扩展名。
const INSTALLER_EXTS: &[&str] = &[
    "exe", "msi", "msix", "msixbundle", "appx", "appxbundle", "dmg", "pkg", "mpkg", "deb", "rpm", "appimage",
    "flatpak", "snap", "run", "pkg.tar.zst", "apk", "xapk", "apks", "apkm", "ipa", "zip", "7z", "rar", "tar.gz",
    "tgz", "tar.xz", "tar.bz2", "tar.zst", "iso", "jar",
];

/// 排在软件名后面、不属于名字的词。
const NOISE: &[&str] = &[
    "setup", "installer", "install", "portable", "x64", "x86", "amd64", "arm64", "aarch64", "i386", "i686", "ia32",
    "64", "32", "bit", "64bit", "32bit", "win", "win32", "win64", "windows", "mac", "macos", "osx", "darwin",
    "linux", "android", "universal", "release", "final", "full", "offline", "online", "latest", "stable", "signed",
    "user", "system", "multi", "en", "us", "zh", "cn", "bin", "binary",
];

/// 把文件名拆成主干和扩展名（小写，不带点）。
pub fn split_ext(file_name: &str) -> (String, String) {
    for ext in MULTI_EXTS {
        let tail_len = ext.len() + 1;
        if file_name.len() > tail_len && file_name.is_char_boundary(file_name.len() - tail_len) {
            let (stem, tail) = file_name.split_at(file_name.len() - tail_len);
            if tail.starts_with('.') && tail[1..].eq_ignore_ascii_case(ext) {
                return (stem.to_string(), ext.to_string());
            }
        }
    }
    if let Some(dot) = file_name.rfind('.') {
        let ext = &file_name[dot + 1..];
        if dot > 0 && !ext.is_empty() && ext.len() <= 10 && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
            return (file_name[..dot].to_string(), ext.to_ascii_lowercase());
        }
    }
    (file_name.to_string(), String::new())
}

/// 扫描文件夹时是否把这个文件当作安装包。
pub fn is_installer(file_name: &str) -> bool {
    let (_, ext) = split_ext(file_name);
    INSTALLER_EXTS.contains(&ext.as_str())
}

fn platform_from_ext(ext: &str) -> Option<Platform> {
    Some(match ext {
        "exe" | "msi" | "msix" | "msixbundle" | "appx" | "appxbundle" => Platform::Windows,
        "dmg" | "pkg" | "mpkg" => Platform::Macos,
        "deb" | "rpm" | "appimage" | "flatpak" | "snap" | "run" | "pkg.tar.zst" => Platform::Linux,
        "apk" | "xapk" | "apks" | "apkm" => Platform::Android,
        "ipa" => Platform::Ios,
        _ => return None,
    })
}

fn arch_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)x86[_-]64|x64|x86|amd64|arm64|aarch64|win64|win32|64[-_ ]?bit|32[-_ ]?bit|i[36]86|ia32")
            .unwrap()
    })
}

fn version_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)v?(\d+(?:\.\d+){1,4}(?:[-_.]?(?:alpha|beta|rc|preview|pre|dev)[-_.]?\d*)?)").unwrap()
    })
}

fn build_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)build[\s_\-.]?(\d{3,})").unwrap())
}

fn token_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"([\s_\-.+()\[\]]*)([^\s_\-.+()\[\]]+)").unwrap())
}

fn attached_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^(.{2,}?)[\s_\-.]*(?:user|system)?(?:setup|installer)$").unwrap())
}

/// 从文件名识别。识别不出的字段留空（平台回落为跨平台）。
pub fn identify(file_name: &str) -> Identity {
    let (stem, ext) = split_ext(file_name);

    // 先把 x64、win32 这类词涂掉再找版本号，免得把 "x64.5.9.0" 认成 64.5.9.0。
    // 涂掉的长度不变，所以位置和原来的主干对得上。
    let blanked = arch_re().replace_all(&stem, |caps: &regex::Captures| "_".repeat(caps[0].len())).to_string();

    let mut version = String::new();
    let mut candidate = stem.clone();
    if let Some(caps) = version_re().captures(&blanked) {
        let whole = caps.get(0).unwrap();
        let digits = caps.get(1).unwrap();
        version = digits.as_str().to_string();
        // "mpv0.39.0" 里的 v 是名字的一部分，不是版本前缀
        let v_belongs_to_name = whole.start() < digits.start()
            && stem[..whole.start()].chars().next_back().is_some_and(|c| c.is_alphabetic());
        let name_end = if v_belongs_to_name { digits.start() } else { whole.start() };
        let before = stem[..name_end].to_string();
        candidate = if has_word(&before) { before } else { stem[whole.end()..].to_string() };
    } else if let Some(caps) = build_re().captures(&blanked) {
        version = caps[1].to_string();
        let before = stem[..caps.get(0).unwrap().start()].to_string();
        if has_word(&before) {
            candidate = before;
        }
    }

    let mut name = clean_name(&candidate);
    if name.is_empty() {
        name = clean_name(&stem);
    }
    if name.is_empty() {
        name = stem.trim().to_string();
    }

    let tokens = tokens_of(&stem);
    let has = |words: &[&str]| tokens.iter().any(|t| words.contains(&t.as_str()));
    let arch = if has(&["arm64", "aarch64"]) {
        "arm64"
    } else if has(&["universal"]) {
        "universal"
    } else if has(&["x64", "amd64", "win64", "64bit"]) {
        "x64"
    } else if has(&["x86", "i386", "i686", "win32", "ia32", "32bit"]) {
        "x86"
    } else {
        ""
    };

    let platform = platform_from_ext(&ext).unwrap_or_else(|| {
        if has(&["windows", "win", "win64", "win32"]) {
            Platform::Windows
        } else if has(&["macos", "mac", "osx", "darwin"]) {
            Platform::Macos
        } else if has(&["linux"]) {
            Platform::Linux
        } else if has(&["android"]) {
            Platform::Android
        } else {
            Platform::Cross
        }
    });

    Identity { name, version, platform, arch: arch.to_string() }
}

fn has_word(text: &str) -> bool {
    text.chars().any(|c| c.is_alphanumeric())
}

/// 小写的词，用来判断架构和平台。"x86_64"、"64-bit" 先并成一个词。
fn tokens_of(stem: &str) -> Vec<String> {
    let lower = stem.to_lowercase();
    let lower = lower.replace("x86_64", "x64").replace("x86-64", "x64");
    let lower = lower.replace("64-bit", "64bit").replace("64 bit", "64bit");
    let lower = lower.replace("32-bit", "32bit").replace("32 bit", "32bit");
    lower.split(|c: char| !c.is_alphanumeric()).filter(|t| !t.is_empty()).map(String::from).collect()
}

/// 去掉名字后面的 setup、x64、portable 之类的词，保留名字自己的连字符和点。
fn clean_name(candidate: &str) -> String {
    let parts: Vec<(&str, &str)> = token_re()
        .captures_iter(candidate)
        .map(|c| (c.get(1).map_or("", |m| m.as_str()), c.get(2).map_or("", |m| m.as_str())))
        .collect();
    let mut end = parts.len();
    while end > 1 {
        let token = parts[end - 1].1.to_lowercase();
        // "Clash for Windows" 里的 Windows 是名字
        let after_for = parts[end - 2].1.eq_ignore_ascii_case("for");
        if NOISE.contains(&token.as_str()) && !after_for {
            end -= 1;
        } else {
            break;
        }
    }
    let mut name = String::new();
    for (i, (sep, token)) in parts[..end].iter().enumerate() {
        if i > 0 {
            name.push_str(sep);
        }
        name.push_str(token);
    }
    // "VSCodeUserSetup"、"WeChatSetup"：连在名字上的 Setup / Installer
    if let Some(caps) = attached_re().captures(&name) {
        name = caps[1].to_string();
    }
    let name = name.replace('_', " ");
    name.split_whitespace().collect::<Vec<_>>().join(" ").trim_matches(|c| "-. ".contains(c)).to_string()
}

/// 用来判断两个名字是不是同一个软件：忽略大小写、空格和标点。
pub fn normalize_name(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

/// 按数字段比较版本号："1.10" 比 "1.9" 新，"2.0" 比 "2.0-beta" 新。
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let parts = |v: &str| -> Vec<String> { v.split(['.', '-', '_', '+']).map(str::to_lowercase).collect() };
    let (pa, pb) = (parts(a), parts(b));
    for i in 0..pa.len().max(pb.len()) {
        let ordering = match (pa.get(i), pb.get(i)) {
            (Some(x), Some(y)) if x == y => continue,
            (None, Some(y)) => {
                // 多出来的是 beta、rc 这类词：短的那个才是正式版
                if y.parse::<u64>().is_ok() { Ordering::Less } else { Ordering::Greater }
            }
            (Some(x), None) => {
                if x.parse::<u64>().is_ok() { Ordering::Greater } else { Ordering::Less }
            }
            (Some(x), Some(y)) => match (x.parse::<u64>(), y.parse::<u64>()) {
                (Ok(m), Ok(n)) => m.cmp(&n),
                (Ok(_), Err(_)) => Ordering::Greater,
                (Err(_), Ok(_)) => Ordering::Less,
                (Err(_), Err(_)) => x.cmp(y),
            },
            (None, None) => Ordering::Equal,
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    Ordering::Equal
}

/// 仓库里的软件和系统里装的软件是不是同一个。
///
/// 名字一样（忽略大小写、空格、标点）算同一个；一个名字只比另一个多出
/// "Desktop"、版本号、"64-bit" 这类词也算："Docker" 和 "Docker Desktop"、"Python" 和 "Python 3.13 (64-bit)"。
/// "Visual Studio" 和 "Visual Studio Code" 不算。
pub fn same_software(a: &str, b: &str) -> bool {
    const GENERIC: &[&str] = &[
        "desktop", "app", "client", "for", "windows", "mac", "macos", "linux", "x64", "x86", "arm64", "bit", "js",
        "version", "edition", "installer", "setup", "user", "system", "the",
    ];
    let words = |name: &str| -> Vec<String> {
        name.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(String::from).collect()
    };
    let (wa, wb) = (words(a), words(b));
    if wa.is_empty() || wb.is_empty() {
        return false;
    }
    if wa.concat() == wb.concat() {
        return true;
    }
    let (short, long) = if wa.len() <= wb.len() { (&wa, &wb) } else { (&wb, &wa) };
    short.concat().chars().count() >= 2
        && long.starts_with(short)
        && long[short.len()..].iter().all(|w| GENERIC.contains(&w.as_str()) || w.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_ordering() {
        use std::cmp::Ordering::*;
        assert_eq!(compare_versions("1.10", "1.9"), Greater);
        assert_eq!(compare_versions("24.09", "26.06"), Less);
        assert_eq!(compare_versions("1.96.2", "1.96.2"), Equal);
        assert_eq!(compare_versions("2.0", "2.0-beta.1"), Greater);
        assert_eq!(compare_versions("2.0-rc.2", "2.0-rc.1"), Greater);
        assert_eq!(compare_versions("1.2", "1.2.1"), Less);
        assert_eq!(compare_versions("4189", "4200"), Less);
    }

    #[test]
    fn matching_installed_names() {
        assert!(same_software("VS Code", "vscode"));
        assert!(same_software("Docker", "Docker Desktop"));
        assert!(same_software("Python", "Python 3.13.1 (64-bit)"));
        assert!(same_software("Node.js", "Node"));
        assert!(same_software("7-Zip 24.09 (x64)", "7-Zip"));
        assert!(same_software("微信", "微信"));
        assert!(!same_software("Git", "GitHub Desktop"));
        assert!(!same_software("Visual Studio", "Visual Studio Code"));
        assert!(!same_software("", "Anything"));
    }

    fn check(file: &str, name: &str, version: &str, platform: Platform, arch: &str) {
        let id = identify(file);
        assert_eq!(
            (id.name.as_str(), id.version.as_str(), id.platform, id.arch.as_str()),
            (name, version, platform, arch),
            "文件名：{file}"
        );
    }

    #[test]
    fn common_installers() {
        check("VSCodeUserSetup-x64-1.96.2.exe", "VSCode", "1.96.2", Platform::Windows, "x64");
        check("Git-2.47.1-64-bit.exe", "Git", "2.47.1", Platform::Windows, "x64");
        check("node-v22.12.0-x64.msi", "node", "22.12.0", Platform::Windows, "x64");
        check("python-3.13.1-amd64.exe", "python", "3.13.1", Platform::Windows, "x64");
        check("Postman-win64-11.23.0-Setup.exe", "Postman", "11.23.0", Platform::Windows, "x64");
        check("Docker Desktop Installer.exe", "Docker Desktop", "", Platform::Windows, "");
        check("WeChatSetup.exe", "WeChat", "", Platform::Windows, "");
        check("tsetup-x64.5.9.0.exe", "tsetup", "5.9.0", Platform::Windows, "x64");
        check("QQ9.9.15.240925_x64.exe", "QQ", "9.9.15.240925", Platform::Windows, "x64");
        check("OBS-Studio-31.0.0-Windows-Installer.exe", "OBS-Studio", "31.0.0", Platform::Windows, "");
    }

    #[test]
    fn other_platforms() {
        check("Obsidian-1.7.7.dmg", "Obsidian", "1.7.7", Platform::Macos, "");
        check("Firefox 133.0.dmg", "Firefox", "133.0", Platform::Macos, "");
        check("code_1.96.2-1734607745_amd64.deb", "code", "1.96.2", Platform::Linux, "x64");
        check("Obsidian-1.7.7-arm64.AppImage", "Obsidian", "1.7.7", Platform::Linux, "arm64");
        check("Telegram_11.5.3_arm64.apk", "Telegram", "11.5.3", Platform::Android, "arm64");
        check("blender-4.3.2-linux-x64.tar.xz", "blender", "4.3.2", Platform::Linux, "x64");
        check("ffmpeg-7.1-macos-universal.zip", "ffmpeg", "7.1", Platform::Macos, "universal");
        check("some-tool-2.0.0.zip", "some-tool", "2.0.0", Platform::Cross, "");
    }

    #[test]
    fn tricky_names() {
        check("mpv0.39.0.exe", "mpv", "0.39.0", Platform::Windows, "");
        // Win32 在这里是名字的一部分，不当作架构标记
        check("Win32DiskImager-1.0.0-binary.zip", "Win32DiskImager", "1.0.0", Platform::Cross, "");
        check("Clash for Windows-0.20.39-win.7z", "Clash for Windows", "0.20.39", Platform::Windows, "");
        check("Sublime Text Build 4189 x64 Setup.exe", "Sublime Text", "4189", Platform::Windows, "x64");
        check("app-2.1.0-beta.3-x64.exe", "app", "2.1.0-beta.3", Platform::Windows, "x64");
        check("setup.exe", "setup", "", Platform::Windows, "");
        check("微信_3.9.12.exe", "微信", "3.9.12", Platform::Windows, "");
        check("1.2.3-foo.exe", "foo", "1.2.3", Platform::Windows, "");
    }

    #[test]
    fn extensions() {
        assert_eq!(split_ext("a.tar.gz"), ("a".into(), "tar.gz".into()));
        assert_eq!(split_ext("A.TAR.GZ"), ("A".into(), "tar.gz".into()));
        assert_eq!(split_ext("pkg-1.0.pkg.tar.zst"), ("pkg-1.0".into(), "pkg.tar.zst".into()));
        assert_eq!(split_ext("readme"), ("readme".into(), "".into()));
        assert_eq!(split_ext(".bashrc"), (".bashrc".into(), "".into()));
        assert!(is_installer("x.AppImage"));
        assert!(!is_installer("notes.txt"));
    }

    #[test]
    fn name_matching() {
        assert_eq!(normalize_name("Visual Studio Code"), normalize_name("visual-studio-code"));
        assert_ne!(normalize_name("Git"), normalize_name("GitHub"));
    }
}
