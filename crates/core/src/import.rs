//! 导入安装包：识别、算哈希、去重，按存储模式复制进仓库或只记路径。

use crate::{
    identify::{identify, is_installer, normalize_name, Identity},
    model::*,
    new_id, now,
    store::{clean_path, sanitize, unique_path, Repo, DATA_DIR},
    Error, Result,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// 扫描文件夹时最多往下走几层。
const MAX_SCAN_DEPTH: usize = 6;
/// 导入时每新登记这么多个文件，或者离上次保存过了这么久，就先存一次索引。
const SAVE_EVERY_FILES: usize = 20;
const SAVE_EVERY: Duration = Duration::from_secs(30);

/// 导入时手动指定的信息，覆盖从文件名识别的结果。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportHint {
    pub name: Option<String>,
    pub version: Option<String>,
    pub platform: Option<Platform>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// collect：正在找文件；import：正在处理第 `current` 个；done：结束。
    pub phase: String,
    pub current: usize,
    pub total: usize,
    pub file: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportItem {
    pub path: String,
    /// imported / duplicate / failed
    pub status: String,
    pub software_id: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub imported: usize,
    pub duplicates: usize,
    pub failed: usize,
    pub items: Vec<ImportItem>,
    /// 用户中途取消了：后面的文件没有处理。
    pub cancelled: bool,
}

/// 一个文件导入后的结果：登记好了，或者是重复的。两种都带着对应软件的 id。
pub enum Outcome {
    Imported(String),
    Duplicate(String),
}

/// 一个准备导入的文件，最慢的一步（算哈希）已经做完。这一步只读文件本身，不碰索引。
pub struct Hashed {
    /// 文件的绝对路径（去掉了符号链接）。
    pub abs: PathBuf,
    pub file_name: String,
    pub size: u64,
    pub sha256: String,
}

/// 读一个待导入的文件、算哈希。不需要占着仓库，可以在别的操作照常进行时做。
pub fn hash_source(src: &Path) -> Result<Hashed> {
    let meta = fs::metadata(src)?;
    if !meta.is_file() {
        return Err(Error::Code("not_a_file"));
    }
    let abs = clean_path(&fs::canonicalize(src)?);
    let file_name = abs.file_name().unwrap_or_default().to_string_lossy().to_string();
    let sha256 = sha256_file(&abs)?;
    Ok(Hashed { abs, file_name, size: meta.len(), sha256 })
}

/// 导入一个文件前做的决定：是重复的（带着已有软件的 id），或者要登记成什么、放在哪。
pub enum Plan {
    Duplicate(String),
    Add(Placement),
}

pub struct Placement {
    /// 从文件名认出（再按手动指定的信息改过）的软件名、版本、平台、架构。
    identity: Identity,
    /// 放在哪个文件夹下：仓库里已经有这个软件时用它现有的名字（可能被改过）。
    folder: String,
    target: Target,
}

enum Target {
    /// 文件本来就在仓库里：就地登记，值是相对路径。
    InRepo(String),
    /// 复制（或移动）进仓库。
    Managed,
    /// 只记录位置。
    Indexed,
}

/// 文件放好之后的样子：存储方式、写进索引的路径、磁盘上的实际位置、是不是这次新放进去的。
pub struct Placed {
    storage: Storage,
    path: String,
    disk: PathBuf,
    created: bool,
}

/// 把展开好的文件列表收集出来：文件原样保留，文件夹里只取安装包。不进隐藏文件夹，也不进仓库自己的数据目录。
pub fn collect_files(paths: &[PathBuf], data_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_dir() {
            let walker = walkdir::WalkDir::new(path).max_depth(MAX_SCAN_DEPTH).into_iter().filter_entry(|entry| {
                let hidden = entry.depth() > 0 && entry.file_name().to_string_lossy().starts_with('.');
                !hidden && !entry.path().starts_with(data_dir) && entry.file_name() != DATA_DIR
            });
            for entry in walker.flatten() {
                if entry.file_type().is_file() && is_installer(&entry.file_name().to_string_lossy()) {
                    files.push(entry.into_path());
                }
            }
        } else {
            files.push(path.clone());
        }
    }
    files.sort();
    files.dedup();
    files
}

