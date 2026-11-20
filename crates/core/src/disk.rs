//! 磁盘分析：扫一个文件夹（或整个盘）有多大、大在哪，给界面画扇形图和矩形图。
//!
//! 思路参考了 SquirrelDisk、Diskplot 这类磁盘可视化工具（只参考思路，代码是新写的）：
//! 先并行走一遍算出每个文件夹的大小，留在内存里；界面要哪一层就从这棵树里取，不用重新扫。
//!
//! - 大小按实际占用的磁盘空间算（macOS / Linux 用块数，稀疏文件不会虚高）；硬链接只算一次。
//! - 不跟着符号链接走，不进别的盘（扫 / 时不进 /Volumes 下面的外接盘）。
//! - 为了省内存：小于 1 MB 的文件不单独记，按文件夹合成一项“小文件”；扫完小于 1 MB 的文件夹不留子项。
//! - 什么都不存到磁盘上；没有权限的文件夹记个数，界面上提示。

use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

/// 比这小的文件不单独列出来，合进所在文件夹的“小文件”。
const SMALL_FILE: u64 = 1024 * 1024;
/// 扫完以后比这小的文件夹不留子项（还能看到它多大，只是不能再往里点）。
const SMALL_DIR: u64 = 1024 * 1024;

/// 树上的一个节点：文件夹、大文件，或者一个文件夹里所有小文件合成的一项。
#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub kind: NodeKind,
    /// 占用的空间（文件夹是里面所有东西加起来）。
    pub size: u64,
    /// 里面有多少个文件（文件是 1）。
    pub files: u64,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeKind {
    Dir,
    File,
    /// 一个文件夹里小于 1 MB 的文件合起来。
    Small,
    /// 界面上太小、合起来显示的“其他”（只出现在 `view` 的结果里）。
    Other,
}

/// 扫描进度。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
    /// 正在扫的某个文件夹，界面上滚动显示。
    pub current: String,
}

/// 扫完的结果。
pub struct Scan {
    pub root: PathBuf,
    pub tree: Node,
    /// 没有权限进去的文件夹个数，和前几个的路径。
    pub denied: u64,
    pub denied_examples: Vec<String>,
    pub elapsed_ms: u64,
}

/// 扫描时共享的计数和去重。
struct Shared<'a> {
    cancel: &'a AtomicBool,
    files: AtomicU64,
    dirs: AtomicU64,
    bytes: AtomicU64,
    current: Mutex<String>,
    denied: AtomicU64,
    denied_examples: Mutex<Vec<String>>,
    /// 有多个硬链接的文件：(设备, inode)，只算第一次。Windows 上用不到（按文件大小算）。
    #[cfg_attr(not(unix), allow(dead_code))]
    linked: Mutex<HashSet<(u64, u64)>>,
    /// 不进去的路径：别的盘的挂载点、系统的虚拟文件系统。
    skip: Vec<PathBuf>,
    root: PathBuf,
    root_dev: Option<u64>,
}

