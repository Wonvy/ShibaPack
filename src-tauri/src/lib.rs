//! 桌面外壳：把核心库的能力包成前端可以调用的命令。
//!
//! 修改数据的命令都返回修改后的整个索引，前端直接替换自己手里的那份。

use serde::{Deserialize, Serialize};
use shibapack_core::{
    download,
    identify::same_software,
    installed,
    sources::{self, Candidate, SourceError},
    summary, BackupInfo, Category, ImportHint, ImportReport, InstalledApp, Library, Release, Repo, Settings, SoftwareEdit,
    SourceKind, Stats, Storage, Subscription, Tag, VerifyReport, VersionEdit,
};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

struct AppState {
    repo: Mutex<Option<Repo>>,
    /// 仓库之外唯一的文件：记录仓库根目录在哪。
    config_file: PathBuf,
    /// 已安装软件的图标缓存。只和这台电脑有关，所以放在系统的缓存目录而不是仓库里。
    icon_cache: PathBuf,
    /// 最近一次列出的已安装软件。卸载、定位时按 id 在这里查，不接受前端传来的路径。
    installed: Mutex<Vec<InstalledApp>>,
    /// 这次运行中已经试过取图标但没取到的软件，不再重复尝试。
    icon_tried: Mutex<HashSet<String>>,
    /// 开发用的自检模式，见 `selftest_report`。值是自检时要导入的一个文件的路径。
    selftest: Option<String>,
    /// 下载一个一个来，后面的排队。
    download_turn: Mutex<()>,
    /// 正在下载或排队的文件（按下载地址）对应的“取消”开关。
    cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// 应用自己的版本号（来自 package.json）。
    version: String,
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
    version: String,
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
struct ImportResult {
    report: ImportReport,
    library: Library,
}

type CmdResult<T> = Result<T, String>;

fn with_repo<T>(state: &State<AppState>, run: impl FnOnce(&mut Repo) -> shibapack_core::Result<T>) -> CmdResult<T> {
    let mut guard = state.repo.lock().map_err(|_| "internal".to_string())?;
    let repo = guard.as_mut().ok_or_else(|| "not_configured".to_string())?;
    run(repo).map_err(|e| e.to_string())
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
    Status {
        configured: repo.is_some(),
        repo_root: repo.map(|r| r.root().to_string_lossy().to_string()),
        recovered: repo.and_then(|r| r.recovered.clone()),
        version: state.version.clone(),
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

#[tauri::command(async)]
fn set_repo_root(app: AppHandle, state: State<AppState>, path: String) -> CmdResult<Status> {
    let repo = Repo::open(Path::new(&path)).map_err(|e| e.to_string())?;
    allow_repo_icons(&app, &repo);
    let bootstrap = Bootstrap { repo_root: Some(repo.root().to_string_lossy().to_string()) };
    if let Some(dir) = state.config_file.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_vec_pretty(&bootstrap).map_err(|e| e.to_string())?;
    fs::write(&state.config_file, json).map_err(|e| e.to_string())?;
    *state.repo.lock().map_err(|_| "internal".to_string())? = Some(repo);
    if let Ok(mut tried) = state.icon_tried.lock() {
        tried.clear();
    }
    Ok(status_of(&state))
}

#[tauri::command(async)]
fn get_library(state: State<AppState>) -> CmdResult<Library> {
    with_repo(&state, |repo| Ok(repo.lib.clone()))
}

/// 导入文件或文件夹。进度通过 `import://progress` 事件发给前端。
#[tauri::command(async)]
fn import_paths(
    app: AppHandle,
    state: State<AppState>,
    paths: Vec<String>,
    mode: Storage,
    hint: Option<ImportHint>,
) -> CmdResult<ImportResult> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    with_repo(&state, |repo| {
        let report = repo.import_paths(&paths, mode, &hint.unwrap_or_default(), &mut |progress| {
            let _ = app.emit("import://progress", &progress);
        })?;
        Ok(ImportResult { report, library: repo.lib.clone() })
    })
}

#[tauri::command(async)]
fn update_software(state: State<AppState>, id: String, edit: SoftwareEdit) -> CmdResult<Library> {
    mutate(&state, |repo| repo.update_software(&id, edit))
}

#[tauri::command(async)]
fn update_version(
    state: State<AppState>,
    software_id: String,
    version_id: String,
    edit: VersionEdit,
) -> CmdResult<Library> {
    mutate(&state, |repo| repo.update_version(&software_id, &version_id, edit))
}

#[tauri::command(async)]
fn delete_software(state: State<AppState>, id: String, delete_files: bool) -> CmdResult<Library> {
    mutate(&state, |repo| repo.delete_software(&id, delete_files).map(trash_external))
}

#[tauri::command(async)]
fn delete_version(
    state: State<AppState>,
    software_id: String,
    version_id: String,
    delete_files: bool,
) -> CmdResult<Library> {
    mutate(&state, |repo| repo.delete_version(&software_id, &version_id, delete_files).map(trash_external))
}

#[tauri::command(async)]
fn delete_file(
    state: State<AppState>,
    software_id: String,
    version_id: String,
    file_id: String,
    delete_from_disk: bool,
) -> CmdResult<Library> {
    mutate(&state, |repo| {
        repo.delete_file(&software_id, &version_id, &file_id, delete_from_disk).map(trash_external)
    })
}

/// 用系统默认方式打开安装包（exe 直接运行，dmg 挂载，deb 交给软件中心）。
#[tauri::command(async)]
fn run_file(
    app: AppHandle,
    state: State<AppState>,
    software_id: String,
    version_id: String,
    file_id: String,
) -> CmdResult<()> {
    let path = with_repo(&state, |repo| repo.file_path(&software_id, &version_id, &file_id))?;
    if !path.is_file() {
        return Err("file_missing".into());
    }
    app.opener().open_path(path.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

/// 在文件管理器里定位到这个文件。
#[tauri::command(async)]
fn reveal_file(
    app: AppHandle,
    state: State<AppState>,
    software_id: String,
    version_id: String,
    file_id: String,
) -> CmdResult<()> {
    let path = with_repo(&state, |repo| repo.file_path(&software_id, &version_id, &file_id))?;
    if !path.exists() {
        return Err("file_missing".into());
    }
    app.opener().reveal_item_in_dir(path).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn open_repo_folder(app: AppHandle, state: State<AppState>) -> CmdResult<()> {
    let root = with_repo(&state, |repo| Ok(repo.root().to_path_buf()))?;
    app.opener().open_path(root.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn set_categories(state: State<AppState>, categories: Vec<Category>) -> CmdResult<Library> {
    mutate(&state, |repo| repo.set_categories(categories))
}

#[tauri::command(async)]
fn set_tags(state: State<AppState>, tags: Vec<Tag>) -> CmdResult<Library> {
    mutate(&state, |repo| repo.set_tags(tags))
}

#[tauri::command(async)]
fn set_settings(state: State<AppState>, settings: Settings) -> CmdResult<Library> {
    mutate(&state, |repo| repo.set_settings(settings))
}

#[tauri::command(async)]
fn get_stats(state: State<AppState>) -> CmdResult<Stats> {
    with_repo(&state, |repo| Ok(repo.stats()))
}

#[tauri::command(async)]
fn list_backups(state: State<AppState>) -> CmdResult<Vec<BackupInfo>> {
    with_repo(&state, |repo| Ok(repo.backups()))
}

#[tauri::command(async)]
fn create_backup(state: State<AppState>) -> CmdResult<Vec<BackupInfo>> {
    with_repo(&state, |repo| {
        repo.create_backup()?;
        Ok(repo.backups())
    })
}

#[tauri::command(async)]
fn restore_backup(state: State<AppState>, name: String) -> CmdResult<Library> {
    mutate(&state, |repo| repo.restore_backup(&name))
}

#[tauri::command(async)]
fn verify_data(state: State<AppState>) -> CmdResult<VerifyReport> {
    with_repo(&state, |repo| Ok(repo.verify()))
}

#[tauri::command(async)]
fn export_backup(state: State<AppState>, dest: String) -> CmdResult<()> {
    with_repo(&state, |repo| repo.export_to(Path::new(&dest)))
}

#[tauri::command(async)]
fn import_backup(state: State<AppState>, src: String) -> CmdResult<Library> {
    mutate(&state, |repo| repo.import_from(Path::new(&src)))
}

#[tauri::command(async)]
fn empty_trash(state: State<AppState>) -> CmdResult<Stats> {
    with_repo(&state, |repo| {
        repo.empty_trash()?;
        Ok(repo.stats())
    })
}

// ── 已安装软件 ───────────────────────────────────────────────

/// 列出这台电脑上已安装的软件（会重新扫描）。
#[tauri::command(async)]
fn list_installed(state: State<AppState>) -> Vec<InstalledApp> {
    let apps = installed::list_installed(&state.icon_cache);
    if let Ok(mut cached) = state.installed.lock() {
        *cached = apps.clone();
    }
    apps
}

/// 统计各软件占用的空间。macOS 上要把每个 .app 走一遍，比较慢，所以和列表分开。
#[tauri::command(async)]
fn measure_installed(state: State<AppState>) -> HashMap<String, u64> {
    let apps = state.installed.lock().map(|list| list.clone()).unwrap_or_default();
    apps.iter().filter_map(|app| installed::measure(app).map(|size| (app.id.clone(), size))).collect()
}

/// 仓库里的软件和已安装软件的对应关系：软件 id → 已安装软件 id。
#[tauri::command(async)]
fn match_installed(state: State<AppState>) -> CmdResult<HashMap<String, String>> {
    let apps = state.installed.lock().map(|list| list.clone()).unwrap_or_default();
    with_repo(&state, |repo| {
        let links = repo.lib.software.iter().filter_map(|software| {
            let app = apps.iter().find(|app| same_software(&app.name, &software.name))?;
            Some((software.id.clone(), app.id.clone()))
        });
        Ok(links.collect())
    })
}

/// 卸载。Windows 上启动系统登记的卸载程序；macOS 上把 .app 移到废纸篓。
#[tauri::command(async)]
fn uninstall_installed(state: State<AppState>, id: String) -> CmdResult<()> {
    let app = find_installed(&state, &id)?;
    if !app.uninstallable {
        return Err("not_uninstallable".into());
    }
    if installed::run_uninstaller(&app).map_err(|e| e.to_string())? {
        return Ok(());
    }
    if cfg!(target_os = "macos") && Path::new(&app.location).is_dir() {
        return trash::delete(&app.location).map_err(|e| e.to_string());
    }
    Err("not_uninstallable".into())
}

#[tauri::command(async)]
fn reveal_installed(app: AppHandle, state: State<AppState>, id: String) -> CmdResult<()> {
    let target = find_installed(&state, &id)?;
    if target.location.is_empty() || !Path::new(&target.location).exists() {
        return Err("file_missing".into());
    }
    app.opener().reveal_item_in_dir(&target.location).map_err(|e| e.to_string())
}

// ── 图标与简介 ───────────────────────────────────────────────

/// 给没有图标的软件补图标，一次最多 8 个，不拖慢别的操作。
#[tauri::command(async)]
fn fill_icons(state: State<AppState>) -> CmdResult<FillResult> {
    let apps = state.installed.lock().map(|list| list.clone()).unwrap_or_default();
    let mut tried = state.icon_tried.lock().map_err(|_| "internal".to_string())?;
    with_repo(&state, |repo| {
        let (changed, more) = repo.fill_icons(8, &mut tried, &apps)?;
        Ok(FillResult { library: changed.then(|| repo.lib.clone()), more })
    })
}

/// 联网取一个软件的简介并存进索引。网络不通时返回 `offline`，什么都不记。
#[tauri::command(async)]
fn fetch_summary(state: State<AppState>, id: String) -> CmdResult<Library> {
    let (name, language) = with_repo(&state, |repo| {
        let software = repo.lib.software.iter().find(|s| s.id == id).ok_or(shibapack_core::Error::Code("not_found"))?;
        Ok((software.name.clone(), repo.lib.settings.language.clone()))
    })?;
    // 联网期间不占着仓库的锁，别的操作照常进行
    let found = summary::fetch_summary(&name, &language).map_err(|_| "offline".to_string())?;
    mutate(&state, |repo| repo.set_summary(&name, found))
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
#[tauri::command(async)]
fn source_lookup(kind: SourceKind, reference: String) -> CmdResult<Release> {
    sources::fetch_release(kind, &reference).map_err(|e| e.code().to_string())
}

/// 在 Homebrew 目录里搜索。目录缓存在仓库的 cache 文件夹里。
#[tauri::command(async)]
fn source_search(state: State<AppState>, query: String) -> CmdResult<Vec<Candidate>> {
    let cache = with_repo(&state, |repo| Ok(repo.data_dir().join("cache")))?;
    sources::search_homebrew(&cache, &query, 30).map_err(|e| e.code().to_string())
}

/// 给一个软件关联在线源。`software_id` 为空时按 `name` 新建一个软件（从在线源添加）。
#[tauri::command(async)]
fn subscribe(
    state: State<AppState>,
    software_id: Option<String>,
    name: Option<String>,
    kind: SourceKind,
    reference: String,
) -> CmdResult<SubscribeResult> {
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
}

#[tauri::command(async)]
fn unsubscribe(state: State<AppState>, software_id: String) -> CmdResult<Library> {
    mutate(&state, |repo| repo.set_subscription(&software_id, None))
}

/// 检查所有关联了在线源的软件有没有新版本。进度通过 `sources://progress` 事件发给前端。
#[tauri::command(async)]
fn check_updates(app: AppHandle, state: State<AppState>) -> CmdResult<CheckResult> {
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
}

/// 下载在线源上的一个安装包并收进仓库。进度通过 `download://progress` 事件发给前端。
#[tauri::command(async)]
fn download_asset(app: AppHandle, state: State<AppState>, software_id: String, url: String) -> CmdResult<Library> {
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
#[tauri::command(async)]
fn tidy_root(state: State<AppState>) -> CmdResult<ImportResult> {
    with_repo(&state, |repo| {
        let report = repo.tidy_root()?;
        Ok(ImportResult { report, library: repo.lib.clone() })
    })
}

#[tauri::command(async)]
fn list_lang_packs(state: State<AppState>) -> CmdResult<Vec<serde_json::Value>> {
    with_repo(&state, |repo| Ok(repo.lang_packs()))
}

#[tauri::command(async)]
fn import_lang_pack(state: State<AppState>, src: String) -> CmdResult<Vec<serde_json::Value>> {
    with_repo(&state, |repo| {
        repo.import_lang_pack(Path::new(&src))?;
        Ok(repo.lang_packs())
    })
}

#[tauri::command(async)]
fn remove_lang_pack(state: State<AppState>, locale: String) -> CmdResult<Vec<serde_json::Value>> {
    with_repo(&state, |repo| {
        repo.remove_lang_pack(&locale)?;
        Ok(repo.lang_packs())
    })
}

/// 把一份语言包（界面当前的全部文字）写到用户选的文件里。
#[tauri::command(async)]
fn export_lang_pack(dest: String, pack: serde_json::Map<String, serde_json::Value>) -> CmdResult<()> {
    let json = serde_json::to_vec_pretty(&pack).map_err(|e| e.to_string())?;
    fs::write(dest, json).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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

            // 上次用的仓库还在就直接打开；不在了（比如移动硬盘没插）就回到引导页
            let repo = fs::read(&config_file)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Bootstrap>(&bytes).ok())
                .and_then(|bootstrap| bootstrap.repo_root)
                .filter(|root| Path::new(root).is_dir())
                .and_then(|root| Repo::open(Path::new(&root)).ok());
            if let Some(repo) = &repo {
                allow_repo_icons(app.handle(), repo);
            }
            app.manage(AppState {
                repo: Mutex::new(repo),
                config_file,
                icon_cache,
                installed: Mutex::new(Vec::new()),
                icon_tried: Mutex::new(HashSet::new()),
                selftest: std::env::var("SHIBAPACK_SELFTEST").ok(),
                download_turn: Mutex::new(()),
                cancels: Mutex::new(HashMap::new()),
                version: app.package_info().version.to_string(),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            set_repo_root,
            get_library,
            import_paths,
            update_software,
            update_version,
            delete_software,
            delete_version,
            delete_file,
            run_file,
            reveal_file,
            open_repo_folder,
            set_categories,
            set_tags,
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
            reveal_installed,
            fill_icons,
            fetch_summary,
            selftest_report,
            source_lookup,
            source_search,
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
        ])
        .run(tauri::generate_context!())
        .expect("ShibaPack 启动失败");
}
