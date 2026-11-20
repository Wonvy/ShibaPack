//! 安装监控：安装前拍一次快照（基线），装完再拍一次，对比出安装程序改了什么。
//!
//! 只记文件的大小和修改时间，不算哈希；注册表的值只记数据的摘要。对比时：
//! - 新增的文件夹只报最上面那一层，里面的文件合进去算大小和个数；删掉的也一样。
//! - `.app` 这类包当成一个整体，不往里走；新增或改了的包对比完再算大小（`fill_sizes`）。
//! - 注册表新增的键也只报最上面那一层；原有的键里新增、修改、删除的值逐个报。
//!
//! 每一项按路径分类（程序、数据、设置、启动项、服务、驱动……），常见的缓存、日志、浏览器资料这类
//! 安装期间别的程序也会改的地方标成“噪音”，界面上默认藏起来。
//!
//! 记录存成 JSON，放在仓库的 `.shibapack/traces/<软件 id>/<记录 id>.json`，不用数据库。

pub mod live;
pub mod persist;
pub mod products;

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Instant, UNIX_EPOCH},
};

/// 监控范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Depth {
    /// 安装程序常写的地方。
    Standard,
    /// 再加上缓存、日志、用户目录里的配置文件夹，系统目录多看一层。慢一些，噪音也多。
    Deep,
}

/// 一个要扫的文件夹。`depth` 是往下走几层（1 表示只看里面直接的文件和文件夹）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Root {
    pub path: PathBuf,
    pub depth: usize,
}

/// 要拍快照的范围。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Scope {
    pub roots: Vec<Root>,
    /// 注册表：`HKLM\SOFTWARE` 这样的键和往下走几层（只有 Windows 有）。
    pub registry: Vec<(String, usize)>,
    /// 不扫的地方（ShibaPack 自己的仓库和数据文件夹）。
    pub exclude: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FileMeta {
    pub(crate) dir: bool,
    /// 当成整体的包（`.app` 等），不往里走。
    pub(crate) bundle: bool,
    pub(crate) size: u64,
    /// 修改时间，从 1970 年起的纳秒。
    pub(crate) mtime: i64,
}

/// 一次快照。平常只在内存里；开始监控时另存一份到磁盘（见 `persist`），重启电脑后还能接着对比。
#[derive(Debug, Default)]
pub struct Snapshot {
    pub(crate) files: HashMap<PathBuf, FileMeta>,
    /// 注册表：键 → 值名 → 数据摘要。
    pub(crate) keys: HashMap<String, HashMap<String, u64>>,
    /// 一共记了多少项。
    pub entries: u64,
    pub elapsed_ms: u64,
}

/// 改动是新增、修改还是删除。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Op {
    Added,
    Modified,
    Removed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemKind {
    File,
    Folder,
    RegistryKey,
    RegistryValue,
}

/// 改动的类别，界面上按它分组。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    /// 程序本身：/Applications、Program Files……
    App,
    /// 数据：Application Support、AppData、ProgramData……
    Data,
    /// 设置：Preferences 下的 plist、注册表里的一般设置。
    Settings,
    /// 开机启动：LaunchAgents、Run 键、启动文件夹、计划任务。
    Startup,
    /// 后台服务：LaunchDaemons、PrivilegedHelperTools、Windows 服务。
    Service,
    /// 驱动和系统扩展。
    Driver,
    /// 快捷方式：开始菜单、桌面。
    Shortcut,
    /// 文件关联、右键菜单（Classes）。
    Association,
    /// 环境变量、PATH。
    Environment,
    /// 卸载信息、安装收据。
    Uninstall,
    Other,
}

/// 一项改动。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub op: Op,
    pub kind: ItemKind,
    /// 文件或文件夹的完整路径；注册表是键的路径（`HKLM\SOFTWARE\Foo`）。
    pub path: String,
    /// 注册表值的名字（默认值是空字符串）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// 注册表值现在的内容（截短的），新增和修改的才有。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    /// 文件或文件夹的大小（文件夹是里面所有文件加起来）。修改的文件是改完后的大小。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// 新增或删除的文件夹、注册表键里一共有几项。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
    pub category: Category,
    /// 多半不是安装程序改的（缓存、日志、浏览器资料……）。
    pub noise: bool,
    /// 安装程序自己登记过这一项（.pkg 的安装收据、MSI 的组件清单），确定是它装的。
    #[serde(default)]
    pub confirmed: bool,
}

/// 一次安装监控的记录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trace {
    pub id: String,
    pub software_id: String,
    pub version_id: String,
    pub software_name: String,
    pub version: String,
    pub file_name: String,
    /// 在哪个系统上装的：windows、macos、linux。
    pub platform: String,
    pub depth: Depth,
    pub started_at: String,
    pub finished_at: String,
    /// 两次快照各用了多久、记了多少项。
    pub baseline_ms: u64,
    pub compare_ms: u64,
    pub entries: u64,
    pub changes: Vec<Change>,
    /// 改动太多，只留了前 `MAX_CHANGES` 项。
    #[serde(default)]
    pub truncated: bool,
    /// 安装程序自己登记的产品：.pkg 的安装收据、MSI 的产品码。
    #[serde(default)]
    pub products: Vec<products::Product>,
    /// 安装期间出现过、装完已经不在的文件（解压出来又删掉的临时文件）。只留前 `MAX_TRANSIENT` 个。
    #[serde(default)]
    pub transient: Vec<live::Transient>,
    #[serde(default)]
    pub transient_count: u64,
    /// 安装程序要求重启电脑才能装完（Windows 的“重启后替换文件”）。
    #[serde(default)]
    pub reboot_required: bool,
    /// 重启电脑后接着完成的监控。
    #[serde(default)]
    pub resumed: bool,
}

