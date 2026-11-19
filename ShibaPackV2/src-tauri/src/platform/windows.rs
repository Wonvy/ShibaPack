use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

use winreg::enums::HKEY_LOCAL_MACHINE;
use winreg::RegKey;

use crate::models::InstalledProgram;

const UNINSTALL_PATHS: &[&str] = &[
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
    r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
];

fn read_value(key: &RegKey, name: &str) -> String {
    key.get_value(name).unwrap_or_default()
}

pub fn list() -> Vec<InstalledProgram> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();

    let roots = vec![RegKey::predef(HKEY_LOCAL_MACHINE)];
    if let Ok(current_user) = RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall")
    {
        collect_from_key(&current_user, &mut seen, &mut result);
    }

    for path in UNINSTALL_PATHS {
        if let Ok(key) = roots[0].open_subkey(path) {
            collect_from_key(&key, &mut seen, &mut result);
        }
    }

    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

fn collect_from_key(key: &RegKey, seen: &mut HashSet<String>, result: &mut Vec<InstalledProgram>) {
    for name in key.enum_keys().flatten() {
        let Ok(sub) = key.open_subkey(&name) else { continue };
        let display_name = read_value(&sub, "DisplayName");
        if display_name.is_empty() || !seen.insert(display_name.clone()) {
            continue;
        }
        let system_component: u32 = sub.get_value("SystemComponent").unwrap_or(0);
        if system_component == 1 {
            continue;
        }
        let lowercase = display_name.to_lowercase();
        if lowercase.contains("update for")
            || lowercase.contains("security update")
            || lowercase.contains("hotfix")
            || lowercase.starts_with("kb")
        {
            continue;
        }

        let display_icon = read_value(&sub, "DisplayIcon");
        let install_location = read_value(&sub, "InstallLocation");
        let quiet = read_value(&sub, "QuietUninstallString");
        let uninstall_string = if quiet.is_empty() {
            read_value(&sub, "UninstallString")
        } else {
            quiet
        };
        let icon = crate::icons::windows_icon(
            &display_icon,
            &install_location,
            &uninstall_string,
            &display_name,
        );
        // EstimatedSize 的单位是 KB。
        let size_bytes: Option<i64> = sub
            .get_value::<u32, _>("EstimatedSize")
            .ok()
            .map(|kb| kb as i64 * 1024);

        result.push(InstalledProgram {
            id: name.clone(),
            name: display_name,
            version: read_value(&sub, "DisplayVersion"),
            publisher: read_value(&sub, "Publisher"),
            install_date: {
                let date = read_value(&sub, "InstallDate");
                if date.is_empty() { None } else { Some(date) }
            },
            location: install_location,
            source: "Registry".into(),
            uninstall_string,
            icon,
            size_bytes,
        });
    }
}

pub fn reveal(path: &Path) -> Result<(), String> {
    let arg = format!("/select,\"{}\"", path.to_string_lossy());
    Command::new("explorer")
        .arg(arg)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("无法在资源管理器中定位：{}", e))
}

pub fn launch(path: &Path) -> Result<(), String> {
    Command::new(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("无法启动安装包：{}", e))
}

pub fn uninstall(app: &InstalledProgram) -> Result<(), String> {
    if app.uninstall_string.trim().is_empty() {
        return Err("该软件未提供卸载程序".to_string());
    }
    Command::new("cmd")
        .arg("/C")
        .arg(&app.uninstall_string)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("无法启动卸载程序：{}", e))
}