/// 扫 `root`。`cancel` 变成真时尽快停下，返回 `None`。`progress` 大约每 150 毫秒调一次。
pub fn scan(root: &Path, cancel: &AtomicBool, progress: &(dyn Fn(Progress) + Sync)) -> Option<Scan> {
    let started = std::time::Instant::now();
    let shared = Shared {
        cancel,
        files: AtomicU64::new(0),
        dirs: AtomicU64::new(0),
        bytes: AtomicU64::new(0),
        current: Mutex::new(String::new()),
        denied: AtomicU64::new(0),
        denied_examples: Mutex::new(Vec::new()),
        linked: Mutex::new(HashSet::new()),
        skip: skip_paths(root),
        root: root.to_path_buf(),
        root_dev: device(root),
    };
    let done = AtomicBool::new(false);
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 16);

    let tree = std::thread::scope(|scope| {
        // 报进度的线程
        scope.spawn(|| {
            while !done.load(Ordering::Relaxed) && !cancel.load(Ordering::Relaxed) {
                progress(shared.snapshot());
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
        });
        let tree = scan_parallel(root, &shared, threads);
        done.store(true, Ordering::Relaxed);
        tree
    })?;
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    progress(shared.snapshot());
    Some(Scan {
        root: root.to_path_buf(),
        tree,
        denied: shared.denied.load(Ordering::Relaxed),
        denied_examples: shared.denied_examples.lock().map(|l| l.clone()).unwrap_or_default(),
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

impl Shared<'_> {
    fn snapshot(&self) -> Progress {
        Progress {
            files: self.files.load(Ordering::Relaxed),
            dirs: self.dirs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
            current: self.current.lock().map(|c| c.clone()).unwrap_or_default(),
        }
    }

    fn deny(&self, path: &Path) {
        self.denied.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut list) = self.denied_examples.lock() {
            if list.len() < 5 {
                list.push(path.to_string_lossy().to_string());
            }
        }
    }
}

/// 往下走：遇到子文件夹时，有闲着的线程就交给它，没有就自己接着走。
/// 这样不管哪个文件夹特别大（~/Library、某个项目的 node_modules），线程都不会闲着。
fn scan_parallel(root: &Path, shared: &Shared, threads: usize) -> Option<Node> {
    let idle = AtomicU64::new(threads as u64 - 1);
    let mut tree = std::thread::scope(|scope| walk(root.to_path_buf(), shared, scope, &idle))?;
    if shared.cancel.load(Ordering::Relaxed) {
        return None;
    }
    tree.name = display_root(root);
    sort(&mut tree);
    Some(tree)
}

/// 一层：大文件和小文件合成的那项，以及子文件夹的路径。
struct Level {
    loose: Vec<Node>,
    dirs: Vec<PathBuf>,
}

fn read_level(dir: &Path, shared: &Shared) -> Option<Level> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => {
            shared.deny(dir);
            return None;
        }
    };
    shared.dirs.fetch_add(1, Ordering::Relaxed);
    let mut level = Level { loose: Vec::new(), dirs: Vec::new() };
    let (mut small_size, mut small_count) = (0u64, 0u64);
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_symlink() {
            continue;
        }
        let path = entry.path();
        if kind.is_dir() {
            if shared.skip.contains(&path) || other_device(&path, shared) {
                continue;
            }
            level.dirs.push(path);
        } else if kind.is_file() {
            let Ok(meta) = entry.metadata() else { continue };
            let size = allocated(&meta, shared);
            shared.files.fetch_add(1, Ordering::Relaxed);
            shared.bytes.fetch_add(size, Ordering::Relaxed);
            if size >= SMALL_FILE {
                let name = entry.file_name().to_string_lossy().to_string();
                level.loose.push(Node { name, kind: NodeKind::File, size, files: 1, children: Vec::new() });
            } else {
                small_size += size;
                small_count += 1;
            }
        }
    }
    if small_count > 0 {
        level.loose.push(Node { name: String::new(), kind: NodeKind::Small, size: small_size, files: small_count, children: Vec::new() });
    }
    Some(level)
}

/// 走完一个文件夹。子文件夹有闲着的线程就交出去，最后等它们做完拼起来。
fn walk<'scope, 'env>(dir: PathBuf, shared: &'scope Shared<'env>, scope: &'scope std::thread::Scope<'scope, 'env>, idle: &'scope AtomicU64) -> Option<Node> {
    if shared.cancel.load(Ordering::Relaxed) {
        return None;
    }
    if let Ok(mut current) = shared.current.try_lock() {
        *current = dir.to_string_lossy().to_string();
    }
    let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let Some(level) = read_level(&dir, shared) else {
        return Some(Node { name, kind: NodeKind::Dir, size: 0, files: 0, children: Vec::new() });
    };
    let mut node = Node { name, kind: NodeKind::Dir, size: 0, files: 0, children: level.loose };
    let mut handed = Vec::new();
    for path in level.dirs {
        let free = take_idle(idle);
        if free {
            handed.push(scope.spawn(move || {
                let child = walk(path, shared, scope, idle);
                idle.fetch_add(1, Ordering::AcqRel);
                child
            }));
        } else if let Some(child) = walk(path, shared, scope, idle) {
            node.children.push(child);
        }
    }
    for handle in handed {
        if let Ok(Some(child)) = handle.join() {
            node.children.push(child);
        }
    }
    total(&mut node);
    if node.size < SMALL_DIR && dir != shared.root {
        node.children.clear();
    }
    Some(node)
}

/// 有闲着的线程就占一个。
fn take_idle(idle: &AtomicU64) -> bool {
    let mut current = idle.load(Ordering::Acquire);
    while current > 0 {
        match idle.compare_exchange_weak(current, current - 1, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return true,
            Err(actual) => current = actual,
        }
    }
    false
}