/// 列表里显示的一条记录（不带改动明细）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceSummary {
    pub id: String,
    pub software_id: String,
    pub version_id: String,
    pub version: String,
    pub file_name: String,
    pub platform: String,
    pub started_at: String,
    pub finished_at: String,
    /// 不算噪音的改动有几项、新增占了多少空间。
    pub changes: usize,
    pub added_bytes: u64,
}

impl Trace {
    pub fn summary(&self) -> TraceSummary {
        let real = self.changes.iter().filter(|c| !c.noise);
        TraceSummary {
            id: self.id.clone(),
            software_id: self.software_id.clone(),
            version_id: self.version_id.clone(),
            version: self.version.clone(),
            file_name: self.file_name.clone(),
            platform: self.platform.clone(),
            started_at: self.started_at.clone(),
            finished_at: self.finished_at.clone(),
            changes: real.clone().count(),
            added_bytes: real.filter(|c| c.op == Op::Added).filter_map(|c| c.size).sum(),
        }
    }
}

/// 一次对比最多留这么多项，再多的多半是噪音，界面也看不过来。
pub const MAX_CHANGES: usize = 20_000;
/// 一次快照最多记这么多项，防止范围里有特别大的文件夹时一直扫下去。
const MAX_ENTRIES: u64 = 4_000_000;

// ───────────────────────────── 范围 ─────────────────────────────

/// 这台电脑上要扫的范围。`exclude` 是不扫的地方（仓库、ShibaPack 自己的数据）。
pub fn scope(depth: Depth, exclude: Vec<PathBuf>) -> Scope {
    let deep = depth == Depth::Deep;
    let mut roots = Vec::new();
    let mut registry = Vec::new();
    let mut add = |path: PathBuf, depth: usize| roots.push(Root { path, depth });

    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        for dir in ["/Applications", "/Applications/Utilities"] {
            add(dir.into(), 2);
        }
        let library = |base: &Path, add: &mut dyn FnMut(PathBuf, usize)| {
            for (name, depth) in [
                ("Application Support", if deep { 6 } else { 4 }),
                ("Preferences", 2),
                ("LaunchAgents", 1),
                ("Services", 2),
                ("Fonts", 2),
                ("Input Methods", 2),
                ("PreferencePanes", 2),
                ("Internet Plug-Ins", 2),
                ("Audio/Plug-Ins", 3),
                ("QuickLook", 2),
                ("Spotlight", 2),
                ("Screen Savers", 2),
                ("Frameworks", 2),
            ] {
                add(base.join(name), depth);
            }
            if deep {
                add(base.join("Caches"), 3);
                add(base.join("Logs"), 3);
                add(base.join("HTTPStorages"), 2);
                add(base.join("WebKit"), 2);
            }
        };
        if let Some(home) = &home {
            add(home.join("Applications"), 2);
            library(&home.join("Library"), &mut add);
            add(home.join(".config"), if deep { 4 } else { 2 });
            add(home.join(".local"), if deep { 4 } else { 3 });
            if deep {
                // 只看家目录直接的文件和文件夹（.xxx 配置），不进桌面、文稿、下载（会弹隐私授权）
                add(home.clone(), 1);
            }
        }
        library(Path::new("/Library"), &mut add);
        for (dir, depth) in [
            ("/Library/LaunchDaemons", 1),
            ("/Library/PrivilegedHelperTools", 1),
            ("/Library/Extensions", 1),
            ("/Library/SystemExtensions", 2),
            ("/Library/StartupItems", 1),
            ("/private/var/db/receipts", 1),
            ("/usr/local", if deep { 5 } else { 3 }),
            ("/opt", 2),
            // /etc 是指向 /private/etc 的链接，只扫一处，不然同一个文件会报两次
            ("/etc/paths.d", 1),
        ] {
            add(dir.into(), depth);
        }
        let _ = &mut registry;
    }

    #[cfg(windows)]
    {
        let env = |name: &str| std::env::var_os(name).map(PathBuf::from);
        let program_depth = if deep { 5 } else { 3 };
        for name in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
            if let Some(dir) = env(name) {
                add(dir, program_depth);
            }
        }
        if let Some(dir) = env("ProgramData") {
            add(dir.clone(), if deep { 5 } else { 3 });
            add(dir.join(r"Microsoft\Windows\Start Menu"), 4);
        }
        if let Some(dir) = env("APPDATA") {
            add(dir.clone(), if deep { 5 } else { 3 });
            add(dir.join(r"Microsoft\Windows\Start Menu"), 4);
        }
        if let Some(dir) = env("LOCALAPPDATA") {
            add(dir.clone(), if deep { 5 } else { 3 });
            add(dir.join("Programs"), program_depth);
        }
        if let Some(dir) = env("USERPROFILE") {
            add(dir.join("Desktop"), 1);
            add(dir.join(".config"), 2);
            if deep {
                add(dir.clone(), 1);
                add(dir.join(r"AppData\LocalLow"), 3);
            }
        }
        if let Some(dir) = env("PUBLIC") {
            add(dir.join("Desktop"), 1);
        }
        if let Some(windir) = env("SystemRoot").or_else(|| env("windir")) {
            add(windir.join(r"System32\drivers"), 1);
            if deep {
                add(windir.clone(), 1);
                add(windir.join("System32"), 1);
                add(windir.join("SysWOW64"), 1);
                add(windir.join("Fonts"), 1);
            }
        }
        let software_depth = if deep { 12 } else { 8 };
        registry.extend([
            (r"HKCU\Software".to_string(), software_depth),
            (r"HKLM\SOFTWARE".to_string(), software_depth),
            (r"HKCU\Environment".to_string(), 1),
            (r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment".to_string(), 1),
            (r"HKLM\SYSTEM\CurrentControlSet\Services".to_string(), 2),
        ]);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        for (dir, depth) in [("/opt", 3), ("/usr/local", 4), ("/usr/share/applications", 1), ("/etc/systemd/system", 2), ("/etc/xdg/autostart", 1)] {
            add(dir.into(), depth);
        }
        if deep {
            add("/usr/bin".into(), 1);
            add("/usr/lib".into(), 2);
            add("/etc".into(), 2);
        }
        if let Some(home) = home {
            add(home.join(".local"), if deep { 5 } else { 4 });
            add(home.join(".config"), if deep { 4 } else { 3 });
            if deep {
                add(home.join(".cache"), 3);
                add(home.clone(), 1);
            }
        }
        let _ = &mut registry;
    }

    let _ = deep;
    roots.retain(|root| root.path.exists());
    Scope { roots, registry, exclude }
}

