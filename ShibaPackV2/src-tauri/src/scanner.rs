use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;
use walkdir::WalkDir;

use crate::db::Database;
use crate::models::{
    PackageInfo, Platform, ScanProgress, ScanResult, Settings, SoftwareFile, SoftwareVersion,
};

const PACKAGE_EXTENSIONS: &[&str] = &[
    "exe", "msi", "msix", "appx", "dmg", "pkg", "app", "deb", "rpm",
    "appimage", "snap", "flatpak", "zip", "7z", "rar", "tar", "gz", "xz", "iso",
];

pub fn is_package_file(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    PACKAGE_EXTENSIONS.contains(&ext.as_str())
}

pub fn platform_for_path(path: &Path, hint: Option<&str>) -> Platform {
    if let Some(hint) = hint {
        let platform = Platform::from_str(hint);
        if platform != Platform::Unknown {
            return platform;
        }
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "exe" | "msi" | "msix" | "appx" => Platform::Windows,
        "dmg" | "pkg" | "app" => Platform::Macos,
        "deb" | "rpm" | "appimage" | "snap" | "flatpak" => Platform::Linux,
        "zip" | "7z" | "rar" | "tar" | "gz" | "xz" | "iso" => Platform::Crossplatform,
        _ => Platform::Unknown,
    }
}

fn clean_software_name(name: &str) -> String {
    let mut result = name.trim().to_string();
    let suffixes = [
        "_setup", "-setup", " setup", "_installer", "-installer", " installer",
        "-x64", "-x86", "_x64", "_x86", "-win", "-mac", "-linux",
        "_win", "_mac", "_linux", "-portable", "_portable", "-64bit", "-32bit",
    ];
    loop {
        let lower = result.to_lowercase();
        let mut changed = false;
        for suffix in suffixes {
            if lower.ends_with(suffix) {
                result = result[..result.len() - suffix.len()].trim().to_string();
                changed = true;
                break;
            }
        }
        if !changed {
            break;
        }
    }
    if result.is_empty() {
        name.trim().to_string()
    } else {
        result
    }
}

fn parse_name_version(stem: &str) -> (String, String) {
    let with_separator = Regex::new(r"(?i)[_\-\s]+v?(\d+(?:\.\d+){1,4}(?:[-+][a-z0-9.]+)?)").unwrap();
    let plain = Regex::new(r"(?i)(\d+(?:\.\d+){1,4})").unwrap();

    if let Some(caps) = with_separator.captures(stem) {
        let version = caps.get(1).map(|m| m.as_str()).unwrap_or("").trim_start_matches('v').to_string();
        let name = stem[..caps.get(0).unwrap().start()].trim();
        return (clean_software_name(name), version);
    }
    if let Some(caps) = plain.captures(stem) {
        let version = caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
        let name = stem[..caps.get(0).unwrap().start()].trim();
        return (clean_software_name(name), version);
    }
    (clean_software_name(stem), String::new())
}

fn iso8601(time: std::time::SystemTime) -> String {
    let datetime: DateTime<Utc> = time.into();
    datetime.to_rfc3339()
}

/// 计算文件 SHA-256
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).with_context(|| "打开文件计算哈希失败")?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn parse_package(path: &Path, hint: Option<&str>) -> Result<PackageInfo> {
    let metadata = fs::metadata(path).with_context(|| "读取文件信息失败")?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();
    let stem = path
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or(&file_name);
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_string();
    let (software_name, version) = parse_name_version(stem);
    let platform = platform_for_path(path, hint);

    Ok(PackageInfo {
        id: Uuid::new_v4().to_string(),
        file_name,
        full_path: path.to_string_lossy().to_string(),
        software_name,
        version,
        platform,
        file_size: metadata.len() as i64,
        added_time: Utc::now().to_rfc3339(),
        modified_time: iso8601(metadata.modified().unwrap_or_else(|_| std::time::SystemTime::now())),
        extension: format!(".{}", extension.to_lowercase()),
        category_id: None,
        product_name: None,
        publisher: None,
        description: None,
    })
}

/// 扫描目录建立索引（索引模式：不复制文件）
pub fn scan_directory(
    root: &Path,
    db: Arc<Database>,
    app: AppHandle,
) -> Result<ScanResult> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root).follow_links(false).max_depth(6) {
        let entry = entry?;
        if entry.file_type().is_file() && is_package_file(entry.path()) {
            files.push(entry.into_path());
        }
    }

    let total = files.len();
    let mut result = ScanResult {
        total,
        ..ScanResult::default()
    };

    db.begin_batch();
    let _flush = BatchGuard(&db);
    for (index, path) in files.iter().enumerate() {
        let _ = app.emit(
            "scan://progress",
            ScanProgress {
                current: index + 1,
                total,
                path: path.to_string_lossy().to_string(),
            },
        );
        match parse_package(path, None) {
            Ok(package) => match db.upsert_package(&package) {
                Ok(true) => result.added += 1,
                Ok(false) => result.updated += 1,
                Err(_) => result.skipped += 1,
            },
            Err(_) => result.skipped += 1,
        }
    }
    Ok(result)
}

