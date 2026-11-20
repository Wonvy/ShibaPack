//! 收藏的路径：软件的配置文件常放在别处（`~/Library/Application Support`、`%APPDATA%`），
//! 收藏起来写上备注，随时备份进仓库，需要时恢复回去。
//!
//! 路径存成可移植的写法：家目录写成 `~`，Windows 上的常见位置写成 `%APPDATA%` 这样的环境变量。
//! 手动输入时可以直接写 `~`、`%NAME%`、`$NAME`、`${NAME}`，用的时候才展开。
//!
//! 备份放在 `.shibapack/paths/<id>/<时间>/`：`content/` 里是原样复制的文件或文件夹，`backup.json` 记着这次备份的情况。

use crate::{model::SavedPath, store::Repo, Error, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

const BACKUP_INFO: &str = "backup.json";
const CONTENT_DIR: &str = "content";
/// 复制不了的文件最多记这么多个名字，再多只记个数。
const SKIPPED_NAMES: usize = 50;

/// 一次备份。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathBackup {
    /// 文件夹名（时间），删除、恢复时用它指明是哪一份。
    #[serde(default)]
    pub name: String,
    pub created_at: String,
    /// 备份时展开后的完整路径。
    pub source: String,
    pub size: u64,
    pub files: u64,
    /// 复制不了的文件（没有权限、被占用），相对备份的那个文件夹。
    #[serde(default)]
    pub skipped: Vec<String>,
    #[serde(default)]
    pub skipped_count: u64,
    /// 恢复之前自动做的备份。
    #[serde(default)]
    pub before_restore: bool,
}

/// 一条收藏在这台电脑上的情况。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathState {
    /// 展开后的完整路径；有不认识的环境变量时是空的。
    pub full: String,
    pub exists: bool,
    pub is_dir: bool,
    /// 展开不了的原因，比如 `path_unknown_var: XDG_CONFIG_HOME`。
    pub error: Option<String>,
    pub backups: usize,
    pub last_backup: Option<String>,
    pub backup_size: u64,
    /// 这个路径能写成的几种样子（见 `forms`），在详情栏里切换。展开不了时是空的。
    pub forms: Vec<String>,
}

// ── 路径的写法 ───────────────────────────────────────────────

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).filter(|h| !h.is_empty()).map(PathBuf::from)
}

fn env(name: &str) -> Option<String> {
    if name.eq_ignore_ascii_case("HOME") {
        return home().map(|h| h.to_string_lossy().to_string());
    }
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// 展开 `~`、`%NAME%`、`$NAME`、`${NAME}`。不认识的变量报 `path_unknown_var: NAME`；展开后不是绝对路径报 `path_not_absolute`。
pub fn expand(text: &str) -> Result<PathBuf> {
    expand_with(text, &env)
}

fn expand_with(text: &str, lookup: &dyn Fn(&str) -> Option<String>) -> Result<PathBuf> {
    let text = text.trim().trim_matches('"');
    // 界面按 `代码: 补充` 翻译
    let unknown = |name: &str| Error::Io(std::io::Error::other(format!("path_unknown_var: {name}")));
    let mut out = String::new();
    let mut rest = text;
    if rest == "~" || rest.starts_with("~/") || rest.starts_with("~\\") {
        out.push_str(&lookup("HOME").ok_or_else(|| unknown("HOME"))?);
        rest = &rest[1..];
    }
    while let Some(at) = rest.find(['%', '$']) {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let (name, used) = if let Some(body) = tail.strip_prefix('%') {
            match body.find('%') {
                Some(end) if end > 0 && body[..end].chars().all(is_var_char) => (&body[..end], end + 2),
                _ => ("", 0),
            }
        } else if let Some(body) = tail.strip_prefix("${") {
            match body.find('}') {
                Some(end) if end > 0 => (&body[..end], end + 3),
                _ => ("", 0),
            }
        } else {
            let end = tail[1..].find(|c: char| !is_var_char(c)).unwrap_or(tail.len() - 1);
            (&tail[1..1 + end], end + 1)
        };
        if name.is_empty() {
            // 不是变量，比如文件名里的 `%` 或 `$`
            out.push_str(&tail[..1]);
            rest = &tail[1..];
            continue;
        }
        out.push_str(&lookup(name).ok_or_else(|| unknown(name))?);
        rest = &tail[used..];
    }
    out.push_str(rest);
    let path = PathBuf::from(&out);
    if !path.is_absolute() {
        return Err(Error::Code("path_not_absolute"));
    }
    Ok(crate::store::clean_path(&path))
}

fn is_var_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '(' || c == ')'
}