/// 名字是这些后缀的文件夹当成一个整体（应用、框架、插件），不往里走。
fn is_bundle(path: &Path) -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    matches!(
        ext.as_str(),
        "app" | "framework" | "bundle" | "plugin" | "kext" | "prefpane" | "appex" | "qlgenerator" | "mdimporter" | "component" | "vst" | "vst3" | "saver" | "xpc" | "systemextension"
    )
}

/// 不往里走的文件夹：文件多、和安装无关、或者读了会弹隐私授权。文件夹本身还是记下来。
fn skip_descend(path: &Path) -> bool {
    let name = path.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    matches!(
        name.as_str(),
        // 缓存和日志
        "cache" | "caches" | "code cache" | "gpucache" | "cacheddata" | "shadercache" | "grshadercache" | "crashpad" | "crashes"
            | "service worker" | "indexeddb" | "blob_storage" | "node_modules" | ".git"
            // macOS 受隐私保护或者特别大的
            | "containers" | "group containers" | "mobilesync" | "addressbook" | "com.apple.tcc" | "callhistorydb"
            | "callhistorytransactions" | "knowledge" | "fileprovider" | "clouddocs" | "cloudstorage" | "mail" | "messages"
            | "safari" | "photos" | "com.apple.sharedfilelist" | "metadata"
            // Windows 自己的
            | "windowsapps" | "packages" | "windows defender" | "temp" | "inetcache" | "webcache" | "explorer" | "installer"
    )
}

// ───────────────────────────── 快照 ─────────────────────────────

