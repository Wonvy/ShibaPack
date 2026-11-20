//! 磁盘分析：扫一个文件夹（或整个盘）有多大、大在哪，给界面画扇形图和矩形图。
//!
//! 思路参考了 SquirrelDisk、Diskplot 这类磁盘可视化工具（只参考思路，代码是新写的）：
//! 先并行走一遍算出每个文件夹的大小，留在内存里；界面要哪一层就从这棵树里取，不用重新扫。
//!
//! - 大小按实际占用的磁盘空间算（macOS / Linux 用块数，稀疏文件不会虚高）；硬链接只算一次。
//! - 不跟着符号链接走，不进别的盘（扫 / 时不进 /Volumes 下面的外接盘）。
//! - 为了省内存：小于 1 MB 的文件不单独记，按文件夹合成一项“小文件”；扫完小于 1 MB 的文件夹不留子项。
//! - 什么都不存到磁盘上；没有权限的文件夹记个数，界面上提示。
//! - 边扫边看（`Live`）：扫的时候前两层每个文件夹已经累计了多少、扫完没有，和到现在为止最大的文件，
//!   界面隔一会儿取一次就能先画出图来，不用等整棵树拼好。

use serde::Serialize;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

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
    /// 边扫边看的累计；不要时是 None。
    live: Option<&'a Live>,
    root_dev: Option<u64>,
}

/// 扫 `root`。`cancel` 变成真时尽快停下，返回 `None`。`progress` 大约每 150 毫秒调一次。
/// 给了 `live` 时扫的过程中往里面累计，界面可以先看（见 `Live`）。
pub fn scan(root: &Path, cancel: &AtomicBool, progress: &(dyn Fn(Progress) + Sync), live: Option<&Live>) -> Option<Scan> {
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
        live,
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

/// 往下走：每个文件夹的子文件夹都放进 rayon 的任务队列，空闲的线程从别的线程那里拿还没扫的
/// （先拿最大块的，也就是靠上层的文件夹）。这样不管哪个文件夹特别大（~/Library、某个项目的
/// node_modules），线程都不会闲着；第一层的文件夹也是一起往前扫，不会等前一个扫完才开始下一个。
/// 用自己的线程池，不占全局的。
fn scan_parallel(root: &Path, shared: &Shared, threads: usize) -> Option<Node> {
    let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).thread_name(|i| format!("disk-scan-{i}")).build().ok()?;
    let mut tree = pool.install(|| walk(root.to_path_buf(), shared, 0, Vec::new()))?;
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

/// 读一层。`counters` 是边扫边看时这一层在前两层里的累计：文件多的文件夹边读边加，
/// 不等读完一整个文件夹才一下子加上（不然一个装了几万个文件的文件夹会让数字停住再猛跳）。
fn read_level(dir: &Path, shared: &Shared, counters: &[Arc<Counter>]) -> Option<Level> {
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
    // 还没加到累计里的
    let (mut unsent_bytes, mut unsent_files) = (0u64, 0u64);
    let send = |bytes: &mut u64, files: &mut u64| {
        for counter in counters {
            counter.bytes.fetch_add(*bytes, Ordering::Relaxed);
            counter.files.fetch_add(*files, Ordering::Relaxed);
        }
        *bytes = 0;
        *files = 0;
    };
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
            unsent_bytes += size;
            unsent_files += 1;
            if unsent_files >= SEND_EVERY {
                send(&mut unsent_bytes, &mut unsent_files);
            }
            if size >= SMALL_FILE {
                if let Some(live) = shared.live {
                    live.offer(size, &path);
                }
                let name = entry.file_name().to_string_lossy().to_string();
                level.loose.push(Node { name, kind: NodeKind::File, size, files: 1, children: Vec::new() });
            } else {
                small_size += size;
                small_count += 1;
            }
        }
    }
    send(&mut unsent_bytes, &mut unsent_files);
    if small_count > 0 {
        level.loose.push(Node { name: String::new(), kind: NodeKind::Small, size: small_size, files: small_count, children: Vec::new() });
    }
    Some(level)
}

