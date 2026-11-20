//! 本机的配置和实时状态：主页上的系统信息、CPU、内存、磁盘，和任务管理器里的进程。
//!
//! 用 sysinfo（MIT）读，什么都不存：主页开着、窗口在前面时界面每 2 秒要一次，切走就停。
//! 结束进程只结束当前用户自己的、不是 ShibaPack 自己的；先请它正常退出，强制结束要另外点。

use serde::Serialize;
use std::path::{Path, PathBuf};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, Signal, System, UpdateKind};

/// 不会变的：电脑名、型号、系统、处理器、显卡、内存总量、磁盘、开机时间。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub host_name: String,
    /// 电脑型号：macOS 是“MacBook Pro (Mac16,1)”，Windows / Linux 是主板登记的厂商和产品名。不知道时为空。
    pub model: String,
    /// 系统名和版本，如 “macOS 15.6 Sequoia”“Windows 11 Pro”。
    pub os: String,
    pub kernel: String,
    /// x86_64、arm64……
    pub arch: String,
    pub cpu_brand: String,
    /// 逻辑核心数。
    pub cpu_threads: usize,
    pub cpu_cores: Option<usize>,
    /// Apple 芯片的性能核和能效核个数，别的为空。
    pub core_split: Option<(usize, usize)>,
    /// 处理器频率（MHz），读不到时为 0（Apple 芯片读不到）。
    pub cpu_mhz: u64,
    /// 显卡：型号，知道的话带显存。
    pub gpus: Vec<GpuInfo>,
    pub total_memory: u64,
    pub total_swap: u64,
    /// 开机时间，Unix 秒。
    pub boot_time: u64,
    /// 本机的硬盘（不含系统内部的卷、虚拟盘），系统盘在前。
    pub disks: Vec<DiskInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfo {
    pub model: String,
    pub memory: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    pub name: String,
    pub mount: String,
    pub total: u64,
    pub available: u64,
    pub removable: bool,
    /// SSD、HDD，不知道时为空。
    pub kind: String,
}

/// 某一刻的状态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    /// 总的 CPU 占用，0–100。
    pub cpu: f32,
    pub memory_used: u64,
    pub memory_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    /// 系统盘。
    pub disk: Option<DiskUsage>,
    pub uptime: u64,
    pub processes: Vec<ProcessInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskUsage {
    pub mount: String,
    pub total: u64,
    pub available: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    /// 占整台电脑 CPU 的百分比（已经除以核心数，所有进程加起来不超过 100）。
    pub cpu: f32,
    pub memory: u64,
    pub exe: Option<String>,
    /// 不能在这里结束：ShibaPack 自己、系统进程、别的用户的进程。
    pub protected: bool,
}

/// 留着一个 `System`：CPU 占用要和上一次比才算得出来。
pub struct Monitor {
    system: System,
    own_pid: Pid,
    /// 型号这些要跑 system_profiler，读一次就留着。
    overview: Option<Overview>,
}

impl Monitor {
    /// 系统不让读（极少见）时返回错误，主页上显示读不到。
    pub fn new() -> Result<Monitor, String> {
        let mut system = System::new_with_specifics(
            RefreshKind::nothing().with_cpu(CpuRefreshKind::nothing().with_usage()).with_memory(MemoryRefreshKind::everything()),
        )
        .map_err(|e| e.to_string())?;
        system.refresh_processes_specifics(ProcessesToUpdate::All, true, process_kind());
        // 第一次的 CPU 占用要隔一会儿再量一次才准
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        system.refresh_cpu_usage();
        system.refresh_processes_specifics(ProcessesToUpdate::All, true, process_kind());
        Ok(Monitor { system, own_pid: Pid::from_u32(std::process::id()), overview: None })
    }

