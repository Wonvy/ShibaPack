use super::{blocking, cache_dir, local_state_dir, log_failure_to, read_repo, repo_ai, AiProgress, AppState, CmdResult, Explanation, Explanations};
use serde::Serialize;
use shibapack_core::{ai, aicache::{self, AiCache, CachedText}, cleanup, Library};
use std::{collections::HashMap, path::{Path, PathBuf}, sync::{atomic::{AtomicBool, Ordering}, Arc}};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

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
pub(super) async fn cleanup_scan(app: AppHandle) -> CmdResult<Vec<cleanup::CleanItem>> {
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
pub(super) async fn cleanup_run(app: AppHandle, picks: Vec<cleanup::Pick>) -> CmdResult<cleanup::Report> {
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
pub(super) async fn cleanup_count_trash(app: AppHandle) -> CmdResult<Vec<cleanup::CleanItem>> {
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
pub(super) async fn cleanup_open_tool(app: AppHandle, id: String) -> CmdResult<()> {
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
pub(super) fn cleanup_cancel_scan(state: State<AppState>) {
    state.cleanup_cancel.store(true, Ordering::SeqCst);
}

/// 只重新量这几项（清理完以后），返回更新后的全部项。还没扫过时返回 `not_found`。
#[tauri::command]
pub(super) async fn cleanup_rescan(app: AppHandle, ids: Vec<String>) -> CmdResult<Vec<cleanup::CleanItem>> {
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
pub(super) async fn ai_explain_cleanup(app: AppHandle, ids: Vec<String>, mode: String) -> CmdResult<Explanations> {
    blocking(app, move |app, state| {
        let home = if cfg!(windows) { std::env::var_os("USERPROFILE") } else { std::env::var_os("HOME") }.map(PathBuf::from);
        let facts: Vec<(String, String)> = {
            let cached = state.cleanup.lock().map_err(|_| "internal".to_string())?;
            let scan = cached.as_ref().ok_or_else(|| "not_found".to_string())?;
            ids.iter().filter_map(|id| scan.facts(id, home.as_deref()).map(|text| (id.clone(), text))).collect()
        };
        let dir = cache_dir(&state)?;
        let language = read_repo(&state, |repo| Ok(repo.lib.settings.language.clone()))?;
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
pub(super) async fn cleanup_quit_app(app: AppHandle, id: String) -> CmdResult<cleanup::CleanItem> {
    blocking(app, move |_app, state| {
        let mut scan = state.cleanup.lock().map_err(|_| "internal".to_string())?;
        scan.as_mut().and_then(|scan| scan.quit_app(&id)).ok_or_else(|| "not_found".to_string())
    })
    .await
}

/// 在文件管理器里定位这一项的某一条，或者它的第一个文件夹。
#[tauri::command]
pub(super) async fn cleanup_reveal(app: AppHandle, id: String, index: Option<usize>) -> CmdResult<()> {
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
pub(super) async fn cleanup_cookie_sites(app: AppHandle) -> CmdResult<Vec<cleanup::CookieSite>> {
    blocking(app, move |app, state| Ok(cleanup::cookie_sites(&cleanup_context(app, &state)?))).await
}

/// 自定义的清理文件夹和排除的文件夹。
#[tauri::command]
pub(super) async fn cleanup_options(app: AppHandle) -> CmdResult<cleanup::Options> {
    blocking(app, move |app, _state| Ok(cleanup::load_options(&local_state_dir(app)?))).await
}

/// 保存自定义的清理文件夹和排除的文件夹。自定义文件夹要过保护检查，不行时返回 `cleanup_protected:路径` 这样的错误。
#[tauri::command]
pub(super) async fn cleanup_set_options(app: AppHandle, options: cleanup::Options) -> CmdResult<cleanup::Options> {
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
pub(super) async fn cleanup_history(app: AppHandle) -> CmdResult<Vec<cleanup::history::Run>> {
    blocking(app, move |app, _state| Ok(cleanup::history::list(&local_state_dir(app)?.join("cleanup-history"), false))).await
}
