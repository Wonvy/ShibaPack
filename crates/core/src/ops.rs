//! 对索引的修改：软件、版本、文件、分类、标签、设置。每个操作成功后都会保存索引。

use crate::{
    catalog::CATALOG_FILE,
    icons,
    identify::{is_installer, normalize_name, same_software},
    import::{sha256_file, ImportHint, ImportReport},
    installed::InstalledApp,
    model::*,
    store::{sanitize, unique_path, Repo, DATA_DIR, INDEX_FILE},
    Error, Result,
};
use serde::Deserialize;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

/// 软件可以在界面上改的字段。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareEdit {
    pub name: String,
    #[serde(default)]
    pub publisher: String,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub category_id: Option<String>,
    #[serde(default)]
    pub tag_ids: Vec<String>,
    #[serde(default)]
    pub default_version_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionEdit {
    pub version: String,
    #[serde(default)]
    pub note: String,
}

impl Repo {
    fn software_at(&self, id: &str) -> Result<usize> {
        self.lib.software.iter().position(|s| s.id == id).ok_or(Error::Code("not_found"))
    }

    fn version_at(&self, s: usize, id: &str) -> Result<usize> {
        self.lib.software[s].versions.iter().position(|v| v.id == id).ok_or(Error::Code("not_found"))
    }

    /// 一个文件记录在磁盘上的绝对路径。
    pub fn file_path(&self, software_id: &str, version_id: &str, file_id: &str) -> Result<PathBuf> {
        let s = self.software_at(software_id)?;
        let v = self.version_at(s, version_id)?;
        let file = self.lib.software[s].versions[v].files.iter().find(|f| f.id == file_id);
        file.map(|f| self.abs_path(f)).ok_or(Error::Code("not_found"))
    }

    pub fn update_software(&mut self, id: &str, edit: SoftwareEdit) -> Result<()> {
        let s = self.software_at(id)?;
        let name = edit.name.trim().to_string();
        if name.is_empty() {
            return Err(Error::Code("invalid_name"));
        }
        let key = normalize_name(&name);
        if self.lib.software.iter().any(|other| other.id != id && normalize_name(&other.name) == key) {
            return Err(Error::Code("name_exists"));
        }
        let category_ok = |c: &String| self.lib.categories.iter().any(|x| &x.id == c);
        let category_id = edit.category_id.filter(category_ok);
        let tag_ids: Vec<String> =
            edit.tag_ids.into_iter().filter(|t| self.lib.tags.iter().any(|x| &x.id == t)).collect();

        let old_name = self.lib.software[s].name.clone();
        if old_name != name {
            // 托管文件的文件夹跟着改名。挪不动的留在原处，路径照旧有效。
            let (from, to) = (sanitize(&old_name), sanitize(&name));
            let moves: Vec<(usize, usize, String)> = self.managed_files(s, |parts| parts[0] == from)
                .into_iter()
                .map(|(v, f, parts)| (v, f, format!("{to}/{}", parts[1..].join("/"))))
                .collect();
            self.apply_moves(s, moves);
        }

        let software = &mut self.lib.software[s];
        software.name = name;
        software.publisher = edit.publisher.trim().to_string();
        software.homepage = edit.homepage.trim().to_string();
        software.note = edit.note;
        software.category_id = category_id;
        software.tag_ids = tag_ids;
        software.default_version_id =
            edit.default_version_id.filter(|v| software.versions.iter().any(|x| &x.id == v));
        self.save()
    }

