//! 仓库：根目录、索引读写、备份与恢复、统计、校验。

use crate::{model::*, Error, Result};
use serde::Serialize;
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    time::{Duration, SystemTime},
};

pub const INDEX_FILE: &str = "software.json";
pub const DATA_DIR: &str = ".shibapack";
/// 最近的备份留这么多份，一份不少。
const MAX_BACKUPS: usize = 8;
/// 比那几份更早的备份，每天留当天最晚的一份，最多留这么多天。忙了一阵也不会把前几天的备份全挤掉。
const MAX_BACKUP_DAYS: usize = 14;
/// 自动备份的最短间隔。连续的小改动不会把 8 份备份都占满。
const BACKUP_INTERVAL: Duration = Duration::from_secs(600);
const TRASH_KEEP: Duration = Duration::from_secs(30 * 24 * 3600);

/// 索引文件的修改时间和大小。和上次读写时不一样，就是被别的程序改过。
type DiskStamp = (SystemTime, u64);

pub struct Repo {
    root: PathBuf,
    pub lib: Library,
    /// 打开时索引损坏并做了恢复：`Some(备份文件名)`；没有可用备份时是 `Some("")`。
    pub recovered: Option<String>,
    /// 最近一次读入或写出索引后，索引文件的样子。
    disk_stamp: Option<DiskStamp>,
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
    ///
    /// 只有内容坏了（不是合法的 JSON，或者结构不对）才算损坏。读不出来（NAS 掉线、文件被别的程序占着）
    /// 和格式比这个版本新，都原样报错、不动原文件，免得拿旧备份盖掉好好的索引。
    pub fn open(root: &Path) -> Result<Repo> {
        fs::create_dir_all(root)?;
        let root = clean_path(&fs::canonicalize(root)?);
        let data = root.join(DATA_DIR);
        let fresh = !data.exists();
        for dir in ["backups", "trash", "cache", "logs"] {
            fs::create_dir_all(data.join(dir))?;
        }
        if fresh {
            hide_on_windows(&data);
        }

        let mut repo = Repo { root, lib: Library::default(), recovered: None, disk_stamp: None };
        let index = repo.index_path();
        if index.exists() {
            match read_library(&index) {
                Ok(lib) => {
                    repo.lib = lib;
                    repo.disk_stamp = stamp_of(&index);
                }
                Err(err @ (Error::Io(_) | Error::Code(_))) => return Err(err),
                Err(Error::Json(_)) => {
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

    /// 把相对路径（用 `/` 分隔）接到仓库根目录上。`..`、盘符、开头的 `/` 这类会跑出仓库的部分直接丢掉：
    /// 索引就算被人改过（或者来自别处），按它删文件、开文件也只会落在仓库里面。
    pub fn root_join(&self, rel: &str) -> PathBuf {
        let mut path = self.root.clone();
        for part in rel.split('/') {
            for component in Path::new(part).components() {
                if let Component::Normal(name) = component {
                    path.push(name);
                }
            }
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

    /// 保存，并且不管离上次备份多久，先把改动前的索引备份一份。用在删除这类没法撤销的改动上。
    pub fn save_with_backup(&mut self) -> Result<()> {
        if self.index_path().exists() {
            self.backup_current()?;
        }
        self.write_index()
    }

    fn write_index(&mut self) -> Result<()> {
        self.lib.format = FORMAT;
        // 临时文件名带上进程号：万一有两个程序同时在存，不会互相写进对方的临时文件
        let tmp = self.root.join(format!("software.json.{}.tmp", std::process::id()));
        let json = serde_json::to_vec_pretty(&self.lib)?;
        let written = (|| -> Result<()> {
            let mut file = fs::File::create(&tmp)?;
            file.write_all(&json)?;
            file.sync_all()?;
            drop(file);
            read_library(&tmp)?;
            Ok(())
        })();
        if let Err(err) = written {
            let _ = fs::remove_file(&tmp);
            return Err(err);
        }
        fs::rename(&tmp, self.index_path())?;
        self.disk_stamp = stamp_of(&self.index_path());
        self.write_catalog();
        Ok(())
    }

    /// 索引文件被别的程序改过（另一个 ShibaPack、另一台电脑、手动编辑）时重新读入，返回是否重新读了。
    ///
    /// 文件暂时读不到（NAS 掉线）或内容坏了时什么都不做，继续用手里这份；格式比这个版本新时报错，
    /// 这时不能再往里写，否则会把新版本记下的内容丢掉。
    pub fn reload_if_changed(&mut self) -> Result<bool> {
        let index = self.index_path();
        let current = stamp_of(&index);
        if current.is_none() || current == self.disk_stamp {
            return Ok(false);
        }
        match read_library(&index) {
            Ok(lib) => {
                self.lib = lib;
                self.disk_stamp = current;
                Ok(true)
            }
            Err(err @ Error::Code(_)) => Err(err),
            Err(_) => Ok(false),
        }
    }

    /// 做一次修改：先读入别处对索引的改动，再执行 `run`。`run` 失败时把内存里的索引退回去，
    /// 免得改了一半的记录在下一次（哪怕是无关的）保存时被写进文件。
    pub fn transact<T>(&mut self, run: impl FnOnce(&mut Repo) -> Result<T>) -> Result<T> {
        self.reload_if_changed()?;
        let before = self.lib.clone();
        let stamp = self.disk_stamp;
        let result = run(self);
        if result.is_err() {
            self.lib = before;
            // 中途已经存过（比如导入时分批保存）：以文件里的为准，那些记录对应的文件已经在仓库里了
            if self.disk_stamp != stamp {
                match read_library(&self.index_path()) {
                    Ok(lib) => self.lib = lib,
                    // 这会儿读不到：下次操作前再读
                    Err(_) => self.disk_stamp = None,
                }
            }
        }
        result
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

    /// 把当前磁盘上的索引存一份备份，再清掉多余的旧备份（见 `stale_backups`）。
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
        for old in stale_backups(&backups) {
            let _ = fs::remove_file(self.backup_dir().join(old));
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
        // 选中的正好是索引本身：把文件复制到自己身上会把它清空
        let index = fs::canonicalize(self.index_path())?;
        if fs::canonicalize(dest).is_ok_and(|dest| dest == index) {
            return Err(Error::Code("export_onto_index"));
        }
        // 先写到旁边的临时文件再改名，中途失败不会留下半个文件
        let name = dest.file_name().ok_or(Error::Code("not_a_file"))?.to_string_lossy().to_string();
        let tmp = dest.with_file_name(format!(".{name}.tmp"));
        if let Err(err) = fs::copy(&index, &tmp).and_then(|_| fs::rename(&tmp, dest)) {
            let _ = fs::remove_file(&tmp);
            return Err(err.into());
        }
        Ok(())
    }

    /// 用外部的一份索引文件替换当前索引（先备份当前的）。
    ///
    /// 只接受看得出是 ShibaPack 索引的文件：随便一个 JSON 对象（比如语言包）按默认值也能读成一份空索引，
    /// 选错文件就会把整个库换没了。
    pub fn import_from(&mut self, src: &Path) -> Result<()> {
        let bytes = fs::read(src)?;
        let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| Error::Code("invalid_backup"))?;
        if value.get("format").is_none() || !value.get("software").is_some_and(serde_json::Value::is_array) {
            return Err(Error::Code("invalid_backup"));
        }
        let lib = parse_library(&bytes).map_err(|err| match err {
            Error::Code(code) => Error::Code(code),
            _ => Error::Code("invalid_backup"),
        })?;
        if !well_formed(&lib) {
            return Err(Error::Code("invalid_backup"));
        }
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
        self.trash_all(&[rel.to_string()]).map(|_| ())
    }

    /// 把一批托管文件移进仓库的回收站，并清掉因此变空的文件夹。
    ///
    /// 要么全部移走，要么都不动：有一个移不动（比如 Windows 上正被别的程序占用）就把已经移走的放回原处，
    /// 再报错。返回每个文件的（原位置，回收站里的位置），之后保存失败时可以用 `untrash` 放回去。
    pub(crate) fn trash_all(&self, rels: &[String]) -> Result<Vec<(PathBuf, PathBuf)>> {
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        let mut moved = Vec::new();
        for rel in rels {
            let src = self.root_join(rel);
            if !src.exists() {
                continue;
            }
            let mut dest = self.trash_dir().join(&stamp);
            dest.push(src.strip_prefix(&self.root).unwrap_or(&src));
            let result = dest.parent().map_or(Ok(()), fs::create_dir_all).and_then(|_| {
                let dest = unique_path(dest);
                fs::rename(&src, &dest).map(|_| dest)
            });
            match result {
                Ok(dest) => moved.push((src, dest)),
                Err(err) => {
                    self.untrash(&moved);
                    return Err(err.into());
                }
            }
        }
        for (src, _) in &moved {
            self.remove_empty_dirs(src.parent());
        }
        Ok(moved)
    }

    /// 把 `trash_all` 移走的文件放回原处。尽力而为：放不回去的留在回收站里，30 天内都还找得到。
    pub(crate) fn untrash(&self, moved: &[(PathBuf, PathBuf)]) {
        for (src, dest) in moved.iter().rev() {
            if let Some(parent) = src.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::rename(dest, src);
        }
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

/// 要清掉的旧备份（`backups` 最新的在前）：最新的 8 份都留着；更早的每天只留当天最晚的一份，最多留 14 天。
fn stale_backups(backups: &[BackupInfo]) -> Vec<&str> {
    let mut days = HashSet::new();
    let mut stale = Vec::new();
    for (i, backup) in backups.iter().enumerate() {
        let day = backup.created_at.get(..10).unwrap_or_default();
        if i < MAX_BACKUPS || (!days.contains(day) && days.len() < MAX_BACKUP_DAYS) {
            days.insert(day);
        } else {
            stale.push(backup.name.as_str());
        }
    }
    stale
}

fn read_library(path: &Path) -> Result<Library> {
    parse_library(&fs::read(path)?)
}

/// 解析索引内容。格式比这个版本新的直接拒绝（`format_too_new`）：新版本可能记了这里不认识的内容，
/// 读进来再存回去就丢了。所以这一步要在完整解析之前做，新版本加的枚举值不会被当成“文件损坏”。
fn parse_library(bytes: &[u8]) -> Result<Library> {
    #[derive(serde::Deserialize)]
    struct Head {
        #[serde(default)]
        format: Option<u32>,
    }
    let head: Head = serde_json::from_slice(bytes)?;
    if head.format.is_some_and(|format| format > FORMAT) {
        return Err(Error::Code("format_too_new"));
    }
    Ok(serde_json::from_slice(bytes)?)
}

fn stamp_of(path: &Path) -> Option<DiskStamp> {
    let meta = fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// 外部来的索引是否可信：记录的 id 只用字母、数字、`-`、`_`（id 会被拿去拼文件名），
/// 托管文件的路径是仓库里的相对路径。
fn well_formed(lib: &Library) -> bool {
    let id_ok = |id: &str| !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    let rel_ok = |path: &str| {
        !path.is_empty()
            && !path.starts_with(['/', '\\'])
            && !path.get(1..2).is_some_and(|c| c == ":")
            && path.split(['/', '\\']).all(|part| part != ".." && part != ".")
    };
    lib.categories.iter().all(|c| id_ok(&c.id))
        && lib.software.iter().all(|s| {
            id_ok(&s.id)
                && s.versions.iter().all(|v| {
                    id_ok(&v.id) && v.files.iter().all(|f| id_ok(&f.id) && (f.storage == Storage::Indexed || rel_ok(&f.path)))
                })
        })
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
    use std::os::windows::process::CommandExt;
    // 不带这个标志，窗口程序里启动命令行工具会闪一下黑色的控制台窗口
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let _ = std::process::Command::new("attrib").arg("+h").arg(path).creation_flags(CREATE_NO_WINDOW).status();
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
        let leftovers = fs::read_dir(dir.path()).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".tmp")).count();
        assert_eq!(leftovers, 0, "不应留下临时文件");
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

    fn category(name: &str) -> Category {
        Category { id: crate::new_id(), name: name.into(), parent_id: None, color: None, icon: None }
    }

    /// 读不出来（这里用“索引是个文件夹”模拟 NAS 掉线、文件被占用）不是损坏：报错，原文件不动，也不去恢复。
    #[test]
    fn unreadable_index_is_not_treated_as_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        Repo::open(dir.path()).unwrap().create_backup().unwrap();
        fs::remove_file(dir.path().join(INDEX_FILE)).unwrap();
        fs::create_dir(dir.path().join(INDEX_FILE)).unwrap();
        assert!(matches!(Repo::open(dir.path()), Err(Error::Io(_))));
        assert!(dir.path().join(INDEX_FILE).is_dir(), "不应被改名");
        assert!(!dir.path().join("software.json.corrupt").exists());
    }

    /// 新版本写的索引（哪怕带着这个版本不认识的平台）：拒绝打开，不当成损坏，不拿备份盖掉它。
    #[test]
    fn newer_format_is_refused_untouched() {
        let dir = tempfile::tempdir().unwrap();
        Repo::open(dir.path()).unwrap().create_backup().unwrap();
        let newer = r#"{ "format": 99, "software": [{ "id": "s", "name": "X", "addedAt": "",
            "versions": [{ "id": "v", "version": "1", "files": [{ "id": "f", "name": "x", "platform": "newos", "size": 1, "sha256": "", "path": "x", "addedAt": "" }] }] }] }"#;
        fs::write(dir.path().join(INDEX_FILE), newer).unwrap();
        assert!(matches!(Repo::open(dir.path()), Err(Error::Code("format_too_new"))));
        assert_eq!(fs::read_to_string(dir.path().join(INDEX_FILE)).unwrap(), newer);
        assert!(!dir.path().join("software.json.corrupt").exists());
    }

    #[test]
    fn export_onto_the_index_itself_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mut repo = Repo::open(dir.path()).unwrap();
        repo.lib.categories.push(category("保留"));
        repo.save().unwrap();
        let index = repo.index_path();
        assert!(matches!(repo.export_to(&index), Err(Error::Code("export_onto_index"))));
        assert!(Repo::open(dir.path()).unwrap().lib.categories.len() == 1, "索引不能被清空");

        let out = tempfile::tempdir().unwrap();
        repo.export_to(&out.path().join("copy.json")).unwrap();
        assert_eq!(fs::read(out.path().join("copy.json")).unwrap(), fs::read(&index).unwrap());
        assert_eq!(fs::read_dir(out.path()).unwrap().count(), 1, "不应留下临时文件");
    }

    #[test]
    fn import_accepts_only_real_indexes() {
        let dir = tempfile::tempdir().unwrap();
        let mut repo = Repo::open(dir.path()).unwrap();
        repo.lib.categories.push(category("保留"));
        repo.save().unwrap();
        let other = tempfile::tempdir().unwrap();
        let try_import = |repo: &mut Repo, body: &str| {
            let path = other.path().join("x.json");
            fs::write(&path, body).unwrap();
            repo.import_from(&path)
        };
        // 语言包、随便一个 JSON 对象：不能被当成一份空索引换进来
        assert!(try_import(&mut repo, r#"{ "_locale": "fr", "nav.all": "Tout" }"#).is_err());
        assert!(try_import(&mut repo, "{}").is_err());
        // 托管文件的路径跑出仓库、id 能拼出路径：不接受
        let escaping = r#"{ "format": 1, "software": [{ "id": "s", "name": "X", "addedAt": "", "versions": [{ "id": "v", "version": "1",
            "files": [{ "id": "f", "name": "x", "platform": "windows", "size": 1, "sha256": "", "storage": "managed", "path": "../../Documents/x", "addedAt": "" }] }] }] }"#;
        assert!(try_import(&mut repo, escaping).is_err());
        assert!(try_import(&mut repo, r#"{ "format": 1, "software": [{ "id": "../../x", "name": "X", "addedAt": "" }] }"#).is_err());
        assert_eq!(repo.lib.categories[0].name, "保留", "被拒绝的导入不应改动当前索引");

        assert!(try_import(&mut repo, r#"{ "format": 1, "software": [], "categories": [] }"#).is_ok());
        assert!(repo.lib.categories.is_empty());
    }

    #[test]
    fn root_join_stays_inside_the_repo() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repo::open(dir.path()).unwrap();
        for rel in ["../../etc/passwd", "/etc/passwd", "a/../../b", "./a/./b", r"C:\Windows"] {
            let path = repo.root_join(rel);
            assert!(path.starts_with(repo.root()) && path != repo.root(), "{rel} -> {path:?}");
            assert!(!path.components().any(|c| c == Component::ParentDir), "{rel} -> {path:?}");
        }
        assert_eq!(repo.root_join("Git/2.47.1/Git-2.47.1-64-bit.exe"), repo.root().join("Git").join("2.47.1").join("Git-2.47.1-64-bit.exe"));
    }

    /// 另一个 ShibaPack（或另一台电脑）改了索引：下次操作前先读进来，而不是拿手里的旧索引盖掉它。
    #[test]
    fn changes_from_elsewhere_are_picked_up_before_saving() {
        let dir = tempfile::tempdir().unwrap();
        let mut here = Repo::open(dir.path()).unwrap();
        let mut there = Repo::open(dir.path()).unwrap();
        there.lib.categories.push(category("别处加的"));
        there.save().unwrap();

        here.transact(|repo| {
            repo.lib.categories.push(category("这里加的"));
            repo.save()
        })
        .unwrap();
        let names: Vec<String> = Repo::open(dir.path()).unwrap().lib.categories.into_iter().map(|c| c.name).collect();
        assert_eq!(names, ["别处加的", "这里加的"]);
        assert!(!here.reload_if_changed().unwrap(), "自己写的不算别处的改动");
    }

    #[test]
    fn failed_transaction_leaves_memory_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let mut repo = Repo::open(dir.path()).unwrap();
        let result: Result<()> = repo.transact(|repo| {
            repo.lib.categories.push(category("改了一半"));
            Err(Error::Code("boom"))
        });
        assert!(result.is_err());
        assert!(repo.lib.categories.is_empty());
        // 下一次无关的保存也不会把那半截改动写进去
        repo.transact(|repo| repo.save()).unwrap();
        assert!(Repo::open(dir.path()).unwrap().lib.categories.is_empty());
    }

    /// 最近 8 份全留；更早的每天留一份，最多 14 天。
    #[test]
    fn old_backups_are_kept_one_per_day() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repo::open(dir.path()).unwrap();
        // 20 天前到 1 天前，每天两份
        for days_ago in 1..=20 {
            let day = chrono::Local::now() - chrono::Duration::days(days_ago);
            for time in ["090000-000", "180000-000"] {
                let name = format!("software-{}-{time}.json", day.format("%Y%m%d"));
                fs::write(repo.backup_dir().join(name), b"{}").unwrap();
            }
        }
        repo.backup_current().unwrap();
        let backups = repo.backups();
        // 最新的 8 份：今天 1 份 + 前 4 天各 2 份（今天、1～3 天前的 2 份、4 天前的 1 份），
        // 再加 9 个更早的日子各 1 份，凑满 14 天
        let days: HashSet<&str> = backups.iter().map(|b| &b.created_at[..10]).collect();
        assert_eq!(days.len(), MAX_BACKUP_DAYS);
        assert_eq!(backups.len(), MAX_BACKUPS + (MAX_BACKUP_DAYS - 5));
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
