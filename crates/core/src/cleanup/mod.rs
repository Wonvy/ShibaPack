//! 清理：找出能删的缓存、临时文件、日志、散落的安装包，用户勾选后删掉。
//!
//! 参考了几个开源清理工具的做法（只参考思路，规则和代码都是新写的）：
//! - MangoDisk（GPL-3.0）：规则写成声明式的文件编进程序，路径只能用规定好的变量，每条规则有风险档位和“为什么能删”的依据。
//! - vyrti/cleaner（Apache-2.0）：分档、只默认勾纯缓存；不跟随链接、不跨盘。
//! - CrapCleaner（MIT）：每项都写清是什么、删了会怎样；删不掉的如实报跳过；每次清理留一份清单。
//! - BleachBit（GPL-3.0）、CCleaner：先预览再删除，清浏览器缓存前要先关掉浏览器。
//!
//! 流程：`scan` 只读，量出每一项多大，结果留在后端；界面勾选后只把项的 id 和不要的条目序号传回来，
//! `clean` 在这次扫描的结果里找到路径，删之前按 `Guard` 再查一遍每一条，然后直接删除（安全档）或者移到回收站。

pub mod elevate;
pub mod guard;
pub mod history;
pub mod measure;
pub mod rule;
pub mod special;
pub mod vars;

pub use rule::{Category, Risk, Text};
pub use special::LibraryFile;
pub use vars::Os;

use guard::{Guard, Scope};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};
use vars::Vars;

/// 界面上每一项最多列出多少条。
const MAX_ENTRIES: usize = 200;

/// 怎么清。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    /// 直接删除：删了会自己重新生成的缓存。进回收站并不腾空间，几万个小文件进回收站也很慢。
    Delete,
    /// 移到回收站（废纸篓）。
    Trash,
    /// 清空回收站。
    EmptyTrash,
    /// 清空剪贴板。
    ClearClipboard,
}

/// 能清理的一项。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanItem {
    pub id: String,
    pub category: Category,
    pub risk: Risk,
    pub action: Action,
    pub name: Text,
    pub about: Text,
    pub effect: Text,
    /// 属于哪个应用（界面上用它的图标，“请先关闭”时显示它）。
    pub app: Option<String>,
    pub evidence: Option<String>,
    /// 在哪些文件夹里找的。
    pub roots: Vec<String>,
    /// 一共多大。读不到时（macOS 的废纸篓要完全磁盘访问权限）是 None。
    pub size: Option<u64>,
    pub files: u64,
    /// 要清的条目（根目录下第一层），最大的在前，最多 200 条。
    pub entries: Vec<Entry>,
    pub entry_count: usize,
    /// 里面最近一次改动（毫秒）。
    pub modified: Option<i64>,
    /// 要先关掉的程序正在运行。
    pub running: bool,
    /// 能帮忙退出那个程序。
    pub can_quit: bool,
    /// `unreadable`：有的地方没权限读；`full_disk_access`：要“完全磁盘访问权限”才量得出来。
    pub note: Option<String>,
    /// 能导入仓库（还没收进仓库的安装包）。
    pub importable: bool,
    /// 要管理员权限：清理时一起交给提权的进程做。
    pub admin: bool,
    /// 浏览器的 Cookie：有保留名单时只删名单以外的网站。
    pub cookies: bool,
    /// 这里不清，要到系统自己的设置里清（`storage`：存储设置）。比如 Windows.old。
    pub tool: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// 在这一项里的序号。不想清某一条时把序号传回来。
    pub index: usize,
    pub path: String,
    pub size: u64,
    pub files: u64,
    pub modified: Option<i64>,
    /// 散落的安装包：仓库里对应的软件。
    pub library: Option<LibraryMatch>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryMatch {
    pub software: String,
    pub versions: Vec<String>,
    /// 和仓库里的文件一模一样。
    pub same: bool,
}

/// 用户自己加的清理文件夹和排除的文件夹。只和这台电脑有关，存在本机应用数据目录。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    #[serde(default)]
    pub custom: Vec<PathBuf>,
    #[serde(default)]
    pub exclude: Vec<PathBuf>,
    /// 清 Cookie 时保留这些网站（域名，包括它的子域名）。
    #[serde(default)]
    pub keep_cookies: Vec<String>,
    /// 用户自己加的清理规则。旧版本里只有文件夹的 `custom` 存的时候会并进来。
    #[serde(default)]
    pub rules: Vec<CustomRule>,
}

/// 用户自己加的一条清理规则：清某个文件夹下第一层的东西。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomRule {
    pub name: String,
    pub path: PathBuf,
    /// 只清名字符合这些通配的（`*.log`）。空的话文件夹里的都清。
    #[serde(default)]
    pub names: Vec<String>,
    /// 只清这么多天里都没动过的。
    #[serde(default)]
    pub older_than_days: Option<u32>,
    /// 清理前要先关掉的程序（进程名）。
    #[serde(default)]
    pub processes: Vec<String>,
    /// 直接删除，不进回收站。
    #[serde(default)]
    pub delete: bool,
}

impl Options {
    /// 全部自定义规则：旧的只有文件夹的也算（当成“里面的都清、移到回收站”的规则）。
    pub fn all_rules(&self) -> Vec<CustomRule> {
        let folders = self.custom.iter().map(|path| CustomRule {
            name: path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.display().to_string()),
            path: path.clone(),
            ..CustomRule::default()
        });
        folders.chain(self.rules.iter().cloned()).collect()
    }
}

const OPTIONS_FILE: &str = "cleanup-options.json";

pub fn load_options(dir: &Path) -> Options {
    std::fs::read(dir.join(OPTIONS_FILE)).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

pub fn save_options(dir: &Path, options: &Options) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(OPTIONS_FILE), serde_json::to_vec_pretty(options).map_err(std::io::Error::other)?)
}

/// 检查用户给的设置：自定义文件夹要过保护检查（不行时返回 `cleanup_protected:路径` 这样的错误），去掉重复的。
pub fn check_options(guard: &Guard, options: Options) -> Result<Options, String> {
    let mut out = Options::default();
    // 旧的只有文件夹的并进规则里
    for mut rule in options.all_rules() {
        guard.custom_root(&rule.path).map_err(|reason| format!("cleanup_{reason}:{}", rule.path.display()))?;
        rule.name = rule.name.trim().to_string();
        if rule.name.is_empty() {
            rule.name = rule.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        }
        rule.names = rule.names.iter().map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect();
        for pattern in &rule.names {
            if pattern.contains('/') || pattern.contains('\\') || pattern.chars().all(|c| c == '*' || c == '?') {
                return Err(format!("cleanup_bad_pattern:{pattern}"));
            }
        }
        if rule.older_than_days == Some(0) {
            rule.older_than_days = None;
        }
        rule.processes = rule.processes.iter().map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect();
        if !out.rules.contains(&rule) {
            out.rules.push(rule);
        }
    }
    for path in options.exclude {
        if !path.is_absolute() {
            return Err(format!("cleanup_missing:{}", path.display()));
        }
        if !out.exclude.contains(&path) {
            out.exclude.push(path);
        }
    }
    for site in options.keep_cookies {
        let domain = special::normalize_domain(&site).ok_or_else(|| format!("cleanup_bad_domain:{site}"))?;
        if !out.keep_cookies.contains(&domain) {
            out.keep_cookies.push(domain);
        }
    }
    Ok(out)
}