/// 复制（或移动）进仓库的 `软件名/版本号/原文件名`。复制时先写 `.part` 再改名，中途失败不会留下半个文件。
/// 不碰索引，只需要仓库根目录，可以在不占着仓库的情况下做。
pub fn place_file(root: &Path, hashed: &Hashed, placement: &Placement, move_file: bool) -> Result<Placed> {
    let (src, size, sha256) = (&hashed.abs, hashed.size, hashed.sha256.as_str());
    match &placement.target {
        Target::InRepo(rel) => return Ok(Placed { storage: Storage::Managed, path: rel.clone(), disk: src.clone(), created: false }),
        Target::Indexed => {
            return Ok(Placed { storage: Storage::Indexed, path: src.to_string_lossy().to_string(), disk: src.clone(), created: false })
        }
        Target::Managed => {}
    }
    let version = &placement.identity.version;
    let version_dir = if version.is_empty() { "unknown".to_string() } else { sanitize(version) };
    let dir = root.join(sanitize(&placement.folder)).join(version_dir);
    fs::create_dir_all(&dir)?;
    let mut dest = dir.join(&hashed.file_name);
    let managed = |dest: PathBuf, created: bool| {
        let path = dest.strip_prefix(root).map(to_rel).unwrap_or_default();
        Placed { storage: Storage::Managed, path, disk: dest, created }
    };
    if dest.exists() {
        // 仓库里已经有一模一样的文件（比如之前删了记录但没删文件）：直接用它
        let same = fs::metadata(&dest).map(|m| m.len() == size).unwrap_or(false) && sha256_file(&dest).map(|h| h == sha256).unwrap_or(false);
        if same {
            return Ok(managed(dest, false));
        }
        dest = unique_path(dest);
    }
    // 同一个磁盘上：直接改名，一步完成，不会有半个文件
    if move_file && fs::rename(src, &dest).is_ok() {
        return Ok(managed(dest, true));
    }
    let part = dest.with_file_name(format!("{}.part", dest.file_name().unwrap_or_default().to_string_lossy()));
    let copied = (|| -> std::io::Result<()> {
        fs::copy(src, &part)?;
        fs::File::open(&part)?.sync_all()?;
        fs::rename(&part, &dest)
    })();
    if let Err(err) = copied {
        let _ = fs::remove_file(&part);
        return Err(err.into());
    }
    if move_file {
        // 跨磁盘移动：确认仓库里这份和原文件一模一样，才删原文件。对不上就当没导入，原文件不动
        if fs::metadata(&dest).map(|m| m.len()).ok() != Some(size) || sha256_file(&dest)? != sha256 {
            let _ = fs::remove_file(&dest);
            return Err(Error::Code("copy_mismatch"));
        }
        // 删不掉（比如原文件在只读的磁盘映像或光盘上）就留着，文件已经安全地在仓库里了
        let _ = fs::remove_file(src);
    }
    Ok(managed(dest, true))
}

impl Repo {
    /// 导入一批文件或文件夹（托管时复制进仓库）。见 `import_paths_with`。
    pub fn import_paths(
        &mut self,
        paths: &[PathBuf],
        mode: Storage,
        hint: &ImportHint,
        on_progress: &mut dyn FnMut(Progress),
    ) -> Result<ImportReport> {
        self.import_paths_with(paths, mode, false, hint, on_progress)
    }

    /// 导入一批文件或文件夹。文件夹会被展开成里面的安装包。分批保存索引。
    ///
    /// `move_files` 只在托管模式下起作用：放进仓库后原文件就不留了。同一个磁盘上直接移动；
    /// 跨磁盘先复制，核对内容一模一样再删原文件。认出是重复的文件不动，原样留在原处。
    ///
    /// 这里一直占着仓库。桌面应用里导入大批文件时不用它，而是把 `hash_source`、`plan_import`、
    /// `place_file`、`register` 分开调用，只在第二、四步短暂地占着仓库。
    pub fn import_paths_with(
        &mut self,
        paths: &[PathBuf],
        mode: Storage,
        move_files: bool,
        hint: &ImportHint,
        on_progress: &mut dyn FnMut(Progress),
    ) -> Result<ImportReport> {
        on_progress(Progress { phase: "collect".into(), current: 0, total: 0, file: String::new() });
        let files = collect_files(paths, &self.data_dir());
        let total = files.len();
        let mut known = self.known_paths();
        let mut report = ImportReport::default();
        let mut saver = BatchSaver::new();
        for (i, file) in files.iter().enumerate() {
            let label = file.file_name().unwrap_or_default().to_string_lossy().to_string();
            on_progress(Progress { phase: "import".into(), current: i + 1, total, file: label });
            let outcome = self.import_one(file, mode, move_files, hint, &mut known);
            report.add(file, outcome);
            saver.maybe_save(self, report.imported)?;
        }
        saver.finish(self, report.imported)?;
        on_progress(Progress { phase: "done".into(), current: total, total, file: String::new() });
        Ok(report)
    }