    pub fn update_version(&mut self, software_id: &str, version_id: &str, edit: VersionEdit) -> Result<()> {
        let s = self.software_at(software_id)?;
        let v = self.version_at(s, version_id)?;
        let version = edit.version.trim().to_string();
        if self.lib.software[s].versions.iter().any(|other| other.id != version_id && other.version == version) {
            return Err(Error::Code("version_exists"));
        }
        let old = self.lib.software[s].versions[v].version.clone();
        if old != version {
            let dir_of = |ver: &str| if ver.is_empty() { "unknown".to_string() } else { sanitize(ver) };
            let (from, to) = (dir_of(&old), dir_of(&version));
            let moves: Vec<(usize, usize, String)> = self
                .managed_files(s, |parts| parts.len() >= 3 && parts[1] == from)
                .into_iter()
                .filter(|(vi, _, _)| *vi == v)
                .map(|(vi, f, parts)| (vi, f, format!("{}/{to}/{}", parts[0], parts[2..].join("/"))))
                .collect();
            self.apply_moves(s, moves);
        }
        let record = &mut self.lib.software[s].versions[v];
        record.version = version;
        record.note = edit.note;
        self.save()
    }

    /// 一个软件下的托管文件里，路径各段满足条件的那些：(版本下标, 文件下标, 路径各段)。
    fn managed_files(&self, s: usize, pick: impl Fn(&[&str]) -> bool) -> Vec<(usize, usize, Vec<String>)> {
        let mut out = Vec::new();
        for (v, version) in self.lib.software[s].versions.iter().enumerate() {
            for (f, file) in version.files.iter().enumerate() {
                if file.storage != Storage::Managed {
                    continue;
                }
                let parts: Vec<&str> = file.path.split('/').collect();
                if parts.len() >= 2 && pick(&parts) {
                    out.push((v, f, parts.into_iter().map(String::from).collect()));
                }
            }
        }
        out
    }

    /// 把托管文件挪到新的相对路径。目标已存在或挪不动时保持原样。
    fn apply_moves(&mut self, s: usize, moves: Vec<(usize, usize, String)>) {
        for (v, f, new_rel) in moves {
            let old_rel = self.lib.software[s].versions[v].files[f].path.clone();
            let (from, to) = (self.root_join(&old_rel), self.root_join(&new_rel));
            if from == to || to.exists() || !from.exists() {
                continue;
            }
            if let Some(parent) = to.parent() {
                if fs::create_dir_all(parent).is_err() {
                    continue;
                }
            }
            if fs::rename(&from, &to).is_ok() {
                self.lib.software[s].versions[v].files[f].path = new_rel;
                self.remove_empty_dirs(from.parent());
            }
        }
    }

    /// 删除一个文件记录。`delete_from_disk` 为真时，托管文件进仓库回收站；
    /// 仓库之外的索引文件由调用方处理，路径通过返回值给出。
    pub fn delete_file(
        &mut self,
        software_id: &str,
        version_id: &str,
        file_id: &str,
        delete_from_disk: bool,
    ) -> Result<Vec<PathBuf>> {
        let s = self.software_at(software_id)?;
        let v = self.version_at(s, version_id)?;
        let files = &self.lib.software[s].versions[v].files;
        let f = files.iter().position(|x| x.id == file_id).ok_or(Error::Code("not_found"))?;
        let file = self.lib.software[s].versions[v].files.remove(f);
        let external = self.discard(&[file], delete_from_disk)?;
        // 版本里没有文件了，这个版本也就没有意义
        if self.lib.software[s].versions[v].files.is_empty() {
            let removed = self.lib.software[s].versions.remove(v);
            if self.lib.software[s].default_version_id.as_deref() == Some(removed.id.as_str()) {
                self.lib.software[s].default_version_id = None;
            }
        }
        self.save()?;
        Ok(external)
    }

    pub fn delete_version(&mut self, software_id: &str, version_id: &str, delete_files: bool) -> Result<Vec<PathBuf>> {
        let s = self.software_at(software_id)?;
        let v = self.version_at(s, version_id)?;
        let removed = self.lib.software[s].versions.remove(v);
        if self.lib.software[s].default_version_id.as_deref() == Some(removed.id.as_str()) {
            self.lib.software[s].default_version_id = None;
        }
        let external = self.discard(&removed.files, delete_files)?;
        self.save()?;
        Ok(external)
    }

    pub fn delete_software(&mut self, id: &str, delete_files: bool) -> Result<Vec<PathBuf>> {
        let s = self.software_at(id)?;
        let removed = self.lib.software.remove(s);
        let files: Vec<PackageFile> = removed.versions.into_iter().flat_map(|v| v.files).collect();
        let external = self.discard(&files, delete_files)?;
        self.save()?;
        Ok(external)
    }

