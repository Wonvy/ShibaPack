use super::*;
use shibapack_core::repository_repair::RepairFile;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VerifyProgress<'a> {
    job: &'a str,
    checked: usize,
    total: usize,
}

#[tauri::command]
pub(super) async fn verify_data(app: AppHandle, revision: Option<u32>, deep: Option<bool>, job: Option<String>) -> CmdResult<VerifyReport> {
    blocking(app, move |app, state| {
        let revision = revision.or_else(|| Some(state.repo_revision.load(Ordering::SeqCst)));
        let target = read_repo_revision(&state, revision, |repo| Ok(repo.verification()))?;
        let job = job.unwrap_or_else(shibapack_core::new_id);
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut active = state.verification.lock().map_err(|_| "internal".to_string())?;
            if active.is_some() {
                return Err("busy".into());
            }
            *active = Some((job.clone(), cancel.clone()));
        }
        let cancelled = || cancel.load(Ordering::Relaxed) || validate_repo_revision(&state, revision).is_err();
        let mut last = Instant::now();
        let result = target.run(deep.unwrap_or(false), &cancelled, &mut |checked, total| {
            if checked == 0 || checked == total || last.elapsed() >= Duration::from_millis(100) {
                last = Instant::now();
                let _ = app.emit("verify://progress", VerifyProgress { job: &job, checked, total });
            }
        });
        if let Ok(mut active) = state.verification.lock() {
            *active = None;
        }
        validate_repo_revision(&state, revision).map_err(|e| e.to_string())?;
        result.map_err(|e| e.to_string())
    }).await
}

#[tauri::command]
pub(super) fn cancel_verify(state: State<AppState>, job: String) {
    if let Ok(active) = state.verification.lock() {
        if let Some((_, cancel)) = active.as_ref().filter(|(id, _)| *id == job) {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RelinkResult {
    library: Library,
    linked: Vec<String>,
}

#[tauri::command]
pub(super) async fn relink_data(
    app: AppHandle, files: Vec<RepairFile>, replacement: String, old_root: Option<String>, revision: u32,
) -> CmdResult<RelinkResult> {
    blocking(app, move |_app, state| {
        let candidates = read_repo_revision(&state, Some(revision), |repo| {
            repo.plan_relink(files, Path::new(&replacement), old_root.as_deref().map(Path::new))
        })?;
        let mut found = Vec::new();
        for candidate in candidates {
            validate_repo_revision(&state, Some(revision)).map_err(|e| e.to_string())?;
            found.extend(candidate.verify());
        }
        with_repo_revision(&state, Some(revision), |repo| {
            let linked = repo.apply_relinks(found)?;
            Ok(RelinkResult { library: repo.lib.clone(), linked })
        })
    }).await
}