/// 把一个绝对路径换成可移植的写法：Windows 上用最贴近的环境变量（`%APPDATA%` 等），其余系统把家目录换成 `~`。
/// 已经带着变量（手动输入的）就原样留着。
pub fn portable(text: &str) -> String {
    let text = text.trim().trim_matches('"');
    if has_vars(text) {
        return text.to_string();
    }
    forms_with(text, cfg!(windows), &env).remove(0)
}

fn has_vars(text: &str) -> bool {
    text.starts_with('~') || text.contains('%') || text.contains('$')
}

/// 一个路径能写成的几种样子，最推荐的在前：用最贴近的变量、用范围大一点的变量……最后是完整路径。
/// 比如 `%LOCALAPPDATA%\x`、`%USERPROFILE%\AppData\Local\x`、`C:\Users\demo\AppData\Local\x`。
pub fn forms(full: &Path) -> Vec<String> {
    forms_with(&full.to_string_lossy(), cfg!(windows), &env)
}

fn forms_with(text: &str, windows: bool, lookup: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    let names: &[&str] = if windows {
        &["LOCALAPPDATA", "APPDATA", "PROGRAMDATA", "USERPROFILE", "ProgramFiles(x86)", "ProgramFiles", "SystemRoot"]
    } else {
        &["HOME"]
    };
    let sep = if windows { '\\' } else { '/' };
    let mut found: Vec<(usize, String)> = Vec::new();
    for name in names {
        let Some(value) = lookup(name) else { continue };
        let value = value.trim_end_matches(['/', '\\']);
        if value.is_empty() || value.len() > text.len() || !text.is_char_boundary(value.len()) {
            continue;
        }
        let head = &text[..value.len()];
        let same = if windows { head.eq_ignore_ascii_case(value) } else { head == value };
        let boundary = text[value.len()..].is_empty() || text[value.len()..].starts_with(['/', '\\']);
        if !same || !boundary {
            continue;
        }
        let var = if windows { format!("%{name}%") } else { "~".to_string() };
        let tail = text[value.len()..].trim_start_matches(['/', '\\']);
        let form = if tail.is_empty() { var } else { format!("{var}{sep}{tail}") };
        if !found.iter().any(|(_, f)| *f == form) {
            found.push((value.len(), form));
        }
    }
    // 变量对应的文件夹越深越贴近，排在前面
    found.sort_by(|a, b| b.0.cmp(&a.0));
    let mut list: Vec<String> = found.into_iter().map(|(_, form)| form).collect();
    list.push(text.to_string());
    list
}

/// 两个展开后的路径是不是同一个（Windows 上不分大小写）。
fn same_path(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
    } else {
        a == b
    }
}

// ── 收藏 ─────────────────────────────────────────────────────

/// 收藏一批路径（拖进来的、选的、手动输入的）。已经收藏过的跳过。返回新加的 id。
/// `convert` 时把绝对路径换成变量的写法（见 `portable`）；不换时原样记下。
pub fn add(repo: &mut Repo, inputs: &[String], convert: bool) -> Result<Vec<String>> {
    let mut added = Vec::new();
    for input in inputs {
        let text = if convert { portable(input) } else { input.trim().trim_matches('"').to_string() };
        if text.is_empty() {
            continue;
        }
        let full = expand(&text)?;
        let known = repo.lib.paths.iter().any(|p| expand(&p.path).is_ok_and(|other| same_path(&other, &full)));
        if known {
            continue;
        }
        let id = crate::new_id();
        repo.lib.paths.push(SavedPath { id: id.clone(), path: text, note: String::new(), is_dir: full.is_dir(), added_at: crate::now() });
        added.push(id);
    }
    if !added.is_empty() {
        repo.save()?;
    }
    Ok(added)
}

/// 改路径和备注。路径改了时重新看一下是不是文件夹。
/// `convert` 时把绝对路径换成变量的写法；不换时照写（切换写法时用，见 `forms`）。
pub fn edit(repo: &mut Repo, id: &str, path: Option<&str>, note: Option<&str>, convert: bool) -> Result<()> {
    let item = repo.lib.paths.iter_mut().find(|p| p.id == id).ok_or(Error::Code("not_found"))?;
    if let Some(path) = path {
        let text = if convert { portable(path) } else { path.trim().trim_matches('"').to_string() };
        let full = expand(&text)?;
        item.path = text;
        if full.exists() {
            item.is_dir = full.is_dir();
        }
    }
    if let Some(note) = note {
        item.note = note.trim().to_string();
    }
    repo.save()
}

