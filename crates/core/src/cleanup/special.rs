//! 规则写不了的几样：正在运行的程序（查看、退出）、回收站、散落在下载文件夹和桌面上的安装包。

use crate::identify;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime};

fn command(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command
}

// ── 正在运行的程序 ───────────────────────────────────────────

/// 正在运行的程序名（小写）：macOS 是可执行文件的名字（Google Chrome），Windows 是映像名（chrome.exe），
/// Linux 是 /proc 里的 comm（最多 15 个字符）。列不出来时是空的，当作都没在运行。
pub fn running_processes() -> HashSet<String> {
    #[cfg(target_os = "macos")]
    {
        let output = command("ps").args(["-axo", "comm="]).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
        output
            .lines()
            .filter_map(|line| Path::new(line.trim()).file_name().map(|n| n.to_string_lossy().to_lowercase()))
            .collect()
    }
    #[cfg(windows)]
    {
        let output = command("tasklist").args(["/fo", "csv", "/nh"]).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
        output.lines().filter_map(|line| line.split("\",\"").next()).map(|name| name.trim_matches('"').to_lowercase()).filter(|n| !n.is_empty()).collect()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let Ok(read) = std::fs::read_dir("/proc") else { return HashSet::new() };
        read.flatten()
            .filter(|entry| entry.file_name().to_string_lossy().chars().all(|c| c.is_ascii_digit()))
            .filter_map(|entry| std::fs::read_to_string(entry.path().join("comm")).ok())
            .map(|name| name.trim().to_lowercase())
            .collect()
    }
    #[cfg(not(any(unix, windows)))]
    HashSet::new()
}

/// 这些进程里有没有正在运行的。
pub fn any_running(processes: &[String], running: &HashSet<String>) -> bool {
    processes.iter().any(|p| running.contains(&p.to_lowercase()))
}

/// 让程序正常退出（和点菜单里的“退出”一样，会保存好再关），不强行结束。等它退出，最多等 `wait`。
/// 返回是不是都退出了。
pub fn quit(processes: &[String], wait: Duration) -> bool {
    let Some(first) = processes.first() else { return true };
    #[cfg(target_os = "macos")]
    {
        let script = format!("tell application \"{}\" to quit", first.replace('\\', "\\\\").replace('"', "\\\""));
        let _ = command("osascript").args(["-e", &script]).output();
    }
    #[cfg(windows)]
    for name in processes {
        // 不带 /F：给窗口发关闭消息，程序自己收尾
        let _ = command("taskkill").args(["/IM", name]).output();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    for name in processes {
        let _ = command("pkill").args(["-TERM", "-x", name]).output();
    }
    let _ = first;
    let start = Instant::now();
    loop {
        if !any_running(processes, &running_processes()) {
            return true;
        }
        if start.elapsed() >= wait {
            return false;
        }
        std::thread::sleep(Duration::from_millis(400));
    }
}

// ── 回收站 ───────────────────────────────────────────────────

/// 回收站里有多少。`size` 为 None 表示读不到（macOS 要“完全磁盘访问权限”）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrashInfo {
    pub size: Option<u64>,
    pub items: u64,
}

/// 量回收站（废纸篓）。这个系统上没有时返回 None。
pub fn trash_info(home: &Path, xdg_data: Option<&Path>) -> Option<TrashInfo> {
    #[cfg(target_os = "macos")]
    {
        let _ = xdg_data;
        let dir = home.join(".Trash");
        match std::fs::read_dir(&dir) {
            Ok(read) => {
                let mut seen = HashSet::new();
                let (mut size, mut items) = (0u64, 0u64);
                for entry in read.flatten() {
                    if entry.file_name() == ".DS_Store" {
                        continue;
                    }
                    items += 1;
                    size += super::measure::measure(&entry.path(), &mut seen).size;
                }
                Some(TrashInfo { size: Some(size), items })
            }
            Err(_) if dir.exists() || std::fs::symlink_metadata(&dir).is_ok() => Some(TrashInfo { size: None, items: 0 }),
            Err(_) => Some(TrashInfo { size: Some(0), items: 0 }),
        }
    }
    #[cfg(windows)]
    {
        let _ = (home, xdg_data);
        use windows_sys::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};
        let mut info = SHQUERYRBINFO { cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32, i64Size: 0, i64NumItems: 0 };
        // 不给盘符：所有盘的回收站加起来
        let result = unsafe { SHQueryRecycleBinW(std::ptr::null(), &mut info) };
        if result != 0 {
            return Some(TrashInfo { size: None, items: 0 });
        }
        Some(TrashInfo { size: Some(info.i64Size.max(0) as u64), items: info.i64NumItems.max(0) as u64 })
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let base = xdg_data.map(Path::to_path_buf).unwrap_or_else(|| home.join(".local/share"));
        let files = base.join("Trash/files");
        let mut seen = HashSet::new();
        let (mut size, mut items) = (0u64, 0u64);
        if let Ok(read) = std::fs::read_dir(&files) {
            for entry in read.flatten() {
                items += 1;
                size += super::measure::measure(&entry.path(), &mut seen).size;
            }
        }
        Some(TrashInfo { size: Some(size), items })
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (home, xdg_data);
        None
    }
}