/// 算出文件夹的大小和文件数（子项已经算好）。
fn total(node: &mut Node) {
    if node.kind == NodeKind::Dir {
        node.size = node.children.iter().map(|c| c.size).sum();
        node.files = node.children.iter().map(|c| c.files).sum();
    }
}

/// 每一层都按大小从大到小排。
fn sort(node: &mut Node) {
    node.children.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
    for child in &mut node.children {
        if !child.children.is_empty() {
            sort(child);
        }
    }
}

fn display_root(root: &Path) -> String {
    root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| root.to_string_lossy().to_string())
}

/// 实际占用的空间。macOS / Linux 用块数（稀疏文件、压缩文件按真实占用），有多个硬链接的只算一次。
#[cfg(unix)]
fn allocated(meta: &std::fs::Metadata, shared: &Shared) -> u64 {
    use std::os::unix::fs::MetadataExt;
    if meta.nlink() > 1 {
        if let Ok(mut seen) = shared.linked.lock() {
            if !seen.insert((meta.dev(), meta.ino())) {
                return 0;
            }
        }
    }
    meta.blocks() * 512
}

#[cfg(not(unix))]
fn allocated(meta: &std::fs::Metadata, _shared: &Shared) -> u64 {
    meta.len()
}

#[cfg(unix)]
fn device(path: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(path).ok().map(|m| m.dev())
}

#[cfg(not(unix))]
fn device(_path: &Path) -> Option<u64> {
    None
}

/// Linux 上别的文件系统（/proc、/sys、外接盘）不进去。macOS 不按设备号判断：
/// 系统卷和数据卷是两个设备，/Users 这些是“固定链接”过去的，按设备号会漏掉所有用户数据；macOS 靠 `skip_paths`。
fn other_device(path: &Path, shared: &Shared) -> bool {
    if !cfg!(target_os = "linux") {
        return false;
    }
    match (shared.root_dev, device(path)) {
        (Some(root), Some(dev)) => root != dev,
        _ => false,
    }
}

/// 不进去的地方：别的盘的挂载点（扫的就是它时除外）和系统的虚拟文件夹。
fn skip_paths(root: &Path) -> Vec<PathBuf> {
    let mut skip: Vec<PathBuf> = Vec::new();
    if cfg!(target_os = "macos") {
        // 数据卷的内容通过 /Users、/Applications 这些已经算过了；/Volumes 下是别的盘
        skip.extend(["/System/Volumes", "/Volumes", "/dev", "/private/var/vm", "/cores"].map(PathBuf::from));
    } else if cfg!(target_os = "linux") {
        skip.extend(["/proc", "/sys", "/dev", "/run", "/snap"].map(PathBuf::from));
    }
    if let Ok(disks) = sysinfo::Disks::new_with_refreshed_list() {
        for disk in disks.iter() {
            let mount = disk.mount_point().to_path_buf();
            if mount != root && mount.parent().is_some() && mount.starts_with(root) {
                skip.push(mount);
            }
        }
    }
    skip.retain(|p| p != root);
    skip
}

/// 一块可以分析的盘。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mount {
    pub name: String,
    pub path: String,
    pub total: u64,
    pub available: u64,
}

/// 本机的盘：去掉系统内部的卷（macOS 的 /System/Volumes/…、Linux 的 /boot、/snap 这类）。
pub fn mounts() -> Vec<Mount> {
    let mut out: Vec<Mount> = Vec::new();
    let Ok(disks) = sysinfo::Disks::new_with_refreshed_list() else { return out };
    for disk in disks.iter() {
        let path = disk.mount_point().to_string_lossy().to_string();
        let internal = ["/System/Volumes", "/private/var", "/boot", "/snap", "/run", "/dev", "/proc", "/sys"].iter().any(|p| path.starts_with(p));
        if internal || disk.total_space() == 0 || out.iter().any(|m| m.path == path) {
            continue;
        }
        let name = disk.name().to_string_lossy().to_string();
        out.push(Mount { name: if name.is_empty() { path.clone() } else { name }, path, total: disk.total_space(), available: disk.available_space() });
    }
    out
}

// ── 给界面的视图 ─────────────────────────────────────────────