/// 走完一个文件夹：子文件夹并行走（在 rayon 的线程池里），最后拼起来。
/// `depth` 是相对扫描根第几层（根是 0）；`counters` 是边扫边看时这个文件夹在前两层里的上级（含自己）的累计。
fn walk(dir: PathBuf, shared: &Shared, depth: usize, counters: Vec<Arc<Counter>>) -> Option<Node> {
    use rayon::prelude::*;
    if shared.cancel.load(Ordering::Relaxed) {
        return None;
    }
    // 走完（或者进不去）时，自己那一项标成扫完了
    let finish = |counters: &[Arc<Counter>]| {
        if depth > 0 && depth <= LIVE_DEPTH {
            if let Some(own) = counters.last() {
                own.done.store(true, Ordering::Release);
            }
        }
    };
    if let Ok(mut current) = shared.current.try_lock() {
        *current = dir.to_string_lossy().to_string();
    }
    let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let Some(level) = read_level(&dir, shared, &counters) else {
        finish(&counters);
        return Some(Node { name, kind: NodeKind::Dir, size: 0, files: 0, children: Vec::new() });
    };
    // 前两层：这一层的文件和子文件夹登记到边扫边看的表里
    let live = shared.live.filter(|_| depth < LIVE_DEPTH);
    let parent = if live.is_some() { relative(&shared.root, &dir) } else { Vec::new() };
    if let Some(live) = live {
        for item in &level.loose {
            live.add(&parent, item.name.clone(), item.kind, item.size, item.files, true);
        }
    }
    let mut node = Node { name, kind: NodeKind::Dir, size: 0, files: 0, children: level.loose };
    // 先把这一层的子文件夹都登记上（界面马上能看到它们都在、还没扫完），再一个个走
    let dirs: Vec<(PathBuf, Vec<Arc<Counter>>)> = level
        .dirs
        .into_iter()
        .map(|path| {
            let mut below = counters.clone();
            if let Some(live) = live {
                let child_name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                below.push(live.add(&parent, child_name, NodeKind::Dir, 0, 0, false));
            }
            (path, below)
        })
        .collect();
    // 一个文件夹一个任务（不按块切），哪个线程闲着就拿走一个
    let children: Vec<Node> = dirs.into_par_iter().with_max_len(1).filter_map(|(path, below)| walk(path, shared, depth + 1, below)).collect();
    node.children.extend(children);
    total(&mut node);
    if node.size < SMALL_DIR && dir != shared.root {
        node.children.clear();
    }
    finish(&counters);
    Some(node)
}

/// `dir` 相对扫描根的路径段。
fn relative(root: &Path, dir: &Path) -> Vec<String> {
    dir.strip_prefix(root).map(|rest| rest.iter().map(|s| s.to_string_lossy().to_string()).collect()).unwrap_or_default()
}

// ── 边扫边看 ─────────────────────────────────────────────

/// 一个文件夹里每读这么多个文件，就把读到的大小加到边扫边看的累计里一次。
const SEND_EVERY: u64 = 256;

/// 边扫边看时累计到第几层（1 是扫描根下面直接的那些，2 是再下一层）。
const LIVE_DEPTH: usize = 2;

/// 一项已经累计了多少、扫完没有。
#[derive(Debug, Default)]
pub struct Counter {
    bytes: AtomicU64,
    files: AtomicU64,
    done: AtomicBool,
}

struct LiveEntry {
    /// 所在文件夹相对扫描根的路径段（根是空的）。
    parent: Vec<String>,
    name: String,
    kind: NodeKind,
    counter: Arc<Counter>,
}

/// 扫描中的结果：前两层每一项累计到多少、扫完没有，和到现在为止最大的文件。扫描线程往里写，界面随时来取。
pub struct Live {
    root: PathBuf,
    entries: Mutex<Vec<LiveEntry>>,
    largest: Mutex<BinaryHeap<Reverse<(u64, PathBuf)>>>,
    /// 最大文件表满了以后，表里最小的那个。比它小的文件不用去抢锁。
    floor: AtomicU64,
    limit: usize,
}

impl Live {
    /// `limit` 是最大的文件留几个。
    pub fn new(root: &Path, limit: usize) -> Live {
        Live { root: root.to_path_buf(), entries: Mutex::new(Vec::new()), largest: Mutex::new(BinaryHeap::new()), floor: AtomicU64::new(0), limit }
    }