/// 拍一次快照。扫的时候隔一阵调一次 `progress`（已经记了多少项）；`cancel` 打开时返回 None。
pub fn snapshot(scope: &Scope, cancel: &AtomicBool, progress: &dyn Fn(u64)) -> Option<Snapshot> {
    let started = Instant::now();
    let mut snap = Snapshot::default();
    let mut reported = 0u64;
    for root in &scope.roots {
        if scope.exclude.iter().any(|ex| root.path.starts_with(ex)) {
            continue;
        }
        let mut walker = walkdir::WalkDir::new(&root.path).follow_links(false).max_depth(root.depth).into_iter();
        while let Some(entry) = walker.next() {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            if scope.exclude.iter().any(|ex| path.starts_with(ex)) {
                if entry.file_type().is_dir() {
                    walker.skip_current_dir();
                }
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            let dir = meta.is_dir();
            let bundle = dir && is_bundle(path);
            let file = if bundle {
                bundle_meta(path, &meta)
            } else {
                FileMeta { dir, bundle: false, size: if dir { 0 } else { meta.len() }, mtime: mtime(&meta) }
            };
            if dir && entry.depth() > 0 && (bundle || skip_descend(path)) {
                walker.skip_current_dir();
            }
            snap.files.insert(path.to_path_buf(), file);
            snap.entries += 1;
            if snap.entries >= MAX_ENTRIES {
                break;
            }
            if snap.entries - reported >= 4000 {
                reported = snap.entries;
                progress(snap.entries);
            }
        }
    }
    #[cfg(windows)]
    for (key, depth) in &scope.registry {
        if !registry::walk(key, *depth, &mut snap, cancel, &mut |n| {
            if n - reported >= 4000 {
                reported = n;
                progress(n);
            }
        }) {
            return None;
        }
    }
    progress(snap.entries);
    snap.elapsed_ms = started.elapsed().as_millis() as u64;
    Some(snap)
}

fn mtime(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

/// 包的指纹：包本身、Contents、Info.plist 里最新的修改时间，和 Info.plist 的大小。
/// 升级时整个包被换掉或者 Info.plist 被改写，这几个总有一个会变。
fn bundle_meta(path: &Path, meta: &fs::Metadata) -> FileMeta {
    let mut newest = mtime(meta);
    let mut size = 0;
    for inner in ["Contents", "Contents/Info.plist", "Info.plist"] {
        if let Ok(m) = fs::symlink_metadata(path.join(inner)) {
            newest = newest.max(mtime(&m));
            if !m.is_dir() {
                size = m.len();
            }
        }
    }
    FileMeta { dir: true, bundle: true, size, mtime: newest }
}

// ───────────────────────────── 对比 ─────────────────────────────

/// 对比两次快照。结果按类别、路径排好；超过 `MAX_CHANGES` 时只留前面的（噪音排在后面先被丢掉）。
pub fn diff(before: &Snapshot, after: &Snapshot) -> (Vec<Change>, bool) {
    let mut changes = Vec::new();

    // 文件：新增和删除的先收起来，按路径排序后合并到最上面那一层
    let mut added: Vec<(&PathBuf, &FileMeta)> = Vec::new();
    let mut removed: Vec<(&PathBuf, &FileMeta)> = Vec::new();
    for (path, meta) in &after.files {
        match before.files.get(path) {
            None => added.push((path, meta)),
            Some(old) => {
                // 普通文件夹的修改时间跟着里面的文件变，不算改动；包当成一个文件看
                let changed = if meta.dir && !meta.bundle { false } else { old.size != meta.size || old.mtime != meta.mtime || old.dir != meta.dir };
                if changed {
                    changes.push(file_change(Op::Modified, path, meta, None));
                }
            }
        }
    }
    for (path, meta) in &before.files {
        if !after.files.contains_key(path) {
            removed.push((path, meta));
        }
    }
    for (op, mut list) in [(Op::Added, added), (Op::Removed, removed)] {
        list.sort_by(|a, b| a.0.cmp(b.0));
        let mut top: Option<(PathBuf, FileMeta, u64, u64)> = None; // 路径、本身、合进来的大小、个数
        let flush = |top: &mut Option<(PathBuf, FileMeta, u64, u64)>, changes: &mut Vec<Change>| {
            if let Some((path, meta, size, count)) = top.take() {
                let mut change = file_change(op, &path, &meta, None);
                if meta.dir && !meta.bundle {
                    change.size = Some(size);
                    change.count = Some(count);
                }
                changes.push(change);
            }
        };
        for (path, meta) in list {
            if let Some((top_path, _, size, count)) = top.as_mut() {
                if path.starts_with(&*top_path) {
                    if !meta.dir {
                        *size += meta.size;
                    }
                    *count += 1;
                    continue;
                }
            }
            flush(&mut top, &mut changes);
            top = Some((path.clone(), *meta, 0, 0));
        }
        flush(&mut top, &mut changes);
    }

    // 注册表
    diff_registry(before, after, &mut changes);

    sort_changes(&mut changes);
    let truncated = changes.len() > MAX_CHANGES;
    changes.truncate(MAX_CHANGES);
    (changes, truncated)
}

/// 排序：不是噪音的在前，再按类别、路径。
pub fn sort_changes(changes: &mut [Change]) {
    changes.sort_by(|a, b| a.noise.cmp(&b.noise).then(rank(a.category).cmp(&rank(b.category))).then_with(|| a.path.cmp(&b.path)));
}

pub(crate) fn file_change(op: Op, path: &Path, meta: &FileMeta, size: Option<u64>) -> Change {
    let text = path.to_string_lossy().to_string();
    let kind = if meta.dir { ItemKind::Folder } else { ItemKind::File };
    Change {
        op,
        kind,
        category: categorize(&text, kind, meta.bundle),
        noise: is_noise(&text),
        path: text,
        value: None,
        data: None,
        size: size.or(if meta.dir { None } else { Some(meta.size) }),
        count: None,
        confirmed: false,
    }
}

fn diff_registry(before: &Snapshot, after: &Snapshot, changes: &mut Vec<Change>) {
    let reg_change = |op: Op, kind: ItemKind, key: &str, value: Option<&str>, count: Option<u64>| {
        let category = categorize(key, kind, false);
        Change {
            op,
            kind,
            path: key.to_string(),
            value: value.map(str::to_string),
            data: None,
            size: None,
            count,
            category,
            noise: is_noise(key),
            confirmed: false,
        }
    };
    for (op, from, to) in [(Op::Added, &after.keys, &before.keys), (Op::Removed, &before.keys, &after.keys)] {
        let mut new_keys: Vec<&String> = from.keys().filter(|k| !to.contains_key(*k)).collect();
        new_keys.sort_by_key(|k| k.to_ascii_lowercase());
        let mut top: Option<(String, u64)> = None;
        for key in new_keys {
            let lower = key.to_ascii_lowercase();
            if let Some((top_key, count)) = top.as_mut() {
                if lower.starts_with(&format!("{}\\", top_key.to_ascii_lowercase())) {
                    *count += 1 + from[key].len() as u64;
                    continue;
                }
            }
            if let Some((k, count)) = top.take() {
                changes.push(reg_change(op, ItemKind::RegistryKey, &k, None, Some(count)));
            }
            top = Some((key.clone(), from[key].len() as u64));
        }
        if let Some((k, count)) = top.take() {
            changes.push(reg_change(op, ItemKind::RegistryKey, &k, None, Some(count)));
        }
    }
    for (key, values) in &after.keys {
        let Some(old) = before.keys.get(key) else { continue };
        for (name, hash) in values {
            match old.get(name) {
                None => changes.push(reg_change(Op::Added, ItemKind::RegistryValue, key, Some(name), None)),
                Some(h) if h != hash => changes.push(reg_change(Op::Modified, ItemKind::RegistryValue, key, Some(name), None)),
                _ => {}
            }
        }
        for name in old.keys() {
            if !values.contains_key(name) {
                changes.push(reg_change(Op::Removed, ItemKind::RegistryValue, key, Some(name), None));
            }
        }
    }
}

/// 对比完补上：新增和修改的包算出实际大小，注册表值读出现在的内容。
pub fn fill_details(changes: &mut [Change]) {
    for change in changes.iter_mut() {
        if change.noise || change.op == Op::Removed {
            continue;
        }
        match change.kind {
            ItemKind::Folder if change.size.is_none() => {
                let (size, count) = dir_size(Path::new(&change.path));
                change.size = Some(size);
                change.count = Some(count);
            }
            #[cfg(windows)]
            ItemKind::RegistryValue => {
                change.data = registry::preview(&change.path, change.value.as_deref().unwrap_or(""));
            }
            _ => {}
        }
    }
}

fn dir_size(dir: &Path) -> (u64, u64) {
    let mut size = 0;
    let mut count: u64 = 0;
    for entry in walkdir::WalkDir::new(dir).follow_links(false).into_iter().flatten() {
        if let Ok(meta) = entry.metadata() {
            count += 1;
            if meta.is_file() {
                size += meta.len();
            }
        }
    }
    (size, count.saturating_sub(1))
}

/// 类别的先后（界面上要紧的排前面）。
fn rank(category: Category) -> u8 {
    match category {
        Category::App => 0,
        Category::Startup => 1,
        Category::Service => 2,
        Category::Driver => 3,
        Category::Environment => 4,
        Category::Shortcut => 5,
        Category::Association => 6,
        Category::Uninstall => 7,
        Category::Data => 8,
        Category::Settings => 9,
        Category::Other => 10,
    }
}

/// 按路径分类。路径一律转成小写、统一用 `/` 再比。
pub fn categorize(path: &str, kind: ItemKind, bundle: bool) -> Category {
    let p = path.to_ascii_lowercase().replace('\\', "/");
    let has = |s: &str| p.contains(s);
    let registry = matches!(kind, ItemKind::RegistryKey | ItemKind::RegistryValue);
    if registry {
        if has("/currentversion/run") || has("/schedule/taskcache/") || has("/currentversion/explorer/startupapproved") {
            return Category::Startup;
        }
        if has("/currentcontrolset/services/") {
            return Category::Service;
        }
        if has("/environment") {
            return Category::Environment;
        }
        if has("/currentversion/uninstall/") || has("/installer/userdata/") || has("/installer/products/") {
            return Category::Uninstall;
        }
        if has("software/classes/") || has("/shellex") || has("/app paths/") {
            return Category::Association;
        }
        return Category::Settings;
    }
    if has("/launchagents/") || has("/startupitems/") || has("/start menu/programs/startup/") || has("/xdg/autostart/") || has("/.config/autostart/") {
        return Category::Startup;
    }
    if has("/launchdaemons/") || has("/privilegedhelpertools/") || has("/systemd/system/") {
        return Category::Service;
    }
    if has("/system32/drivers/") || has("/library/extensions/") || has("/library/systemextensions/") || p.ends_with(".kext") || p.ends_with(".sys") {
        return Category::Driver;
    }
    if has("/paths.d/") {
        return Category::Environment;
    }
    if has("/var/db/receipts/") {
        return Category::Uninstall;
    }
    if p.ends_with(".lnk") || p.ends_with(".url") || has("/start menu/") || has("/desktop/") || has("/share/applications/") {
        return Category::Shortcut;
    }
    if has("/preferences/") || p.ends_with(".plist") {
        return Category::Settings;
    }
    if bundle && p.ends_with(".app") {
        return Category::App;
    }
    if has("/applications/") || has("/program files") || has("/appdata/local/programs/") || p.starts_with("/opt/") || has("/usr/local/bin/") {
        return Category::App;
    }
    if has("/application support/") || has("/appdata/") || has("/programdata/") || has("/.config/") || has("/.local/") || has("/library/") {
        return Category::Data;
    }
    Category::Other
}

/// 在多半和安装无关的地方（缓存、日志、浏览器资料、系统自己一直在写的地方）。不看文件名，
/// 所以安装程序自己的 .tmp、.log 不算——实时监听记临时文件时用它。
pub fn noise_place(path: &str) -> bool {
    let p = path.to_ascii_lowercase().replace('\\', "/");
    const PARTS: &[&str] = &[
        "/caches/",
        "/cache/",
        "/code cache/",
        "/gpucache/",
        "/logs/",
        "/log/",
        "/crashpad/",
        "/sentry/",
        // Electron 应用（VS Code、Claude、Slack……）一直在写的显卡缓存
        "/dawngraphitecache/",
        "/dawnwebgpucache/",
        "/shadercache/",
        "/grshadercache/",
        "/preferences/contextstoreagent",
        "/crashreporter/",
        "/diagnosticreports/",
        "/saved application state/",
        "/temp/",
        "/tmp/",
        "/inetcache/",
        "/prefetch/",
        "/httpstorages/",
        "/webkit/",
        "/cookies",
        "/google/chrome/",
        "/chrome/user data/",
        "/microsoft edge/",
        "/microsoft/edge/",
        "/firefox/profiles/",
        "/mozilla/firefox/",
        "/bravesoftware/",
        "/com.apple.",
        "/preferences/com.apple.",
        "/currentversion/explorer/",
        "/microsoft/windows/recent/",
        "/microsoft/windows/shell/",
        "/currentversion/cloudstore",
        "/windows defender",
        "/microsoft/tracing",
        "/systemcertificates/",
        "/currentversion/notifications",
        "/currentversion/search",
        "/currentversion/appmodel",
        "/currentversion/deliveryoptimization",
        "/microsoft/input/",
        "/microsoft/windows/windowsupdate",
        "/bam/",
        "/classes/local settings/",
        "/software/microsoft/windows nt/currentversion/winlogon/",
        // ShibaPack 自己的仓库数据。不要写成 "/shibapack/"：那样连用户自己叫 ShibaPack 的文件夹也算噪音了
        "/.shibapack/",
    ];
    PARTS.iter().any(|s| p.contains(s))
}

/// 多半不是安装程序改的：在无关的地方（`noise_place`），或者是日志、锁、数据库日志这类文件。
pub fn is_noise(path: &str) -> bool {
    let p = path.to_ascii_lowercase().replace('\\', "/");
    const ENDS: &[&str] = &[".ds_store", ".log", "-journal", "-wal", "-shm", ".tmp", ".lock", "lockfile", ".etl", "ntuser.dat"];
    noise_place(path) || ENDS.iter().any(|s| p.ends_with(s))
}

// ───────────────────────────── 存储 ─────────────────────────────

/// id 只能是字母、数字、短横线，拼到路径里不会跑出记录文件夹。
fn safe_id(id: &str) -> Result<&str> {
    if !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        Ok(id)
    } else {
        Err(Error::Code("not_found"))
    }
}

/// 一个软件的记录放在哪：`<数据文件夹>/traces/<软件 id>/`。
fn software_dir(dir: &Path, software_id: &str) -> Result<PathBuf> {
    Ok(dir.join(safe_id(software_id)?))
}

/// 存一条记录（先写临时文件再换上，写一半断电也不会坏）。
pub fn save(dir: &Path, trace: &Trace) -> Result<()> {
    let folder = software_dir(dir, &trace.software_id)?;
    fs::create_dir_all(&folder)?;
    let path = folder.join(format!("{}.json", safe_id(&trace.id)?));
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec(trace)?)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

/// 读一条记录。
pub fn load(dir: &Path, software_id: &str, id: &str) -> Result<Trace> {
    let path = software_dir(dir, software_id)?.join(format!("{}.json", safe_id(id)?));
    let bytes = fs::read(&path).map_err(|_| Error::Code("not_found"))?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// 一个软件的所有记录（新的在前）。`software_id` 为 None 时列出全部。读不了的文件跳过。
pub fn list(dir: &Path, software_id: Option<&str>) -> Vec<TraceSummary> {
    let folders: Vec<PathBuf> = match software_id {
        Some(id) => software_dir(dir, id).into_iter().collect(),
        None => fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect(),
    };
    let mut out: Vec<TraceSummary> = folders
        .iter()
        .flat_map(|folder| fs::read_dir(folder).into_iter().flatten().flatten())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .filter_map(|p| fs::read(p).ok())
        .filter_map(|bytes| serde_json::from_slice::<Trace>(&bytes).ok())
        .map(|t| t.summary())
        .collect();
    out.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    out
}

/// 删一条记录。
pub fn delete(dir: &Path, software_id: &str, id: &str) -> Result<()> {
    let folder = software_dir(dir, software_id)?;
    let path = folder.join(format!("{}.json", safe_id(id)?));
    fs::remove_file(&path).map_err(|_| Error::Code("not_found"))?;
    if fs::read_dir(&folder).map(|mut d| d.next().is_none()).unwrap_or(false) {
        let _ = fs::remove_dir(&folder);
    }
    Ok(())
}

// ───────────────────────────── 安装程序还在不在跑 ─────────────────────────────

/// 安装程序是不是还在跑。看不出来时（.dmg 拖进“应用程序”这种）返回 None。
/// - Windows：有进程的可执行文件就是这个安装包；.msi 看有没有 msiexec 在跑。
/// - macOS：.pkg 看“安装器”还开着没有。
pub fn installer_running(installer: &Path) -> Option<bool> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let ext = installer.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let names: Vec<String> = match (std::env::consts::OS, ext.as_str()) {
        ("windows", "exe") => vec![installer.file_name()?.to_string_lossy().to_ascii_lowercase()],
        ("windows", "msi" | "msix" | "msixbundle" | "appx") => vec!["msiexec.exe".into(), "appinstaller.exe".into()],
        ("macos", "pkg" | "mpkg") => vec!["installer".into()],
        _ => return None,
    };
    let Ok(mut system) = System::new() else { return None };
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet));
    let running = system.processes().values().any(|p| {
        let name = p.name().to_string_lossy().to_ascii_lowercase();
        names.contains(&name) || p.exe().is_some_and(|exe| exe == installer)
    });
    Some(running)
}