/// 清空回收站（废纸篓）。macOS 让访达来清（第一次会问能不能控制“访达”），用户不允许时返回 `automation_denied`。
pub fn empty_trash(home: &Path, xdg_data: Option<&Path>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let _ = (home, xdg_data);
        let output = command("osascript").args(["-e", "tell application \"Finder\" to empty trash"]).output().map_err(|e| e.to_string())?;
        if output.status.success() {
            return Ok(());
        }
        let text = String::from_utf8_lossy(&output.stderr).to_string();
        // -1743：没有允许控制访达；-128：用户取消
        if text.contains("-1743") {
            Err("automation_denied".into())
        } else if text.contains("-128") {
            Err("cancelled".into())
        } else {
            Err(text.trim().to_string())
        }
    }
    #[cfg(windows)]
    {
        let _ = (home, xdg_data);
        use windows_sys::Win32::UI::Shell::{SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND};
        let result = unsafe { SHEmptyRecycleBinW(std::ptr::null_mut(), std::ptr::null(), SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND) };
        // 已经是空的时候返回 E_UNEXPECTED，也算成功
        if result == 0 || result as u32 == 0x8000_FFFF {
            Ok(())
        } else {
            Err(format!("0x{:08X}", result as u32))
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let base = xdg_data.map(Path::to_path_buf).unwrap_or_else(|| home.join(".local/share"));
        let mut failed = 0;
        for sub in ["files", "info", "expunged"] {
            let Ok(read) = std::fs::read_dir(base.join("Trash").join(sub)) else { continue };
            for entry in read.flatten() {
                failed += super::measure::remove_tree(&entry.path()).failed;
            }
        }
        if failed == 0 { Ok(()) } else { Err("in_use".into()) }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (home, xdg_data);
        Err("not_supported".into())
    }
}

// ── 剪贴板 ───────────────────────────────────────────────────

/// Linux 上清剪贴板用的程序：Wayland 用 wl-copy，X11 用 xsel。都没有时清不了。
#[cfg(all(unix, not(target_os = "macos")))]
fn linux_clipboard_tool() -> Option<&'static [&'static str]> {
    let has = |program: &str| command("sh").args(["-c", &format!("command -v {program}")]).output().map(|o| o.status.success()).unwrap_or(false);
    if std::env::var_os("WAYLAND_DISPLAY").is_some() && has("wl-copy") {
        return Some(&["wl-copy", "--clear"]);
    }
    if std::env::var_os("DISPLAY").is_some() && has("xsel") {
        return Some(&["xsel", "--clipboard", "--clear"]);
    }
    None
}

/// 这个系统上清得了剪贴板。
pub fn clipboard_supported() -> bool {
    #[cfg(any(target_os = "macos", windows))]
    {
        true
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        linux_clipboard_tool().is_some()
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

/// 清空剪贴板。
pub fn clear_clipboard() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        // 给 pbcopy 一个空的输入：剪贴板里就什么都没有了
        let mut child = command("pbcopy").stdin(std::process::Stdio::piped()).spawn().map_err(|e| e.to_string())?;
        drop(child.stdin.take());
        let status = child.wait().map_err(|e| e.to_string())?;
        if status.success() { Ok(()) } else { Err("in_use".into()) }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard};
        // 别的程序正开着剪贴板时打不开：算“正在被使用”
        unsafe {
            if OpenClipboard(std::ptr::null_mut()) == 0 {
                return Err("in_use".into());
            }
            let emptied = EmptyClipboard() != 0;
            CloseClipboard();
            if emptied { Ok(()) } else { Err("in_use".into()) }
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let tool = linux_clipboard_tool().ok_or_else(|| "not_supported".to_string())?;
        let status = command(tool[0]).args(&tool[1..]).status().map_err(|e| e.to_string())?;
        if status.success() { Ok(()) } else { Err("in_use".into()) }
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err("not_supported".into())
    }
}

