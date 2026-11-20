//! 启动项与服务：登录或开机时自动运行的程序，以及在后台运行的系统服务。
//!
//! | 系统 | 启动项 | 服务 |
//! |---|---|---|
//! | macOS | 登录项（系统设置里的“登录时打开”）、LaunchAgents | LaunchDaemons |
//! | Windows | 注册表的 Run 键、“启动”文件夹 | Win32 服务 |
//! | Linux | XDG 自动启动（autostart 文件夹里的 .desktop） | systemd 的 service 单元（系统的和用户的） |
//!
//! 停用的做法和系统自带的工具一样，都能原样恢复：
//! - macOS：launchd 的项用 `launchctl disable`，同时卸下（停止）。登录项系统里没有“停用”，只能从列表里删掉，
//!   所以删之前把它记在本机的 `startup-disabled.json` 里，重新启用时按记下的路径加回去。
//! - Windows：和任务管理器一样写 `StartupApproved`，Run 键和快捷方式本身不动；服务改启动类型（`sc.exe config`）。
//! - Linux：在用户的 autostart 文件夹里放一份 `Hidden=true` 的同名文件；服务 `systemctl disable --now`。
//!
//! 需要管理员权限的改动会弹出系统的授权提示：macOS 输入密码，Windows 是 UAC，Linux 是 pkexec。
//!
//! 解析各种系统输出的函数不按系统区分编译，在任何系统上都能测试。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use crate::installed::InstalledApp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// 登录时自动运行的程序。
    Startup,
    /// 后台服务。
    Service,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    /// 只对当前用户。
    User,
    /// 这台电脑的所有用户。
    Machine,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupItem {
    /// 稳定的标识：plist 的路径、注册表位置、服务名、单元名。
    pub id: String,
    pub kind: Kind,
    /// 登记在哪一类地方：loginItem / launchAgent / launchDaemon / registry / startupFolder / service / autostart / systemd
    pub source: String,
    /// 显示的名字。
    pub name: String,
    /// 系统内部的名字：launchd 的 Label、Windows 的服务名、systemd 的单元名、注册表的值名、文件名。
    pub label: String,
    /// 系统或程序自带的说明（Windows 服务的描述、systemd 的 Description、.desktop 的 Comment、exe 的文件说明）。
    pub description: String,
    pub publisher: String,
    /// 运行的命令。
    pub command: String,
    /// 在哪登记的：plist 文件、注册表键、快捷方式、单元文件。
    pub location: String,
    pub scope: Scope,
    /// 系统自带的（Apple、Microsoft、发行版的核心服务）。界面默认不列出来，改动前要再确认一次。
    pub system: bool,
    /// 会不会自动运行。
    pub enabled: bool,
    pub can_toggle: bool,
    /// 服务的启动方式。Windows：auto / delayed / manual / disabled；systemd：enabled / disabled / static / masked …
    pub start_type: Option<String>,
    /// 可以改成哪几种启动方式。只有 Windows 的服务有，别的用开关。
    pub start_types: Vec<String>,
    /// 正在运行。不知道时（登录项、注册表启动项）为 `None`。
    pub running: Option<bool>,
    pub pid: Option<u32>,
    /// 可以启动、停止。
    pub can_control: bool,
    /// 改动它要管理员权限，会弹出系统的授权提示。
    pub needs_admin: bool,
    /// 它属于哪个应用（macOS：程序在哪个 .app 里，或 plist 里写的关联应用）。
    pub app: String,
    /// 图标文件的绝对路径。
    pub icon: Option<String>,
    /// 什么时候运行（launchd）：login、load、keepAlive、interval:<秒>、calendar、watch、demand、hidden。
    pub triggers: Vec<String>,
    /// 以哪个账户运行（Windows 服务、systemd 的 User=）。
    pub account: String,
    /// 程序文件，“打开所在位置”时用。
    #[serde(skip)]
    pub program: Option<PathBuf>,
    #[serde(skip)]
    pub(crate) icon_source: Option<PathBuf>,
    #[serde(skip)]
    pub(crate) handle: Handle,
}

/// 改动时要用到的、各系统自己的信息。
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub(crate) enum Handle {
    #[default]
    None,
    /// macOS 登录项。
    LoginItem { path: String, hidden: bool },
    /// launchd：`domain` 是 `gui/501` 或 `system`。
    Launchd { domain: String, label: String, plist: PathBuf },
    /// Windows 注册表 Run 键里的一个值。`approved` 是 StartupApproved 下对应的子键（Run、Run32）。
    RunValue { machine: bool, approved: &'static str, value: String },
    /// Windows “启动”文件夹里的一个文件。
    StartupFile { machine: bool, file: String },
    WinService { name: String },
    /// Linux 自动启动：用户文件夹里的那份（有的话）和系统的那份（有的话）。
    Autostart { file_name: String, user_file: Option<PathBuf>, system_file: Option<PathBuf> },
    Systemd { unit: String, user: bool },
}

impl StartupItem {
    fn new(id: String, kind: Kind, source: &str, name: String) -> StartupItem {
        StartupItem {
            id,
            kind,
            source: source.to_string(),
            label: name.clone(),
            name,
            description: String::new(),
            publisher: String::new(),
            command: String::new(),
            location: String::new(),
            scope: Scope::User,
            system: false,
            enabled: true,
            can_toggle: true,
            start_type: None,
            start_types: Vec::new(),
            running: None,
            pid: None,
            can_control: false,
            needs_admin: false,
            app: String::new(),
            icon: None,
            triggers: Vec::new(),
            account: String::new(),
            program: None,
            icon_source: None,
            handle: Handle::None,
        }
    }
}

/// 列出来的结果。`notes` 是列的时候遇到的问题（给界面翻译的短代码），比如 `login_items_denied`。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub items: Vec<StartupItem>,
    pub notes: Vec<String>,
}

/// 要做的事。界面传来的是字符串：enable、disable、start、stop、restart、type:auto 这样。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Enable,
    Disable,
    Start,
    Stop,
    Restart,
    /// 改服务的启动类型（只有 Windows 的服务）：auto / delayed / manual / disabled。
    StartType(String),
}

impl Action {
    pub fn parse(text: &str) -> Option<Action> {
        Some(match text {
            "enable" => Action::Enable,
            "disable" => Action::Disable,
            "start" => Action::Start,
            "stop" => Action::Stop,
            "restart" => Action::Restart,
            other => {
                let kind = other.strip_prefix("type:")?;
                if !["auto", "delayed", "manual", "disabled"].contains(&kind) {
                    return None;
                }
                Action::StartType(kind.to_string())
            }
        })
    }
}

/// 列出这台电脑上的启动项和服务。
///
/// `icon_cache` 放取出来的图标；`state_dir` 是本机的应用数据文件夹（macOS 记停用的登录项）；
/// `apps` 是已经列出来的已安装软件，用来认出启动项属于哪个应用、借它的图标。
/// `login_items` 为假时不列 macOS 的登录项：要通过“系统事件”，第一次会弹出授权提示（自检时不想要）。
pub fn list(icon_cache: &Path, state_dir: &Path, apps: &[InstalledApp], login_items: bool) -> Listing {
    let mut listing = platform::list(state_dir, apps, login_items);
    for item in &mut listing.items {
        if item.icon.is_none() {
            item.icon = item.icon_source.as_deref().and_then(|source| crate::installed::icon_for(source, icon_cache));
        }
    }
    listing.items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.id.cmp(&b.id)));
    listing
}

/// 对一项做一件事。出错时返回给界面翻译的短代码：`cancelled`（在授权提示里点了取消）、`not_supported`，
/// 或者 `startup_failed: 系统的原话`。
pub fn apply(item: &StartupItem, action: &Action, state_dir: &Path) -> Result<(), String> {
    match action {
        Action::Enable | Action::Disable if !item.can_toggle => return Err("not_supported".into()),
        Action::Start | Action::Stop | Action::Restart if !item.can_control => return Err("not_supported".into()),
        Action::StartType(kind) if !item.start_types.contains(kind) => return Err("not_supported".into()),
        _ => {}
    }
    platform::apply(item, action, state_dir)
}

/// 交给 AI 解释时带上的信息：界面上能看到的那些，一行一项，英文的键。
pub fn facts(item: &StartupItem) -> String {
    let os = if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(windows) {
        "Windows"
    } else {
        "Linux"
    };
    let kind = match item.source.as_str() {
        "loginItem" => "login item (opened when the user logs in)",
        "launchAgent" => "launchd agent (runs in the user's session)",
        "launchDaemon" => "launchd daemon (system-wide background service, runs as root)",
        "registry" => "startup entry in the registry Run key",
        "startupFolder" => "shortcut in the Startup folder",
        "service" => "Windows service",
        "autostart" => "XDG autostart entry",
        "systemd" => "systemd service unit",
        other => other,
    };
    let mut lines = vec![format!("Operating system: {os}"), format!("Type: {kind}"), format!("Name: {}", item.name)];
    let mut push = |key: &str, value: &str| {
        if !value.trim().is_empty() {
            lines.push(format!("{key}: {}", value.trim()));
        }
    };
    if item.label != item.name {
        push("Internal name", &item.label);
    }
    push("Description", &item.description);
    push("Publisher", &item.publisher);
    push("Belongs to app", &item.app);
    push("Command", &item.command.chars().take(400).collect::<String>());
    push("Registered at", &item.location);
    push("When it runs", &item.triggers.join(", "));
    push("Start type", item.start_type.as_deref().unwrap_or(""));
    push("Runs as", &item.account);
    if let Some(running) = item.running {
        push("Currently", if running { "running" } else { "not running" });
    }
    push("Shipped with the operating system", if item.system { "yes" } else { "" });
    lines.join("\n")
}

// ── 通用的小工具 ─────────────────────────────────────────────

/// 准备运行一个系统命令。Windows 上不弹出黑色的命令行窗口。
fn command(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command
}

/// 运行一个命令，最多等 `limit`。超时就结束它，返回 `None`。
pub(crate) fn output_within(mut command: Command, limit: Duration) -> Option<Output> {
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().ok()?;
    // 输出要边跑边读，不然输出多了会把管道塞满，命令卡住
    let mut stdout = child.stdout.take()?;
    let mut stderr = child.stderr.take()?;
    let out = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let err = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf);
        buf
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < limit => std::thread::sleep(Duration::from_millis(40)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    Some(Output { status, stdout: out.join().unwrap_or_default(), stderr: err.join().unwrap_or_default() })
}

