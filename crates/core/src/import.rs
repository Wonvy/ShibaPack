//! 导入安装包：识别、算哈希、去重，按存储模式复制进仓库或只记路径。

use crate::{
    identify::{identify, is_installer, normalize_name},
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
};

/// 扫描文件夹时最多往下走几层。
const MAX_SCAN_DEPTH: usize = 6;

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
}

enum Outcome {
    Imported(String),
    Duplicate(String),
}

impl Repo {
    /// 导入一批文件或文件夹。文件夹会被展开成里面的安装包。结束后保存一次索引。
    pub fn import_paths(
        &mut self,
        paths: &[PathBuf],
        mode: Storage,
        hint: &ImportHint,
        on_progress: &mut dyn FnMut(Progress),
    ) -> Result<ImportReport> {
        on_progress(Progress { phase: "collect".into(), current: 0, total: 0, file: String::new() });
        let files = self.collect_files(paths);
        let total = files.len();

        // 已经登记过的文件路径。重新扫描同一个文件夹时不用再算一遍哈希。
        let mut known: HashSet<PathBuf> = self
            .lib
            .software
            .iter()
            .flat_map(|s| &s.versions)
            .flat_map(|v| &v.files)
            .map(|f| self.abs_path(f))
            .collect();

        let mut report = ImportReport::default();
        for (i, file) in files.iter().enumerate() {
            let label = file.file_name().unwrap_or_default().to_string_lossy().to_string();
            on_progress(Progress { phase: "import".into(), current: i + 1, total, file: label });
            let item_path = file.to_string_lossy().to_string();
            let item = match self.import_one(file, mode, hint, &mut known) {
                Ok(Outcome::Imported(id)) => {
                    report.imported += 1;
                    ImportItem { path: item_path, status: "imported".into(), software_id: Some(id), message: None }
                }
                Ok(Outcome::Duplicate(id)) => {
                    report.duplicates += 1;
                    ImportItem { path: item_path, status: "duplicate".into(), software_id: Some(id), message: None }
                }
                Err(err) => {
                    report.failed += 1;
                    ImportItem {
                        path: item_path,
                        status: "failed".into(),
                        software_id: None,
                        message: Some(err.to_string()),
                    }
                }
            };
            report.items.push(item);
        }
        if report.imported > 0 {
            self.save()?;
        }
        on_progress(Progress { phase: "done".into(), current: total, total, file: String::new() });
        Ok(report)
    }