    /// 处理被删记录对应的磁盘文件，返回需要调用方送进系统回收站的外部文件。
    fn discard(&self, files: &[PackageFile], delete_from_disk: bool) -> Result<Vec<PathBuf>> {
        let mut external = Vec::new();
        if !delete_from_disk {
            return Ok(external);
        }
        for file in files {
            match file.storage {
                Storage::Managed => self.trash_managed(&file.path)?,
                Storage::Indexed => external.push(PathBuf::from(&file.path)),
            }
        }
        Ok(external)
    }

    /// 整体替换分类列表（新建、改名、改色、调整顺序、删除都走这里）。
    pub fn set_categories(&mut self, mut categories: Vec<Category>) -> Result<()> {
        let mut seen = HashSet::new();
        for category in &mut categories {
            category.name = category.name.trim().to_string();
            if category.id.is_empty() || category.name.is_empty() || !seen.insert(category.id.clone()) {
                return Err(Error::Code("invalid_name"));
            }
        }
        // 父分类不存在，或者绕成了环：提到顶层
        let parent_of = |list: &[Category], id: &str| -> Option<String> {
            list.iter().find(|c| c.id == id).and_then(|c| c.parent_id.clone())
        };
        for i in 0..categories.len() {
            let Some(parent) = categories[i].parent_id.clone() else { continue };
            let mut valid = seen.contains(&parent) && parent != categories[i].id;
            let mut cursor = Some(parent);
            let mut steps = 0;
            while let Some(current) = cursor {
                if current == categories[i].id || steps > categories.len() {
                    valid = false;
                    break;
                }
                cursor = parent_of(&categories, &current);
                steps += 1;
            }
            if !valid {
                categories[i].parent_id = None;
            }
        }
        for software in &mut self.lib.software {
            if software.category_id.as_ref().is_some_and(|c| !seen.contains(c)) {
                software.category_id = None;
            }
        }
        self.lib.categories = categories;
        self.save()
    }

    /// 整体替换标签列表。被删掉的标签会从所有软件上摘掉。
    pub fn set_tags(&mut self, mut tags: Vec<Tag>) -> Result<()> {
        let mut seen = HashSet::new();
        for tag in &mut tags {
            tag.name = tag.name.trim().to_string();
            if tag.id.is_empty() || tag.name.is_empty() || !seen.insert(tag.id.clone()) {
                return Err(Error::Code("invalid_name"));
            }
        }
        for software in &mut self.lib.software {
            software.tag_ids.retain(|t| seen.contains(t));
        }
        self.lib.tags = tags;
        self.save()
    }

    pub fn set_settings(&mut self, settings: Settings) -> Result<()> {
        self.lib.settings = settings;
        self.save()
    }

    /// 仓库里放软件图标的文件夹。
    pub fn icons_dir(&self) -> PathBuf {
        self.data_dir().join("icons")
    }

    /// 从一个文件里取图标存进仓库，返回可以写进 `Software.icon` 的相对路径。
    pub(crate) fn icon_from(&self, software_id: &str, source: &Path) -> Option<String> {
        let path = icons::extract_icon(source, &self.icons_dir(), software_id)?;
        Some(format!("{DATA_DIR}/icons/{}", path.file_name()?.to_string_lossy()))
    }

