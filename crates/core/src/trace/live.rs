//! 安装期间实时监听文件改动。
//!
//! 快照对比才是结果的依据；这里只做两件事：
//! - 安装时让界面实时看到装到哪了（各类别动了几个文件、最近改的是哪几个）。
//! - 记下安装期间出现过、装完又被删掉的文件（解压出来的临时文件），快照对比看不到它们。
//!
//! macOS 用 FSEvents，Windows 用 ReadDirectoryChangesW，Linux 用 inotify（都由 notify 库包好）。
//! 注册表的改动不在这里看，装完对比快照时才知道。

use super::{categorize, is_noise, noise_place, Category, ItemKind, Op, Scope};
use notify::{event::ModifyKind, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};

/// 最近改动最多留几条给界面看。
const RECENT: usize = 6;
/// 临时文件最多记几个。
pub const MAX_TRANSIENT: usize = 300;
/// 一次监控最多跟踪这么多个路径，再多的只计数（防止什么程序疯狂写文件把内存吃光）。
const MAX_SEEN: usize = 500_000;

/// 一次改动。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveEvent {
    pub op: Op,
    pub path: String,
    pub category: Category,
    /// 开始监听后过了多少毫秒。
    pub at_ms: u64,
}

/// 到现在为止看到的改动。
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveSummary {
    /// 动过的文件（不算噪音，同一个文件只算一次）。
    pub files: u64,
    /// 按类别。
    pub counts: HashMap<Category, u64>,
    /// 噪音动过几个文件。
    pub noise: u64,
    /// 最近的几次改动，新的在前。
    pub recent: Vec<LiveEvent>,
}

/// 安装期间出现过、装完已经不在的文件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transient {
    pub path: String,
    pub category: Category,
    /// 在多半无关的地方（缓存、系统自己的文件夹……）。文件名不算：安装程序自己的 .tmp 正是要看的。
    pub noise: bool,
}

struct Seen {
    /// 监听期间被新建过。
    created: bool,
    category: Category,
    /// 在无关的地方（只看位置），记临时文件时用。
    place_noise: bool,
}

struct State {
    seen: HashMap<PathBuf, Seen>,
    counts: HashMap<Category, u64>,
    files: u64,
    noise: u64,
    recent: VecDeque<LiveEvent>,
    started: Instant,
    exclude: Vec<PathBuf>,
}

impl State {
    fn record(&mut self, event: Event) {
        let op = match event.kind {
            EventKind::Create(_) => Op::Added,
            EventKind::Remove(_) => Op::Removed,
            // 改名：按改完后在不在算新增或删除（拖进“应用程序”、安装程序把临时文件换上去都是改名）
            EventKind::Modify(ModifyKind::Name(_)) => {
                for path in &event.paths {
                    let op = if path.exists() { Op::Added } else { Op::Removed };
                    self.one(op, path);
                }
                return;
            }
            EventKind::Modify(ModifyKind::Metadata(_)) | EventKind::Access(_) | EventKind::Other => return,
            EventKind::Modify(_) | EventKind::Any => Op::Modified,
        };
        for path in &event.paths {
            self.one(op, path);
        }
    }

    fn one(&mut self, op: Op, path: &Path) {
        if self.exclude.iter().any(|ex| path.starts_with(ex)) {
            return;
        }
        // 文件夹不算：里面的文件会各自报上来
        if op != Op::Removed && path.is_dir() {
            return;
        }
        let text = path.to_string_lossy();
        let category = categorize(&text, ItemKind::File, false);
        let noise = is_noise(&text);
        match self.seen.get_mut(path) {
            Some(seen) => seen.created |= op == Op::Added,
            None => {
                if self.seen.len() >= MAX_SEEN {
                    return;
                }
                self.seen.insert(path.to_path_buf(), Seen { created: op == Op::Added, category, place_noise: noise_place(&text) });
                if noise {
                    self.noise += 1;
                } else {
                    self.files += 1;
                    *self.counts.entry(category).or_default() += 1;
                }
            }
        }
        if noise {
            return;
        }
        // 同一个文件只留最近一条；新建以后接着写内容还算“新增”
        let mut op = op;
        if let Some(at) = self.recent.iter().position(|e| e.path == text) {
            if let Some(old) = self.recent.remove(at) {
                if old.op == Op::Added && op == Op::Modified {
                    op = Op::Added;
                }
            }
        }
        self.recent.push_front(LiveEvent { op, path: text.to_string(), category, at_ms: self.started.elapsed().as_millis() as u64 });
        self.recent.truncate(RECENT);
    }
}

/// 正在监听。丢掉就停。
pub struct LiveWatch {
    _watcher: RecommendedWatcher,
    state: Arc<Mutex<State>>,
}