    pub fn overview(&mut self) -> Overview {
        if let Some(overview) = &self.overview {
            return overview.clone();
        }
        self.system.refresh_cpu_frequency();
        let cpus = self.system.cpus();
        let mac = mac_hardware();
        let gpus = sysinfo::Gpus::new_with_refreshed_list()
            .map(|gpus| {
                gpus.iter()
                    .filter_map(|gpu| {
                        let model = gpu.model().unwrap_or_default().trim().to_string();
                        (!model.is_empty()).then(|| GpuInfo { model, memory: gpu.total_memory().filter(|m| *m > 0) })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let overview = Overview {
            host_name: System::host_name().unwrap_or_default(),
            model: mac.as_ref().map(|m| m.model.clone()).unwrap_or_else(product_model),
            core_split: mac.as_ref().and_then(|m| m.core_split),
            cpu_mhz: cpus.first().map(|cpu| cpu.frequency()).unwrap_or(0),
            gpus,
            disks: disks(),
            os: System::long_os_version().or_else(|_| System::name()).unwrap_or_default(),
            kernel: System::kernel_version().unwrap_or_default(),
            arch: System::cpu_arch(),
            cpu_brand: cpus.first().map(|cpu| cpu.brand().trim().to_string()).unwrap_or_default(),
            cpu_threads: cpus.len(),
            cpu_cores: System::physical_core_count().ok(),
            total_memory: self.system.total_memory(),
            total_swap: self.system.total_swap(),
            boot_time: System::boot_time().unwrap_or(0),
        };
        self.overview = Some(overview.clone());
        overview
    }


    pub fn sample(&mut self) -> Sample {
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.system.refresh_processes_specifics(ProcessesToUpdate::All, true, process_kind());
        let threads = self.system.cpus().len().max(1) as f32;
        let own_user = self.system.process(self.own_pid).and_then(|p| p.user_id()).cloned();
        let mut processes: Vec<ProcessInfo> = self
            .system
            .processes()
            .values()
            // Linux 上的线程也在列表里，只要进程
            .filter(|p| p.thread_kind().is_none())
            .map(|p| ProcessInfo {
                pid: p.pid().as_u32(),
                name: display_name(&p.name().to_string_lossy(), p.exe()),
                cpu: (p.cpu_usage() / threads).clamp(0.0, 100.0),
                memory: p.memory(),
                exe: p.exe().map(|exe| exe.to_string_lossy().to_string()),
                protected: protected(p.pid(), self.own_pid, p.user_id(), own_user.as_ref()),
            })
            .collect();
        processes.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then(b.memory.cmp(&a.memory)));
        Sample {
            cpu: self.system.global_cpu_usage().clamp(0.0, 100.0),
            memory_used: self.system.used_memory(),
            memory_total: self.system.total_memory(),
            swap_used: self.system.used_swap(),
            swap_total: self.system.total_swap(),
            disk: system_disk(),
            uptime: System::uptime().unwrap_or(0),
            processes,
        }
    }

    /// 结束一个进程。`force` 为假时请它正常退出（macOS、Linux 发 SIGTERM，Windows 用 taskkill 不带 /F）；
    /// 为真时强制结束。不能结束的返回 `protected`，没有权限返回 `access_denied`，已经没了返回 `not_found`。
    pub fn end(&mut self, pid: u32, force: bool) -> Result<(), String> {
        let pid = Pid::from_u32(pid);
        self.system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid, self.own_pid]), false, process_kind());
        let own_user = self.system.process(self.own_pid).and_then(|p| p.user_id()).cloned();
        let process = self.system.process(pid).ok_or_else(|| "not_found".to_string())?;
        if protected(pid, self.own_pid, process.user_id(), own_user.as_ref()) {
            return Err("protected".into());
        }
        if force {
            return if process.kill() { Ok(()) } else { Err("access_denied".into()) };
        }
        match process.kill_with(Signal::Term) {
            Some(true) => Ok(()),
            Some(false) => Err("access_denied".into()),
            // Windows 没有 SIGTERM：taskkill 不带 /F 会让它像点了关闭一样退出
            None => graceful_windows(pid.as_u32()),
        }
    }
}

fn process_kind() -> ProcessRefreshKind {
    ProcessRefreshKind::nothing().with_cpu().with_memory().with_exe(UpdateKind::OnlyIfNotSet).with_user(UpdateKind::OnlyIfNotSet)
}

/// 不让在这里结束的：ShibaPack 自己、0/1/4 号（内核、init/launchd、Windows 的 System）、和 ShibaPack 不是同一个用户的。
fn protected(pid: Pid, own: Pid, user: Option<&sysinfo::Uid>, own_user: Option<&sysinfo::Uid>) -> bool {
    if pid == own || pid.as_u32() <= 4 {
        return true;
    }
    match (user, own_user) {
        (Some(user), Some(own_user)) => user != own_user,
        // 不知道是谁的：当作别人的
        _ => true,
    }
}

/// macOS 上进程名会被截短，在 .app 里的用应用名（“Google Chrome Helper (Renderer)” 这种辅助进程照旧用进程名）。
fn display_name(name: &str, exe: Option<&Path>) -> String {
    if let Some(exe) = exe {
        let text = exe.to_string_lossy();
        if let Some(i) = text.find(".app/Contents/MacOS/") {
            let bundle = &text[..i];
            let app = bundle.rsplit('/').next().unwrap_or(name);
            let binary = exe.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            // 主程序（和 .app 同名或是它的截短）用应用名
            if !bundle[..bundle.len() - app.len()].contains(".app/") && (binary == app || app.starts_with(name)) {
                return app.to_string();
            }
            return binary;
        }
    }
    name.to_string()
}