/// 扫描和清理要用到的东西。
pub struct Context {
    pub os: Os,
    pub vars: Vars,
    pub guard: Guard,
    pub options: Options,
    /// 仓库里的文件，拿来和散落的安装包比对。
    pub library: Vec<LibraryFile>,
    /// 仓库里下载用的临时文件夹（ShibaPack 自己没下完的文件）。
    pub repo_downloads: Option<PathBuf>,
    /// 在提权的进程里：要管理员权限的项在这里才清。
    pub elevated: bool,
    /// 仓库在哪：交给提权的进程，让它也封住仓库。
    pub repo_root: Option<PathBuf>,
    /// macOS 上读不了废纸篓时，问访达里面有几项（第一次会弹授权提示，自检和测试时不问）。
    pub ask_finder: bool,
    /// 放缓存的文件夹（散落安装包算过的 SHA-256）。本机应用数据目录，不在仓库里。
    pub cache_dir: Option<PathBuf>,
}

impl Context {
    pub fn new(repo_root: Option<&Path>, repo_downloads: Option<PathBuf>, library: Vec<LibraryFile>, options: Options) -> Context {
        let vars = Vars::from_env();
        let guard = Guard::new(&vars, repo_root, repo_downloads.as_deref());
        let mut ctx = Context::with(vars, guard, options, library, repo_downloads);
        ctx.repo_root = repo_root.map(Path::to_path_buf);
        ctx
    }

    /// 给定变量和保护检查（测试用）。要管理员权限的规则能清的系统文件夹在这里按规则展开。
    pub fn with(vars: Vars, guard: Guard, options: Options, library: Vec<LibraryFile>, repo_downloads: Option<PathBuf>) -> Context {
        let os = Os::current();
        let admin_roots: Vec<PathBuf> = rule::catalog()
            .iter()
            .filter(|r| r.admin)
            .flat_map(|r| r.roots.get(os).iter().flat_map(|t| vars::expand(t, &vars)))
            .collect();
        let guard = guard.with_admin_roots(&admin_roots);
        Context { os, vars, guard, options, library, repo_downloads, elevated: false, repo_root: None, ask_finder: false, cache_dir: None }
    }
}

/// 一次扫描的结果。清理时只认这里面的项和路径。
#[derive(Debug, Clone, Default)]
pub struct Scan {
    found: Vec<Found>,
}

#[derive(Debug, Clone)]
struct Found {
    item: CleanItem,
    targets: Vec<Target>,
    processes: Vec<String>,
    filter: Filter,
    /// 不删文件，运行这条命令。
    command: Option<String>,
}

#[derive(Debug, Clone)]
struct Target {
    path: PathBuf,
    root: PathBuf,
    size: u64,
}

/// 根目录下第一层的哪些要清。
#[derive(Debug, Clone, Default)]
struct Filter {
    names: Vec<String>,
    keep: Vec<String>,
    older_than: Option<Duration>,
    own_only: bool,
    /// 隐私痕迹：可以删浏览记录和 Cookie。
    private: bool,
    /// 要管理员权限。
    admin: bool,
    /// Firefox 的 places.sqlite：只删里面的浏览记录，不删文件。
    history_db: bool,
}

impl Filter {
    fn narrow(&self) -> bool {
        !self.names.is_empty()
    }

    fn scope(&self) -> Scope {
        Scope { narrow: self.narrow(), private: self.private, admin: self.admin, history_db: self.history_db }
    }

    fn wants(&self, name: &str) -> bool {
        (self.names.is_empty() || self.names.iter().any(|p| vars::glob(p, name))) && !self.keep.iter().any(|p| vars::glob(p, name))
    }
}

impl Scan {
    pub fn items(&self) -> Vec<CleanItem> {
        self.found.iter().map(|f| f.item.clone()).collect()
    }

    pub fn item(&self, id: &str) -> Option<&CleanItem> {
        self.found.iter().find(|f| f.item.id == id).map(|f| &f.item)
    }

    /// 在文件管理器里定位用的路径：某一条，或者这一项的第一个根目录。
    pub fn locate(&self, id: &str, index: Option<usize>) -> Option<PathBuf> {
        let found = self.found.iter().find(|f| f.item.id == id)?;
        match index {
            Some(i) => found.targets.get(i).map(|t| t.path.clone()),
            None => found.item.roots.first().map(PathBuf::from),
        }
    }

    /// 只重新量这几项（清理完、退出程序以后），别的不动。量完没东西了的项拿掉。
    pub fn rescan(&mut self, ctx: &Context, ids: &[String]) {
        let running = special::running_processes();
        let me = ctx.vars.get("home").and_then(owner);
        let now = SystemTime::now();
        let list = sources(ctx);
        let loose = ids.iter().any(|id| id.starts_with("installers.") && id.ends_with("-library")).then(|| loose_found(ctx));
        for id in ids {
            let fresh: Option<Found> = match id.as_str() {
                "trash.empty" => trash_found(ctx),
                "privacy.clipboard" => clipboard_found(),
                "system.windows-old" => windows_old_found(ctx),
                "installers.in-library" | "installers.not-in-library" => loose.as_ref().and_then(|found| found.iter().find(|f| &f.item.id == id).cloned()),
                _ => list.iter().find(|source| &source.id == id).and_then(|source| measure_source(source, ctx, me, now)),
            };
            let at = self.found.iter().position(|f| &f.item.id == id);
            match (at, fresh) {
                (Some(i), Some(mut f)) => {
                    f.item.running = special::any_running(&f.processes, &running);
                    self.found[i] = f;
                }
                (Some(i), None) => {
                    self.found.remove(i);
                }
                (None, Some(mut f)) => {
                    f.item.running = special::any_running(&f.processes, &running);
                    self.found.push(f);
                }
                (None, None) => {}
            }
        }
    }

    /// 给 AI 看的这一项的情况：名字、类别、怎么清、在哪、多大、里面最大的几样。用户文件夹写成 `~`，不把用户名发出去。
    pub fn facts(&self, id: &str, home: Option<&Path>) -> Option<String> {
        let found = self.found.iter().find(|f| f.item.id == id)?;
        let item = &found.item;
        let home = home.map(|h| h.to_string_lossy().to_string()).filter(|h| h.len() > 1);
        let short = |path: &str| match &home {
            Some(home) if path.starts_with(home.as_str()) => format!("~{}", &path[home.len()..]),
            _ => path.to_string(),
        };
        let mut lines = vec![
            format!("name: {}", item.name.en),
            format!("category: {}", item.category.name()),
            format!("action: {}", if item.action == Action::Trash { "move to trash" } else { "delete" }),
        ];
        if let Some(app) = &item.app {
            lines.push(format!("app: {app}"));
        }
        lines.push(format!("folders: {}", item.roots.iter().take(4).map(|r| short(r)).collect::<Vec<_>>().join("; ")));
        if let Some(size) = item.size {
            lines.push(format!("size: {} MB in {} files", size / 1_000_000, item.files));
        }
        let largest: Vec<String> = item
            .entries
            .iter()
            .take(8)
            .map(|e| format!("{} ({} MB)", Path::new(&e.path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), e.size / 1_000_000))
            .collect();
        if !largest.is_empty() {
            lines.push(format!("largest entries: {}", largest.join(", ")));
        }
        if !found.filter.names.is_empty() {
            lines.push(format!("only names matching: {}", found.filter.names.join(", ")));
        }
        Some(lines.join("\n"))
    }

