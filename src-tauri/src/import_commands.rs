use super::{ai_service, blocking, log_failure_to, read_repo, with_repo_quick, AppState, ClearOnDrop, CmdResult, ImportResult};
use shibapack_core::{ai, import::{self, BatchSaver, Outcome, Plan}, AtStep, ImportHint, ImportReport, Library, Progress, Storage, Suggestion};
use std::{collections::{HashMap, HashSet}, path::{Path, PathBuf}, sync::atomic::{AtomicBool, Ordering}};
use tauri::{AppHandle, Emitter, State};

/// 导入文件或文件夹。`move_files` 为真时（只对托管模式有效）导入后不留原文件。
/// 进度通过 `import://progress` 事件发给前端，收集、计算哈希和复制途中都可以取消。
///
/// 分批保存期间锁住其他设备的写入。算哈希、复制文件时不占本机互斥锁，界面上的其他操作照常进行。
/// 同时只跑一批导入，第二批会得到 `busy`。
#[tauri::command]
pub(super) async fn import_paths(app: AppHandle, paths: Vec<String>, mode: Storage, move_files: Option<bool>, hint: Option<ImportHint>) -> CmdResult<ImportResult> {
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
        let (root, data_dir) = read_repo(&state, |repo| Ok((repo.root().to_path_buf(), repo.data_dir())))?;
        // 展开文件夹也不占着仓库：NAS 上的大文件夹要走一会儿
        let cancelled = || state.import_cancel.load(Ordering::Relaxed);
        let files = match import::collect_files_cancellable(&paths, &data_dir, &hint.package_set(), &cancelled) {
            Ok(files) => files,
            Err(err) if err.reason() == "cancelled" => {
                progress("done", 0, 0, String::new());
                let library = read_repo(&state, |repo| Ok(repo.lib.clone()))?;
                return Ok(ImportResult { report: ImportReport { cancelled: true, ..ImportReport::default() }, library });
            }
            Err(err) => return Err(err.to_string()),
        };
        let (writer, mut known, auto_classify) = with_repo_quick(&state, Some(&root), |repo| {
            let writer = repo.begin_write()?;
            Ok((writer, repo.known_paths(), hint.uses_auto_classification(&repo.lib.settings)))
        }).map_err(|e| e.to_string())?;
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
                if err.reason() == "cancelled" {
                    report.cancelled = true;
                    break;
                }
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
        drop(writer);
        if auto_classify && report.imported > 0 && !cancelled() {
            progress("classify", 0, 0, String::new());
            let classification = if let Some(previews) = &hint.category_previews {
                let assignments = report.items.iter().filter(|item| item.status == "imported").filter_map(|item| {
                    let preview = previews.get(&item.path)?;
                    Some(Suggestion { software_id: item.software_id.clone()?, category_id: preview.category_id.clone(), new_category: preview.new_category.clone(), unchanged: false, known: false, evidence: Vec::new(), reason: String::new() })
                }).collect();
                with_repo_quick(&state, Some(&root), |repo| repo.transact(|repo| repo.apply_import_classification(assignments)))
                    .map(|count| report.classified = count).map_err(|e| e.to_string())
            } else {
                classify_import(&state, &root, &library, &mut report)
            };
            if let Err(error) = classification {
                if !state.import_cancel.load(Ordering::SeqCst) {
                    report.classification_error = Some(error);
                }
            }
            library = with_repo_quick(&state, Some(&root), |repo| Ok(repo.lib.clone())).map_err(|e| e.to_string())?;
        }
        report.cancelled |= cancelled();
        progress("done", report.items.len(), total, String::new());
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
    let cancelled = || state.import_cancel.load(Ordering::Relaxed);
    let hashed = import::hash_source_cancellable(file, &cancelled).at("read")?;
    let placement = match with_repo_quick(state, Some(root), |repo| Ok(repo.plan_import(&hashed, mode, hint)))? {
        Plan::Duplicate(owner) => return Ok(Outcome::Duplicate(owner)),
        Plan::Add(placement) => placement,
    };
    let placed = import::place_file_cancellable(root, &hashed, &placement, move_files, &cancelled).at("copy")?;
    with_repo_quick(state, Some(root), |repo| repo.register(&hashed, placement, placed, known)).at("save")
}

/// 导入前看一看拖进来的东西（给确认窗口用）：文件夹像不像一个软件、里面有几个安装包、会被认成什么。
#[tauri::command]
pub(super) async fn inspect_drop(app: AppHandle, paths: Vec<String>, include_children: Option<bool>) -> CmdResult<Vec<import::DropEntry>> {
    blocking(app, move |_app, state| {
        let (data_dir, software_names) = read_repo(&state, |repo| {
            let names: Vec<_> = repo.lib.software.iter().map(|s| (s.id.clone(), s.name.clone())).collect();
            Ok((repo.data_dir(), names))
        })?;
        let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        Ok(import::inspect(&paths, &data_dir, &software_names, include_children.unwrap_or(false)))
    })
    .await
}

#[tauri::command]
pub(super) async fn preview_import_categories(
    app: AppHandle,
    targets: Vec<shibapack_core::classify::ImportCategoryTarget>,
) -> CmdResult<Vec<shibapack_core::classify::ImportCategoryPreview>> {
    blocking(app, move |_app, state| {
        let (mut previews, mut library) = read_repo(&state, |repo| Ok((repo.preview_import_categories(&targets), repo.lib.clone())))?;
        let Ok(service) = ai_service(&library.settings.ai, None) else { return Ok(previews) };
        let mut ids = Vec::new();
        let mut indices = HashMap::new();
        for (index, (target, preview)) in targets.iter().zip(&previews).enumerate() {
            if preview.category_id.is_some() || preview.new_category.is_some() { continue; }
            let id = format!("import-preview-{index}");
            indices.insert(id.clone(), index);
            ids.push(id.clone());
            library.software.push(shibapack_core::Software {
                id, name: target.name.clone(), publisher: target.publisher.clone(), homepage: String::new(), note: String::new(),
                category_id: None, starred: false, icon: None, icon_data: None, default_version_id: None,
                added_at: String::new(), versions: Vec::new(), subscription: None,
            });
        }
        if !ids.is_empty() {
            let cancel = AtomicBool::new(false);
            let suggestions = ai::suggest_categories(&service, &library, &ids, &cancel, &mut |_, _| {}).map_err(|e| e.code())?;
            for suggestion in suggestions {
                if let Some(&index) = indices.get(&suggestion.software_id) {
                    previews[index].category_id = suggestion.category_id;
                    previews[index].new_category = suggestion.new_category;
                }
            }
        }
        Ok(previews)
    }).await
}

/// 停下正在进行的导入。已经导入的保留。
#[tauri::command]
pub(super) fn cancel_import(state: State<AppState>) {
    state.import_cancel.store(true, Ordering::SeqCst);
}
