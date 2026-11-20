//! 规则里路径开头的变量。每个系统各有一套，规则只能用这里列出来的，不能写绝对路径或任意环境变量。
//!
//! 变量后面可以跟 `/` 分隔的几段，其中一段可以带 `*`，只匹配一层文件夹（`${user_cache}/Google/Chrome/*/Cache`
//! 里的 `*` 是浏览器的各个配置）。展开时只认真实存在的文件夹，符号链接不算。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    Macos,
    Windows,
    Linux,
}

impl Os {
    pub fn current() -> Os {
        if cfg!(target_os = "macos") {
            Os::Macos
        } else if cfg!(windows) {
            Os::Windows
        } else {
            Os::Linux
        }
    }

    pub const ALL: [Os; 3] = [Os::Macos, Os::Windows, Os::Linux];
}

/// 每个系统能用的变量名。
pub fn names(os: Os) -> &'static [&'static str] {
    match os {
        Os::Macos => &["home", "temp", "downloads", "desktop", "user_library", "user_cache", "app_support", "user_logs", "system_library"],
        Os::Windows => &["home", "temp", "downloads", "desktop", "local_app_data", "roaming_app_data", "windows", "program_data", "system_drive"],
        Os::Linux => &["home", "temp", "downloads", "desktop", "xdg_cache", "xdg_config", "xdg_data", "xdg_state"],
    }
}

/// 这些变量指的是个人文件夹或一整类数据的总文件夹：不按名字筛的规则不能直接拿它们当根目录（会把整个文件夹清空）。
pub const BROAD: &[&str] = &[
    "home",
    "downloads",
    "desktop",
    "user_library",
    "user_cache",
    "app_support",
    "local_app_data",
    "roaming_app_data",
    "xdg_cache",
    "xdg_config",
    "xdg_data",
    "xdg_state",
    "system_library",
    "windows",
    "program_data",
    "system_drive",
];

/// 系统级的位置：只有要管理员权限的规则能用，而且只能用 `rule.rs` 里列出来的那几个文件夹。
pub const SYSTEM: &[&str] = &["system_library", "windows", "program_data", "system_drive"];

/// 变量名 → 这台电脑上的路径。
#[derive(Debug, Clone, Default)]
pub struct Vars {
    map: HashMap<String, PathBuf>,
}

impl Vars {
    /// 按当前用户和系统算出来。连用户文件夹都找不到时是空的，所有规则都展开不出东西。
    pub fn from_env() -> Vars {
        let env = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from).filter(|p| p.is_absolute());
        let mut vars = Vars::default();
        let Some(home) = (if cfg!(windows) { env("USERPROFILE") } else { env("HOME") }) else { return vars };
        vars.set("home", home.clone());
        vars.set("temp", std::env::temp_dir());
        match Os::current() {
            Os::Macos => {
                let library = home.join("Library");
                vars.set("user_cache", library.join("Caches"));
                vars.set("app_support", library.join("Application Support"));
                vars.set("user_logs", library.join("Logs"));
                vars.set("user_library", library);
                vars.set("downloads", home.join("Downloads"));
                vars.set("desktop", home.join("Desktop"));
                vars.set("system_library", PathBuf::from("/Library"));
            }
            Os::Windows => {
                vars.set("local_app_data", env("LOCALAPPDATA").unwrap_or_else(|| home.join("AppData").join("Local")));
                vars.set("roaming_app_data", env("APPDATA").unwrap_or_else(|| home.join("AppData").join("Roaming")));
                vars.set("downloads", home.join("Downloads"));
                vars.set("desktop", home.join("Desktop"));
                let drive = std::env::var_os("SystemDrive").filter(|v| !v.is_empty()).map(|d| PathBuf::from(format!("{}\\", d.to_string_lossy())));
                let drive = drive.unwrap_or_else(|| PathBuf::from("C:\\"));
                vars.set("windows", env("SystemRoot").or_else(|| env("windir")).unwrap_or_else(|| drive.join("Windows")));
                vars.set("program_data", env("ProgramData").unwrap_or_else(|| drive.join("ProgramData")));
                vars.set("system_drive", drive);
            }
            Os::Linux => {
                vars.set("xdg_cache", env("XDG_CACHE_HOME").unwrap_or_else(|| home.join(".cache")));
                vars.set("xdg_config", env("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config")));
                vars.set("xdg_data", env("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local").join("share")));
                vars.set("xdg_state", env("XDG_STATE_HOME").unwrap_or_else(|| home.join(".local").join("state")));
                let config = vars.get("xdg_config").map(Path::to_path_buf).unwrap_or_else(|| home.join(".config"));
                let user_dirs = std::fs::read_to_string(config.join("user-dirs.dirs")).unwrap_or_default();
                let pick = |key: &str, fallback: &str| user_dir(&user_dirs, key, &home).unwrap_or_else(|| home.join(fallback));
                vars.set("downloads", pick("XDG_DOWNLOAD_DIR", "Downloads"));
                vars.set("desktop", pick("XDG_DESKTOP_DIR", "Desktop"));
            }
        }
        vars
    }

    /// 测试用：直接给出每个变量。
    pub fn with(pairs: &[(&str, &Path)]) -> Vars {
        let mut vars = Vars::default();
        for (name, path) in pairs {
            vars.set(name, path.to_path_buf());
        }
        vars
    }

    fn set(&mut self, name: &str, path: PathBuf) {
        self.map.insert(name.to_string(), path);
    }

    pub fn get(&self, name: &str) -> Option<&Path> {
        self.map.get(name).map(PathBuf::as_path)
    }
}