/// 取消收藏。`delete_backups` 时连同它的备份一起删掉（送进系统回收站）。
pub fn remove(repo: &mut Repo, ids: &[String], delete_backups: bool) -> Result<()> {
    let root = backups_root(repo);
    repo.lib.paths.retain(|p| !ids.contains(&p.id));
    repo.save_with_backup()?;
    if delete_backups {
        for id in ids {
            let dir = root.join(id_dir(id)?);
            if dir.exists() {
                let _ = trash::delete(&dir);
            }
        }
    }
    Ok(())
}

pub fn backups_root(repo: &Repo) -> PathBuf {
    repo.data_dir().join("paths")
}

/// id、备份名只能是一层普通的名字，不能借它跑出备份文件夹。
fn id_dir(name: &str) -> Result<&str> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', ':']) {
        return Err(Error::Code("not_found"));
    }
    Ok(name)
}

/// 每条收藏在这台电脑上的情况。
pub fn states(items: &[SavedPath], backups_root: &Path) -> Vec<(String, PathState)> {
    items
        .iter()
        .map(|item| {
            let backups = id_dir(&item.id).map(|id| list_backups(&backups_root.join(id))).unwrap_or_default();
            let state = match expand(&item.path) {
                Ok(full) => {
                    let meta = fs::metadata(&full).ok();
                    PathState {
                        full: full.to_string_lossy().to_string(),
                        exists: meta.is_some(),
                        is_dir: meta.map_or(item.is_dir, |m| m.is_dir()),
                        error: None,
                        backups: backups.len(),
                        last_backup: backups.first().map(|b| b.created_at.clone()),
                        backup_size: backups.iter().map(|b| b.size).sum(),
                        forms: forms(&full),
                    }
                }
                Err(err) => PathState {
                    full: String::new(),
                    exists: false,
                    is_dir: item.is_dir,
                    error: Some(err.to_string()),
                    backups: backups.len(),
                    last_backup: backups.first().map(|b| b.created_at.clone()),
                    backup_size: backups.iter().map(|b| b.size).sum(),
                    forms: Vec::new(),
                },
            };
            (item.id.clone(), state)
        })
        .collect()
}

/// 一条收藏的所有备份，最新的在前。
pub fn list_backups(dir: &Path) -> Vec<PathBackup> {
    let mut list: Vec<PathBackup> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            let text = fs::read(entry.path().join(BACKUP_INFO)).ok()?;
            let mut info: PathBackup = serde_json::from_slice(&text).ok()?;
            info.name = name;
            Some(info)
        })
        .collect();
    list.sort_by(|a, b| b.name.cmp(&a.name));
    list
}

// ── 备份与恢复 ───────────────────────────────────────────────

