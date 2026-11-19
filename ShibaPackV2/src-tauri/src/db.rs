use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use uuid::Uuid;

use crate::models::{
    BackupData, BackupInfo, Category, LibraryData, LibraryStats, PackageInfo, Platform,
    ScanResult, Settings, Software, SoftwareFile, SoftwareGroup, SoftwareSummary, SoftwareVersion,
    StorageInfo, Tag,
};

const STORE_VERSION: u32 = 1;
const MAX_BACKUPS: usize = 8;

/// 仓库唯一数据文件：software.json
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
struct StoreData {
    #[serde(default = "default_version")]
    schema_version: u32,
    #[serde(default)]
    settings: Settings,
    #[serde(default)]
    categories: Vec<Category>,
    #[serde(default)]
    tags: Vec<Tag>,
    #[serde(default)]
    software: Vec<Software>,
    /// 摘要缓存（不属于方案核心，但保留以便离线显示）
    #[serde(default)]
    summaries: HashMap<String, SoftwareSummary>,
}

fn default_version() -> u32 {
    STORE_VERSION
}

impl Default for StoreData {
    fn default() -> Self {
        let defaults: [(&str, &str, &str, Option<usize>, i32); 9] = [
            ("开发工具", "#5b8cff", "code", None, 1),
            ("设计创作", "#e56a8f", "palette", None, 2),
            ("办公效率", "#e0a133", "briefcase", None, 3),
            ("网络通信", "#5b8cff", "globe", None, 4),
            ("影音娱乐", "#a25bff", "play", None, 5),
            ("系统工具", "#2fb37b", "settings", None, 6),
            ("安全防护", "#e56a8f", "shield", None, 7),
            ("游戏娱乐", "#a25bff", "gamepad", None, 8),
            ("其他", "#8b8b99", "package", None, 9),
        ];
        let ids: Vec<String> = (0..defaults.len()).map(|_| Uuid::new_v4().to_string()).collect();
        let categories = defaults
            .iter()
            .enumerate()
            .map(|(index, (name, color, icon, parent, order))| Category {
                id: ids[index].clone(),
                name: name.to_string(),
                color: color.to_string(),
                icon: icon.to_string(),
                parent_id: parent.map(|p| ids[p].clone()),
                sort_order: *order,
                package_count: 0,
            })
            .collect();
        Self {
            schema_version: STORE_VERSION,
            settings: Settings::default(),
            categories,
            tags: Vec::new(),
            software: Vec::new(),
            summaries: HashMap::new(),
        }
    }
}

struct PathState {
    path: PathBuf,
    backup_dir: PathBuf,
    repo_root: PathBuf,
}

pub struct Database {
    paths: Mutex<PathState>,
    data: Mutex<StoreData>,
    defer_save: AtomicBool,
}

impl Database {
    pub fn open(path: &Path, repo_root: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let backup_dir = path.parent().unwrap_or(Path::new(".")).join("backups");
        fs::create_dir_all(&backup_dir)?;

        let data = if path.exists() {
            match fs::read_to_string(path)
                .with_context(|| "读取 software.json 失败")
                .and_then(|content| {
                    serde_json::from_str::<StoreData>(&content).context("解析 software.json 失败")
                }) {
                Ok(data) => data,
                Err(_) => {
                    // 损坏时留现场，尝试从最新备份恢复，否则重建
                    let _ = fs::copy(path, path.with_extension("json.corrupt"));
                    Self::load_latest_backup(&backup_dir, path).unwrap_or_else(|_| StoreData::default())
                }
            }
        } else {
            // 兼容迁移：存在旧 data.json 时转换为新结构
            let legacy = path.with_file_name("data.json");
            if legacy.exists() {
                Self::migrate_legacy(&legacy).unwrap_or_else(|_| StoreData::default())
            } else {
                StoreData::default()
            }
        };

        let db = Self {
            paths: Mutex::new(PathState {
                path: path.to_path_buf(),
                backup_dir,
                repo_root: repo_root.to_path_buf(),
            }),
            data: Mutex::new(data),
            defer_save: AtomicBool::new(false),
        };
        db.persist()?;
        Ok(db)
    }