    /// 要管理员权限的项。
    pub fn is_admin(&self, id: &str) -> bool {
        self.item(id).is_some_and(|item| item.admin)
    }

    /// 界面上去掉的条目的路径（交给提权的进程，它按路径认，不按序号）。
    pub fn skipped_paths(&self, picks: &[Pick]) -> Vec<PathBuf> {
        picks
            .iter()
            .filter_map(|pick| self.found.iter().find(|f| f.item.id == pick.id).map(|f| (pick, f)))
            .flat_map(|(pick, f)| pick.skip.iter().filter_map(|i| f.targets.get(*i).map(|t| t.path.clone())))
            .collect()
    }

    /// 让这一项要关掉的程序正常退出，等它退出。返回更新后的这一项。
    pub fn quit_app(&mut self, id: &str) -> Option<CleanItem> {
        let found = self.found.iter_mut().find(|f| f.item.id == id)?;
        if !found.processes.is_empty() {
            special::quit(&found.processes, Duration::from_secs(10));
            found.item.running = special::any_running(&found.processes, &special::running_processes());
        }
        Some(found.item.clone())
    }
}

/// 一个要量的来源：规则、自定义文件夹、ShibaPack 自己没下完的文件。
struct Source {
    id: String,
    category: Category,
    risk: Risk,
    name: Text,
    about: Text,
    effect: Text,
    app: Option<String>,
    evidence: Option<String>,
    processes: Vec<String>,
    roots: Vec<PathBuf>,
    filter: Filter,
    action: Action,
    cookies: bool,
    command: Option<String>,
}

fn days(n: Option<u32>) -> Option<Duration> {
    n.map(|d| Duration::from_secs(u64::from(d) * 86_400))
}

fn sources(ctx: &Context) -> Vec<Source> {
    let mut out = Vec::new();
    for rule in rule::catalog() {
        let templates = rule.roots.get(ctx.os);
        if templates.is_empty() {
            continue;
        }
        let expanded: Vec<PathBuf> = templates.iter().flat_map(|t| vars::expand(t, &ctx.vars)).collect();
        // 按名字筛的只看根目录下第一层的那几个名字，根目录套着根目录也不会算两遍（浏览器配置和它下面的 Network）
        let roots = if rule.narrow() { dedup(expanded) } else { outermost(expanded) };
        out.push(Source {
            id: rule.id.clone(),
            category: rule.category,
            risk: rule.risk,
            name: rule.name.clone(),
            about: rule.about.clone(),
            effect: rule.effect.clone(),
            app: rule.app.clone(),
            evidence: Some(rule.evidence.clone()),
            processes: rule.processes.get(ctx.os).clone(),
            roots,
            filter: Filter {
                names: rule.names.clone(),
                keep: rule.keep.clone(),
                older_than: days(rule.older_than_days),
                own_only: rule.own_only,
                private: rule.category == Category::Privacy,
                admin: rule.admin,
                history_db: rule.history_db,
            },
            action: if rule.deletes() { Action::Delete } else { Action::Trash },
            cookies: rule.cookies,
            command: rule.command_for(ctx.os).map(str::to_string),
        });
    }
    if let Some(dir) = ctx.repo_downloads.as_ref().filter(|dir| vars::real_dir(dir)) {
        out.push(Source {
            id: "installers.shibapack-partial".into(),
            category: Category::Installers,
            risk: Risk::Safe,
            name: Text::new("ShibaPack 没下完的文件", "ShibaPack unfinished downloads"),
            about: Text::new(
                "在 ShibaPack 里下载安装包时中断或取消留下的半截文件，本来是留着从断点接着下的。这里只算 1 天里都没动过的。",
                "Partial files left when a download in ShibaPack was interrupted or cancelled, kept for resuming. Only files untouched for a day are counted.",
            ),
            effect: Text::new("再下载这个安装包时要从头下。", "Downloading that installer again starts from the beginning."),
            app: Some("ShibaPack".into()),
            evidence: None,
            processes: Vec::new(),
            roots: vec![dir.clone()],
            filter: Filter { names: vec!["*.part".into(), "*.part.etag".into()], older_than: days(Some(1)), ..Filter::default() },
            action: Action::Delete,
            cookies: false,
            command: None,
        });
    }
    for (i, custom) in ctx.options.all_rules().iter().enumerate() {
        if ctx.guard.custom_root(&custom.path).is_err() {
            continue;
        }
        let whole = custom.names.is_empty() && custom.older_than_days.is_none();
        out.push(Source {
            id: format!("custom.{i}"),
            category: Category::Custom,
            risk: Risk::Recoverable,
            name: Text::new(&custom.name, &custom.name),
            about: if whole {
                Text::new("你自己加的清理规则，这个文件夹里的东西都会清掉。", "A rule you added. Everything inside this folder is cleaned.")
            } else {
                Text::new("你自己加的清理规则，只清这个文件夹里符合条件的。", "A rule you added. Only matching items in this folder are cleaned.")
            },
            effect: if custom.delete {
                Text::new("直接删除，不进回收站，删了找不回来。", "Deleted directly, skipping the Recycle Bin/Trash. It cannot be recovered.")
            } else {
                Text::new("移到回收站，需要时可以从回收站找回来。", "Moved to the Recycle Bin/Trash, where you can restore it if needed.")
            },
            app: None,
            evidence: None,
            processes: custom.processes.clone(),
            roots: vec![custom.path.clone()],
            filter: Filter { names: custom.names.clone(), older_than: days(custom.older_than_days), ..Filter::default() },
            action: if custom.delete { Action::Delete } else { Action::Trash },
            cookies: false,
            command: None,
        });
    }
    out
}

/// 去掉在别的根目录里面的根目录（`*` 可能匹配到另一个根目录下面），免得同一批文件算两遍。
fn dedup(mut roots: Vec<PathBuf>) -> Vec<PathBuf> {
    roots.sort();
    roots.dedup();
    roots
}

fn outermost(mut roots: Vec<PathBuf>) -> Vec<PathBuf> {
    roots.sort();
    roots.dedup();
    roots.iter().filter(|root| !roots.iter().any(|other| other != *root && root.starts_with(other))).cloned().collect()
}

fn millis(time: Option<SystemTime>) -> Option<i64> {
    time.and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()).and_then(|d| i64::try_from(d.as_millis()).ok())
}

/// 当前用户的 id：用户文件夹的主人。
#[cfg(unix)]
fn owner(path: &Path) -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(path).ok().map(|m| m.uid())
}

#[cfg(not(unix))]
fn owner(_path: &Path) -> Option<u32> {
    None
}

/// 这一条现在还能不能清：过保护检查、不在排除列表里、属于当前用户、最近没动过。不行时返回原因。
fn still_ok(ctx: &Context, filter: &Filter, root: &Path, path: &Path, me: Option<u32>, now: SystemTime) -> Result<measure::Measured, &'static str> {
    if ctx.guard.excluded(path, &ctx.options.exclude) {
        return Err("excluded");
    }
    ctx.guard.entry_check(root, path, filter.scope())?;
    if filter.own_only && me.is_some() && owner(path) != me {
        return Err("not_allowed");
    }
    let measured = measure::measure(path, &mut HashSet::new());
    if measured.in_use {
        return Err("in_use");
    }
    if let Some(age) = filter.older_than {
        let fresh = measured.newest.is_none_or(|t| now.duration_since(t).map(|d| d < age).unwrap_or(true));
        if fresh {
            return Err("changed");
        }
    }
    Ok(measured)
}