// ── 系统自带的清理命令 ───────────────────────────────────────

/// 运行规则里写好的 PowerShell 命令（只有 Windows 上要管理员权限的规则有，比如 `Delete-DeliveryOptimizationCache -Force`）。
pub fn run_command(script: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        let output = command("powershell").args(["-NoProfile", "-NonInteractive", "-Command", script]).output().map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            let text = String::from_utf8_lossy(&output.stderr).trim().chars().take(300).collect::<String>();
            Err(if text.is_empty() { "failed".into() } else { text })
        }
    }
    #[cfg(not(windows))]
    {
        let _ = script;
        Err("not_supported".into())
    }
}

// ── Cookie ───────────────────────────────────────────────────

/// 保留名单里的一项写成规范的域名：去掉 `https://`、路径、端口、开头的 `www.` 和 `.`，转小写。不像域名时返回 None。
pub fn normalize_domain(input: &str) -> Option<String> {
    let lower = input.trim().to_lowercase();
    let rest = lower.split_once("://").map(|(_, rest)| rest).unwrap_or(&lower);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit_once('@').map(|(_, h)| h).unwrap_or(host);
    let host = host.split(':').next().unwrap_or("");
    let host = host.trim_start_matches("*.").trim_matches('.');
    let host = host.strip_prefix("www.").unwrap_or(host);
    let valid = !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| !label.is_empty() && label.len() <= 63 && label.chars().all(|c| c.is_alphanumeric() || c == '-'))
        && (host.contains('.') || host == "localhost");
    valid.then(|| host.to_string())
}

/// 这个 Cookie 的网站在不在保留名单里（名单里的域名本身和它的子域名都算）。
pub fn cookie_kept(host: &str, keep: &[String]) -> bool {
    let host = host.trim_start_matches('.').to_lowercase();
    keep.iter().any(|k| host == *k || host.ends_with(&format!(".{k}")))
}

/// 是浏览器的 Cookie 数据库本身（不是它的日志文件）。
pub fn is_cookie_db(path: &Path) -> bool {
    let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    name == "cookies" || name == "cookies.sqlite"
}

/// 数据库加上它的日志文件一共多大。
#[cfg_attr(not(feature = "sqlite"), allow(dead_code))]
fn db_size(path: &Path) -> u64 {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    ["", "-journal", "-wal", "-shm"]
        .iter()
        .filter_map(|suffix| std::fs::metadata(path.with_file_name(format!("{name}{suffix}"))).ok())
        .map(|meta| meta.len())
        .sum()
}

/// 删掉 Cookie 数据库里保留名单以外网站的 Cookie，再整理数据库。返回腾出的空间。
/// 浏览器开着时数据库被锁住，返回 `in_use`。Chromium 系的表是 `cookies`（`host_key`），Firefox 是 `moz_cookies`（`host`）。
#[cfg(feature = "sqlite")]
pub fn prune_cookies(path: &Path, keep: &[String]) -> Result<u64, String> {
    use rusqlite::{Connection, OpenFlags};
    let before = db_size(path);
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX).map_err(db_error)?;
    conn.busy_timeout(Duration::from_millis(800)).map_err(db_error)?;
    let (table, column) = cookie_table(&conn)?;
    let hosts: Vec<String> = {
        let mut statement = conn.prepare(&format!("SELECT DISTINCT {column} FROM {table}")).map_err(db_error)?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0)).map_err(db_error)?;
        rows.collect::<Result<_, _>>().map_err(db_error)?
    };
    let doomed: Vec<&String> = hosts.iter().filter(|host| !cookie_kept(host, keep)).collect();
    if !doomed.is_empty() {
        let tx = conn.unchecked_transaction().map_err(db_error)?;
        {
            let mut delete = tx.prepare(&format!("DELETE FROM {table} WHERE {column} = ?1")).map_err(db_error)?;
            for host in doomed {
                delete.execute([host]).map_err(db_error)?;
            }
        }
        tx.commit().map_err(db_error)?;
        // 整理一下文件才会变小；整理不了也不要紧，Cookie 已经删了
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;");
    }
    conn.close().map_err(|(_, e)| db_error(e))?;
    Ok(before.saturating_sub(db_size(path)))
}