/// 运行一个命令并等它结束（不限时：可能在等用户在授权提示里输入密码）。
#[allow(dead_code)]
fn output(mut command: Command) -> Result<Output, String> {
    command.stdin(Stdio::null()).output().map_err(|e| e.to_string())
}

/// 命令失败时的原话：先看错误输出，没有就看普通输出。
#[allow(dead_code)]
fn failure(output: &Output) -> String {
    let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let text = if err.is_empty() { String::from_utf8_lossy(&output.stdout).trim().to_string() } else { err };
    let text = if text.is_empty() { format!("exit {}", output.status.code().unwrap_or(-1)) } else { text };
    format!("startup_failed: {}", text.lines().last().unwrap_or(&text).trim())
}

/// 把命令拼成一行给人看：带空格的参数加引号。
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn join_command(args: &[String]) -> String {
    args.iter()
        .map(|arg| if arg.is_empty() || arg.contains(char::is_whitespace) { format!("\"{arg}\"") } else { arg.clone() })
        .collect::<Vec<_>>()
        .join(" ")
}

/// sh 的单引号字符串。
#[allow(dead_code)]
fn sh_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// AppleScript 的双引号字符串。
#[allow(dead_code)]
fn applescript_quote(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// PowerShell 的单引号字符串。
#[allow(dead_code)]
fn ps_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// PowerShell 的 `-EncodedCommand` 要的格式：UTF-16LE 再 Base64。省得为各种引号转义操心。
#[allow(dead_code)]
fn ps_encode(script: &str) -> String {
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    base64(&bytes)
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

// ── macOS：launchd ───────────────────────────────────────────

/// `launchctl print-disabled <domain>` 的输出：Label → 是否停用。
/// 新系统写 `=> disabled` / `=> enabled`，老系统写 `=> true` / `=> false`。
pub fn parse_print_disabled(text: &str) -> HashMap<String, bool> {
    let mut map = HashMap::new();
    for line in text.lines() {
        let Some((label, state)) = line.trim().split_once("=>") else { continue };
        let label = label.trim().trim_matches('"');
        let disabled = match state.trim() {
            "disabled" | "true" => true,
            "enabled" | "false" => false,
            _ => continue,
        };
        if !label.is_empty() {
            map.insert(label.to_string(), disabled);
        }
    }
    map
}

/// `launchctl print <domain>` 里 `services = { … }` 一段：Label → 进程号（0 是没在运行）。
/// 每行是 `进程号  上次退出状态  Label`。
pub fn parse_domain_services(text: &str) -> HashMap<String, u32> {
    let mut map = HashMap::new();
    let mut inside = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if !inside {
            inside = trimmed == "services = {";
            continue;
        }
        if trimmed == "}" {
            break;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        if let Ok(pid) = parts[0].parse::<u32>() {
            map.insert(parts[parts.len() - 1].to_string(), pid);
        }
    }
    map
}

/// `launchctl print <domain>/<label>` 里的程序和参数。读不了 plist（有的只有 root 能读）时从这里拿。
pub fn parse_service_program(text: &str) -> Vec<String> {
    let mut program = None;
    let mut args = Vec::new();
    let mut in_args = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if in_args {
            if trimmed == "}" {
                break;
            }
            args.push(trimmed.to_string());
        } else if trimmed == "arguments = {" {
            in_args = true;
        } else if let Some(value) = trimmed.strip_prefix("program = ") {
            program.get_or_insert_with(|| value.to_string());
        }
    }
    if args.is_empty() {
        args.extend(program);
    }
    args
}

/// 命令里第一个 .app 的路径（最外层的那个）：`/Applications/Foo.app/Contents/MacOS/foo` → `/Applications/Foo.app`。
/// 命令可能是 `/bin/sh -c "/Applications/Foo.app/…"`，所以每个参数都看。
pub fn bundle_in(args: &[String]) -> Option<PathBuf> {
    for arg in args {
        let lower = arg.to_ascii_lowercase();
        let Some(at) = lower.find(".app/").or_else(|| lower.ends_with(".app").then(|| lower.len() - 4)) else { continue };
        let before = &arg[..at];
        // 参数里可能还有别的东西（sh -c 后面的整条命令），从 .app 往前找到这段路径的开头
        let start = before.rfind(" /").map(|i| i + 1).or_else(|| before.starts_with('/').then_some(0));
        if let Some(start) = start {
            return Some(PathBuf::from(&arg[start..at + 4]));
        }
    }
    None
}

/// 从 launchd 的 plist 里读出的东西。
#[derive(Debug, Default, PartialEq)]
pub struct LaunchdPlist {
    pub label: String,
    pub args: Vec<String>,
    pub disabled: bool,
    pub triggers: Vec<String>,
    pub bundle_ids: Vec<String>,
    pub user: String,
}

pub fn parse_launchd_plist(value: &plist::Value) -> Option<LaunchdPlist> {
    let dict = value.as_dictionary()?;
    let text = |key: &str| dict.get(key).and_then(plist::Value::as_string).map(str::to_string);
    let truthy = |key: &str| match dict.get(key) {
        Some(plist::Value::Boolean(b)) => *b,
        Some(plist::Value::Dictionary(d)) => !d.is_empty(),
        Some(plist::Value::Array(a)) => !a.is_empty(),
        Some(_) => true,
        None => false,
    };
    let mut args: Vec<String> = dict
        .get("ProgramArguments")
        .and_then(plist::Value::as_array)
        .map(|list| list.iter().filter_map(|v| v.as_string().map(str::to_string)).collect())
        .unwrap_or_default();
    if let Some(program) = text("Program") {
        // 同时写了 Program 时，它才是真正运行的程序，ProgramArguments 只是参数
        if args.first() != Some(&program) {
            if args.is_empty() {
                args.push(program);
            } else {
                args[0] = program;
            }
        }
    }
    let mut triggers = Vec::new();
    if truthy("RunAtLoad") {
        triggers.push("load".to_string());
    }
    if truthy("KeepAlive") {
        triggers.push("keepAlive".to_string());
    }
    if let Some(seconds) = dict.get("StartInterval").and_then(plist::Value::as_unsigned_integer) {
        triggers.push(format!("interval:{seconds}"));
    }
    if truthy("StartCalendarInterval") {
        triggers.push("calendar".to_string());
    }
    if truthy("WatchPaths") || truthy("QueueDirectories") || truthy("StartOnMount") {
        triggers.push("watch".to_string());
    }
    if triggers.is_empty() {
        // 没写什么时候运行：有别的程序来找它（MachServices、Sockets）时才启动
        triggers.push("demand".to_string());
    }
    let bundle_ids = match dict.get("AssociatedBundleIdentifiers") {
        Some(plist::Value::String(id)) => vec![id.clone()],
        Some(plist::Value::Array(list)) => list.iter().filter_map(|v| v.as_string().map(str::to_string)).collect(),
        _ => Vec::new(),
    };
    Some(LaunchdPlist {
        label: text("Label").unwrap_or_default(),
        args,
        disabled: matches!(dict.get("Disabled"), Some(plist::Value::Boolean(true))),
        triggers,
        bundle_ids,
        user: text("UserName").unwrap_or_default(),
    })
}

/// 停用的登录项：系统里没有“停用”，删掉之前记在这里，重新启用时加回去。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DisabledLogin {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub hidden: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalState {
    #[serde(default)]
    login_items: Vec<DisabledLogin>,
}

const STATE_FILE: &str = "startup-disabled.json";

#[allow(dead_code)]
fn read_state(dir: &Path) -> LocalState {
    std::fs::read(dir.join(STATE_FILE)).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

/// 先写临时文件再换上，写到一半断电也不会把原来的记录弄坏。
#[allow(dead_code)]
fn write_state(dir: &Path, state: &LocalState) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let body = serde_json::to_vec_pretty(state).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!("{STATE_FILE}.tmp"));
    std::fs::write(&tmp, body).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dir.join(STATE_FILE)).map_err(|e| e.to_string())
}