/// 界面上画的一个节点。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewNode {
    pub name: String,
    /// 相对扫描根的路径段；“小文件”“其他”为空。
    pub path: Vec<String>,
    pub kind: NodeKind,
    pub size: u64,
    pub files: u64,
    /// 还能不能往里点（文件夹而且留着子项）。
    pub has_children: bool,
    pub children: Vec<ViewNode>,
}

/// 从 `path` 那个文件夹开始，往下取 `depth` 层。占它不到 `min_ratio` 的子项合成一个“其他”。
pub fn view(tree: &Node, path: &[String], depth: usize, min_ratio: f64) -> Option<ViewNode> {
    let node = find(tree, path)?;
    let threshold = (node.size as f64 * min_ratio) as u64;
    Some(build(node, path.to_vec(), depth, threshold))
}

pub fn find<'a>(tree: &'a Node, path: &[String]) -> Option<&'a Node> {
    let mut node = tree;
    for segment in path {
        node = node.children.iter().find(|c| c.kind != NodeKind::Small && c.name == *segment)?;
    }
    Some(node)
}

fn build(node: &Node, path: Vec<String>, depth: usize, threshold: u64) -> ViewNode {
    let mut children = Vec::new();
    if depth > 0 {
        let (mut other_size, mut other_files, mut other_count) = (0u64, 0u64, 0u64);
        for child in &node.children {
            if child.size < threshold.max(1) {
                other_size += child.size;
                other_files += child.files;
                other_count += 1;
                continue;
            }
            let child_path = if child.kind == NodeKind::Small { Vec::new() } else { [path.clone(), vec![child.name.clone()]].concat() };
            children.push(build(child, child_path, depth - 1, threshold));
        }
        if other_count > 0 {
            children.push(ViewNode {
                name: other_count.to_string(),
                path: Vec::new(),
                kind: NodeKind::Other,
                size: other_size,
                files: other_files,
                has_children: false,
                children: Vec::new(),
            });
        }
    }
    ViewNode {
        name: node.name.clone(),
        path,
        kind: node.kind,
        size: node.size,
        files: node.files,
        has_children: node.kind == NodeKind::Dir && !node.children.is_empty(),
        children,
    }
}

/// 删掉（移到回收站以后）树上的一项，把上面每一层的大小和文件数减掉。返回去掉的那一项。
pub fn remove(tree: &mut Node, path: &[String]) -> Option<u64> {
    let (first, rest) = path.split_first()?;
    let index = tree.children.iter().position(|c| c.kind != NodeKind::Small && c.name == *first)?;
    let (size, files) = if rest.is_empty() {
        let removed = tree.children.remove(index);
        (removed.size, removed.files)
    } else {
        let child = &mut tree.children[index];
        let before = child.files;
        let size = remove(child, rest)?;
        (size, before - child.files)
    };
    tree.size = tree.size.saturating_sub(size);
    tree.files = tree.files.saturating_sub(files);
    Some(size)
}

/// 问 AI 时给的事实：路径（用户文件夹写成 ~）、是文件夹还是文件、大小、文件数、里面最大的几项。
pub fn facts(root: &Path, tree: &Node, path: &[String], home: Option<&Path>) -> Option<String> {
    let node = find(tree, path)?;
    let display = display_path(&full_path(root, path), home);
    let mut lines = vec![
        format!("path: {display}"),
        format!("type: {}", if node.kind == NodeKind::Dir { "folder" } else { "file" }),
        format!("size: {:.1} MB, files: {}", node.size as f64 / 1_048_576.0, node.files),
        format!("os: {}", std::env::consts::OS),
    ];
    let largest: Vec<String> = node.children.iter().filter(|c| c.kind != NodeKind::Small).take(6).map(|c| format!("{} ({:.0} MB)", c.name, c.size as f64 / 1_048_576.0)).collect();
    if !largest.is_empty() {
        lines.push(format!("largest inside: {}", largest.join(", ")));
    }
    Some(lines.join("\n"))
}

/// 路径给 AI、当缓存的键时用：用户文件夹写成 ~，分隔符统一成 /。
pub fn display_path(path: &Path, home: Option<&Path>) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    if let Some(home) = home {
        let home = home.to_string_lossy().replace('\\', "/");
        if let Some(rest) = text.strip_prefix(&home) {
            if rest.is_empty() || rest.starts_with('/') {
                return format!("~{rest}");
            }
        }
    }
    text
}