// ───────────────────────────── 注册表（Windows） ─────────────────────────────

#[cfg(windows)]
mod registry {
    use super::Snapshot;
    use std::{
        collections::HashMap,
        hash::{Hash, Hasher},
        sync::atomic::{AtomicBool, Ordering},
    };
    use winreg::{enums::*, RegKey, HKEY};

    /// 不往里走的键：系统自己一直在写、和安装无关、又特别大的。
    const SKIP: &[&str] = &[
        r"software\microsoft\windows\currentversion\explorer",
        r"software\microsoft\windows\currentversion\cloudstore",
        r"software\microsoft\windows\currentversion\appmodel",
        r"software\microsoft\windows\currentversion\search",
        r"software\microsoft\windows\currentversion\notifications",
        r"software\microsoft\windows\currentversion\deliveryoptimization",
        r"software\microsoft\windows\currentversion\windowsupdate",
        r"software\microsoft\windows\currentversion\component based servicing",
        r"software\microsoft\windows\currentversion\sidebyside",
        r"software\microsoft\windows\currentversion\appx",
        r"software\microsoft\windows\shell",
        r"software\microsoft\windows defender",
        r"software\microsoft\tracing",
        r"software\microsoft\systemcertificates",
        r"software\microsoft\windows nt\currentversion\winlogon\notifications",
        r"software\microsoft\windows nt\currentversion\notifications",
        r"software\microsoft\windows search",
        r"software\microsoft\input",
        r"software\microsoft\speech_onecore",
        r"software\classes\local settings",
        r"software\classes\activatableclasses",
        r"software\classes\extensions\contractid",
        r"software\policies\microsoft\systemcertificates",
    ];

