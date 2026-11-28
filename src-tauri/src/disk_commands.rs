use super::*;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DiskLocation {
    path: String,
    name: String,
    /// home：用户文件夹；disk：一块盘。
    kind: String,
    total: u64,
    available: u64,
}

/// 可以分析的地方：用户文件夹和每一块盘。
#[tauri::command]
pub(super) async fn disk_locations(app: AppHandle) -> Vec<DiskLocation> {
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
pub(super) struct DiskSummary {
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
pub(super) async fn disk_live_view(app: AppHandle, path: Vec<String>, min_ratio: f64) -> Option<shibapack_core::disk::ViewNode> {
    blocking(app, move |_app, state| {
        let live = state.disk_live.lock().ok()?.clone()?;
        live.view(&path, min_ratio.clamp(0.0, 0.5))
    })
    .await
}

/// 扫描中到现在为止最大的文件（没在扫时是空的）。
#[tauri::command]
pub(super) async fn disk_live_largest(app: AppHandle) -> Vec<LargeFile> {
    blocking(app, move |_app, state| {
        let Some(live) = state.disk_live.lock().ok().and_then(|slot| slot.clone()) else { return Vec::new() };
        let root = live.root().to_path_buf();
        large_files(&root, live.largest())
    })
    .await
}

/// 扫一个文件夹或一块盘（只读）。扫的时候发 `disk://progress`，界面可以用 `disk_live_view` 先看；停下了返回 `cancelled`。
#[tauri::command]
pub(super) async fn disk_scan(app: AppHandle, root: String) -> CmdResult<DiskSummary> {
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
pub(super) fn disk_cancel(state: State<AppState>) {
    state.disk_cancel.store(true, Ordering::SeqCst);
}

/// 从扫完的树里取一块给界面画：`path` 是相对扫描根的路径段，往下 `depth` 层，太小的合成“其他”。
#[tauri::command]
pub(super) async fn disk_view(app: AppHandle, path: Vec<String>, depth: usize, min_ratio: f64) -> CmdResult<shibapack_core::disk::ViewNode> {
    blocking(app, move |_app, state| {
        let slot = state.disk.lock().map_err(|_| "internal".to_string())?;
        let scan = slot.as_ref().ok_or_else(|| "not_found".to_string())?;
        shibapack_core::disk::view(&scan.tree, &path, depth.min(8), min_ratio.clamp(0.0, 0.5)).ok_or_else(|| "not_found".to_string())
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LargeFile {
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
pub(super) async fn disk_largest(app: AppHandle, path: Vec<String>, limit: usize) -> CmdResult<Vec<LargeFile>> {
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
pub(super) async fn ai_explain_folders(app: AppHandle, paths: Vec<Vec<String>>, mode: String) -> CmdResult<HashMap<String, String>> {
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
        let language = read_repo(&state, |repo| Ok(repo.lib.settings.language.clone()))?;
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
pub(super) async fn disk_reveal(app: AppHandle, path: Vec<String>, open: bool) -> CmdResult<()> {
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
pub(super) async fn disk_full_path(app: AppHandle, path: Vec<String>) -> CmdResult<String> {
    blocking(app, move |_app, state| Ok(disk_path(&state, &path)?.0.to_string_lossy().to_string())).await
}

/// 移到回收站（废纸篓），并从树上去掉。系统文件夹、用户文件夹本身、应用程序、仓库这些不行，返回 `protected:<原因>`。
#[tauri::command]
pub(super) async fn disk_trash(app: AppHandle, path: Vec<String>) -> CmdResult<u64> {
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
