use super::{blocking, read_repo_revision, with_repo_revision, AppState, CmdResult};
use serde::Serialize;
use shibapack_core::{identify::same_software, installed, leftovers::{self, Leftover, RemoveReport, Target}, InstalledApp};
use std::{collections::{BTreeMap, HashMap}, path::{Path, PathBuf}, sync::{atomic::Ordering, Arc, Mutex}, time::{Duration, Instant}};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

/// 一个软件的卸载过程：卸载前记下的样子和最近一次找到的项。可以同时卸载好几个，按已安装软件的 id 分开。
pub(super) struct UninstallSession {
    target: Target,
    found: Vec<Leftover>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LeftoverScan {
    items: Vec<Leftover>,
    /// 软件看起来还在（卸载程序可能被取消了）：找到的都没有默认勾选。卸载前的分析里总是假。
    still_installed: bool,
    /// 卸载前的分析里“App”那一行显示的：Windows 是登记的卸载命令，macOS 是 .app 的位置。
    #[serde(skip_serializing_if = "Option::is_none")]
    app_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    install_folder: Option<InstallFolder>,
}

#[derive(Serialize)]
struct InstallFolder {
    path: String,
    inferred: bool,
}

fn find_installed(state: &State<AppState>, id: &str) -> CmdResult<InstalledApp> {
    let list = state.installed.lock().map_err(|_| "internal".to_string())?;
    list.iter().find(|app| app.id == id).cloned().ok_or_else(|| "not_found".to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct InstalledListing {
    apps: Vec<InstalledApp>,
    cached_icons: Option<BTreeMap<String, Arc<str>>>,
}

/// 列出这台电脑上已安装的软件（会重新扫描）。
#[tauri::command]
pub(super) async fn list_installed(app: AppHandle, revision: Option<u32>) -> InstalledListing {
    blocking(app, move |_app, state| {
        let revision = revision.or_else(|| Some(state.repo_revision.load(Ordering::SeqCst)));
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
        let cached_icons = with_repo_revision(&state, revision, |repo| {
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
pub(super) async fn measure_installed(app: AppHandle) -> HashMap<String, u64> {
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
pub(super) async fn match_installed(app: AppHandle, revision: Option<u32>) -> CmdResult<HashMap<String, String>> {
    blocking(app, move |_app, state| {
        let apps = state.installed.lock().map(|list| list.clone()).unwrap_or_default();
        read_repo_revision(&state, revision, |repo| {
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
pub(super) async fn analyze_uninstall(app: AppHandle, id: String) -> CmdResult<LeftoverScan> {
    blocking(app, move |_app, state| {
        let app = find_installed(&state, &id)?;
        let target = Target::of(&app);
        let others: Vec<InstalledApp> = state.installed.lock().map(|list| list.iter().filter(|other| other.id != id).cloned().collect()).unwrap_or_default();
        let install_folder = leftovers::preview_install_dir(&target, &others).map(|(path, inferred)| InstallFolder {
            path: path.to_string_lossy().into_owned(),
            inferred,
        });
        let items = leftovers::scan_before(&target, &others);
        if let Ok(mut sessions) = state.uninstall.lock() {
            sessions.insert(id.clone(), UninstallSession { target, found: items.clone() });
        }
        let app_path = app.uninstall_command.clone().filter(|_| app.source == "registry").or_else(|| (!app.location.is_empty()).then(|| app.location.clone()));
        Ok(LeftoverScan { items, still_installed: false, app_path, install_folder })
    })
    .await
}

/// 卸载。Windows 上启动系统登记的卸载程序；macOS 上把 .app 移到废纸篓。
#[tauri::command]
pub(super) async fn uninstall_installed(app: AppHandle, id: String) -> CmdResult<()> {
    blocking(app, move |_app, state| {
        let app = find_installed(&state, &id)?;
        if !app.uninstallable {
            return Err("not_uninstallable".into());
        }
        // 卸载以后就查不到它的安装位置、发布者了，先记下来，找残留时用；分析过的话留着分析的结果
        if let Ok(mut sessions) = state.uninstall.lock() {
            sessions.entry(id.clone()).or_insert_with(|| UninstallSession { target: Target::of(&app), found: Vec::new() });
        }
        if installed::run_uninstaller(&app).map_err(|e| format!("uninstall_launch_failed: {e}"))? {
            return Ok(());
        }
        if cfg!(target_os = "macos") && Path::new(&app.location).is_dir() {
            trash::delete(&app.location).map_err(|e| e.to_string())?;
            if let Ok(mut installed) = state.installed.lock() {
                installed.retain(|app| app.id != id);
            }
            return Ok(());
        }
        Err("not_uninstallable".into())
    })
    .await
}

/// 卸载程序还没跑完（软件还在）。Windows 看卸载信息还在不在；macOS 是直接移到废纸篓，不用等。
#[tauri::command]
pub(super) async fn uninstall_status(app: AppHandle, id: String) -> CmdResult<bool> {
    blocking(app, move |_app, state| {
        let target = state.uninstall.lock().map_err(|_| "internal".to_string())?
            .get(&id).map(|s| s.target.clone()).ok_or_else(|| "not_found".to_string())?;
        let still_installed = leftovers::still_installed(&target);
        if !still_installed {
            if let Ok(mut installed) = state.installed.lock() {
                installed.retain(|app| app.id != id);
            }
        }
        Ok(still_installed)
    })
    .await
}

/// 找刚卸载的软件留下的文件夹、文件和注册表项。只读。
#[tauri::command]
pub(super) async fn scan_leftovers(app: AppHandle, id: String) -> CmdResult<LeftoverScan> {
    blocking(app, move |_app, state| {
        let target = state.uninstall.lock().ok().and_then(|s| s.get(&id).map(|s| s.target.clone())).ok_or_else(|| "not_found".to_string())?;
        let others: Vec<InstalledApp> = state.installed.lock().map(|list| list.iter().filter(|app| app.id != id).cloned().collect()).unwrap_or_default();
        let still_installed = leftovers::still_installed(&target);
        let items = leftovers::scan(&target, &others, still_installed);
        if let Some(session) = state.uninstall.lock().ok().as_mut().and_then(|s| s.get_mut(&id)) {
            session.found = items.clone();
        }
        Ok(LeftoverScan { items, still_installed, app_path: None, install_folder: None })
    })
    .await
}

/// 清理勾选的残留（按上次找到的列表里的序号）：文件和文件夹移到回收站，注册表项先备份再删。
/// `elevated` 为真时以管理员身份做（Windows 会弹 UAC 提示），用在普通权限删不掉的那些上。
#[tauri::command]
pub(super) async fn remove_leftovers(app: AppHandle, id: String, indices: Vec<usize>, elevated: bool) -> CmdResult<RemoveReport> {
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
pub(super) async fn reveal_leftover(app: AppHandle, id: String, index: usize) -> CmdResult<()> {
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
pub(super) async fn open_uninstall_backup(app: AppHandle, path: String) -> CmdResult<()> {
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
pub(super) async fn reveal_installed(app: AppHandle, id: String) -> CmdResult<()> {
    blocking(app, move |app, state| {
        let target = find_installed(&state, &id)?;
        if target.location.is_empty() || !Path::new(&target.location).exists() {
            return Err("file_missing".into());
        }
        app.opener().reveal_item_in_dir(&target.location).map_err(|e| e.to_string())
    })
    .await
}