/// 登录项列表的输出：每行 `名字<TAB>路径<TAB>是否隐藏`。
pub fn parse_login_items(text: &str) -> Vec<DisabledLogin> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let name = parts.next()?.trim().to_string();
            let path = parts.next()?.trim().to_string();
            let hidden = parts.next().is_some_and(|h| h.trim() == "true");
            (!path.is_empty() && path != "missing value").then_some(DisabledLogin { name, path, hidden })
        })
        .collect()
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    /// 列登录项最多等多久。第一次会弹出“允许 ShibaPack 控制系统事件”的提示，用户不理它也不能让列表一直出不来。
    const LOGIN_ITEMS_LIMIT: Duration = Duration::from_secs(20);

    fn uid() -> u32 {
        std::env::var_os("HOME").and_then(|home| std::fs::metadata(home).ok()).map(|meta| meta.uid()).unwrap_or(501)
    }

    fn launchctl(args: &[&str]) -> Option<String> {
        let mut cmd = command("launchctl");
        cmd.args(args);
        let out = output_within(cmd, Duration::from_secs(15))?;
        Some(String::from_utf8_lossy(&out.stdout).to_string())
    }

    /// 一个 .app 的名字、发布者和图标来源。
    struct Bundle {
        name: String,
        publisher: String,
        icon: Option<String>,
        icon_source: Option<PathBuf>,
    }

    fn bundle_info(bundle: &Path, apps: &[InstalledApp]) -> Option<Bundle> {
        if let Some(app) = apps.iter().find(|app| app.install_dir.as_deref() == Some(bundle)) {
            return Some(Bundle { name: app.name.clone(), publisher: app.publisher.clone(), icon: app.icon.clone(), icon_source: app.icon_source.clone() });
        }
        if !bundle.is_dir() {
            return None;
        }
        let info = plist::Value::from_file(bundle.join("Contents/Info.plist")).ok().and_then(plist::Value::into_dictionary);
        let text = |key: &str| info.as_ref().and_then(|d| d.get(key)).and_then(plist::Value::as_string).map(str::trim).filter(|s| !s.is_empty());
        let mut publisher = text("NSHumanReadableCopyright").map(crate::installed::publisher_from_copyright).unwrap_or_default();
        if publisher.is_empty() {
            publisher = text("CFBundleIdentifier").map(crate::installed::publisher_from_bundle_id).unwrap_or_default();
        }
        Some(Bundle {
            name: bundle.file_stem()?.to_string_lossy().to_string(),
            publisher,
            icon: None,
            icon_source: crate::installed::bundle_icon(bundle, text("CFBundleIconFile")),
        })
    }

    pub fn list(state_dir: &Path, apps: &[InstalledApp], with_login_items: bool) -> Listing {
        let mut items = launchd_items(apps);
        let mut notes = Vec::new();
        if !with_login_items {
            return Listing { items, notes };
        }
        match login_items() {
            Ok(list) => items.extend(login_entries(list, state_dir, apps)),
            Err(note) => {
                notes.push(note);
                // 列不出来时，至少把停用过的那些列出来，免得以为它们没了
                items.extend(login_entries(Vec::new(), state_dir, apps));
            }
        }
        Listing { items, notes }
    }

    /// LaunchAgents 和 LaunchDaemons。
    pub(super) fn launchd_items(apps: &[InstalledApp]) -> Vec<StartupItem> {
        let uid = uid();
        let gui = format!("gui/{uid}");
        let gui_disabled = launchctl(&["print-disabled", &gui]).map(|t| parse_print_disabled(&t)).unwrap_or_default();
        let system_disabled = launchctl(&["print-disabled", "system"]).map(|t| parse_print_disabled(&t)).unwrap_or_default();
        let gui_running = launchctl(&["print", &gui]).map(|t| parse_domain_services(&t)).unwrap_or_default();
        let system_running = launchctl(&["print", "system"]).map(|t| parse_domain_services(&t)).unwrap_or_default();

        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        // （文件夹，类型，范围，系统自带，所在的 launchd 域）
        let roots = [
            (home.join("Library/LaunchAgents"), Kind::Startup, Scope::User, false, false),
            (PathBuf::from("/Library/LaunchAgents"), Kind::Startup, Scope::Machine, false, false),
            (PathBuf::from("/System/Library/LaunchAgents"), Kind::Startup, Scope::Machine, true, false),
            (PathBuf::from("/Library/LaunchDaemons"), Kind::Service, Scope::Machine, false, true),
            (PathBuf::from("/System/Library/LaunchDaemons"), Kind::Service, Scope::Machine, true, true),
        ];
        let mut items = Vec::new();
        for (root, kind, scope, system, daemon) in roots {
            let (domain, disabled, running) =
                if daemon { ("system".to_string(), &system_disabled, &system_running) } else { (gui.clone(), &gui_disabled, &gui_running) };
            let mut files: Vec<PathBuf> = std::fs::read_dir(&root)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|ext| ext == "plist"))
                .collect();
            files.sort();
            for plist in files {
                let stem = plist.file_stem().unwrap_or_default().to_string_lossy().to_string();
                let parsed = plist::Value::from_file(&plist).ok().as_ref().and_then(parse_launchd_plist);
                let readable = parsed.is_some();
                let mut info = parsed.unwrap_or_default();
                if info.label.is_empty() {
                    info.label = stem.clone();
                }
                if !readable {
                    // 只有 root 能读的 plist：程序和参数从 launchd 那里问
                    info.args = launchctl(&["print", &format!("{domain}/{}", info.label)]).map(|t| parse_service_program(&t)).unwrap_or_default();
                }
                let source = if daemon { "launchDaemon" } else { "launchAgent" };
                let mut item = StartupItem::new(plist.to_string_lossy().to_string(), kind, source, info.label.clone());
                item.command = join_command(&info.args);
                item.location = plist.to_string_lossy().to_string();
                item.scope = scope;
                item.system = system;
                item.enabled = disabled.get(&info.label).map_or(!info.disabled, |off| !off);
                item.can_control = true;
                item.needs_admin = daemon;
                let pid = running.get(&info.label).copied();
                item.running = Some(pid.is_some_and(|pid| pid > 0));
                item.pid = pid.filter(|pid| *pid > 0);
                item.triggers = if readable { info.triggers.clone() } else { Vec::new() };
                item.account = if daemon && info.user.is_empty() { "root".to_string() } else { info.user.clone() };
                item.program = info.args.first().map(PathBuf::from).filter(|p| p.is_absolute());

                // 属于哪个应用：程序在哪个 .app 里；不在 .app 里的看 plist 写的关联应用
                let bundle = bundle_in(&info.args).and_then(|b| bundle_info(&b, apps)).or_else(|| {
                    info.bundle_ids.iter().find_map(|id| {
                        let app = apps.iter().find(|app| app.bundle_id.as_deref() == Some(id.as_str()))?;
                        Some(Bundle { name: app.name.clone(), publisher: app.publisher.clone(), icon: app.icon.clone(), icon_source: app.icon_source.clone() })
                    })
                });
                match bundle {
                    Some(bundle) => {
                        item.app = bundle.name;
                        item.publisher = bundle.publisher;
                        item.icon = bundle.icon;
                        item.icon_source = bundle.icon_source;
                    }
                    None => item.publisher = if system { "Apple".to_string() } else { crate::installed::publisher_from_bundle_id(&info.label) },
                }
                item.handle = Handle::Launchd { domain: domain.clone(), label: info.label, plist };
                items.push(item);
            }
        }
        items
    }

    /// 系统设置里“登录时打开”的那些。要通过“系统事件”问，第一次会请用户允许。
    fn login_items() -> Result<Vec<DisabledLogin>, String> {
        let script = r#"set out to ""
tell application "System Events"
  repeat with li in login items
    try
      set out to out & (name of li) & tab & (POSIX path of (path of li)) & tab & ((hidden of li) as text) & linefeed
    end try
  end repeat
end tell
return out"#;
        let mut cmd = command("osascript");
        cmd.args(["-e", script]);
        let out = output_within(cmd, LOGIN_ITEMS_LIMIT).ok_or_else(|| "login_items_timeout".to_string())?;
        if out.status.success() {
            return Ok(parse_login_items(&String::from_utf8_lossy(&out.stdout)));
        }
        let err = String::from_utf8_lossy(&out.stderr);
        // -1743：没有允许控制“系统事件”；-1728、-600：系统事件没起来
        Err(if err.contains("-1743") { "login_items_denied" } else { "login_items_failed" }.to_string())
    }

    fn login_entries(live: Vec<DisabledLogin>, state_dir: &Path, apps: &[InstalledApp]) -> Vec<StartupItem> {
        let state = read_state(state_dir);
        let mut entries: Vec<(DisabledLogin, bool)> = live.into_iter().map(|item| (item, true)).collect();
        for item in state.login_items {
            // 停用后又在系统设置里加回去了：以系统里的为准
            if !entries.iter().any(|(live, _)| live.path == item.path) && Path::new(&item.path).exists() {
                entries.push((item, false));
            }
        }
        entries
            .into_iter()
            .map(|(entry, enabled)| {
                let path = PathBuf::from(entry.path.trim_end_matches('/'));
                let mut item = StartupItem::new(format!("login:{}", path.display()), Kind::Startup, "loginItem", entry.name.clone());
                item.command = path.to_string_lossy().to_string();
                item.location = path.to_string_lossy().to_string();
                item.enabled = enabled;
                item.program = Some(path.clone());
                item.triggers = if entry.hidden { vec!["login".into(), "hidden".into()] } else { vec!["login".into()] };
                let bundle = path.extension().is_some_and(|ext| ext == "app").then(|| bundle_info(&path, apps)).flatten();
                if let Some(bundle) = bundle {
                    item.app = bundle.name;
                    item.publisher = bundle.publisher;
                    item.icon = bundle.icon;
                    item.icon_source = bundle.icon_source;
                }
                item.handle = Handle::LoginItem { path: entry.path, hidden: entry.hidden };
                item
            })
            .collect()
    }

    pub fn apply(item: &StartupItem, action: &Action, state_dir: &Path) -> Result<(), String> {
        match &item.handle {
            Handle::LoginItem { path, hidden } => login_toggle(&item.name, path, *hidden, action, state_dir),
            // 停用了的 launchd 任务加载不了（launchctl 只报“Input/output error”），先说清楚原因
            Handle::Launchd { .. } if matches!(action, Action::Start | Action::Restart) && !item.enabled => Err("startup_disabled".into()),
            Handle::Launchd { domain, label, plist } => launchd_apply(domain, label, plist, action),
            _ => Err("not_supported".into()),
        }
    }

    fn osascript(script: &str) -> Result<(), String> {
        let mut cmd = command("osascript");
        cmd.args(["-e", script]);
        let out = output(cmd)?;
        if out.status.success() {
            return Ok(());
        }
        let err = String::from_utf8_lossy(&out.stderr);
        if err.contains("-128") {
            return Err("cancelled".into());
        }
        if err.contains("-1743") {
            return Err("login_items_denied".into());
        }
        Err(failure(&out))
    }

    fn login_toggle(name: &str, path: &str, hidden: bool, action: &Action, state_dir: &Path) -> Result<(), String> {
        let mut state = read_state(state_dir);
        match action {
            Action::Disable => {
                // 先记下来再删：删掉以后就找不回它的路径了
                state.login_items.retain(|item| item.path != path);
                state.login_items.push(DisabledLogin { name: name.to_string(), path: path.to_string(), hidden });
                write_state(state_dir, &state)?;
                let script = format!(
                    "tell application \"System Events\" to delete (every login item whose path is {})",
                    applescript_quote(path.trim_end_matches('/'))
                );
                if let Err(err) = osascript(&script) {
                    state.login_items.retain(|item| item.path != path);
                    let _ = write_state(state_dir, &state);
                    return Err(err);
                }
                Ok(())
            }
            Action::Enable => {
                let script = format!(
                    "tell application \"System Events\" to make login item at end with properties {{path:{}, hidden:{hidden}}}",
                    applescript_quote(path.trim_end_matches('/'))
                );
                osascript(&script)?;
                state.login_items.retain(|item| item.path != path);
                write_state(state_dir, &state)
            }
            _ => Err("not_supported".into()),
        }
    }

    /// launchd 的项：用户自己的（gui 域）直接改；系统的（system 域）要管理员密码，整段命令一次授权做完。
    fn launchd_apply(domain: &str, label: &str, plist: &Path, action: &Action) -> Result<(), String> {
        let target = format!("{domain}/{label}");
        let loaded = || command("launchctl").args(["print", &target]).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
        // 每一步：（参数，失败了算不算错）
        let mut steps: Vec<(Vec<String>, bool)> = Vec::new();
        let step = |args: &[&str], must: bool| (args.iter().map(|a| a.to_string()).collect::<Vec<_>>(), must);
        let plist = plist.to_string_lossy().to_string();
        match action {
            Action::Disable => {
                steps.push(step(&["disable", &target], true));
                steps.push(step(&["bootout", &target], false));
            }
            Action::Enable => {
                steps.push(step(&["enable", &target], true));
                // 只限在登录窗口等别的会话里运行的，现在加载不了，下次自然会加载，不算错
                if !loaded() {
                    steps.push(step(&["bootstrap", domain, &plist], false));
                }
            }
            Action::Start => {
                if !loaded() {
                    steps.push(step(&["bootstrap", domain, &plist], true));
                }
                steps.push(step(&["kickstart", &target], true));
            }
            Action::Stop => steps.push(step(&["bootout", &target], true)),
            Action::Restart => {
                if loaded() {
                    // 刚启动不到 10 秒的任务，launchd 要等满 ThrottleInterval（默认 10 秒）才重启它
                    steps.push(step(&["kickstart", "-k", &target], true));
                } else {
                    steps.push(step(&["bootstrap", domain, &plist], true));
                    steps.push(step(&["kickstart", &target], true));
                }
            }
            Action::StartType(_) => return Err("not_supported".into()),
        }

        if domain == "system" {
            let script = steps
                .iter()
                .map(|(args, must)| {
                    let line = format!("/bin/launchctl {}", args.iter().map(|a| sh_quote(a)).collect::<Vec<_>>().join(" "));
                    if *must { format!("{line} || exit $?") } else { format!("{line} >/dev/null 2>&1") }
                })
                .collect::<Vec<_>>()
                .join("; ");
            return osascript(&format!("do shell script {} with administrator privileges", applescript_quote(&script)));
        }
        for (args, must) in steps {
            let mut cmd = command("launchctl");
            cmd.args(&args);
            let out = output(cmd)?;
            if must && !out.status.success() {
                return Err(failure(&out));
            }
        }
        Ok(())
    }
}