/// 把一条收藏现在的样子复制一份到 `dir`（这条收藏的备份文件夹）。不占着仓库：复制可能要一会儿。
/// `repo_root` 用来拦住把仓库自己（或者仓库所在的文件夹）收藏进来再备份的情况。
pub fn backup(item: &SavedPath, dir: &Path, repo_root: &Path, before_restore: bool) -> Result<PathBackup> {
    let source = expand(&item.path)?;
    let meta = fs::symlink_metadata(&source).map_err(|_| Error::Code("file_missing"))?;
    if repo_root.starts_with(&source) || source.starts_with(repo_root.join(crate::store::DATA_DIR)) {
        return Err(Error::Code("path_in_library"));
    }
    let name = loop {
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S-%3f").to_string();
        if !dir.join(&stamp).exists() {
            break stamp;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    };
    let target = dir.join(&name);
    let content = target.join(CONTENT_DIR);
    fs::create_dir_all(&content)?;
    let leaf = source.file_name().map(|n| n.to_os_string()).unwrap_or_else(|| "root".into());
    let mut info = PathBackup {
        name: name.clone(),
        created_at: crate::now(),
        source: source.to_string_lossy().to_string(),
        size: 0,
        files: 0,
        skipped: Vec::new(),
        skipped_count: 0,
        before_restore,
    };
    let copied = if meta.is_dir() {
        copy_tree(&source, &content.join(&leaf), Path::new(""), &mut info);
        Ok(())
    } else {
        fs::copy(&source, content.join(&leaf)).map(|size| {
            info.size = size;
            info.files = 1;
        })
    };
    if let Err(err) = copied {
        let _ = fs::remove_dir_all(&target);
        return Err(err.into());
    }
    fs::write(target.join(BACKUP_INFO), serde_json::to_vec_pretty(&info)?)?;
    Ok(info)
}

/// 复制一个文件夹。复制不了的文件跳过、记下来，不让一个被占用的文件挡住整次备份。
fn copy_tree(src: &Path, dst: &Path, rel: &Path, info: &mut PathBackup) {
    let skip = |info: &mut PathBackup, rel: &Path| {
        info.skipped_count += 1;
        if info.skipped.len() < SKIPPED_NAMES {
            info.skipped.push(rel.to_string_lossy().to_string());
        }
    };
    if fs::create_dir_all(dst).is_err() {
        return skip(info, rel);
    }
    let Ok(entries) = fs::read_dir(src) else { return skip(info, rel) };
    for entry in entries.flatten() {
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let rel = rel.join(entry.file_name());
        let Ok(kind) = entry.file_type() else {
            skip(info, &rel);
            continue;
        };
        if kind.is_symlink() {
            // 链接原样复制成链接，不跟进去（可能指向很大的地方，或者绕回自己）
            #[cfg(unix)]
            if let Ok(link) = fs::read_link(&from) {
                if std::os::unix::fs::symlink(link, &to).is_ok() {
                    continue;
                }
            }
            skip(info, &rel);
        } else if kind.is_dir() {
            copy_tree(&from, &to, &rel, info);
        } else {
            match fs::copy(&from, &to) {
                Ok(size) => {
                    info.size += size;
                    info.files += 1;
                }
                Err(_) => skip(info, &rel),
            }
        }
    }
}

/// 用一份备份替换现在的文件或文件夹。现在的先备份一份（标成“恢复前”），再送进系统回收站，然后把备份复制回去。
/// 返回恢复前的那份备份（原来的不在时没有）。
pub fn restore(item: &SavedPath, dir: &Path, name: &str, repo_root: &Path) -> Result<Option<PathBackup>> {
    let backup_dir = dir.join(id_dir(name)?);
    let content = backup_dir.join(CONTENT_DIR);
    let stored = fs::read_dir(&content)
        .map_err(|_| Error::Code("not_found"))?
        .flatten()
        .next()
        .ok_or(Error::Code("not_found"))?
        .path();
    let target = expand(&item.path)?;
    let before = if fs::symlink_metadata(&target).is_ok() {
        let before = backup(item, dir, repo_root, true)?;
        trash::delete(&target).map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;
        Some(before)
    } else {
        None
    };
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    if stored.is_dir() {
        let mut info = PathBackup {
            name: String::new(),
            created_at: String::new(),
            source: String::new(),
            size: 0,
            files: 0,
            skipped: Vec::new(),
            skipped_count: 0,
            before_restore: false,
        };
        copy_tree(&stored, &target, Path::new(""), &mut info);
        if info.skipped_count > 0 {
            return Err(Error::Code("restore_partial"));
        }
    } else {
        fs::copy(&stored, &target)?;
    }
    Ok(before)
}

/// 删掉一份备份（送进系统回收站）。
pub fn delete_backup(dir: &Path, name: &str) -> Result<()> {
    let path = dir.join(id_dir(name)?);
    if !path.join(BACKUP_INFO).exists() {
        return Err(Error::Code("not_found"));
    }
    trash::delete(&path).or_else(|_| fs::remove_dir_all(&path).map_err(|e| e.to_string())).map_err(|e| Error::Io(std::io::Error::other(e)))
}

/// 一份备份里放内容的那个文件或文件夹（在文件管理器里显示用）。
pub fn backup_content(dir: &Path, name: &str) -> Result<PathBuf> {
    let content = dir.join(id_dir(name)?).join(CONTENT_DIR);
    Ok(fs::read_dir(&content).ok().and_then(|mut entries| entries.next()).and_then(|e| e.ok()).map(|e| e.path()).unwrap_or(content))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(name: &str) -> Option<String> {
        match name {
            "HOME" => Some("/Users/demo".into()),
            "APPDATA" => Some(r"C:\Users\demo\AppData\Roaming".into()),
            "LOCALAPPDATA" => Some(r"C:\Users\demo\AppData\Local".into()),
            "USERPROFILE" => Some(r"C:\Users\demo".into()),
            "XDG" => Some("/x".into()),
            _ => None,
        }
    }

    #[test]
    fn portable_picks_closest_variable() {
        let portable_with = |text: &str, windows: bool, lookup: &dyn Fn(&str) -> Option<String>| forms_with(text, windows, lookup).remove(0);
        assert_eq!(portable_with("/Users/demo/Library/Prefs", false, &vars), "~/Library/Prefs");
        assert_eq!(portable_with("/Users/demo", false, &vars), "~");
        assert_eq!(portable_with("/Users/demon/x", false, &vars), "/Users/demon/x");
        assert_eq!(portable_with(r"c:\users\demo\AppData\Roaming\Code\User", true, &vars), r"%APPDATA%\Code\User");
        assert_eq!(portable_with(r"C:\Users\demo\AppData\Local\x", true, &vars), r"%LOCALAPPDATA%\x");
        assert_eq!(portable_with(r"C:\Users\demo\.ssh", true, &vars), r"%USERPROFILE%\.ssh");
        assert_eq!(portable_with(r"D:\Tools", true, &vars), r"D:\Tools");
    }

    #[test]
    fn forms_list_every_variable_then_the_full_path() {
        assert_eq!(
            forms_with(r"C:\Users\demo\AppData\Local\x", true, &vars),
            [r"%LOCALAPPDATA%\x", r"%USERPROFILE%\AppData\Local\x", r"C:\Users\demo\AppData\Local\x"]
        );
        assert_eq!(forms_with("/Users/demo/.zshrc", false, &vars), ["~/.zshrc", "/Users/demo/.zshrc"]);
        assert_eq!(forms_with("/opt/x", false, &vars), ["/opt/x"]);
    }

    #[cfg(unix)]
    #[test]
    fn expand_understands_all_forms() {
        assert_eq!(expand_with("~/a", &vars).unwrap(), PathBuf::from("/Users/demo/a"));
        assert_eq!(expand_with("$HOME/a", &vars).unwrap(), PathBuf::from("/Users/demo/a"));
        assert_eq!(expand_with("${XDG}/b", &vars).unwrap(), PathBuf::from("/x/b"));
        assert_eq!(expand_with("%XDG%/c", &vars).unwrap(), PathBuf::from("/x/c"));
        assert_eq!(expand_with("/a/100%/b", &vars).unwrap(), PathBuf::from("/a/100%/b"));
        assert!(expand_with("$NOPE/a", &vars).unwrap_err().to_string().starts_with("path_unknown_var: NOPE"));
        assert!(expand_with("relative/x", &vars).is_err());
    }

    #[test]
    fn backup_and_restore_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("cfg");
        fs::create_dir_all(src.join("sub")).unwrap();
        fs::write(src.join("a.json"), "1").unwrap();
        fs::write(src.join("sub/b.txt"), "22").unwrap();
        let repo_root = tmp.path().join("repo");
        let dir = repo_root.join(".shibapack/paths/x");
        let item = SavedPath { id: "x".into(), path: src.to_string_lossy().into(), note: String::new(), is_dir: true, added_at: String::new() };

        let first = backup(&item, &dir, &repo_root, false).unwrap();
        assert_eq!((first.files, first.size), (2, 3));
        fs::write(src.join("a.json"), "changed").unwrap();
        fs::remove_file(src.join("sub/b.txt")).unwrap();

        // 回收站在测试环境里不一定能用：先自己挪走，模拟“原来的已经不在了”
        fs::rename(&src, tmp.path().join("moved")).unwrap();
        assert!(restore(&item, &dir, &first.name, &repo_root).unwrap().is_none());
        assert_eq!(fs::read_to_string(src.join("a.json")).unwrap(), "1");
        assert_eq!(fs::read_to_string(src.join("sub/b.txt")).unwrap(), "22");
        assert_eq!(list_backups(&dir).len(), 1);
    }

    #[test]
    fn refuses_to_back_up_the_library() {
        let tmp = tempfile::tempdir().unwrap();
        let repo_root = tmp.path().join("repo");
        fs::create_dir_all(&repo_root).unwrap();
        let item = SavedPath { id: "x".into(), path: tmp.path().to_string_lossy().into(), note: String::new(), is_dir: true, added_at: String::new() };
        let err = backup(&item, &repo_root.join(".shibapack/paths/x"), &repo_root, false).unwrap_err();
        assert_eq!(err.to_string(), "path_in_library");
    }
}
