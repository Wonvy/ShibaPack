use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{command, State};

use crate::models::{
    AppStatus, BackupInfo, Category, InstalledProgram, LibraryData, ScanResult, Settings,
    Software, SoftwareFile, SoftwareVersion, StorageInfo, Tag,
};
use crate::platform;
use crate::scanner;
use crate::state::AppState;
use crate::summary;
use chrono::Utc;

#[command]
pub fn get_status(state: State<'_, AppState>) -> Result<AppStatus, String> {
    let settings = state.settings().map_err(|e| e.to_string())?;
    // 数据目录即仓库根：software.json 位于此，设置页据此显示真实数据文件
    let repo_root = state.db.repo_root();
    Ok(AppStatus {
        configured: !settings.package_root.is_empty(),
        platform: platform::platform_name(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        data_dir: repo_root.to_string_lossy().to_string(),
        repo_root: repo_root.to_string_lossy().to_string(),
    })
}

#[command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    state.settings().map_err(|e| e.to_string())
}

#[command]
pub fn update_settings(
    settings: Settings,
    state: State<'_, AppState>,
) -> Result<Settings, String> {
    let mut settings = settings;
    let current_root = state.db.repo_root();
    let requested = PathBuf::from(&settings.package_root);

    // 仅在仓库根确实变化时迁移；路径格式差异（尾斜杠、大小写）不算变化。
    // 若指向的是文件（例如误填旧版 data.json 路径），也不迁移，避免保存设置整体失败。
    if !settings.package_root.trim().is_empty()
        && !requested.is_file()
        && !crate::state::same_path(&requested, &current_root)
    {
        state
            .switch_repo(&requested)
            .map_err(|e| e.to_string())?;
    }
    // 回填规范化后的仓库根，保证 settings 与实际数据文件位置一致
    settings.package_root = state.db.repo_root().to_string_lossy().to_string();
    state.db.save_settings(&settings).map_err(|e| e.to_string())?;
    Ok(settings)
}

#[command]
pub fn get_library(state: State<'_, AppState>) -> Result<LibraryData, String> {
    let mut data = state.db.get_library().map_err(|e| e.to_string())?;
    // 懒补全：历史数据（重构前导入）没有图标，首次读取时按安装包文件补提取
    let repo_root = state.db.repo_root();
    let mut patched: Vec<(String, String)> = Vec::new();
    for software in &data.software {
        if !software.icon.is_empty() || patched.len() >= 8 {
            continue;
        }
        let Some(file) = software
            .versions
            .iter()
            .flat_map(|v| v.files.iter())
            .next()
        else {
            continue;
        };
        let actual = scanner::resolve_stored_path(&file.path, &repo_root);
        if !actual.exists() {
            continue;
        }
        let icon = crate::icons::package_icon(&actual);
        if !icon.is_empty() {
            patched.push((software.id.clone(), icon));
        }
    }
    if !patched.is_empty() {
        state.db.begin_batch();
        for (id, icon) in &patched {
            let _ = state.db.set_software_icon(id, icon);
            if let Some(item) = data.software.iter_mut().find(|s| s.id == *id) {
                item.icon = icon.clone();
            }
        }
        let _ = state.db.end_batch();
    }
    Ok(data)
}

#[command]
pub fn get_tags(state: State<'_, AppState>) -> Result<Vec<Tag>, String> {
    state.db.get_tags().map_err(|e| e.to_string())
}

#[command]
pub fn create_tag(name: String, color: String, state: State<'_, AppState>) -> Result<Tag, String> {
    state.db.create_tag(&name, &color).map_err(|e| e.to_string())
}

#[command]
pub fn update_tag(tag: Tag, state: State<'_, AppState>) -> Result<(), String> {
    state.db.update_tag(&tag).map_err(|e| e.to_string())
}

#[command]
pub fn delete_tag(id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.db.delete_tag(&id).map_err(|e| e.to_string())
}

#[command]
pub fn assign_tags(
    software_id: String,
    tags: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .db
        .assign_tags(&software_id, tags)
        .map_err(|e| e.to_string())
}