// ── Windows ──────────────────────────────────────────────────

/// StartupApproved 里的值：第一个字节是偶数（02、06）表示启用，奇数（03、07）表示停用。没有这个值也算启用。
pub fn approved_enabled(bytes: &[u8]) -> bool {
    bytes.first().is_none_or(|b| b & 1 == 0)
}

/// 写进 StartupApproved 的 12 个字节：启用是 02 加 0；停用是 03 加停用的时间（FILETIME，1601 年起的 100 纳秒数）。
pub fn approved_bytes(enabled: bool, filetime: u64) -> [u8; 12] {
    let mut bytes = [0u8; 12];
    bytes[0] = if enabled { 2 } else { 3 };
    if !enabled {
        bytes[4..].copy_from_slice(&filetime.to_le_bytes());
    }
    bytes
}

/// 现在的 FILETIME。
#[allow(dead_code)]
fn filetime_now() -> u64 {
    let since_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    (since_unix.as_nanos() / 100) as u64 + 116_444_736_000_000_000
}

/// 命令里的程序路径：`"C:\App\app.exe" --min` → `C:\App\app.exe`；没引号的取到 .exe 为止，再不行取第一段。
pub fn program_in_command(command: &str) -> Option<String> {
    let command = command.trim();
    if command.is_empty() {
        return None;
    }
    if let Some(rest) = command.strip_prefix('"') {
        return rest.split('"').next().map(str::to_string).filter(|s| !s.is_empty());
    }
    if let Some(end) = command.to_ascii_lowercase().find(".exe") {
        return Some(command[..end + 4].to_string());
    }
    command.split_whitespace().next().map(str::to_string)
}

