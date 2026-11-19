//! 仓库：根目录、索引读写、备份与恢复、统计、校验。

use crate::{model::*, Error, Result};
use serde::Serialize;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

pub const INDEX_FILE: &str = "software.json";
pub const DATA_DIR: &str = ".shibapack";
const MAX_BACKUPS: usize = 8;
/// 自动备份的最短间隔。连续的小改动不会把 8 份备份都占满。
const BACKUP_INTERVAL: Duration = Duration::from_secs(600);
const TRASH_KEEP: Duration = Duration::from_secs(30 * 24 * 3600);

pub struct Repo {
    root: PathBuf,
    pub lib: Library,
    /// 打开时索引损坏并做了恢复：`Some(备份文件名)`；没有可用备份时是 `Some("")`。
    pub recovered: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub name: String,
    pub created_at: String,
    pub size: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub software: usize,
    pub versions: usize,
    pub files: usize,
    pub total_size: u64,
    pub managed_size: u64,
    pub indexed_size: u64,
    pub backups: usize,
    pub trash_size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingFile {
    pub software_id: String,
    pub version_id: String,
    pub file_id: String,
    pub software: String,
    pub version: String,
    pub name: String,
    pub path: String,
    /// missing：文件不在了；size：大小和记录的不一样。
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyReport {
    pub checked: usize,
    pub problems: Vec<MissingFile>,
}

impl Repo {
    /// 打开（或新建）一个仓库。索引损坏时自动从最新可用的备份恢复。
    pub fn open(root: &Path) -> Result<Repo> {
        fs::create_dir_all(root)?;
        let root = clean_path(&fs::canonicalize(root)?);
        let data = root.join(DATA_DIR);
        for dir in ["backups", "trash", "cache", "logs"] {
            fs::create_dir_all(data.join(dir))?;
        }
        hide_on_windows(&data);

        let mut repo = Repo { root, lib: Library::default(), recovered: None };
        let index = repo.index_path();
        if index.exists() {
            match read_library(&index) {
                Ok(lib) => repo.lib = lib,
                Err(_) => {
                    // 留下损坏的文件供排查，再从备份里找一份能读的
                    fs::rename(&index, unique_path(repo.root.join("software.json.corrupt")))?;
                    let restored = repo.backups().into_iter().find_map(|b| {
                        read_library(&repo.backup_dir().join(&b.name)).ok().map(|lib| (b.name, lib))
                    });
                    match restored {
                        Some((name, lib)) => {
                            repo.lib = lib;
                            repo.recovered = Some(name);
                        }
                        None => repo.recovered = Some(String::new()),
                    }
                    repo.write_index()?;
                }
            }
        } else {
            repo.write_index()?;
        }
        repo.purge_trash();
        Ok(repo)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn index_path(&self) -> PathBuf {
        self.root.join(INDEX_FILE)
    }

    pub fn data_dir(&self) -> PathBuf {
        self.root.join(DATA_DIR)
    }

    fn backup_dir(&self) -> PathBuf {
        self.data_dir().join("backups")
    }

    fn trash_dir(&self) -> PathBuf {
        self.data_dir().join("trash")
    }

    /// 把相对路径（用 `/` 分隔）接到仓库根目录上。
    pub fn root_join(&self, rel: &str) -> PathBuf {
        let mut path = self.root.clone();
        for part in rel.split('/').filter(|p| !p.is_empty()) {
            path.push(part);
        }
        path
    }

    /// 一个文件记录在磁盘上的绝对路径。
    pub fn abs_path(&self, file: &PackageFile) -> PathBuf {
        match file.storage {
            Storage::Managed => self.root_join(&file.path),
            Storage::Indexed => PathBuf::from(&file.path),
        }
    }

    /// 保存索引：写临时文件 → 读回校验 → 需要时备份旧索引 → 原子替换。
    pub fn save(&mut self) -> Result<()> {
        if self.index_path().exists() && self.backup_due() {
            self.backup_current()?;
        }
        self.write_index()
    }

    fn write_index(&mut self) -> Result<()> {
        self.lib.format = FORMAT;
        let tmp = self.root.join("software.json.tmp");
        let json = serde_json::to_vec_pretty(&self.lib)?;
        {
            let mut file = fs::File::create(&tmp)?;
            file.write_all(&json)?;
            file.sync_all()?;
        }
        if let Err(err) = read_library(&tmp) {
            let _ = fs::remove_file(&tmp);
            return Err(err);
        }
        fs::rename(&tmp, self.index_path())?;
        self.write_catalog();
        Ok(())
    }

    fn backup_due(&self) -> bool {
        let newest = self.backups().first().and_then(|b| chrono::DateTime::parse_from_rfc3339(&b.created_at).ok());
        match newest {
            Some(time) => {
                let age = chrono::Local::now().signed_duration_since(time);
                age.to_std().map(|d| d >= BACKUP_INTERVAL).unwrap_or(true)
            }
            None => true,
        }
    }

    /// 把当前磁盘上的索引存一份备份，并只保留最新的 8 份。
    fn backup_current(&self) -> Result<BackupInfo> {
        // 文件名里带到毫秒的时间，按名字排序就是按时间排序
        let dest = loop {
            let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S-%3f");
            let candidate = self.backup_dir().join(format!("software-{stamp}.json"));
            if !candidate.exists() {
                break candidate;
            }
            std::thread::sleep(Duration::from_millis(2));
        };
        fs::write(&dest, fs::read(self.index_path())?)?;
        let backups = self.backups();
        for old in backups.iter().skip(MAX_BACKUPS) {
            let _ = fs::remove_file(self.backup_dir().join(&old.name));
        }
        let name = dest.file_name().unwrap_or_default().to_string_lossy().to_string();
        backups.into_iter().find(|b| b.name == name).ok_or(Error::Code("backup_failed"))
    }

    /// 立即备份（界面上的“立即备份”）。
    pub fn create_backup(&mut self) -> Result<BackupInfo> {
        self.write_index()?;
        self.backup_current()
    }

    /// 备份列表，最新的在前。
    pub fn backups(&self) -> Vec<BackupInfo> {
        let mut list: Vec<BackupInfo> = fs::read_dir(self.backup_dir())
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.ends_with(".json") {
                    return None;
                }
                let meta = entry.metadata().ok()?;
                // 时间以文件名为准：复制文件时有的系统会保留原文件的修改时间
                let created = match backup_time(&name) {
                    Some(time) => time,
                    None => meta.modified().ok()?.into(),
                };
                Some(BackupInfo {
                    name,
                    created_at: created.to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
                    size: meta.len(),
                })
            })
            .collect();
        list.sort_by(|a, b| b.name.cmp(&a.name));
        list
    }

    /// 用一份备份替换当前索引。替换前先把当前索引也备份一次，所以可以再恢复回来。
    pub fn restore_backup(&mut self, name: &str) -> Result<()> {
        if name.contains(['/', '\\']) || !name.ends_with(".json") {
            return Err(Error::Code("invalid_backup"));
        }
        let lib = read_library(&self.backup_dir().join(name)).map_err(|_| Error::Code("invalid_backup"))?;
        self.backup_current()?;
        self.lib = lib;
        self.write_index()
    }

    /// 把索引导出到仓库之外的一个文件。
    pub fn export_to(&mut self, dest: &Path) -> Result<()> {
        self.write_index()?;
        fs::copy(self.index_path(), dest)?;
        Ok(())
    }

    /// 用外部的一份索引文件替换当前索引（先备份当前的）。
    pub fn import_from(&mut self, src: &Path) -> Result<()> {
        let lib = read_library(src).map_err(|_| Error::Code("invalid_backup"))?;
        self.backup_current()?;
        self.lib = lib;
        self.write_index()
    }

    pub fn stats(&self) -> Stats {
        let mut stats = Stats {
            software: self.lib.software.len(),
            backups: self.backups().len(),
            trash_size: dir_size(&self.trash_dir()),
            ..Stats::default()
        };
        for software in &self.lib.software {
            stats.versions += software.versions.len();
            for version in &software.versions {
                for file in &version.files {
                    stats.files += 1;
                    stats.total_size += file.size;
                    match file.storage {
                        Storage::Managed => stats.managed_size += file.size,
                        Storage::Indexed => stats.indexed_size += file.size,
                    }
                }
            }
        }
        stats
    }

    /// 逐个检查索引里的文件是否还在、大小是否对得上。
    pub fn verify(&self) -> VerifyReport {
        let mut report = VerifyReport::default();
        for software in &self.lib.software {
            for version in &software.versions {
                for file in &version.files {
                    report.checked += 1;
                    let reason = match fs::metadata(self.abs_path(file)) {
                        Ok(meta) if meta.is_file() && meta.len() == file.size => continue,
                        Ok(meta) if meta.is_file() => "size",
                        _ => "missing",
                    };
                    report.problems.push(MissingFile {
                        software_id: software.id.clone(),
                        version_id: version.id.clone(),
                        file_id: file.id.clone(),
                        software: software.name.clone(),
                        version: version.version.clone(),
                        name: file.name.clone(),
                        path: self.abs_path(file).to_string_lossy().to_string(),
                        reason: reason.to_string(),
                    });
                }
            }
        }
        report
    }

    /// 把一个托管文件移进仓库的回收站，并清掉因此变空的文件夹。
    pub(crate) fn trash_managed(&self, rel: &str) -> Result<()> {
        let src = self.root_join(rel);
        if !src.exists() {
            return Ok(());
        }
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        let mut dest = self.trash_dir().join(stamp);
        for part in rel.split('/').filter(|p| !p.is_empty()) {
            dest.push(part);
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&src, unique_path(dest))?;
        self.remove_empty_dirs(src.parent());
        Ok(())
    }

    /// 从 `dir` 往上删除空文件夹，到仓库根目录为止。
    pub(crate) fn remove_empty_dirs(&self, mut dir: Option<&Path>) {
        while let Some(current) = dir {
            if current == self.root || !current.starts_with(&self.root) || fs::remove_dir(current).is_err() {
                break;
            }
            dir = current.parent();
        }
    }

    /// 清空回收站。
    pub fn empty_trash(&self) -> Result<()> {
        for entry in fs::read_dir(self.trash_dir())?.flatten() {
            remove_any(&entry.path());
        }
        Ok(())
    }

    fn purge_trash(&self) {
        for entry in fs::read_dir(self.trash_dir()).into_iter().flatten().flatten() {
            let old = entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| SystemTime::now().duration_since(t).ok())
                .is_some_and(|age| age > TRASH_KEEP);
            if old {
                remove_any(&entry.path());
            }
        }
    }
}