#[command]
pub fn create_software(
    name: String,
    category_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Software, String> {
    state
        .db
        .create_software(&name, category_id.as_deref())
        .map_err(|e| e.to_string())
}

#[command]
pub fn update_software(software: Software, state: State<'_, AppState>) -> Result<(), String> {
    state.db.update_software(&software).map_err(|e| e.to_string())
}

#[command]
pub fn delete_software(id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.db.delete_software(&id).map_err(|e| e.to_string())
}

#[command]
pub fn add_version(
    software_id: String,
    version: SoftwareVersion,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .db
        .add_version(&software_id, version)
        .map_err(|e| e.to_string())
}

#[command]
pub fn delete_version(
    software_id: String,
    version_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .db
        .delete_version(&software_id, &version_id)
        .map_err(|e| e.to_string())
}

#[command]
pub fn set_default_version(
    software_id: String,
    version_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .db
        .set_default_version(&software_id, &version_id)
        .map_err(|e| e.to_string())
}

#[command]
pub fn delete_file(
    software_id: String,
    version_id: String,
    file_id: String,
    delete_from_disk: Option<bool>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let path = state
        .db
        .delete_file(&software_id, &version_id, &file_id)
        .map_err(|e| e.to_string())?;
    if delete_from_disk.unwrap_or(false) {
        if let Some(path) = path {
            let actual = scanner::resolve_stored_path(&path, &state.db.repo_root());
            let _ = fs::remove_file(actual);
        }
    }
    Ok(())
}

#[command]
pub fn run_file(
    software_id: String,
    file_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let path = state
        .db
        .resolve_file(&software_id, &file_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "找不到安装文件记录".to_string())?;
    let actual = scanner::resolve_stored_path(&path, &state.db.repo_root());
    platform::launch(&actual)
}

#[command]
pub fn reveal_file(
    software_id: String,
    file_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let path = state
        .db
        .resolve_file(&software_id, &file_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "找不到安装文件记录".to_string())?;
    let actual = scanner::resolve_stored_path(&path, &state.db.repo_root());
    platform::reveal(&actual)
}

/// 校验文件状态（是否存在）
#[command]
pub fn check_file(path: String, state: State<'_, AppState>) -> Result<bool, String> {
    let actual = scanner::resolve_stored_path(&path, &state.db.repo_root());
    Ok(actual.exists())
}

#[command]
pub async fn scan_directory(
    path: Option<String>,
    state: State<'_, AppState>,
) -> Result<ScanResult, String> {
    let root = PathBuf::from(path.unwrap_or_else(|| state.db.repo_root().to_string_lossy().to_string()));
    if root.as_os_str().is_empty() || !root.exists() {
        return Ok(ScanResult::default());
    }
    let db = Arc::clone(&state.db);
    let app = state.app_handle.clone();
    tauri::async_runtime::spawn_blocking(move || scanner::scan_directory(&root, db, app))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[command]
pub async fn add_packages(
    paths: Vec<String>,
    platform_hint: Option<String>,
    state: State<'_, AppState>,
) -> Result<ScanResult, String> {
    let settings = state.settings().map_err(|e| e.to_string())?;
    let db = Arc::clone(&state.db);
    let repo_root = state.db.repo_root();
    let app = state.app_handle.clone();
    tauri::async_runtime::spawn_blocking(move || {
        scanner::import_paths(&paths, platform_hint.as_deref(), &settings, db, &repo_root, app)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[command]
pub fn delete_package(
    id: String,
    delete_file: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let path = state.db.delete_package(&id).map_err(|e| e.to_string())?;
    if delete_file {
        if let Some(path) = path {
            let actual = scanner::resolve_stored_path(&path, &state.db.repo_root());
            let _ = fs::remove_file(actual);
        }
    }
    Ok(())
}

#[command]
pub fn run_package(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let path = state
        .db
        .get_package_path(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "找不到安装包记录".to_string())?;
    let actual = scanner::resolve_stored_path(&path, &state.db.repo_root());
    platform::launch(&actual)
}

#[command]
pub fn reveal_package(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let path = state
        .db
        .get_package_path(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "找不到安装包记录".to_string())?;
    let actual = scanner::resolve_stored_path(&path, &state.db.repo_root());
    platform::reveal(&actual)
}

#[command]
pub fn reveal_data_dir(state: State<'_, AppState>) -> Result<(), String> {
    platform::reveal(&state.db.repo_root())
}

#[command]
pub fn reveal_path(path: String) -> Result<(), String> {
    platform::reveal(&PathBuf::from(path))
}

#[command]
pub fn write_text_file(path: String, content: String) -> Result<(), String> {
    fs::write(&path, content).map_err(|e| format!("写入文件失败：{}", e))
}

#[command]
pub fn read_text_file(path: String) -> Result<String, String> {
    fs::read_to_string(&path).map_err(|e| format!("读取文件失败：{}", e))
}

#[command]
pub async fn fetch_summary(
    name: String,
    force: Option<bool>,
    state: State<'_, AppState>,
) -> Result<crate::models::SoftwareSummary, String> {
    let key = name.trim().to_lowercase();
    if key.is_empty() {
        return Err("软件名称为空".to_string());
    }
    if !force.unwrap_or(false) {
        if let Some(cached) = state.db.get_summary(&key).map_err(|e| e.to_string())? {
            return Ok(cached);
        }
    }
    let found = summary::lookup(name.trim()).await;
    let result = match found {
        Some((text, source, url)) => crate::models::SoftwareSummary {
            name: key,
            summary: text,
            source,
            url,
            updated_at: Utc::now().to_rfc3339(),
        },
        None => crate::models::SoftwareSummary {
            name: key,
            summary: String::new(),
            source: "none".to_string(),
            url: String::new(),
            updated_at: Utc::now().to_rfc3339(),
        },
    };
    state.db.save_summary(&result).map_err(|e| e.to_string())?;
    Ok(result)
}

#[command]
pub async fn get_installed_apps() -> Result<Vec<InstalledProgram>, String> {
    tauri::async_runtime::spawn_blocking(platform::installed_apps)
        .await
        .map_err(|e| e.to_string())
}

#[command]
pub fn uninstall_app(program: InstalledProgram) -> Result<(), String> {
    platform::uninstall(&program)
}

#[command]
pub fn create_category(
    name: String,
    color: String,
    icon: String,
    parent_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Category, String> {
    state
        .db
        .create_category(&name, &color, &icon, parent_id.as_deref())
        .map_err(|e| e.to_string())
}

#[command]
pub fn update_category(
    category: Category,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.db.update_category(&category).map_err(|e| e.to_string())
}

#[command]
pub fn delete_category(id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.db.delete_category(&id).map_err(|e| e.to_string())
}

#[command]
pub fn reorder_categories(ids: Vec<String>, state: State<'_, AppState>) -> Result<(), String> {
    state.db.reorder_categories(&ids).map_err(|e| e.to_string())
}

#[command]
pub fn assign_category(
    software_id: String,
    category_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .db
        .assign_category(&software_id, category_id.as_deref())
        .map_err(|e| e.to_string())
}

// ---------- 存储与备份 ----------
#[command]
pub fn get_storage_info(state: State<'_, AppState>) -> Result<StorageInfo, String> {
    state.db.get_storage_info().map_err(|e| e.to_string())
}

#[command]
pub fn list_backups(state: State<'_, AppState>) -> Result<Vec<BackupInfo>, String> {
    state.db.list_backups().map_err(|e| e.to_string())
}

#[command]
pub fn restore_backup(path: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .db
        .restore_backup(&PathBuf::from(path))
        .map_err(|e| e.to_string())
}

#[command]
pub fn verify_data(state: State<'_, AppState>) -> Result<bool, String> {
    state.db.verify().map_err(|e| e.to_string())
}

#[command]
pub fn export_backup(path: String, state: State<'_, AppState>) -> Result<(), String> {
    state.db.export_backup(&PathBuf::from(path)).map_err(|e| e.to_string())
}

#[command]
pub async fn import_backup(path: String, state: State<'_, AppState>) -> Result<ScanResult, String> {
    let path = PathBuf::from(path);
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || db.import_backup(&path))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// 供详情面板展示：读取某软件的完整结构
#[command]
pub fn get_software(id: String, state: State<'_, AppState>) -> Result<Option<Software>, String> {
    Ok(state
        .db
        .get_software()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|s| s.id == id))
}

#[command]
pub fn get_files(
    software_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<SoftwareFile>, String> {
    Ok(state
        .db
        .get_software()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|s| s.id == software_id)
        .map(|s| s.versions.into_iter().flat_map(|v| v.files).collect())
        .unwrap_or_default())
}