/// 系统盘：Windows 是 %SystemDrive%，其他系统是 /。
fn system_disk() -> Option<DiskUsage> {
    let mount = if cfg!(windows) { format!("{}\\", std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into())) } else { "/".to_string() };
    let disks = sysinfo::Disks::new_with_refreshed_list().ok()?;
    let disk = disks.iter().find(|d| d.mount_point().to_string_lossy().eq_ignore_ascii_case(&mount))?;
    Some(DiskUsage { mount, total: disk.total_space(), available: disk.available_space() })
}

#[cfg(windows)]
fn graceful_windows(pid: u32) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let status = std::process::Command::new("taskkill").args(["/PID", &pid.to_string()]).creation_flags(CREATE_NO_WINDOW).status().map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err("access_denied".into()) }
}

#[cfg(not(windows))]
fn graceful_windows(_pid: u32) -> Result<(), String> {
    Err("not_supported".into())
}

/// 进程的图标：macOS 是包着它的最外层 .app 的图标（辅助进程也用主程序的），Windows 是 exe 自己的图标。
/// 取出来的放在 `cache`（按来源和修改时间，取过就直接用）。取不到返回 `None`。
pub fn process_icon(exe: &Path, cache: &Path) -> Option<String> {
    if let Some(bundle) = outer_bundle(exe) {
        let declared = plist::Value::from_file(bundle.join("Contents/Info.plist"))
            .ok()
            .and_then(|info| info.as_dictionary().and_then(|d| d.get("CFBundleIconFile")).and_then(|v| v.as_string()).map(str::to_string));
        let icns = crate::installed::bundle_icon(&bundle, declared.as_deref())?;
        return crate::installed::icon_for(&icns, cache);
    }
    if exe.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("exe")) {
        return crate::installed::icon_for(exe, cache);
    }
    None
}

/// 路径里最外面的 .app：“/Applications/Google Chrome.app/…/Helper.app/Contents/MacOS/x” → “/Applications/Google Chrome.app”。
fn outer_bundle(exe: &Path) -> Option<PathBuf> {
    let text = exe.to_string_lossy();
    let end = text.find(".app/")? + ".app".len();
    Some(PathBuf::from(&text[..end]))
}

struct MacHardware {
    model: String,
    core_split: Option<(usize, usize)>,
}

/// macOS 的型号名和性能核、能效核：从 system_profiler 读（半秒左右，只读一次）。
fn mac_hardware() -> Option<MacHardware> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let output = std::process::Command::new("/usr/sbin/system_profiler").args(["SPHardwareDataType", "-json"]).output().ok()?;
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    parse_mac_hardware(&json)
}

fn parse_mac_hardware(json: &serde_json::Value) -> Option<MacHardware> {
    let item = json.get("SPHardwareDataType")?.get(0)?;
    let text = |key: &str| item.get(key).and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let (name, id) = (text("machine_name"), text("machine_model"));
    let model = match (name.is_empty(), id.is_empty()) {
        (false, false) => format!("{name} ({id})"),
        (false, true) => name,
        _ => id,
    };
    // “proc 14:10:4” = 一共 14 核，10 个性能核，4 个能效核
    let core_split = text("number_processors").strip_prefix("proc ").and_then(|rest| {
        let parts: Vec<usize> = rest.split(':').filter_map(|p| p.trim().parse().ok()).collect();
        (parts.len() == 3).then(|| (parts[1], parts[2]))
    });
    Some(MacHardware { model, core_split })
}

/// Windows、Linux 的型号：主板（SMBIOS）登记的厂商和产品名，厂商名已经在产品名里的不重复。
fn product_model() -> String {
    let vendor = sysinfo::Product::vendor_name().unwrap_or_default().trim().to_string();
    let name = sysinfo::Product::name().unwrap_or_default().trim().to_string();
    let junk = |s: &str| s.is_empty() || s.eq_ignore_ascii_case("to be filled by o.e.m.") || s.eq_ignore_ascii_case("system product name");
    match (junk(&vendor), junk(&name)) {
        (true, true) => String::new(),
        (true, false) => name,
        (false, true) => vendor,
        (false, false) if name.to_lowercase().starts_with(&vendor.to_lowercase()) => name,
        (false, false) => format!("{vendor} {name}"),
    }
}