    fn split(path: &str) -> Option<(HKEY, &str)> {
        let (hive, rest) = path.split_once('\\').unwrap_or((path, ""));
        let hive = match hive.to_ascii_uppercase().as_str() {
            "HKLM" | "HKEY_LOCAL_MACHINE" => HKEY_LOCAL_MACHINE,
            "HKCU" | "HKEY_CURRENT_USER" => HKEY_CURRENT_USER,
            "HKCR" | "HKEY_CLASSES_ROOT" => HKEY_CLASSES_ROOT,
            "HKU" | "HKEY_USERS" => HKEY_USERS,
            _ => return None,
        };
        Some((hive, rest))
    }

    /// 把一个键和它下面 `depth` 层记进快照。`cancel` 打开时返回 false。
    pub fn walk(path: &str, depth: usize, snap: &mut Snapshot, cancel: &AtomicBool, progress: &mut dyn FnMut(u64)) -> bool {
        let Some((hive, rest)) = split(path) else { return true };
        let Ok(key) = RegKey::predef(hive).open_subkey_with_flags(rest, KEY_READ) else { return true };
        let mut stack = vec![(key, path.to_string(), 0usize)];
        while let Some((key, name, level)) = stack.pop() {
            if cancel.load(Ordering::Relaxed) {
                return false;
            }
            let mut values = HashMap::new();
            for (value, data) in key.enum_values().flatten() {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                data.bytes.hash(&mut hasher);
                (data.vtype as u32).hash(&mut hasher);
                values.insert(value, hasher.finish());
            }
            snap.entries += 1 + values.len() as u64;
            snap.keys.insert(name.clone(), values);
            progress(snap.entries);
            if level >= depth || snap.entries >= super::MAX_ENTRIES {
                continue;
            }
            for sub in key.enum_keys().flatten() {
                let full = format!("{name}\\{sub}");
                let lower = full.to_ascii_lowercase();
                if SKIP.iter().any(|s| lower.contains(s)) {
                    continue;
                }
                if let Ok(child) = key.open_subkey_with_flags(&sub, KEY_READ) {
                    stack.push((child, full, level + 1));
                }
            }
        }
        true
    }