    /// 给还没有图标的软件补图标，一次最多处理 `limit` 个。
    ///
    /// 先从它自己的安装包里取；取不到再看这台电脑上是不是装着同一个软件，借用那边的图标。
    /// 试过但取不到的记在 `tried` 里，同一次运行中不再重复尝试。返回 (有没有改动, 还有没有没试过的)。
    pub fn fill_icons(&mut self, limit: usize, tried: &mut HashSet<String>, installed: &[InstalledApp]) -> Result<(bool, bool)> {
        let pending: Vec<usize> = (0..self.lib.software.len())
            .filter(|&s| self.lib.software[s].icon.is_none() && !tried.contains(&self.lib.software[s].id))
            .collect();
        let mut changed = false;
        for &s in pending.iter().take(limit) {
            let software = &self.lib.software[s];
            let id = software.id.clone();
            tried.insert(id.clone());
            let own = software
                .versions
                .iter()
                .flat_map(|v| &v.files)
                .map(|f| self.abs_path(f))
                .filter(|path| path.is_file())
                .find_map(|path| self.icon_from(&id, &path));
            let borrowed = || {
                let app = installed.iter().find(|app| app.icon.is_some() && same_software(&app.name, &software.name))?;
                self.icon_from(&id, Path::new(app.icon.as_deref()?))
            };
            if let Some(icon) = own.or_else(borrowed) {
                self.lib.software[s].icon = Some(icon);
                changed = true;
            }
        }
        if changed {
            self.save()?;
        }
        Ok((changed, pending.len() > limit))
    }