#[cfg(not(feature = "sqlite"))]
pub fn prune_cookies(_path: &Path, _keep: &[String]) -> Result<u64, String> {
    Err("not_supported".into())
}

#[cfg(feature = "sqlite")]
fn db_error(err: rusqlite::Error) -> String {
    match err.sqlite_error_code() {
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => "in_use".into(),
        Some(rusqlite::ErrorCode::NotADatabase) => "unknown_format".into(),
        _ => err.to_string(),
    }
}

#[cfg(feature = "sqlite")]
fn cookie_table(conn: &rusqlite::Connection) -> Result<(&'static str, &'static str), String> {
    let has = |name: &str| {
        conn.query_row("SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1", [name], |row| row.get::<_, i64>(0)).map(|n| n > 0).unwrap_or(false)
    };
    if has("cookies") {
        Ok(("cookies", "host_key"))
    } else if has("moz_cookies") {
        Ok(("moz_cookies", "host"))
    } else {
        Err("unknown_format".into())
    }
}

/// 网站的主域名：`accounts.google.com` → `google.com`，`www.bbc.co.uk` → `bbc.co.uk`。只用来给保留名单出主意，不求准。
pub fn site_of(host: &str) -> String {
    let host = host.trim_start_matches('.').to_lowercase();
    let labels: Vec<&str> = host.split('.').collect();
    let n = labels.len();
    let keep = if n >= 3 && labels[n - 1].len() == 2 && ["co", "com", "net", "org", "gov", "edu", "ac"].contains(&labels[n - 2]) { 3 } else { 2 };
    labels[n.saturating_sub(keep)..].join(".")
}