    /// 已经登记过的文件路径。重新扫描同一个文件夹时，这些文件不用再算一遍哈希。
    pub fn known_paths(&self) -> HashSet<PathBuf> {
        self.lib.software.iter().flat_map(|s| &s.versions).flat_map(|v| &v.files).map(|f| self.abs_path(f)).collect()
    }

    /// `known` 里有这个文件时，它已经登记过：返回它所属软件的 id。
    pub fn already_known(&self, src: &Path, known: &HashSet<PathBuf>) -> Option<String> {
        let abs = clean_path(&fs::canonicalize(src).ok()?);
        known.contains(&abs).then(|| self.owner_of(&abs).unwrap_or_default())
    }

    fn import_one(&mut self, src: &Path, mode: Storage, move_files: bool, hint: &ImportHint, known: &mut HashSet<PathBuf>) -> Result<Outcome> {
        if let Some(owner) = self.already_known(src, known) {
            return Ok(Outcome::Duplicate(owner));
        }
        let hashed = hash_source(src)?;
        match self.plan_import(&hashed, mode, hint) {
            Plan::Duplicate(owner) => Ok(Outcome::Duplicate(owner)),
            Plan::Add(placement) => {
                let placed = place_file(self.root(), &hashed, &placement, move_files)?;
                self.register(&hashed, placement, placed, known)
            }
        }
    }

    /// 认软件名和版本、查重复，决定这个文件登记成什么、放在哪。只读索引。
    pub fn plan_import(&self, hashed: &Hashed, mode: Storage, hint: &ImportHint) -> Plan {
        let mut identity = identify(&hashed.file_name);
        if let Some(name) = hint.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
            identity.name = name.to_string();
        }
        if let Some(version) = hint.version.as_deref().map(str::trim) {
            identity.version = version.to_string();
        }
        if let Some(platform) = hint.platform {
            identity.platform = platform;
        }
        if let Some(owner) = self.duplicate_of(&identity, &hashed.sha256) {
            return Plan::Duplicate(owner);
        }
        let key = normalize_name(&identity.name);
        let folder = self.lib.software.iter().find(|s| normalize_name(&s.name) == key).map_or(identity.name.clone(), |s| s.name.clone());
        let target = if let Ok(rel) = hashed.abs.strip_prefix(self.root()) {
            Target::InRepo(to_rel(rel))
        } else if mode == Storage::Managed {
            Target::Managed
        } else {
            Target::Indexed
        };
        Plan::Add(Placement { identity, folder, target })
    }

    /// 同一个软件、同一个版本里已经有内容相同的文件：返回那个软件的 id。
    fn duplicate_of(&self, identity: &Identity, sha256: &str) -> Option<String> {
        let key = normalize_name(&identity.name);
        let software = self.lib.software.iter().find(|s| normalize_name(&s.name) == key)?;
        let version = software.versions.iter().find(|v| v.version == identity.version)?;
        version.files.iter().any(|f| f.sha256 == sha256).then(|| software.id.clone())
    }

    /// 把放好的文件登记进索引（不保存）。放文件的这段时间里如果别处已经登记了同样的内容，
    /// 就当重复处理，这次新放进仓库的那份删掉（内容已经在仓库里了）。
    pub fn register(&mut self, hashed: &Hashed, placement: Placement, placed: Placed, known: &mut HashSet<PathBuf>) -> Result<Outcome> {
        let Placement { identity, .. } = placement;
        if let Some(owner) = self.duplicate_of(&identity, &hashed.sha256) {
            if placed.created && placed.disk != hashed.abs {
                let _ = fs::remove_file(&placed.disk);
                self.remove_empty_dirs(placed.disk.parent());
            }
            return Ok(Outcome::Duplicate(owner));
        }
        let added_at = now();
        let file = PackageFile {
            id: new_id(),
            name: placed.disk.file_name().unwrap_or_default().to_string_lossy().to_string(),
            platform: identity.platform,
            arch: identity.arch,
            size: hashed.size,
            sha256: hashed.sha256.clone(),
            storage: placed.storage,
            path: placed.path,
            added_at: added_at.clone(),
        };
        let key = normalize_name(&identity.name);
        let s = match self.lib.software.iter().position(|s| normalize_name(&s.name) == key) {
            Some(s) => s,
            None => {
                self.lib.software.push(Software {
                    id: new_id(),
                    name: identity.name,
                    publisher: String::new(),
                    homepage: String::new(),
                    note: String::new(),
                    category_id: None,
                    starred: false,
                    icon: None,
                    default_version_id: None,
                    added_at: added_at.clone(),
                    versions: Vec::new(),
                    subscription: None,
                });
                self.lib.software.len() - 1
            }
        };
        let software = &mut self.lib.software[s];
        let v = match software.versions.iter().position(|v| v.version == identity.version) {
            Some(v) => v,
            None => {
                software.versions.push(Version { id: new_id(), version: identity.version, note: String::new(), added_at, files: Vec::new() });
                software.versions.len() - 1
            }
        };
        software.versions[v].files.push(file);
        let software_id = software.id.clone();
        // 软件还没有图标（或只有联网取的）时，从刚导入的文件里取一个
        if crate::ops::needs_local_icon(&self.lib.software[s]) {
            if let Some(icon) = self.icon_from(&software_id, &placed.disk) {
                self.set_local_icon(s, icon);
            }
        }
        known.insert(placed.disk);
        known.insert(hashed.abs.clone());
        Ok(Outcome::Imported(software_id))
    }

    fn owner_of(&self, abs: &Path) -> Option<String> {
        self.lib.software.iter().find_map(|s| {
            s.versions.iter().flat_map(|v| &v.files).any(|f| self.abs_path(f) == abs).then(|| s.id.clone())
        })
    }
}