/// `~/.config/user-dirs.dirs` 里的一行：`XDG_DOWNLOAD_DIR="$HOME/下载"`。
fn user_dir(text: &str, key: &str, home: &Path) -> Option<PathBuf> {
    let line = text.lines().map(str::trim).find(|line| line.starts_with(key) && line[key.len()..].trim_start().starts_with('='))?;
    let value = line.split_once('=')?.1.trim().trim_matches('"');
    let path = match value.strip_prefix("$HOME") {
        Some(rest) => home.join(rest.trim_start_matches('/')),
        None => PathBuf::from(value),
    };
    // 没设置时这一项等于用户文件夹本身，那就当没有
    (path.is_absolute() && path != home).then_some(path)
}

/// 把 `${name}/a/b` 拆成变量名和后面的部分。
pub fn split(template: &str) -> Option<(&str, &str)> {
    let rest = template.strip_prefix("${")?;
    let (name, tail) = rest.split_once('}')?;
    if !tail.is_empty() && !tail.starts_with('/') {
        return None;
    }
    Some((name, tail.strip_prefix('/').unwrap_or(tail)))
}

/// 规则里的路径写得对不对：以这个系统的变量开头，用 `/` 分隔，没有 `..`、反斜杠、空段，
/// 带 `*` 的段不能只有 `*` 加别的通配（`**`）。不对时返回原因。
pub fn check(template: &str, os: Os) -> Result<(), String> {
    let (name, tail) = split(template).ok_or_else(|| format!("{template}：要以 ${{变量}} 开头"))?;
    if !names(os).contains(&name) {
        return Err(format!("{template}：{os:?} 没有变量 {name}"));
    }
    if template.contains('\\') {
        return Err(format!("{template}：用 / 分隔"));
    }
    for segment in tail.split('/').filter(|_| !tail.is_empty()) {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(format!("{template}：路径里有空段、. 或 .."));
        }
        if segment.contains("**") {
            return Err(format!("{template}：* 只匹配一层"));
        }
    }
    Ok(())
}

/// 展开成这台电脑上真实存在的文件夹（不是符号链接），按路径排好。变量没有值或者路径不存在时是空的。
pub fn expand(template: &str, vars: &Vars) -> Vec<PathBuf> {
    let Some((name, tail)) = split(template) else { return Vec::new() };
    let Some(base) = vars.get(name) else { return Vec::new() };
    let mut current = vec![base.to_path_buf()];
    for segment in tail.split('/').filter(|s| !s.is_empty()) {
        current = if segment.contains('*') || segment.contains('?') {
            current.iter().flat_map(|dir| matching_children(dir, segment)).collect()
        } else {
            current.into_iter().map(|dir| dir.join(segment)).collect()
        };
    }
    let mut found: Vec<PathBuf> = current.into_iter().filter(|path| real_dir(path)).collect();
    found.sort();
    found.dedup();
    found
}