    /// 从最新备份恢复数据文件
    fn load_latest_backup(backup_dir: &Path, target: &Path) -> Result<StoreData> {
        let mut backups: Vec<PathBuf> = fs::read_dir(backup_dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
            .collect();
        backups.sort_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok());
        let latest = backups.pop().context("没有可用备份")?;
        let content = fs::read_to_string(&latest)?;
        let mut data: StoreData = serde_json::from_str(&content)?;
        // 恢复后立刻写回主文件
        fs::write(target, serde_json::to_string_pretty(&data)?)?;
        data.schema_version = STORE_VERSION;
        Ok(data)
    }

    /// 将旧版扁平 data.json 迁移为三级结构
    fn migrate_legacy(path: &Path) -> Result<StoreData> {
        #[derive(Deserialize)]
        struct LegacyStore {
            #[serde(default)]
            version: u32,
            #[serde(default)]
            settings: Settings,
            #[serde(default)]
            categories: Vec<Category>,
            #[serde(default)]
            summaries: HashMap<String, SoftwareSummary>,
            #[serde(default)]
            packages: Vec<PackageInfo>,
        }
        let content = fs::read_to_string(path)?;
        let legacy: LegacyStore = serde_json::from_str(&content)?;
        let now = Utc::now().to_rfc3339();

        // 按软件名聚合为 software -> versions -> files
        let mut by_name: HashMap<String, Vec<PackageInfo>> = HashMap::new();
        for pkg in &legacy.packages {
            by_name
                .entry(pkg.software_name.clone())
                .or_default()
                .push(pkg.clone());
        }
        let software: Vec<Software> = by_name
            .into_iter()
            .map(|(name, mut packages)| {
                packages.sort_by(|a, b| b.added_time.cmp(&a.added_time));
                let software_id = Uuid::new_v4().to_string();
                let category_id = packages
                    .iter()
                    .find(|p| p.category_id.is_some())
                    .and_then(|p| p.category_id.clone());
                let versions = packages
                    .into_iter()
                    .map(|pkg| SoftwareVersion {
                        id: Uuid::new_v4().to_string(),
                        version: pkg.version.clone(),
                        release_date: None,
                        is_default: false,
                        created_at: pkg.added_time.clone(),
                        files: vec![SoftwareFile {
                            id: Uuid::new_v4().to_string(),
                            name: pkg.file_name.clone(),
                            path: pkg.full_path.clone(),
                            format: pkg.extension.trim_start_matches('.').to_string(),
                            size: pkg.file_size,
                            sha256: String::new(),
                            role: "installer".to_string(),
                            platform: pkg.platform,
                            added_at: pkg.added_time.clone(),
                        }],
                    })
                    .collect::<Vec<_>>();
                // 最高版本设为默认（按版本字符串粗略比较）
                let mut versions = versions;
                versions.sort_by(|a, b| b.version.cmp(&a.version));
                if let Some(first) = versions.first_mut() {
                    first.is_default = true;
                }
                Software {
                    id: software_id,
                    name,
                    icon: String::new(),
                    category_id,
                    tags: Vec::new(),
                    description: String::new(),
                    created_at: now.clone(),
                    updated_at: now.clone(),
                    versions,
                }
            })
            .collect();

        // 迁移后保留旧文件，避免误以为丢失
        let _ = fs::rename(path, path.with_extension("json.migrated"));

        Ok(StoreData {
            schema_version: STORE_VERSION,
            settings: legacy.settings,
            categories: legacy.categories,
            tags: Vec::new(),
            software,
            summaries: legacy.summaries,
        })
    }