/// 从备份文件名 `software-20261119-120000-123.json` 里读出时间。
fn backup_time(name: &str) -> Option<chrono::DateTime<chrono::Local>> {
    let stamp = name.strip_prefix("software-")?.get(..15)?;
    let naive = chrono::NaiveDateTime::parse_from_str(stamp, "%Y%m%d-%H%M%S").ok()?;
    naive.and_local_timezone(chrono::Local).single()
}

fn read_library(path: &Path) -> Result<Library> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn remove_any(path: &Path) {
    if path.is_dir() {
        let _ = fs::remove_dir_all(path);
    } else {
        let _ = fs::remove_file(path);
    }
}

fn dir_size(dir: &Path) -> u64 {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

/// 路径已存在时在文件名后加 ` (2)`、` (3)`……直到不重名。
pub fn unique_path(path: PathBuf) -> PathBuf {
    if !path.exists() {
        return path;
    }
    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
    let (stem, ext) = crate::identify::split_ext(&name);
    for n in 2.. {
        let candidate = if ext.is_empty() { format!("{stem} ({n})") } else { format!("{stem} ({n}).{ext}") };
        let next = path.with_file_name(candidate);
        if !next.exists() {
            return next;
        }
    }
    path
}

/// 去掉 Windows 上 `canonicalize` 加的 `\\?\` 前缀，路径才适合显示和保存。
pub fn clean_path(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    }
}