/// 把 `%SystemRoot%` 这样的环境变量换成实际的值。不认识的原样留着。
pub fn expand_env(text: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 => {
                let name = &after[..end];
                match lookup(name) {
                    Some(value) => out.push_str(&value),
                    None => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// .lnk 快捷方式指向的本地路径（LinkInfo 里的 LocalBasePath）。指向网络位置、或者格式不认识时返回 `None`。
pub fn lnk_target(data: &[u8]) -> Option<String> {
    let u32_at = |at: usize| data.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let u16_at = |at: usize| data.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
    if u32_at(0)? != 0x4C {
        return None;
    }
    let flags = u32_at(0x14)?;
    let mut pos = 0x4C;
    if flags & 1 != 0 {
        // HasLinkTargetIDList：跳过 IDList
        pos += 2 + u16_at(pos)?;
    }
    if flags & 2 == 0 {
        return None;
    }
    let info = pos;
    let header = u32_at(info + 4)?;
    let info_flags = u32_at(info + 8)?;
    if info_flags & 1 == 0 {
        return None;
    }
    // 新格式有 Unicode 的路径，优先用它
    if header >= 0x24 {
        let offset = u32_at(info + 0x1C)?;
        if offset > 0 {
            let start = info + offset;
            let units: Vec<u16> =
                data.get(start..)?.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).take_while(|u| *u != 0).collect();
            let text = String::from_utf16_lossy(&units);
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    let start = info + u32_at(info + 0x10)?;
    let bytes: Vec<u8> = data.get(start..)?.iter().copied().take_while(|b| *b != 0).collect();
    (!bytes.is_empty()).then(|| String::from_utf8_lossy(&bytes).to_string())
}

/// `Get-CimInstance Win32_Service | ConvertTo-Json` 的一项。
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct WinServiceRow {
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub state: Option<String>,
    pub start_mode: Option<String>,
    pub delayed_auto_start: Option<bool>,
    pub path_name: Option<String>,
    pub start_name: Option<String>,
    pub process_id: Option<u32>,
}

/// 解析服务列表。只有一个服务时 PowerShell 输出的是对象而不是数组，两种都认。
pub fn parse_win_services(json: &str) -> Vec<WinServiceRow> {
    let json = json.trim().trim_start_matches('\u{feff}');
    if json.is_empty() {
        return Vec::new();
    }
    serde_json::from_str::<Vec<WinServiceRow>>(json)
        .or_else(|_| serde_json::from_str::<WinServiceRow>(json).map(|one| vec![one]))
        .unwrap_or_default()
}

/// 一个 Windows 服务对应的条目（不含图标、发布者，这些要读 exe）。
pub fn win_service_item(row: &WinServiceRow) -> StartupItem {
    let name = row.display_name.clone().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| row.name.clone());
    let mut item = StartupItem::new(format!("service:{}", row.name), Kind::Service, "service", name);
    item.label = row.name.clone();
    item.description = row.description.clone().unwrap_or_default().trim().to_string();
    item.command = row.path_name.clone().unwrap_or_default();
    item.location = format!(r"HKLM\SYSTEM\CurrentControlSet\Services\{}", row.name);
    item.scope = Scope::Machine;
    let start = match row.start_mode.as_deref().unwrap_or("").to_ascii_lowercase().as_str() {
        "auto" if row.delayed_auto_start == Some(true) => "delayed",
        "auto" => "auto",
        "manual" => "manual",
        "disabled" => "disabled",
        "boot" => "boot",
        "system" => "system",
        _ => "unknown",
    };
    item.enabled = start != "disabled";
    item.start_type = Some(start.to_string());
    item.start_types = if ["auto", "delayed", "manual", "disabled"].contains(&start) {
        ["auto", "delayed", "manual", "disabled"].iter().map(|s| s.to_string()).collect()
    } else {
        Vec::new()
    };
    item.can_toggle = !item.start_types.is_empty();
    let state = row.state.as_deref().unwrap_or("");
    item.running = Some(state.eq_ignore_ascii_case("running"));
    item.pid = row.process_id.filter(|pid| *pid > 0);
    item.can_control = true;
    item.needs_admin = true;
    item.account = row.start_name.clone().unwrap_or_default();
    // Windows 文件夹里的程序（svchost 之类）算系统自带
    let lower = item.command.to_ascii_lowercase();
    item.system = lower.contains(r"\windows\") || lower.contains("%systemroot%") || lower.contains("%windir%");
    item.handle = Handle::WinService { name: row.name.clone() };
    item
}

/// sc.exe 的退出码换成界面的短代码。
pub fn sc_error(code: i32) -> String {
    match code {
        1223 => "cancelled".into(),
        5 => "permission_denied".into(),
        1051 => "service_dependents".into(),
        1053 => "service_timeout".into(),
        1058 => "service_disabled".into(),
        1060 => "not_found".into(),
        other => format!("startup_failed: sc.exe {other}"),
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use winreg::{enums::*, RegKey};

    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const RUN32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
    const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";

    fn env(name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    fn hive(machine: bool) -> RegKey {
        RegKey::predef(if machine { HKEY_LOCAL_MACHINE } else { HKEY_CURRENT_USER })
    }

    fn approved(machine: bool, sub: &str, value: &str) -> bool {
        hive(machine)
            .open_subkey(format!(r"{APPROVED}\{sub}"))
            .ok()
            .and_then(|key| key.get_raw_value(value).ok())
            .is_none_or(|raw| approved_enabled(&raw.bytes))
    }

    /// exe 里的文件说明和公司名。
    fn version_strings(path: &Path) -> (String, String) {
        let read = || -> Option<(String, String)> {
            let file = std::fs::File::open(path).ok()?;
            // 安全性：映射期间文件被别的程序改写会读到变化的内容，这里只读不写，最坏结果是读不出说明
            let map = unsafe { memmap2::Mmap::map(&file).ok()? };
            let pe = pelite::PeFile::from_bytes(&map[..]).ok()?;
            let info = pe.resources().ok()?.version_info().ok()?;
            let lang = *info.translation().first()?;
            let get = |key: &str| info.value(lang, key).map(|s| s.trim().trim_end_matches('\0').to_string()).unwrap_or_default();
            Some((get("FileDescription"), get("CompanyName")))
        };
        read().unwrap_or_default()
    }

    /// 读 exe 的说明、发布者，并把它当作图标来源。
    fn describe_program(item: &mut StartupItem, program: Option<String>) {
        let Some(program) = program.map(|p| expand_env(&p, env)) else { return };
        let path = PathBuf::from(&program);
        if !path.is_file() {
            return;
        }
        if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("exe")) {
            let (description, company) = version_strings(&path);
            if item.description.is_empty() {
                item.description = description;
            }
            if item.publisher.is_empty() {
                item.publisher = company;
            }
            item.icon_source = Some(path.clone());
        }
        item.program = Some(path);
    }

    pub fn list(_state_dir: &Path, _apps: &[InstalledApp], _login_items: bool) -> Listing {
        let mut items = Vec::new();
        // Run 键：（是不是 HKLM，键，StartupApproved 下的子键）
        for (machine, key, sub) in [(false, RUN, "Run"), (true, RUN, "Run"), (true, RUN32, "Run32")] {
            let Ok(run) = hive(machine).open_subkey(key) else { continue };
            for (value, data) in run.enum_values().flatten() {
                let text = match data.vtype {
                    REG_SZ | REG_EXPAND_SZ => String::from_utf16_lossy(
                        &data.bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|u| *u != 0).collect::<Vec<_>>(),
                    ),
                    _ => continue,
                };
                let hive_name = if machine { "HKLM" } else { "HKCU" };
                let mut item = StartupItem::new(format!("run:{hive_name}:{sub}:{value}"), Kind::Startup, "registry", value.clone());
                item.command = text.clone();
                item.location = format!(r"{hive_name}\{key}");
                item.scope = if machine { Scope::Machine } else { Scope::User };
                item.enabled = approved(machine, sub, &value);
                item.needs_admin = machine;
                item.triggers = vec!["login".into()];
                item.handle = Handle::RunValue { machine, approved: if sub == "Run" { "Run" } else { "Run32" }, value };
                describe_program(&mut item, program_in_command(&text));
                items.push(item);
            }
        }
        // “启动”文件夹
        let folders = [
            (false, env("APPDATA").map(|d| PathBuf::from(d).join(r"Microsoft\Windows\Start Menu\Programs\Startup"))),
            (true, env("ProgramData").map(|d| PathBuf::from(d).join(r"Microsoft\Windows\Start Menu\Programs\StartUp"))),
        ];
        for (machine, folder) in folders {
            let Some(folder) = folder else { continue };
            for entry in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
                let path = entry.path();
                let file = entry.file_name().to_string_lossy().to_string();
                if !path.is_file() || file.eq_ignore_ascii_case("desktop.ini") {
                    continue;
                }
                let name = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
                let scope = if machine { "common" } else { "user" };
                let mut item = StartupItem::new(format!("folder:{scope}:{file}"), Kind::Startup, "startupFolder", name);
                item.label = file.clone();
                item.location = path.to_string_lossy().to_string();
                item.scope = if machine { Scope::Machine } else { Scope::User };
                item.enabled = approved(machine, "StartupFolder", &file);
                item.needs_admin = machine;
                item.triggers = vec!["login".into()];
                let target = if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("lnk")) {
                    std::fs::read(&path).ok().and_then(|data| lnk_target(&data))
                } else {
                    Some(path.to_string_lossy().to_string())
                };
                item.command = target.clone().unwrap_or_else(|| path.to_string_lossy().to_string());
                item.program = Some(path.clone());
                describe_program(&mut item, target);
                item.handle = Handle::StartupFile { machine, file };
                items.push(item);
            }
        }

        let mut notes = Vec::new();
        match services() {
            Some(rows) => {
                for row in rows {
                    let mut item = win_service_item(&row);
                    // 系统自带的服务太多（两三百个），读 exe 很慢，只读第三方的
                    if !item.system {
                        let program = program_in_command(&item.command);
                        describe_program(&mut item, program);
                    } else {
                        item.publisher = "Microsoft".into();
                    }
                    items.push(item);
                }
            }
            None => notes.push("services_failed".into()),
        }
        Listing { items, notes }
    }

    fn powershell(script: &str, limit: Duration) -> Option<Output> {
        let mut cmd = command("powershell");
        cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-EncodedCommand", &ps_encode(script)]);
        output_within(cmd, limit)
    }

    fn services() -> Option<Vec<WinServiceRow>> {
        let script = "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; \
            Get-CimInstance -ClassName Win32_Service | \
            Select-Object Name,DisplayName,Description,State,StartMode,DelayedAutoStart,PathName,StartName,ProcessId | \
            ConvertTo-Json -Compress";
        let out = powershell(script, Duration::from_secs(60))?;
        out.status.success().then(|| parse_win_services(&String::from_utf8_lossy(&out.stdout)))
    }

    /// 先用普通权限跑一段 PowerShell；返回 5（拒绝访问）时以管理员身份再跑一次（弹 UAC），返回退出码。
    fn run_script(script: &str, elevate: bool) -> Result<i32, String> {
        let direct = powershell(script, Duration::from_secs(120)).ok_or_else(|| "startup_failed: powershell".to_string())?;
        let code = direct.status.code().unwrap_or(-1);
        if code != 5 || !elevate {
            return Ok(code);
        }
        run_elevated_only(script)
    }

    pub fn apply(item: &StartupItem, action: &Action, _state_dir: &Path) -> Result<(), String> {
        match &item.handle {
            Handle::RunValue { machine, approved, value } => set_approved(*machine, approved, value, action),
            Handle::StartupFile { machine, file } => set_approved(*machine, "StartupFolder", file, action),
            Handle::WinService { name } => service_apply(name, action),
            _ => Err("not_supported".into()),
        }
    }

    /// 和任务管理器一样：在 StartupApproved 里记启用还是停用。HKCU 直接写；HKLM 要管理员权限。
    fn set_approved(machine: bool, sub: &str, value: &str, action: &Action) -> Result<(), String> {
        let enabled = match action {
            Action::Enable => true,
            Action::Disable => false,
            _ => return Err("not_supported".into()),
        };
        let bytes = approved_bytes(enabled, filetime_now());
        let path = format!(r"{APPROVED}\{sub}");
        let direct = hive(machine).create_subkey(&path).and_then(|(key, _)| {
            key.set_raw_value(value, &winreg::RegValue { bytes: bytes.to_vec().into(), vtype: REG_BINARY })
        });
        match direct {
            Ok(()) => Ok(()),
            Err(err) if machine && err.kind() == std::io::ErrorKind::PermissionDenied => {
                let list = bytes.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(",");
                let script = format!(
                    "$ErrorActionPreference = 'Stop'; try {{ $p = {}; if (-not (Test-Path $p)) {{ New-Item -Path $p -Force | Out-Null }}; \
                     New-ItemProperty -Path $p -Name {} -PropertyType Binary -Value ([byte[]]({list})) -Force | Out-Null; exit 0 }} \
                     catch {{ exit 5 }}",
                    ps_quote(&format!(r"HKLM:\{path}")),
                    ps_quote(value)
                );
                // 普通权限写不进 HKLM，直接要管理员权限
                match run_elevated_only(&script)? {
                    0 => Ok(()),
                    code => Err(sc_error(code)),
                }
            }
            Err(err) => Err(format!("startup_failed: {err}")),
        }
    }

    /// 以管理员身份跑一段 PowerShell（弹 UAC），返回退出码。在 UAC 提示里点了“否”时是 1223。
    fn run_elevated_only(script: &str) -> Result<i32, String> {
        let outer = format!(
            "try {{ $p = Start-Process -FilePath powershell -Verb RunAs -WindowStyle Hidden -Wait -PassThru \
             -ArgumentList '-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-EncodedCommand','{}'; exit $p.ExitCode }} \
             catch {{ exit 1223 }}",
            ps_encode(script)
        );
        let mut cmd = command("powershell");
        cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-EncodedCommand", &ps_encode(&outer)]);
        Ok(output(cmd)?.status.code().unwrap_or(-1))
    }

    fn service_apply(name: &str, action: &Action) -> Result<(), String> {
        let q = ps_quote(name);
        // sc.exe 的启动、停止命令发出去就返回，等到真的停下来或起来再算完
        let wait = |state: &str| {
            format!("try {{ (Get-Service -Name {q}).WaitForStatus('{state}', [TimeSpan]::FromSeconds(30)) }} catch {{ exit 1053 }}")
        };
        let start = format!("& sc.exe start {q} | Out-Null; $c = $LASTEXITCODE; if ($c -ne 0 -and $c -ne 1056) {{ exit $c }}; {}", wait("Running"));
        let stop = format!("& sc.exe stop {q} | Out-Null; $c = $LASTEXITCODE; if ($c -ne 0 -and $c -ne 1062) {{ exit $c }}; {}", wait("Stopped"));
        let config = |mode: &str| format!("& sc.exe config {q} start= {mode} | Out-Null; if ($LASTEXITCODE -ne 0) {{ exit $LASTEXITCODE }}");
        let body = match action {
            Action::Start => start,
            Action::Stop => stop,
            Action::Restart => format!("{stop}; {start}"),
            Action::Enable => config("demand"),
            Action::Disable => config("disabled"),
            Action::StartType(kind) => config(match kind.as_str() {
                "auto" => "auto",
                "delayed" => "delayed-auto",
                "manual" => "demand",
                _ => "disabled",
            }),
        };
        match run_script(&format!("{body}; exit 0"), true)? {
            0 => Ok(()),
            code => Err(sc_error(code)),
        }
    }
}

// ── Linux ────────────────────────────────────────────────────

/// .desktop 文件里 `[Desktop Entry]` 一节的键值。
pub fn desktop_fields(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut inside = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line == "[Desktop Entry]";
            continue;
        }
        if !inside || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            map.entry(key.trim().to_string()).or_insert_with(|| value.trim().to_string());
        }
    }
    map
}

/// 按界面语言挑 `Name[zh_CN]` 这样的本地化字段，没有就用不带语言的。
pub fn localized(fields: &HashMap<String, String>, key: &str, locale: &str) -> String {
    let full = locale.split(['.', '@']).next().unwrap_or("");
    let base = full.split('_').next().unwrap_or("");
    [format!("{key}[{full}]"), format!("{key}[{base}]"), key.to_string()]
        .iter()
        .find_map(|k| fields.get(k).filter(|v| !v.is_empty()).cloned())
        .unwrap_or_default()
}

/// 自动启动的项是否启用：`Hidden=true` 或 `X-GNOME-Autostart-enabled=false` 都算停用。
pub fn autostart_enabled(fields: &HashMap<String, String>) -> bool {
    let off = |key: &str, value: &str| fields.get(key).is_some_and(|v| v.eq_ignore_ascii_case(value));
    !off("Hidden", "true") && !off("X-GNOME-Autostart-enabled", "false")
}

/// 这个桌面环境下会不会运行（OnlyShowIn / NotShowIn）。`desktops` 是 XDG_CURRENT_DESKTOP 拆开的。
pub fn shown_in(fields: &HashMap<String, String>, desktops: &[String]) -> bool {
    let list = |key: &str| fields.get(key).map(|v| v.split(';').filter(|s| !s.is_empty()).map(str::to_string).collect::<Vec<_>>());
    if desktops.is_empty() {
        return true;
    }
    if let Some(only) = list("OnlyShowIn") {
        if !only.iter().any(|d| desktops.iter().any(|c| c.eq_ignore_ascii_case(d))) {
            return false;
        }
    }
    if let Some(not) = list("NotShowIn") {
        if not.iter().any(|d| desktops.iter().any(|c| c.eq_ignore_ascii_case(d))) {
            return false;
        }
    }
    true
}

