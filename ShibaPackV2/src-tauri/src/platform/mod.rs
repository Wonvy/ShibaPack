use std::path::Path;

use crate::models::InstalledProgram;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod linux;

pub fn platform_name() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        "linux"
    }
}

pub fn installed_apps() -> Vec<InstalledProgram> {
    #[cfg(target_os = "macos")]
    {
        macos::list()
    }
    #[cfg(target_os = "windows")]
    {
        windows::list()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        linux::list()
    }
}

pub fn reveal(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::reveal(path)
    }
    #[cfg(target_os = "windows")]
    {
        windows::reveal(path)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        linux::reveal(path)
    }
}

pub fn launch(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::launch(path)
    }
    #[cfg(target_os = "windows")]
    {
        windows::launch(path)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        linux::launch(path)
    }
}

pub fn uninstall(app: &InstalledProgram) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::uninstall(app)
    }
    #[cfg(target_os = "windows")]
    {
        windows::uninstall(app)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        linux::uninstall(app)
    }
}