/// 把软件名或版本号变成各系统都能用的文件夹名。
pub fn sanitize(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { '_' } else { c })
        .collect();
    out = out.trim().trim_end_matches('.').trim().to_string();
    if out.starts_with('.') {
        out.insert(0, '_');
    }
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1",
        "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(&out)) {
        out.push('_');
    }
    if out.is_empty() {
        out = "unnamed".into();
    }
    out
}

#[cfg(windows)]
fn hide_on_windows(path: &Path) {
    let _ = std::process::Command::new("attrib").arg("+h").arg(path).status();
}

#[cfg(not(windows))]
fn hide_on_windows(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_creates_index_and_reopens() {
        let dir = tempfile::tempdir().unwrap();
        let mut repo = Repo::open(dir.path()).unwrap();
        assert!(repo.index_path().exists());
        repo.lib.categories.push(Category { id: "t1".into(), name: "常用".into(), parent_id: None, color: None, icon: None });
        repo.save().unwrap();

        let again = Repo::open(dir.path()).unwrap();
        assert_eq!(again.lib.categories.len(), 1);
        assert!(again.recovered.is_none());
        assert!(!dir.path().join("software.json.tmp").exists());
    }

    #[test]
    fn corrupt_index_is_restored_from_backup() {
        let dir = tempfile::tempdir().unwrap();
        let mut repo = Repo::open(dir.path()).unwrap();
        repo.lib.categories.push(Category { id: "t1".into(), name: "保留".into(), parent_id: None, color: None, icon: None });
        repo.create_backup().unwrap();
        fs::write(repo.index_path(), b"{ not json").unwrap();

        let repo = Repo::open(dir.path()).unwrap();
        assert!(repo.recovered.as_deref().is_some_and(|name| name.starts_with("software-")));
        assert_eq!(repo.lib.categories[0].name, "保留");
        assert!(dir.path().join("software.json.corrupt").exists());
    }

    #[test]
    fn corrupt_index_without_backup_starts_empty() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(INDEX_FILE), b"oops").unwrap();
        let repo = Repo::open(dir.path()).unwrap();
        assert_eq!(repo.recovered.as_deref(), Some(""));
        assert!(repo.lib.software.is_empty());
    }

    #[test]
    fn backups_rotate_and_restore() {
        let dir = tempfile::tempdir().unwrap();
        let mut repo = Repo::open(dir.path()).unwrap();
        for n in 0..11 {
            repo.lib.categories = vec![Category { id: "t".into(), name: format!("第 {n} 版"), parent_id: None, color: None, icon: None }];
            repo.create_backup().unwrap();
        }
        let backups = repo.backups();
        assert_eq!(backups.len(), MAX_BACKUPS);

        // 最旧的那份还留着的是第 3 版（0、1、2 被轮转掉了）
        let oldest = backups.last().unwrap().name.clone();
        repo.restore_backup(&oldest).unwrap();
        assert_eq!(repo.lib.categories[0].name, "第 3 版");
        assert!(repo.restore_backup("../software.json").is_err());
    }

    #[test]
    fn old_index_with_tags_still_opens() {
        let dir = tempfile::tempdir().unwrap();
        let old = r#"{ "format": 1, "tags": [{ "id": "t", "name": "常用" }],
            "software": [{ "id": "s", "name": "Git", "tagIds": ["t"], "addedAt": "2026-01-01T00:00:00+08:00" }] }"#;
        fs::write(dir.path().join(INDEX_FILE), old).unwrap();
        let mut repo = Repo::open(dir.path()).unwrap();
        assert!(repo.recovered.is_none(), "旧格式不应被当成损坏");
        assert_eq!(repo.lib.software[0].name, "Git");
        assert!(!repo.lib.software[0].starred);
        repo.save().unwrap();
        let saved = fs::read_to_string(repo.index_path()).unwrap();
        assert!(!saved.contains("tagIds") && !saved.contains("\"tags\""));
    }

    #[test]
    fn sanitize_names() {
        assert_eq!(sanitize("a/b:c"), "a_b_c");
        assert_eq!(sanitize("  name. "), "name");
        assert_eq!(sanitize("con"), "con_");
        assert_eq!(sanitize(".hidden"), "_.hidden");
        assert_eq!(sanitize(""), "unnamed");
        assert_eq!(sanitize("微信"), "微信");
    }
}