/// 在 `[Desktop Entry]` 一节里设一个键（有就改，没有就加在这一节的末尾），别的内容原样保留。
pub fn set_desktop_key(text: &str, key: &str, value: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut inside = false;
    let mut done = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            if inside && !done {
                // 这一节结束了还没有这个键：加在节尾（跳过节尾的空行）
                let at = out.iter().rposition(|l| !l.trim().is_empty()).map_or(out.len(), |i| i + 1);
                out.insert(at, format!("{key}={value}"));
                done = true;
            }
            inside = trimmed == "[Desktop Entry]";
        } else if inside && !done && trimmed.split_once('=').is_some_and(|(k, _)| k.trim() == key) {
            out.push(format!("{key}={value}"));
            done = true;
            continue;
        }
        out.push(line.to_string());
    }
    if !done {
        if !inside {
            out.push("[Desktop Entry]".to_string());
        }
        out.push(format!("{key}={value}"));
    }
    let mut joined = out.join("\n");
    joined.push('\n');
    joined
}

/// `systemctl show -p A,B unit1 unit2 …` 的输出：每个单元一段 `键=值`，段与段之间空一行。
pub fn parse_systemctl_show(text: &str) -> Vec<HashMap<String, String>> {
    let mut blocks = Vec::new();
    let mut current = HashMap::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            if !current.is_empty() {
                blocks.push(std::mem::take(&mut current));
            }
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            current.insert(key.to_string(), value.to_string());
        }
    }
    if !current.is_empty() {
        blocks.push(current);
    }
    blocks
}

/// `systemctl list-unit-files --type=service --plain --no-legend` 的输出：单元名 → 状态。模板单元（foo@.service）跳过。
pub fn parse_unit_files(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let unit = parts.next()?;
            let state = parts.next()?;
            (unit.ends_with(".service") && !unit.contains("@.")).then(|| (unit.to_string(), state.to_string()))
        })
        .collect()
}

/// ExecStart 的值 `{ path=/usr/bin/foo ; argv[]=/usr/bin/foo -x ; … }` 里的命令行。
pub fn exec_command(value: &str) -> String {
    match value.split_once("argv[]=") {
        Some((_, rest)) => rest.split(" ;").next().unwrap_or("").trim().to_string(),
        None => value.trim().trim_matches(['{', '}']).trim().to_string(),
    }
}

/// 各发行版都有的核心服务，默认不列出来。
const CORE_UNITS: &[&str] = &[
    "dbus", "dbus-broker", "polkit", "rtkit-daemon", "upower", "udisks2", "accounts-daemon", "getty", "serial-getty", "user", "user-runtime-dir",
    "emergency", "rescue", "console-getty", "container-getty", "kmod-static-nodes", "plymouth", "ldconfig", "auditd", "apparmor", "selinux-autorelabel",
];

/// 一个 systemd 单元对应的条目。`fields` 是 `systemctl show` 的一段；`file_state` 是 list-unit-files 给的状态。
pub fn systemd_item(fields: &HashMap<String, String>, file_state: &str, user: bool) -> Option<StartupItem> {
    let unit = fields.get("Id")?.clone();
    let get = |key: &str| fields.get(key).cloned().unwrap_or_default();
    let short = unit.trim_end_matches(".service").to_string();
    let scope_name = if user { "user" } else { "system" };
    let mut item = StartupItem::new(format!("systemd:{scope_name}:{unit}"), Kind::Service, "systemd", short.clone());
    item.label = unit.clone();
    item.description = get("Description");
    if item.description == unit {
        item.description.clear();
    }
    item.command = exec_command(&get("ExecStart"));
    item.location = get("FragmentPath");
    item.scope = if user { Scope::User } else { Scope::Machine };
    let state = if file_state.is_empty() { get("UnitFileState") } else { file_state.to_string() };
    item.enabled = state.starts_with("enabled") || state == "linked" || state == "alias";
    item.can_toggle = state == "enabled" || state == "disabled";
    item.start_type = Some(state.clone());
    item.running = Some(get("SubState") == "running");
    item.pid = get("MainPID").parse::<u32>().ok().filter(|pid| *pid > 0);
    item.can_control = state != "masked";
    item.needs_admin = !user;
    item.account = get("User");
    let base = short.split('@').next().unwrap_or(&short);
    item.system = !matches!(state.as_str(), "enabled" | "disabled") || short.starts_with("systemd-") || CORE_UNITS.contains(&base);
    item.program = program_in_command(&item.command).map(PathBuf::from).filter(|p| p.is_absolute());
    item.handle = Handle::Systemd { unit, user };
    Some(item)
}

#[cfg(all(unix, not(target_os = "macos")))]
mod platform {
    use super::*;

    fn home() -> PathBuf {
        std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
    }

    fn user_autostart() -> PathBuf {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home().join(".config")).join("autostart")
    }

    fn system_autostart() -> Vec<PathBuf> {
        let dirs = std::env::var("XDG_CONFIG_DIRS").ok().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "/etc/xdg".into());
        dirs.split(':').filter(|d| !d.is_empty()).map(|d| PathBuf::from(d).join("autostart")).collect()
    }

    fn icon_dirs() -> Vec<PathBuf> {
        let home = home();
        vec![
            PathBuf::from("/usr/share/icons/hicolor"),
            home.join(".local/share/icons/hicolor"),
            PathBuf::from("/var/lib/flatpak/exports/share/icons/hicolor"),
            home.join(".local/share/flatpak/exports/share/icons/hicolor"),
            PathBuf::from("/usr/share/pixmaps"),
        ]
    }

    fn desktop_files(dir: &Path) -> Vec<(String, PathBuf)> {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "desktop"))
            .filter_map(|p| Some((p.file_name()?.to_string_lossy().to_string(), p)))
            .collect()
    }

    pub fn list(_state_dir: &Path, _apps: &[InstalledApp], _login_items: bool) -> Listing {
        let locale = std::env::var("LC_ALL").or_else(|_| std::env::var("LC_MESSAGES")).or_else(|_| std::env::var("LANG")).unwrap_or_default();
        let desktops: Vec<String> =
            std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().split(':').filter(|s| !s.is_empty()).map(str::to_string).collect();
        let icon_dirs = icon_dirs();

        // 同名文件：用户文件夹里的盖过系统的；系统的几个文件夹，排在前面的优先
        let mut system: HashMap<String, PathBuf> = HashMap::new();
        for dir in system_autostart().iter().rev() {
            for (name, path) in desktop_files(dir) {
                system.insert(name, path);
            }
        }
        let user: HashMap<String, PathBuf> = desktop_files(&user_autostart()).into_iter().collect();
        let mut names: Vec<&String> = system.keys().chain(user.keys()).collect();
        names.sort();
        names.dedup();

        let mut items = Vec::new();
        for file_name in names {
            let user_file = user.get(file_name).cloned();
            let system_file = system.get(file_name).cloned();
            let Some(path) = user_file.clone().or_else(|| system_file.clone()) else { continue };
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let fields = desktop_fields(&text);
            if fields.get("Type").is_some_and(|t| t != "Application") || !shown_in(&fields, &desktops) {
                continue;
            }
            let mut name = localized(&fields, "Name", &locale);
            if name.is_empty() {
                name = file_name.trim_end_matches(".desktop").to_string();
            }
            let mut item = StartupItem::new(format!("autostart:{file_name}"), Kind::Startup, "autostart", name);
            item.label = file_name.clone();
            item.description = localized(&fields, "Comment", &locale);
            item.command = fields.get("Exec").cloned().unwrap_or_default();
            item.location = path.to_string_lossy().to_string();
            item.scope = if user_file.is_some() && system_file.is_none() { Scope::User } else { Scope::Machine };
            item.system = system_file.is_some() && user_file.is_none();
            item.enabled = autostart_enabled(&fields);
            item.triggers = vec!["login".into()];
            item.icon_source = crate::installed::find_desktop_icon(fields.get("Icon").map(String::as_str).unwrap_or(""), &icon_dirs);
            item.program = program_in_command(&item.command).map(PathBuf::from).filter(|p| p.is_absolute());
            item.handle = Handle::Autostart { file_name: file_name.clone(), user_file, system_file };
            items.push(item);
        }

        let mut notes = Vec::new();
        match systemd_units(false) {
            Some(units) => items.extend(units),
            None => notes.push("services_failed".into()),
        }
        // 用户自己的 systemd 服务。没有用户会话管理器（比如远程登录）时没有，不算错
        items.extend(systemd_units(true).unwrap_or_default());
        Listing { items, notes }
    }

    fn systemctl(user: bool, args: &[&str]) -> Option<String> {
        let mut cmd = command("systemctl");
        if user {
            cmd.arg("--user");
        }
        cmd.args(["--no-pager", "--plain"]).args(args);
        let out = output_within(cmd, Duration::from_secs(30))?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())
    }

    fn systemd_units(user: bool) -> Option<Vec<StartupItem>> {
        let files = parse_unit_files(&systemctl(user, &["list-unit-files", "--type=service", "--no-legend"])?);
        if files.is_empty() {
            return Some(Vec::new());
        }
        let states: HashMap<&str, &str> = files.iter().map(|(u, s)| (u.as_str(), s.as_str())).collect();
        let mut args = vec!["show", "-p", "Id,Description,FragmentPath,MainPID,ActiveState,SubState,UnitFileState,ExecStart,User", "--"];
        args.extend(files.iter().map(|(u, _)| u.as_str()));
        let shown = systemctl(user, &args)?;
        Some(
            parse_systemctl_show(&shown)
                .iter()
                .filter_map(|fields| {
                    let state = fields.get("Id").and_then(|id| states.get(id.as_str())).copied().unwrap_or("");
                    systemd_item(fields, state, user)
                })
                .collect(),
        )
    }

    pub fn apply(item: &StartupItem, action: &Action, _state_dir: &Path) -> Result<(), String> {
        match &item.handle {
            Handle::Autostart { file_name, user_file, system_file } => autostart_toggle(file_name, user_file.as_deref(), system_file.as_deref(), action),
            Handle::Systemd { unit, user } => systemd_apply(unit, *user, action),
            _ => Err("not_supported".into()),
        }
    }

    fn autostart_toggle(file_name: &str, user_file: Option<&Path>, system_file: Option<&Path>, action: &Action) -> Result<(), String> {
        let enable = match action {
            Action::Enable => true,
            Action::Disable => false,
            _ => return Err("not_supported".into()),
        };
        // 系统的那份不动：在用户文件夹里放一份同名的盖过它
        let source = user_file.or(system_file).ok_or_else(|| "not_found".to_string())?;
        let mut text = std::fs::read_to_string(source).map_err(|e| format!("startup_failed: {e}"))?;
        text = set_desktop_key(&text, "Hidden", if enable { "false" } else { "true" });
        if enable && desktop_fields(&text).get("X-GNOME-Autostart-enabled").is_some() {
            text = set_desktop_key(&text, "X-GNOME-Autostart-enabled", "true");
        }
        let dir = user_autostart();
        std::fs::create_dir_all(&dir).map_err(|e| format!("startup_failed: {e}"))?;
        let dest = dir.join(file_name);
        let tmp = dir.join(format!(".{file_name}.tmp"));
        std::fs::write(&tmp, text).map_err(|e| format!("startup_failed: {e}"))?;
        std::fs::rename(&tmp, &dest).map_err(|e| format!("startup_failed: {e}"))
    }

    fn systemd_apply(unit: &str, user: bool, action: &Action) -> Result<(), String> {
        let verb: &[&str] = match action {
            Action::Enable => &["enable", "--now"],
            Action::Disable => &["disable", "--now"],
            Action::Start => &["start"],
            Action::Stop => &["stop"],
            Action::Restart => &["restart"],
            Action::StartType(_) => return Err("not_supported".into()),
        };
        let mut args: Vec<&str> = Vec::new();
        if user {
            args.push("--user");
        }
        args.extend_from_slice(verb);
        args.push("--");
        args.push(unit);
        // 系统服务要 root：用 pkexec 弹出授权窗口；没有 pkexec 时让 systemctl 自己向 polkit 要授权
        let out = if user {
            output({
                let mut cmd = command("systemctl");
                cmd.args(&args);
                cmd
            })?
        } else {
            let mut cmd = command("pkexec");
            cmd.arg("systemctl").args(&args);
            match cmd.stdin(Stdio::null()).output() {
                Ok(out) => out,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => output({
                    let mut cmd = command("systemctl");
                    cmd.args(&args);
                    cmd
                })?,
                Err(err) => return Err(format!("startup_failed: {err}")),
            }
        };
        match out.status.code() {
            Some(0) => Ok(()),
            // pkexec：126 是关掉了授权窗口，127 是没通过授权
            Some(126) if !user => Err("cancelled".into()),
            Some(127) if !user => Err("permission_denied".into()),
            _ => Err(failure(&out)),
        }
    }
}

