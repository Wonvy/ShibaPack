use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::models::InstalledProgram;

fn desktop_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
    ];
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local/share/applications"));
    }
    dirs
}

fn field(lines: &[&str], key: &str) -> String {
    lines
        .iter()
        .find_map(|line| line.strip_prefix(&format!("{}=", key)))
        .unwrap_or("")
        .trim()
        .to_string()
}

pub fn list() -> Vec<InstalledProgram> {
    let mut result = Vec::new();
    for dir in desktop_dirs() {
        let Ok(entries) = fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            let Ok(content) = fs::read_to_string(&path) else { continue };
            let lines: Vec<&str> = content.lines().collect();
            let name = field(&lines, "Name");
            if name.is_empty() || field(&lines, "NoDisplay") == "true" || field(&lines, "Hidden") == "true" {
                continue;
            }
            let icon = crate::icons::linux_icon(&field(&lines, "Icon"));
            result.push(InstalledProgram {
                id: path.to_string_lossy().to_string(),
                name,
                version: field(&lines, "Version"),
                publisher: String::new(),
                install_date: None,
                location: field(&lines, "Exec"),
                source: "Desktop entry".into(),
                uninstall_string: String::new(),
                icon,
                size_bytes: None,
            });
        }
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

pub fn reveal(path: &Path) -> Result<(), String> {
    let target = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent().map(Path::to_path_buf).unwrap_or_else(|| path.to_path_buf())
    };
    Command::new("xdg-open")
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("无法打开文件管理器：{}", e))
}

pub fn launch(path: &Path) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("无法启动安装包：{}", e))
}

pub fn uninstall(_app: &InstalledProgram) -> Result<(), String> {
    Err("Linux 请通过系统包管理器卸载软件".to_string())
}