fn measure_source(source: &Source, ctx: &Context, me: Option<u32>, now: SystemTime) -> Option<Found> {
    let scope = source.filter.scope();
    let mut seen = HashSet::new();
    let mut targets: Vec<(Target, measure::Measured)> = Vec::new();
    let mut unreadable = false;
    let mut roots = Vec::new();
    for root in &source.roots {
        if !ctx.guard.root_ok(root, scope) {
            continue;
        }
        roots.push(root.to_string_lossy().to_string());
        let Ok(read) = std::fs::read_dir(root) else {
            unreadable = true;
            continue;
        };
        let mut children: Vec<PathBuf> = read.flatten().map(|e| e.path()).collect();
        children.sort();
        for path in children {
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if !source.filter.wants(&name) || ctx.guard.excluded(&path, &ctx.options.exclude) || ctx.guard.entry_check(root, &path, scope).is_err() {
                continue;
            }
            if source.filter.own_only && me.is_some() && owner(&path) != me {
                continue;
            }
            let measured = measure::measure(&path, &mut seen);
            if measured.in_use {
                continue;
            }
            if let Some(age) = source.filter.older_than {
                if measured.newest.is_none_or(|t| now.duration_since(t).map(|d| d < age).unwrap_or(true)) {
                    continue;
                }
            }
            unreadable |= measured.unreadable;
            targets.push((Target { path, root: root.clone(), size: measured.size }, measured));
        }
    }
    // 要管理员权限的：普通权限读不到时照样列出来，大小写“未知”，提权以后再量
    let blind = source.filter.admin && unreadable && targets.is_empty();
    if targets.is_empty() && !blind {
        return None;
    }
    targets.sort_by(|a, b| b.0.size.cmp(&a.0.size).then_with(|| a.0.path.cmp(&b.0.path)));
    let entries = targets
        .iter()
        .take(MAX_ENTRIES)
        .enumerate()
        .map(|(index, (target, m))| Entry {
            index,
            path: target.path.to_string_lossy().to_string(),
            size: target.size,
            files: m.files,
            modified: millis(m.newest),
            library: None,
        })
        .collect();
    let note = if blind {
        Some("needs_admin")
    } else if unreadable {
        Some("unreadable")
    } else {
        None
    };
    let item = CleanItem {
        id: source.id.clone(),
        category: source.category,
        risk: source.risk,
        action: source.action,
        name: source.name.clone(),
        about: source.about.clone(),
        effect: source.effect.clone(),
        app: source.app.clone(),
        evidence: source.evidence.clone(),
        roots,
        size: (!blind).then(|| targets.iter().map(|(t, _)| t.size).sum()),
        files: targets.iter().map(|(_, m)| m.files).sum(),
        entries,
        entry_count: targets.len(),
        modified: millis(targets.iter().filter_map(|(_, m)| m.newest).max()),
        running: false,
        can_quit: !source.processes.is_empty(),
        note: note.map(str::to_string),
        importable: false,
        admin: source.filter.admin,
        cookies: source.cookies,
        tool: None,
    };
    Some(Found {
        item,
        targets: targets.into_iter().map(|(t, _)| t).collect(),
        processes: source.processes.clone(),
        filter: source.filter.clone(),
        command: source.command.clone(),
    })
}

/// 一项不在规则里、也没有文件可列的东西（回收站、剪贴板、Windows.old）。
fn plain_item(id: &str, category: Category, risk: Risk, action: Action, name: Text, about: Text, effect: Text) -> CleanItem {
    CleanItem {
        id: id.into(),
        category,
        risk,
        action,
        name,
        about,
        effect,
        app: None,
        evidence: None,
        roots: Vec::new(),
        size: None,
        files: 0,
        entries: Vec::new(),
        entry_count: 0,
        modified: None,
        running: false,
        can_quit: false,
        note: None,
        importable: false,
        admin: false,
        cookies: false,
        tool: None,
    }
}

fn plain_found(item: CleanItem) -> Found {
    Found { item, targets: Vec::new(), processes: Vec::new(), filter: Filter::default(), command: None }
}

/// 剪贴板：这个系统上清得了才列。
fn clipboard_found() -> Option<Found> {
    if !special::clipboard_supported() {
        return None;
    }
    let item = plain_item(
        "privacy.clipboard",
        Category::Privacy,
        Risk::Recoverable,
        Action::ClearClipboard,
        Text::new("剪贴板", "Clipboard"),
        Text::new("最近一次复制的内容。可能是密码、验证码或者别的不想留下的东西。", "Whatever you copied last. It may be a password, a code or something else you'd rather not leave behind."),
        Text::new("剪贴板清空，粘贴不出东西。", "The clipboard is emptied; there is nothing to paste."),
    );
    Some(plain_found(item))
}

/// Windows.old：升级系统后留下的旧系统。这里不删，交给系统设置里的“临时文件”清理。
fn windows_old_found(ctx: &Context) -> Option<Found> {
    if ctx.os != Os::Windows {
        return None;
    }
    let path = ctx.vars.get("system_drive")?.join("Windows.old");
    if !vars::real_dir(&path) {
        return None;
    }
    let mut item = plain_item(
        "system.windows-old",
        Category::System,
        Risk::High,
        Action::Delete,
        Text::new("以前的 Windows 安装", "Previous Windows installation"),
        Text::new(
            "升级 Windows 以后留下的旧系统（Windows.old），常常有十几 GB。删了就不能退回到以前的版本。",
            "The old system (Windows.old) left after a Windows upgrade, often over 10 GB. Once removed, you can't go back to the previous version.",
        ),
        Text::new(
            "要在系统设置的“临时文件”里勾选“以前的 Windows 安装”来删，ShibaPack 不直接删它。",
            "Remove it in Settings under Temporary files (Previous Windows installation). ShibaPack doesn't delete it directly.",
        ),
    );
    item.roots = vec![path.to_string_lossy().to_string()];
    item.tool = Some("storage".into());
    item.note = Some("system_tool".into());
    item.evidence = Some("https://support.microsoft.com/en-us/windows/manage-drive-space-with-storage-sense-654f6ada-7bfc-45e5-966b-e24aded96ad5".into());
    Some(plain_found(item))
}

/// 回收站一项。空的时候不列。
fn trash_found(ctx: &Context) -> Option<Found> {
    let home = ctx.vars.get("home")?;
    let info = special::trash_info(home, ctx.vars.get("xdg_data"), ctx.ask_finder)?;
    if info.size == Some(0) && info.items == 0 {
        return None;
    }
    let note = info.size.is_none().then(|| if ctx.os == Os::Macos { "full_disk_access" } else { "unreadable" }.to_string());
    let mut item = plain_item(
        "trash.empty",
        Category::Trash,
        Risk::High,
        Action::EmptyTrash,
        Text::new("回收站", "Recycle Bin / Trash"),
        Text::new("已经删除、还放在回收站（废纸篓）里的文件。", "Files you deleted that are still in the Recycle Bin or Trash."),
        Text::new("清空以后就找不回来了。", "Once emptied, these files cannot be recovered."),
    );
    item.size = info.size;
    item.files = info.items;
    item.entry_count = info.items as usize;
    item.note = note;
    Some(plain_found(item))
}