    /// 注册表值现在的内容，截到 200 个字。
    pub fn preview(path: &str, value: &str) -> Option<String> {
        let (hive, rest) = split(path)?;
        let key = RegKey::predef(hive).open_subkey_with_flags(rest, KEY_READ).ok()?;
        let raw = key.get_raw_value(value).ok()?;
        let text = match raw.vtype {
            REG_SZ | REG_EXPAND_SZ | REG_MULTI_SZ => {
                let wide: Vec<u16> = raw.bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                String::from_utf16_lossy(&wide).trim_end_matches('\0').replace('\0', " ; ")
            }
            REG_DWORD if raw.bytes.len() >= 4 => u32::from_le_bytes(raw.bytes[..4].try_into().ok()?).to_string(),
            REG_QWORD if raw.bytes.len() >= 8 => u64::from_le_bytes(raw.bytes[..8].try_into().ok()?).to_string(),
            _ => raw.bytes.iter().take(32).map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" "),
        };
        Some(text.chars().take(200).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope_of(root: &Path) -> Scope {
        Scope { roots: vec![Root { path: root.to_path_buf(), depth: 8 }], registry: vec![], exclude: vec![] }
    }

    fn snap(scope: &Scope) -> Snapshot {
        snapshot(scope, &AtomicBool::new(false), &|_| {}).unwrap()
    }

    #[test]
    fn finds_added_modified_removed_and_folds_new_folders() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("keep")).unwrap();
        fs::write(root.join("keep/a.txt"), "1").unwrap();
        fs::write(root.join("keep/gone.txt"), "x").unwrap();
        let scope = scope_of(root);
        let before = snap(&scope);

        fs::create_dir_all(root.join("NewApp/sub")).unwrap();
        fs::write(root.join("NewApp/one.bin"), vec![0u8; 100]).unwrap();
        fs::write(root.join("NewApp/sub/two.bin"), vec![0u8; 50]).unwrap();
        fs::write(root.join("keep/a.txt"), "changed").unwrap();
        fs::remove_file(root.join("keep/gone.txt")).unwrap();
        let after = snap(&scope);