/// 扫描根加上路径段。
pub fn full_path(root: &Path, path: &[String]) -> PathBuf {
    path.iter().fold(root.to_path_buf(), |acc, segment| acc.join(segment))
}

/// 不让在磁盘分析里移到废纸篓的：扫描根本身、系统文件夹、用户文件夹本身和它下面的标准文件夹、
/// 别的盘的挂载点、应用程序（要卸载请用卸载，会连带清理残留）、ShibaPack 的仓库。
pub fn protected(path: &Path, root: &Path, repo: Option<&Path>) -> Option<&'static str> {
    if path == root || path.parent().is_none() {
        return Some("root");
    }
    let lower = path.to_string_lossy().replace('\\', "/").to_lowercase();
    let lower = lower.trim_end_matches('/');
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(|h| h.to_string_lossy().replace('\\', "/").to_lowercase())
        .unwrap_or_default();
    let system = if cfg!(windows) {
        let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()).to_lowercase();
        ["windows", "program files", "program files (x86)", "programdata", "users", "$recycle.bin", "system volume information", "recovery", "boot"]
            .iter()
            .any(|dir| lower == format!("{drive}/{dir}") || lower.starts_with(&format!("{drive}/{dir}/")) && *dir != "users")
    } else {
        ["/system", "/usr", "/bin", "/sbin", "/etc", "/var", "/private", "/library", "/opt", "/cores", "/dev", "/boot", "/lib", "/lib64", "/proc", "/sys", "/snap", "/users", "/home"]
            .iter()
            .any(|dir| lower == *dir || (lower.starts_with(&format!("{dir}/")) && *dir != "/users" && *dir != "/home" && *dir != "/opt"))
    };
    if system {
        return Some("system");
    }
    if lower.ends_with(".app") || lower.contains(".app/") || (cfg!(windows) && lower.contains("/program files")) {
        return Some("app");
    }
    if !home.is_empty() {
        if lower == home {
            return Some("personal");
        }
        if let Some(rest) = lower.strip_prefix(&format!("{home}/")) {
            let standard = ["desktop", "documents", "downloads", "pictures", "movies", "music", "videos", "library", "appdata", "applications", "public", "onedrive", ".ssh", ".gnupg"];
            if !rest.contains('/') && standard.contains(&rest) {
                return Some("personal");
            }
            if rest.starts_with("library/keychains") || rest.starts_with("library/mobile documents") {
                return Some("personal");
            }
        }
    }
    if let Some(repo) = repo {
        let repo = repo.to_string_lossy().replace('\\', "/").to_lowercase();
        if lower == repo || lower.starts_with(&format!("{repo}/")) || repo.starts_with(&format!("{lower}/")) {
            return Some("library");
        }
    }
    if let Ok(disks) = sysinfo::Disks::new_with_refreshed_list() {
        if disks.iter().any(|d| d.mount_point() == path) {
            return Some("mount");
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, bytes: usize) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![7u8; bytes]).unwrap();
    }

    fn sample() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        write(&base.join("big/video.mov"), 3 * 1024 * 1024);
        write(&base.join("big/inner/deep/blob.bin"), 2 * 1024 * 1024);
        write(&base.join("big/inner/deep/tiny.txt"), 100);
        for i in 0..20 {
            write(&base.join(format!("many/f{i}.txt")), 2000);
        }
        write(&base.join("top.iso"), 1536 * 1024);
        dir
    }

    #[test]
    fn scans_sizes_and_collapses_small_things() {
        let dir = sample();
        let cancel = AtomicBool::new(false);
        let scan = scan(dir.path(), &cancel, &|_| {}).unwrap();
        let tree = &scan.tree;
        assert_eq!(tree.files, 24);
        // 按大小排：big 在最前
        assert_eq!(tree.children[0].name, "big");
        let big = &tree.children[0];
        assert!(big.size >= 5 * 1024 * 1024);
        // 大文件单列；小文件合成一项；小文件夹 many（< 1 MB）不留子项
        assert!(big.children.iter().any(|c| c.kind == NodeKind::File && c.name == "video.mov"));
        let many = tree.children.iter().find(|c| c.name == "many").unwrap();
        assert!(many.children.is_empty() && many.files == 20);
        let deep = find(tree, &["big".into(), "inner".into(), "deep".into()]).unwrap();
        assert!(deep.children.iter().any(|c| c.kind == NodeKind::Small && c.files == 1));
        // 大小一层层对得上
        assert_eq!(tree.size, tree.children.iter().map(|c| c.size).sum::<u64>());
    }

    #[test]
    fn view_merges_tiny_children_and_removal_updates_totals() {
        let dir = sample();
        let cancel = AtomicBool::new(false);
        let mut scan = scan(dir.path(), &cancel, &|_| {}).unwrap();
        let view = view(&scan.tree, &[], 2, 0.2).unwrap();
        // 占不到 20% 的合成“其他”
        assert!(view.children.iter().any(|c| c.kind == NodeKind::Other));
        assert_eq!(view.children.iter().map(|c| c.size).sum::<u64>(), view.size);
        assert_eq!(view.children[0].path, vec!["big".to_string()]);

        let before = scan.tree.size;
        let removed = remove(&mut scan.tree, &["big".into(), "video.mov".into()]).unwrap();
        assert!(removed >= 3 * 1024 * 1024);
        assert_eq!(scan.tree.size, before - removed);
        assert_eq!(find(&scan.tree, &["big".into()]).unwrap().files, 2);
    }

    #[test]
    fn cancelling_stops_the_scan() {
        let dir = sample();
        let cancel = AtomicBool::new(true);
        assert!(scan(dir.path(), &cancel, &|_| {}).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn hard_links_count_once() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a/file.bin"), 2 * 1024 * 1024);
        std::fs::create_dir_all(dir.path().join("b")).unwrap();
        std::fs::hard_link(dir.path().join("a/file.bin"), dir.path().join("b/file.bin")).unwrap();
        let cancel = AtomicBool::new(false);
        let scan = scan(dir.path(), &cancel, &|_| {}).unwrap();
        assert!(scan.tree.size < 3 * 1024 * 1024, "{}", scan.tree.size);
    }

    #[test]
    fn paths_for_ai_hide_the_user_name() {
        let home = Path::new("/Users/me");
        assert_eq!(display_path(Path::new("/Users/me/Library/Caches"), Some(home)), "~/Library/Caches");
        assert_eq!(display_path(Path::new("/Users/me"), Some(home)), "~");
        assert_eq!(display_path(Path::new("/Users/meow/x"), Some(home)), "/Users/meow/x");
        assert_eq!(display_path(Path::new("/opt/homebrew"), Some(home)), "/opt/homebrew");
    }

    #[test]
    fn protects_system_and_personal_folders() {
        let root = Path::new("/");
        if cfg!(unix) {
            assert_eq!(protected(Path::new("/"), root, None), Some("root"));
            assert_eq!(protected(Path::new("/System/Library"), root, None), Some("system"));
            assert_eq!(protected(Path::new("/Applications/Foo.app"), root, None), Some("app"));
            let home = std::env::var("HOME").unwrap();
            assert_eq!(protected(&Path::new(&home).join("Documents"), root, None), Some("personal"));
            assert_eq!(protected(&Path::new(&home).join("Documents/old"), root, None), None);
            assert_eq!(protected(&Path::new(&home).join("Lib/x"), root, Some(&Path::new(&home).join("Lib"))), Some("library"));
        }
    }
}

/// 只读地扫一个真实的文件夹，看看速度和结果：`SHIBAPACK_DISK=~/某处 cargo test -p shibapack-core scans_real_folder -- --ignored --nocapture`
#[cfg(test)]
#[test]
#[ignore]
fn scans_real_folder() {
    let root = std::env::var("SHIBAPACK_DISK").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(std::env::var("HOME").unwrap()));
    let cancel = AtomicBool::new(false);
    let scan = scan(&root, &cancel, &|_| {}).unwrap();
    fn count(node: &Node) -> usize {
        1 + node.children.iter().map(count).sum::<usize>()
    }
    println!("{} · {} 个文件 · {:.1} GB · {} ms · 树上 {} 个节点 · 没有权限 {}", root.display(), scan.tree.files, scan.tree.size as f64 / 1e9, scan.elapsed_ms, count(&scan.tree), scan.denied);
    for child in scan.tree.children.iter().take(8) {
        println!("  {:>8.2} GB  {}", child.size as f64 / 1e9, child.name);
    }
}