impl ImportReport {
    /// 记下一个文件的导入结果。
    pub fn add(&mut self, file: &Path, outcome: Result<Outcome>) {
        let path = file.to_string_lossy().to_string();
        let item = match outcome {
            Ok(Outcome::Imported(id)) => {
                self.imported += 1;
                ImportItem { path, status: "imported".into(), software_id: Some(id), message: None }
            }
            Ok(Outcome::Duplicate(id)) => {
                self.duplicates += 1;
                ImportItem { path, status: "duplicate".into(), software_id: Some(id), message: None }
            }
            Err(err) => {
                self.failed += 1;
                ImportItem { path, status: "failed".into(), software_id: None, message: Some(err.to_string()) }
            }
        };
        self.items.push(item);
    }
}

/// 导入大批文件时分批保存：每新登记 20 个文件或每过 30 秒存一次。
/// 中途崩溃或断电，已经复制进仓库的文件大多也已登记。
pub struct BatchSaver {
    saved: usize,
    last: Instant,
}

impl BatchSaver {
    pub fn new() -> Self {
        Self { saved: 0, last: Instant::now() }
    }

    /// `imported` 是到目前为止新登记的文件数。到了该存的时候就存。
    pub fn maybe_save(&mut self, repo: &mut Repo, imported: usize) -> Result<()> {
        if imported > self.saved && (imported - self.saved >= SAVE_EVERY_FILES || self.last.elapsed() >= SAVE_EVERY) {
            repo.save()?;
            (self.saved, self.last) = (imported, Instant::now());
        }
        Ok(())
    }

    /// 导入结束：还有没存的就存一次。
    pub fn finish(&mut self, repo: &mut Repo, imported: usize) -> Result<()> {
        if imported > self.saved {
            repo.save()?;
            self.saved = imported;
        }
        Ok(())
    }
}

impl Default for BatchSaver {
    fn default() -> Self {
        Self::new()
    }
}