/// 本机的硬盘：去掉系统内部的卷（macOS 的 /System/Volumes/…、Linux 的 /boot、/snap 这类）和重复的（同一块 APFS 上的几个卷），系统盘在前。
fn disks() -> Vec<DiskInfo> {
    let Ok(list) = sysinfo::Disks::new_with_refreshed_list() else { return Vec::new() };
    let system_mount = if cfg!(windows) { format!("{}\\", std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into())) } else { "/".to_string() };
    let mut out: Vec<DiskInfo> = Vec::new();
    for disk in list.iter() {
        let mount = disk.mount_point().to_string_lossy().to_string();
        let internal = ["/System/Volumes", "/private/var", "/boot", "/snap", "/run", "/dev", "/proc", "/sys", "/var/lib"].iter().any(|p| mount.starts_with(p));
        if internal || disk.total_space() == 0 {
            continue;
        }
        // 同一块盘上的几个卷大小一样：只留一个
        if out.iter().any(|d| d.total == disk.total_space() && d.name == disk.name().to_string_lossy()) {
            continue;
        }
        let kind = match disk.kind() {
            sysinfo::DiskKind::SSD => "SSD",
            sysinfo::DiskKind::HDD => "HDD",
            _ => "",
        };
        out.push(DiskInfo {
            name: disk.name().to_string_lossy().to_string(),
            mount,
            total: disk.total_space(),
            available: disk.available_space(),
            removable: disk.is_removable(),
            kind: kind.into(),
        });
    }
    out.sort_by_key(|d| (!d.mount.eq_ignore_ascii_case(&system_mount), d.removable, d.mount.clone()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_mac_hardware_from_system_profiler() {
        let json = serde_json::json!({"SPHardwareDataType": [{"machine_name": "MacBook Pro", "machine_model": "Mac16,1", "number_processors": "proc 14:10:4"}]});
        let hw = parse_mac_hardware(&json).unwrap();
        assert_eq!(hw.model, "MacBook Pro (Mac16,1)");
        assert_eq!(hw.core_split, Some((10, 4)));
        let intel = serde_json::json!({"SPHardwareDataType": [{"machine_name": "iMac", "number_processors": 8}]});
        let hw = parse_mac_hardware(&intel).unwrap();
        assert_eq!((hw.model.as_str(), hw.core_split), ("iMac", None));
    }

    #[test]
    fn outermost_app_bundle() {
        let exe = Path::new("/Applications/Google Chrome.app/Contents/Frameworks/X.framework/Helpers/Google Chrome Helper.app/Contents/MacOS/Google Chrome Helper");
        assert_eq!(outer_bundle(exe), Some(PathBuf::from("/Applications/Google Chrome.app")));
        assert_eq!(outer_bundle(Path::new("/bin/zsh")), None);
    }

    #[test]
    fn names_from_app_bundles() {
        let exe = |path: &str| Some(Path::new(path).to_path_buf());
        assert_eq!(display_name("Google Chrome", exe("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome").as_deref()), "Google Chrome");
        // 被截短的进程名用 .app 的名字
        assert_eq!(display_name("Visual Studio C", exe("/Applications/Visual Studio Code.app/Contents/MacOS/Electron").as_deref()), "Visual Studio Code");
        // 嵌在别的 .app 里的辅助进程用自己的名字
        assert_eq!(
            display_name(
                "Google Chrome H",
                exe("/Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Framework.framework/Helpers/Google Chrome Helper.app/Contents/MacOS/Google Chrome Helper")
                    .as_deref()
            ),
            "Google Chrome Helper"
        );
        assert_eq!(display_name("bash", exe("/bin/bash").as_deref()), "bash");
        assert_eq!(display_name("svchost.exe", None), "svchost.exe");
    }

    // Windows 的 Uid 是 SID，没法从数字构造；规则一样
    #[cfg(unix)]
    #[test]
    fn protects_itself_system_and_other_users() {
        let own = Pid::from_u32(500);
        let me = sysinfo::Uid::try_from(501usize).ok();
        let root = sysinfo::Uid::try_from(0usize).ok();
        assert!(protected(own, own, me.as_ref(), me.as_ref()));
        assert!(protected(Pid::from_u32(1), own, me.as_ref(), me.as_ref()));
        assert!(protected(Pid::from_u32(900), own, root.as_ref(), me.as_ref()));
        assert!(protected(Pid::from_u32(900), own, None, me.as_ref()));
        assert!(!protected(Pid::from_u32(900), own, me.as_ref(), me.as_ref()));
    }

    /// 真的读一次本机：总量、自己这个进程、系统盘都应该有。
    #[test]
    fn samples_this_machine() {
        let mut monitor = Monitor::new().unwrap();
        let overview = monitor.overview();
        assert!(overview.total_memory > 0 && overview.cpu_threads > 0);
        assert!(!overview.disks.is_empty());
        if cfg!(target_os = "macos") {
            assert!(!overview.model.is_empty() && !overview.gpus.is_empty());
        }
        let sample = monitor.sample();
        assert!(sample.memory_used > 0 && sample.memory_used <= sample.memory_total);
        let me = sample.processes.iter().find(|p| p.pid == std::process::id()).expect("own process listed");
        assert!(me.protected);
        assert!(sample.disk.is_some_and(|d| d.total > 0));
        assert!(monitor.end(std::process::id(), false).is_err());
    }
}