        let (changes, truncated) = diff(&before, &after);
        assert!(!truncated);
        let added: Vec<_> = changes.iter().filter(|c| c.op == Op::Added).collect();
        assert_eq!(added.len(), 1, "{changes:#?}");
        assert!(added[0].path.ends_with("NewApp"));
        assert_eq!(added[0].kind, ItemKind::Folder);
        assert_eq!(added[0].size, Some(150));
        assert_eq!(added[0].count, Some(3));
        assert!(changes.iter().any(|c| c.op == Op::Modified && c.path.ends_with("a.txt")));
        assert!(changes.iter().any(|c| c.op == Op::Removed && c.path.ends_with("gone.txt")));
        // 改了里面的文件，文件夹本身不算改动
        assert!(!changes.iter().any(|c| c.op == Op::Modified && c.path.ends_with("keep")));
    }

    #[test]
    fn excluded_paths_are_not_scanned() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("repo")).unwrap();
        let mut scope = scope_of(root);
        scope.exclude.push(root.join("repo"));
        let before = snap(&scope);
        fs::write(root.join("repo/x"), "1").unwrap();
        let after = snap(&scope);
        assert!(diff(&before, &after).0.is_empty());
    }

    #[test]
    fn categories_and_noise() {
        assert_eq!(categorize("/Library/LaunchDaemons/com.foo.helper.plist", ItemKind::File, false), Category::Service);
        assert_eq!(categorize("/Users/me/Library/LaunchAgents/com.foo.plist", ItemKind::File, false), Category::Startup);
        assert_eq!(categorize("/Applications/Foo.app", ItemKind::Folder, true), Category::App);
        assert_eq!(categorize("/Users/me/Library/Preferences/com.foo.plist", ItemKind::File, false), Category::Settings);
        assert_eq!(categorize(r"C:\Program Files\Foo\foo.exe", ItemKind::File, false), Category::App);
        assert_eq!(categorize(r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Foo.lnk", ItemKind::File, false), Category::Shortcut);
        assert_eq!(categorize(r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", ItemKind::RegistryValue, false), Category::Startup);
        assert_eq!(categorize(r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Foo", ItemKind::RegistryKey, false), Category::Uninstall);
        assert_eq!(categorize(r"HKLM\SYSTEM\CurrentControlSet\Services\FooSvc", ItemKind::RegistryKey, false), Category::Service);
        assert_eq!(categorize(r"HKCU\Software\Classes\.foo", ItemKind::RegistryKey, false), Category::Association);
        assert_eq!(categorize(r"HKCU\Environment", ItemKind::RegistryValue, false), Category::Environment);
        assert!(is_noise("/Users/me/Library/Caches/com.foo/x"));
        assert!(is_noise(r"C:\Users\me\AppData\Local\Google\Chrome\User Data\Default\History"));
        assert!(is_noise("/Users/me/Library/Application Support/Foo/.DS_Store"));
        assert!(is_noise("/Users/me/Library/Application Support/Code/DawnGraphiteCache/data_1"));
        assert!(!is_noise("/Applications/Foo.app"));
        // 只是文件夹名里有 ShibaPack 的不算（用户的项目、CI 的检出目录）
        assert!(!is_noise("/Users/me/Desktop/ShibaPack/build/app.dat"));
        assert!(is_noise("/Volumes/NAS/Installers/.shibapack/software.json"));
        // 安装程序自己的临时文件：按位置不算噪音（实时监听记临时文件时用），按文件名算
        assert!(!noise_place("/Users/me/Library/Application Support/Foo/setup.tmp"));
        assert!(is_noise("/Users/me/Library/Application Support/Foo/setup.tmp"));
    }

    /// 在本机真实范围上拍两次快照，看用时和两次之间的噪音：`cargo test -p shibapack-core real_scope -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_scope() {
        for depth in [Depth::Standard, Depth::Deep] {
            let scope = scope(depth, vec![]);
            let cancel = AtomicBool::new(false);
            let a = snapshot(&scope, &cancel, &|_| {}).unwrap();
            std::thread::sleep(std::time::Duration::from_secs(3));
            let b = snapshot(&scope, &cancel, &|_| {}).unwrap();
            let (changes, _) = diff(&a, &b);
            let real: Vec<_> = changes.iter().filter(|c| !c.noise).collect();
            for root in &scope.roots {
                let n = a.files.keys().filter(|p| p.starts_with(&root.path)).count();
                println!("  root {} (depth {}) -> {n}", root.path.display(), root.depth);
            }
            println!("{depth:?}: {} roots, {} entries, {} ms / {} ms, {} changes ({} not noise)", scope.roots.len(), a.entries, a.elapsed_ms, b.elapsed_ms, changes.len(), real.len());
            for c in real.iter().take(15) {
                println!("  {:?} {:?} {}", c.op, c.category, c.path);
            }
        }
    }

    #[test]
    fn save_list_load_delete() {
        let dir = tempfile::tempdir().unwrap();
        let trace = Trace {
            id: "t1".into(),
            software_id: "s1".into(),
            version_id: "v1".into(),
            software_name: "Foo".into(),
            version: "1.0".into(),
            file_name: "foo.pkg".into(),
            platform: "macos".into(),
            depth: Depth::Standard,
            started_at: "2026-01-01T00:00:00+08:00".into(),
            finished_at: "2026-01-01T00:01:00+08:00".into(),
            baseline_ms: 1,
            compare_ms: 1,
            entries: 1,
            changes: vec![Change {
                op: Op::Added,
                kind: ItemKind::Folder,
                path: "/Applications/Foo.app".into(),
                value: None,
                data: None,
                size: Some(10),
                count: Some(1),
                category: Category::App,
                noise: false,
                confirmed: false,
            }],
            truncated: false,
            products: vec![],
            transient: vec![],
            transient_count: 0,
            reboot_required: false,
            resumed: false,
        };
        save(dir.path(), &trace).unwrap();
        let listed = list(dir.path(), Some("s1"));
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].added_bytes, 10);
        assert_eq!(load(dir.path(), "s1", "t1").unwrap(), trace);
        assert!(load(dir.path(), "../x", "t1").is_err());
        delete(dir.path(), "s1", "t1").unwrap();
        assert!(list(dir.path(), None).is_empty());
    }
}
