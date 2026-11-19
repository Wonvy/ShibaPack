use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::AppHandle;

use crate::db::Database;
use crate::models::Settings;

/// 记录当前仓库根目录的小文件（位于应用配置目录）
const LOCATION_FILE: &str = "repo-location.txt";
const STORE_FILE: &str = "software.json";

/// 规范化路径用于比较：去首尾空白与尾部斜杠，Windows 下忽略大小写。
/// 仅用于「是否同一个位置」的判断，不用于实际读写路径。
fn normalize_for_compare(path: &Path) -> String {
    let raw = path.to_string_lossy().trim().to_string();
    let trimmed = raw.trim_end_matches(|c| c == '\\' || c == '/');
    let value = if trimmed.is_empty() { raw.as_str() } else { trimmed };
    // Windows 下 \ 与 / 等价；其余平台路径不含反斜杠，统一替换无副作用
    let value = value.replace('\\', "/");
    #[cfg(windows)]
    let value = value.to_lowercase();
    value
}

/// 两个路径是否指向同一位置（忽略尾部斜杠、Windows 大小写差异）。
pub fn same_path(a: &Path, b: &Path) -> bool {
    normalize_for_compare(a) == normalize_for_compare(b)
}

/// 迁移数据文件：优先 rename，跨卷/被占用时回退 copy + remove。
/// 源文件不存在或目标已存在时视为无需迁移，绝不覆盖目标数据。
fn migrate_store_file(old_store: &Path, new_store: &Path) -> Result<()> {
    if same_path(old_store, new_store) || !old_store.exists() || new_store.exists() {
        return Ok(());
    }
    if let Some(parent) = new_store.parent() {
        fs::create_dir_all(parent).with_context(|| "创建目标仓库目录失败")?;
    }
    if fs::rename(old_store, new_store).is_ok() {
        return Ok(());
    }
    fs::copy(old_store, new_store).with_context(|| {
        format!(
            "迁移数据文件失败（{} → {}）",
            old_store.display(),
            new_store.display()
        )
    })?;
    let _ = fs::remove_file(old_store);
    Ok(())
}

#[derive(Clone)]
pub struct AppPaths {
    /// 应用配置目录（存放仓库位置记录，仓库根以 db 为准）
    pub config_dir: PathBuf,
}

pub struct AppState {
    pub db: Arc<Database>,
    pub paths: AppPaths,
    pub app_handle: AppHandle,
}

/// 读取仓库位置；未配置时回退到应用配置目录
fn read_repo_location(config_dir: &Path) -> PathBuf {
    let location = config_dir.join(LOCATION_FILE);
    if let Ok(content) = fs::read_to_string(&location) {
        let value = content.trim();
        if !value.is_empty() {
            let path = PathBuf::from(value);
            if path.exists() {
                return path;
            }
        }
    }

    // 兼容迁移：从旧 data.json 或 software.json 中读取 package_root
    for name in ["software.json", "data.json"] {
        let candidate = config_dir.join(name);
        if let Ok(content) = fs::read_to_string(&candidate) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(root) = value
                    .get("settings")
                    .and_then(|s| s.get("package_root"))
                    .and_then(|v| v.as_str())
                {
                    if !root.is_empty() {
                        let path = PathBuf::from(root);
                        if path.exists() {
                            return path;
                        }
                    }
                }
            }
        }
    }

    config_dir.to_path_buf()
}

