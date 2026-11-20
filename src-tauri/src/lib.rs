//! 桌面外壳：把核心库的能力包成前端可以调用的命令。
//!
//! 修改数据的命令都返回修改后的整个索引，前端直接替换自己手里的那份。

use serde::{Deserialize, Serialize};
use shibapack_core::{
    ai, aicache::{self, AiCache, CachedSuggestion, CachedText}, cleanup, download, faillog,
    leftovers::{self, Leftover, RemoveReport, Target},
    import::{self, BatchSaver, Outcome, Plan},
    identify::same_software,
    installed, AtStep,
    sources::{self, Candidate, SourceError},
    startup::{self, Listing, StartupItem},
    summary, trace, AiSettings, Assignment, BackupInfo, Category, ImportHint, ImportReport, InstalledApp, Library, Progress, Release, Repo, Settings,
    SoftwareEdit, SourceKind, Stats, Storage, Subscription, Suggestion, VerifyReport, VersionEdit, VersionRef, webicon,
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, Window};
use tauri_plugin_opener::OpenerExt;

struct AppState {
    repo: Mutex<Option<Repo>>,
    /// 启动时上次用的仓库没打开：（仓库路径，原因）。原因是 `missing`（文件夹不在，比如 NAS 没连上）
    /// 或者打开时的错误。选好仓库后清掉。
    open_failure: Mutex<Option<(String, String)>>,
    /// 仓库之外唯一的文件：记录仓库根目录在哪。
    config_file: PathBuf,
    /// 本机提取图标时用的磁盘缓存。实际图标也随仓库索引保存。
    icon_cache: PathBuf,
    /// 最近一次列出的已安装软件。卸载、定位时按 id 在这里查，不接受前端传来的路径。
    installed: Mutex<Vec<InstalledApp>>,
    /// 这次运行中已经试过取图标但没取到的软件，不再重复尝试。
    icon_tried: Mutex<HashSet<String>>,
    /// 开发用的自检模式，见 `selftest_report`。值是自检时要导入的一个文件的路径。
    selftest: Option<String>,
    /// 下载一个一个来，后面的排队。
    download_turn: Mutex<()>,
    /// 正在导入：同时只跑一批。
    importing: AtomicBool,
    /// 用户点了取消导入。
    import_cancel: AtomicBool,
    /// 正在下载或排队的文件（按下载地址）对应的“取消”开关。
    cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// 应用自己的版本号（来自 package.json）。
    version: String,
    /// 正在进行的 AI 分类的“取消”开关。开始新的一次时把上一次的也关掉。
    ai_cancel: Mutex<Arc<AtomicBool>>,
    /// 统计过的已安装软件大小：“文件夹|版本”→（统计的时间，字节数）。窗口来回切换时不用每次重走一遍。
    measured: Mutex<HashMap<String, (Instant, u64)>>,
    /// 统计大小同时只跑一轮，后来的等前一轮跑完、直接用记下的结果。
    measure_turn: Mutex<()>,
    /// 正在卸载的软件：卸载前记下的样子，和找到的残留。清理时只认这里面的项，不接受界面随便给一个路径。
    uninstall: Mutex<HashMap<String, UninstallSession>>,
    /// 主页和任务管理器读系统状态用的，第一次打开主页时才建。
    monitor: Mutex<Option<shibapack_core::system::Monitor>>,
    /// 进程的可执行文件 → 图标文件（取不到的也记下，不再试）。和 `monitor` 分开锁，取图标慢时不挡着刷新。
    process_icons: Mutex<HashMap<String, Option<String>>>,
    /// 最近一次列出的启动项和服务。启用、停用、定位时按 id 在这里查，不接受前端传来的路径或命令。
    startup: Mutex<Vec<StartupItem>>,
    /// 列启动项和改启动项一次只做一件，免得改到一半又被一次列表盖掉。
    startup_turn: Mutex<()>,
    /// 读改写 AI 缓存文件时一次只让一个命令做（问 AI 的时候不占着它）。
    ai_cache_turn: Mutex<()>,
    /// 最近一次清理扫描的结果。清理时按 id 和序号在这里找路径，不接受前端传来的路径。
    cleanup: Mutex<Option<cleanup::Scan>>,
    /// 扫描和清理一次只做一件。
    cleanup_turn: Mutex<()>,
    /// 用户点了取消扫描。
    cleanup_cancel: AtomicBool,
    /// 磁盘分析：最近一次扫完的树（只在内存里），和停止扫描的开关。
    disk: Mutex<Option<shibapack_core::disk::Scan>>,
    /// 正在扫的时候：边扫边看的累计（扫完或停下就清掉）。
    disk_live: Mutex<Option<Arc<shibapack_core::disk::Live>>>,
    disk_cancel: AtomicBool,
    /// 失败日志放在哪（应用自己的日志文件夹，只和这台电脑有关）。
    log_dir: PathBuf,
    /// 正在进行的安装监控（同时只有一个）：基线快照和要装的是什么。
    trace: Mutex<Option<TraceSession>>,
    /// 拍快照时用户点了取消。
    trace_cancel: AtomicBool,
    /// 正在拍快照（开始时的基线，或完成时的第二次），这时不接受别的开始、完成。
    trace_busy: AtomicBool,
}

/// 记一条失败日志。
fn log_failure_to(state: &State<AppState>, area: &str, message: &str, detail: &str) {
    faillog::append(&state.log_dir, area, message, detail);
}