    /// 把传入的路径展开成文件列表：文件原样保留，文件夹里只取安装包。
    fn collect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        let data_dir = self.data_dir();
        let mut files = Vec::new();
        for path in paths {
            if path.is_dir() {
                let walker = walkdir::WalkDir::new(path).max_depth(MAX_SCAN_DEPTH).into_iter().filter_entry(|entry| {
                    // 不进隐藏文件夹，也不进仓库自己的数据目录
                    let hidden = entry.depth() > 0 && entry.file_name().to_string_lossy().starts_with('.');
                    !hidden && !entry.path().starts_with(&data_dir) && entry.file_name() != DATA_DIR
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

    fn import_one(
        &mut self,
        src: &Path,
        mode: Storage,
        hint: &ImportHint,
        known: &mut HashSet<PathBuf>,
    ) -> Result<Outcome> {
        let meta = fs::metadata(src)?;
        if !meta.is_file() {
            return Err(Error::Code("not_a_file"));
        }
        let abs = clean_path(&fs::canonicalize(src)?);
        let file_name = abs.file_name().unwrap_or_default().to_string_lossy().to_string();
        if known.contains(&abs) {
            let owner = self.owner_of(&abs).unwrap_or_default();
            return Ok(Outcome::Duplicate(owner));
        }

        let mut id = identify(&file_name);
        if let Some(name) = hint.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
            id.name = name.to_string();
        }
        if let Some(version) = hint.version.as_deref().map(str::trim) {
            id.version = version.to_string();
        }
        if let Some(platform) = hint.platform {
            id.platform = platform;
        }
        let sha256 = sha256_file(&abs)?;

        // 同一个软件、同一个版本里已经有内容相同的文件，就是重复
        let key = normalize_name(&id.name);
        let software_at = self.lib.software.iter().position(|s| normalize_name(&s.name) == key);
        let version_at = software_at
            .and_then(|s| self.lib.software[s].versions.iter().position(|v| v.version == id.version));
        if let (Some(s), Some(v)) = (software_at, version_at) {
            if self.lib.software[s].versions[v].files.iter().any(|f| f.sha256 == sha256) {
                return Ok(Outcome::Duplicate(self.lib.software[s].id.clone()));
            }
        }

        let software_name = software_at.map(|s| self.lib.software[s].name.clone()).unwrap_or_else(|| id.name.clone());
        let (storage, path, disk_path) = if let Ok(rel) = abs.strip_prefix(self.root()) {
            // 文件本来就在仓库里：就地登记，不再复制
            (Storage::Managed, to_rel(rel), abs.clone())
        } else if mode == Storage::Managed {
            let dest = self.copy_into_repo(&abs, &software_name, &id.version, &file_name, meta.len(), &sha256)?;
            let rel = dest.strip_prefix(self.root()).map(to_rel).unwrap_or_default();
            (Storage::Managed, rel, dest)
        } else {
            (Storage::Indexed, abs.to_string_lossy().to_string(), abs.clone())
        };

        let added_at = now();
        let file = PackageFile {
            id: new_id(),
            name: disk_path.file_name().unwrap_or_default().to_string_lossy().to_string(),
            platform: id.platform,
            arch: id.arch,
            size: meta.len(),
            sha256,
            storage,
            path,
            added_at: added_at.clone(),
        };

        let s = match software_at {
            Some(s) => s,
            None => {
                self.lib.software.push(Software {
                    id: new_id(),
                    name: id.name,
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
        let v = match version_at {
            Some(v) => v,
            None => {
                software.versions.push(Version {
                    id: new_id(),
                    version: id.version,
                    note: String::new(),
                    added_at,
                    files: Vec::new(),
                });
                software.versions.len() - 1
            }
        };
        software.versions[v].files.push(file);
        let software_id = software.id.clone();
        // 软件还没有图标（或只有联网取的）时，从刚导入的文件里取一个
        if crate::ops::needs_local_icon(&self.lib.software[s]) {
            if let Some(icon) = self.icon_from(&software_id, &disk_path) {
                self.set_local_icon(s, icon);
            }
        }
        known.insert(disk_path);
        known.insert(abs);
        Ok(Outcome::Imported(software_id))
    }

    /// 复制到 `软件名/版本号/原文件名`。先写 `.part` 再改名，中途失败不会留下半个文件。
    fn copy_into_repo(
        &self,
        src: &Path,
        software: &str,
        version: &str,
        file_name: &str,
        size: u64,
        sha256: &str,
    ) -> Result<PathBuf> {
        let version_dir = if version.is_empty() { "unknown".to_string() } else { sanitize(version) };
        let dir = self.root().join(sanitize(software)).join(version_dir);
        fs::create_dir_all(&dir)?;
        let mut dest = dir.join(file_name);
        if dest.exists() {
            // 仓库里已经有一模一样的文件（比如之前删了记录但没删文件）：直接用它
            let same = fs::metadata(&dest).map(|m| m.len() == size).unwrap_or(false)
                && sha256_file(&dest).map(|h| h == sha256).unwrap_or(false);
            if same {
                return Ok(dest);
            }
            dest = unique_path(dest);
        }
        let part = dest.with_file_name(format!("{}.part", dest.file_name().unwrap_or_default().to_string_lossy()));
        if let Err(err) = fs::copy(src, &part).and_then(|_| fs::rename(&part, &dest)) {
            let _ = fs::remove_file(&part);
            return Err(err.into());
        }
        Ok(dest)
    }

    fn owner_of(&self, abs: &Path) -> Option<String> {
        self.lib.software.iter().find_map(|s| {
            s.versions.iter().flat_map(|v| &v.files).any(|f| self.abs_path(f) == abs).then(|| s.id.clone())
        })
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