impl AppState {
    pub fn new(app_handle: AppHandle) -> Result<Self> {
        let config_dir = dirs::config_dir()
            .or_else(|| dirs::home_dir().map(|home| home.join(".config")))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ShibaPack");
        fs::create_dir_all(&config_dir).with_context(|| "创建配置目录失败")?;

        let repo_root = read_repo_location(&config_dir);
        fs::create_dir_all(&repo_root).with_context(|| "创建仓库目录失败")?;

        // 若旧版 software.json / data.json 仍在配置目录，搬到仓库根
        let store_path = repo_root.join(STORE_FILE);
        if repo_root != config_dir {
            migrate_store_file(&config_dir.join(STORE_FILE), &store_path)?;
            migrate_store_file(&config_dir.join("data.json"), &repo_root.join("data.json"))?;
        }

        let db = Arc::new(Database::open(&store_path, &repo_root)?);
        let settings = db.get_settings()?;
        let installer_dir = repo_root.join(
            settings
                .installer_directory
                .trim_start_matches("./")
                .trim_start_matches('/'),
        );
        fs::create_dir_all(&installer_dir).with_context(|| "创建安装包目录失败")?;
        fs::create_dir_all(repo_root.join("Cache"))?;
        fs::create_dir_all(repo_root.join("Logs"))?;

        // 同步位置记录
        fs::write(config_dir.join(LOCATION_FILE), repo_root.to_string_lossy().as_bytes())?;

        Ok(Self {
            db,
            paths: AppPaths { config_dir },
            app_handle,
        })
    }

    pub fn settings(&self) -> Result<Settings> {
        self.db.get_settings()
    }

    /// 切换仓库根目录：迁移数据文件、创建标准目录、更新位置记录
    pub fn switch_repo(&self, new_root: &Path) -> Result<()> {
        // old_root 必须以 db 为准：AppPaths 是启动快照，切换后不会更新。
        let old_root = self.db.repo_root();
        if same_path(&old_root, new_root) {
            return Ok(());
        }
        fs::create_dir_all(new_root).with_context(|| "创建目标仓库目录失败")?;

        let old_store = old_root.join(STORE_FILE);
        let new_store = new_root.join(STORE_FILE);
        migrate_store_file(&old_store, &new_store)?;

        fs::create_dir_all(new_root.join("Cache"))?;
        fs::create_dir_all(new_root.join("Logs"))?;
        let installer_dir = new_root.join(
            self.settings()?
                .installer_directory
                .trim_start_matches("./")
                .trim_start_matches('/'),
        );
        fs::create_dir_all(&installer_dir)?;

        // 让 Database 立即切换到新路径，避免迁移后仍写旧文件
        self.db
            .relocate(&new_store, new_root)
            .context("切换数据文件路径失败")?;

        fs::write(
            self.paths.config_dir.join(LOCATION_FILE),
            new_root.to_string_lossy().as_bytes(),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn same_path_ignores_trailing_separator() {
        assert!(same_path(
            Path::new("C:\\Users\\Demo\\ShibaPack"),
            Path::new("C:\\Users\\Demo\\ShibaPack\\")
        ));
        assert!(!same_path(
            Path::new("C:\\Users\\Demo\\ShibaPack"),
            Path::new("C:\\Users\\Demo\\Other")
        ));
    }

    #[cfg(windows)]
    #[test]
    fn same_path_ignores_case_on_windows() {
        assert!(same_path(
            Path::new("C:\\Users\\Demo\\ShibaPack"),
            Path::new("c:/users/demo/shibapack/")
        ));
    }

    #[test]
    fn migrate_store_file_moves_content_and_never_overwrites_target() {
        let base = std::env::temp_dir().join(format!("shibapack-test-{}", Uuid::new_v4()));
        let old = base.join("old").join(STORE_FILE);
        let new = base.join("new").join(STORE_FILE);
        fs::create_dir_all(old.parent().unwrap()).unwrap();
        fs::write(&old, r#"{"schema_version":1}"#).unwrap();

        migrate_store_file(&old, &new).unwrap();
        assert!(!old.exists(), "源文件应被搬走");
        assert_eq!(
            fs::read_to_string(&new).unwrap(),
            r#"{"schema_version":1}"#,
            "目标文件内容应完整"
        );

        // 目标已存在数据时不得覆盖
        fs::write(&old, "old-data").unwrap();
        fs::write(&new, "keep-data").unwrap();
        migrate_store_file(&old, &new).unwrap();
        assert_eq!(fs::read_to_string(&new).unwrap(), "keep-data");

        let _ = fs::remove_dir_all(&base);
    }
}