/// 扫描/导入中途出错时也能把批量数据落盘。
struct BatchGuard<'a>(&'a Database);

impl Drop for BatchGuard<'_> {
    fn drop(&mut self) {
        let _ = self.0.end_batch();
    }
}

/// 生成仓库内的目标路径：Installers/软件名/版本/文件名
fn managed_target(
    repo_root: &Path,
    installer_dir: &str,
    software_name: &str,
    version: &str,
    file_name: &str,
) -> PathBuf {
    let safe_software = sanitize_component(software_name);
    let safe_version = if version.trim().is_empty() {
        "unknown".to_string()
    } else {
        sanitize_component(version)
    };
    repo_root
        .join(installer_dir.trim_start_matches("./").trim_start_matches('/'))
        .join(safe_software)
        .join(safe_version)
        .join(file_name)
}

fn sanitize_component(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').to_string();
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed
    }
}

/// 导入文件到仓库（托管模式复制进仓库，索引模式只记录路径）
pub fn import_paths(
    paths: &[String],
    hint: Option<&str>,
    settings: &Settings,
    db: Arc<Database>,
    repo_root: &Path,
    app: AppHandle,
) -> Result<ScanResult> {
    let installer_dir = &settings.installer_directory;
    let managed = settings.storage_mode == "managed";
    let total = paths.len();
    let mut result = ScanResult {
        total,
        ..ScanResult::default()
    };

    db.begin_batch();
    let _flush = BatchGuard(&db);

    for (index, value) in paths.iter().enumerate() {
        let _ = app.emit(
            "import://progress",
            ScanProgress {
                current: index + 1,
                total,
                path: value.clone(),
            },
        );
        let source = PathBuf::from(value.as_str());
        if !source.is_file() {
            result.skipped += 1;
            continue;
        }
        let pkg = match parse_package(&source, hint) {
            Ok(p) => p,
            Err(_) => {
                result.skipped += 1;
                continue;
            }
        };

        let target = if managed {
            let mut target = managed_target(
                repo_root,
                installer_dir,
                &pkg.software_name,
                &pkg.version,
                &pkg.file_name,
            );
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).with_context(|| "创建版本目录失败")?;
            }
            if target.exists() {
                let stamp = Utc::now().format("%Y%m%d%H%M%S");
                let stem = source.file_stem().and_then(|n| n.to_str()).unwrap_or("package");
                let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("");
                let suffix = if ext.is_empty() { String::new() } else { format!(".{}", ext) };
                target = target.with_file_name(format!("{}_{}{}", stem, stamp, suffix));
            }
            fs::copy(&source, &target).with_context(|| "复制安装包到仓库失败")?;
            target
        } else {
            source.clone()
        };

        // 计算哈希并构造新结构记录
        let sha256 = sha256_file(&target).unwrap_or_default();
        let now = Utc::now().to_rfc3339();
        let software = match db.find_software_by_name(&pkg.software_name) {
            Some(s) => s,
            None => db.create_software(&pkg.software_name, None)?,
        };
        // 导入时顺带提取安装包图标，列表与详情即可立刻显示真实图标
        if software.icon.is_empty() {
            let icon = crate::icons::package_icon(&target);
            if !icon.is_empty() {
                let _ = db.set_software_icon(&software.id, &icon);
            }
        }
        let version = SoftwareVersion {
            id: Uuid::new_v4().to_string(),
            version: pkg.version.clone(),
            release_date: None,
            is_default: software.versions.is_empty(),
            created_at: now.clone(),
            files: vec![SoftwareFile {
                id: Uuid::new_v4().to_string(),
                name: target
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&pkg.file_name)
                    .to_string(),
                // 托管模式存相对仓库路径，索引模式存绝对路径
                path: if managed {
                    path_relative(&target, repo_root)
                } else {
                    target.to_string_lossy().to_string()
                },
                format: pkg.extension.trim_start_matches('.').to_string(),
                size: pkg.file_size,
                sha256: sha256.clone(),
                role: "installer".to_string(),
                platform: pkg.platform,
                added_at: now.clone(),
            }],
        };

        // 重复检测：同软件同版本且哈希相同则跳过
        let duplicate = software.versions.iter().any(|v| {
            v.version == version.version
                && v.files.iter().any(|f| !sha256.is_empty() && f.sha256 == sha256)
        });
        if duplicate {
            result.skipped += 1;
            if managed {
                let _ = fs::remove_file(&target); // 删除刚复制的重复文件
            }
            continue;
        }

        db.add_version(&software.id, version)?;
        result.added += 1;
    }
    Ok(result)
}

/// 计算相对仓库根目录的路径；不在仓库内则返回绝对路径
fn path_relative(path: &Path, repo_root: &Path) -> String {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo_root.join(path)
    };
    match abs.strip_prefix(repo_root) {
        Ok(rel) => rel.to_string_lossy().to_string(),
        Err(_) => abs.to_string_lossy().to_string(),
    }
}

/// 解析存储的路径为实际可访问路径
pub fn resolve_stored_path(stored: &str, repo_root: &Path) -> PathBuf {
    let path = PathBuf::from(stored);
    if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    }
}