/// 下载文件夹和桌面上的安装包：和仓库里一模一样的一项，还没收进仓库的一项。
fn loose_found(ctx: &Context) -> Vec<Found> {
    let dirs: Vec<PathBuf> = ["downloads", "desktop"].iter().filter_map(|n| ctx.vars.get(n)).filter(|d| vars::real_dir(d)).map(Path::to_path_buf).collect();
    let mut hashes = ctx.cache_dir.as_deref().map(special::HashCache::load).unwrap_or_default();
    let all = special::loose_installers(&dirs, &ctx.library, &mut hashes);
    if let Some(dir) = &ctx.cache_dir {
        hashes.save(dir);
    }
    let loose: Vec<special::Loose> = all
        .into_iter()
        .filter(|l| !ctx.guard.excluded(&l.path, &ctx.options.exclude) && ctx.guard.entry_check(&l.root, &l.path, Scope::narrow()).is_ok())
        .collect();
    let narrow = Filter { names: vec!["*".into()], ..Filter::default() };
    let mut out = Vec::new();
    for same in [true, false] {
        let mut group: Vec<&special::Loose> = loose.iter().filter(|l| l.same_as.is_some() == same).collect();
        if group.is_empty() {
            continue;
        }
        group.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.path.cmp(&b.path)));
        let entries = group
            .iter()
            .take(MAX_ENTRIES)
            .enumerate()
            .map(|(index, l)| Entry {
                index,
                path: l.path.to_string_lossy().to_string(),
                size: l.size,
                files: 1,
                modified: millis(l.modified),
                library: match (&l.same_as, &l.known) {
                    (Some((software, version)), _) => Some(LibraryMatch { software: software.clone(), versions: vec![version.clone()], same: true }),
                    (None, Some((software, versions))) => Some(LibraryMatch { software: software.clone(), versions: versions.clone(), same: false }),
                    _ => None,
                },
            })
            .collect();
        let mut roots: Vec<String> = dirs.iter().map(|d| d.to_string_lossy().to_string()).collect();
        roots.dedup();
        let (id, name, about, effect) = if same {
            (
                "installers.in-library",
                Text::new("仓库里已有的安装包", "Installers already in your library"),
                Text::new(
                    "下载文件夹和桌面上的这些安装包，和仓库里的文件一模一样（SHA-256 相同）。",
                    "These installers in Downloads and on the Desktop are identical to files in your library (same SHA-256).",
                ),
                Text::new("移到回收站。仓库里的那一份不受影响。", "Moved to the Recycle Bin/Trash. The copy in your library is not affected."),
            )
        } else {
            (
                "installers.not-in-library",
                Text::new("没收进仓库的安装包", "Installers not in your library"),
                Text::new(
                    "下载文件夹和桌面上还没收进仓库的安装包。可以先导入仓库，再清理这里的原件。",
                    "Installers in Downloads and on the Desktop that are not in your library yet. You can import them first, then clean up the originals.",
                ),
                Text::new("移到回收站。没导入仓库的话，以后要用得重新下载。", "Moved to the Recycle Bin/Trash. If you don't import them, you'll need to download them again later."),
            )
        };
        let item = CleanItem {
            id: id.into(),
            category: Category::Installers,
            risk: Risk::Recoverable,
            action: Action::Trash,
            name,
            about,
            effect,
            app: None,
            evidence: None,
            roots,
            size: Some(group.iter().map(|l| l.size).sum()),
            files: group.len() as u64,
            entries,
            entry_count: group.len(),
            modified: millis(group.iter().filter_map(|l| l.modified).max()),
            running: false,
            can_quit: false,
            note: None,
            importable: !same,
            admin: false,
            cookies: false,
            tool: None,
        };
        let targets = group.iter().map(|l| Target { path: l.path.clone(), root: l.root.clone(), size: l.size }).collect();
        out.push(Found { item, targets, processes: Vec::new(), filter: narrow.clone(), command: None });
    }
    out
}

/// 只读地量一遍。每量完一项调用一次 `on_item`（可能在别的线程里）。没有东西可清的项不列。
pub fn scan(ctx: &Context, on_item: &(dyn Fn(&CleanItem) + Sync)) -> Scan {
    scan_at(ctx, on_item, SystemTime::now())
}

/// 和 `scan` 一样，另外报进度（`on_progress(量完几处, 一共几处)`），并且可以取消：
/// `cancel` 变成真以后不再量新的，已经量完的照样返回（结果不完整）。
pub fn scan_with(ctx: &Context, on_item: &(dyn Fn(&CleanItem) + Sync), on_progress: &(dyn Fn(usize, usize) + Sync), cancel: &AtomicBool) -> Scan {
    scan_full(ctx, on_item, on_progress, cancel, SystemTime::now())
}

/// `now` 是“现在”：测试里拿它代替改文件时间（Unix 上改修改时间会同时更新状态改变时间，改不旧）。
fn scan_at(ctx: &Context, on_item: &(dyn Fn(&CleanItem) + Sync), now: SystemTime) -> Scan {
    scan_full(ctx, on_item, &|_, _| {}, &AtomicBool::new(false), now)
}

fn scan_full(ctx: &Context, on_item: &(dyn Fn(&CleanItem) + Sync), on_progress: &(dyn Fn(usize, usize) + Sync), cancel: &AtomicBool, now: SystemTime) -> Scan {
    enum Task<'a> {
        Source(&'a Source),
        Loose,
        Trash,
        Clipboard,
        WindowsOld,
    }
    let running = special::running_processes();
    let me = ctx.vars.get("home").and_then(owner);
    let list = sources(ctx);
    let mut tasks: Vec<Task> = list.iter().map(Task::Source).collect();
    tasks.push(Task::Loose);
    tasks.push(Task::Trash);
    tasks.push(Task::Clipboard);
    tasks.push(Task::WindowsOld);
    let next = AtomicUsize::new(0);
    let finished = AtomicUsize::new(0);
    on_progress(0, tasks.len());
    let results: Mutex<Vec<(usize, Found)>> = Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).clamp(1, 6);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some(task) = tasks.get(i) else { break };
                let found = match task {
                    Task::Source(source) => measure_source(source, ctx, me, now).into_iter().collect(),
                    Task::Loose => loose_found(ctx),
                    Task::Trash => trash_found(ctx).into_iter().collect::<Vec<_>>(),
                    Task::Clipboard => clipboard_found().into_iter().collect(),
                    Task::WindowsOld => windows_old_found(ctx).into_iter().collect(),
                };
                for mut f in found {
                    f.item.running = special::any_running(&f.processes, &running);
                    on_item(&f.item);
                    results.lock().unwrap_or_else(|e| e.into_inner()).push((i, f));
                }
                on_progress(finished.fetch_add(1, Ordering::SeqCst) + 1, tasks.len());
            });
        }
    });
    let mut results = results.into_inner().unwrap_or_else(|e| e.into_inner());
    results.sort_by_key(|(i, _)| *i);
    Scan { found: results.into_iter().map(|(_, f)| f).collect() }
}