/// 这些 Cookie 数据库里各个网站有多少个 Cookie（按主域名合并），多的在前。先复制一份再读，不锁住浏览器正在用的文件。
#[cfg(feature = "sqlite")]
pub fn cookie_sites(dbs: &[PathBuf]) -> Vec<(String, u64)> {
    use rusqlite::{Connection, OpenFlags};
    let mut counts: HashMap<String, u64> = HashMap::new();
    let Ok(dir) = tempfile_dir() else { return Vec::new() };
    for (i, db) in dbs.iter().enumerate() {
        let copy = dir.join(format!("{i}.sqlite"));
        if std::fs::copy(db, &copy).is_err() {
            continue;
        }
        let wal = db.with_file_name(format!("{}-wal", db.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
        let _ = std::fs::copy(&wal, dir.join(format!("{i}.sqlite-wal")));
        let Ok(conn) = Connection::open_with_flags(&copy, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX) else { continue };
        let Ok((table, column)) = cookie_table(&conn) else { continue };
        let Ok(mut statement) = conn.prepare(&format!("SELECT {column}, count(*) FROM {table} GROUP BY {column}")) else { continue };
        let Ok(rows) = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))) else { continue };
        for (host, n) in rows.flatten() {
            *counts.entry(site_of(&host)).or_default() += n.max(0) as u64;
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    let mut list: Vec<(String, u64)> = counts.into_iter().filter(|(site, _)| !site.is_empty()).collect();
    list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    list
}

#[cfg(not(feature = "sqlite"))]
pub fn cookie_sites(_dbs: &[PathBuf]) -> Vec<(String, u64)> {
    Vec::new()
}

#[cfg(feature = "sqlite")]
fn tempfile_dir() -> std::io::Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("shibapack-cookies-{}", crate::new_id()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

// ── 散落的安装包 ─────────────────────────────────────────────

/// 仓库里的一个文件：拿来和下载文件夹里的安装包比对。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryFile {
    pub size: u64,
    pub sha256: String,
    pub software: String,
    pub version: String,
}

/// 下载文件夹或桌面上找到的一个安装包。
#[derive(Debug, Clone, PartialEq)]
pub struct Loose {
    pub path: PathBuf,
    /// 它所在的文件夹（下载文件夹本身，或者下面的一层）。
    pub root: PathBuf,
    pub size: u64,
    pub modified: Option<SystemTime>,
    /// 和仓库里的哪个文件一模一样：（软件名，版本号）。
    pub same_as: Option<(String, String)>,
    /// 不一样，但仓库里有这个软件：（软件名，仓库里的版本号）。
    pub known: Option<(String, Vec<String>)>,
}

/// 不在仓库里的、才当作安装包列出来的扩展名。和仓库比对一模一样的不限扩展名（压缩包也算）。
const STRICT: &[&str] = &[
    "exe", "msi", "msix", "msixbundle", "appx", "appxbundle", "appinstaller", "dmg", "pkg", "mpkg", "deb", "rpm", "appimage", "flatpak",
    "snap", "apk", "xapk", "apks", "apkm", "ipa",
];

/// 在这些文件夹（下载文件夹、桌面）和它们下面一层里找安装包，和仓库比对：大小一样的才算 SHA-256。
/// 隐藏的文件夹、.app、里面有 .git 的项目文件夹不进去。
pub fn loose_installers(dirs: &[PathBuf], library: &[LibraryFile]) -> Vec<Loose> {
    let mut by_size: HashMap<u64, Vec<&LibraryFile>> = HashMap::new();
    for file in library {
        by_size.entry(file.size).or_default().push(file);
    }
    let mut by_name: HashMap<String, (String, Vec<String>)> = HashMap::new();
    for file in library {
        let entry = by_name.entry(identify::normalize_name(&file.software)).or_insert_with(|| (file.software.clone(), Vec::new()));
        if !file.version.is_empty() && !entry.1.contains(&file.version) {
            entry.1.push(file.version.clone());
        }
    }
    let mut found = Vec::new();
    for dir in dirs {
        let mut folders = vec![dir.clone()];
        for entry in read_sorted(dir) {
            let name = entry.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if super::vars::real_dir(&entry) && !name.starts_with('.') && !name.to_lowercase().ends_with(".app") && !entry.join(".git").exists() {
                folders.push(entry);
            }
        }
        for folder in folders {
            for path in read_sorted(&folder) {
                let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
                if !meta.is_file() {
                    continue;
                }
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                if name.starts_with('.') || !identify::is_installer(&name) {
                    continue;
                }
                let size = meta.len();
                let same_as = by_size.get(&size).and_then(|candidates| {
                    let hash = crate::import::sha256_file(&path).ok()?;
                    candidates.iter().find(|file| file.sha256.eq_ignore_ascii_case(&hash)).map(|file| (file.software.clone(), file.version.clone()))
                });
                let (_, ext) = identify::split_ext(&name);
                if same_as.is_none() && !STRICT.contains(&ext.as_str()) {
                    continue;
                }
                let known = if same_as.is_none() {
                    let guess = identify::identify(&name);
                    by_name.get(&identify::normalize_name(&guess.name)).cloned()
                } else {
                    None
                };
                found.push(Loose { path, root: folder.clone(), size, modified: meta.modified().ok(), same_as, known });
            }
        }
    }
    found
}

fn read_sorted(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir).map(|read| read.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    paths.sort();
    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn finds_installers_and_matches_the_library() {
        let dir = tempfile::tempdir().unwrap();
        let downloads = dir.path().join("Downloads");
        fs::create_dir_all(downloads.join("tools")).unwrap();
        fs::create_dir_all(downloads.join("proj/.git")).unwrap();
        fs::create_dir_all(downloads.join(".hidden")).unwrap();
        fs::write(downloads.join("Foo-1.2.0.dmg"), b"same bytes").unwrap();
        fs::write(downloads.join("Bar-2.0.exe"), b"different").unwrap();
        fs::write(downloads.join("tools/Baz-1.0.msi"), b"baz").unwrap();
        fs::write(downloads.join("photos.zip"), b"not an installer").unwrap();
        fs::write(downloads.join("proj/App.exe"), b"built locally").unwrap();
        fs::write(downloads.join(".hidden/Secret.exe"), b"hidden").unwrap();
        fs::write(downloads.join("notes.txt"), b"text").unwrap();
        let hash = crate::import::sha256_file(&downloads.join("Foo-1.2.0.dmg")).unwrap();
        let library = vec![
            LibraryFile { size: 10, sha256: hash, software: "Foo".into(), version: "1.2.0".into() },
            LibraryFile { size: 99, sha256: "00".into(), software: "Bar".into(), version: "1.0".into() },
        ];
        let found = loose_installers(std::slice::from_ref(&downloads), &library);
        let names: Vec<String> = found.iter().map(|f| f.path.file_name().unwrap().to_string_lossy().to_string()).collect();
        assert_eq!(names, vec!["Bar-2.0.exe", "Foo-1.2.0.dmg", "Baz-1.0.msi"]);
        let foo = found.iter().find(|f| f.path.ends_with("Foo-1.2.0.dmg")).unwrap();
        assert_eq!(foo.same_as, Some(("Foo".into(), "1.2.0".into())));
        let bar = found.iter().find(|f| f.path.ends_with("Bar-2.0.exe")).unwrap();
        assert_eq!(bar.same_as, None);
        assert_eq!(bar.known, Some(("Bar".into(), vec!["1.0".into()])));
        let baz = found.iter().find(|f| f.path.ends_with("Baz-1.0.msi")).unwrap();
        assert_eq!(baz.root, downloads.join("tools"));
    }

    #[test]
    fn a_zip_identical_to_a_library_file_counts() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Tool-3.zip"), b"zip bytes").unwrap();
        let hash = crate::import::sha256_file(&dir.path().join("Tool-3.zip")).unwrap();
        let library = vec![LibraryFile { size: 9, sha256: hash, software: "Tool".into(), version: "3".into() }];
        let found = loose_installers(&[dir.path().to_path_buf()], &library);
        assert_eq!(found.len(), 1);
        assert!(found[0].same_as.is_some());
    }

    #[test]
    fn keep_list_domains_are_normalized() {
        assert_eq!(normalize_domain("https://www.GitHub.com/login?x=1"), Some("github.com".into()));
        assert_eq!(normalize_domain(".google.com"), Some("google.com".into()));
        assert_eq!(normalize_domain("*.bbc.co.uk:443"), Some("bbc.co.uk".into()));
        assert_eq!(normalize_domain("localhost"), Some("localhost".into()));
        assert_eq!(normalize_domain("not a domain"), None);
        assert_eq!(normalize_domain(""), None);
        let keep = vec!["github.com".to_string()];
        assert!(cookie_kept(".github.com", &keep));
        assert!(cookie_kept("gist.github.com", &keep));
        assert!(!cookie_kept("notgithub.com", &keep));
        assert_eq!(site_of(".accounts.google.com"), "google.com");
        assert_eq!(site_of("www.bbc.co.uk"), "bbc.co.uk");
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn prunes_cookies_outside_the_keep_list() {
        let dir = tempfile::tempdir().unwrap();
        for (file, table, column) in [("Cookies", "cookies", "host_key"), ("cookies.sqlite", "moz_cookies", "host")] {
            let path = dir.path().join(file);
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch(&format!("CREATE TABLE {table} ({column} TEXT, name TEXT, value BLOB);")).unwrap();
            for host in [".github.com", "gist.github.com", ".tracker.example", "ads.example.net"] {
                for i in 0..50 {
                    conn.execute(&format!("INSERT INTO {table} VALUES (?1, ?2, zeroblob(200))"), rusqlite::params![host, format!("c{i}")]).unwrap();
                }
            }
            drop(conn);
            let sites = cookie_sites(std::slice::from_ref(&path));
            assert!(sites.iter().any(|(site, n)| site == "github.com" && *n == 100));
            let freed = prune_cookies(&path, &["github.com".to_string()]).unwrap();
            assert!(freed > 0, "{file} 应该变小");
            let conn = rusqlite::Connection::open(&path).unwrap();
            let left: Vec<String> = conn
                .prepare(&format!("SELECT DISTINCT {column} FROM {table} ORDER BY 1"))
                .unwrap()
                .query_map([], |row| row.get(0))
                .unwrap()
                .flatten()
                .collect();
            assert_eq!(left, vec![".github.com".to_string(), "gist.github.com".to_string()]);
        }
        // 不是 Cookie 数据库的不碰
        std::fs::write(dir.path().join("Cookies2"), b"not sqlite").unwrap();
        assert!(prune_cookies(&dir.path().join("Cookies2"), &[]).is_err());
    }
}
