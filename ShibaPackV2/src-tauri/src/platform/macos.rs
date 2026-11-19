use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{DateTime, Utc};
use regex::Regex;
use walkdir::WalkDir;

use crate::models::InstalledProgram;

fn plist_value(xml: &str, key: &str) -> String {
    let pattern = format!(r"(?s)<key>\s*{}</key>\s*<string>\s*(.*?)\s*</string>", regex::escape(key));
    let Ok(re) = Regex::new(&pattern) else {
        return String::new();
    };
    re.captures(xml)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_default()
}

fn app_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
    ];
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join("Applications"));
    }
    dirs
}

/// 粗略统计 .app 目录占用空间，限制遍历深度避免符号链接循环。
fn app_dir_size(path: &Path) -> Option<i64> {
    let mut total: i64 = 0;
    let mut count = 0usize;
    for entry in WalkDir::new(path).follow_links(false).into_iter().flatten() {
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                total += meta.len() as i64;
                count += 1;
                if count > 200_000 {
                    break;
                }
            }
        }
    }
    if total > 0 { Some(total) } else { None }
}

pub fn list() -> Vec<InstalledProgram> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();

    for dir in app_dirs() {
        if !dir.exists() {
            continue;
        }
        for entry in WalkDir::new(dir).max_depth(2).follow_links(false) {
            let Ok(entry) = entry else { continue };
            if !entry.file_type().is_dir() || entry.path().extension().and_then(|e| e.to_str()) != Some("app") {
                continue;
            }
            let app_path = entry.path();
            let plist = app_path.join("Contents/Info.plist");
            let Ok(xml) = fs::read_to_string(&plist) else { continue };

            let name = plist_value(&xml, "CFBundleDisplayName")
                .or_else(|| plist_value(&xml, "CFBundleName"))
                .or_else(|| {
                    app_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .map(|s| s.trim_end_matches(".app").to_string())
                })
                .unwrap_or_default();
            if name.is_empty() || !seen.insert(name.clone()) {
                continue;
            }

            let version = plist_value(&xml, "CFBundleShortVersionString");
            let publisher = plist_value(&xml, "CFBundleIdentifier");
            let install_date = fs::metadata(app_path)
                .and_then(|m| m.created())
                .ok()
                .map(|t| -> DateTime<Utc> { t.into() })
                .map(|d| d.to_rfc3339());

            let icon = crate::icons::macos_icon(app_path, &xml);
            let size_bytes = app_dir_size(app_path);

            result.push(InstalledProgram {
                id: plist_value(&xml, "CFBundleIdentifier")
                    .or_else(|| app_path.to_string_lossy().to_string().into())
                    .unwrap_or_else(|| name.clone()),
                name,
                version,
                publisher,
                install_date,
                location: app_path.to_string_lossy().to_string(),
                source: "Applications".into(),
                uninstall_string: String::new(),
                icon,
                size_bytes,
            });
        }
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

pub fn reveal(path: &Path) -> Result<(), String> {
    Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("无法在 Finder 中定位：{}", e))
}

pub fn launch(path: &Path) -> Result<(), String> {
    Command::new("open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("无法启动安装包：{}", e))
}

pub fn uninstall(app: &InstalledProgram) -> Result<(), String> {
    let path = Path::new(&app.location);
    if path.extension().and_then(|e| e.to_str()) != Some("app") || !path.exists() {
        return Err("找不到应用目录".to_string());
    }
    let escaped = app
        .location
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let script = format!("tell application \"Finder\" to delete POSIX file \"{}\"", escaped);
    let status = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .status()
        .map_err(|e| format!("无法调用 Finder：{}", e))?;
    if status.success() {
        Ok(())
    } else {
        Err("Finder 未能将应用移入废纸篓".to_string())
    }
}