/// 是真实存在的文件夹，不是指向文件夹的符号链接或联接点。
pub fn real_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).map(|meta| meta.is_dir()).unwrap_or(false)
}

fn matching_children(dir: &Path, pattern: &str) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(dir) else { return Vec::new() };
    read.flatten()
        .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
        .filter(|entry| glob(pattern, &entry.file_name().to_string_lossy()))
        .map(|entry| entry.path())
        .collect()
}

/// 只认 `*`（任意多个字符）和 `?`（一个字符）的通配，不分大小写。
pub fn glob(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let n: Vec<char> = name.to_lowercase().chars().collect();
    let (mut pi, mut ni) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ni;
            pi += 1;
        } else if let Some(at) = star {
            pi = at + 1;
            mark += 1;
            ni = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn glob_matches_like_a_shell() {
        assert!(glob("*.part", "Setup.exe.PART"));
        assert!(glob("thumbcache_*.db", "thumbcache_256.db"));
        assert!(!glob("thumbcache_*.db", "thumbcache_256.db-wal"));
        assert!(glob("Profile ?", "Profile 1"));
        assert!(glob("*", ""));
        assert!(!glob("a*b", "ac"));
    }

    #[test]
    fn templates_are_checked_per_system() {
        assert!(check("${user_cache}/pip", Os::Macos).is_ok());
        assert!(check("${user_cache}/pip", Os::Windows).is_err());
        assert!(check("${local_app_data}/pip/Cache", Os::Windows).is_ok());
        assert!(check("/tmp/x", Os::Linux).is_err());
        assert!(check("${home}/../x", Os::Linux).is_err());
        assert!(check("${home}//x", Os::Linux).is_err());
        assert!(check("${home}/a\\b", Os::Linux).is_err());
        assert!(check("${home}/**/x", Os::Linux).is_err());
        assert!(check("${temp}", Os::Linux).is_ok());
    }

    #[test]
    fn expands_one_level_wildcards_to_real_folders() {
        let dir = tempfile::tempdir().unwrap();
        let chrome = dir.path().join("Chrome");
        fs::create_dir_all(chrome.join("Default/Cache")).unwrap();
        fs::create_dir_all(chrome.join("Profile 1/Cache")).unwrap();
        fs::create_dir_all(chrome.join("Crashpad")).unwrap();
        fs::write(chrome.join("Local State"), b"{}").unwrap();
        let vars = Vars::with(&[("user_cache", dir.path())]);
        let found = expand("${user_cache}/Chrome/*/Cache", &vars);
        assert_eq!(found, vec![chrome.join("Default/Cache"), chrome.join("Profile 1/Cache")]);
        assert!(expand("${user_cache}/Missing/*", &vars).is_empty());
        assert!(expand("${app_support}/Chrome", &vars).is_empty(), "没有值的变量展开不出东西");
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_roots_are_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("Cache")).unwrap();
        let vars = Vars::with(&[("user_cache", dir.path())]);
        assert!(expand("${user_cache}/Cache", &vars).is_empty());
        assert!(expand("${user_cache}/*", &vars).is_empty());
    }

    #[test]
    fn reads_linux_user_dirs() {
        let home = Path::new("/home/a");
        let text = "# comment\nXDG_DOWNLOAD_DIR=\"$HOME/下载\"\nXDG_DESKTOP_DIR=\"$HOME/\"\n";
        assert_eq!(user_dir(text, "XDG_DOWNLOAD_DIR", home), Some(home.join("下载")));
        assert_eq!(user_dir(text, "XDG_DESKTOP_DIR", home), None);
        assert_eq!(user_dir(text, "XDG_MUSIC_DIR", home), None);
    }
}