#[cfg(not(any(unix, windows)))]
mod platform {
    use super::*;
    pub fn list(_state_dir: &Path, _apps: &[InstalledApp], _login_items: bool) -> Listing {
        Listing::default()
    }
    pub fn apply(_item: &StartupItem, _action: &Action, _state_dir: &Path) -> Result<(), String> {
        Err("not_supported".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_launchctl_output() {
        let disabled = "\n\tdisabled services = {\n\t\t\"com.apple.Siri.agent\" => disabled\n\t\t\"com.carriez.RustDesk_server\" => enabled\n\t\t\"old.style\" => true\n\t}\n";
        let map = parse_print_disabled(disabled);
        assert_eq!(map.get("com.apple.Siri.agent"), Some(&true));
        assert_eq!(map.get("com.carriez.RustDesk_server"), Some(&false));
        assert_eq!(map.get("old.style"), Some(&true));

        let print = "system = {\n\ttype = system\n\tservices = {\n\t\t       0      - \tcom.apple.lskdd\n\t\t     399      1 \tcom.cloudflare.cloudflared\n\t\t       0   (pe) \tcom.apple.kernelmanager_helper\n\t}\n\n\tunmanaged processes = {\n\t\t123 x\n\t}\n}";
        let services = parse_domain_services(print);
        assert_eq!(services.get("com.cloudflare.cloudflared"), Some(&399));
        assert_eq!(services.get("com.apple.lskdd"), Some(&0));
        assert_eq!(services.get("com.apple.kernelmanager_helper"), Some(&0));
        assert_eq!(services.len(), 3);

        let service = "system/x = {\n\tstate = running\n\n\tprogram = /usr/local/bin/cloudflared\n\targuments = {\n\t\t/usr/local/bin/cloudflared\n\t\ttunnel\n\t\trun\n\t}\n\n\tstdout path = /tmp/x\n}";
        assert_eq!(parse_service_program(service), vec!["/usr/local/bin/cloudflared", "tunnel", "run"]);
    }

    #[test]
    fn reads_launchd_plists() {
        let plist = plist::Value::from_reader_xml(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
  <key>Label</key><string>com.microsoft.EdgeUpdater.wake</string>
  <key>ProgramArguments</key><array>
    <string>/Users/a/Library/Application Support/Microsoft/EdgeUpdater/Current/EdgeUpdater.app/Contents/MacOS/EdgeUpdater</string>
    <string>--wake-all</string>
  </array>
  <key>StartInterval</key><integer>3600</integer>
  <key>AssociatedBundleIdentifiers</key><array><string>com.microsoft.EdgeUpdater</string></array>
</dict></plist>"#
                .as_bytes(),
        )
        .unwrap();
        let info = parse_launchd_plist(&plist).unwrap();
        assert_eq!(info.label, "com.microsoft.EdgeUpdater.wake");
        assert_eq!(info.triggers, vec!["interval:3600"]);
        assert_eq!(info.bundle_ids, vec!["com.microsoft.EdgeUpdater"]);
        assert!(!info.disabled);
        assert_eq!(
            bundle_in(&info.args),
            Some(PathBuf::from("/Users/a/Library/Application Support/Microsoft/EdgeUpdater/Current/EdgeUpdater.app"))
        );
        assert_eq!(
            join_command(&info.args),
            "\"/Users/a/Library/Application Support/Microsoft/EdgeUpdater/Current/EdgeUpdater.app/Contents/MacOS/EdgeUpdater\" --wake-all"
        );

        let daemon = plist::Value::from_reader_xml(
            r#"<plist version="1.0"><dict>
  <key>Label</key><string>com.carriez.RustDesk_service</string>
  <key>ProgramArguments</key><array><string>/bin/sh</string><string>-c</string><string>/Applications/RustDesk.app/Contents/MacOS/service</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>Disabled</key><true/>
</dict></plist>"#
                .as_bytes(),
        )
        .unwrap();
        let info = parse_launchd_plist(&daemon).unwrap();
        assert_eq!(info.triggers, vec!["load", "keepAlive"]);
        assert!(info.disabled);
        assert_eq!(bundle_in(&info.args), Some(PathBuf::from("/Applications/RustDesk.app")));
        // 命令写在 sh -c 的一整串里
        assert_eq!(bundle_in(&["cd /tmp && /Applications/Foo Bar.app/Contents/MacOS/foo".to_string()]), Some(PathBuf::from("/Applications/Foo Bar.app")));
        assert_eq!(bundle_in(&["/usr/local/bin/cloudflared".to_string()]), None);
    }

    #[test]
    fn login_item_lines() {
        let text = "Dropbox\t/Applications/Dropbox.app/\tfalse\nBroken\tmissing value\tfalse\nHidden One\t/Applications/X.app\ttrue\n";
        let items = parse_login_items(text);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0], DisabledLogin { name: "Dropbox".into(), path: "/Applications/Dropbox.app/".into(), hidden: false });
        assert!(items[1].hidden);
    }