/// 仓库内的相对路径，统一用 `/`，换个系统打开也能用。
fn to_rel(rel: &Path) -> String {
    rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn import(repo: &mut Repo, paths: &[PathBuf], mode: Storage) -> ImportReport {
        repo.import_paths(paths, mode, &ImportHint::default(), &mut |_| {}).unwrap()
    }

    #[test]
    fn managed_import_copies_and_groups() {
        let repo_dir = tempfile::tempdir().unwrap();
        let src_dir = tempfile::tempdir().unwrap();
        let win = src_dir.path().join("Obsidian-1.7.7-x64.exe");
        let mac = src_dir.path().join("Obsidian-1.7.7.dmg");
        let old = src_dir.path().join("Obsidian-1.6.0.dmg");
        fs::write(&win, b"windows build").unwrap();
        fs::write(&mac, b"mac build").unwrap();
        fs::write(&old, b"old mac build").unwrap();

        let mut repo = Repo::open(repo_dir.path()).unwrap();
        let report = import(&mut repo, &[win.clone(), mac, old], Storage::Managed);
        assert_eq!((report.imported, report.duplicates, report.failed), (3, 0, 0));

        assert_eq!(repo.lib.software.len(), 1);
        let software = &repo.lib.software[0];
        assert_eq!(software.name, "Obsidian");
        assert_eq!(software.versions.len(), 2);
        let v177 = software.versions.iter().find(|v| v.version == "1.7.7").unwrap();
        assert_eq!(v177.files.len(), 2);
        let file = v177.files.iter().find(|f| f.platform == Platform::Windows).unwrap();
        assert_eq!(file.path, "Obsidian/1.7.7/Obsidian-1.7.7-x64.exe");
        assert_eq!(file.arch, "x64");
        assert!(repo.abs_path(file).is_file());
        assert!(win.is_file(), "原文件应当保留");

        // 再导入同一个文件：重复，不产生新记录
        let again = import(&mut repo, &[win], Storage::Managed);
        assert_eq!((again.imported, again.duplicates), (0, 1));
        assert_eq!(repo.stats().files, 3);
    }

    #[test]
    fn indexed_import_keeps_file_in_place() {
        let repo_dir = tempfile::tempdir().unwrap();
        let src_dir = tempfile::tempdir().unwrap();
        let file = src_dir.path().join("7-Zip 24.09 x64.exe");
        fs::write(&file, b"7z").unwrap();

        let mut repo = Repo::open(repo_dir.path()).unwrap();
        let report = import(&mut repo, &[file.clone()], Storage::Indexed);
        assert_eq!(report.imported, 1);
        let record = &repo.lib.software[0].versions[0].files[0];
        assert_eq!(record.storage, Storage::Indexed);
        assert_eq!(repo.abs_path(record), clean_path(&fs::canonicalize(&file).unwrap()));
        assert!(!repo.root().join("7-Zip").exists(), "索引模式不应复制文件");

        // 重新扫描：按路径直接判定为已登记
        let again = import(&mut repo, &[file], Storage::Indexed);
        assert_eq!((again.imported, again.duplicates), (0, 1));
    }

    #[test]
    fn scanning_a_folder_takes_only_installers() {
        let repo_dir = tempfile::tempdir().unwrap();
        let src_dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(src_dir.path().join("dev/.cache")).unwrap();
        fs::write(src_dir.path().join("dev/Git-2.47.1-64-bit.exe"), b"git").unwrap();
        fs::write(src_dir.path().join("dev/.cache/hidden-1.0.exe"), b"hidden").unwrap();
        fs::write(src_dir.path().join("readme.txt"), b"text").unwrap();
        fs::write(src_dir.path().join("vlc-3.0.21.dmg"), b"vlc").unwrap();

        let mut repo = Repo::open(repo_dir.path()).unwrap();
        let mut phases = Vec::new();
        let report = repo
            .import_paths(&[src_dir.path().to_path_buf()], Storage::Indexed, &ImportHint::default(), &mut |p| {
                phases.push(p.phase)
            })
            .unwrap();
        assert_eq!(report.imported, 2);
        let mut names: Vec<_> = repo.lib.software.iter().map(|s| s.name.as_str()).collect();
        names.sort();
        assert_eq!(names, ["Git", "vlc"]);
        assert_eq!(phases.first().map(String::as_str), Some("collect"));
        assert_eq!(phases.last().map(String::as_str), Some("done"));
    }

    #[test]
    fn files_already_inside_the_repo_are_registered_in_place() {
        let repo_dir = tempfile::tempdir().unwrap();
        let mut repo = Repo::open(repo_dir.path()).unwrap();
        let inside = repo.root().join("Blender").join("4.3.2");
        fs::create_dir_all(&inside).unwrap();
        fs::write(inside.join("blender-4.3.2-windows-x64.msi"), b"blender").unwrap();

        let root = repo.root().to_path_buf();
        let report = import(&mut repo, &[root], Storage::Indexed);
        assert_eq!(report.imported, 1);
        let record = &repo.lib.software[0].versions[0].files[0];
        assert_eq!(record.storage, Storage::Managed);
        assert_eq!(record.path, "Blender/4.3.2/blender-4.3.2-windows-x64.msi");
    }

    /// 桌面应用里导入是分步做的，复制文件时不占着仓库。这期间别处登记了同样的内容：
    /// 登记时认出是重复的，这次新复制进仓库的那份删掉，不留一个没登记的文件。
    #[test]
    fn registering_after_a_concurrent_import_counts_as_duplicate() {
        let repo_dir = tempfile::tempdir().unwrap();
        let src = tempfile::tempdir().unwrap();
        let mut repo = Repo::open(repo_dir.path()).unwrap();
        let file = src.path().join("Tool-1.0.exe");
        fs::write(&file, b"tool").unwrap();
        let mut known = repo.known_paths();
        let hashed = hash_source(&file).unwrap();
        let Plan::Add(placement) = repo.plan_import(&hashed, Storage::Managed, &ImportHint::default()) else { panic!("应当是新文件") };

        // 复制期间，同样内容的另一个文件被导入了
        let other = src.path().join("Tool-1.0-x64.exe");
        fs::write(&other, b"tool").unwrap();
        assert_eq!(import(&mut repo, &[other], Storage::Managed).imported, 1);

        let placed = place_file(repo.root(), &hashed, &placement, false).unwrap();
        let copy = placed.disk.clone();
        assert!(copy.is_file());
        assert!(matches!(repo.register(&hashed, placement, placed, &mut known).unwrap(), Outcome::Duplicate(_)));
        assert!(!copy.exists(), "多复制的那份应当删掉");
        assert_eq!(repo.lib.software[0].versions[0].files.len(), 1);
    }

    /// 移动进仓库：原文件不留；认出是重复的文件原样留在原处。
    #[test]
    fn move_mode_removes_originals_but_keeps_duplicates() {
        let repo_dir = tempfile::tempdir().unwrap();
        let src = tempfile::tempdir().unwrap();
        let mut repo = Repo::open(repo_dir.path()).unwrap();
        let first = src.path().join("Tool-1.0.exe");
        fs::write(&first, b"tool").unwrap();
        let report = repo.import_paths_with(&[first.clone()], Storage::Managed, true, &ImportHint::default(), &mut |_| {}).unwrap();
        assert_eq!(report.imported, 1);
        assert!(!first.exists(), "原文件应当移走");
        let stored = repo.abs_path(&repo.lib.software[0].versions[0].files[0]);
        assert_eq!(fs::read(&stored).unwrap(), b"tool");

        // 同样内容的文件再导入一次：是重复的，不移动、不删除
        let again = src.path().join("Tool-1.0-x64.exe");
        fs::write(&again, b"tool").unwrap();
        let report = repo.import_paths_with(&[again.clone()], Storage::Managed, true, &ImportHint::default(), &mut |_| {}).unwrap();
        assert_eq!(report.duplicates, 1);
        assert!(again.exists(), "重复的文件留在原处");

        // 只记录位置时不管这个开关，原文件不动
        let indexed = src.path().join("Other-2.0.exe");
        fs::write(&indexed, b"other").unwrap();
        repo.import_paths_with(&[indexed.clone()], Storage::Indexed, true, &ImportHint::default(), &mut |_| {}).unwrap();
        assert!(indexed.exists());
    }

    #[test]
    fn hint_overrides_identification() {
        let repo_dir = tempfile::tempdir().unwrap();
        let src_dir = tempfile::tempdir().unwrap();
        let file = src_dir.path().join("setup.exe");
        fs::write(&file, b"x").unwrap();

        let mut repo = Repo::open(repo_dir.path()).unwrap();
        let hint = ImportHint { name: Some("微信".into()), version: Some("3.9.12".into()), platform: None };
        repo.import_paths(&[file], Storage::Managed, &hint, &mut |_| {}).unwrap();
        let software = &repo.lib.software[0];
        assert_eq!(software.name, "微信");
        assert_eq!(software.versions[0].files[0].path, "微信/3.9.12/setup.exe");
    }
}
