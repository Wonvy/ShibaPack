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
    summary, AiSettings, Assignment, BackupInfo, Category, ImportHint, ImportReport, InstalledApp, Library, Progress, Release, Repo, Settings,
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
    uninstall: Mutex<Option<UninstallSession>>,
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
    /// 失败日志放在哪（应用自己的日志文件夹，只和这台电脑有关）。
    log_dir: PathBuf,
}

/// 记一条失败日志。
fn log_failure_to(state: &State<AppState>, area: &str, message: &str, detail: &str) {
    faillog::append(&state.log_dir, area, message, detail);
}

struct UninstallSession {
    id: String,
    target: Target,
    found: Vec<Leftover>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LeftoverScan {
    items: Vec<Leftover>,
    /// 软件看起来还在（卸载程序可能被取消了）：找到的都没有默认勾选。
    still_installed: bool,
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

/// 卸载。Windows 上启动系统登记的卸载程序；macOS 上把 .app 移到废纸篓。
#[tauri::command]
async fn uninstall_installed(app: AppHandle, id: String) -> CmdResult<()> {
    blocking(app, move |_app, state| {
        let app = find_installed(&state, &id)?;
        if !app.uninstallable {
            return Err("not_uninstallable".into());
        }
        // 卸载以后就查不到它的安装位置、发布者了，先记下来，找残留时用
        if let Ok(mut session) = state.uninstall.lock() {
            *session = Some(UninstallSession { id: id.clone(), target: Target::of(&app), found: Vec::new() });
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
        let target = state.uninstall.lock().ok().and_then(|s| s.as_ref().filter(|s| s.id == id).map(|s| s.target.clone()));
        target.is_some_and(|target| leftovers::still_installed(&target))
    })
    .await
}

/// 找刚卸载的软件留下的文件夹、文件和注册表项。只读。
#[tauri::command]
async fn scan_leftovers(app: AppHandle, id: String) -> CmdResult<LeftoverScan> {
    blocking(app, move |_app, state| {
        let target = state
            .uninstall
            .lock()
            .ok()
            .and_then(|s| s.as_ref().filter(|s| s.id == id).map(|s| s.target.clone()))
            .ok_or_else(|| "not_found".to_string())?;
        let others: Vec<InstalledApp> = state.installed.lock().map(|list| list.iter().filter(|app| app.id != id).cloned().collect()).unwrap_or_default();
        let still_installed = leftovers::still_installed(&target);
        let items = leftovers::scan(&target, &others, still_installed);
        if let Ok(mut session) = state.uninstall.lock() {
            if let Some(session) = session.as_mut().filter(|s| s.id == id) {
                session.found = items.clone();
            }
        }
        Ok(LeftoverScan { items, still_installed })
    })
    .await
}

/// 清理勾选的残留（按上次找到的列表里的序号）：文件和文件夹移到回收站，注册表项先备份再删。
/// `elevated` 为真时以管理员身份做（Windows 会弹 UAC 提示），用在普通权限删不掉的那些上。
#[tauri::command]
async fn remove_leftovers(app: AppHandle, id: String, indices: Vec<usize>, elevated: bool) -> CmdResult<RemoveReport> {
    blocking(app, move |app, state| {
        let items: Vec<Leftover> = {
            let session = state.uninstall.lock().map_err(|_| "internal".to_string())?;
            let session = session.as_ref().filter(|s| s.id == id).ok_or_else(|| "not_found".to_string())?;
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
    let options = cleanup::load_options(&local_state_dir(app)?);
    Ok(cleanup::Context::new(root.as_deref(), downloads, library, options))
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
        let scan = cleanup::scan(&ctx, &|item| {
            let _ = app.emit("cleanup://item", item);
        });
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
        if !run.items.is_empty() || run.skipped > 0 {
            report.history = cleanup::history::record(&local_state_dir(app)?.join("cleanup-history"), &run).ok();
        }
        // 清掉的东西不在了：下次清理前要重新扫描
        if let Ok(mut cached) = state.cleanup.lock() {
            *cached = None;
        }
        Ok(report)
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
                uninstall: Mutex::new(None),
                startup: Mutex::new(Vec::new()),
                startup_turn: Mutex::new(()),
                ai_cache_turn: Mutex::new(()),
                cleanup: Mutex::new(None),
                cleanup_turn: Mutex::new(()),
                log_dir: app.path().app_log_dir().unwrap_or_else(|_| std::env::temp_dir().join("ShibaPack-logs")),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
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
            uninstall_installed,
            uninstall_status,
            scan_leftovers,
            remove_leftovers,
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
