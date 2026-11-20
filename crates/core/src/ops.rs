//! 对索引的修改：软件、版本、文件、分类、收藏、设置。每个操作成功后都会保存索引。

use crate::{
    catalog::CATALOG_FILE,
    fileinfo::{self, InfoCache},
    icons,
    identify::{is_installer, normalize_name, same_software},
    import::{sha256_file, ImportHint, ImportItem, ImportReport},
    installed::InstalledApp,
    model::*,
    store::{sanitize, unique_path, Repo, DATA_DIR, INDEX_FILE},
    webicon::WebIcon,
    new_id, Error, Result,
};
use serde::Deserialize;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

/// 联网取的图标文件名带这个后缀，好和安装包里取的区分开。
const WEB_SUFFIX: &str = "-web";

fn is_web_icon(icon: &str) -> bool {
    Path::new(icon).file_stem().is_some_and(|stem| stem.to_string_lossy().ends_with(WEB_SUFFIX))
}

/// 用软件 id 拼文件名时只留字母、数字、`-`、`_`。自己生成的 id 本来就是这样；
/// 被改过的索引里的 id 也不会拼出 `../` 这种跑到别处的路径。
pub(crate) fn id_stem(id: &str) -> String {
    id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

/// 还没有图标，或者只有联网取的图标。
pub(crate) fn needs_local_icon(software: &Software) -> bool {
    software.icon.as_deref().is_none_or(is_web_icon)
}

/// 联网找图标时用的主页和 GitHub 仓库。
#[derive(Debug, Clone, PartialEq)]
pub struct WebIconQuery {
    pub homepage: String,
    /// `owner/repo`
    pub github: Option<String>,
}

impl WebIconQuery {
    /// 记在“没找到”标记文件里的内容。主页或仓库改了就和它对不上，会重新去找。
    fn key(&self) -> String {
        format!("{}\n{}", self.homepage, self.github.as_deref().unwrap_or(""))
    }
}

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
    pub starred: bool,
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

/// 某个软件的某个版本，批量删除版本时用。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionRef {
    pub software_id: String,
    pub version_id: String,
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
        software.starred = edit.starred;
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
        let file = files[f].clone();
        self.remove_records(&[file], delete_from_disk, |lib| {
            let software = &mut lib.software[s];
            software.versions[v].files.remove(f);
            // 版本里没有文件了，这个版本也就没有意义
            if software.versions[v].files.is_empty() {
                let removed = software.versions.remove(v);
                if software.default_version_id.as_deref() == Some(removed.id.as_str()) {
                    software.default_version_id = None;
                }
            }
        })
    }

    pub fn delete_version(&mut self, software_id: &str, version_id: &str, delete_files: bool) -> Result<Vec<PathBuf>> {
        let s = self.software_at(software_id)?;
        let v = self.version_at(s, version_id)?;
        let files = self.lib.software[s].versions[v].files.clone();
        self.remove_records(&files, delete_files, |lib| {
            let software = &mut lib.software[s];
            let removed = software.versions.remove(v);
            if software.default_version_id.as_deref() == Some(removed.id.as_str()) {
                software.default_version_id = None;
            }
        })
    }

    pub fn delete_software(&mut self, id: &str, delete_files: bool) -> Result<Vec<PathBuf>> {
        self.delete_software_many(&[id.to_string()], delete_files)
    }

    /// 一次删掉多个软件（多选删除），只存一次索引。找不到的 id 跳过。
    pub fn delete_software_many(&mut self, ids: &[String], delete_files: bool) -> Result<Vec<PathBuf>> {
        let wanted: HashSet<&str> = ids.iter().map(String::as_str).collect();
        if !self.lib.software.iter().any(|s| wanted.contains(s.id.as_str())) {
            return Err(Error::Code("not_found"));
        }
        let files: Vec<PackageFile> = self
            .lib
            .software
            .iter()
            .filter(|s| wanted.contains(s.id.as_str()))
            .flat_map(|s| &s.versions)
            .flat_map(|v| v.files.iter().cloned())
            .collect();
        self.remove_records(&files, delete_files, |lib| lib.software.retain(|s| !wanted.contains(s.id.as_str())))
    }

    /// 一次删掉多个版本（清理旧版本），只存一次索引。找不到的跳过；
    /// 删掉的版本正好是默认版本时，改回“始终用最新版本”。
    pub fn delete_versions(&mut self, targets: &[VersionRef], delete_files: bool) -> Result<Vec<PathBuf>> {
        let wanted: HashSet<(&str, &str)> = targets.iter().map(|t| (t.software_id.as_str(), t.version_id.as_str())).collect();
        let files: Vec<PackageFile> = self
            .lib
            .software
            .iter()
            .flat_map(|s| s.versions.iter().filter(|v| wanted.contains(&(s.id.as_str(), v.id.as_str()))))
            .flat_map(|v| v.files.iter().cloned())
            .collect();
        if files.is_empty() {
            return Ok(Vec::new());
        }
        self.remove_records(&files, delete_files, |lib| {
            for software in &mut lib.software {
                let id = software.id.clone();
                software.versions.retain(|v| !wanted.contains(&(id.as_str(), v.id.as_str())));
                if software.default_version_id.as_deref().is_some_and(|d| !software.versions.iter().any(|v| v.id == d)) {
                    software.default_version_id = None;
                }
            }
        })
    }

    /// 删除记录的公共部分，顺序是为了任何一步失败都不丢东西：
    ///
    /// 1. 要删文件时，先把托管文件移进仓库回收站。有一个移不动就全部放回原处、记录也不动，直接报错。
    /// 2. 改记录（`edit`），先备份一份改动前的索引再保存。
    /// 3. 保存失败就把文件放回原处（内存里的记录由 `Repo::transact` 退回）。
    ///
    /// 别的记录还在用的同一个文件不移走。返回需要调用方送进系统回收站的外部文件。
    fn remove_records(&mut self, files: &[PackageFile], delete_from_disk: bool, edit: impl FnOnce(&mut Library)) -> Result<Vec<PathBuf>> {
        let (mut managed, mut external) = (Vec::new(), Vec::new());
        if delete_from_disk {
            let removing: HashSet<&str> = files.iter().map(|f| f.id.as_str()).collect();
            let still_used: HashSet<&str> = self
                .lib
                .software
                .iter()
                .flat_map(|s| &s.versions)
                .flat_map(|v| &v.files)
                .filter(|f| !removing.contains(f.id.as_str()))
                .map(|f| f.path.as_str())
                .collect();
            for file in files.iter().filter(|f| !still_used.contains(f.path.as_str())) {
                match file.storage {
                    Storage::Managed => managed.push(file.path.clone()),
                    Storage::Indexed => external.push(PathBuf::from(&file.path)),
                }
            }
        }
        let moved = self.trash_all(&managed)?;
        edit(&mut self.lib);
        if let Err(err) = self.save_with_backup() {
            self.untrash(&moved);
            return Err(err);
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

    /// 收藏或取消收藏。
    pub fn set_starred(&mut self, software_id: &str, starred: bool) -> Result<()> {
        let s = self.software_at(software_id)?;
        self.lib.software[s].starred = starred;
        self.save()
    }

    /// 一次收藏或取消收藏多个软件，只存一次索引。找不到的 id 跳过。
    pub fn set_starred_many(&mut self, ids: &[String], starred: bool) -> Result<()> {
        let wanted: HashSet<&str> = ids.iter().map(String::as_str).collect();
        for software in self.lib.software.iter_mut().filter(|s| wanted.contains(s.id.as_str())) {
            software.starred = starred;
        }
        self.save()
    }

    /// 把多个软件移到一个分类（`None` 是未分类），只存一次索引。找不到的 id 跳过；分类不存在时报错。
    pub fn move_to_category(&mut self, ids: &[String], category_id: Option<&str>) -> Result<()> {
        if let Some(id) = category_id {
            if !self.lib.categories.iter().any(|c| c.id == id) {
                return Err(Error::Code("not_found"));
            }
        }
        let wanted: HashSet<&str> = ids.iter().map(String::as_str).collect();
        for software in self.lib.software.iter_mut().filter(|s| wanted.contains(s.id.as_str())) {
            software.category_id = category_id.map(String::from);
        }
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
        let path = icons::extract_icon(source, &self.icons_dir(), &id_stem(software_id))?;
        Some(format!("{DATA_DIR}/icons/{}", path.file_name()?.to_string_lossy()))
    }

    /// 给软件换上从安装包里取的图标。原来的图标是联网取的，就把那个文件删掉。
    pub(crate) fn set_local_icon(&mut self, s: usize, icon: String) {
        if let Some(old) = self.lib.software[s].icon.replace(icon) {
            // 只删仓库图标文件夹里联网取的那个文件，索引被改过也不会删到别处
            if is_web_icon(&old) && old.starts_with(&format!("{DATA_DIR}/icons/")) {
                let _ = fs::remove_file(self.root_join(&old));
            }
        }
    }

    /// 用安装包自带的属性补全已经登记的软件（Windows 的 exe、msi 文件属性里的产品版本、公司名）：
    /// 版本号是空的文件（放在 `unknown` 文件夹里的），读到版本号就挪到那个版本，托管的文件跟着挪到对应的文件夹；
    /// 发布者空着的软件补上公司名。
    ///
    /// 读过的按 SHA-256 记在 `.shibapack/cache/fileinfo.json`（读不出来的也记），同一个文件只读一次。
    /// 一次最多真的去读 `limit` 个文件。返回 (有没有改动, 还有没有没读的)。
    pub fn fill_file_info(&mut self, limit: usize) -> Result<(bool, bool)> {
        let cache_dir = self.data_dir().join("cache");
        let mut cache = InfoCache::load(&cache_dir);
        let (mut reads, mut more) = (0, false);
        // (软件 id, 版本 id, 文件 id, 读到的信息)
        let mut found = Vec::new();
        for software in &self.lib.software {
            let wants_publisher = software.publisher.trim().is_empty();
            for version in &software.versions {
                if !version.version.is_empty() && !wants_publisher {
                    continue;
                }
                for file in &version.files {
                    if !fileinfo::has_info(&file.name) || file.sha256.is_empty() {
                        continue;
                    }
                    if !cache.files.contains_key(&file.sha256) && reads >= limit {
                        more = true;
                        continue;
                    }
                    let (info, read) = cache.get_or_read(&file.sha256, &self.abs_path(file));
                    reads += usize::from(read);
                    if let Some(info) = info {
                        found.push((software.id.clone(), version.id.clone(), file.id.clone(), info));
                    }
                }
            }
        }
        if reads > 0 {
            let _ = cache.save(&cache_dir);
        }
        let mut changed = false;
        for (software_id, version_id, file_id, info) in found {
            let Ok(s) = self.software_at(&software_id) else { continue };
            if self.lib.software[s].publisher.trim().is_empty() && !info.company.trim().is_empty() {
                self.lib.software[s].publisher = info.company.trim().to_string();
                changed = true;
            }
            let Ok(v) = self.version_at(s, &version_id) else { continue };
            if self.lib.software[s].versions[v].version.is_empty() && !info.version.is_empty() {
                changed |= self.move_file_to_version(s, v, &file_id, &info.version);
            }
        }
        if changed {
            self.save()?;
        }
        Ok((changed, more))
    }

    /// 把一个文件从它现在的版本挪到另一个版本（没有就新建），托管的文件在磁盘上跟着挪到 `软件名/新版本号/`。
    /// 原来的版本挪空了、也没写备注就删掉。目标版本里已经有内容相同的文件时不挪。返回有没有挪。
    fn move_file_to_version(&mut self, s: usize, v: usize, file_id: &str, version: &str) -> bool {
        let Some(f) = self.lib.software[s].versions[v].files.iter().position(|f| f.id == file_id) else { return false };
        let sha256 = self.lib.software[s].versions[v].files[f].sha256.clone();
        let existing = self.lib.software[s].versions.iter().position(|other| other.version == version);
        if existing.is_some_and(|t| self.lib.software[s].versions[t].files.iter().any(|other| other.sha256 == sha256)) {
            return false;
        }
        // 托管文件先在磁盘上挪：`软件名/unknown/文件` → `软件名/版本号/文件`。挪不动就只改记录，文件留在原处也能找到
        let mut file = self.lib.software[s].versions[v].files.remove(f);
        if file.storage == Storage::Managed {
            let parts: Vec<&str> = file.path.split('/').collect();
            if parts.len() >= 3 && parts[1] == "unknown" {
                let new_rel = format!("{}/{}/{}", parts[0], sanitize(version), parts[2..].join("/"));
                let (from, to) = (self.root_join(&file.path), self.root_join(&new_rel));
                if !to.exists() && to.parent().is_some_and(|dir| fs::create_dir_all(dir).is_ok()) && fs::rename(&from, &to).is_ok() {
                    file.path = new_rel;
                    self.remove_empty_dirs(from.parent());
                }
            }
        }
        let software = &mut self.lib.software[s];
        let t = match existing {
            Some(t) => t,
            None => {
                let added_at = software.versions[v].added_at.clone();
                software.versions.push(Version { id: new_id(), version: version.to_string(), note: String::new(), added_at, files: Vec::new() });
                software.versions.len() - 1
            }
        };
        let target_id = software.versions[t].id.clone();
        software.versions[t].files.push(file);
        if software.versions[v].files.is_empty() && software.versions[v].note.trim().is_empty() {
            let old = software.versions.remove(v);
            if software.default_version_id.as_deref() == Some(old.id.as_str()) {
                software.default_version_id = Some(target_id);
            }
        }
        true
    }

    /// 联网找图标要用的信息。软件已经有图标、没有主页也没有 GitHub 仓库、
    /// 或者上次用同样的信息找过没找到时，返回 None。
    pub fn web_icon_query(&self, software_id: &str) -> Result<Option<WebIconQuery>> {
        let software = &self.lib.software[self.software_at(software_id)?];
        if software.icon.is_some() {
            return Ok(None);
        }
        let github = software
            .subscription
            .as_ref()
            .filter(|sub| sub.source == SourceKind::Github)
            .map(|sub| sub.reference.trim().to_string());
        let query = WebIconQuery { homepage: software.homepage.trim().to_string(), github };
        if query.homepage.is_empty() && query.github.is_none() {
            return Ok(None);
        }
        let marker = self.icons_dir().join(format!("{}.none", id_stem(software_id)));
        if fs::read_to_string(marker).is_ok_and(|asked| asked == query.key()) {
            return Ok(None);
        }
        Ok(Some(query))
    }

    /// 存下联网找到的图标，返回写进 `Software.icon` 的相对路径。没找到时记下这次问过什么，
    /// 主页或仓库没变就不再重复去问。这期间软件已经有了图标（比如刚从安装包里取到），就不换。
    pub fn set_web_icon(&mut self, software_id: &str, query: &WebIconQuery, found: Option<WebIcon>) -> Result<Option<String>> {
        let s = self.software_at(software_id)?;
        if let Some(icon) = &self.lib.software[s].icon {
            return Ok(Some(icon.clone()));
        }
        let dir = self.icons_dir();
        fs::create_dir_all(&dir)?;
        let marker = dir.join(format!("{}.none", id_stem(software_id)));
        let Some(found) = found else {
            fs::write(marker, query.key())?;
            return Ok(None);
        };
        let name = format!("{}{WEB_SUFFIX}.{}", id_stem(software_id), found.ext);
        fs::write(dir.join(&name), &found.bytes)?;
        let _ = fs::remove_file(marker);
        let icon = format!("{DATA_DIR}/icons/{name}");
        self.lib.software[s].icon = Some(icon.clone());
        self.save()?;
        Ok(Some(icon))
    }

    /// 给还没有图标的软件补图标，一次最多处理 `limit` 个。联网取的图标也会被换掉：安装包里的更准。
    ///
    /// 先从它自己的安装包里取；取不到再看这台电脑上是不是装着同一个软件，借用那边的图标。
    /// 试过但取不到的记在 `tried` 里，同一次运行中不再重复尝试。返回 (有没有改动, 还有没有没试过的)。
    pub fn fill_icons(&mut self, limit: usize, tried: &mut HashSet<String>, installed: &[InstalledApp]) -> Result<(bool, bool)> {
        let pending: Vec<usize> = (0..self.lib.software.len())
            .filter(|&s| needs_local_icon(&self.lib.software[s]) && !tried.contains(&self.lib.software[s].id))
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
                .filter_map(|path| crate::import::icon_source(&path))
                .find_map(|path| self.icon_from(&id, &path));
            let borrowed = || {
                let app = installed.iter().find(|app| app.icon.is_some() && same_software(&app.name, &software.name))?;
                self.icon_from(&id, Path::new(app.icon.as_deref()?))
            };
            if let Some(icon) = own.or_else(borrowed) {
                self.set_local_icon(s, icon);
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
            starred: false,
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
        if needs_local_icon(&self.lib.software[s]) {
            if let Some(icon) = self.icon_from(software_id, &dest) {
                self.set_local_icon(s, icon);
            }
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
        // 一个文件挪不动（比如正被占用）只记一笔失败，别的照常整理：已经挪走的文件一定要登记上
        let (mut placed, mut renamed, mut failed) = (Vec::new(), HashSet::new(), Vec::new());
        for path in loose {
            match self.tidy_one(&path) {
                Ok((dest, was_moved)) => {
                    if was_moved {
                        renamed.insert(dest.to_string_lossy().to_string());
                    }
                    placed.push(dest);
                }
                Err(err) => failed.push(ImportItem {
                    path: path.to_string_lossy().to_string(),
                    status: "failed".into(),
                    software_id: None,
                    message: Some(err.to_string()),
                    step: Some("copy".into()),
                    reason: Some(err.reason().to_string()),
                }),
            }
        }
        let mut report = self.import_paths(&placed, Storage::Managed, &ImportHint::default(), &mut |_| {})?;
        // 挪过去之后才认出是重复的（同一个软件同一个版本里已经有内容相同的文件）：这份多余，收进回收站，
        // 不留一个没登记的文件在那儿
        for item in report.items.iter().filter(|item| item.status == "duplicate" && renamed.contains(&item.path)) {
            if let Ok(rel) = Path::new(&item.path).strip_prefix(self.root()) {
                let rel = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
                let _ = self.trash_managed(&rel);
            }
        }
        report.failed += failed.len();
        report.items.extend(failed);
        Ok(report)
    }

    /// 把根目录里的一个安装包挪进它的 `软件名/版本号/` 文件夹。返回它现在的位置，以及是不是这次挪过去的。
    fn tidy_one(&self, path: &Path) -> Result<(PathBuf, bool)> {
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let id = crate::identify::identify(&name);
        let version_dir = if id.version.is_empty() { "unknown".to_string() } else { sanitize(&id.version) };
        // 仓库里已经有这个软件时，放进它现有的文件夹（名字可能被改过）
        let key = normalize_name(&id.name);
        let folder = self.lib.software.iter().find(|s| normalize_name(&s.name) == key).map_or(id.name.clone(), |s| s.name.clone());
        let dir = self.root().join(sanitize(&folder)).join(version_dir);
        fs::create_dir_all(&dir)?;
        let dest = dir.join(&name);
        // 目标位置已经有一个内容相同的文件：根目录这份是多余的，收进回收站。
        // 那一份交给导入去登记：已经登记过的会被当成重复跳过，没登记过的就地登记
        if dest.exists() && sha256_file(&dest).ok() == sha256_file(path).ok() {
            self.trash_managed(&name)?;
            return Ok((dest, false));
        }
        let dest = unique_path(dest);
        fs::rename(path, &dest)?;
        Ok((dest, true))
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

    /// 存一条简介的译文（`language` 是界面语言）。简介已经不在了就什么都不做。
    pub fn set_summary_translation(&mut self, name: &str, language: &str, text: String) -> Result<()> {
        let Some(summary) = self.lib.summaries.get_mut(name) else { return Ok(()) };
        summary.translations.insert(language.to_string(), text);
        self.save()
    }

    /// 存一条联网取回的简介。`None` 表示查过了但没有，也记下来，免得每次启动都再查。
    /// 一次存好几个简介（让 AI 一批写完时），只存一次索引。
    pub fn set_summaries(&mut self, summaries: Vec<(String, Summary)>) -> Result<()> {
        if summaries.is_empty() {
            return Ok(());
        }
        for (name, summary) in summaries {
            self.lib.summaries.insert(name, summary);
        }
        self.save()
    }

    /// 删掉一个名字的简介（清空自己写的描述时）。之后开着“联网获取简介”的话会再去网上找。
    pub fn clear_summary(&mut self, name: &str) -> Result<()> {
        if self.lib.summaries.remove(name).is_some() {
            self.save()?;
        }
        Ok(())
    }

    pub fn set_summary(&mut self, name: &str, summary: Option<Summary>) -> Result<()> {
        let summary = summary.unwrap_or_else(|| Summary { text: String::new(), source: "none".into(), fetched_at: crate::now(), ..Summary::default() });
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
            starred: software.starred,
            default_version_id: software.default_version_id.clone(),
        }
    }

    #[test]
    fn web_icons_are_saved_and_misses_remembered() {
        let (_r, src, mut repo) = repo_with(&[("Tool-1.0.zip", b"zip")]);
        let id = repo.lib.software[0].id.clone();
        assert!(repo.lib.software[0].icon.is_none());
        assert_eq!(repo.web_icon_query(&id).unwrap(), None, "没有主页也没有 GitHub，不去找");

        let mut edit = edit_of(&repo.lib.software[0]);
        edit.homepage = " https://tool.example ".into();
        repo.update_software(&id, edit.clone()).unwrap();
        let query = repo.web_icon_query(&id).unwrap().expect("有主页就该去找");
        assert_eq!(query.homepage, "https://tool.example");

        // 没找到：同样的主页不再找；主页改了再找
        assert_eq!(repo.set_web_icon(&id, &query, None).unwrap(), None);
        assert_eq!(repo.web_icon_query(&id).unwrap(), None);
        edit.homepage = "https://tool.example/new".into();
        repo.update_software(&id, edit).unwrap();
        let query = repo.web_icon_query(&id).unwrap().expect("主页改了");

        let png = WebIcon { bytes: b"\x89PNG\r\n\x1a\nweb".to_vec(), ext: "png" };
        let icon = repo.set_web_icon(&id, &query, Some(png)).unwrap().unwrap();
        assert!(icon.ends_with(&format!("{id}-web.png")), "{icon}");
        let web_file = repo.root_join(&icon);
        assert!(web_file.is_file());
        assert!(!repo.icons_dir().join(format!("{id}.none")).exists(), "找到后去掉“没找到”标记");
        assert_eq!(repo.web_icon_query(&id).unwrap(), None, "有图标了就不再找");
        assert_eq!(Repo::open(repo.root()).unwrap().lib.software[0].icon.as_deref(), Some(icon.as_str()));

        // 本机装着同一个软件、取得到它的图标时，换掉联网取的
        let local = src.path().join("tool.png");
        fs::write(&local, b"\x89PNG\r\n\x1a\nlocal").unwrap();
        let app = InstalledApp {
            id: "tool".into(),
            name: repo.lib.software[0].name.clone(),
            version: String::new(),
            publisher: String::new(),
            installed_at: String::new(),
            size: None,
            source: "applications".into(),
            location: String::new(),
            uninstallable: false,
            icon: Some(local.to_string_lossy().to_string()),
            icon_source: None,
            uninstall_command: None,
            install_dir: None,
            bundle_id: None,
        };
        let (changed, _) = repo.fill_icons(8, &mut HashSet::new(), &[app]).unwrap();
        assert!(changed);
        let now = repo.lib.software[0].icon.clone().unwrap();
        assert!(now.ends_with(&format!("{id}.png")), "{now}");
        assert_eq!(fs::read(repo.root_join(&now)).unwrap(), b"\x89PNG\r\n\x1a\nlocal");
        assert!(!web_file.exists(), "联网取的旧图标文件要删掉");
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
    fn batch_deletes() {
        let (_r, _s, mut repo) = repo_with(&[
            ("Git-2.47.1.exe", b"git 1"),
            ("Git-2.46.0.exe", b"git 2"),
            ("Git-2.45.2.exe", b"git 3"),
            ("vlc-3.0.21.dmg", b"vlc"),
            ("7z2409-x64.exe", b"7z"),
        ]);
        let git = repo.lib.software.iter().find(|s| s.name == "Git").unwrap().clone();
        assert_eq!(git.versions.len(), 3);
        let old: Vec<&Version> = git.versions.iter().filter(|v| v.version != "2.47.1").collect();
        let mut edit = edit_of(&git);
        edit.default_version_id = Some(old[0].id.clone());
        repo.update_software(&git.id, edit).unwrap();

        // 删掉两个旧版本，其中一个是默认版本；托管文件进回收站
        let paths: Vec<PathBuf> = old.iter().map(|v| repo.abs_path(&v.files[0])).collect();
        let mut targets: Vec<VersionRef> =
            old.iter().map(|v| VersionRef { software_id: git.id.clone(), version_id: v.id.clone() }).collect();
        targets.push(VersionRef { software_id: git.id.clone(), version_id: "missing".into() });
        let external = repo.delete_versions(&targets, true).unwrap();
        assert!(external.is_empty());
        let after = repo.lib.software.iter().find(|s| s.id == git.id).unwrap();
        assert_eq!(after.versions.iter().map(|v| v.version.as_str()).collect::<Vec<_>>(), ["2.47.1"]);
        assert_eq!(after.default_version_id, None, "删掉的默认版本应当改回自动");
        assert!(paths.iter().all(|p| !p.exists()), "旧版本的托管文件应当移走");
        assert!(repo.delete_versions(&[], true).unwrap().is_empty());

        // 一次删两个软件，只留记录不动文件
        let ids: Vec<String> = repo.lib.software.iter().filter(|s| s.name != "Git").map(|s| s.id.clone()).collect();
        assert_eq!(ids.len(), 2);
        let files: Vec<PathBuf> = repo.lib.software.iter().filter(|s| s.name != "Git").map(|s| repo.abs_path(&s.versions[0].files[0])).collect();
        repo.delete_software_many(&ids, false).unwrap();
        assert_eq!(repo.lib.software.len(), 1);
        assert!(files.iter().all(|p| p.exists()), "只删记录时文件留着");
        assert!(repo.delete_software_many(&["nope".into()], false).is_err());
        let reopened = Repo::open(repo.root()).unwrap();
        assert_eq!(reopened.lib.software.len(), 1);
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
    fn categories_clean_up_references_and_stars_stick() {
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

        let id = repo.lib.software[0].id.clone();
        let mut edit = edit_of(&repo.lib.software[0]);
        edit.category_id = Some("c".into());
        edit.starred = true;
        repo.update_software(&id, edit).unwrap();
        assert_eq!(repo.lib.software[0].category_id.as_deref(), Some("c"));

        repo.set_categories(vec![cat("a", None)]).unwrap();
        assert!(repo.lib.software[0].category_id.is_none());
        assert!(repo.lib.software[0].starred, "改分类不应影响收藏");
        repo.set_starred(&id, false).unwrap();
        assert!(!Repo::open(repo.root()).unwrap().lib.software[0].starred);
        assert!(repo.set_starred("nope", true).is_err());
    }

    #[test]
    fn batch_star_and_move_save_once() {
        let (_r, _s, mut repo) = repo_with(&[("Aaa-1.0.exe", b"a"), ("Bbb-1.0.exe", b"b"), ("Ccc-1.0.exe", b"c")]);
        let ids: Vec<String> = repo.lib.software.iter().take(2).map(|s| s.id.clone()).collect();
        repo.set_starred_many(&ids, true).unwrap();
        let category = Category { id: "dev".into(), name: "开发".into(), parent_id: None, color: None, icon: None };
        repo.set_categories(vec![category]).unwrap();
        repo.move_to_category(&ids, Some("dev")).unwrap();
        assert!(repo.move_to_category(&ids, Some("nope")).is_err(), "分类不存在");

        let again = Repo::open(repo.root()).unwrap();
        let flags: Vec<(bool, Option<&str>)> = again.lib.software.iter().map(|s| (s.starred, s.category_id.as_deref())).collect();
        assert_eq!(flags, [(true, Some("dev")), (true, Some("dev")), (false, None)]);

        repo.move_to_category(&ids, None).unwrap();
        assert!(repo.lib.software.iter().all(|s| s.category_id.is_none()));
    }

    /// 根目录里那份挪过去之后才认出是重复的（文件名不同、内容相同）：收进回收站，不留一个没登记的文件。
    #[test]
    fn tidying_trashes_duplicates_it_moved() {
        let (_r, _s, mut repo) = repo_with(&[("Git-2.47.1.exe", b"git")]);
        let root = repo.root().to_path_buf();
        fs::write(root.join("Git-2.47.1-64-bit.exe"), b"git").unwrap();
        let report = repo.tidy_root().unwrap();
        assert_eq!((report.imported, report.duplicates), (0, 1));
        assert!(!root.join("Git-2.47.1-64-bit.exe").exists());
        assert!(!root.join("Git/2.47.1/Git-2.47.1-64-bit.exe").exists(), "没登记的副本不应留在文件夹里");
        assert!(root.join("Git/2.47.1/Git-2.47.1.exe").is_file(), "登记着的那份不能动");
    }

    /// 删除时有一个文件移不动（Windows 上常见：安装包正被占用）：已经移走的放回原处，记录和索引都不变。
    #[test]
    #[cfg(unix)]
    fn delete_is_all_or_nothing_when_a_file_cannot_be_moved() {
        use std::os::unix::fs::PermissionsExt;
        let (_r, _s, mut repo) = repo_with(&[("Aaa-1.0.exe", b"a"), ("Zzz-1.0.exe", b"z")]);
        let ids: Vec<String> = repo.lib.software.iter().map(|s| s.id.clone()).collect();
        let files: Vec<PathBuf> = repo.lib.software.iter().map(|s| repo.abs_path(&s.versions[0].files[0])).collect();
        // 第二个文件所在的文件夹只读：里面的文件挪不出去
        let locked = files[1].parent().unwrap().to_path_buf();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
        let backups_before = repo.backups().len();

        let result = repo.transact(|repo| repo.delete_software_many(&ids, true));
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

        assert!(result.is_err());
        assert!(files.iter().all(|f| f.is_file()), "已经移进回收站的那个应当放回原处");
        assert_eq!(repo.lib.software.len(), 2, "内存里的记录退回原样");
        assert_eq!(Repo::open(repo.root()).unwrap().lib.software.len(), 2, "索引文件没被改动");
        assert_eq!(repo.backups().len(), backups_before, "什么都没删，也就没有多出备份");
    }

    #[test]
    fn deleting_backs_up_first_and_keeps_shared_files() {
        let (_r, _s, mut repo) = repo_with(&[("Tool-1.0.exe", b"tool")]);
        repo.create_backup().unwrap();
        let before = repo.backups().len();
        // 再登记一个记录，指向同一个托管文件
        let mut copy = repo.lib.software[0].clone();
        copy.id = crate::new_id();
        copy.name = "Tool copy".into();
        copy.versions[0].files[0].id = crate::new_id();
        let shared = repo.abs_path(&copy.versions[0].files[0]);
        repo.lib.software.push(copy);
        repo.save().unwrap();

        let first = repo.lib.software[0].id.clone();
        repo.delete_software(&first, true).unwrap();
        assert!(shared.is_file(), "别的记录还在用的文件不能移走");
        assert!(repo.backups().len() > before, "删除前不管隔多久都先备份一次");

        let second = repo.lib.software[0].id.clone();
        repo.delete_software(&second, true).unwrap();
        assert!(!shared.exists(), "最后一个用它的记录删掉后才移进回收站");
    }

    #[test]
    fn file_properties_fix_unknown_versions_once() {
        let repo_dir = tempfile::tempdir().unwrap();
        let src_dir = tempfile::tempdir().unwrap();
        let file = src_dir.path().join("WinThumbsPreloader.msi");
        crate::fileinfo::write_test_msi(&file, "WinThumbsPreloader", "1.1.0.0", "Dmitry Bruhov");
        let mut repo = Repo::open(repo_dir.path()).unwrap();
        // 以前导入的：没读属性，版本号是空的，放在 unknown 里
        let old = ImportHint { version: Some(String::new()), ..Default::default() };
        repo.import_paths(&[file], Storage::Managed, &old, &mut |_| {}).unwrap();
        repo.lib.software[0].publisher.clear();
        let unknown_id = repo.lib.software[0].versions[0].id.clone();
        repo.lib.software[0].default_version_id = Some(unknown_id);
        assert_eq!(repo.lib.software[0].versions[0].files[0].path, "WinThumbsPreloader/unknown/WinThumbsPreloader.msi");

        assert_eq!(repo.fill_file_info(0).unwrap(), (false, true), "一个都不让读时说还有没读的");
        assert_eq!(repo.fill_file_info(10).unwrap(), (true, false));
        let software = &repo.lib.software[0];
        assert_eq!(software.publisher, "Dmitry Bruhov");
        assert_eq!(software.versions.len(), 1, "挪空了的 unknown 版本删掉");
        assert_eq!(software.versions[0].version, "1.1.0");
        assert_eq!(software.default_version_id.as_deref(), Some(software.versions[0].id.as_str()));
        let moved = &software.versions[0].files[0];
        assert_eq!(moved.path, "WinThumbsPreloader/1.1.0/WinThumbsPreloader.msi");
        assert!(repo.abs_path(moved).is_file());
        assert!(!repo.root().join("WinThumbsPreloader/unknown").exists());
        // 记在缓存里：之后不再读
        assert!(repo.data_dir().join("cache/fileinfo.json").is_file());
        assert_eq!(repo.fill_file_info(0).unwrap(), (false, false));
    }
}