/// 一个软件的卸载过程：卸载前记下的样子和最近一次找到的项。可以同时卸载好几个，按已安装软件的 id 分开。
struct UninstallSession {
    target: Target,
    found: Vec<Leftover>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LeftoverScan {
    items: Vec<Leftover>,
    /// 软件看起来还在（卸载程序可能被取消了）：找到的都没有默认勾选。卸载前的分析里总是假。
    still_installed: bool,
    /// 卸载前的分析里“App”那一行显示的：Windows 是登记的卸载命令，macOS 是 .app 的位置。
    #[serde(skip_serializing_if = "Option::is_none")]
    app_path: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Bootstrap {
    repo_root: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    configured: bool,
    repo_root: Option<String>,
    /// 启动时索引损坏并做了恢复：恢复所用的备份文件名，空字符串表示没有可用备份。
    recovered: Option<String>,
    /// 上次用的仓库没打开时，它的路径和原因（见 `AppState::open_failure`）。
    last_root: Option<String>,
    open_error: Option<String>,
    version: String,
    /// 这台电脑的处理器架构：x64 或 arm64。用来挑适合本机的安装包。
    arch: String,
    selftest: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SubscribeResult {
    library: Library,
    software_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckResult {
    library: Library,
    checked: usize,
    failed: usize,
    /// GitHub 限流了：GitHub 和 winget 源的软件这次没查完。
    rate_limited: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckProgress<'a> {
    current: usize,
    total: usize,
    name: &'a str,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadProgress<'a> {
    software_id: &'a str,
    url: &'a str,
    name: &'a str,
    received: u64,
    total: Option<u64>,
    /// 这个文件的下载结束了（成功、失败或取消）。
    done: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FillResult {
    /// 有图标补上了才带回新的索引。
    library: Option<Library>,
    /// 还有没处理的软件，前端应当再调一次。
    more: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AiSuggestions {
    suggestions: Vec<Suggestion>,
    /// 其中有几个用的是上次 AI 的建议（这次没有问）。
    cached: usize,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiProgress {
    done: usize,
    total: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportResult {
    report: ImportReport,
    library: Library,
}

type CmdResult<T> = Result<T, String>;

/// 在专门跑阻塞任务的线程池里执行一个命令。
///
/// 命令里都是读写磁盘、联网、等锁这类会卡住线程的事。Tauri 把 `async` 命令放在处理命令的异步线程上跑，
/// 那里的线程数和 CPU 核数一样多：几个慢的命令同时在跑（导入、排队的下载、扫描已安装软件、联网查简介），
/// 别的命令连同插件就都跟着卡住。所以每个命令都换到 `spawn_blocking` 的线程池里去做。
async fn blocking<T: Send + 'static>(app: AppHandle, run: impl FnOnce(&AppHandle, State<'_, AppState>) -> T + Send + 'static) -> T {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        run(&app, state)
    })
    .await
    .expect("ShibaPack 命令执行时崩溃")
}

/// 在仓库上做一件事。先读入别处对索引的改动；失败时内存里的索引退回原样（见 `Repo::transact`）。
fn with_repo<T>(state: &State<AppState>, run: impl FnOnce(&mut Repo) -> shibapack_core::Result<T>) -> CmdResult<T> {
    let mut guard = state.repo.lock().map_err(|_| "internal".to_string())?;
    let repo = guard.as_mut().ok_or_else(|| "not_configured".to_string())?;
    repo.transact(run).map_err(|e| e.to_string())
}

/// 短暂地占一下仓库做一件小事，不像 `with_repo` 那样先整份备份内存里的索引。用在导入这种
/// 一个文件一个文件登记的地方。给了 `root` 时，仓库已经换成别的（导入途中换了仓库）就报错。
fn with_repo_quick<T>(
    state: &State<AppState>,
    root: Option<&Path>,
    run: impl FnOnce(&mut Repo) -> shibapack_core::Result<T>,
) -> shibapack_core::Result<T> {
    let mut guard = state.repo.lock().map_err(|_| shibapack_core::Error::Code("internal"))?;
    let repo = guard.as_mut().filter(|repo| root.is_none_or(|root| repo.root() == root)).ok_or(shibapack_core::Error::Code("not_configured"))?;
    repo.reload_if_changed()?;
    run(repo)
}

/// 离开作用域时把开关放下，中途出错返回也一样。
struct ClearOnDrop<'a>(&'a AtomicBool);

impl Drop for ClearOnDrop<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// 做一次修改，然后把修改后的索引交回前端。
fn mutate(state: &State<AppState>, run: impl FnOnce(&mut Repo) -> shibapack_core::Result<()>) -> CmdResult<Library> {
    with_repo(state, |repo| {
        run(repo)?;
        Ok(repo.lib.clone())
    })
}

fn status_of(state: &State<AppState>) -> Status {
    let guard = state.repo.lock().ok();
    let repo = guard.as_ref().and_then(|g| g.as_ref());
    let failure = state.open_failure.lock().ok().and_then(|f| f.clone());
    Status {
        configured: repo.is_some(),
        repo_root: repo.map(|r| r.root().to_string_lossy().to_string()),
        recovered: repo.and_then(|r| r.recovered.clone()),
        last_root: failure.as_ref().map(|(root, _)| root.clone()),
        open_error: failure.map(|(_, reason)| reason),
        version: state.version.clone(),
        arch: match std::env::consts::ARCH {
            "x86_64" => "x64",
            "aarch64" => "arm64",
            "x86" => "x86",
            other => other,
        }
        .to_string(),
        selftest: state.selftest.clone(),
    }
}

/// 打开一个仓库后，允许界面读取它的图标文件夹。
fn allow_repo_icons(app: &AppHandle, repo: &Repo) {
    let _ = app.asset_protocol_scope().allow_directory(repo.icons_dir(), true);
}

fn find_installed(state: &State<AppState>, id: &str) -> CmdResult<InstalledApp> {
    let list = state.installed.lock().map_err(|_| "internal".to_string())?;
    list.iter().find(|app| app.id == id).cloned().ok_or_else(|| "not_found".to_string())
}

/// 仓库之外的文件送进系统回收站。删不掉就留着，不算错误：记录已经移除，文件没有丢。
fn trash_external(paths: Vec<PathBuf>) {
    for path in paths {
        if path.exists() {
            let _ = trash::delete(&path);
        }
    }
}

#[tauri::command]
fn get_status(state: State<AppState>) -> Status {
    status_of(&state)
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DetailWindowResize {
    open: bool,
    detail_width: u32,
    before: u32,
    after: u32,
}

/// 让窗口宽度跟随右栏，在最小宽度或屏幕边缘截断时也可以还原。
#[tauri::command]
async fn resize_detail_window(
    window: Window,
    open: bool,
    detail_width: f64,
    dock_min_width: f64,
    previous: Option<DetailWindowResize>,
) -> Result<Option<DetailWindowResize>, String> {
    resize_detail(&window, open, detail_width, dock_min_width, previous).map_err(|err| err.to_string())
}

fn resize_detail(
    window: &Window,
    open: bool,
    detail_width: f64,
    dock_min_width: f64,
    previous: Option<DetailWindowResize>,
) -> tauri::Result<Option<DetailWindowResize>> {
    if window.is_maximized()? || window.is_fullscreen()? || window.is_minimized()? || !window.is_resizable()? {
        return Ok(None);
    }
    let size = window.inner_size()?;
    let scale = window.scale_factor()?;
    let pixels = |logical: f64| (logical * scale).round() as u32;
    let detail_width = pixels(detail_width);
    let restored = previous.filter(|last| last.open != open && last.detail_width == detail_width && last.after.abs_diff(size.width) <= 1);
    let desired = if let Some(last) = restored {
        last.before
    } else if open {
        size.width.saturating_add(detail_width)
    } else if size.width >= pixels(dock_min_width) {
        size.width.saturating_sub(detail_width)
    } else {
        // 浮动详情没有占用中栏宽度，收起时无需缩小原窗口。
        return Ok(None);
    };

    let mut width = desired;
    if width < size.width {
        let min = window.app_handle().config().app.windows.iter().find(|config| config.label == window.label())
            .and_then(|config| config.min_width).unwrap_or_default();
        width = width.max(pixels(min).min(size.width));
    } else if width > size.width {
        if let Some(monitor) = window.current_monitor()? {
            let area = monitor.work_area();
            let frame = window.outer_size()?.width.saturating_sub(size.width);
            width = width.min(area.size.width.saturating_sub(frame).max(size.width));
            if width > size.width {
                let position = window.outer_position()?;
                let left = i64::from(area.position.x);
                let right = left + i64::from(area.size.width);
                let max_left = right - i64::from(width) - i64::from(frame);
                let x = i64::from(position.x).clamp(left, max_left);
                if x != i64::from(position.x) && window.set_position(PhysicalPosition::new(x as i32, position.y)).is_err() {
                    let available = (right - i64::from(position.x) - i64::from(frame)).max(0) as u32;
                    width = width.min(available.max(size.width));
                }
            }
        }
    }
    if width == size.width {
        return Ok(None);
    }
    window.set_size(PhysicalSize::new(width, size.height))?;
    Ok(Some(DetailWindowResize { open, detail_width, before: size.width, after: width }))
}

#[tauri::command]
async fn set_repo_root(app: AppHandle, path: String) -> CmdResult<Status> {
    blocking(app, move |app, state| {
        let repo = Repo::open(Path::new(&path)).map_err(|e| e.to_string())?;
        allow_repo_icons(app, &repo);
        let bootstrap = Bootstrap { repo_root: Some(repo.root().to_string_lossy().to_string()) };
        if let Some(dir) = state.config_file.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_vec_pretty(&bootstrap).map_err(|e| e.to_string())?;
        fs::write(&state.config_file, json).map_err(|e| e.to_string())?;
        *state.repo.lock().map_err(|_| "internal".to_string())? = Some(repo);
        if let Ok(mut failure) = state.open_failure.lock() {
            *failure = None;
        }
        if let Ok(mut tried) = state.icon_tried.lock() {
            tried.clear();
        }
        Ok(status_of(&state))
    })
    .await
}

#[tauri::command]
async fn get_library(app: AppHandle) -> CmdResult<Library> {
    blocking(app, move |_app, state| {
        with_repo(&state, |repo| Ok(repo.lib.clone()))
    })
    .await
}

/// 窗口回到前台时调用：索引被别处改过（另一台电脑、手动编辑）就重新读入并交回前端，没改过返回 null。
#[tauri::command]
async fn refresh_library(app: AppHandle) -> CmdResult<Option<Library>> {
    blocking(app, move |_app, state| {
        let mut guard = state.repo.lock().map_err(|_| "internal".to_string())?;
        let repo = guard.as_mut().ok_or_else(|| "not_configured".to_string())?;
        let changed = repo.reload_if_changed().map_err(|e| e.to_string())?;
        Ok(changed.then(|| repo.lib.clone()))
    })
    .await
}

/// 导入文件或文件夹。`move_files` 为真时（只对托管模式有效）导入后不留原文件。
/// 进度通过 `import://progress` 事件发给前端，`cancel_import` 可以在两个文件之间停下。
///
/// 算哈希、复制文件都不占着仓库，只在查重和登记时短暂地占一下：导入几个 G 的文件时，
/// 界面上的其他操作照常进行。同时只跑一批导入，第二批会得到 `busy`。
#[tauri::command]
async fn import_paths(app: AppHandle, paths: Vec<String>, mode: Storage, move_files: Option<bool>, hint: Option<ImportHint>) -> CmdResult<ImportResult> {
    blocking(app, move |app, state| {
        if state.importing.swap(true, Ordering::SeqCst) {
            return Err("busy".into());
        }
        let _importing = ClearOnDrop(&state.importing);
        state.import_cancel.store(false, Ordering::SeqCst);
        let progress = |phase: &str, current: usize, total: usize, file: String| {
            let _ = app.emit("import://progress", &Progress { phase: phase.into(), current, total, file });
        };
        progress("collect", 0, 0, String::new());

        let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        let (hint, move_files) = (hint.unwrap_or_default(), move_files.unwrap_or(false));
        let (root, data_dir, mut known, auto_classify) = with_repo_quick(&state, None, |repo| {
            Ok((repo.root().to_path_buf(), repo.data_dir(), repo.known_paths(), hint.uses_auto_classification(&repo.lib.settings)))
        })
            .map_err(|e| e.to_string())?;
        // 展开文件夹也不占着仓库：NAS 上的大文件夹要走一会儿
        let files = import::collect_files(&paths, &data_dir, &hint.package_set());
        let total = files.len();
        let mut report = ImportReport::default();
        let mut saver = BatchSaver::new();
        for (i, file) in files.iter().enumerate() {
            if state.import_cancel.load(Ordering::SeqCst) {
                report.cancelled = true;
                break;
            }
            progress("import", i + 1, total, file.file_name().unwrap_or_default().to_string_lossy().to_string());
            let outcome = import_one(&state, &root, file, mode, move_files, &hint, &mut known);
            if let Err(err) = &outcome {
                let message = format!("{} · {} · {err}", err.step().unwrap_or("import"), err.reason());
                log_failure_to(&state, "import", &message, &file.to_string_lossy());
            }
            report.add(file, outcome);
            with_repo_quick(&state, Some(&root), |repo| saver.maybe_save(repo, report.imported)).map_err(|e| e.to_string())?;
        }
        let mut library = with_repo_quick(&state, Some(&root), |repo| {
            saver.finish(repo, report.imported)?;
            Ok(repo.lib.clone())
        })
        .map_err(|e| e.to_string())?;
        if auto_classify && report.imported > 0 && !state.import_cancel.load(Ordering::SeqCst) {
            progress("classify", 0, 0, String::new());
            if let Err(error) = classify_import(&state, &root, &library, &mut report) {
                if !state.import_cancel.load(Ordering::SeqCst) {
                    report.classification_error = Some(error);
                }
            }
            library = with_repo_quick(&state, Some(&root), |repo| Ok(repo.lib.clone())).map_err(|e| e.to_string())?;
        }
        progress("done", total, total, String::new());
        Ok(ImportResult { report, library })
    })
    .await
}

fn classify_import(state: &State<AppState>, root: &Path, library: &Library, report: &mut ImportReport) -> CmdResult<()> {
    let imported: HashSet<&str> = report.items.iter().filter(|item| item.status == "imported").filter_map(|item| item.software_id.as_deref()).collect();
    let ids: Vec<String> = library.software.iter().filter(|s| s.category_id.is_none() && imported.contains(s.id.as_str())).map(|s| s.id.clone()).collect();
    if ids.is_empty() {
        return Ok(());
    }
    let (classified, library, remaining) = with_repo_quick(state, Some(root), |repo| repo.transact(|repo| {
        let classified = repo.apply_import_classification(repo.suggest_categories(&ids))?;
        let remaining: Vec<String> = ids.into_iter().filter(|id| repo.lib.software.iter().any(|s| s.id == *id && s.category_id.is_none())).collect();
        Ok((classified, repo.lib.clone(), remaining))
    })).map_err(|e| e.to_string())?;
    report.classified = classified;
    if remaining.is_empty() || state.import_cancel.load(Ordering::SeqCst) {
        return Ok(());
    }
    // 没有配置可用的 AI 时，内置规则的结果已经保存。
    let Ok(service) = ai_service(&library.settings.ai, None) else { return Ok(()) };
    let suggestions = ai::suggest_categories(&service, &library, &remaining, &state.import_cancel, &mut |_, _| {})
        .map_err(|e| e.code())?;
    if !state.import_cancel.load(Ordering::SeqCst) {
        report.classified += with_repo_quick(state, Some(root), |repo| repo.transact(|repo| repo.apply_import_classification(suggestions)))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 导入一个文件。最慢的两步（算哈希、复制或移动）不占着仓库。
fn import_one(
    state: &State<AppState>,
    root: &Path,
    file: &Path,
    mode: Storage,
    move_files: bool,
    hint: &ImportHint,
    known: &mut HashSet<PathBuf>,
) -> shibapack_core::Result<Outcome> {
    if let Some(owner) = with_repo_quick(state, Some(root), |repo| Ok(repo.already_known(file, known)))? {
        return Ok(Outcome::Duplicate(owner));
    }
    let hashed = import::hash_source(file).at("read")?;
    let placement = match with_repo_quick(state, Some(root), |repo| Ok(repo.plan_import(&hashed, mode, hint)))? {
        Plan::Duplicate(owner) => return Ok(Outcome::Duplicate(owner)),
        Plan::Add(placement) => placement,
    };
    let placed = import::place_file(root, &hashed, &placement, move_files).at("copy")?;
    with_repo_quick(state, Some(root), |repo| repo.register(&hashed, placement, placed, known)).at("save")
}

/// 导入前看一看拖进来的东西（给确认窗口用）：文件夹像不像一个软件、里面有几个安装包、会被认成什么。
#[tauri::command]
async fn inspect_drop(app: AppHandle, paths: Vec<String>, include_children: Option<bool>) -> CmdResult<Vec<import::DropEntry>> {
    blocking(app, move |_app, state| {
        let (data_dir, software_names) = with_repo(&state, |repo| {
            let names: Vec<_> = repo.lib.software.iter().map(|s| (s.id.clone(), s.name.clone())).collect();
            Ok((repo.data_dir(), names))
        })?;
        let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        Ok(import::inspect(&paths, &data_dir, &software_names, include_children.unwrap_or(false)))
    })
    .await
}

#[tauri::command]
async fn preview_import_categories(
    app: AppHandle,
    targets: Vec<shibapack_core::classify::ImportCategoryTarget>,
) -> CmdResult<Vec<shibapack_core::classify::ImportCategoryPreview>> {
    blocking(app, move |_app, state| with_repo(&state, |repo| Ok(repo.preview_import_categories(&targets)))).await
}

/// 在两个文件之间停下正在进行的导入。已经导入的保留。
#[tauri::command]
fn cancel_import(state: State<AppState>) {
    state.import_cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
async fn update_software(app: AppHandle, id: String, edit: SoftwareEdit) -> CmdResult<Library> {
    blocking(app, move |_app, state| {
        mutate(&state, |repo| repo.update_software(&id, edit))
    })
    .await
}

#[tauri::command]
async fn update_version(app: AppHandle, software_id: String, version_id: String, edit: VersionEdit) -> CmdResult<Library> {
    blocking(app, move |_app, state| {
        mutate(&state, |repo| repo.update_version(&software_id, &version_id, edit))
    })
    .await
}

#[tauri::command]
async fn delete_software(app: AppHandle, id: String, delete_files: bool) -> CmdResult<Library> {
    blocking(app, move |_app, state| {
        mutate(&state, |repo| repo.delete_software(&id, delete_files).map(trash_external))
    })
    .await
}

/// 多选删除。
#[tauri::command]
async fn delete_software_many(app: AppHandle, ids: Vec<String>, delete_files: bool) -> CmdResult<Library> {
    blocking(app, move |_app, state| {
        mutate(&state, |repo| repo.delete_software_many(&ids, delete_files).map(trash_external))
    })
    .await
}

/// 清理旧版本：一次删掉多个版本。
#[tauri::command]
async fn delete_versions(app: AppHandle, targets: Vec<VersionRef>, delete_files: bool) -> CmdResult<Library> {
    blocking(app, move |_app, state| {
        mutate(&state, |repo| repo.delete_versions(&targets, delete_files).map(trash_external))
    })
    .await
}

#[tauri::command]
async fn delete_version(app: AppHandle, software_id: String, version_id: String, delete_files: bool) -> CmdResult<Library> {
    blocking(app, move |_app, state| {
        mutate(&state, |repo| repo.delete_version(&software_id, &version_id, delete_files).map(trash_external))
    })
    .await
}

#[tauri::command]
async fn delete_file(app: AppHandle, software_id: String, version_id: String, file_id: String, delete_from_disk: bool) -> CmdResult<Library> {
    blocking(app, move |_app, state| {
        mutate(&state, |repo| {
            repo.delete_file(&software_id, &version_id, &file_id, delete_from_disk).map(trash_external)
        })
    })
    .await
}

/// 用系统默认方式打开安装包（exe 直接运行，dmg 挂载，deb 交给软件中心）。
#[tauri::command]
async fn run_file(app: AppHandle, software_id: String, version_id: String, file_id: String) -> CmdResult<()> {
    blocking(app, move |app, state| {
        let path = with_repo(&state, |repo| repo.file_path(&software_id, &version_id, &file_id))?;
        if !path.exists() {
            return Err("file_missing".into());
        }
        // 文件夹当作安装包时打开里面的主程序（.app 直接打开），找不到就打开文件夹
        let target = import::launch_target(&path);
        app.opener().open_path(target.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
    })
    .await
}

/// 在文件管理器里定位到这个文件。
#[tauri::command]
async fn reveal_file(app: AppHandle, software_id: String, version_id: String, file_id: String) -> CmdResult<()> {
    blocking(app, move |app, state| {
        let path = with_repo(&state, |repo| repo.file_path(&software_id, &version_id, &file_id))?;
        if !path.exists() {
            return Err("file_missing".into());
        }
        app.opener().reveal_item_in_dir(path).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
async fn open_repo_folder(app: AppHandle) -> CmdResult<()> {
    blocking(app, move |app, state| {
        let root = with_repo(&state, |repo| Ok(repo.root().to_path_buf()))?;
        app.opener().open_path(root.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
async fn set_categories(app: AppHandle, categories: Vec<Category>) -> CmdResult<Library> {
    blocking(app, move |_app, state| {
        mutate(&state, |repo| repo.set_categories(categories))
    })
    .await
}

#[tauri::command]
async fn set_starred(app: AppHandle, software_id: String, starred: bool) -> CmdResult<Library> {
    blocking(app, move |_app, state| {
        mutate(&state, |repo| repo.set_starred(&software_id, starred))
    })
    .await
}

/// 智能分类第一步：给一批软件出建议，不改动任何东西。
/// 多选收藏或取消收藏，只存一次索引。
#[tauri::command]
async fn set_starred_many(app: AppHandle, ids: Vec<String>, starred: bool) -> CmdResult<Library> {
    blocking(app, move |_app, state| mutate(&state, |repo| repo.set_starred_many(&ids, starred))).await
}

/// 把多个软件移到一个分类（null 是未分类），只存一次索引。
#[tauri::command]
async fn move_software(app: AppHandle, ids: Vec<String>, category_id: Option<String>) -> CmdResult<Library> {
    blocking(app, move |_app, state| mutate(&state, |repo| repo.move_to_category(&ids, category_id.as_deref()))).await
}

#[tauri::command]
async fn suggest_categories(app: AppHandle, software_ids: Vec<String>) -> CmdResult<Vec<Suggestion>> {
    blocking(app, move |_app, state| {
        with_repo(&state, |repo| Ok(repo.suggest_categories(&software_ids)))
    })
    .await
}

/// 智能分类第二步：按用户在预览里确认的结果分类。
#[tauri::command]
async fn apply_classification(app: AppHandle, assignments: Vec<Assignment>) -> CmdResult<Library> {
    blocking(app, move |_app, state| {
        mutate(&state, |repo| repo.apply_classification(assignments).map(|_| ()))
    })
    .await
}

#[tauri::command]
async fn set_settings(app: AppHandle, settings: Settings) -> CmdResult<Library> {
    blocking(app, move |_app, state| {
        mutate(&state, |repo| repo.set_settings(settings))
    })
    .await
}

#[tauri::command]
async fn get_stats(app: AppHandle) -> CmdResult<Stats> {
    blocking(app, move |_app, state| {
        with_repo(&state, |repo| Ok(repo.stats()))
    })
    .await
}

#[tauri::command]
async fn list_backups(app: AppHandle) -> CmdResult<Vec<BackupInfo>> {
    blocking(app, move |_app, state| {
        with_repo(&state, |repo| Ok(repo.backups()))
    })
    .await
}

#[tauri::command]
async fn create_backup(app: AppHandle) -> CmdResult<Vec<BackupInfo>> {
    blocking(app, move |_app, state| {
        with_repo(&state, |repo| {
            repo.create_backup()?;
            Ok(repo.backups())
        })
    })
    .await
}

#[tauri::command]
async fn restore_backup(app: AppHandle, name: String) -> CmdResult<Library> {
    blocking(app, move |_app, state| {
        mutate(&state, |repo| repo.restore_backup(&name))
    })
    .await
}

#[tauri::command]
async fn verify_data(app: AppHandle) -> CmdResult<VerifyReport> {
    blocking(app, move |_app, state| {
        with_repo(&state, |repo| Ok(repo.verify()))
    })
    .await
}

#[tauri::command]
async fn export_backup(app: AppHandle, dest: String) -> CmdResult<()> {
    blocking(app, move |_app, state| {
        with_repo(&state, |repo| repo.export_to(Path::new(&dest)))
    })
    .await
}

#[tauri::command]
async fn import_backup(app: AppHandle, src: String) -> CmdResult<Library> {
    blocking(app, move |_app, state| {
        mutate(&state, |repo| repo.import_from(Path::new(&src)))
    })
    .await
}

#[tauri::command]
async fn empty_trash(app: AppHandle) -> CmdResult<Stats> {
    blocking(app, move |_app, state| {
        with_repo(&state, |repo| {
            repo.empty_trash()?;
            Ok(repo.stats())
        })
    })
    .await
}

// ── 已安装软件 ───────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstalledListing {
    apps: Vec<InstalledApp>,
    cached_icons: Option<BTreeMap<String, Arc<str>>>,
}

/// 列出这台电脑上已安装的软件（会重新扫描）。
#[tauri::command]
async fn list_installed(app: AppHandle) -> InstalledListing {
    blocking(app, move |_app, state| {
        let apps = installed::list_installed(&state.icon_cache);
        let changed = if let Ok(mut cached) = state.installed.lock() {
            let changed = !cached.iter().map(|app| (&app.id, &app.icon)).eq(apps.iter().map(|app| (&app.id, &app.icon)));
            *cached = apps.clone();
            changed
        } else {
            false
        };
        if changed {
            if let Ok(mut tried) = state.icon_tried.lock() {
                tried.clear();
            }
        }
        let cached_icons = with_repo(&state, |repo| {
            repo.cache_installed_icons(&apps)?;
            Ok(repo.lib.installed_icons.clone())
        }).ok();
        InstalledListing { apps, cached_icons }
    })
    .await
}

/// 统计过的大小记多久。软件升级（版本号变了）会马上重新统计。
const MEASURE_TTL: Duration = Duration::from_secs(30 * 60);
/// 同时走几个文件夹。
const MEASURE_WORKERS: usize = 4;

/// 统计各软件实际占用的空间：有自己文件夹的（macOS 的 .app、Windows 登记的安装文件夹）走一遍加起来，
/// 其余用安装程序登记的大小（见 `installed::size_sources`）。要走很多文件，比较慢，所以和列表分开。
#[tauri::command]
async fn measure_installed(app: AppHandle) -> HashMap<String, u64> {
    blocking(app, move |_app, state| {
        let _turn = state.measure_turn.lock();
        let apps = state.installed.lock().map(|list| list.clone()).unwrap_or_default();
        let by_id: HashMap<&str, &InstalledApp> = apps.iter().map(|app| (app.id.as_str(), app)).collect();
        let mut sizes = HashMap::new();
        // 要走一遍的：（软件 id，记录用的键，文件夹，走不了时用的登记大小）
        let mut todo: Vec<(String, String, PathBuf, Option<u64>)> = Vec::new();
        {
            let cache = state.measured.lock().unwrap_or_else(|e| e.into_inner());
            for (id, source) in installed::size_sources(&apps) {
                match source {
                    installed::SizeSource::Registered(size) => {
                        sizes.insert(id, size);
                    }
                    installed::SizeSource::Unknown => {}
                    installed::SizeSource::Scan(dir) => {
                        let app = by_id[id.as_str()];
                        let key = format!("{}|{}", dir.display(), app.version);
                        match cache.get(&key).filter(|(at, _)| at.elapsed() < MEASURE_TTL) {
                            Some((_, size)) => {
                                sizes.insert(id, *size);
                            }
                            None => todo.push((id, key, dir, app.size)),
                        }
                    }
                }
            }
        }

        let next = std::sync::atomic::AtomicUsize::new(0);
        let found: Mutex<Vec<(usize, Option<u64>)>> = Mutex::new(Vec::new());
        std::thread::scope(|scope| {
            for _ in 0..MEASURE_WORKERS.min(todo.len()) {
                scope.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some((_, _, dir, _)) = todo.get(i) else { break };
                    let size = installed::folder_size(dir);
                    if let Ok(mut found) = found.lock() {
                        found.push((i, size));
                    }
                });
            }
        });

        let mut cache = state.measured.lock().unwrap_or_else(|e| e.into_inner());
        for (i, size) in found.into_inner().unwrap_or_else(|e| e.into_inner()) {
            let (id, key, _, registered) = &todo[i];
            match size {
                Some(size) => {
                    cache.insert(key.clone(), (Instant::now(), size));
                    sizes.insert(id.clone(), size);
                }
                // 走不了（文件太多、打不开）：用登记的大小
                None => {
                    if let Some(size) = registered {
                        sizes.insert(id.clone(), *size);
                    }
                }
            }
        }
        sizes
    })
    .await
}

/// 仓库里的软件和已安装软件的对应关系：软件 id → 已安装软件 id。
#[tauri::command]
async fn match_installed(app: AppHandle) -> CmdResult<HashMap<String, String>> {
    blocking(app, move |_app, state| {
        let apps = state.installed.lock().map(|list| list.clone()).unwrap_or_default();
        with_repo(&state, |repo| {
            let links = repo.lib.software.iter().filter_map(|software| {
                let app = apps.iter().find(|app| same_software(&app.name, &software.name))?;
                Some((software.id.clone(), app.id.clone()))
            });
            Ok(links.collect())
        })
    })
    .await
}

/// 卸载前先分析：这个软件的数据、快捷方式、设置和开机启动项在哪、多大。只读。
/// 软件本身（安装文件夹、卸载项）不在里面，交给卸载程序；卸载完再用 `scan_leftovers` 找它没删干净的。
#[tauri::command]
async fn analyze_uninstall(app: AppHandle, id: String) -> CmdResult<LeftoverScan> {
    blocking(app, move |_app, state| {
        let app = find_installed(&state, &id)?;
        let target = Target::of(&app);
        let others: Vec<InstalledApp> = state.installed.lock().map(|list| list.iter().filter(|other| other.id != id).cloned().collect()).unwrap_or_default();
        let items = leftovers::scan_before(&target, &others);
        if let Ok(mut sessions) = state.uninstall.lock() {
            sessions.insert(id.clone(), UninstallSession { target, found: items.clone() });
        }
        let app_path = app.uninstall_command.clone().filter(|_| app.source == "registry").or_else(|| (!app.location.is_empty()).then(|| app.location.clone()));
        Ok(LeftoverScan { items, still_installed: false, app_path })
    })
    .await
}

/// 卸载。Windows 上启动系统登记的卸载程序；macOS 上把 .app 移到废纸篓。
#[tauri::command]
async fn uninstall_installed(app: AppHandle, id: String) -> CmdResult<()> {
    blocking(app, move |_app, state| {
        let app = find_installed(&state, &id)?;
        if !app.uninstallable {
            return Err("not_uninstallable".into());
        }
        // 卸载以后就查不到它的安装位置、发布者了，先记下来，找残留时用；分析过的话留着分析的结果
        if let Ok(mut sessions) = state.uninstall.lock() {
            sessions.entry(id.clone()).or_insert_with(|| UninstallSession { target: Target::of(&app), found: Vec::new() });
        }
        if installed::run_uninstaller(&app).map_err(|e| e.to_string())? {
            return Ok(());
        }
        if cfg!(target_os = "macos") && Path::new(&app.location).is_dir() {
            return trash::delete(&app.location).map_err(|e| e.to_string());
        }
        Err("not_uninstallable".into())
    })
    .await
}

/// 卸载程序还没跑完（软件还在）。Windows 看卸载信息还在不在；macOS 是直接移到废纸篓，不用等。
#[tauri::command]
async fn uninstall_status(app: AppHandle, id: String) -> bool {
    blocking(app, move |_app, state| {
        let target = state.uninstall.lock().ok().and_then(|s| s.get(&id).map(|s| s.target.clone()));
        target.is_some_and(|target| leftovers::still_installed(&target))
    })
    .await
}

/// 找刚卸载的软件留下的文件夹、文件和注册表项。只读。
#[tauri::command]
async fn scan_leftovers(app: AppHandle, id: String) -> CmdResult<LeftoverScan> {
    blocking(app, move |_app, state| {
        let target = state.uninstall.lock().ok().and_then(|s| s.get(&id).map(|s| s.target.clone())).ok_or_else(|| "not_found".to_string())?;
        let others: Vec<InstalledApp> = state.installed.lock().map(|list| list.iter().filter(|app| app.id != id).cloned().collect()).unwrap_or_default();
        let still_installed = leftovers::still_installed(&target);
        let items = leftovers::scan(&target, &others, still_installed);
        if let Some(session) = state.uninstall.lock().ok().as_mut().and_then(|s| s.get_mut(&id)) {
            session.found = items.clone();
        }
        Ok(LeftoverScan { items, still_installed, app_path: None })
    })
    .await
}

/// 清理勾选的残留（按上次找到的列表里的序号）：文件和文件夹移到回收站，注册表项先备份再删。
/// `elevated` 为真时以管理员身份做（Windows 会弹 UAC 提示），用在普通权限删不掉的那些上。
#[tauri::command]
async fn remove_leftovers(app: AppHandle, id: String, indices: Vec<usize>, elevated: bool) -> CmdResult<RemoveReport> {
    blocking(app, move |app, state| {
        let items: Vec<Leftover> = {
            let sessions = state.uninstall.lock().map_err(|_| "internal".to_string())?;
            let session = sessions.get(&id).ok_or_else(|| "not_found".to_string())?;
            indices.iter().filter_map(|i| session.found.get(*i).cloned()).collect()
        };
        let backups = app.path().app_local_data_dir().map_err(|e| e.to_string())?.join("uninstall-backups");
        if elevated {
            leftovers::remove_elevated(&items, &backups)
        } else {
            Ok(leftovers::remove(&items, &backups))
        }
    })
    .await
}

/// 在文件管理器里显示分析或查找到的某一项（按序号；注册表项没法显示）。
#[tauri::command]
async fn reveal_leftover(app: AppHandle, id: String, index: usize) -> CmdResult<()> {
    blocking(app, move |app, state| {
        let item = state.uninstall.lock().ok().and_then(|s| s.get(&id).and_then(|s| s.found.get(index).cloned())).ok_or_else(|| "not_found".to_string())?;
        if !matches!(item.kind, leftovers::LeftoverKind::Folder | leftovers::LeftoverKind::File) || !Path::new(&item.path).exists() {
            return Err("file_missing".into());
        }
        app.opener().reveal_item_in_dir(&item.path).map_err(|e| e.to_string())
    })
    .await
}

/// 打开清理时备份注册表的文件夹。只接受备份文件夹里面的路径。
#[tauri::command]
async fn open_uninstall_backup(app: AppHandle, path: String) -> CmdResult<()> {
    blocking(app, move |app, _state| {
        let backups = app.path().app_local_data_dir().map_err(|e| e.to_string())?.join("uninstall-backups");
        let path = PathBuf::from(path);
        if !path.starts_with(&backups) || !path.is_dir() {
            return Err("file_missing".into());
        }
        app.opener().open_path(path.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
async fn reveal_installed(app: AppHandle, id: String) -> CmdResult<()> {
    blocking(app, move |app, state| {
        let target = find_installed(&state, &id)?;
        if target.location.is_empty() || !Path::new(&target.location).exists() {
            return Err("file_missing".into());
        }
        app.opener().reveal_item_in_dir(&target.location).map_err(|e| e.to_string())
    })
    .await
}

// ── 主页：系统状态和进程 ─────────────────────────────────────

/// 用（第一次时建）读系统状态的 `Monitor`。
fn with_monitor<T>(state: &State<AppState>, f: impl FnOnce(&mut shibapack_core::system::Monitor) -> CmdResult<T>) -> CmdResult<T> {
    let mut guard = state.monitor.lock().map_err(|_| "internal".to_string())?;
    if guard.is_none() {
        *guard = Some(shibapack_core::system::Monitor::new()?);
    }
    f(guard.as_mut().expect("monitor just created"))
}

/// 电脑名、系统、处理器、内存总量、开机时间。
#[tauri::command]
async fn system_overview(app: AppHandle) -> CmdResult<shibapack_core::system::Overview> {
    blocking(app, move |_app, state| with_monitor(&state, |monitor| Ok(monitor.overview()))).await
}

/// 此刻的 CPU、内存、系统盘和所有进程。主页开着时每 2 秒要一次。
#[tauri::command]
async fn system_sample(app: AppHandle) -> CmdResult<shibapack_core::system::Sample> {
    blocking(app, move |_app, state| with_monitor(&state, |monitor| Ok(monitor.sample()))).await
}

/// 这些进程的图标（按可执行文件）：在某个 .app 里的用那个 .app 的图标，Windows 从 exe 里取。取不到的不在结果里。
#[tauri::command]
async fn process_icons(app: AppHandle, exes: Vec<String>) -> CmdResult<HashMap<String, String>> {
    blocking(app, move |_app, state| {
        let known: HashMap<String, Option<String>> = state.process_icons.lock().map(|m| m.clone()).unwrap_or_default();
        let mut out = HashMap::new();
        let mut found = Vec::new();
        for exe in exes {
            let icon = match known.get(&exe) {
                Some(icon) => icon.clone(),
                None => {
                    let icon = shibapack_core::system::process_icon(Path::new(&exe), &state.icon_cache);
                    found.push((exe.clone(), icon.clone()));
                    icon
                }
            };
            if let Some(icon) = icon {
                out.insert(exe, icon);
            }
        }
        if let Ok(mut cache) = state.process_icons.lock() {
            cache.extend(found);
        }
        Ok(out)
    })
    .await
}

/// 结束一个进程：`force` 为假时请它正常退出。ShibaPack 自己、系统进程、别的用户的不行。
#[tauri::command]
async fn end_process(app: AppHandle, pid: u32, force: bool) -> CmdResult<()> {
    blocking(app, move |_app, state| with_monitor(&state, |monitor| monitor.end(pid, force))).await
}

// ── 窗口外观 ─────────────────────────────────────────────────

/// 毛玻璃：窗口背景透出后面的桌面并模糊。macOS 用系统的 vibrancy，Windows 11 用 Mica；别的系统不支持，返回 false。
/// 窗口本身一直是透明的（tauri.conf.json 的 transparent），关掉时界面画不透明的底色，看起来和普通窗口一样。
#[tauri::command]
fn set_window_glass(window: tauri::WebviewWindow, enabled: bool) -> CmdResult<bool> {
    use tauri::window::{Effect, EffectState, EffectsBuilder};
    if !enabled {
        window.set_effects(None).map_err(|e| e.to_string())?;
        return Ok(false);
    }
    let effects = if cfg!(target_os = "macos") {
        EffectsBuilder::new().effect(Effect::UnderWindowBackground).state(EffectState::FollowsWindowActiveState).build()
    } else if cfg!(windows) {
        EffectsBuilder::new().effect(Effect::Mica).build()
    } else {
        return Ok(false);
    };
    window.set_effects(effects).map_err(|e| e.to_string())?;
    Ok(true)
}

// ── 磁盘分析 ─────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiskLocation {
    path: String,
    name: String,
    /// home：用户文件夹；disk：一块盘。
    kind: String,
    total: u64,
    available: u64,
}

// ───────────────────────────── 安装监控 ─────────────────────────────

/// 一次安装监控。`pending` 同时存在磁盘上（见 `trace::persist`），重启电脑后从那里接着来。
struct TraceSession {
    pending: trace::persist::Pending,
    /// 基线快照。重启后是 None，完成时再从磁盘读。
    baseline: Option<trace::Snapshot>,
    /// 实时监听文件改动（开不了时是 None）。
    live: Option<trace::live::LiveWatch>,
    /// 这是 ShibaPack 重新打开后接着的监控（多半是安装程序要求重启了电脑）。
    resumed: bool,
}

/// 没完成的监控存在哪：应用自己的数据文件夹（只和这台电脑有关）。
fn trace_pending_dir(app: &AppHandle) -> PathBuf {
    app.path().app_local_data_dir().unwrap_or_else(|_| std::env::temp_dir().join("ShibaPack")).join("trace")
}

/// 启动时：上次有没完成的监控（比如重启了电脑）就接着监听。
fn resume_trace(app: &AppHandle) -> Option<TraceSession> {
    let pending = trace::persist::load(&trace_pending_dir(app))?;
    let live = trace::live::LiveWatch::start(&pending.scope);
    Some(TraceSession { pending, baseline: None, live, resumed: true })
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TraceProgress {
    /// baseline：开始前拍基线；compare：装完拍第二次。
    stage: &'static str,
    entries: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TraceStatus {
    software_id: String,
    version_id: String,
    software_name: String,
    version: String,
    file_name: String,
    elapsed_ms: u64,
    /// 安装程序还在不在跑；看不出来时（.dmg 这种）是 null。
    installer_running: Option<bool>,
    /// 到现在为止看到的文件改动；监听不了时是 null。
    live: Option<trace::live::LiveSummary>,
    /// 安装程序要求重启电脑（Windows）。
    reboot_required: bool,
    /// ShibaPack 重新打开后接着的监控。
    resumed: bool,
}

/// 拍快照时一直占着“正在拍”的开关，离开时放下。
struct BusyGuard<'a>(&'a AtomicBool);
impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn trace_busy<'a>(state: &'a State<AppState>) -> CmdResult<BusyGuard<'a>> {
    if state.trace_busy.swap(true, Ordering::SeqCst) {
        return Err("trace_busy".into());
    }
    state.trace_cancel.store(false, Ordering::SeqCst);
    Ok(BusyGuard(&state.trace_busy))
}

/// 开始监控安装：先拍基线（发 `trace://progress`），拍完开始实时监听、把这次监控存到磁盘，再运行安装包。
/// `deep` 时范围大一些。
#[tauri::command]
async fn trace_start(app: AppHandle, software_id: String, version_id: String, file_id: String, deep: bool) -> CmdResult<()> {
    blocking(app, move |app, state| {
        if state.trace.lock().map_err(|_| "internal".to_string())?.is_some() {
            return Err("trace_busy".into());
        }
        let _busy = trace_busy(&state)?;
        let (installer, software_name, version, file_name, root, data_dir) = with_repo(&state, |repo| {
            let path = repo.file_path(&software_id, &version_id, &file_id)?;
            let software = repo.lib.software.iter().find(|s| s.id == software_id).ok_or(shibapack_core::Error::Code("not_found"))?;
            let version = software.versions.iter().find(|v| v.id == version_id).ok_or(shibapack_core::Error::Code("not_found"))?;
            let file = version.files.iter().find(|f| f.id == file_id).ok_or(shibapack_core::Error::Code("not_found"))?;
            Ok((path, software.name.clone(), version.version.clone(), file.name.clone(), repo.root().to_path_buf(), repo.data_dir().join("traces")))
        })?;
        if !installer.exists() {
            return Err("file_missing".into());
        }
        // 不扫 ShibaPack 自己的东西：仓库、应用数据、缓存、日志
        let mut exclude = vec![root, state.icon_cache.clone(), state.log_dir.clone()];
        for dir in [app.path().app_data_dir(), app.path().app_cache_dir(), app.path().app_config_dir(), app.path().app_local_data_dir()].into_iter().flatten() {
            exclude.push(dir);
        }
        let depth = if deep { trace::Depth::Deep } else { trace::Depth::Standard };
        let scope = trace::scope(depth, exclude);
        let baseline = trace::snapshot(&scope, &state.trace_cancel, &|entries| {
            let _ = app.emit("trace://progress", TraceProgress { stage: "baseline", entries });
        })
        .ok_or_else(|| "cancelled".to_string())?;

        let target = import::launch_target(&installer);
        let pending = trace::persist::Pending {
            software_id,
            version_id,
            software_name,
            version,
            file_name,
            depth,
            msi: trace::products::msi_info(&target),
            reboot_marker: trace::persist::reboot_marker(),
            scope,
            installer: target.clone(),
            data_dir,
            started_at: shibapack_core::now(),
            started_ms: now_ms(),
            baseline_ms: baseline.elapsed_ms,
        };
        // 存不下来也照样监控，只是重启电脑后就接不上了
        if let Err(err) = trace::persist::save(&trace_pending_dir(app), &pending, &baseline) {
            log_failure_to(&state, "trace", "save pending", &err.to_string());
        }
        let live = trace::live::LiveWatch::start(&pending.scope);
        if let Err(err) = app.opener().open_path(target.to_string_lossy(), None::<&str>) {
            trace::persist::clear(&trace_pending_dir(app));
            return Err(err.to_string());
        }
        *state.trace.lock().map_err(|_| "internal".to_string())? = Some(TraceSession { pending, baseline: Some(baseline), live, resumed: false });
        open_trace_mini(app);
        Ok(())
    })
    .await
}

/// 正在监控的安装（没有时是 null）：装的是什么、过了多久、安装程序还在不在跑、实时看到的改动。
#[tauri::command]
async fn trace_status(app: AppHandle) -> Option<TraceStatus> {
    blocking(app, move |_app, state| {
        let (status, installer, marker) = {
            let slot = state.trace.lock().ok()?;
            let session = slot.as_ref()?;
            let p = &session.pending;
            (
                TraceStatus {
                    software_id: p.software_id.clone(),
                    version_id: p.version_id.clone(),
                    software_name: p.software_name.clone(),
                    version: p.version.clone(),
                    file_name: p.file_name.clone(),
                    elapsed_ms: (now_ms() - p.started_ms).max(0) as u64,
                    installer_running: None,
                    live: session.live.as_ref().map(|live| live.summary()),
                    reboot_required: false,
                    resumed: session.resumed,
                },
                p.installer.clone(),
                p.reboot_marker,
            )
        };
        Some(TraceStatus {
            installer_running: trace::installer_running(&installer),
            reboot_required: trace::persist::reboot_required(marker),
            ..status
        })
    })
    .await
}

/// 装完了：再拍一次（发 `trace://progress`），对比，用安装程序自己登记的清单确认，存下记录并返回。
/// 取消了时监控还在，可以再点完成。
#[tauri::command]
async fn trace_finish(app: AppHandle) -> CmdResult<trace::Trace> {
    blocking(app, move |app, state| {
        let _busy = trace_busy(&state)?;
        let mut session = state.trace.lock().map_err(|_| "internal".to_string())?.take().ok_or_else(|| "not_found".to_string())?;
        let put_back = |session: TraceSession| {
            if let Ok(mut slot) = state.trace.lock() {
                *slot = Some(session);
            }
        };
        let pending_dir = trace_pending_dir(app);
        let baseline = match session.baseline.take() {
            Some(baseline) => baseline,
            None => match trace::persist::load_baseline(&pending_dir) {
                Ok(baseline) => baseline,
                Err(err) => {
                    put_back(session);
                    return Err(err.to_string());
                }
            },
        };
        let started = Instant::now();
        let Some(after) = trace::snapshot(&session.pending.scope, &state.trace_cancel, &|entries| {
            let _ = app.emit("trace://progress", TraceProgress { stage: "compare", entries });
        }) else {
            session.baseline = Some(baseline);
            put_back(session);
            return Err("cancelled".into());
        };
        let p = &session.pending;
        let (mut changes, truncated) = trace::diff(&baseline, &after);
        let products = trace::products::apply(&mut changes, &baseline, p.msi.as_ref(), p.started_ms);
        trace::fill_details(&mut changes);
        let (transient, transient_count) = session.live.as_ref().map(|live| live.transient()).unwrap_or_default();
        let record = trace::Trace {
            id: shibapack_core::new_id(),
            software_id: p.software_id.clone(),
            version_id: p.version_id.clone(),
            software_name: p.software_name.clone(),
            version: p.version.clone(),
            file_name: p.file_name.clone(),
            platform: std::env::consts::OS.to_string(),
            depth: p.depth,
            started_at: p.started_at.clone(),
            finished_at: shibapack_core::now(),
            baseline_ms: p.baseline_ms,
            compare_ms: started.elapsed().as_millis() as u64,
            entries: after.entries,
            changes,
            truncated,
            products,
            transient,
            transient_count,
            reboot_required: trace::persist::reboot_required(p.reboot_marker),
            resumed: session.resumed,
        };
        if let Err(err) = trace::save(&p.data_dir, &record) {
            session.baseline = Some(baseline);
            put_back(session);
            return Err(err.to_string());
        }
        trace::persist::clear(&pending_dir);
        close_trace_mini(app);
        Ok(record)
    })
    .await
}

/// 不监控了：正在拍快照就停下；已经在等安装时直接丢掉基线（磁盘上存的也删掉）。
#[tauri::command]
fn trace_cancel(app: AppHandle, state: State<AppState>) {
    state.trace_cancel.store(true, Ordering::SeqCst);
    if !state.trace_busy.load(Ordering::SeqCst) {
        if let Ok(mut slot) = state.trace.lock() {
            *slot = None;
        }
        trace::persist::clear(&trace_pending_dir(&app));
        close_trace_mini(&app);
    }
}

/// 浮动小窗上点了按钮：把主窗口叫到前面，告诉它点的是什么（`finish` 或 `open`）。
#[tauri::command]
fn trace_mini_action(app: AppHandle, action: String) -> CmdResult<()> {
    if action != "finish" && action != "open" {
        return Err("not_supported".into());
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
    app.emit_to("main", "trace://mini-action", action).map_err(|e| e.to_string())
}

/// 打开安装时的浮动小窗：总在最前面，放在屏幕右上角，显示实时改动，可以直接点“完成”。已经开着就不动。
/// 监控开始（或启动时接着上次的监控）时打开，完成或放弃时关掉（`close_trace_mini`），都由这边管，界面不用操心。
fn open_trace_mini(app: &AppHandle) {
    if app.get_webview_window("trace-mini").is_some() {
        return;
    }
    const SIZE: (f64, f64) = (320.0, 184.0);
    let mut builder = tauri::WebviewWindowBuilder::new(app, "trace-mini", tauri::WebviewUrl::App("index.html?mini=trace".into()))
        .title("ShibaPack")
        .inner_size(SIZE.0, SIZE.1)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false);
    if let Ok(Some(monitor)) = app.primary_monitor() {
        let scale = monitor.scale_factor();
        let size = monitor.size().to_logical::<f64>(scale);
        let origin = monitor.position().to_logical::<f64>(scale);
        builder = builder.position(origin.x + size.width - SIZE.0 - 24.0, origin.y + 48.0);
    }
    if let Err(err) = builder.build() {
        eprintln!("trace mini window: {err}");
    }
}

fn close_trace_mini(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("trace-mini") {
        let _ = window.close();
    }
}

/// 重新打开浮动小窗（关掉以后从监控窗口里再叫出来）。没有正在进行的监控时不开。
#[tauri::command]
async fn trace_mini(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    if state.trace.lock().map_err(|_| "internal".to_string())?.is_some() {
        open_trace_mini(&app);
    }
    Ok(())
}

fn traces_dir(state: &State<AppState>) -> CmdResult<PathBuf> {
    let guard = state.repo.lock().map_err(|_| "internal".to_string())?;
    let repo = guard.as_ref().ok_or_else(|| "not_configured".to_string())?;
    Ok(repo.data_dir().join("traces"))
}

/// 安装记录列表（新的在前）。给了软件 id 只列这个软件的。
#[tauri::command]
async fn list_traces(app: AppHandle, software_id: Option<String>) -> CmdResult<Vec<trace::TraceSummary>> {
    blocking(app, move |_app, state| Ok(trace::list(&traces_dir(&state)?, software_id.as_deref()))).await
}

#[tauri::command]
async fn get_trace(app: AppHandle, software_id: String, id: String) -> CmdResult<trace::Trace> {
    blocking(app, move |_app, state| trace::load(&traces_dir(&state)?, &software_id, &id).map_err(|e| e.to_string())).await
}

#[tauri::command]
async fn delete_trace(app: AppHandle, software_id: String, id: String) -> CmdResult<()> {
    blocking(app, move |_app, state| trace::delete(&traces_dir(&state)?, &software_id, &id).map_err(|e| e.to_string())).await
}

/// 在文件管理器里定位记录里的一项（按序号，不接受前端传来的路径）。已经不在了就定位到还在的上一级。
#[tauri::command]
async fn trace_reveal(app: AppHandle, software_id: String, id: String, index: usize) -> CmdResult<()> {
    blocking(app, move |app, state| {
        let record = trace::load(&traces_dir(&state)?, &software_id, &id).map_err(|e| e.to_string())?;
        let change = record.changes.get(index).ok_or_else(|| "not_found".to_string())?;
        if !matches!(change.kind, trace::ItemKind::File | trace::ItemKind::Folder) {
            return Err("not_found".into());
        }
        let mut path = PathBuf::from(&change.path);
        while !path.exists() {
            path = path.parent().ok_or_else(|| "file_missing".to_string())?.to_path_buf();
        }
        app.opener().reveal_item_in_dir(path).map_err(|e| e.to_string())
    })
    .await
}

/// 可以分析的地方：用户文件夹和每一块盘。
#[tauri::command]
async fn disk_locations(app: AppHandle) -> Vec<DiskLocation> {
    blocking(app, move |app, _state| {
        let mut out = Vec::new();
        if let Ok(home) = app.path().home_dir() {
            out.push(DiskLocation { name: home.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), path: home.to_string_lossy().to_string(), kind: "home".into(), total: 0, available: 0 });
        }
        for mount in shibapack_core::disk::mounts() {
            if out.iter().any(|l: &DiskLocation| l.path == mount.path) {
                continue;
            }
            out.push(DiskLocation { name: mount.name, path: mount.path, kind: "disk".into(), total: mount.total, available: mount.available });
        }
        out
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiskSummary {
    root: String,
    size: u64,
    files: u64,
    denied: u64,
    denied_examples: Vec<String>,
    elapsed_ms: u64,
}

/// 边扫边看时最大的文件留几个。
const LARGEST_LIVE: usize = 200;

/// 扫描中的样子（没在扫时是 null）：`path` 只能是根或第一层的文件夹，还没扫完的项带 `pending`。
#[tauri::command]
async fn disk_live_view(app: AppHandle, path: Vec<String>, min_ratio: f64) -> Option<shibapack_core::disk::ViewNode> {
    blocking(app, move |_app, state| {
        let live = state.disk_live.lock().ok()?.clone()?;
        live.view(&path, min_ratio.clamp(0.0, 0.5))
    })
    .await
}

/// 扫描中到现在为止最大的文件（没在扫时是空的）。
#[tauri::command]
async fn disk_live_largest(app: AppHandle) -> Vec<LargeFile> {
    blocking(app, move |_app, state| {
        let Some(live) = state.disk_live.lock().ok().and_then(|slot| slot.clone()) else { return Vec::new() };
        let root = live.root().to_path_buf();
        large_files(&root, live.largest())
    })
    .await
}

/// 扫一个文件夹或一块盘（只读）。扫的时候发 `disk://progress`，界面可以用 `disk_live_view` 先看；停下了返回 `cancelled`。
#[tauri::command]
async fn disk_scan(app: AppHandle, root: String) -> CmdResult<DiskSummary> {
    blocking(app, move |app, state| {
        let root = PathBuf::from(&root);
        if !root.is_dir() {
            return Err("file_missing".into());
        }
        state.disk_cancel.store(false, Ordering::SeqCst);
        let live = Arc::new(shibapack_core::disk::Live::new(&root, LARGEST_LIVE));
        if let Ok(mut slot) = state.disk_live.lock() {
            *slot = Some(Arc::clone(&live));
        }
        let scan = shibapack_core::disk::scan(
            &root,
            &state.disk_cancel,
            &|progress| {
                let _ = app.emit("disk://progress", progress);
            },
            Some(&live),
        );
        if let Ok(mut slot) = state.disk_live.lock() {
            *slot = None;
        }
        let scan = scan.ok_or_else(|| "cancelled".to_string())?;
        let summary = DiskSummary {
            root: scan.root.to_string_lossy().to_string(),
            size: scan.tree.size,
            files: scan.tree.files,
            denied: scan.denied,
            denied_examples: scan.denied_examples.clone(),
            elapsed_ms: scan.elapsed_ms,
        };
        if let Ok(mut slot) = state.disk.lock() {
            *slot = Some(scan);
        }
        Ok(summary)
    })
    .await
}

#[tauri::command]
fn disk_cancel(state: State<AppState>) {
    state.disk_cancel.store(true, Ordering::SeqCst);
}

/// 从扫完的树里取一块给界面画：`path` 是相对扫描根的路径段，往下 `depth` 层，太小的合成“其他”。
#[tauri::command]
async fn disk_view(app: AppHandle, path: Vec<String>, depth: usize, min_ratio: f64) -> CmdResult<shibapack_core::disk::ViewNode> {
    blocking(app, move |_app, state| {
        let slot = state.disk.lock().map_err(|_| "internal".to_string())?;
        let scan = slot.as_ref().ok_or_else(|| "not_found".to_string())?;
        shibapack_core::disk::view(&scan.tree, &path, depth.min(8), min_ratio.clamp(0.0, 0.5)).ok_or_else(|| "not_found".to_string())
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LargeFile {
    /// 相对扫描根的路径段，定位、移到回收站都按它。
    path: Vec<String>,
    name: String,
    size: u64,
    /// 所在文件夹（用户文件夹写成 ~）。
    folder: String,
    /// 修改时间，从 1970 年起的毫秒（读不到时是 null）。
    modified_ms: Option<u64>,
}

/// 磁盘分析：`path` 那个文件夹里（连同下面所有层）最大的 `limit` 个文件，从大到小。只从扫完的树里取，不重新扫。
#[tauri::command]
async fn disk_largest(app: AppHandle, path: Vec<String>, limit: usize) -> CmdResult<Vec<LargeFile>> {
    blocking(app, move |_app, state| {
        let (root, found) = {
            let slot = state.disk.lock().map_err(|_| "internal".to_string())?;
            let scan = slot.as_ref().ok_or_else(|| "not_found".to_string())?;
            let found = shibapack_core::disk::largest_files(&scan.tree, &path, limit.min(1000)).ok_or_else(|| "not_found".to_string())?;
            (scan.root.clone(), found)
        };
        Ok(large_files(&root, found))
    })
    .await
}

/// 给界面的大文件：所在文件夹（用户文件夹写成 ~）和修改时间。
fn large_files(root: &Path, found: Vec<(Vec<String>, u64)>) -> Vec<LargeFile> {
    let home = if cfg!(windows) { std::env::var_os("USERPROFILE") } else { std::env::var_os("HOME") }.map(PathBuf::from);
    found
        .into_iter()
        .map(|(path, size)| {
            let full = shibapack_core::disk::full_path(root, &path);
            let modified_ms = fs::metadata(&full)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64);
            let folder = full.parent().map(|p| shibapack_core::disk::display_path(p, home.as_deref())).unwrap_or_default();
            LargeFile { name: path.last().cloned().unwrap_or_default(), path, size, folder, modified_ms }
        })
        .collect()
}

/// 磁盘分析里文件夹（或大文件）是做什么的。`mode` 是 `cached`（只看缓存，不问 AI）或 `ask`（没记下的问 AI）。
/// 缓存单独放在仓库的 `.shibapack/cache/ai-folders.json`。返回：路径段用 / 连起来 → 解释。
#[tauri::command]
async fn ai_explain_folders(app: AppHandle, paths: Vec<Vec<String>>, mode: String) -> CmdResult<HashMap<String, String>> {
    blocking(app, move |_app, state| {
        let home = if cfg!(windows) { std::env::var_os("USERPROFILE") } else { std::env::var_os("HOME") }.map(PathBuf::from);
        // （界面上的键，缓存的键，问 AI 时给的事实）
        let items: Vec<(String, String, String)> = {
            let slot = state.disk.lock().map_err(|_| "internal".to_string())?;
            let scan = slot.as_ref().ok_or_else(|| "not_found".to_string())?;
            paths
                .iter()
                .filter_map(|path| {
                    let facts = shibapack_core::disk::facts(&scan.root, &scan.tree, path, home.as_deref())?;
                    let shown = shibapack_core::disk::display_path(&shibapack_core::disk::full_path(&scan.root, path), home.as_deref());
                    Some((path.join("/"), shown, facts))
                })
                .collect()
        };
        let dir = cache_dir(&state)?;
        let language = with_repo(&state, |repo| Ok(repo.lib.settings.language.clone()))?;
        let key_of = |shown: &str| aicache::folder_key(std::env::consts::OS, shown, &language);
        let mut out = HashMap::new();
        let mut todo = Vec::new();
        {
            let _turn = state.ai_cache_turn.lock().unwrap_or_else(|e| e.into_inner());
            let cache = aicache::FolderCache::load(&dir);
            for item in &items {
                match cache.folders.get(&key_of(&item.1)) {
                    Some(hit) => {
                        out.insert(item.0.clone(), hit.text.clone());
                    }
                    None => todo.push(item),
                }
            }
        }
        if mode != "ask" || todo.is_empty() {
            return Ok(out);
        }
        let (service, library) = repo_ai(&state)?;
        let questions: Vec<(String, String)> = todo.iter().map(|item| (item.0.clone(), item.2.clone())).collect();
        let cancel = AtomicBool::new(false);
        let answers = ai::explain_folders_many(&service, &library.settings.language, &questions, &cancel, &mut |_, _| {}).map_err(|e| e.code())?;
        let at = shibapack_core::now();
        let model = service.model().to_string();
        let _turn = state.ai_cache_turn.lock().unwrap_or_else(|e| e.into_inner());
        let mut cache = aicache::FolderCache::load(&dir);
        for (key, text) in answers {
            let Some(item) = todo.iter().find(|item| item.0 == key) else { continue };
            cache.folders.insert(key_of(&item.1), CachedText { text: text.clone(), model: model.clone(), at: at.clone() });
            out.insert(key, text);
        }
        let _ = cache.save(&dir);
        Ok(out)
    })
    .await
}

/// 扫过的树里某一项的完整路径（不在树里的不认）。
fn disk_path(state: &State<AppState>, path: &[String]) -> CmdResult<(PathBuf, PathBuf)> {
    let slot = state.disk.lock().map_err(|_| "internal".to_string())?;
    let scan = slot.as_ref().ok_or_else(|| "not_found".to_string())?;
    shibapack_core::disk::find(&scan.tree, path).ok_or_else(|| "not_found".to_string())?;
    Ok((shibapack_core::disk::full_path(&scan.root, path), scan.root.clone()))
}

/// 在文件管理器里显示（`open` 为真时用默认程序打开）。
#[tauri::command]
async fn disk_reveal(app: AppHandle, path: Vec<String>, open: bool) -> CmdResult<()> {
    blocking(app, move |app, state| {
        let (full, _) = disk_path(&state, &path)?;
        if !full.exists() {
            return Err("file_missing".into());
        }
        if open {
            app.opener().open_path(full.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
        } else {
            app.opener().reveal_item_in_dir(&full).map_err(|e| e.to_string())
        }
    })
    .await
}

/// 某一项的完整路径，复制用。
#[tauri::command]
async fn disk_full_path(app: AppHandle, path: Vec<String>) -> CmdResult<String> {
    blocking(app, move |_app, state| Ok(disk_path(&state, &path)?.0.to_string_lossy().to_string())).await
}

/// 移到回收站（废纸篓），并从树上去掉。系统文件夹、用户文件夹本身、应用程序、仓库这些不行，返回 `protected:<原因>`。
#[tauri::command]
async fn disk_trash(app: AppHandle, path: Vec<String>) -> CmdResult<u64> {
    blocking(app, move |_app, state| {
        let (full, root) = disk_path(&state, &path)?;
        let repo = state.repo.lock().ok().and_then(|r| r.as_ref().map(|r| r.root().to_path_buf()));
        if let Some(reason) = shibapack_core::disk::protected(&full, &root, repo.as_deref()) {
            return Err(format!("disk_protected:{reason}"));
        }
        trash::delete(&full).map_err(|e| e.to_string())?;
        let mut slot = state.disk.lock().map_err(|_| "internal".to_string())?;
        let scan = slot.as_mut().ok_or_else(|| "not_found".to_string())?;
        Ok(shibapack_core::disk::remove(&mut scan.tree, &path).unwrap_or(0))
    })
    .await
}

// ── 启动项与服务 ─────────────────────────────────────────────

/// 本机的应用数据文件夹（macOS 在这里记停用的登录项）。只和这台电脑有关，所以不放在仓库里。
fn local_state_dir(app: &AppHandle) -> CmdResult<PathBuf> {
    app.path().app_local_data_dir().map_err(|e| e.to_string())
}

/// 重新列出启动项和服务，记下来供之后按 id 查。
fn relist_startup(app: &AppHandle, state: &State<AppState>) -> CmdResult<Listing> {
    let apps = state.installed.lock().map(|list| list.clone()).unwrap_or_default();
    // 自检时不列登录项：要通过“系统事件”，会弹出授权提示把自检卡住
    let listing = startup::list(&state.icon_cache, &local_state_dir(app)?, &apps, state.selftest.is_none());
    if let Ok(mut cached) = state.startup.lock() {
        *cached = listing.items.clone();
    }
    Ok(listing)
}

fn find_startup(state: &State<AppState>, id: &str) -> CmdResult<StartupItem> {
    let list = state.startup.lock().map_err(|_| "internal".to_string())?;
    list.iter().find(|item| item.id == id).cloned().ok_or_else(|| "not_found".to_string())
}

/// 列出这台电脑上的启动项和服务。
#[tauri::command]
async fn list_startup(app: AppHandle) -> CmdResult<Listing> {
    blocking(app, move |app, state| {
        let _turn = state.startup_turn.lock().unwrap_or_else(|e| e.into_inner());
        relist_startup(app, &state)
    })
    .await
}

/// 启用、停用、启动、停止、重启一项，或改服务的启动类型（`type:auto` 这样）。做完重新列一遍交回去。
/// 需要管理员权限时会弹出系统的授权提示，用户取消时返回 `cancelled`。
#[tauri::command]
async fn startup_action(app: AppHandle, id: String, action: String) -> CmdResult<Listing> {
    blocking(app, move |app, state| {
        let _turn = state.startup_turn.lock().unwrap_or_else(|e| e.into_inner());
        let action = startup::Action::parse(&action).ok_or_else(|| "not_supported".to_string())?;
        let item = find_startup(&state, &id)?;
        let result = startup::apply(&item, &action, &local_state_dir(app)?);
        if let Err(err) = &result {
            log_failure_to(&state, "startup", err, &format!("{action:?} · {}", item.label));
        }
        // 失败了也重新列一遍：有的改动可能做了一半（比如停用成功、停止失败）
        let listing = relist_startup(app, &state)?;
        result.map(|()| listing)
    })
    .await
}

/// 在文件管理器里定位：先找程序文件，没有就找登记它的文件（plist、快捷方式、.desktop、单元文件）。
#[tauri::command]
async fn reveal_startup(app: AppHandle, id: String) -> CmdResult<()> {
    blocking(app, move |app, state| {
        let item = find_startup(&state, &id)?;
        let target = item
            .program
            .clone()
            .filter(|path| path.exists())
            .or_else(|| Some(PathBuf::from(&item.location)).filter(|path| path.is_absolute() && path.exists()))
            .ok_or_else(|| "file_missing".to_string())?;
        app.opener().reveal_item_in_dir(target).map_err(|e| e.to_string())
    })
    .await
}

/// 打开系统自己管理启动项的设置页。`page` 是 `loginItems`（登录项）或 `automation`（允许控制“系统事件”，macOS 列登录项要用）。
#[tauri::command]
fn open_startup_settings(app: AppHandle, page: String) -> CmdResult<()> {
    let url = match (page.as_str(), std::env::consts::OS) {
        ("automation", "macos") => "x-apple.systempreferences:com.apple.preference.security?Privacy_Automation",
        ("loginItems", "macos") => "x-apple.systempreferences:com.apple.LoginItems-Settings.extension",
        ("loginItems", "windows") => "ms-settings:startupapps",
        // macOS 的“App 管理”：没有苹果开发者签名的应用要在这里放行，才能更新自己
        ("appManagement", "macos") => "x-apple.systempreferences:com.apple.preference.security?Privacy_AppBundles",
        // 清理时量废纸篓要“完全磁盘访问权限”
        ("fullDiskAccess", "macos") => "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles",
        ("storage", "macos") => "x-apple.systempreferences:com.apple.settings.Storage",
        ("storage", "windows") => "ms-settings:storagesense",
        _ => return Err("not_supported".into()),
    };
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

// ── 收藏的路径 ───────────────────────────────────────────────

use shibapack_core::{paths::{self, PathBackup, PathState}, SavedPath};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PathsAdded {
    library: Library,
    added: Vec<String>,
}

/// 一条收藏、它的备份文件夹、仓库根目录。拿到就放开仓库：备份、恢复可能要复制一会儿。
fn saved_path(state: &State<AppState>, id: &str) -> CmdResult<(SavedPath, PathBuf, PathBuf)> {
    with_repo(state, |repo| {
        let item = repo.lib.paths.iter().find(|p| p.id == id).cloned().ok_or(shibapack_core::Error::Code("not_found"))?;
        let dir = paths::backups_root(repo).join(&item.id);
        Ok((item, dir, repo.root().to_path_buf()))
    })
}

/// 收藏一批路径（拖进来的、选的、手动输入的，可以带 `~`、`%APPDATA%`、`$HOME` 这类变量）。
#[tauri::command]
async fn paths_add(app: AppHandle, paths: Vec<String>) -> CmdResult<PathsAdded> {
    blocking(app, move |_, state| {
        with_repo(&state, |repo| {
            let added = paths::add(repo, &paths)?;
            Ok(PathsAdded { library: repo.lib.clone(), added })
        })
    })
    .await
}

#[tauri::command]
async fn path_edit(app: AppHandle, id: String, path: Option<String>, note: Option<String>) -> CmdResult<Library> {
    blocking(app, move |_, state| mutate(&state, |repo| paths::edit(repo, &id, path.as_deref(), note.as_deref()))).await
}

#[tauri::command]
async fn paths_remove(app: AppHandle, ids: Vec<String>, delete_backups: bool) -> CmdResult<Library> {
    blocking(app, move |_, state| mutate(&state, |repo| paths::remove(repo, &ids, delete_backups))).await
}

/// 每条收藏在这台电脑上的情况：展开后的路径、在不在、有几份备份。
#[tauri::command]
async fn path_states(app: AppHandle) -> CmdResult<HashMap<String, PathState>> {
    blocking(app, move |_, state| {
        let (items, root) = with_repo(&state, |repo| Ok((repo.lib.paths.clone(), paths::backups_root(repo))))?;
        Ok(paths::states(&items, &root).into_iter().collect())
    })
    .await
}

#[tauri::command]
async fn path_backup(app: AppHandle, id: String) -> CmdResult<PathBackup> {
    blocking(app, move |_, state| {
        let (item, dir, root) = saved_path(&state, &id)?;
        let result = paths::backup(&item, &dir, &root, false).map_err(|e| e.to_string());
        if let Err(err) = &result {
            log_failure_to(&state, "paths", err, &item.path);
        }
        result
    })
    .await
}

#[tauri::command]
async fn path_backups(app: AppHandle, id: String) -> CmdResult<Vec<PathBackup>> {
    blocking(app, move |_, state| {
        let (_, dir, _) = saved_path(&state, &id)?;
        Ok(paths::list_backups(&dir))
    })
    .await
}

/// 用一份备份替换现在的。现在的先自动备份一份，再送进回收站。
#[tauri::command]
async fn path_restore(app: AppHandle, id: String, name: String) -> CmdResult<Option<PathBackup>> {
    blocking(app, move |_, state| {
        let (item, dir, root) = saved_path(&state, &id)?;
        let result = paths::restore(&item, &dir, &name, &root).map_err(|e| e.to_string());
        if let Err(err) = &result {
            log_failure_to(&state, "paths", err, &format!("{} · {name}", item.path));
        }
        result
    })
    .await
}

#[tauri::command]
async fn path_delete_backup(app: AppHandle, id: String, name: String) -> CmdResult<Vec<PathBackup>> {
    blocking(app, move |_, state| {
        let (_, dir, _) = saved_path(&state, &id)?;
        paths::delete_backup(&dir, &name).map_err(|e| e.to_string())?;
        Ok(paths::list_backups(&dir))
    })
    .await
}

/// 在文件管理器里显示（`open` 时直接打开文件夹）。
#[tauri::command]
async fn path_reveal(app: AppHandle, id: String, open: bool) -> CmdResult<()> {
    blocking(app, move |app, state| {
        let (item, _, _) = saved_path(&state, &id)?;
        let full = paths::expand(&item.path).map_err(|e| e.to_string())?;
        if !full.exists() {
            return Err("file_missing".into());
        }
        if open {
            app.opener().open_path(full.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
        } else {
            app.opener().reveal_item_in_dir(full).map_err(|e| e.to_string())
        }
    })
    .await
}

#[tauri::command]
async fn path_reveal_backup(app: AppHandle, id: String, name: String) -> CmdResult<()> {
    blocking(app, move |app, state| {
        let (_, dir, _) = saved_path(&state, &id)?;
        let target = paths::backup_content(&dir, &name).map_err(|e| e.to_string())?;
        app.opener().reveal_item_in_dir(target).map_err(|e| e.to_string())
    })
    .await
}

// ── 清理 ─────────────────────────────────────────────────────

/// 仓库里的每个文件（大小、SHA-256、软件名、版本号），拿来和下载文件夹里的安装包比对。
fn library_files(lib: &Library) -> Vec<cleanup::LibraryFile> {
    lib.software
        .iter()
        .flat_map(|s| {
            s.versions.iter().flat_map(move |v| {
                v.files.iter().map(move |f| cleanup::LibraryFile { size: f.size, sha256: f.sha256.clone(), software: s.name.clone(), version: v.version.clone() })
            })
        })
        .collect()
}

/// 扫描和清理要用到的：仓库在哪（整个封住不碰）、仓库里下载用的临时文件夹、仓库里的文件、用户的设置。
fn cleanup_context(app: &AppHandle, state: &State<AppState>) -> CmdResult<cleanup::Context> {
    let (root, downloads, library) = {
        let guard = state.repo.lock().map_err(|_| "internal".to_string())?;
        match guard.as_ref() {
            Some(repo) => (Some(repo.root().to_path_buf()), Some(repo.downloads_dir()), library_files(&repo.lib)),
            None => (None, None, Vec::new()),
        }
    };
    let local = local_state_dir(app)?;
    let options = cleanup::load_options(&local);
    let mut ctx = cleanup::Context::new(root.as_deref(), downloads, library, options);
    // 用户点过“看看废纸篓”并允许了，扫描时才问访达（否则第一次扫描就会弹授权）；自检时不问
    ctx.ask_finder = ctx.options.ask_finder && state.selftest.is_none();
    ctx.cache_dir = Some(local);
    ctx.installed = state
        .installed
        .lock()
        .map(|apps| apps.iter().filter(|app| !app.location.is_empty()).map(|app| (app.name.clone(), PathBuf::from(&app.location))).collect())
        .unwrap_or_default();
    Ok(ctx)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CleanupProgress {
    done: usize,
    total: usize,
    /// 正在等用户在系统的授权提示里输入密码或点“是”。
    waiting_admin: bool,
}

/// 只读地找一遍能清理的东西。每量完一项通过 `cleanup://item` 发给界面，最后返回全部。
#[tauri::command]
async fn cleanup_scan(app: AppHandle) -> CmdResult<Vec<cleanup::CleanItem>> {
    blocking(app, move |app, state| {
        let _turn = state.cleanup_turn.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = cleanup_context(app, &state)?;
        state.cleanup_cancel.store(false, Ordering::SeqCst);
        let scan = cleanup::scan_with(
            &ctx,
            &|item| {
                let _ = app.emit("cleanup://item", item);
            },
            &|done, total| {
                let _ = app.emit("cleanup://scan", CleanupProgress { done, total, waiting_admin: false });
            },
            &state.cleanup_cancel,
        );
        let items = scan.items();
        if let Ok(mut cached) = state.cleanup.lock() {
            *cached = Some(scan);
        }
        Ok(items)
    })
    .await
}

/// 清理勾选的项：每一项给 id 和不要的条目序号。删之前每一条都再查一遍。进度通过 `cleanup://progress` 发给界面。
/// 做完写一份清理记录，放在本机应用数据目录的 `cleanup-history/`。
#[tauri::command]
async fn cleanup_run(app: AppHandle, picks: Vec<cleanup::Pick>) -> CmdResult<cleanup::Report> {
    blocking(app, move |app, state| {
        let _turn = state.cleanup_turn.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = cleanup_context(app, &state)?;
        let scan = state.cleanup.lock().ok().and_then(|scan| scan.clone()).ok_or_else(|| "not_found".to_string())?;
        // 清理前后各看一眼系统报告的可用空间，和我们自己算的对一对
        let free = || ctx.vars.get("home").and_then(cleanup::special::free_space);
        let disk_before = free();
        // 要管理员权限的项另外交给提权的进程，一次授权做完
        let (admin, normal): (Vec<cleanup::Pick>, Vec<cleanup::Pick>) = picks.into_iter().partition(|pick| scan.is_admin(&pick.id));
        let (mut report, mut run) = cleanup::clean(&scan, &normal, &ctx, &|done, total| {
            let _ = app.emit("cleanup://progress", CleanupProgress { done, total, waiting_admin: false });
        });
        if !admin.is_empty() {
            let ids: Vec<String> = admin.iter().map(|pick| pick.id.clone()).collect();
            let _ = app.emit("cleanup://progress", CleanupProgress { done: 0, total: 0, waiting_admin: true });
            match cleanup::elevate::clean_as_admin(&ids, scan.skipped_paths(&admin), &ctx.options, ctx.repo_root.as_deref()) {
                Ok((elevated, elevated_run)) => {
                    report.merge(elevated);
                    run.freed += elevated_run.freed;
                    run.trashed += elevated_run.trashed;
                    run.skipped += elevated_run.skipped;
                    run.items.extend(elevated_run.items);
                }
                Err(reason) => {
                    if reason != "cancelled" {
                        log_failure_to(&state, "cleanup", &reason, &ids.join(", "));
                    }
                    run.skipped += ids.len();
                    report.refuse(&ids, &reason);
                }
            }
        }
        report.disk_before = disk_before;
        report.disk_after = free();
        if !run.items.is_empty() || run.skipped > 0 {
            report.history = cleanup::history::record(&local_state_dir(app)?.join("cleanup-history"), &run).ok();
        }
        // 扫描结果留着：界面接着用 `cleanup_rescan` 只重新量清过的这几项。
        // 已经不在的条目就算再传回来，删之前的复查也会当作“已经不在了”
        Ok(report)
    })
    .await
}

/// 问访达废纸篓里有几项（macOS 没有完全磁盘访问权限时）。第一次会弹“允许控制访达”的授权。
/// 允许了就记下来，以后扫描时直接问；不允许时返回 `automation_denied`。返回更新后的全部项。
#[tauri::command]
async fn cleanup_count_trash(app: AppHandle) -> CmdResult<Vec<cleanup::CleanItem>> {
    blocking(app, move |app, state| {
        #[cfg(target_os = "macos")]
        {
            if cleanup::special::finder_trash_count().is_none() {
                return Err("automation_denied".into());
            }
            let local = local_state_dir(app)?;
            let mut options = cleanup::load_options(&local);
            options.ask_finder = true;
            cleanup::save_options(&local, &options).map_err(|e| e.to_string())?;
            let _turn = state.cleanup_turn.lock().unwrap_or_else(|e| e.into_inner());
            let ctx = cleanup_context(app, &state)?;
            let mut cached = state.cleanup.lock().map_err(|_| "internal".to_string())?;
            let scan = cached.as_mut().ok_or_else(|| "not_found".to_string())?;
            scan.rescan(&ctx, &["trash.empty".to_string()]);
            Ok(scan.items())
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (app, state);
            Err("not_supported".into())
        }
    })
    .await
}

/// 这一项要在别处清：打开系统的存储设置（Windows.old），或者打开那个应用（微信、飞书……用它自带的清理）。
#[tauri::command]
async fn cleanup_open_tool(app: AppHandle, id: String) -> CmdResult<()> {
    blocking(app, move |app, state| {
        let (tool, root) = {
            let cached = state.cleanup.lock().map_err(|_| "internal".to_string())?;
            let item = cached.as_ref().and_then(|scan| scan.item(&id)).ok_or_else(|| "not_found".to_string())?;
            (item.tool.clone().unwrap_or_default(), item.roots.first().cloned())
        };
        match tool.as_str() {
            "storage" => {
                let url = if cfg!(windows) { "ms-settings:storagesense" } else { "x-apple.systempreferences:com.apple.settings.Storage" };
                app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
            }
            "app" => {
                let path = root.filter(|p| Path::new(p).exists()).ok_or_else(|| "file_missing".to_string())?;
                app.opener().open_path(path, None::<&str>).map_err(|e| e.to_string())
            }
            _ => Err("not_supported".into()),
        }
    })
    .await
}

/// 停下正在进行的扫描：不再量新的，已经量完的照样作为结果（不完整）。
#[tauri::command]
fn cleanup_cancel_scan(state: State<AppState>) {
    state.cleanup_cancel.store(true, Ordering::SeqCst);
}

/// 只重新量这几项（清理完以后），返回更新后的全部项。还没扫过时返回 `not_found`。
#[tauri::command]
async fn cleanup_rescan(app: AppHandle, ids: Vec<String>) -> CmdResult<Vec<cleanup::CleanItem>> {
    blocking(app, move |app, state| {
        let _turn = state.cleanup_turn.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = cleanup_context(app, &state)?;
        let mut cached = state.cleanup.lock().map_err(|_| "internal".to_string())?;
        let scan = cached.as_mut().ok_or_else(|| "not_found".to_string())?;
        scan.rescan(&ctx, &ids);
        Ok(scan.items())
    })
    .await
}

/// 让 AI 解释几个清理项：这些文件是什么、删了会怎样。做法和解释启动项一样：以前解释过的用记下的，
/// 没解释过的凑成一批问。发给 AI 的是名字、文件夹（用户文件夹写成 `~`）、大小和里面最大的几样的名字。
#[tauri::command]
async fn ai_explain_cleanup(app: AppHandle, ids: Vec<String>, mode: String) -> CmdResult<Explanations> {
    blocking(app, move |app, state| {
        let home = if cfg!(windows) { std::env::var_os("USERPROFILE") } else { std::env::var_os("HOME") }.map(PathBuf::from);
        let facts: Vec<(String, String)> = {
            let cached = state.cleanup.lock().map_err(|_| "internal".to_string())?;
            let scan = cached.as_ref().ok_or_else(|| "not_found".to_string())?;
            ids.iter().filter_map(|id| scan.facts(id, home.as_deref()).map(|text| (id.clone(), text))).collect()
        };
        let dir = cache_dir(&state)?;
        let language = with_repo(&state, |repo| Ok(repo.lib.settings.language.clone()))?;
        // 文件夹不一样（比如改了自定义规则）就算另一项，重新问
        let key_of = |id: &str, text: &str| {
            let folders = text.lines().find(|line| line.starts_with("folders:")).unwrap_or("");
            aicache::explain_key(std::env::consts::OS, "cleanup", &format!("{id} {folders}"), &language)
        };
        let mut explanations = HashMap::new();
        let mut todo: Vec<&(String, String)> = Vec::new();
        {
            let _turn = state.ai_cache_turn.lock().unwrap_or_else(|e| e.into_inner());
            let cache = AiCache::load(&dir);
            for entry in &facts {
                match cache.explain.get(&key_of(&entry.0, &entry.1)).filter(|_| mode != "refresh") {
                    Some(hit) => {
                        explanations.insert(entry.0.clone(), Explanation { text: hit.text.clone(), model: hit.model.clone(), at: hit.at.clone(), cached: true });
                    }
                    None => todo.push(entry),
                }
            }
        }
        if mode == "cached" || todo.is_empty() {
            return Ok(Explanations { explanations, asked: 0, missing: 0 });
        }
        let cancel = Arc::new(AtomicBool::new(false));
        if let Ok(mut current) = state.ai_cancel.lock() {
            current.store(true, Ordering::SeqCst);
            *current = cancel.clone();
        }
        let (service, library) = repo_ai(&state)?;
        let questions: Vec<(String, String)> = todo.iter().map(|entry| (*entry).clone()).collect();
        let answers = ai::explain_cleanup_many(&service, &library.settings.language, &questions, &cancel, &mut |done, total| {
            let _ = app.emit("ai://progress", AiProgress { done, total });
        })
        .map_err(|e| e.code())?;
        let at = shibapack_core::now();
        let model = service.model().to_string();
        let _turn = state.ai_cache_turn.lock().unwrap_or_else(|e| e.into_inner());
        let mut cache = AiCache::load(&dir);
        for (id, text) in &answers {
            let Some(entry) = todo.iter().find(|entry| &entry.0 == id) else { continue };
            cache.explain.insert(key_of(&entry.0, &entry.1), CachedText { text: text.clone(), model: model.clone(), at: at.clone() });
            explanations.insert(id.clone(), Explanation { text: text.clone(), model: model.clone(), at: at.clone(), cached: false });
        }
        let _ = cache.save(&dir);
        Ok(Explanations { explanations, asked: todo.len(), missing: todo.len() - answers.len() })
    })
    .await
}

/// 让这一项要先关掉的程序正常退出（不强行结束），最多等 10 秒。返回更新后的这一项（`running` 看它退了没有）。
#[tauri::command]
async fn cleanup_quit_app(app: AppHandle, id: String) -> CmdResult<cleanup::CleanItem> {
    blocking(app, move |_app, state| {
        let mut scan = state.cleanup.lock().map_err(|_| "internal".to_string())?;
        scan.as_mut().and_then(|scan| scan.quit_app(&id)).ok_or_else(|| "not_found".to_string())
    })
    .await
}

/// 在文件管理器里定位这一项的某一条，或者它的第一个文件夹。
#[tauri::command]
async fn cleanup_reveal(app: AppHandle, id: String, index: Option<usize>) -> CmdResult<()> {
    blocking(app, move |app, state| {
        let path = state.cleanup.lock().ok().and_then(|scan| scan.as_ref().and_then(|scan| scan.locate(&id, index))).ok_or_else(|| "not_found".to_string())?;
        if std::fs::symlink_metadata(&path).is_err() {
            return Err("file_missing".into());
        }
        app.opener().reveal_item_in_dir(path).map_err(|e| e.to_string())
    })
    .await
}

/// 浏览器里有 Cookie 的网站，给保留名单出主意。只读，先复制一份 Cookie 文件再读。
#[tauri::command]
async fn cleanup_cookie_sites(app: AppHandle) -> CmdResult<Vec<cleanup::CookieSite>> {
    blocking(app, move |app, state| Ok(cleanup::cookie_sites(&cleanup_context(app, &state)?))).await
}

/// 自定义的清理文件夹和排除的文件夹。
#[tauri::command]
async fn cleanup_options(app: AppHandle) -> CmdResult<cleanup::Options> {
    blocking(app, move |app, _state| Ok(cleanup::load_options(&local_state_dir(app)?))).await
}

/// 保存自定义的清理文件夹和排除的文件夹。自定义文件夹要过保护检查，不行时返回 `cleanup_protected:路径` 这样的错误。
#[tauri::command]
async fn cleanup_set_options(app: AppHandle, options: cleanup::Options) -> CmdResult<cleanup::Options> {
    blocking(app, move |app, state| {
        let ctx = cleanup_context(app, &state)?;
        let options = cleanup::check_options(&ctx.guard, options)?;
        cleanup::save_options(&local_state_dir(app)?, &options).map_err(|e| e.to_string())?;
        Ok(options)
    })
    .await
}

/// 最近 20 次清理（不带具体路径）。
#[tauri::command]
async fn cleanup_history(app: AppHandle) -> CmdResult<Vec<cleanup::history::Run>> {
    blocking(app, move |app, _state| Ok(cleanup::history::list(&local_state_dir(app)?.join("cleanup-history"), false))).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Explanation {
    text: String,
    model: String,
    at: String,
    /// 用的是以前记下的解释，这次没有问 AI。
    cached: bool,
}

/// 仓库里放缓存的文件夹（AI 的答案、Homebrew 目录）。
fn cache_dir(state: &State<AppState>) -> CmdResult<PathBuf> {
    with_repo(state, |repo| Ok(repo.data_dir().join("cache")))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Explanations {
    /// 启动项 id → 解释。
    explanations: HashMap<String, Explanation>,
    /// 这次真的问了 AI 的有几项（其余是以前记下的）。
    asked: usize,
    /// 问了但 AI 没给出解释的有几项（漏掉的、说不认识的）。
    missing: usize,
}

/// 让 AI 解释一批启动项或服务：是什么、能不能停用。以前解释过的直接用记下的；没解释过的凑成一批问 AI
/// （每次请求放 10 项），不是一项一项地问。解释记在仓库的缓存里。进度通过 `ai://progress` 发给界面。
/// `mode`：`cached` 只看缓存、不问 AI；`auto` 有缓存用缓存，没有才问；`refresh` 全部重新问。
#[tauri::command]
async fn ai_explain_startup_many(app: AppHandle, ids: Vec<String>, mode: String) -> CmdResult<Explanations> {
    blocking(app, move |app, state| {
        let items: Vec<StartupItem> = {
            let list = state.startup.lock().map_err(|_| "internal".to_string())?;
            ids.iter().filter_map(|id| list.iter().find(|item| &item.id == id).cloned()).collect()
        };
        let dir = cache_dir(&state)?;
        let language = with_repo(&state, |repo| Ok(repo.lib.settings.language.clone()))?;
        let key_of = |item: &StartupItem| aicache::explain_key(std::env::consts::OS, &item.source, &item.label, &language);

        let mut explanations = HashMap::new();
        let mut todo: Vec<&StartupItem> = Vec::new();
        {
            let _turn = state.ai_cache_turn.lock().unwrap_or_else(|e| e.into_inner());
            let cache = AiCache::load(&dir);
            for item in &items {
                match cache.explain.get(&key_of(item)).filter(|_| mode != "refresh") {
                    Some(hit) => {
                        explanations.insert(item.id.clone(), Explanation { text: hit.text.clone(), model: hit.model.clone(), at: hit.at.clone(), cached: true });
                    }
                    None => todo.push(item),
                }
            }
        }
        if mode == "cached" || todo.is_empty() {
            return Ok(Explanations { explanations, asked: 0, missing: 0 });
        }

        let cancel = Arc::new(AtomicBool::new(false));
        if let Ok(mut current) = state.ai_cancel.lock() {
            current.store(true, Ordering::SeqCst);
            *current = cancel.clone();
        }
        let (service, library) = repo_ai(&state)?;
        let questions: Vec<(String, String)> = todo.iter().map(|item| (item.id.clone(), startup::facts(item))).collect();
        let answers = ai::explain_startup_many(&service, &library.settings.language, &questions, &cancel, &mut |done, total| {
            let _ = app.emit("ai://progress", AiProgress { done, total });
        })
        .map_err(|e| e.code())?;

        let at = shibapack_core::now();
        let model = service.model().to_string();
        let _turn = state.ai_cache_turn.lock().unwrap_or_else(|e| e.into_inner());
        let mut cache = AiCache::load(&dir);
        for (id, text) in &answers {
            let Some(item) = todo.iter().find(|item| &item.id == id) else { continue };
            cache.explain.insert(key_of(item), CachedText { text: text.clone(), model: model.clone(), at: at.clone() });
            explanations.insert(id.clone(), Explanation { text: text.clone(), model: model.clone(), at: at.clone(), cached: false });
        }
        // 存不进去也照样把解释给用户，只是下次要重新问
        let _ = cache.save(&dir);
        Ok(Explanations { explanations, asked: todo.len(), missing: todo.len() - answers.len() })
    })
    .await
}

// ── 失败日志与自己的更新 ─────────────────────────────────────

/// 界面上出的错也记进失败日志（导入、启动项这些后端自己会记）。
#[tauri::command]
fn log_failure(state: State<AppState>, area: String, message: String, detail: Option<String>) {
    log_failure_to(&state, &area, &message, detail.as_deref().unwrap_or(""));
}

/// 最近的失败日志，新的在前。
#[tauri::command]
async fn read_failure_log(app: AppHandle, limit: Option<usize>) -> Vec<faillog::FailureEntry> {
    blocking(app, move |_app, state| faillog::read(&state.log_dir, limit.unwrap_or(500))).await
}

#[tauri::command]
async fn clear_failure_log(app: AppHandle) -> CmdResult<()> {
    blocking(app, move |_app, state| faillog::clear(&state.log_dir).map_err(|e| e.to_string())).await
}

/// 在文件管理器里打开日志文件夹（定位到日志文件）。
#[tauri::command]
async fn open_log_folder(app: AppHandle) -> CmdResult<()> {
    blocking(app, move |app, state| {
        std::fs::create_dir_all(&state.log_dir).map_err(|e| e.to_string())?;
        let file = faillog::path(&state.log_dir);
        if file.exists() {
            app.opener().reveal_item_in_dir(file).map_err(|e| e.to_string())
        } else {
            app.opener().open_path(state.log_dir.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
        }
    })
    .await
}

/// 自己更新时用 latest.json 里的哪一项。Windows 上按当初怎么装的来：用 `-setup.exe` 装的（当前用户，
/// 文件夹里有 uninstall.exe）用 NSIS 更新包，受保护目录由安装器请求管理员授权；用 MSI 装的仍用 MSI。
/// 别的系统用默认的。
#[tauri::command]
fn updater_target() -> Option<String> {
    if !cfg!(windows) {
        return None;
    }
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        "x86" => "i686",
        other => other,
    };
    let exe = std::env::current_exe().ok()?;
    let nsis = exe.parent().is_some_and(|dir| dir.join("uninstall.exe").exists());
    Some(format!("windows-{arch}-{}", if nsis { "nsis" } else { "msi" }))
}

/// 自动安装装不上时（macOS 不让没有签名的应用替换自己），把这个版本的安装包下载到“下载”文件夹并打开，
/// 让用户自己装：macOS 是磁盘映像，Windows 是 `-setup.exe`，Linux 是 AppImage。返回下载好的文件路径。
#[tauri::command]
async fn download_installer(app: AppHandle, version: String) -> CmdResult<String> {
    blocking(app, move |app, state| {
        let version = version.trim_start_matches('v').to_string();
        if version.is_empty() || !version.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+')) {
            return Err("invalid_reference".into());
        }
        let name = if cfg!(target_os = "macos") {
            format!("ShibaPack_{version}_universal.dmg")
        } else if cfg!(windows) {
            format!("ShibaPack_{version}_x64-setup.exe")
        } else {
            format!("ShibaPack_{version}_amd64.AppImage")
        };
        let url = format!("https://github.com/{RELEASE_REPO}/releases/download/v{version}/{name}");
        let dir = app.path().download_dir().map_err(|e| e.to_string())?;
        let cancel = AtomicBool::new(false);
        let done = download::download(&url, &dir, &name, None, &cancel, &mut |_, _| {}).map_err(|e| {
            let code = e.code();
            log_failure_to(&state, "update", &code, &url);
            code
        })?;
        app.opener().open_path(done.path.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())?;
        Ok(done.path.to_string_lossy().to_string())
    })
    .await
}

// ── 图标与简介 ───────────────────────────────────────────────

/// 给没有图标的软件补图标，一次最多 8 个，不拖慢别的操作。
#[tauri::command]
async fn fill_icons(app: AppHandle) -> CmdResult<FillResult> {
    blocking(app, move |_app, state| {
        let apps = state.installed.lock().map(|list| list.clone()).unwrap_or_default();
        let mut tried = state.icon_tried.lock().map_err(|_| "internal".to_string())?;
        with_repo(&state, |repo| {
            let (changed, more) = repo.fill_icons(8, &mut tried, &apps)?;
            Ok(FillResult { library: changed.then(|| repo.lib.clone()), more })
        })
    })
    .await
}

/// 用安装包自带的属性补版本号和发布者（版本号是空的文件挪到读到的版本），一次最多读 8 个文件。
/// 读过的记在仓库的缓存里，同一个文件不再读。
#[tauri::command]
async fn fill_file_info(app: AppHandle) -> CmdResult<FillResult> {
    blocking(app, move |_app, state| {
        with_repo(&state, |repo| {
            let (changed, more) = repo.fill_file_info(8)?;
            Ok(FillResult { library: changed.then(|| repo.lib.clone()), more })
        })
    })
    .await
}

/// 联网给一个软件找图标。
/// 返回图标的相对路径和缓存内容，没找到返回 null。网络不通时返回 `offline`。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IconUpdate {
    icon: String,
    icon_data: Option<Arc<str>>,
}

#[tauri::command]
async fn fetch_icon(app: AppHandle, id: String) -> CmdResult<Option<IconUpdate>> {
    blocking(app, move |_app, state| {
        let (root, query) = with_repo(&state, |repo| Ok((repo.root().to_path_buf(), repo.web_icon_query(&id)?)))?;
        let Some(query) = query else { return Ok(None) };
        // 联网期间不占着仓库的锁
        let found = webicon::fetch_web_icon(&query.homepage, query.github.as_deref(), query.the_svg_name.as_deref())
            .map_err(|_| "offline".to_string())?;
        with_repo(&state, |repo| {
            if repo.root() != root {
                return Ok(None);
            }
            let Some(icon) = repo.set_web_icon(&id, &query, found)? else { return Ok(None) };
            let icon_data = repo.lib.software.iter().find(|software| software.id == id).and_then(|software| software.icon_data.clone());
            Ok(Some(IconUpdate { icon, icon_data }))
        })
    })
    .await
}

/// 联网取一个软件的简介并存进索引（按名字存）；简介不是界面语言、又开着自动翻译时，再翻译成界面语言。
/// 已经有简介的只做翻译；`refresh` 为真时不管有没有都重新取。仓库里的软件和只装在这台电脑上的软件都用它。
/// 网络不通时返回 `offline`（已经取到的简介照样存下）。
#[tauri::command]
async fn fetch_summary(app: AppHandle, name: String, refresh: Option<bool>) -> CmdResult<Library> {
    blocking(app, move |_app, state| {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err("not_found".into());
        }
        let (language, existing, translate) = with_repo(&state, |repo| {
            let settings = &repo.lib.settings;
            let existing = if refresh.unwrap_or(false) { None } else { repo.lib.summaries.get(&name).cloned() };
            Ok((settings.language.clone(), existing, settings.translate_summaries))
        })?;
        // 联网期间不占着仓库的锁，别的操作照常进行
        let (summary, mut library) = match existing {
            Some(summary) => (Some(summary), None),
            None => {
                let found = summary::fetch_summary(&name, &language).map_err(|_| "offline".to_string())?;
                let library = mutate(&state, |repo| repo.set_summary(&name, found.clone()))?;
                (found, Some(library))
            }
        };
        if let Some(summary) = summary.filter(|s| translate && summary::needs_translation(s, &language)) {
            let text = summary::translate(&summary.text, &summary::summary_language(&summary), &language).map_err(|_| "offline".to_string())?;
            library = Some(mutate(&state, |repo| repo.set_summary_translation(&name, &language, text))?);
        }
        match library {
            Some(library) => Ok(library),
            None => with_repo(&state, |repo| Ok(repo.lib.clone())),
        }
    })
    .await
}

/// 开发用的自检：界面把检查结果一行行报回来，打印到终端，最后退出。
/// 只有用 `SHIBAPACK_SELFTEST=<一个安装包的路径>` 启动时才起作用。
#[tauri::command]
fn selftest_report(app: AppHandle, state: State<AppState>, line: String, done: bool) {
    if state.selftest.is_none() {
        return;
    }
    println!("[selftest] {line}");
    if done {
        app.exit(0);
    }
}

// ── 在线源 ───────────────────────────────────────────────────

/// 查一个软件在在线源上的最新版本，不保存。用来在关联之前先看一眼。
#[tauri::command]
async fn source_lookup(app: AppHandle, kind: SourceKind, reference: String) -> CmdResult<Release> {
    blocking(app, move |_app, _state| {
        sources::fetch_release(kind, &reference).map_err(|e| e.code().to_string())
    })
    .await
}

/// 在 Homebrew 目录里搜索。目录缓存在仓库的 cache 文件夹里。
#[tauri::command]
async fn source_search(app: AppHandle, query: String) -> CmdResult<Vec<Candidate>> {
    blocking(app, move |_app, state| {
        let cache = with_repo(&state, |repo| Ok(repo.data_dir().join("cache")))?;
        sources::search_homebrew(&cache, &query, 30).map_err(|e| e.code().to_string())
    })
    .await
}

/// ShibaPack 自己发布在哪个 GitHub 仓库（和 tauri.conf.json 里更新地址的仓库一致）。
const RELEASE_REPO: &str = "Wonvy/ShibaPack";

/// “软件更新”窗口：某个版本的更新说明（GitHub Release 的说明原文）。
#[tauri::command]
async fn release_notes(app: AppHandle, version: String) -> CmdResult<String> {
    blocking(app, move |_app, _state| {
        let tag = format!("v{}", version.trim_start_matches('v'));
        sources::github_release_notes(RELEASE_REPO, &tag).map_err(|e| e.code().to_string())
    })
    .await
}

/// 顶栏搜索：在 GitHub 上搜仓库。只读。
#[tauri::command]
async fn github_search(app: AppHandle, query: String) -> CmdResult<Vec<sources::RepoHit>> {
    blocking(app, move |_app, _state| sources::search_github(&query, 8).map_err(|e| e.code().to_string())).await
}

/// 给一个软件关联在线源。`software_id` 为空时按 `name` 新建一个软件（从在线源添加）。
#[tauri::command]
async fn subscribe(app: AppHandle, software_id: Option<String>, name: Option<String>, kind: SourceKind, reference: String) -> CmdResult<SubscribeResult> {
    blocking(app, move |_app, state| {
        let reference = sources::normalize_reference(kind, &reference).map_err(|e| e.code().to_string())?;
        // 联网期间不占着仓库的锁
        let release = sources::fetch_release(kind, &reference).map_err(|e| e.code().to_string())?;
        with_repo(&state, |repo| {
            let id = match software_id {
                Some(id) => id,
                None => repo.ensure_software(name.as_deref().unwrap_or(&reference), &release.page)?,
            };
            let subscription = Subscription { source: kind, reference, latest: Some(release), checked_at: shibapack_core::now() };
            repo.set_subscription(&id, Some(subscription))?;
            Ok(SubscribeResult { library: repo.lib.clone(), software_id: id })
        })
    })
    .await
}

#[tauri::command]
async fn unsubscribe(app: AppHandle, software_id: String) -> CmdResult<Library> {
    blocking(app, move |_app, state| {
        mutate(&state, |repo| repo.set_subscription(&software_id, None))
    })
    .await
}

/// 检查所有关联了在线源的软件有没有新版本。进度通过 `sources://progress` 事件发给前端。
#[tauri::command]
async fn check_updates(app: AppHandle) -> CmdResult<CheckResult> {
    blocking(app, move |app, state| {
        let targets: Vec<(String, String, SourceKind, String)> = with_repo(&state, |repo| {
            let subscribed = repo.lib.software.iter().filter_map(|software| {
                let subscription = software.subscription.as_ref()?;
                Some((software.id.clone(), software.name.clone(), subscription.source, subscription.reference.clone()))
            });
            Ok(subscribed.collect())
        })?;
        let total = targets.len();
        let mut found = Vec::new();
        let (mut failed, mut rate_limited) = (0, false);
        for (i, (id, name, kind, reference)) in targets.iter().enumerate() {
            let _ = app.emit("sources://progress", CheckProgress { current: i + 1, total, name });
            // GitHub 限流之后，走 GitHub 的两种源就不用再试了；Homebrew 不受影响
            if rate_limited && *kind != SourceKind::Homebrew {
                failed += 1;
                continue;
            }
            match sources::fetch_release(*kind, reference) {
                Ok(release) => found.push((id.clone(), release)),
                Err(SourceError::RateLimited) => {
                    rate_limited = true;
                    failed += 1;
                }
                Err(_) => failed += 1,
            }
        }
        let _ = app.emit("sources://progress", CheckProgress { current: total, total: 0, name: "" });
        let checked = found.len();
        let library = mutate(&state, |repo| {
            for (id, release) in found {
                repo.set_latest(&id, release);
            }
            repo.save()
        })?;
        Ok(CheckResult { library, checked, failed, rate_limited })
    })
    .await
}

/// 下载在线源上的一个安装包并收进仓库。进度通过 `download://progress` 事件发给前端。
#[tauri::command]
async fn download_asset(app: AppHandle, software_id: String, url: String) -> CmdResult<Library> {
    blocking(app, move |app, state| {
        // 只下载索引里记着的地址，不接受前端随便给一个
        let (asset, version, dir) = with_repo(&state, |repo| {
            let software = repo.lib.software.iter().find(|s| s.id == software_id).ok_or(shibapack_core::Error::Code("not_found"))?;
            let release = software.subscription.as_ref().and_then(|s| s.latest.as_ref()).ok_or(shibapack_core::Error::Code("not_found"))?;
            let asset = release.assets.iter().find(|a| a.url == url).ok_or(shibapack_core::Error::Code("not_found"))?;
            Ok((asset.clone(), release.version.clone(), repo.downloads_dir()))
        })?;

        let cancel = Arc::new(AtomicBool::new(false));
        if let Ok(mut cancels) = state.cancels.lock() {
            cancels.insert(url.clone(), cancel.clone());
        }
        let report = |received: u64, total: Option<u64>, done: bool| {
            let _ = app.emit("download://progress", DownloadProgress { software_id: &software_id, url: &url, name: &asset.name, received, total, done });
        };
        report(0, asset.size, false);

        let result = {
            let _turn = state.download_turn.lock();
            let mut last = Instant::now() - Duration::from_secs(1);
            download::download(&asset.url, &dir, &asset.name, asset.sha256.as_deref(), &cancel, &mut |received, total| {
                // 界面一秒刷新八次就够了
                if last.elapsed() >= Duration::from_millis(120) {
                    last = Instant::now();
                    report(received, total, false);
                }
            })
        };
        if let Ok(mut cancels) = state.cancels.lock() {
            cancels.remove(&url);
        }
        report(0, None, true);
        let done = result.map_err(|e| e.code())?;
        mutate(&state, |repo| repo.adopt_download(&software_id, &version, &done.path, &asset, done.sha256))
    })
    .await
}

#[tauri::command]
fn cancel_download(state: State<AppState>, url: String) {
    if let Some(cancel) = state.cancels.lock().ok().and_then(|cancels| cancels.get(&url).cloned()) {
        cancel.store(true, Ordering::Relaxed);
    }
}

/// 用系统的浏览器打开一个网址（发布说明、主页）。只接受 http 和 https。
#[tauri::command]
fn open_url(app: AppHandle, url: String) -> CmdResult<()> {
    let lower = url.to_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Err("invalid_reference".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

// ── 整理与语言包 ─────────────────────────────────────────────

/// 把直接丢在仓库根目录里的安装包归位。
#[tauri::command]
async fn tidy_root(app: AppHandle) -> CmdResult<ImportResult> {
    blocking(app, move |_app, state| {
        with_repo(&state, |repo| {
            let report = repo.tidy_root()?;
            Ok(ImportResult { report, library: repo.lib.clone() })
        })
    })
    .await
}

#[tauri::command]
async fn list_lang_packs(app: AppHandle) -> CmdResult<Vec<serde_json::Value>> {
    blocking(app, move |_app, state| {
        with_repo(&state, |repo| Ok(repo.lang_packs()))
    })
    .await
}

#[tauri::command]
async fn import_lang_pack(app: AppHandle, src: String) -> CmdResult<Vec<serde_json::Value>> {
    blocking(app, move |_app, state| {
        with_repo(&state, |repo| {
            repo.import_lang_pack(Path::new(&src))?;
            Ok(repo.lang_packs())
        })
    })
    .await
}

#[tauri::command]
async fn remove_lang_pack(app: AppHandle, locale: String) -> CmdResult<Vec<serde_json::Value>> {
    blocking(app, move |_app, state| {
        with_repo(&state, |repo| {
            repo.remove_lang_pack(&locale)?;
            Ok(repo.lang_packs())
        })
    })
    .await
}

/// 把一份语言包（界面当前的全部文字）写到用户选的文件里。
#[tauri::command]
async fn export_lang_pack(app: AppHandle, dest: String, pack: serde_json::Map<String, serde_json::Value>) -> CmdResult<()> {
    blocking(app, move |_app, _state| {
        let json = serde_json::to_vec_pretty(&pack).map_err(|e| e.to_string())?;
        fs::write(dest, json).map_err(|e| e.to_string())
    })
    .await
}

// ── AI 模型 ──────────────────────────────────────────────────

const KEYCHAIN_SERVICE: &str = "app.shibapack.ai";

fn keychain_entry(base_url: &str) -> CmdResult<keyring::Entry> {
    keyring::Entry::new(KEYCHAIN_SERVICE, &ai::key_account(base_url)).map_err(|e| format!("ai_keychain: {e}"))
}

/// 这台电脑的钥匙串里存着的、这个地址的 API Key。
fn saved_key(base_url: &str) -> Option<String> {
    keychain_entry(base_url).ok()?.get_password().ok().filter(|key| !key.trim().is_empty())
}

/// 连 AI 服务。`key` 为空时用钥匙串里存的（设置对话框里改了还没保存的密钥会直接传进来）。
fn ai_service(settings: &AiSettings, key: Option<String>) -> CmdResult<ai::Service> {
    let key = key.filter(|key| !key.trim().is_empty()).or_else(|| saved_key(&settings.base_url));
    ai::Service::new(settings, key.as_deref()).map_err(|e| e.code())
}

/// 仓库设置里的 AI 服务，和此刻索引的一份副本。联网期间不占着仓库的锁。
fn repo_ai(state: &State<AppState>) -> CmdResult<(ai::Service, Library)> {
    let library = with_repo(state, |repo| Ok(repo.lib.clone()))?;
    Ok((ai_service(&library.settings.ai, None)?, library))
}

#[tauri::command]
async fn ai_key_saved(app: AppHandle, base_url: String) -> bool {
    blocking(app, move |_app, _state| saved_key(&base_url).is_some()).await
}

#[tauri::command]
async fn ai_key_preview(app: AppHandle, base_url: String) -> Option<String> {
    blocking(app, move |_app, _state| saved_key(&base_url).map(|key| ai::key_preview(&key))).await
}

/// 存这个地址的 API Key；`key` 为空时删掉。只存在这台电脑的系统钥匙串里，不写进仓库。
#[tauri::command]
async fn ai_set_key(app: AppHandle, base_url: String, key: String) -> CmdResult<()> {
    blocking(app, move |_app, _state| {
        let entry = keychain_entry(&base_url)?;
        let key = key.trim();
        let result = if key.is_empty() {
            match entry.delete_credential() {
                Err(keyring::Error::NoEntry) => Ok(()),
                other => other,
            }
        } else {
            entry.set_password(key)
        };
        result.map_err(|e| format!("ai_keychain: {e}"))
    })
    .await
}

/// 列出服务上的模型。用设置对话框里还没保存的地址和密钥。
#[tauri::command]
async fn ai_models(app: AppHandle, settings: AiSettings, key: Option<String>) -> CmdResult<Vec<String>> {
    blocking(app, move |_app, _state| ai_service(&settings, key)?.models().map_err(|e| e.code())).await
}

/// 试着和模型说一句话，返回它的回答。
#[tauri::command]
async fn ai_test(app: AppHandle, settings: AiSettings, key: Option<String>) -> CmdResult<String> {
    blocking(app, move |_app, _state| {
        let service = ai_service(&settings, key)?;
        let reply = service.chat("Reply with the single word OK.", "ping", false, 0.0).map_err(|e| e.code())?;
        Ok(reply.chars().take(60).collect())
    })
    .await
}

/// 自己写的简介：存进 `summaries`，来源记成 `manual`，之后不会被自动取的简介盖掉。`text` 为空时删掉简介。
#[tauri::command]
async fn set_summary(app: AppHandle, name: String, text: String) -> CmdResult<Library> {
    blocking(app, move |_app, state| {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err("not_found".into());
        }
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        mutate(&state, |repo| {
            if text.is_empty() {
                return repo.clear_summary(&name);
            }
            let lang = repo.lib.settings.language.split(['-', '_']).next().unwrap_or("").to_lowercase();
            repo.set_summary(&name, Some(shibapack_core::Summary { text, source: "manual".into(), fetched_at: shibapack_core::now(), lang, translations: Default::default() }))
        })
    })
    .await
}

/// 让 AI 给一个名字写简介并存进索引（简介按名字存，仓库里的软件和只装在本机的软件都用它）。
/// `publisher` 给只装在本机、仓库里没有的软件用。
#[tauri::command]
async fn ai_describe(app: AppHandle, name: String, publisher: Option<String>) -> CmdResult<Library> {
    blocking(app, move |_app, state| {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err("not_found".into());
        }
        let (service, library) = repo_ai(&state)?;
        let summary = ai::describe(&service, &library, &name, publisher.as_deref().unwrap_or("")).map_err(|e| e.code())?;
        mutate(&state, |repo| repo.set_summary(&name, Some(summary)))
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DescribeTarget {
    name: String,
    #[serde(default)]
    publisher: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DescribeResult {
    library: Library,
    /// 写好了几个。
    written: usize,
    /// AI 不认识、或者漏掉的有几个。
    unknown: usize,
}

/// 让 AI 给一批软件写简介：凑成一批问（每次请求放 15 个），写好的一次存进索引。进度通过 `ai://progress` 发给界面。
#[tauri::command]
async fn ai_describe_many(app: AppHandle, items: Vec<DescribeTarget>) -> CmdResult<DescribeResult> {
    blocking(app, move |app, state| {
        let targets: Vec<(String, String)> =
            items.into_iter().map(|t| (t.name.trim().to_string(), t.publisher)).filter(|(name, _)| !name.is_empty()).collect();
        let cancel = Arc::new(AtomicBool::new(false));
        if let Ok(mut current) = state.ai_cancel.lock() {
            current.store(true, Ordering::SeqCst);
            *current = cancel.clone();
        }
        let (service, library) = repo_ai(&state)?;
        let written = ai::describe_many(&service, &library, &targets, &cancel, &mut |done, total| {
            let _ = app.emit("ai://progress", AiProgress { done, total });
        })
        .map_err(|e| e.code())?;
        let count = written.len();
        let library = mutate(&state, |repo| repo.set_summaries(written))?;
        Ok(DescribeResult { library, written: count, unknown: targets.len() - count })
    })
    .await
}

/// 用 AI 给一批软件推荐分类，只读。进度通过 `ai://progress` 事件发给前端。
#[tauri::command]
async fn ai_suggest_categories(app: AppHandle, software_ids: Vec<String>, refresh: Option<bool>) -> CmdResult<AiSuggestions> {
    blocking(app, move |app, state| {
        let cancel = Arc::new(AtomicBool::new(false));
        if let Ok(mut current) = state.ai_cancel.lock() {
            current.store(true, Ordering::SeqCst);
            *current = cancel.clone();
        }
        let (service, library) = repo_ai(&state)?;
        let dir = cache_dir(&state)?;
        let model = service.model().to_string();
        let fingerprint = |id: &str| library.software.iter().find(|s| s.id == id).map(|s| aicache::classify_fingerprint(&library, s, &model));

        // 软件、分类树、模型都没变的，直接用上次 AI 的建议
        let mut known: HashMap<String, Suggestion> = HashMap::new();
        if !refresh.unwrap_or(false) {
            let _turn = state.ai_cache_turn.lock().unwrap_or_else(|e| e.into_inner());
            let cache = AiCache::load(&dir);
            for id in &software_ids {
                let hit = cache.classify.get(id).filter(|entry| Some(&entry.fingerprint) == fingerprint(id).as_ref());
                if let Some(entry) = hit {
                    known.insert(id.clone(), entry.suggestion.clone());
                }
            }
        }
        let todo: Vec<String> = software_ids.iter().filter(|id| !known.contains_key(*id)).cloned().collect();
        let cached = software_ids.len() - todo.len();
        if !todo.is_empty() {
            let fresh = ai::suggest_categories(&service, &library, &todo, &cancel, &mut |done, total| {
                let _ = app.emit("ai://progress", AiProgress { done, total });
            })
            .map_err(|e| e.code())?;
            let _turn = state.ai_cache_turn.lock().unwrap_or_else(|e| e.into_inner());
            let mut cache = AiCache::load(&dir);
            let at = shibapack_core::now();
            for suggestion in fresh {
                if let Some(fingerprint) = fingerprint(&suggestion.software_id) {
                    cache.classify.insert(suggestion.software_id.clone(), CachedSuggestion { fingerprint, suggestion: suggestion.clone(), at: at.clone() });
                }
                known.insert(suggestion.software_id.clone(), suggestion);
            }
            let _ = cache.save(&dir);
        }
        let suggestions = software_ids.iter().filter_map(|id| known.remove(id)).collect();
        Ok(AiSuggestions { suggestions, cached })
    })
    .await
}

/// 停下正在进行的 AI 分类（在两批之间停）。
#[tauri::command]
fn cancel_ai(state: State<AppState>) {
    if let Ok(current) = state.ai_cancel.lock() {
        current.store(true, Ordering::SeqCst);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 以管理员身份清理卸载残留时，会带着特别的参数再启动一次自己：只做清理，不开窗口（见 leftovers::remove_elevated）
    if let Some(code) = leftovers::elevated_entry() {
        std::process::exit(code);
    }
    // 以管理员身份清理时提权启动的那个进程：只做清理，不开窗口
    if let Some(code) = cleanup::elevate::elevated_entry() {
        std::process::exit(code);
    }
    let mut builder = tauri::Builder::default();
    // 同一台电脑上只开一个 ShibaPack：两个窗口各存各的索引，会互相盖掉对方的改动。
    // 再打开一次时只把已有的窗口叫到前面。用 SHIBAPACK_CONFIG 换了引导文件（开发、自检）时不限制
    #[cfg(desktop)]
    if std::env::var_os("SHIBAPACK_CONFIG").is_none() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 第二次启动的那个进程会悄悄退出：在这边记一句，不然在终端里只看到它“直接退回提示符”
            eprintln!("[single-instance] 又启动了一次 ShibaPack，已经把这个窗口叫到前面（第二个进程已退出）");
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }));
    }
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        // 应用自己的在线更新：检查、下载、验证签名、安装；装完用 process 插件重启
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // 两个环境变量只给开发和测试用：换一个引导文件、换一个缓存目录，不碰正式的数据
            let env_path = |name: &str| std::env::var_os(name).map(PathBuf::from);
            let config_file = match env_path("SHIBAPACK_CONFIG") {
                Some(path) => path,
                None => app.path().app_config_dir()?.join("config.json"),
            };
            let icon_cache = match env_path("SHIBAPACK_CACHE") {
                Some(path) => path,
                None => app.path().app_cache_dir()?.join("installed-icons"),
            };
            let _ = app.asset_protocol_scope().allow_directory(&icon_cache, true);

            // 上次用的仓库还在就直接打开；不在了（比如移动硬盘没插）或打不开，就回到引导页并说明原因
            let last_root = fs::read(&config_file)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Bootstrap>(&bytes).ok())
                .and_then(|bootstrap| bootstrap.repo_root);
            let (repo, open_failure) = match last_root {
                None => (None, None),
                Some(root) if !Path::new(&root).is_dir() => (None, Some((root, "missing".to_string()))),
                Some(root) => match Repo::open(Path::new(&root)) {
                    Ok(repo) => (Some(repo), None),
                    Err(err) => (None, Some((root, err.to_string()))),
                },
            };
            if let Some(repo) = &repo {
                allow_repo_icons(app.handle(), repo);
            }
            let trace = resume_trace(app.handle());
            let resuming = trace.is_some();
            app.manage(AppState {
                repo: Mutex::new(repo),
                open_failure: Mutex::new(open_failure),
                config_file,
                icon_cache,
                installed: Mutex::new(Vec::new()),
                icon_tried: Mutex::new(HashSet::new()),
                selftest: std::env::var("SHIBAPACK_SELFTEST").ok(),
                download_turn: Mutex::new(()),
                importing: AtomicBool::new(false),
                import_cancel: AtomicBool::new(false),
                cancels: Mutex::new(HashMap::new()),
                version: app.package_info().version.to_string(),
                ai_cancel: Mutex::new(Arc::new(AtomicBool::new(false))),
                measured: Mutex::new(HashMap::new()),
                measure_turn: Mutex::new(()),
                uninstall: Mutex::new(HashMap::new()),
                monitor: Mutex::new(None),
                disk: Mutex::new(None),
                disk_live: Mutex::new(None),
                disk_cancel: AtomicBool::new(false),
                process_icons: Mutex::new(HashMap::new()),
                startup: Mutex::new(Vec::new()),
                startup_turn: Mutex::new(()),
                ai_cache_turn: Mutex::new(()),
                cleanup: Mutex::new(None),
                cleanup_turn: Mutex::new(()),
                cleanup_cancel: AtomicBool::new(false),
                log_dir: app.path().app_log_dir().unwrap_or_else(|_| std::env::temp_dir().join("ShibaPack-logs")),
                trace: Mutex::new(trace),
                trace_cancel: AtomicBool::new(false),
                trace_busy: AtomicBool::new(false),
            });
            // 接着上次没完成的监控：小窗也开出来
            if resuming {
                open_trace_mini(app.handle());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            paths_add,
            path_edit,
            paths_remove,
            path_states,
            path_backup,
            path_backups,
            path_restore,
            path_delete_backup,
            path_reveal,
            path_reveal_backup,
            get_status,
            resize_detail_window,
            set_repo_root,
            get_library,
            refresh_library,
            import_paths,
            cancel_import,
            update_software,
            update_version,
            delete_software,
            delete_software_many,
            delete_version,
            delete_versions,
            delete_file,
            run_file,
            reveal_file,
            open_repo_folder,
            set_categories,
            set_starred,
            set_starred_many,
            move_software,
            suggest_categories,
            apply_classification,
            set_settings,
            get_stats,
            list_backups,
            create_backup,
            restore_backup,
            verify_data,
            export_backup,
            import_backup,
            empty_trash,
            list_installed,
            measure_installed,
            match_installed,
            analyze_uninstall,
            system_overview,
            set_window_glass,
            disk_locations,
            disk_scan,
            disk_cancel,
            disk_largest,
            disk_live_view,
            disk_live_largest,
            trace_start,
            trace_status,
            trace_finish,
            trace_cancel,
            trace_mini,
            trace_mini_action,
            list_traces,
            get_trace,
            delete_trace,
            trace_reveal,
            disk_view,
            disk_reveal,
            disk_full_path,
            disk_trash,
            ai_explain_folders,
            system_sample,
            process_icons,
            end_process,
            uninstall_installed,
            uninstall_status,
            scan_leftovers,
            remove_leftovers,
            reveal_leftover,
            open_uninstall_backup,
            reveal_installed,
            list_startup,
            startup_action,
            reveal_startup,
            open_startup_settings,
            cleanup_scan,
            cleanup_run,
            cleanup_quit_app,
            cleanup_reveal,
            cleanup_options,
            cleanup_set_options,
            cleanup_history,
            cleanup_cookie_sites,
            cleanup_cancel_scan,
            cleanup_count_trash,
            cleanup_open_tool,
            cleanup_rescan,
            ai_explain_cleanup,
            ai_explain_startup_many,
            fill_icons,
            fill_file_info,
            inspect_drop,
            preview_import_categories,
            fetch_icon,
            fetch_summary,
            selftest_report,
            source_lookup,
            source_search,
            github_search,
            release_notes,
            log_failure,
            read_failure_log,
            clear_failure_log,
            open_log_folder,
            updater_target,
            download_installer,
            subscribe,
            unsubscribe,
            check_updates,
            download_asset,
            cancel_download,
            tidy_root,
            list_lang_packs,
            import_lang_pack,
            remove_lang_pack,
            export_lang_pack,
            open_url,
            ai_key_saved,
            ai_key_preview,
            ai_set_key,
            ai_models,
            ai_test,
            ai_describe,
            ai_describe_many,
            set_summary,
            ai_suggest_categories,
            cancel_ai,
        ])
        .run(tauri::generate_context!())
        .expect("ShibaPack 启动失败");
}
