//! 本机的配置和实时状态：主页上的系统信息、CPU、内存、显卡、网络、磁盘，和任务管理器里的进程。
//!
//! 用 sysinfo（MIT）读，什么都不存：主页开着、窗口在前面时界面每 2 秒要一次，切走就停。
//! 结束进程只结束当前用户自己的、不是 ShibaPack 自己的；先请它正常退出，强制结束要另外点。

use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;
use sysinfo::{CpuRefreshKind, DiskRefreshKind, Disks, Gpus, Networks, MemoryRefreshKind, Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, Signal, System, UpdateKind};

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
    /// 最忙的那块显卡。读不到占用时为空（Windows 第一次总是读不到，要和上一次比）。
    pub gpu: Option<GpuLoad>,
    /// 每秒收发的字节数，不含回环和虚拟网卡。
    pub network: Option<NetworkRate>,
    /// 每秒读写硬盘的字节数，所有硬盘加起来。
    pub disk_io: Option<DiskIo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuLoad {
    pub model: String,
    /// 0–100。
    pub usage: f32,
    /// 独立显存用了多少、一共多少。和内存共用的（Apple 芯片、核显）为空。
    pub memory_used: Option<u64>,
    pub memory_total: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkRate {
    pub received: u64,
    pub sent: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskIo {
    pub read: u64,
    pub written: u64,
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

/// sysinfo 的 `Gpus` 在 Windows 上带着 PDH 查询句柄、在 Linux 上带着 NVML 库的句柄，都是裸指针，所以不是 `Send`。
/// 这两种句柄都不限定只能在创建它的线程上用；Monitor 在主程序里放在 Mutex 里，同一时间只有一个线程碰它。
struct SendGpus(Gpus);

// SAFETY: 见上面。只通过 `&mut Monitor` 访问，从不同时在两个线程上用。
unsafe impl Send for SendGpus {}

// 主程序把 Monitor 放在跨线程共享的状态里（Tauri 要求 Send）。在这里就检查，哪个系统上不满足都编译不过，
// 不用等到打包时才发现（Windows 上 sysinfo 的 Gpus 就不是 Send）。
const _: fn() = || {
    fn send<T: Send>() {}
    send::<Monitor>();
};

/// 留着一个 `System`：CPU 占用要和上一次比才算得出来。
pub struct Monitor {
    system: System,
    own_pid: Pid,
    /// 型号这些要跑 system_profiler，读一次就留着。
    overview: Option<Overview>,
    /// 网络、硬盘读写、显卡占用也要和上一次比。
    networks: Option<Networks>,
    disks: Option<Disks>,
    gpus: Option<SendGpus>,
    /// 上一次量网络和硬盘读写的时间，算每秒多少用。
    last_io: Instant,
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
        let networks = Networks::new_with_refreshed_list().ok();
        let disks = Disks::new_with_refreshed_list_specifics(DiskRefreshKind::nothing().with_io_usage()).ok();
        let gpus = Gpus::new_with_refreshed_list().ok().map(SendGpus);
        let last_io = Instant::now();
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        system.refresh_cpu_usage();
        system.refresh_processes_specifics(ProcessesToUpdate::All, true, process_kind());
        Ok(Monitor { system, own_pid: Pid::from_u32(std::process::id()), overview: None, networks, disks, gpus, last_io })
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
        let (network, disk_io) = self.io_rates();
        Sample {
            gpu: self.gpu_load(),
            network,
            disk_io,
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

    /// 从上一次到现在每秒收发、读写了多少。
    fn io_rates(&mut self) -> (Option<NetworkRate>, Option<DiskIo>) {
        let now = Instant::now();
        let seconds = now.duration_since(self.last_io).as_secs_f64();
        self.last_io = now;
        let per_second = |bytes: u64| if seconds > 0.0 { (bytes as f64 / seconds).round() as u64 } else { 0 };
        let network = self.networks.as_mut().map(|networks| {
            networks.refresh(true);
            let (received, sent) = networks
                .iter()
                .filter(|(name, _)| !virtual_interface(&name.to_string_lossy()))
                .fold((0, 0), |(r, s), (_, data)| (r + data.received(), s + data.transmitted()));
            NetworkRate { received: per_second(received), sent: per_second(sent) }
        });
        let disk_io = self.disks.as_mut().map(|disks| {
            disks.refresh_specifics(true, DiskRefreshKind::nothing().with_io_usage());
            // macOS 上同一块硬盘的几个卷报的是整块硬盘的数，累计值一样的只算一次
            let mut seen = HashSet::new();
            let (read, written) = disks
                .iter()
                .map(|disk| disk.usage())
                .filter(|usage| seen.insert((usage.total_read_bytes, usage.total_written_bytes)))
                .fold((0, 0), |(r, w), usage| (r + usage.read_bytes, w + usage.written_bytes));
            DiskIo { read: per_second(read), written: per_second(written) }
        });
        (network, disk_io)
    }

    /// 占用最高的那块显卡。
    fn gpu_load(&mut self) -> Option<GpuLoad> {
        let gpus = &mut self.gpus.as_mut()?.0;
        gpus.refresh(false);
        gpus.iter()
            .filter_map(|gpu| Some((gpu, gpu.usage()?)))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(gpu, usage)| GpuLoad {
                model: gpu.model().unwrap_or_default().trim().to_string(),
                usage: usage.clamp(0.0, 100.0),
                memory_used: gpu.used_memory(),
                memory_total: gpu.total_memory().filter(|m| *m > 0),
            })
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

/// 回环和虚拟网卡：VPN、虚拟机、容器的流量也会从真网卡走一遍，算上就重复了。
fn virtual_interface(name: &str) -> bool {
    let lower = name.to_lowercase();
    const PREFIXES: &[&str] = &[
        "utun", "ipsec", "gif", "stf", "awdl", "llw", "bridge", "anpi", "ap", "vmnet", "vboxnet", "docker", "veth", "br-", "virbr",
        "tun", "tap", "wg", "zt", "vethernet", "isatap", "teredo",
    ];
    // lo、lo0 是回环；“Local Area Connection” 不是
    let loopback = lower.strip_prefix("lo").is_some_and(|rest| rest.chars().all(|c| c.is_ascii_digit()));
    loopback || PREFIXES.iter().any(|prefix| lower.starts_with(prefix))
        || ["loopback", "virtual", "vmware", "virtualbox", "hyper-v", "pseudo"].iter().any(|word| lower.contains(word))
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
    fn skips_loopback_and_virtual_interfaces() {
        for name in ["lo", "lo0", "utun3", "awdl0", "bridge100", "docker0", "vEthernet (WSL)", "Loopback Pseudo-Interface 1", "VMware Network Adapter VMnet8"] {
            assert!(virtual_interface(name), "{name}");
        }
        for name in ["en0", "eth0", "wlan0", "Wi-Fi", "Ethernet", "Local Area Connection", "WLAN"] {
            assert!(!virtual_interface(name), "{name}");
        }
    }

    /// 在这台电脑上量几次，看看显卡、网络、硬盘读写读不读得到：
    /// `cargo test -p shibapack-core samples_io_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn samples_io_live() {
        let mut monitor = Monitor::new().unwrap();
        for _ in 0..3 {
            std::thread::sleep(std::time::Duration::from_secs(1));
            let sample = monitor.sample();
            eprintln!("gpu {:?} · net {:?} · disk {:?}", sample.gpu, sample.network, sample.disk_io);
        }
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