    fn add(&self, parent: &[String], name: String, kind: NodeKind, size: u64, files: u64, done: bool) -> Arc<Counter> {
        let counter = Arc::new(Counter { bytes: AtomicU64::new(size), files: AtomicU64::new(files), done: AtomicBool::new(done) });
        if let Ok(mut entries) = self.entries.lock() {
            entries.push(LiveEntry { parent: parent.to_vec(), name, kind, counter: Arc::clone(&counter) });
        }
        counter
    }

    fn offer(&self, size: u64, path: &Path) {
        if self.limit == 0 || size <= self.floor.load(Ordering::Relaxed) {
            return;
        }
        if let Ok(mut heap) = self.largest.lock() {
            heap.push(Reverse((size, path.to_path_buf())));
            if heap.len() > self.limit {
                heap.pop();
            }
            if heap.len() >= self.limit {
                if let Some(Reverse((min, _))) = heap.peek() {
                    self.floor.store(*min, Ordering::Relaxed);
                }
            }
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 到现在为止最大的文件（路径段相对扫描根，从大到小）。
    pub fn largest(&self) -> Vec<(Vec<String>, u64)> {
        let Ok(heap) = self.largest.lock() else { return Vec::new() };
        let mut out: Vec<(Vec<String>, u64)> = heap.iter().map(|Reverse((size, path))| (relative(&self.root, path), *size)).collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        out
    }

    /// 现在的样子，给界面画。`path` 只能是根（空）或第一层的某个文件夹；还没扫完的项 `pending` 为真。
    /// 占不到 `min_ratio` 的合成“其他”。第一层的文件夹能点进去看第二层。
    pub fn view(&self, path: &[String], min_ratio: f64) -> Option<ViewNode> {
        if path.len() > 1 {
            return None;
        }
        let entries = self.entries.lock().ok()?;
        let item = |e: &LiveEntry, children: Vec<ViewNode>| {
            // 和扫完后的视图一样：“小文件”没有自己的路径
            let entry_path = if e.kind == NodeKind::Small { Vec::new() } else { [e.parent.clone(), vec![e.name.clone()]].concat() };
            ViewNode {
                name: e.name.clone(),
                path: entry_path,
                kind: e.kind,
                size: e.counter.bytes.load(Ordering::Relaxed),
                files: e.counter.files.load(Ordering::Relaxed),
                has_children: false,
                pending: !e.counter.done.load(Ordering::Acquire),
                children,
            }
        };
        let level = |parent: &[String]| -> Vec<ViewNode> {
            entries.iter().filter(|e| e.parent == parent).map(|e| {
                // 第一层的文件夹带上它下面的第二层（扇形图画两圈）
                let children = if parent.is_empty() && e.kind == NodeKind::Dir {
                    let mut inner = e.parent.clone();
                    inner.push(e.name.clone());
                    entries.iter().filter(|c| c.parent == inner).map(|c| item(c, Vec::new())).collect()
                } else {
                    Vec::new()
                };
                let mut node = item(e, children);
                node.has_children = parent.is_empty() && e.kind == NodeKind::Dir && entries.iter().any(|c| c.parent.len() == 1 && c.parent[0] == e.name);
                node
            }).collect()
        };
        let children = level(path);
        let (name, pending) = match path.first() {
            None => (display_root(&self.root), children.iter().any(|c| c.pending)),
            Some(first) => {
                let own = entries.iter().find(|e| e.parent.is_empty() && e.name == *first)?;
                (first.clone(), !own.counter.done.load(Ordering::Acquire))
            }
        };
        drop(entries);
        let size: u64 = children.iter().map(|c| c.size).sum();
        let files: u64 = children.iter().map(|c| c.files).sum();
        Some(ViewNode {
            name,
            path: path.to_vec(),
            kind: NodeKind::Dir,
            size,
            files,
            has_children: !children.is_empty(),
            pending,
            children: group_small(children, (size as f64 * min_ratio) as u64),
        })
    }
}

/// 按大小排好，太小的合成一个“其他”（每一层都这样）。
fn group_small(mut children: Vec<ViewNode>, threshold: u64) -> Vec<ViewNode> {
    children.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
    let mut out = Vec::new();
    let (mut size, mut files, mut count) = (0u64, 0u64, 0u64);
    for mut child in children {
        // 还在扫的文件夹哪怕现在还小也留着，过一会儿可能就大了
        if child.size < threshold.max(1) && !child.pending {
            size += child.size;
            files += child.files;
            count += 1;
            continue;
        }
        let inner = std::mem::take(&mut child.children);
        child.children = group_small(inner, threshold);
        out.push(child);
    }
    if count > 0 {
        out.push(ViewNode { name: count.to_string(), path: Vec::new(), kind: NodeKind::Other, size, files, has_children: false, pending: false, children: Vec::new() });
    }
    out
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
    /// 边扫边看时：这一项还没扫完，大小还会变。
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub pending: bool,
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
                pending: false,
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
        pending: false,
        children,
    }
}

/// `path` 那个文件夹（连同下面所有层）里最大的 `limit` 个文件，从大到小。
/// 返回每个文件的路径段（相对扫描根）和大小。小于 1 MB 的文件扫描时没单独记，不会出现在这里。
pub fn largest_files(tree: &Node, path: &[String], limit: usize) -> Option<Vec<(Vec<String>, u64)>> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;
    let start = find(tree, path)?;
    // 小顶堆只留最大的 limit 个：比堆里最小的还小就不用看了
    let mut heap: BinaryHeap<Reverse<(u64, Vec<String>)>> = BinaryHeap::new();
    let mut stack: Vec<(&Node, Vec<String>)> = vec![(start, path.to_vec())];
    while let Some((node, at)) = stack.pop() {
        for child in &node.children {
            match child.kind {
                NodeKind::File => {
                    if limit == 0 || (heap.len() >= limit && heap.peek().is_some_and(|Reverse((min, _))| child.size <= *min)) {
                        continue;
                    }
                    let mut file = at.clone();
                    file.push(child.name.clone());
                    heap.push(Reverse((child.size, file)));
                    if heap.len() > limit {
                        heap.pop();
                    }
                }
                NodeKind::Dir => {
                    // 整个文件夹都比堆里最小的小，里面不会有更大的文件
                    if heap.len() >= limit && heap.peek().is_some_and(|Reverse((min, _))| child.size <= *min) {
                        continue;
                    }
                    let mut dir = at.clone();
                    dir.push(child.name.clone());
                    stack.push((child, dir));
                }
                NodeKind::Small | NodeKind::Other => {}
            }
        }
    }
    let mut out: Vec<(Vec<String>, u64)> = heap.into_iter().map(|Reverse((size, path))| (path, size)).collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Some(out)
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