    fn data(&self) -> std::sync::MutexGuard<'_, StoreData> {
        self.data.lock().expect("data mutex poisoned")
    }

    fn paths(&self) -> std::sync::MutexGuard<'_, PathState> {
        self.paths.lock().expect("paths mutex poisoned")
    }

    /// 写入临时文件 → 校验 JSON → 备份轮转 → 原子替换
    fn persist(&self) -> Result<()> {
        let json = {
            let data = self.data();
            serde_json::to_string_pretty(&*data)?
        };
        serde_json::from_str::<StoreData>(&json).context("临时文件 JSON 校验失败")?;

        let (path, backup_dir) = {
            let paths = self.paths();
            (paths.path.clone(), paths.backup_dir.clone())
        };

        // 写临时文件并校验格式，避免写坏主文件
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, &json).with_context(|| "写入临时数据文件失败")?;

        // 备份轮转：写入前把当前有效文件保存为备份
        if path.exists() {
            let stamp = Utc::now().format("%Y%m%d%H%M%S%3f");
            let backup = backup_dir.join(format!("software-{}.json", stamp));
            let _ = fs::copy(&path, &backup);
            Self::rotate_backups(&backup_dir);
        }

        if fs::rename(&tmp, &path).is_err() {
            fs::write(&path, &json).with_context(|| "写入数据文件失败")?;
            let _ = fs::remove_file(&tmp);
        }
        Ok(())
    }

    fn rotate_backups(backup_dir: &Path) {
        let mut backups: Vec<PathBuf> = fs::read_dir(backup_dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
                    .collect()
            })
            .unwrap_or_default();
        backups.sort();
        while backups.len() > MAX_BACKUPS {
            if let Some(old) = backups.first() {
                let _ = fs::remove_file(old);
                backups.remove(0);
            } else {
                break;
            }
        }
    }

    fn flush(&self) -> Result<()> {
        if self.defer_save.load(Ordering::Relaxed) {
            return Ok(());
        }
        self.persist()
    }

    pub fn begin_batch(&self) {
        self.defer_save.store(true, Ordering::Relaxed);
    }

    pub fn end_batch(&self) -> Result<()> {
        self.defer_save.store(false, Ordering::Relaxed);
        self.persist()
    }

    pub fn repo_root(&self) -> PathBuf {
        self.paths().repo_root.clone()
    }

    /// 切换数据文件与仓库根路径（仓库迁移后调用，避免重启才生效）
    pub fn relocate(&self, new_store: &Path, new_repo_root: &Path) -> Result<()> {
        {
            let mut paths = self.paths();
            paths.path = new_store.to_path_buf();
            paths.repo_root = new_repo_root.to_path_buf();
            paths.backup_dir = new_store
                .parent()
                .unwrap_or(Path::new("."))
                .join("backups");
            fs::create_dir_all(&paths.backup_dir)?;
        }
        self.persist()
    }

    // ---------- 摘要缓存 ----------
    pub fn get_summary(&self, name: &str) -> Result<Option<SoftwareSummary>> {
        Ok(self.data().summaries.get(&name.to_lowercase()).cloned())
    }

    pub fn save_summary(&self, summary: &SoftwareSummary) -> Result<()> {
        self.data()
            .summaries
            .insert(summary.name.to_lowercase(), summary.clone());
        self.flush()
    }

    // ---------- 设置 ----------
    pub fn get_settings(&self) -> Result<Settings> {
        Ok(self.data().settings.clone())
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        self.data().settings = settings.clone();
        self.flush()
    }

    // ---------- 分类 ----------
    pub fn get_categories(&self) -> Result<Vec<Category>> {
        let data = self.data();
        let mut categories = data.categories.clone();
        for category in &mut categories {
            category.package_count = data
                .software
                .iter()
                .filter(|s| s.category_id.as_deref() == Some(category.id.as_str()))
                .count() as i64;
        }
        categories.sort_by(|a, b| a.sort_order.cmp(&b.sort_order).then(a.name.cmp(&b.name)));
        Ok(categories)
    }

    pub fn reorder_categories(&self, ids: &[String]) -> Result<()> {
        {
            let mut data = self.data();
            for (order, id) in ids.iter().enumerate() {
                if let Some(category) = data.categories.iter_mut().find(|c| &c.id == id) {
                    category.sort_order = order as i32;
                }
            }
        }
        self.flush()
    }

    pub fn create_category(
        &self,
        name: &str,
        color: &str,
        icon: &str,
        parent_id: Option<&str>,
    ) -> Result<Category> {
        let mut data = self.data();
        let max_order = data.categories.iter().map(|c| c.sort_order).max().unwrap_or(0);
        let category = Category {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            color: color.to_string(),
            icon: icon.to_string(),
            parent_id: parent_id.map(str::to_string),
            sort_order: max_order + 1,
            package_count: 0,
        };
        data.categories.push(category.clone());
        drop(data);
        self.flush()?;
        Ok(category)
    }

    pub fn update_category(&self, category: &Category) -> Result<()> {
        if let Some(existing) = self.data().categories.iter_mut().find(|c| c.id == category.id) {
            existing.name = category.name.clone();
            existing.color = category.color.clone();
            existing.icon = category.icon.clone();
            existing.parent_id = category.parent_id.clone();
            existing.sort_order = category.sort_order;
        }
        self.flush()
    }

    pub fn delete_category(&self, id: &str) -> Result<()> {
        {
            let mut data = self.data();
            data.categories.retain(|c| c.id != id);
            for category in &mut data.categories {
                if category.parent_id.as_deref() == Some(id) {
                    category.parent_id = None;
                }
            }
            for software in &mut data.software {
                if software.category_id.as_deref() == Some(id) {
                    software.category_id = None;
                }
            }
        }
        self.flush()
    }

    pub fn assign_category(&self, software_id: &str, category_id: Option<&str>) -> Result<()> {
        let mut data = self.data();
        if let Some(software) = data.software.iter_mut().find(|s| s.id == software_id) {
            software.category_id = category_id.map(str::to_string);
            software.updated_at = Utc::now().to_rfc3339();
        }
        drop(data);
        self.flush()
    }

    // ---------- 标签 ----------
    pub fn get_tags(&self) -> Result<Vec<Tag>> {
        Ok(self.data().tags.clone())
    }

    pub fn create_tag(&self, name: &str, color: &str) -> Result<Tag> {
        let tag = Tag {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            color: color.to_string(),
        };
        self.data().tags.push(tag.clone());
        drop(self.data());
        self.flush()?;
        Ok(tag)
    }

    pub fn update_tag(&self, tag: &Tag) -> Result<()> {
        if let Some(existing) = self.data().tags.iter_mut().find(|t| t.id == tag.id) {
            existing.name = tag.name.clone();
            existing.color = tag.color.clone();
        }
        self.flush()
    }

    pub fn delete_tag(&self, id: &str) -> Result<()> {
        {
            let mut data = self.data();
            data.tags.retain(|t| t.id != id);
            for software in &mut data.software {
                software.tags.retain(|t| t != id);
            }
        }
        self.flush()
    }

    pub fn assign_tags(&self, software_id: &str, tags: Vec<String>) -> Result<()> {
        let mut data = self.data();
        if let Some(software) = data.software.iter_mut().find(|s| s.id == software_id) {
            software.tags = tags;
            software.updated_at = Utc::now().to_rfc3339();
        }
        drop(data);
        self.flush()
    }

    // ---------- 软件 ----------
    pub fn get_software(&self) -> Result<Vec<Software>> {
        Ok(self.data().software.clone())
    }

    pub fn find_software_by_name(&self, name: &str) -> Option<Software> {
        self.data()
            .software
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
            .cloned()
    }

    pub fn create_software(&self, name: &str, category_id: Option<&str>) -> Result<Software> {
        let now = Utc::now().to_rfc3339();
        let software = Software {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            icon: String::new(),
            category_id: category_id.map(str::to_string),
            tags: Vec::new(),
            description: String::new(),
            created_at: now.clone(),
            updated_at: now,
            versions: Vec::new(),
        };
        self.data().software.push(software.clone());
        drop(self.data());
        self.flush()?;
        Ok(software)
    }

    /// 导入安装包后写入软件图标（data URL），仅在该软件还没有图标时调用。
    pub fn set_software_icon(&self, id: &str, icon: &str) -> Result<()> {
        {
            let mut data = self.data();
            if let Some(existing) = data.software.iter_mut().find(|s| s.id == id) {
                existing.icon = icon.to_string();
                existing.updated_at = Utc::now().to_rfc3339();
            }
        }
        self.flush()
    }

    pub fn update_software(&self, software: &Software) -> Result<()> {
        let mut data = self.data();
        if let Some(existing) = data.software.iter_mut().find(|s| s.id == software.id) {
            existing.name = software.name.clone();
            existing.icon = software.icon.clone();
            existing.category_id = software.category_id.clone();
            existing.tags = software.tags.clone();
            existing.description = software.description.clone();
            existing.updated_at = Utc::now().to_rfc3339();
        }
        drop(data);
        self.flush()
    }

    pub fn delete_software(&self, id: &str) -> Result<()> {
        {
            let mut data = self.data();
            data.software.retain(|s| s.id != id);
        }
        self.flush()
    }

    // ---------- 版本与文件 ----------
    pub fn add_version(&self, software_id: &str, version: SoftwareVersion) -> Result<()> {
        let mut data = self.data();
        if let Some(software) = data.software.iter_mut().find(|s| s.id == software_id) {
            // 同版本号已存在则合并文件
            if let Some(existing) = software
                .versions
                .iter_mut()
                .find(|v| v.version == version.version)
            {
                for file in version.files {
                    if !existing.files.iter().any(|f| f.path == file.path) {
                        existing.files.push(file);
                    }
                }
            } else {
                software.versions.push(version);
            }
            software.updated_at = Utc::now().to_rfc3339();
        }
        drop(data);
        self.flush()
    }

    pub fn delete_version(&self, software_id: &str, version_id: &str) -> Result<()> {
        let mut data = self.data();
        if let Some(software) = data.software.iter_mut().find(|s| s.id == software_id) {
            software.versions.retain(|v| v.id != version_id);
            if !software.versions.iter().any(|v| v.is_default) {
                if let Some(first) = software.versions.first_mut() {
                    first.is_default = true;
                }
            }
            software.updated_at = Utc::now().to_rfc3339();
        }
        drop(data);
        self.flush()
    }

    pub fn set_default_version(&self, software_id: &str, version_id: &str) -> Result<()> {
        let mut data = self.data();
        if let Some(software) = data.software.iter_mut().find(|s| s.id == software_id) {
            for version in &mut software.versions {
                version.is_default = version.id == version_id;
            }
            software.updated_at = Utc::now().to_rfc3339();
        }
        drop(data);
        self.flush()
    }

    pub fn add_file(&self, software_id: &str, version_id: &str, file: SoftwareFile) -> Result<()> {
        let mut data = self.data();
        if let Some(software) = data.software.iter_mut().find(|s| s.id == software_id) {
            if let Some(version) = software.versions.iter_mut().find(|v| v.id == version_id) {
                if !version.files.iter().any(|f| f.sha256 == file.sha256 && !file.sha256.is_empty()
                    || f.path == file.path)
                {
                    version.files.push(file);
                }
            }
            software.updated_at = Utc::now().to_rfc3339();
        }
        drop(data);
        self.flush()
    }

    pub fn delete_file(&self, software_id: &str, version_id: &str, file_id: &str) -> Result<Option<String>> {
        let mut data = self.data();
        let mut removed = None;
        if let Some(software) = data.software.iter_mut().find(|s| s.id == software_id) {
            if let Some(version) = software.versions.iter_mut().find(|v| v.id == version_id) {
                if let Some(index) = version.files.iter().position(|f| f.id == file_id) {
                    removed = Some(version.files.remove(index).path);
                }
            }
            software.updated_at = Utc::now().to_rfc3339();
        }
        drop(data);
        self.flush()?;
        Ok(removed)
    }

    /// 通过文件 id 找到所属软件与路径（用于运行/定位）
    pub fn resolve_file(&self, software_id: &str, file_id: &str) -> Result<Option<String>> {
        Ok(self
            .data()
            .software
            .iter()
            .find(|s| s.id == software_id)
            .and_then(|s| s.versions.iter().flat_map(|v| &v.files).find(|f| f.id == file_id))
            .map(|f| f.path.clone()))
    }

    /// 兼容旧接口：按（扁平）包 id 找路径。新数据里 package id 即 file id。
    pub fn get_package_path(&self, id: &str) -> Result<Option<String>> {
        Ok(self
            .data()
            .software
            .iter()
            .flat_map(|s| &s.versions)
            .flat_map(|v| &v.files)
            .find(|f| f.id == id)
            .map(|f| f.path.clone()))
    }

    /// 插入或更新一个扁平包（供扫描/导入兼容使用）
    pub fn upsert_package(&self, pkg: &PackageInfo) -> Result<bool> {
        let mut data = self.data();
        let now = Utc::now().to_rfc3339();
        // 先尝试按路径找到已有文件
        let mut found = false;
        for software in &mut data.software {
            for version in &mut software.versions {
                if let Some(file) = version.files.iter_mut().find(|f| f.path == pkg.full_path) {
                    file.name = pkg.file_name.clone();
                    file.size = pkg.file_size;
                    file.format = pkg.extension.trim_start_matches('.').to_string();
                    file.platform = pkg.platform;
                    found = true;
                    break;
                }
            }
            if found {
                break;
            }
        }
        if found {
            drop(data);
            self.flush()?;
            return Ok(false);
        }

        let software = match data
            .software
            .iter_mut()
            .find(|s| s.name.eq_ignore_ascii_case(&pkg.software_name))
        {
            Some(s) => s,
            None => {
                data.software.push(Software {
                    id: Uuid::new_v4().to_string(),
                    name: pkg.software_name.clone(),
                    icon: String::new(),
                    category_id: pkg.category_id.clone(),
                    tags: Vec::new(),
                    description: String::new(),
                    created_at: now.clone(),
                    updated_at: now.clone(),
                    versions: Vec::new(),
                });
                data.software.last_mut().unwrap()
            }
        };

        let version = match software
            .versions
            .iter_mut()
            .find(|v| v.version == pkg.version)
        {
            Some(v) => v,
            None => {
                software.versions.push(SoftwareVersion {
                    id: Uuid::new_v4().to_string(),
                    version: pkg.version.clone(),
                    release_date: None,
                    is_default: software.versions.is_empty(),
                    created_at: now.clone(),
                    files: Vec::new(),
                });
                software.versions.last_mut().unwrap()
            }
        };
        version.files.push(SoftwareFile {
            id: pkg.id.clone(),
            name: pkg.file_name.clone(),
            path: pkg.full_path.clone(),
            format: pkg.extension.trim_start_matches('.').to_string(),
            size: pkg.file_size,
            sha256: String::new(),
            role: "installer".to_string(),
            platform: pkg.platform,
            added_at: now.clone(),
        });
        software.updated_at = now;
        drop(data);
        self.flush()?;
        Ok(true)
    }

    /// 兼容旧接口：删除包
    pub fn delete_package(&self, id: &str) -> Result<Option<String>> {
        let mut data = self.data();
        let mut removed = None;
        'outer: for software in &mut data.software {
            for version in &mut software.versions {
                if let Some(index) = version.files.iter().position(|f| f.id == id) {
                    removed = Some(version.files.remove(index).path);
                    break 'outer;
                }
            }
        }
        drop(data);
        self.flush()?;
        Ok(removed)
    }

    // ---------- 统计与备份 ----------
    pub fn get_storage_info(&self) -> Result<StorageInfo> {
        let (repo, backup_dir) = {
            let paths = self.paths();
            (paths.repo_root.clone(), paths.backup_dir.clone())
        };
        let data = self.data();
        let mut version_count = 0i64;
        let mut file_count = 0i64;
        let mut total_size = 0i64;
        let mut managed_size = 0i64;
        let mut indexed_size = 0i64;
        let mut missing_files = 0i64;
        let installer_dir = data.settings.installer_directory.trim_start_matches("./");
        let installer_root = repo.join(installer_dir);

        for software in &data.software {
            version_count += software.versions.len() as i64;
            for version in &software.versions {
                for file in &version.files {
                    file_count += 1;
                    total_size += file.size;
                    let path = PathBuf::from(&file.path);
                    let is_managed = path.is_absolute() && path.starts_with(&installer_root);
                    if is_managed {
                        managed_size += file.size;
                    } else {
                        indexed_size += file.size;
                    }
                    if !path.exists() {
                        missing_files += 1;
                    }
                }
            }
        }

        let backup_count = fs::read_dir(&backup_dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
                    .count() as i64
            })
            .unwrap_or(0);

        Ok(StorageInfo {
            repo_root: repo.to_string_lossy().to_string(),
            installer_directory: data.settings.installer_directory.clone(),
            storage_mode: data.settings.storage_mode.clone(),
            software_count: data.software.len() as i64,
            version_count,
            file_count,
            total_size,
            managed_size,
            indexed_size,
            missing_files,
            backup_count,
        })
    }

    pub fn list_backups(&self) -> Result<Vec<BackupInfo>> {
        let backup_dir = self.paths().backup_dir.clone();
        let mut result = Vec::new();
        for entry in fs::read_dir(&backup_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let meta = fs::metadata(&path)?;
            let modified = meta
                .modified()
                .ok()
                .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
                .unwrap_or_default();
            result.push(BackupInfo {
                name: entry.file_name().to_string_lossy().to_string(),
                path: path.to_string_lossy().to_string(),
                size: meta.len() as i64,
                created_at: modified,
            });
        }
        result.sort_by(|a, b| b.name.cmp(&a.name));
        Ok(result)
    }

    /// 从指定备份恢复
    pub fn restore_backup(&self, path: &Path) -> Result<()> {
        let content = fs::read_to_string(path).with_context(|| "读取备份失败")?;
        let data: StoreData = serde_json::from_str(&content).context("备份文件格式错误")?;
        *self.data() = data;
        self.persist()
    }

    /// 校验当前 JSON 是否可解析
    pub fn verify(&self) -> Result<bool> {
        let path = self.paths().path.clone();
        let content = fs::read_to_string(&path)?;
        Ok(serde_json::from_str::<StoreData>(&content).is_ok())
    }

    pub fn export_backup(&self, path: &Path) -> Result<()> {
        let data = self.data();
        let backup = BackupData {
            version: STORE_VERSION,
            exported_at: Utc::now().to_rfc3339(),
            settings: data.settings.clone(),
            categories: data.categories.clone(),
            tags: data.tags.clone(),
            software: data.software.clone(),
            packages: Vec::new(),
        };
        let json = serde_json::to_string_pretty(&backup)?;
        fs::write(path, json).with_context(|| "写入备份文件失败")?;
        Ok(())
    }

    pub fn import_backup(&self, path: &Path) -> Result<ScanResult> {
        let content = fs::read_to_string(path).with_context(|| "读取备份文件失败")?;
        let backup: BackupData = serde_json::from_str(&content)?;
        let mut result = ScanResult {
            total: backup.software.len(),
            ..ScanResult::default()
        };
        {
            let mut data = self.data();
            for category in &backup.categories {
                if !data.categories.iter().any(|c| c.id == category.id || c.name == category.name) {
                    data.categories.push(category.clone());
                }
            }
            for tag in &backup.tags {
                if !data.tags.iter().any(|t| t.id == tag.id || t.name == tag.name) {
                    data.tags.push(tag.clone());
                }
            }
            for software in &backup.software {
                if !data.software.iter().any(|s| s.id == software.id || s.name == software.name) {
                    data.software.push(software.clone());
                    result.added += 1;
                } else {
                    result.updated += 1;
                }
            }
        }
        self.flush()?;
        Ok(result)
    }

    /// 生成扁平兼容视图（packages + groups + stats）
    pub fn get_library(&self) -> Result<LibraryData> {
        let data = self.data();
        let mut packages = Vec::new();
        let mut platform_counts: HashMap<String, i64> = HashMap::new();
        let mut total_size = 0i64;
        // 分类计数内联计算，避免在持锁时调用 get_categories 造成死锁
        let mut categories = data.categories.clone();
        for category in &mut categories {
            category.package_count = data
                .software
                .iter()
                .filter(|s| s.category_id.as_deref() == Some(category.id.as_str()))
                .count() as i64;
        }
        categories.sort_by(|a, b| a.sort_order.cmp(&b.sort_order).then(a.name.cmp(&b.name)));

        for software in &data.software {
            for version in &software.versions {
                for file in &version.files {
                    let added = if file.added_at.is_empty() {
                        version.created_at.clone()
                    } else {
                        file.added_at.clone()
                    };
                    packages.push(PackageInfo {
                        id: file.id.clone(),
                        file_name: file.name.clone(),
                        full_path: file.path.clone(),
                        software_name: software.name.clone(),
                        version: version.version.clone(),
                        platform: file.platform,
                        file_size: file.size,
                        added_time: added.clone(),
                        modified_time: added,
                        extension: format!(".{}", file.format),
                        category_id: software.category_id.clone(),
                        product_name: None,
                        publisher: None,
                        description: if software.description.is_empty() {
                            None
                        } else {
                            Some(software.description.clone())
                        },
                    });
                    *platform_counts
                        .entry(file.platform.as_str().to_string())
                        .or_default() += 1;
                    total_size += file.size;
                }
            }
        }
        packages.sort_by(|a, b| b.added_time.cmp(&a.added_time));

        let mut by_software: HashMap<String, Vec<PackageInfo>> = HashMap::new();
        for pkg in &packages {
            by_software
                .entry(pkg.software_name.clone())
                .or_default()
                .push(pkg.clone());
        }
        let mut groups: Vec<SoftwareGroup> = by_software
            .into_iter()
            .map(|(name, mut items)| {
                items.sort_by(|a, b| b.added_time.cmp(&a.added_time));
                let latest_version = items.first().map(|p| p.version.clone()).unwrap_or_default();
                let mut platforms: Vec<Platform> = Vec::new();
                for item in &items {
                    if !platforms.contains(&item.platform) {
                        platforms.push(item.platform);
                    }
                }
                SoftwareGroup {
                    name,
                    packages: items,
                    latest_version,
                    platforms,
                }
            })
            .collect();
        groups.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(LibraryData {
            stats: LibraryStats {
                software_count: data.software.len() as i64,
                package_count: packages.len() as i64,
                total_size,
                platform_counts,
            },
            packages,
            categories,
            groups,
            software: data.software.clone(),
            tags: data.tags.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_store_round_trip() {
        let dir = std::env::temp_dir().join(format!("shibapack-test-{}", Uuid::new_v4()));
        let path = dir.join("software.json");
        let db = Database::open(&path, &dir).unwrap();
        assert_eq!(db.get_categories().unwrap().len(), 9);

        let mut settings = Settings::default();
        settings.package_root = "/tmp/pkgs".into();
        settings.storage_mode = "indexed".into();
        db.save_settings(&settings).unwrap();

        let software = db.create_software("Blender", None).unwrap();
        db.add_version(
            &software.id,
            SoftwareVersion {
                id: Uuid::new_v4().to_string(),
                version: "4.5.0".into(),
                release_date: None,
                is_default: true,
                created_at: Utc::now().to_rfc3339(),
                files: vec![SoftwareFile {
                    id: Uuid::new_v4().to_string(),
                    name: "Blender_4.5.0.exe".into(),
                    path: "/tmp/pkgs/Blender/4.5.0/Blender_4.5.0.exe".into(),
                    format: "exe".into(),
                    size: 1024,
                    sha256: String::new(),
                    role: "installer".into(),
                    platform: Platform::Windows,
                    added_at: Utc::now().to_rfc3339(),
                }],
            },
        )
        .unwrap();

        let reopened = Database::open(&path, &dir).unwrap();
        assert_eq!(reopened.get_settings().unwrap().storage_mode, "indexed");
        assert_eq!(reopened.get_software().unwrap().len(), 1);
        assert_eq!(reopened.get_library().unwrap().stats.package_count, 1);
        assert!(reopened.verify().unwrap());

        let _ = fs::remove_dir_all(dir);
    }
}