impl LiveWatch {
    /// 监听范围里的每个文件夹（连同下面所有层）。一个都监听不了时返回 None。
    pub fn start(scope: &Scope) -> Option<LiveWatch> {
        let state = Arc::new(Mutex::new(State {
            seen: HashMap::new(),
            counts: HashMap::new(),
            files: 0,
            noise: 0,
            recent: VecDeque::new(),
            started: Instant::now(),
            exclude: scope.exclude.clone(),
        }));
        let sink = Arc::clone(&state);
        let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
            if let (Ok(event), Ok(mut state)) = (result, sink.lock()) {
                state.record(event);
            }
        })
        .ok()?;
        let mut watching = false;
        for path in watch_roots(scope) {
            watching |= watcher.watch(&path, RecursiveMode::Recursive).is_ok();
        }
        watching.then_some(LiveWatch { _watcher: watcher, state })
    }

    pub fn summary(&self) -> LiveSummary {
        let Ok(state) = self.state.lock() else { return LiveSummary::default() };
        LiveSummary { files: state.files, counts: state.counts.clone(), noise: state.noise, recent: state.recent.iter().cloned().collect() }
    }

    /// 安装期间新建过、现在已经不在的文件（只留前 `MAX_TRANSIENT` 个，不是噪音的在前），和一共有几个。
    pub fn transient(&self) -> (Vec<Transient>, u64) {
        let Ok(state) = self.state.lock() else { return (Vec::new(), 0) };
        let mut gone: Vec<Transient> = state
            .seen
            .iter()
            .filter(|(path, seen)| seen.created && !path.exists())
            .map(|(path, seen)| Transient { path: path.to_string_lossy().to_string(), category: seen.category, noise: seen.place_noise })
            .collect();
        let count = gone.len() as u64;
        gone.sort_by(|a, b| a.noise.cmp(&b.noise).then_with(|| a.path.cmp(&b.path)));
        gone.truncate(MAX_TRANSIENT);
        (gone, count)
    }
}

/// 要监听的文件夹：范围里的根，去掉被别的根包含的（监听本身就往下走所有层），也去掉不扫的地方。
fn watch_roots(scope: &Scope) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = scope.roots.iter().map(|r| r.path.clone()).filter(|p| p.is_dir()).collect();
    roots.sort();
    roots.dedup();
    let all = roots.clone();
    roots.retain(|p| !all.iter().any(|other| other != p && p.starts_with(other)));
    roots.retain(|p| !scope.exclude.iter().any(|ex| p.starts_with(ex)));
    roots
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::Root;
    use std::{fs, time::Duration};

    #[test]
    fn nested_roots_are_watched_once() {
        let scope = Scope {
            roots: vec![
                Root { path: std::env::temp_dir(), depth: 1 },
                Root { path: std::env::temp_dir().join("x"), depth: 1 },
            ],
            registry: vec![],
            exclude: vec![],
        };
        assert_eq!(watch_roots(&scope).len(), 1);
    }

    #[test]
    fn counts_files_and_remembers_temporary_ones() {
        let dir = tempfile::tempdir().unwrap();
        // macOS 的临时文件夹在 /var 下，是 /private/var 的链接；FSEvents 报的是真实路径
        let root = dir.path().canonicalize().unwrap();
        let scope = Scope { roots: vec![Root { path: root.clone(), depth: 4 }], registry: vec![], exclude: vec![] };
        let watch = LiveWatch::start(&scope).expect("watch");
        std::thread::sleep(Duration::from_millis(300));
        fs::create_dir_all(root.join("Foo")).unwrap();
        fs::write(root.join("Foo/keep.bin"), "1").unwrap();
        fs::write(root.join("Foo/temp.tmp"), "2").unwrap();
        std::thread::sleep(Duration::from_millis(300));
        fs::remove_file(root.join("Foo/temp.tmp")).unwrap();
        // FSEvents 有一点延迟
        let mut summary = LiveSummary::default();
        for _ in 0..40 {
            std::thread::sleep(Duration::from_millis(100));
            summary = watch.summary();
            if summary.files >= 1 && !watch.transient().0.is_empty() {
                break;
            }
        }
        assert!(summary.files >= 1, "{summary:?}");
        assert!(!summary.recent.is_empty());
        let (gone, count) = watch.transient();
        assert!(count >= 1);
        assert!(gone.iter().any(|t| t.path.ends_with("temp.tmp") && !t.noise), "安装程序自己的 .tmp 不算噪音：{gone:?}");
        let recent: Vec<&str> = summary.recent.iter().map(|e| e.path.as_str()).collect();
        assert!(recent.iter().all(|p| recent.iter().filter(|q| *q == p).count() == 1), "{recent:?}");
        assert!(!recent.iter().any(|p| p.ends_with("/Foo")), "文件夹不算：{recent:?}");
        assert!(!gone.iter().any(|t| t.path.ends_with("keep.bin")));
    }
}