    /// 在本机的用户文件夹上量一下：边扫边看拖不拖慢扫描，扫的途中能不能先看到东西。
    /// `cargo test --release -p shibapack-core live_on_home -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_on_home() {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        let cancel = AtomicBool::new(false);
        let plain = scan(&home, &cancel, &|_| {}, None).unwrap();
        println!("不边扫边看：{} 个文件，{} ms", plain.tree.files, plain.elapsed_ms);
        let live = Live::new(&home, 200);
        let done = AtomicBool::new(false);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                let started = std::time::Instant::now();
                while !done.load(Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    if let Some(view) = live.view(&[], 0.004) {
                        let pending = view.children.iter().filter(|c| c.pending).count();
                        let biggest = view.children.first().map(|c| format!("{} {}", c.name, c.size / 1_000_000)).unwrap_or_default();
                        println!("  {:>5} ms：第一层 {} 项，{} 项没扫完，最大 {} MB，已有 {} 个大文件", started.elapsed().as_millis(), view.children.len(), pending, biggest, live.largest().len());
                    }
                }
            });
            let with = scan(&home, &cancel, &|_| {}, Some(&live)).unwrap();
            done.store(true, Ordering::Relaxed);
            println!("边扫边看：{} 个文件，{} ms", with.tree.files, with.elapsed_ms);
            let view = live.view(&[], 0.0).unwrap();
            assert_eq!(view.size, with.tree.size);
            assert!(!view.pending);
        });
    }

    #[test]
    fn live_view_matches_the_finished_scan() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("a/deep/deeper")).unwrap();
        std::fs::create_dir_all(root.join("b")).unwrap();
        std::fs::write(root.join("a/deep/deeper/big.bin"), vec![1u8; 3 * 1024 * 1024]).unwrap();
        std::fs::write(root.join("a/one.bin"), vec![1u8; 2 * 1024 * 1024]).unwrap();
        std::fs::write(root.join("b/small.txt"), "hi").unwrap();
        std::fs::write(root.join("top.bin"), vec![1u8; 1024 * 1024 + 10]).unwrap();
        let live = Live::new(root, 10);
        let cancel = AtomicBool::new(false);
        let scan = scan(root, &cancel, &|_| {}, Some(&live)).unwrap();

        let view = live.view(&[], 0.0).unwrap();
        assert!(!view.pending, "扫完以后都不是 pending");
        assert_eq!(view.size, scan.tree.size);
        let a = view.children.iter().find(|c| c.name == "a").unwrap();
        assert_eq!(a.size, find(&scan.tree, &["a".into()]).unwrap().size);
        assert!(a.has_children);
        assert!(a.children.iter().any(|c| c.name == "deep" && c.path == vec!["a", "deep"]));
        assert!(view.children.iter().any(|c| c.kind == NodeKind::File && c.name == "top.bin"));
        // 第一层能点进去，再往下不行
        let inside = live.view(&["a".into()], 0.0).unwrap();
        assert_eq!(inside.size, a.size);
        assert!(live.view(&["a".into(), "deep".into()], 0.0).is_none());
        // 最大的文件
        let largest = live.largest();
        assert_eq!(largest[0].0, vec!["a", "deep", "deeper", "big.bin"]);
        assert_eq!(largest.len(), 3);
    }

    #[test]
    fn largest_files_across_levels() {
        let file = |name: &str, size: u64| Node { name: name.into(), kind: NodeKind::File, size, files: 1, children: vec![] };
        let dir = |name: &str, children: Vec<Node>| Node {
            name: name.into(),
            kind: NodeKind::Dir,
            size: children.iter().map(|c| c.size).sum(),
            files: children.iter().map(|c| c.files).sum(),
            children,
        };
        let tree = dir("root", vec![
            dir("a", vec![file("big.iso", 900), dir("deep", vec![file("huge.vmdk", 2000), file("tiny", 5)])]),
            dir("b", vec![file("mid.mp4", 500), Node { name: String::new(), kind: NodeKind::Small, size: 3000, files: 99, children: vec![] }]),
            file("top.dmg", 700),
        ]);
        let top = largest_files(&tree, &[], 3).unwrap();
        let names: Vec<String> = top.iter().map(|(p, s)| format!("{}={s}", p.join("/"))).collect();
        assert_eq!(names, vec!["a/deep/huge.vmdk=2000", "a/big.iso=900", "top.dmg=700"]);
        // 只看某个文件夹里的
        let in_a = largest_files(&tree, &["a".into()], 10).unwrap();
        assert_eq!(in_a.len(), 3);
        assert_eq!(in_a[0].0, vec!["a", "deep", "huge.vmdk"]);
        assert!(largest_files(&tree, &["nope".into()], 10).is_none());
    }

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
        let scan = scan(dir.path(), &cancel, &|_| {}, None).unwrap();
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
        let mut scan = scan(dir.path(), &cancel, &|_| {}, None).unwrap();
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
        assert!(scan(dir.path(), &cancel, &|_| {}, None).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn hard_links_count_once() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a/file.bin"), 2 * 1024 * 1024);
        std::fs::create_dir_all(dir.path().join("b")).unwrap();
        std::fs::hard_link(dir.path().join("a/file.bin"), dir.path().join("b/file.bin")).unwrap();
        let cancel = AtomicBool::new(false);
        let scan = scan(dir.path(), &cancel, &|_| {}, None).unwrap();
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
    let scan = scan(&root, &cancel, &|_| {}, None).unwrap();
    fn count(node: &Node) -> usize {
        1 + node.children.iter().map(count).sum::<usize>()
    }
    println!("{} · {} 个文件 · {:.1} GB · {} ms · 树上 {} 个节点 · 没有权限 {}", root.display(), scan.tree.files, scan.tree.size as f64 / 1e9, scan.elapsed_ms, count(&scan.tree), scan.denied);
    for child in scan.tree.children.iter().take(8) {
        println!("  {:>8.2} GB  {}", child.size as f64 / 1e9, child.name);
    }
}
