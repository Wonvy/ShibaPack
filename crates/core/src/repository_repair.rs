//! 将丢失的安装包记录重新关联到经验证的现有文件。

use crate::{import::hash_source, store::clean_path, Error, PackageFile, Repo, Result, Storage};
use serde::Deserialize;
use std::{fs, path::{Path, PathBuf}};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairFile {
    pub software_id: String,
    pub version_id: String,
    pub file_id: String,
}

pub struct RelinkCandidate {
    ids: RepairFile,
    original: PackageFile,
    missing_path: PathBuf,
    replacement: PathBuf,
    selected_root: Option<PathBuf>,
}

pub struct VerifiedRelink {
    candidate: RelinkCandidate,
    sha256: String,
}

impl RelinkCandidate {
    /// 读取大文件的时间不占用仓库锁。
    pub fn verify(self) -> Option<VerifiedRelink> {
        if !matches!(self.missing_path.try_exists(), Ok(false)) {
            return None;
        }
        let hashed = hash_source(&self.replacement).ok()?;
        let inside_selection = self.selected_root.as_ref().is_none_or(|root| hashed.abs.starts_with(root));
        let matches = if self.original.sha256.is_empty() {
            self.original.name.eq_ignore_ascii_case(&hashed.file_name)
        } else {
            self.original.sha256.eq_ignore_ascii_case(&hashed.sha256)
        };
        if !inside_selection || !matches || self.original.size != hashed.size {
            return None;
        }
        Some(VerifiedRelink {
            candidate: RelinkCandidate { replacement: hashed.abs, ..self },
            sha256: hashed.sha256,
        })
    }
}

impl Repo {
    /// 单个文件选择完整路径。批量操作保留旧目录下的相对路径。
    pub fn plan_relink(
        &self, files: Vec<RepairFile>, replacement: &Path, old_root: Option<&Path>,
    ) -> Result<Vec<RelinkCandidate>> {
        if !replacement.is_absolute() || old_root.is_some_and(|path| !path.is_absolute())
            || (old_root.is_none() && files.len() != 1)
        {
            return Err(Error::Code("invalid_path"));
        }
        let selected_root = if old_root.is_some() {
            let root = clean_path(&replacement.canonicalize()?);
            if !root.is_dir() {
                return Err(Error::Code("invalid_path"));
            }
            Some(root)
        } else {
            None
        };
        let old_root = old_root.map(|path| {
            path.canonicalize().map(|absolute| clean_path(&absolute)).unwrap_or_else(|_| clean_path(path))
        });
        Ok(files.into_iter().filter_map(|ids| {
            let original = self.lib.software.iter().find(|software| software.id == ids.software_id)?
                .versions.iter().find(|version| version.id == ids.version_id)?
                .files.iter().find(|file| file.id == ids.file_id)?.clone();
            let missing_path = self.abs_path(&original);
            let replacement = match (old_root.as_deref(), &selected_root) {
                (Some(old), Some(new)) => new.join(missing_path.strip_prefix(old).ok()?),
                _ => replacement.to_path_buf(),
            };
            Some(RelinkCandidate { ids, original, missing_path, replacement, selected_root: selected_root.clone() })
        }).collect())
    }

    pub fn apply_relinks(&mut self, files: Vec<VerifiedRelink>) -> Result<Vec<String>> {
        let _writer = self.begin_write()?;
        let root = self.root().to_path_buf();
        let mut applied = Vec::new();
        for VerifiedRelink { candidate, sha256 } in files {
            let RelinkCandidate { ids, original, missing_path, replacement, .. } = candidate;
            let Some(file) = self.lib.software.iter_mut().find(|software| software.id == ids.software_id)
                .and_then(|software| software.versions.iter_mut().find(|version| version.id == ids.version_id))
                .and_then(|version| version.files.iter_mut().find(|file| file.id == ids.file_id))
                .filter(|file| **file == original)
            else { continue };
            if !matches!(missing_path.try_exists(), Ok(false)) || fs::metadata(&replacement).is_err() {
                continue;
            }
            if let Some(relative) = replacement.strip_prefix(&root).ok().filter(|_| file.storage == Storage::Managed) {
                file.path = relative.to_string_lossy().replace('\\', "/");
            } else {
                file.storage = Storage::Indexed;
                file.path = replacement.to_string_lossy().into_owned();
            }
            file.sha256 = sha256;
            applied.push(ids.file_id);
        }
        if !applied.is_empty() {
            self.save_with_backup()?;
        }
        Ok(applied)
    }
}
