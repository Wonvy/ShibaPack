use super::*;

// ── 图标与简介 ───────────────────────────────────────────────

/// 给没有图标的软件补图标，一次最多 8 个，不拖慢别的操作。
#[tauri::command]
pub(super) async fn fill_icons(app: AppHandle, revision: Option<u32>) -> CmdResult<FillResult> {
    blocking(app, move |_app, state| {
        let apps = state.installed.lock().map(|list| list.clone()).unwrap_or_default();
        let mut tried = state.icon_tried.lock().map_err(|_| "internal".to_string())?;
        with_repo_revision(&state, revision, |repo| {
            let (changed, more) = repo.fill_icons(8, &mut tried, &apps)?;
            Ok(FillResult { library: changed.then(|| repo.lib.clone()), more })
        })
    })
    .await
}

/// 用安装包自带的属性补版本号和发布者（版本号是空的文件挪到读到的版本），一次最多读 8 个文件。
/// 读过的记在仓库的缓存里，同一个文件不再读。
#[tauri::command]
pub(super) async fn fill_file_info(app: AppHandle, revision: Option<u32>) -> CmdResult<FillResult> {
    blocking(app, move |_app, state| {
        with_repo_revision(&state, revision, |repo| {
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
pub(super) struct IconUpdate {
    icon: String,
    icon_data: Option<Arc<str>>,
}

#[tauri::command]
pub(super) async fn refresh_icon(app: AppHandle, id: String, revision: Option<u32>) -> CmdResult<Option<IconUpdate>> {
    blocking(app, move |_app, state| {
        let apps = state.installed.lock().map(|list| list.clone()).unwrap_or_default();
        with_repo_revision(&state, revision, |repo| {
            let Some(icon) = repo.refresh_icon(&id, &apps)? else { return Ok(None) };
            let icon_data = repo.lib.software.iter().find(|software| software.id == id).and_then(|software| software.icon_data.clone());
            Ok(Some(IconUpdate { icon, icon_data }))
        })
    })
    .await
}

#[tauri::command]
pub(super) async fn fetch_icon(app: AppHandle, id: String, revision: Option<u32>) -> CmdResult<Option<IconUpdate>> {
    blocking(app, move |_app, state| {
        let revision = revision.or_else(|| Some(state.repo_revision.load(Ordering::SeqCst)));
        let (root, query) = read_repo_revision(&state, revision, |repo| Ok((repo.root().to_path_buf(), repo.web_icon_query(&id)?)))?;
        let Some(query) = query else { return Ok(None) };
        // 联网期间不占着仓库的锁
        let found = webicon::fetch_web_icon(&query.homepage, query.github.as_deref(), query.the_svg_name.as_deref())
            .map_err(|_| "offline".to_string())?;
        with_repo_revision(&state, revision, |repo| {
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
pub(super) async fn fetch_summary(app: AppHandle, name: String, refresh: Option<bool>, revision: Option<u32>) -> CmdResult<Library> {
    blocking(app, move |_app, state| {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err("not_found".into());
        }
        let revision = revision.or_else(|| Some(state.repo_revision.load(Ordering::SeqCst)));
        let (language, existing, translate) = read_repo_revision(&state, revision, |repo| {
            let settings = &repo.lib.settings;
            let existing = if refresh.unwrap_or(false) { None } else { repo.lib.summaries.get(&name).cloned() };
            Ok((settings.language.clone(), existing, settings.translate_summaries))
        })?;
        // 联网期间不占着仓库的锁，别的操作照常进行
        let (summary, mut library) = match existing {
            Some(summary) => (Some(summary), None),
            None => {
                let found = summary::fetch_summary(&name, &language).map_err(|_| "offline".to_string())?;
                let library = mutate_revision(&state, revision, |repo| repo.set_summary(&name, found.clone()))?;
                (found, Some(library))
            }
        };
        if let Some(summary) = summary.filter(|s| translate && summary::needs_translation(s, &language)) {
            let text = summary::translate(&summary.text, &summary::summary_language(&summary), &language).map_err(|_| "offline".to_string())?;
            library = Some(mutate_revision(&state, revision, |repo| repo.set_summary_translation(&name, &language, text))?);
        }
        match library {
            Some(library) => Ok(library),
            None => read_repo_revision(&state, revision, |repo| Ok(repo.lib.clone())),
        }
    })
    .await
}