/// 要清的一项：id，和不想清的条目序号。
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pick {
    pub id: String,
    #[serde(default)]
    pub skip: Vec<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// 直接删除腾出的空间。
    pub freed: u64,
    /// 移到回收站的大小（清空回收站之前不腾空间）。
    pub trashed: u64,
    /// 清掉了几条。
    pub removed: usize,
    pub items: Vec<ItemReport>,
    /// 跳过的，最多 200 条；一共几条看 skippedCount。
    pub skipped: Vec<Skipped>,
    pub skipped_count: usize,
    /// 这次的清理记录。
    pub history: Option<String>,
    /// 清理前后，用户文件夹所在的盘系统报告的可用空间。量不出来时是 None。
    #[serde(default)]
    pub disk_before: Option<u64>,
    #[serde(default)]
    pub disk_after: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemReport {
    pub id: String,
    pub freed: u64,
    pub trashed: u64,
    pub removed: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skipped {
    pub item: String,
    pub path: Option<String>,
    /// `app_running`、`in_use`、`access_denied`、`changed`（最近动过）、`excluded`、`not_allowed`、`missing`，
    /// 或者系统给的原话。
    pub reason: String,
}

/// 清理勾选的项。`progress(做完几条, 一共几条)`。返回结果和要写进清理记录的内容。
pub fn clean(scan: &Scan, picks: &[Pick], ctx: &Context, progress: &dyn Fn(usize, usize)) -> (Report, history::Run) {
    clean_at(scan, picks, ctx, progress, SystemTime::now())
}

fn clean_at(scan: &Scan, picks: &[Pick], ctx: &Context, progress: &dyn Fn(usize, usize), now: SystemTime) -> (Report, history::Run) {
    let running = special::running_processes();
    let me = ctx.vars.get("home").and_then(owner);
    let mut report = Report::default();
    let mut run = history::Run { at: crate::now(), ..Default::default() };
    let total: usize = picks
        .iter()
        .filter_map(|p| scan.found.iter().find(|f| f.item.id == p.id))
        .map(|f| if single_step(f, ctx) { 1 } else { f.targets.len() })
        .sum();
    let mut done = 0;
    let skip = |report: &mut Report, item: &str, path: Option<&Path>, reason: &str| {
        report.skipped_count += 1;
        if report.skipped.len() < 200 {
            report.skipped.push(Skipped { item: item.into(), path: path.map(|p| p.to_string_lossy().to_string()), reason: reason.into() });
        }
    };
    for pick in picks {
        let Some(found) = scan.found.iter().find(|f| f.item.id == pick.id) else {
            skip(&mut report, &pick.id, None, "missing");
            continue;
        };
        let mut item_report = ItemReport { id: pick.id.clone(), ..Default::default() };
        let mut run_item = history::RunItem { id: pick.id.clone(), name: found.item.name.clone(), ..Default::default() };
        // 这里不清的：要到系统设置里清的、要管理员权限而这里不是提权的进程
        let refused = if found.item.tool.is_some() {
            Some("not_supported")
        } else if found.item.admin && !ctx.elevated {
            Some("needs_admin")
        } else {
            None
        };
        if let Some(reason) = refused {
            skip(&mut report, &pick.id, None, reason);
            item_report.skipped += 1;
            done += if single_step(found, ctx) { 1 } else { found.targets.len() };
            progress(done, total);
        } else if special::any_running(&found.processes, &running) {
            skip(&mut report, &pick.id, None, "app_running");
            item_report.skipped += 1;
            done += found.targets.len().max(1);
            progress(done, total);
        } else if found.item.action == Action::EmptyTrash {
            let home = ctx.vars.get("home").unwrap_or(Path::new("/"));
            match special::empty_trash(home, ctx.vars.get("xdg_data")) {
                Ok(()) => {
                    item_report.freed = found.item.size.unwrap_or(0);
                    item_report.removed = found.item.entry_count;
                }
                Err(reason) => {
                    skip(&mut report, &pick.id, None, &reason);
                    item_report.skipped += 1;
                }
            }
            done += 1;
            progress(done, total);
        } else if found.item.action == Action::ClearClipboard {
            match special::clear_clipboard() {
                Ok(()) => item_report.removed = 1,
                Err(reason) => {
                    skip(&mut report, &pick.id, None, &reason);
                    item_report.skipped += 1;
                }
            }
            done += 1;
            progress(done, total);
        } else if let Some(command) = &found.command {
            // 用系统自带的命令清（传递优化缓存）：腾出多少按清之前和之后的大小算
            let size = |f: &Found| f.item.roots.iter().map(|r| measure::measure(Path::new(r), &mut HashSet::new()).size).sum::<u64>();
            let before = size(found);
            match special::run_command(command) {
                Ok(()) => {
                    item_report.freed = before.saturating_sub(size(found));
                    item_report.removed = 1;
                }
                Err(reason) => {
                    skip(&mut report, &pick.id, None, &reason);
                    item_report.skipped += 1;
                }
            }
            done += 1;
            progress(done, total);
        } else {
            // Cookie 有保留名单：只删名单以外网站的 Cookie，数据库文件留着；日志文件（-journal、-wal、-shm）不动
            let prune = found.item.cookies && !ctx.options.keep_cookies.is_empty();
            let history_db = found.filter.history_db;
            for (index, target) in found.targets.iter().enumerate() {
                done += 1;
                if pick.skip.contains(&index) {
                    progress(done, total);
                    continue;
                }
                if let Err(reason) = still_ok(ctx, &found.filter, &target.root, &target.path, me, now) {
                    skip(&mut report, &pick.id, Some(&target.path), reason);
                    item_report.skipped += 1;
                    progress(done, total);
                    continue;
                }
                if history_db {
                    // Firefox 的 places.sqlite：只删里面的浏览记录，文件任何时候都不删
                    match special::prune_firefox_history(&target.path) {
                        Ok(freed) => {
                            item_report.freed += freed;
                            item_report.removed += 1;
                            run_item.push_path(&target.path);
                        }
                        Err(reason) => {
                            skip(&mut report, &pick.id, Some(&target.path), &reason);
                            item_report.skipped += 1;
                        }
                    }
                    progress(done, total);
                    continue;
                }
                if prune {
                    if special::is_cookie_db(&target.path) {
                        match special::prune_cookies(&target.path, &ctx.options.keep_cookies) {
                            Ok(freed) => {
                                item_report.freed += freed;
                                item_report.removed += 1;
                                run_item.push_path(&target.path);
                            }
                            Err(reason) => {
                                skip(&mut report, &pick.id, Some(&target.path), &reason);
                                item_report.skipped += 1;
                            }
                        }
                    }
                    progress(done, total);
                    continue;
                }
                match found.item.action {
                    Action::Delete => {
                        let removed = measure::remove_tree(&target.path);
                        item_report.freed += removed.freed;
                        if removed.failed > 0 {
                            skip(&mut report, &pick.id, Some(&target.path), if removed.denied { "access_denied" } else { "in_use" });
                            item_report.skipped += 1;
                        } else {
                            item_report.removed += 1;
                        }
                        if removed.freed > 0 || removed.failed == 0 {
                            run_item.push_path(&target.path);
                        }
                    }
                    Action::Trash => match trash::delete(&target.path) {
                        Ok(()) => {
                            item_report.trashed += target.size;
                            item_report.removed += 1;
                            run_item.push_path(&target.path);
                        }
                        Err(err) => {
                            let text = err.to_string();
                            let lower = text.to_lowercase();
                            let reason = if lower.contains("denied") || lower.contains("permission") || text.contains("0x80070005") {
                                "access_denied".to_string()
                            } else if lower.contains("in use") || lower.contains("being used") || text.contains("0x80070020") {
                                "in_use".to_string()
                            } else {
                                text
                            };
                            skip(&mut report, &pick.id, Some(&target.path), &reason);
                            item_report.skipped += 1;
                        }
                    },
                    Action::EmptyTrash | Action::ClearClipboard => {}
                }
                progress(done, total);
            }
        }
        report.freed += item_report.freed;
        report.trashed += item_report.trashed;
        report.removed += item_report.removed;
        run_item.freed = item_report.freed;
        run_item.trashed = item_report.trashed;
        run_item.removed = item_report.removed;
        if item_report.removed > 0 || item_report.freed > 0 {
            run.items.push(run_item);
        }
        report.items.push(item_report);
    }
    run.freed = report.freed;
    run.trashed = report.trashed;
    run.skipped = report.skipped_count;
    (report, run)
}

impl Report {
    /// 把提权的进程做的那部分并进来。
    pub fn merge(&mut self, other: Report) {
        self.freed += other.freed;
        self.trashed += other.trashed;
        self.removed += other.removed;
        self.items.extend(other.items);
        self.skipped_count += other.skipped_count;
        for skipped in other.skipped {
            if self.skipped.len() < 200 {
                self.skipped.push(skipped);
            }
        }
    }

    /// 这些项一条都没清，原因都一样（比如在授权提示里点了取消）。
    pub fn refuse(&mut self, ids: &[String], reason: &str) {
        for id in ids {
            self.skipped_count += 1;
            if self.skipped.len() < 200 {
                self.skipped.push(Skipped { item: id.clone(), path: None, reason: reason.into() });
            }
            self.items.push(ItemReport { id: id.clone(), skipped: 1, ..Default::default() });
        }
    }
}

/// 保留名单的候选：浏览器里有 Cookie 的网站。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CookieSite {
    pub site: String,
    pub count: u64,
}

/// 这台电脑上的浏览器里，哪些网站有 Cookie（按主域名合并，多的在前，最多 60 个）。只读，先复制一份再读。
pub fn cookie_sites(ctx: &Context) -> Vec<CookieSite> {
    let dbs: Vec<PathBuf> = sources(ctx)
        .iter()
        .filter(|source| source.cookies)
        .flat_map(|source| source.roots.iter().flat_map(|root| ["Cookies", "cookies.sqlite"].map(|name| root.join(name))))
        .filter(|path| path.is_file())
        .collect();
    special::cookie_sites(&dbs).into_iter().take(60).map(|(site, count)| CookieSite { site, count }).collect()
}

/// 一下子做完、不按条目算进度的项。
fn single_step(found: &Found, ctx: &Context) -> bool {
    matches!(found.item.action, Action::EmptyTrash | Action::ClearClipboard)
        || found.command.is_some()
        || found.item.tool.is_some()
        || (found.item.admin && !ctx.elevated)
        || found.targets.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// 这么多天以后。
    fn later(days: u64) -> SystemTime {
        SystemTime::now() + Duration::from_secs(days * 86_400)
    }

    fn context(home: &Path, options: Options) -> Context {
        let vars = Vars::with(&[
            ("home", home),
            ("temp", home.join("tmp").as_path()),
            ("user_cache", home.join("Library/Caches").as_path()),
            ("app_support", home.join("Library/Application Support").as_path()),
            ("user_logs", home.join("Library/Logs").as_path()),
            ("user_library", home.join("Library").as_path()),
            ("local_app_data", home.join("AppData/Local").as_path()),
            ("roaming_app_data", home.join("AppData/Roaming").as_path()),
            ("xdg_cache", home.join(".cache").as_path()),
            ("xdg_config", home.join(".config").as_path()),
            ("xdg_data", home.join(".local/share").as_path()),
            ("downloads", home.join("Downloads").as_path()),
            ("desktop", home.join("Desktop").as_path()),
        ]);
        let repo = home.join("Repo");
        let guard = Guard::new(&vars, Some(&repo), Some(&repo.join(".shibapack/downloads")));
        Context::with(vars, guard, options, Vec::new(), Some(repo.join(".shibapack/downloads")))
    }

    fn pip_dir(home: &Path) -> PathBuf {
        match Os::current() {
            Os::Macos => home.join("Library/Caches/pip"),
            Os::Windows => home.join("AppData/Local/pip/Cache"),
            Os::Linux => home.join(".cache/pip"),
        }
    }

    #[test]
    fn scans_and_cleans_a_cache_rule() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let pip = pip_dir(&home);
        fs::create_dir_all(pip.join("http/a")).unwrap();
        fs::write(pip.join("http/a/blob"), vec![7u8; 10_000]).unwrap();
        fs::write(pip.join("selfcheck.json"), b"{}").unwrap();
        let ctx = context(&home, Options::default());
        let scan = scan(&ctx, &|_| {});
        let item = scan.item("development.pip").expect("应该找到 pip 缓存").clone();
        assert_eq!(item.action, Action::Delete);
        assert_eq!(item.entry_count, 2);
        assert_eq!(item.entries[0].path, pip.join("http").to_string_lossy());
        assert!(item.size.unwrap() >= 10_000);
        // 不清 selfcheck.json（序号 1）
        let (report, run) = clean(&scan, &[Pick { id: item.id.clone(), skip: vec![1] }], &ctx, &|_, _| {});
        assert_eq!(report.removed, 1);
        assert!(report.freed >= 10_000);
        assert!(!pip.join("http").exists());
        assert!(pip.join("selfcheck.json").exists());
        assert!(pip.exists(), "根目录本身留着");
        assert_eq!(run.items.len(), 1);
    }

    #[test]
    fn temp_rule_only_takes_old_unused_entries() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let temp = home.join("tmp");
        fs::create_dir_all(temp.join("some-dir")).unwrap();
        fs::write(temp.join("some-dir/x"), b"x").unwrap();
        fs::write(temp.join("file.bin"), b"y").unwrap();
        fs::write(temp.join("app.lock"), b"z").unwrap();
        let ctx = context(&home, Options::default());
        let names = |scan: &Scan| -> Vec<String> {
            scan.item("system.temp")
                .map(|i| i.entries.iter().map(|e| Path::new(&e.path).file_name().unwrap().to_string_lossy().to_string()).collect())
                .unwrap_or_default()
        };
        // 刚放进去的不算
        assert!(names(&scan_at(&ctx, &|_| {}, SystemTime::now())).is_empty());
        // 8 天以后：都算，keep 里的锁文件除外
        let found = scan_at(&ctx, &|_| {}, later(8));
        let mut got = names(&found);
        got.sort();
        assert_eq!(got, vec!["file.bin".to_string(), "some-dir".to_string()]);
        assert!(!names(&found).contains(&"app.lock".to_string()));
        // 清理时再看一遍：这时又“刚动过”了的不清
        let (report, _) = clean_at(&found, &[Pick { id: "system.temp".into(), skip: vec![] }], &ctx, &|_, _| {}, SystemTime::now());
        assert_eq!(report.removed, 0);
        assert!(report.skipped.iter().all(|s| s.reason == "changed"));
        let (report, _) = clean_at(&found, &[Pick { id: "system.temp".into(), skip: vec![] }], &ctx, &|_, _| {}, later(8));
        assert_eq!(report.removed, 2);
        assert!(temp.join("app.lock").exists());
    }

    #[test]
    fn nested_roots_are_counted_once() {
        let roots = outermost(vec![PathBuf::from("/a/GPU/GPUCache"), PathBuf::from("/a/GPU"), PathBuf::from("/a/GPUCache"), PathBuf::from("/a/GPU")]);
        assert_eq!(roots, vec![PathBuf::from("/a/GPU"), PathBuf::from("/a/GPUCache")]);
    }

    #[test]
    fn running_apps_block_their_items() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let pip = pip_dir(&home);
        fs::create_dir_all(&pip).unwrap();
        fs::write(pip.join("blob"), b"x").unwrap();
        let ctx = context(&home, Options::default());
        let mut found = scan(&ctx, &|_| {});
        // 假装 pip 规则要关掉一个正在运行的程序：用测试进程自己的名字
        let me = std::env::current_exe().unwrap().file_name().unwrap().to_string_lossy().to_string();
        let me = if cfg!(target_os = "linux") { me.chars().take(15).collect() } else { me };
        for f in &mut found.found {
            if f.item.id == "development.pip" {
                f.processes = vec![me.clone()];
            }
        }
        let (report, _) = clean(&found, &[Pick { id: "development.pip".into(), skip: vec![] }], &ctx, &|_, _| {});
        assert_eq!(report.removed, 0);
        assert_eq!(report.skipped[0].reason, "app_running");
        assert!(pip.join("blob").exists());
    }

    #[test]
    fn recheck_refuses_entries_excluded_after_the_scan() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let pip = pip_dir(&home);
        fs::create_dir_all(pip.join("keepme")).unwrap();
        fs::write(pip.join("keepme/blob"), b"x").unwrap();
        let found = scan(&context(&home, Options::default()), &|_| {});
        let ctx = context(&home, Options { exclude: vec![pip.join("keepme")], ..Options::default() });
        let (report, _) = clean(&found, &[Pick { id: "development.pip".into(), skip: vec![] }], &ctx, &|_, _| {});
        assert_eq!(report.removed, 0);
        assert_eq!(report.skipped[0].reason, "excluded");
        assert!(pip.join("keepme/blob").exists());
    }

    #[test]
    fn custom_folders_go_through_the_guard() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let mine = home.join("Work/scratch");
        fs::create_dir_all(mine.join("build")).unwrap();
        fs::create_dir_all(mine.join("repo/.git")).unwrap();
        fs::write(mine.join("build/out.o"), b"o").unwrap();
        fs::create_dir_all(home.join("Downloads")).unwrap();
        let ctx = context(&home, Options::default());
        assert!(check_options(&ctx.guard, Options { custom: vec![home.join("Downloads")], ..Options::default() }).unwrap_err().starts_with("cleanup_protected:"));
        let options = check_options(&ctx.guard, Options { custom: vec![mine.clone(), mine.clone()], ..Options::default() }).unwrap();
        assert!(options.custom.is_empty(), "旧的文件夹并进规则里");
        assert_eq!(options.rules.len(), 1);
        assert_eq!(options.rules[0].name, "scratch");
        let ctx = context(&home, options);
        let found = scan(&ctx, &|_| {});
        let item = found.item("custom.0").expect("自定义文件夹");
        assert_eq!(item.action, Action::Trash);
        // 里面有 .git 的文件夹不算
        assert_eq!(item.entry_count, 1);
        assert!(item.entries[0].path.ends_with("build"));
    }

    #[test]
    fn shibapack_partial_downloads_inside_the_repo() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let downloads = home.join("Repo/.shibapack/downloads");
        fs::create_dir_all(&downloads).unwrap();
        fs::write(downloads.join("abcd.part"), b"half").unwrap();
        fs::write(downloads.join("abcd.part.etag"), b"etag").unwrap();
        fs::write(downloads.join("other.bin"), b"keep").unwrap();
        let ctx = context(&home, Options::default());
        assert!(scan(&ctx, &|_| {}).item("installers.shibapack-partial").is_none(), "正在下的不算");
        let found = scan_at(&ctx, &|_| {}, later(2));
        let item = found.item("installers.shibapack-partial").expect("ShibaPack 没下完的文件");
        assert_eq!(item.entry_count, 2);
        let (report, _) = clean_at(&found, &[Pick { id: item.id.clone(), skip: vec![] }], &ctx, &|_, _| {}, later(2));
        assert_eq!(report.removed, 2);
        assert!(downloads.join("other.bin").exists());
    }

    #[test]
    fn custom_rules_filter_and_rescan() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let logs = home.join("Work/app/logs");
        fs::create_dir_all(&logs).unwrap();
        fs::write(logs.join("a.log"), vec![1u8; 5000]).unwrap();
        fs::write(logs.join("b.log"), vec![1u8; 100]).unwrap();
        fs::write(logs.join("keep.db"), b"data").unwrap();
        let ctx = context(&home, Options::default());
        assert!(check_options(&ctx.guard, Options { rules: vec![CustomRule { name: "x".into(), path: logs.clone(), names: vec!["*".into()], ..Default::default() }], ..Options::default() })
            .unwrap_err()
            .starts_with("cleanup_bad_pattern:"));
        let rule = CustomRule { name: " 应用日志 ".into(), path: logs.clone(), names: vec!["*.log".into()], delete: true, ..Default::default() };
        let options = check_options(&ctx.guard, Options { rules: vec![rule], ..Options::default() }).unwrap();
        assert_eq!(options.rules[0].name, "应用日志");
        let ctx = context(&home, options);
        let mut found = scan(&ctx, &|_| {});
        let item = found.item("custom.0").expect("自定义规则").clone();
        assert_eq!(item.action, Action::Delete);
        assert_eq!(item.name.zh, "应用日志");
        assert_eq!(item.entry_count, 2, "只清 *.log");
        // 清掉一条以后只重新量这一项
        let (report, _) = clean(&found, &[Pick { id: item.id.clone(), skip: vec![1] }], &ctx, &|_, _| {});
        assert_eq!(report.removed, 1);
        found.rescan(&ctx, std::slice::from_ref(&item.id));
        assert_eq!(found.item("custom.0").unwrap().entry_count, 1);
        fs::remove_file(logs.join("b.log")).unwrap();
        found.rescan(&ctx, std::slice::from_ref(&item.id));
        assert!(found.item("custom.0").is_none(), "没东西了就拿掉");
        assert!(logs.join("keep.db").exists());
    }

    #[test]
    fn a_cancelled_scan_stops_early_and_reports_progress() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        fs::create_dir_all(pip_dir(&home)).unwrap();
        fs::write(pip_dir(&home).join("blob"), b"x").unwrap();
        let ctx = context(&home, Options::default());
        let seen = Mutex::new(Vec::new());
        let full = scan_with(&ctx, &|_| {}, &|done, total| seen.lock().unwrap().push((done, total)), &AtomicBool::new(false));
        assert!(full.item("development.pip").is_some());
        let seen = seen.into_inner().unwrap();
        let total = seen[0].1;
        assert!(total > 10 && seen.contains(&(total, total)), "{seen:?}");
        let cancelled = scan_with(&ctx, &|_| {}, &|_, _| {}, &AtomicBool::new(true));
        assert!(cancelled.items().is_empty(), "一开始就取消：什么都不量");
    }

    /// 只读地扫一遍这台电脑，打印每一项：`cargo test -p shibapack-core scans_this_machine -- --ignored --nocapture`。
    #[test]
    #[ignore]
    fn scans_this_machine() {
        let ctx = Context::new(None, None, Vec::new(), Options::default());
        let started = std::time::Instant::now();
        let found = scan(&ctx, &|_| {});
        for item in found.items() {
            let mb = item.size.map(|s| format!("{:.1} MB", s as f64 / 1e6)).unwrap_or_else(|| "?".into());
            println!("{:<40} {:>12} {:>6} 条 {:?} {}{}", item.id, mb, item.entry_count, item.action, if item.running { "运行中 " } else { "" }, item.note.unwrap_or_default());
            for root in &item.roots {
                println!("    {root}");
            }
        }
        println!("用时 {:?}", started.elapsed());
    }
}