    /// 新建一个还没有任何安装包的软件（从在线源添加时用）。已有同名的就用已有的。返回它的 id。
    pub fn ensure_software(&mut self, name: &str, homepage: &str) -> Result<String> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Code("invalid_name"));
        }
        let key = normalize_name(name);
        if let Some(existing) = self.lib.software.iter().find(|s| normalize_name(&s.name) == key) {
            return Ok(existing.id.clone());
        }
        let id = crate::new_id();
        self.lib.software.push(Software {
            id: id.clone(),
            name: name.to_string(),
            publisher: String::new(),
            homepage: homepage.trim().to_string(),
            note: String::new(),
            category_id: None,
            tag_ids: Vec::new(),
            icon: None,
            default_version_id: None,
            added_at: crate::now(),
            versions: Vec::new(),
            subscription: None,
        });
        Ok(id)
    }

    /// 关联、更换或取消（`None`）一个软件的在线源。
    pub fn set_subscription(&mut self, software_id: &str, subscription: Option<Subscription>) -> Result<()> {
        let s = self.software_at(software_id)?;
        self.lib.software[s].subscription = subscription;
        self.save()
    }

    /// 记下检查更新的结果。不立即保存，批量检查完由调用方保存一次。
    pub fn set_latest(&mut self, software_id: &str, release: Release) {
        if let Some(subscription) = self.lib.software.iter_mut().find(|s| s.id == software_id).and_then(|s| s.subscription.as_mut()) {
            subscription.latest = Some(release);
            subscription.checked_at = crate::now();
        }
    }

    /// 下载用的临时文件夹。
    pub fn downloads_dir(&self) -> PathBuf {
        self.data_dir().join("downloads")
    }

    /// 把下载好的文件收进仓库：挪到 `软件名/版本号/` 下面并登记为托管文件。
    pub fn adopt_download(&mut self, software_id: &str, version: &str, downloaded: &Path, asset: &Asset, sha256: String) -> Result<()> {
        let s = self.software_at(software_id)?;
        let size = fs::metadata(downloaded)?.len();
        let file_name = downloaded.file_name().unwrap_or_default().to_string_lossy().to_string();
        let version_dir = if version.is_empty() { "unknown".to_string() } else { sanitize(version) };
        let dir = self.root().join(sanitize(&self.lib.software[s].name)).join(&version_dir);
        fs::create_dir_all(&dir)?;
        let dest = unique_path(dir.join(&file_name));
        fs::rename(downloaded, &dest)?;
        let rel = dest
            .strip_prefix(self.root())
            .map(|p| p.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"))
            .unwrap_or_default();

        let added_at = crate::now();
        let record = PackageFile {
            id: crate::new_id(),
            name: dest.file_name().unwrap_or_default().to_string_lossy().to_string(),
            platform: asset.platform,
            arch: asset.arch.clone(),
            size,
            sha256,
            storage: Storage::Managed,
            path: rel,
            added_at: added_at.clone(),
        };
        let software = &mut self.lib.software[s];
        let v = match software.versions.iter().position(|v| v.version == version) {
            Some(v) => v,
            None => {
                software.versions.push(Version { id: crate::new_id(), version: version.to_string(), note: String::new(), added_at, files: Vec::new() });
                software.versions.len() - 1
            }
        };
        software.versions[v].files.push(record);
        if self.lib.software[s].icon.is_none() {
            self.lib.software[s].icon = self.icon_from(software_id, &dest);
        }
        self.save()
    }

    /// 整理仓库根目录：把直接丢在根目录里的安装包识别出来，挪进各自的 `软件名/版本号/` 文件夹并登记。
    pub fn tidy_root(&mut self) -> Result<ImportReport> {
        let loose: Vec<PathBuf> = fs::read_dir(self.root())?
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_file())
            .filter(|path| {
                let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                !name.starts_with('.') && name != INDEX_FILE && name != CATALOG_FILE && !name.starts_with("software.json") && is_installer(&name)
            })
            .collect();
        let mut moved = Vec::new();
        for path in loose {
            let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            let id = crate::identify::identify(&name);
            let version_dir = if id.version.is_empty() { "unknown".to_string() } else { sanitize(&id.version) };
            // 仓库里已经有这个软件时，放进它现有的文件夹（名字可能被改过）
            let key = normalize_name(&id.name);
            let folder = self.lib.software.iter().find(|s| normalize_name(&s.name) == key).map_or(id.name.clone(), |s| s.name.clone());
            let dir = self.root().join(sanitize(&folder)).join(version_dir);
            fs::create_dir_all(&dir)?;
            let dest = dir.join(&name);
            // 目标位置已经有一个内容相同的文件：根目录这份是多余的，收进回收站
            if dest.exists() && sha256_file(&dest).ok() == sha256_file(&path).ok() {
                self.trash_managed(&name)?;
                continue;
            }
            let dest = unique_path(dest);
            fs::rename(&path, &dest)?;
            moved.push(dest);
        }
        self.import_paths(&moved, Storage::Managed, &ImportHint::default(), &mut |_| {})
    }

    /// 放语言包的文件夹。
    fn lang_dir(&self) -> PathBuf {
        self.data_dir().join("lang")
    }

    /// 已导入的语言包。每个是一个 JSON 对象：`_locale` 是语言代码，其余是“词条 → 文字”。
    pub fn lang_packs(&self) -> Vec<serde_json::Value> {
        let mut packs: Vec<serde_json::Value> = fs::read_dir(self.lang_dir())
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
            .filter_map(|entry| serde_json::from_slice(&fs::read(entry.path()).ok()?).ok())
            .filter(|pack: &serde_json::Value| pack["_locale"].is_string())
            .collect();
        packs.sort_by(|a, b| a["_locale"].as_str().cmp(&b["_locale"].as_str()));
        packs
    }

    /// 导入一个语言包文件。同一种语言再导入一次会合并：新文件里有的词条覆盖旧的，没有的保留。
    pub fn import_lang_pack(&self, src: &Path) -> Result<String> {
        let pack: serde_json::Value = serde_json::from_slice(&fs::read(src)?).map_err(|_| Error::Code("invalid_lang_pack"))?;
        let locale = pack["_locale"].as_str().unwrap_or("").to_string();
        let valid = (2..=16).contains(&locale.len()) && locale.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
        let Some(entries) = pack.as_object().filter(|_| valid) else { return Err(Error::Code("invalid_lang_pack")) };

        fs::create_dir_all(self.lang_dir())?;
        let path = self.lang_dir().join(format!("{locale}.json"));
        let mut merged = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(&bytes).ok())
            .unwrap_or_default();
        for (key, value) in entries {
            // 只收文字，别的类型一概不要
            if value.is_string() {
                merged.insert(key.clone(), value.clone());
            }
        }
        fs::write(&path, serde_json::to_vec_pretty(&merged)?)?;
        Ok(locale)
    }

    /// 删除一个导入的语言包。
    pub fn remove_lang_pack(&self, locale: &str) -> Result<()> {
        if !locale.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(Error::Code("invalid_lang_pack"));
        }
        let _ = fs::remove_file(self.lang_dir().join(format!("{locale}.json")));
        Ok(())
    }

    /// 存一条联网取回的简介。`None` 表示查过了但没有，也记下来，免得每次启动都再查。
    pub fn set_summary(&mut self, name: &str, summary: Option<Summary>) -> Result<()> {
        let summary = summary.unwrap_or_else(|| Summary { text: String::new(), source: "none".into(), fetched_at: crate::now() });
        self.lib.summaries.insert(name.to_string(), summary);
        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::ImportHint;

    fn repo_with(files: &[(&str, &[u8])]) -> (tempfile::TempDir, tempfile::TempDir, Repo) {
        let repo_dir = tempfile::tempdir().unwrap();
        let src_dir = tempfile::tempdir().unwrap();
        let mut repo = Repo::open(repo_dir.path()).unwrap();
        let paths: Vec<PathBuf> = files
            .iter()
            .map(|(name, body)| {
                let path = src_dir.path().join(name);
                fs::write(&path, body).unwrap();
                path
            })
            .collect();
        repo.import_paths(&paths, Storage::Managed, &ImportHint::default(), &mut |_| {}).unwrap();
        (repo_dir, src_dir, repo)
    }

    fn edit_of(software: &Software) -> SoftwareEdit {
        SoftwareEdit {
            name: software.name.clone(),
            publisher: software.publisher.clone(),
            homepage: software.homepage.clone(),
            note: software.note.clone(),
            category_id: software.category_id.clone(),
            tag_ids: software.tag_ids.clone(),
            default_version_id: software.default_version_id.clone(),
        }
    }

    #[test]
    fn subscriptions_and_downloads() {
        let (_r, src, mut repo) = repo_with(&[("Obsidian-1.7.7.exe", b"old")]);
        let id = repo.ensure_software("obsidian", "").unwrap();
        assert_eq!(id, repo.lib.software[0].id, "同名的软件不应新建第二个");
        let fresh = repo.ensure_software("Brand New", "https://example.com").unwrap();
        assert_eq!(repo.lib.software.len(), 2);
        assert!(repo.ensure_software("  ", "").is_err());

        let asset = Asset { name: "Obsidian-1.15.1.exe".into(), url: "https://x/o.exe".into(), platform: Platform::Windows, arch: "x64".into(), size: None, sha256: None };
        let release = Release { version: "1.15.1".into(), published_at: String::new(), page: String::new(), assets: vec![asset.clone()] };
        repo.set_subscription(&id, Some(Subscription { source: SourceKind::Github, reference: "obsidianmd/obsidian-releases".into(), latest: None, checked_at: String::new() })).unwrap();
        repo.set_latest(&id, release);
        repo.save().unwrap();

        // 下载好的文件收进仓库
        let downloaded = src.path().join("Obsidian-1.15.1.exe");
        fs::write(&downloaded, b"new build").unwrap();
        repo.adopt_download(&id, "1.15.1", &downloaded, &asset, "abc".into()).unwrap();
        assert!(!downloaded.exists());
        let reopened = Repo::open(repo.root()).unwrap();
        let software = reopened.lib.software.iter().find(|s| s.id == id).unwrap();
        assert_eq!(software.subscription.as_ref().unwrap().latest.as_ref().unwrap().version, "1.15.1");
        let version = software.versions.iter().find(|v| v.version == "1.15.1").unwrap();
        assert_eq!(version.files[0].path, "Obsidian/1.15.1/Obsidian-1.15.1.exe");
        assert_eq!((version.files[0].arch.as_str(), version.files[0].size), ("x64", 9));
        assert!(reopened.root().join("Obsidian/1.15.1/Obsidian-1.15.1.exe").is_file());
        assert!(reopened.lib.software.iter().any(|s| s.id == fresh && s.versions.is_empty()));
    }

    #[test]
    fn tidying_the_root_files_loose_installers() {
        let (_r, _s, mut repo) = repo_with(&[("Git-2.47.1.exe", b"git")]);
        let root = repo.root().to_path_buf();
        fs::write(root.join("Git-2.48.0-64-bit.exe"), b"git 2.48").unwrap();
        fs::write(root.join("vlc-3.0.21.dmg"), b"vlc").unwrap();
        fs::write(root.join("Git-2.47.1.exe"), b"git").unwrap(); // 和仓库里已有的一模一样
        fs::write(root.join("notes.txt"), b"not an installer").unwrap();

        let report = repo.tidy_root().unwrap();
        assert_eq!((report.imported, report.failed), (2, 0));
        assert!(root.join("Git/2.48.0/Git-2.48.0-64-bit.exe").is_file());
        assert!(root.join("vlc/3.0.21/vlc-3.0.21.dmg").is_file());
        assert!(root.join("notes.txt").is_file(), "不是安装包的文件不动");
        assert!(!root.join("Git-2.47.1.exe").exists(), "重复的那份应当收进回收站");
        assert!(root.join("software.json").is_file() && root.join("index.html").is_file());
        let git = repo.lib.software.iter().find(|s| s.name == "Git").unwrap();
        assert_eq!(git.versions.len(), 2);
    }

    #[test]
    fn language_packs() {
        let (_r, src, repo) = repo_with(&[]);
        let file = src.path().join("ja.json");
        fs::write(&file, r#"{ "_locale": "ja-JP", "_name": "日本語", "app.add": "追加", "bad": 42 }"#).unwrap();
        assert_eq!(repo.import_lang_pack(&file).unwrap(), "ja-JP");
        // 再导入一次只带一个词条：合并，原有的保留
        fs::write(&file, r#"{ "_locale": "ja-JP", "app.scan": "スキャン" }"#).unwrap();
        repo.import_lang_pack(&file).unwrap();
        let packs = repo.lang_packs();
        assert_eq!(packs.len(), 1);
        assert_eq!((packs[0]["app.add"].as_str(), packs[0]["app.scan"].as_str(), packs[0]["_name"].as_str()), (Some("追加"), Some("スキャン"), Some("日本語")));
        assert!(packs[0].get("bad").is_none());

        fs::write(&file, r#"{ "_locale": "../evil" }"#).unwrap();
        assert!(repo.import_lang_pack(&file).is_err());
        fs::write(&file, "not json").unwrap();
        assert!(repo.import_lang_pack(&file).is_err());
        repo.remove_lang_pack("ja-JP").unwrap();
        assert!(repo.lang_packs().is_empty());
    }

    #[test]
    fn renaming_software_moves_its_folder() {
        let (_r, _s, mut repo) = repo_with(&[("VSCodeUserSetup-x64-1.96.2.exe", b"code")]);
        let id = repo.lib.software[0].id.clone();
        let mut edit = edit_of(&repo.lib.software[0]);
        edit.name = "Visual Studio Code".into();
        repo.update_software(&id, edit).unwrap();

        let file = &repo.lib.software[0].versions[0].files[0];
        assert_eq!(file.path, "Visual Studio Code/1.96.2/VSCodeUserSetup-x64-1.96.2.exe");
        assert!(repo.abs_path(file).is_file());
        assert!(!repo.root().join("VSCode").exists(), "旧文件夹应当被清掉");
    }

    #[test]
    fn renaming_to_an_existing_name_is_rejected() {
        let (_r, _s, mut repo) = repo_with(&[("Git-2.47.1.exe", b"git"), ("Obsidian-1.7.7.exe", b"obs")]);
        let git = repo.lib.software.iter().find(|s| s.name == "Git").unwrap();
        let (id, mut edit) = (git.id.clone(), edit_of(git));
        edit.name = "obsidian".into();
        assert!(matches!(repo.update_software(&id, edit), Err(Error::Code("name_exists"))));
    }

    #[test]
    fn changing_version_moves_files() {
        let (_r, _s, mut repo) = repo_with(&[("tool.exe", b"tool")]);
        let (sid, vid) = (repo.lib.software[0].id.clone(), repo.lib.software[0].versions[0].id.clone());
        assert_eq!(repo.lib.software[0].versions[0].files[0].path, "tool/unknown/tool.exe");
        repo.update_version(&sid, &vid, VersionEdit { version: "2.0".into(), note: String::new() }).unwrap();
        let file = &repo.lib.software[0].versions[0].files[0];
        assert_eq!(file.path, "tool/2.0/tool.exe");
        assert!(repo.abs_path(file).is_file());
    }

    #[test]
    fn deleting_a_file_moves_it_to_trash_and_drops_empty_version() {
        let (_r, _s, mut repo) = repo_with(&[("Git-2.47.1.exe", b"git")]);
        let software = &repo.lib.software[0];
        let (sid, vid, fid) =
            (software.id.clone(), software.versions[0].id.clone(), software.versions[0].files[0].id.clone());
        let on_disk = repo.file_path(&sid, &vid, &fid).unwrap();

        let external = repo.delete_file(&sid, &vid, &fid, true).unwrap();
        assert!(external.is_empty());
        assert!(!on_disk.exists());
        assert!(!repo.root().join("Git").exists());
        assert!(repo.lib.software[0].versions.is_empty());
        assert!(repo.stats().trash_size > 0);

        repo.empty_trash().unwrap();
        assert_eq!(repo.stats().trash_size, 0);
    }

    #[test]
    fn deleting_records_only_keeps_files() {
        let (_r, _s, mut repo) = repo_with(&[("Git-2.47.1.exe", b"git")]);
        let id = repo.lib.software[0].id.clone();
        repo.delete_software(&id, false).unwrap();
        assert!(repo.lib.software.is_empty());
        assert!(repo.root().join("Git/2.47.1/Git-2.47.1.exe").is_file());
    }

    #[test]
    fn deleting_indexed_files_hands_paths_back() {
        let repo_dir = tempfile::tempdir().unwrap();
        let src_dir = tempfile::tempdir().unwrap();
        let file = src_dir.path().join("vlc-3.0.21.dmg");
        fs::write(&file, b"vlc").unwrap();
        let mut repo = Repo::open(repo_dir.path()).unwrap();
        repo.import_paths(&[file.clone()], Storage::Indexed, &ImportHint::default(), &mut |_| {}).unwrap();
        let id = repo.lib.software[0].id.clone();
        let external = repo.delete_software(&id, true).unwrap();
        assert_eq!(external.len(), 1);
        assert!(file.exists(), "仓库外的文件由调用方处理，这里不动");
    }

    #[test]
    fn categories_and_tags_clean_up_references() {
        let (_r, _s, mut repo) = repo_with(&[("Git-2.47.1.exe", b"git")]);
        let cat = |id: &str, parent: Option<&str>| Category {
            id: id.into(),
            name: id.into(),
            parent_id: parent.map(String::from),
            color: None,
            icon: None,
        };
        // b → a → b 是个环，c 的父分类不存在
        repo.set_categories(vec![cat("a", Some("b")), cat("b", Some("a")), cat("c", Some("nope"))]).unwrap();
        assert!(repo.lib.categories.iter().find(|c| c.id == "c").unwrap().parent_id.is_none());
        let cyclic = repo.lib.categories.iter().filter(|c| c.parent_id.is_some()).count();
        assert!(cyclic <= 1, "环必须被打断");

        repo.set_tags(vec![Tag { id: "t".into(), name: "常用".into(), color: None }]).unwrap();
        let id = repo.lib.software[0].id.clone();
        let mut edit = edit_of(&repo.lib.software[0]);
        edit.category_id = Some("c".into());
        edit.tag_ids = vec!["t".into(), "ghost".into()];
        repo.update_software(&id, edit).unwrap();
        assert_eq!(repo.lib.software[0].tag_ids, ["t"]);

        repo.set_categories(vec![cat("a", None)]).unwrap();
        repo.set_tags(vec![]).unwrap();
        assert!(repo.lib.software[0].category_id.is_none());
        assert!(repo.lib.software[0].tag_ids.is_empty());
    }
}