    #[test]
    fn quoting() {
        assert_eq!(sh_quote("it's"), r"'it'\''s'");
        assert_eq!(applescript_quote(r#"a "b" \c"#), r#""a \"b\" \\c""#);
        assert_eq!(ps_quote("O'Brien"), "'O''Brien'");
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        // PowerShell 要 UTF-16LE：'a' → 61 00
        assert_eq!(ps_encode("a"), "YQA=");
    }

    #[test]
    fn actions_parse() {
        assert_eq!(Action::parse("enable"), Some(Action::Enable));
        assert_eq!(Action::parse("type:delayed"), Some(Action::StartType("delayed".into())));
        assert_eq!(Action::parse("type:boot"), None);
        assert_eq!(Action::parse("rm -rf"), None);
    }

    #[test]
    fn windows_startup_approved() {
        assert!(approved_enabled(&[]));
        assert!(approved_enabled(&[2, 0, 0, 0]));
        assert!(approved_enabled(&[6, 0, 0, 0]));
        assert!(!approved_enabled(&[3, 0, 0, 0]));
        assert!(!approved_enabled(&[7, 0, 0, 0]));
        assert_eq!(approved_bytes(true, 123), [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let off = approved_bytes(false, 0x0102_0304_0506_0708);
        assert_eq!(off[0], 3);
        assert_eq!(&off[4..], &[8, 7, 6, 5, 4, 3, 2, 1]);
        // 2026 年前后的 FILETIME 大约是 1.34e17
        assert!(filetime_now() > 133_000_000_000_000_000);
    }

    #[test]
    fn windows_commands() {
        assert_eq!(program_in_command(r#""C:\Program Files\App\app.exe" --min"#).as_deref(), Some(r"C:\Program Files\App\app.exe"));
        assert_eq!(program_in_command(r"C:\Program Files\App\app.EXE /background").as_deref(), Some(r"C:\Program Files\App\app.EXE"));
        assert_eq!(program_in_command("rundll32 shell32.dll").as_deref(), Some("rundll32"));
        assert_eq!(program_in_command("  "), None);
        let env = |name: &str| (name.eq_ignore_ascii_case("SystemRoot")).then(|| r"C:\Windows".to_string());
        assert_eq!(expand_env(r"%SystemRoot%\system32\svchost.exe -k netsvcs", env), r"C:\Windows\system32\svchost.exe -k netsvcs");
        assert_eq!(expand_env("100% sure %UNKNOWN% here", env), "100% sure %UNKNOWN% here");
    }

    #[test]
    fn windows_shortcut_target() {
        // 只有文件头和 LinkInfo 的最小快捷方式，指向 C:\Tools\a.exe
        let mut data = vec![0u8; 0x4C];
        data[0] = 0x4C;
        data[0x14] = 2; // HasLinkInfo
        let path = b"C:\\Tools\\a.exe\0";
        let header = 0x1Cu32;
        let mut info = Vec::new();
        let total = header as usize + path.len();
        info.extend_from_slice(&(total as u32).to_le_bytes());
        info.extend_from_slice(&header.to_le_bytes());
        info.extend_from_slice(&1u32.to_le_bytes()); // VolumeIDAndLocalBasePath
        info.extend_from_slice(&0u32.to_le_bytes()); // VolumeIDOffset
        info.extend_from_slice(&header.to_le_bytes()); // LocalBasePathOffset
        info.extend_from_slice(&0u32.to_le_bytes());
        info.extend_from_slice(&0u32.to_le_bytes());
        info.extend_from_slice(path);
        data.extend_from_slice(&info);
        assert_eq!(lnk_target(&data).as_deref(), Some(r"C:\Tools\a.exe"));
        assert_eq!(lnk_target(b"not a shortcut"), None);
    }

    #[test]
    fn windows_services() {
        let json = r#"[{"Name":"wuauserv","DisplayName":"Windows Update","Description":"启用检测、下载和安装更新。","State":"Running","StartMode":"Manual","DelayedAutoStart":false,"PathName":"C:\\WINDOWS\\system32\\svchost.exe -k netsvcs -p","StartName":"LocalSystem","ProcessId":1234},
            {"Name":"AnyDesk","DisplayName":"AnyDesk Service","Description":null,"State":"Stopped","StartMode":"Auto","DelayedAutoStart":true,"PathName":"\"C:\\Program Files (x86)\\AnyDesk\\AnyDesk.exe\" --service","StartName":"LocalSystem","ProcessId":0}]"#;
        let rows = parse_win_services(json);
        assert_eq!(rows.len(), 2);
        let update = win_service_item(&rows[0]);
        assert_eq!(update.name, "Windows Update");
        assert_eq!(update.label, "wuauserv");
        assert_eq!(update.start_type.as_deref(), Some("manual"));
        assert!(update.system && update.enabled);
        assert_eq!(update.running, Some(true));
        assert_eq!(update.pid, Some(1234));
        let anydesk = win_service_item(&rows[1]);
        assert_eq!(anydesk.start_type.as_deref(), Some("delayed"));
        assert!(!anydesk.system);
        assert_eq!(anydesk.running, Some(false));
        assert_eq!(anydesk.pid, None);
        assert_eq!(anydesk.start_types.len(), 4);
        // 只有一个服务时是对象
        assert_eq!(parse_win_services(r#"{"Name":"one","StartMode":"Disabled"}"#).len(), 1);
        assert!(!win_service_item(&parse_win_services(r#"{"Name":"one","StartMode":"Disabled"}"#)[0]).enabled);
        assert_eq!(sc_error(1223), "cancelled");
        assert_eq!(sc_error(1058), "service_disabled");
    }

    #[test]
    fn linux_autostart_entries() {
        let text = "[Desktop Entry]\nType=Application\nName=Dropbox\nName[zh_CN]=多宝箱\nComment=Sync files\nExec=dropbox start -i\nIcon=dropbox\nOnlyShowIn=GNOME;Unity;\n\n[Desktop Action New]\nName=Other\n";
        let fields = desktop_fields(text);
        assert_eq!(localized(&fields, "Name", "zh_CN.UTF-8"), "多宝箱");
        assert_eq!(localized(&fields, "Name", "en_US.UTF-8"), "Dropbox");
        assert_eq!(localized(&fields, "Comment", "zh_CN.UTF-8"), "Sync files");
        assert!(autostart_enabled(&fields));
        assert!(shown_in(&fields, &["ubuntu".into(), "GNOME".into()]));
        assert!(!shown_in(&fields, &["KDE".into()]));
        assert!(shown_in(&fields, &[]));

        let hidden = set_desktop_key(text, "Hidden", "true");
        let fields = desktop_fields(&hidden);
        assert!(!autostart_enabled(&fields));
        // 加在 [Desktop Entry] 一节里，不是文件末尾的另一节
        assert!(hidden.find("Hidden=true").unwrap() < hidden.find("[Desktop Action New]").unwrap());
        assert!(hidden.contains("Name=Other"));
        let shown = set_desktop_key(&hidden, "Hidden", "false");
        assert_eq!(shown.matches("Hidden=").count(), 1);
        assert!(autostart_enabled(&desktop_fields(&shown)));

        let gnome_off = desktop_fields("[Desktop Entry]\nName=x\nX-GNOME-Autostart-enabled=false\n");
        assert!(!autostart_enabled(&gnome_off));
    }

    #[test]
    fn linux_systemd_units() {
        let files = "docker.service enabled enabled\nsshd.service disabled disabled\ngetty@.service enabled enabled\ndbus.service static -\n";
        let parsed = parse_unit_files(files);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0], ("docker.service".to_string(), "enabled".to_string()));

        let show = "Id=docker.service\nDescription=Docker Application Container Engine\nFragmentPath=/usr/lib/systemd/system/docker.service\nMainPID=812\nActiveState=active\nSubState=running\nUnitFileState=enabled\nExecStart={ path=/usr/bin/dockerd ; argv[]=/usr/bin/dockerd -H fd:// ; ignore_errors=no ; start_time=[n/a] }\nUser=\n\nId=dbus.service\nDescription=D-Bus System Message Bus\nMainPID=0\nActiveState=inactive\nSubState=dead\nUnitFileState=static\n";
        let blocks = parse_systemctl_show(show);
        assert_eq!(blocks.len(), 2);
        let docker = systemd_item(&blocks[0], "enabled", false).unwrap();
        assert_eq!(docker.name, "docker");
        assert_eq!(docker.command, "/usr/bin/dockerd -H fd://");
        assert_eq!(docker.program, Some(PathBuf::from("/usr/bin/dockerd")));
        assert!(docker.enabled && docker.can_toggle && !docker.system && docker.needs_admin);
        assert_eq!(docker.running, Some(true));
        assert_eq!(docker.pid, Some(812));
        let dbus = systemd_item(&blocks[1], "static", false).unwrap();
        assert!(dbus.system && !dbus.can_toggle);
        assert_eq!(dbus.running, Some(false));
    }

    #[test]
    fn checks_before_acting() {
        let mut item = StartupItem::new("x".into(), Kind::Startup, "registry", "x".into());
        item.can_toggle = false;
        assert_eq!(apply(&item, &Action::Disable, Path::new("/nonexistent")), Err("not_supported".into()));
        assert_eq!(apply(&item, &Action::Start, Path::new("/nonexistent")), Err("not_supported".into()));
        assert_eq!(apply(&item, &Action::StartType("auto".into()), Path::new("/nonexistent")), Err("not_supported".into()));
    }

    /// 在本机真的列一遍 launchd 的项（只读）。不碰登录项：那要通过“系统事件”，会弹出授权提示。
    #[test]
    #[cfg(target_os = "macos")]
    fn lists_real_launchd_items() {
        let items = platform::launchd_items(&[]);
        let daemons = items.iter().filter(|i| i.source == "launchDaemon" && i.system).count();
        assert!(daemons > 50, "只列出了 {daemons} 个系统服务");
        assert!(items.iter().all(|i| !i.id.is_empty() && !i.name.is_empty() && i.running.is_some()));
        for item in items.iter().filter(|i| !i.system) {
            eprintln!("{} | {} | app={} | enabled={} running={:?} pid={:?} | {:?} | {}", item.name, item.source, item.app, item.enabled, item.running, item.pid, item.triggers, item.command);
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod timing {
    /// 冷缓存时列一遍要多久（要取图标）。`cargo test -p shibapack-core -- --ignored startup_timing --nocapture`
    #[test]
    #[ignore]
    fn startup_timing() {
        let cache = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        for round in ["冷", "热"] {
            let start = std::time::Instant::now();
            let listing = super::list(cache.path(), state.path(), &[], false);
            let icons = listing.items.iter().filter(|i| i.icon.is_some()).count();
            eprintln!("{round}缓存：{} 项，{icons} 个有图标，用时 {:?}", listing.items.len(), start.elapsed());
        }
    }

    /// 真的在当前用户的 launchd 里走一遍启动、停止、停用、启用。用的是一个只会 sleep 的临时任务，做完卸掉。
    /// 会在 `launchctl print-disabled` 里留下一条 `app.shibapack.selftest => enabled`，没有别的影响。
    /// `cargo test -p shibapack-core -- --ignored launchd_actions --nocapture`
    #[test]
    #[ignore]
    fn launchd_actions() {
        use super::*;
        let dir = tempfile::tempdir().unwrap();
        let plist = dir.path().join("app.shibapack.selftest.plist");
        std::fs::write(
            &plist,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
  <key>Label</key><string>app.shibapack.selftest</string>
  <key>ProgramArguments</key><array><string>/bin/sleep</string><string>120</string></array>
</dict></plist>"#,
        )
        .unwrap();
        let uid = String::from_utf8(Command::new("id").arg("-u").output().unwrap().stdout).unwrap().trim().to_string();
        let domain = format!("gui/{uid}");
        let target = format!("{domain}/app.shibapack.selftest");
        let mut item = StartupItem::new("x".into(), Kind::Startup, "launchAgent", "app.shibapack.selftest".into());
        item.can_control = true;
        item.handle = Handle::Launchd { domain: domain.clone(), label: "app.shibapack.selftest".into(), plist: plist.clone() };
        let state = dir.path();
        let print = || Command::new("launchctl").args(["print", &target]).output().unwrap();
        // kickstart 发出去就返回，进程要过一会儿（xpcproxy 之后）才算 running
        let running = || {
            (0..40).any(|_| {
                let up = String::from_utf8_lossy(&print().stdout).contains("state = running");
                if !up {
                    std::thread::sleep(Duration::from_millis(50));
                }
                up
            })
        };
        let loaded = || print().status.success();
        let disabled = || {
            let out = Command::new("launchctl").args(["print-disabled", &domain]).output().unwrap();
            parse_print_disabled(&String::from_utf8_lossy(&out.stdout)).get("app.shibapack.selftest").copied()
        };

        let result = std::panic::catch_unwind(|| {
            apply(&item, &Action::Start, state).unwrap();
            assert!(running(), "启动后应当在运行");
            apply(&item, &Action::Restart, state).unwrap();
            assert!(running(), "重启后应当在运行");
            apply(&item, &Action::Stop, state).unwrap();
            assert!(!loaded(), "停止后应当卸下");
            apply(&item, &Action::Disable, state).unwrap();
            assert_eq!(disabled(), Some(true));
            // 停用了就启动不了，报错而不是假装成功
            assert!(apply(&item, &Action::Start, state).is_err());
            apply(&item, &Action::Enable, state).unwrap();
            assert_eq!(disabled(), Some(false));
            assert!(loaded(), "启用后应当加载");
        });
        // 不管成没成功都收拾干净
        let _ = Command::new("launchctl").args(["enable", &target]).status();
        let _ = Command::new("launchctl").args(["bootout", &target]).status();
        assert!(!loaded());
        result.unwrap();
    }
}
