//! 删除前的最后一道检查。规则、界面、扫描结果都可能出错，这里只看路径本身。
//!
//! 三种受保护的文件夹：
//! - 封住的：它和里面的一切都不碰。系统文件夹、密钥、同步盘、ShibaPack 的仓库。
//! - 个人文件夹：用户文件夹、文档、桌面、下载……它本身不能删，也不能整个清空，只能按名字筛着清（下载文件夹里没下完的文件）。
//! - 标准文件夹：Caches、Logs、AppData、Temp……它本身不能删，里面的可以按规则清。
//!
//! 另外，名字是浏览器密码、书签、自动填充、加密密钥这类的，和里面有 `.git` 的文件夹，一律不删；
//! 浏览记录、Cookie 这类只有隐私痕迹的规则能删。
//!
//! 要管理员权限的规则只能清 `rule::ADMIN_ROOTS` 里那几个系统文件夹（`admin_roots`），它们在封住的范围里开了个口子。

use super::vars::Vars;
use crate::installed::{path_key, Protected};
use std::path::{Path, PathBuf};

/// 这些名字的文件和文件夹一律不删（不分大小写）：浏览器的密码、书签、自动填充、加密密钥，Git 仓库，密钥。
const NEVER: &[&str] = &[
    ".git",
    ".ssh",
    ".gnupg",
    "login data",
    "login data-journal",
    "login data for account",
    "login data for account-journal",
    "bookmarks",
    "bookmarks.bak",
    "web data",
    "web data-journal",
    "local state",
    "key4.db",
    "logins.json",
    "places.sqlite",
    "places.sqlite-wal",
    "favicons.sqlite",
    "cert9.db",
    "keychains",
];

/// 这些名字只有隐私痕迹的规则能删：浏览记录和 Cookie。
const PRIVATE: &[&str] = &[
    "history",
    "history-journal",
    "visited links",
    "top sites",
    "top sites-journal",
    "shortcuts",
    "shortcuts-journal",
    "network action predictor",
    "network action predictor-journal",
    "cookies",
    "cookies-journal",
    "cookies.sqlite",
    "cookies.sqlite-wal",
    "cookies.sqlite-shm",
];

/// 一条规则清东西的方式，决定保护检查放宽到哪里。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Scope {
    /// 只按名字筛着清一部分。
    pub narrow: bool,
    /// 隐私痕迹：可以删浏览记录和 Cookie。
    pub private: bool,
    /// 要管理员权限：根目录必须是 `admin_roots` 里的系统文件夹。
    pub admin: bool,
}

impl Scope {
    pub fn broad() -> Scope {
        Scope::default()
    }

    pub fn narrow() -> Scope {
        Scope { narrow: true, ..Scope::default() }
    }
}

#[derive(Debug, Clone)]
pub struct Guard {
    fold_case: bool,
    sealed: Vec<String>,
    personal: Vec<String>,
    keep: Vec<String>,
    /// ~/Library：它本身和下面第一层（Caches、Logs、Application Support……）都不能删。
    library: Option<String>,
    /// 临时文件夹：Linux 上是 /tmp，只有一层也允许当根目录。
    temp: Option<String>,
    /// 封住的文件夹里的例外：这些文件夹里可以按名字筛着清（ShibaPack 自己没下完的文件在仓库的 `.shibapack/downloads` 里）。
    narrow_ok: Vec<String>,
    /// 要管理员权限的规则能清的系统文件夹（展开以后的 `rule::ADMIN_ROOTS`）。
    admin_roots: Vec<String>,
}

impl Guard {
    /// `repo_root` 是 ShibaPack 的仓库，整个封住；`repo_downloads` 是仓库里下载用的临时文件夹，可以按名字筛着清。
    pub fn new(vars: &Vars, repo_root: Option<&Path>, repo_downloads: Option<&Path>) -> Guard {
        let fold_case = cfg!(windows);
        let env = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
        let home = vars.get("home").map(Path::to_path_buf);
        let protected = Protected::from_env();

        let mut sealed: Vec<PathBuf> = Vec::new();
        if cfg!(unix) {
            for path in [
                "/System", "/usr", "/bin", "/sbin", "/etc", "/private/etc", "/dev", "/proc", "/sys", "/boot", "/Applications", "/Library", "/opt",
                "/snap", "/nix", "/private/var/db", "/var/lib", "/var/db",
            ] {
                sealed.push(PathBuf::from(path));
            }
        }
        for name in ["SystemRoot", "windir", "ProgramFiles", "ProgramFiles(x86)", "ProgramW6432", "ProgramData", "OneDrive", "OneDriveConsumer", "OneDriveCommercial"] {
            sealed.extend(env(name));
        }
        if let Some(home) = &home {
            for rel in [
                ".ssh", ".gnupg", ".aws", ".kube", ".docker", ".password-store", "Library/Keychains", "Library/Mobile Documents", "Library/CloudStorage",
                "Library/Mail", "Library/Messages", "OneDrive", "Dropbox", "Google Drive", "iCloudDrive", "Applications",
            ] {
                sealed.push(home.join(rel));
            }
        }
        sealed.extend(repo_root.map(Path::to_path_buf));

        let mut personal: Vec<PathBuf> = Vec::new();
        if let Some(home) = &home {
            personal.push(home.clone());
            for rel in ["Documents", "Desktop", "Downloads", "Pictures", "Music", "Movies", "Videos", "Public", "Templates", "Projects", "Developer"] {
                personal.push(home.join(rel));
            }
        }
        personal.extend(["downloads", "desktop"].iter().filter_map(|name| vars.get(name).map(Path::to_path_buf)));

        let mut keep: Vec<PathBuf> = ["temp", "user_cache", "app_support", "user_logs", "local_app_data", "roaming_app_data", "xdg_cache", "xdg_config", "xdg_data", "xdg_state"]
            .iter()
            .filter_map(|name| vars.get(name).map(Path::to_path_buf))
            .collect();
        if let Some(home) = &home {
            for rel in [".local", ".cache", ".config", "AppData", "AppData/LocalLow"] {
                keep.push(home.join(rel));
            }
        }

        let key = |path: &PathBuf| path_key(path, fold_case);
        let mut guard = Guard {
            fold_case,
            sealed: sealed.iter().map(key).chain(protected.under.iter().cloned()).collect(),
            personal: personal.iter().map(key).collect(),
            keep: keep.iter().map(key).chain(protected.exact.iter().cloned()).collect(),
            library: home.as_ref().filter(|_| cfg!(target_os = "macos")).map(|home| key(&home.join("Library"))),
            temp: vars.get("temp").map(|temp| path_key(temp, fold_case)),
            narrow_ok: repo_downloads.map(|dir| path_key(dir, fold_case)).into_iter().collect(),
            admin_roots: Vec::new(),
        };
        guard.sealed.sort();
        guard.sealed.dedup();
        guard
    }

    /// 要管理员权限的规则能清的系统文件夹。
    pub fn with_admin_roots(mut self, roots: &[PathBuf]) -> Guard {
        self.admin_roots = roots.iter().map(|root| path_key(root, self.fold_case)).collect();
        self
    }

    fn key(&self, path: &Path) -> String {
        path_key(path, self.fold_case)
    }

    fn sealed(&self, key: &str) -> bool {
        self.sealed.iter().any(|p| under_or_same(key, p))
    }

    /// ~/Library 本身，或者它下面第一层的标准文件夹。
    fn library_folder(&self, key: &str) -> bool {
        let Some(library) = &self.library else { return false };
        key == library || key.strip_prefix(library.as_str()).and_then(|rest| rest.strip_prefix('/')).is_some_and(|rest| !rest.contains('/'))
    }

    /// 本身不能删的文件夹。
    fn standing(&self, key: &str) -> bool {
        self.personal.iter().any(|p| p == key) || self.keep.iter().any(|p| p == key) || self.library_folder(key)
    }

    /// 能不能把这个文件夹当根目录，清它里面的东西。
    pub fn root_ok(&self, root: &Path, scope: Scope) -> bool {
        if !root.is_absolute() || !super::vars::real_dir(root) {
            return false;
        }
        let key = self.key(root);
        // 要管理员权限的：只认列出来的系统文件夹，别的一律不行
        if scope.admin {
            return self.admin_roots.contains(&key) && !self.personal.contains(&key);
        }
        let depth = depth(&key);
        // 整个盘不行；只有一层的（/tmp）只认临时文件夹
        if depth == 0 || (depth == 1 && (self.fold_case || self.temp.as_deref() != Some(key.as_str()))) {
            return false;
        }
        if self.sealed(&key) && !(scope.narrow && self.narrow_ok.contains(&key)) {
            return false;
        }
        scope.narrow || !self.personal.contains(&key)
    }

    /// 删除一项（根目录下第一层的文件或文件夹）之前的检查。不行时返回原因：`missing`（已经没了）或 `not_allowed`。
    pub fn entry_check(&self, root: &Path, entry: &Path, scope: Scope) -> Result<(), &'static str> {
        let Ok(meta) = std::fs::symlink_metadata(entry) else { return Err("missing") };
        let parent = entry.parent().map(|parent| self.key(parent));
        if parent.as_deref() != Some(self.key(root).as_str()) || !self.root_ok(root, scope) {
            return Err("not_allowed");
        }
        let key = self.key(entry);
        // 管理员的系统文件夹、仓库里下载用的临时文件夹：根目录已经核对过是开了口子的那几个
        let exempt = scope.admin || (scope.narrow && self.narrow_ok.contains(&self.key(root)));
        if (!exempt && self.sealed(&key)) || self.standing(&key) || !name_ok(entry, scope) {
            return Err("not_allowed");
        }
        if meta.is_dir() && std::fs::symlink_metadata(entry.join(".git")).is_ok() {
            return Err("not_allowed");
        }
        // 不跨到别的盘：挂载在这里的网络盘、外置盘不算
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if std::fs::metadata(root).map(|root| root.dev() != meta.dev()).unwrap_or(true) {
                return Err("not_allowed");
            }
        }
        Ok(())
    }

    /// 用户自己加的清理文件夹能不能用。不行时返回原因：`missing`、`too_shallow`、`protected`。
    pub fn custom_root(&self, root: &Path) -> Result<(), &'static str> {
        if !root.is_absolute() || !super::vars::real_dir(root) {
            return Err("missing");
        }
        let key = self.key(root);
        if depth(&key) < 2 {
            return Err("too_shallow");
        }
        // 它本身受保护，或者里面装着受保护的文件夹（比如选了仓库所在的盘）
        let contains = |list: &[String]| list.iter().any(|p| under_or_same(p, &key));
        if self.sealed(&key) || self.standing(&key) || contains(&self.sealed) || contains(&self.personal) || !name_ok(root, Scope::broad()) {
            return Err("protected");
        }
        Ok(())
    }

    /// 在排除列表里（排除的文件夹本身或者在它里面）。
    pub fn excluded(&self, path: &Path, exclude: &[PathBuf]) -> bool {
        let key = self.key(path);
        exclude.iter().any(|ex| under_or_same(&key, &self.key(ex)))
    }
}

/// 名字能不能删：密码、书签这类一律不行；浏览记录、Cookie 只有隐私痕迹能删。
fn name_ok(path: &Path, scope: Scope) -> bool {
    let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    !NEVER.contains(&name.as_str()) && (scope.private || !PRIVATE.contains(&name.as_str()))
}

fn under_or_same(key: &str, dir: &str) -> bool {
    key == dir || (key.starts_with(dir) && key[dir.len()..].starts_with('/')) || (dir == "/" && key.starts_with('/'))
}

fn depth(key: &str) -> usize {
    key.split('/').filter(|part| !part.is_empty()).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Fixture {
        _dir: tempfile::TempDir,
        home: PathBuf,
        guard: Guard,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        for rel in ["Library/Caches/pip/http", "Library/Logs/App", "Downloads", "Desktop", "Documents/notes", ".ssh", "repo/.shibapack/downloads", "proj/.git"] {
            fs::create_dir_all(home.join(rel)).unwrap();
        }
        let vars = Vars::with(&[
            ("home", home.as_path()),
            ("user_cache", home.join("Library/Caches").as_path()),
            ("user_logs", home.join("Library/Logs").as_path()),
            ("downloads", home.join("Downloads").as_path()),
            ("desktop", home.join("Desktop").as_path()),
        ]);
        let guard = Guard::new(&vars, Some(&home.join("repo")), Some(&home.join("repo/.shibapack/downloads")));
        Fixture { _dir: dir, home, guard }
    }

    #[test]
    fn cache_contents_are_allowed_but_standard_folders_are_not() {
        let f = fixture();
        let pip = f.home.join("Library/Caches/pip");
        assert!(f.guard.root_ok(&pip, Scope::broad()));
        assert_eq!(f.guard.entry_check(&pip, &pip.join("http"), Scope::broad()), Ok(()));
        // 不在根目录正下方的不行
        assert_eq!(f.guard.entry_check(&f.home.join("Library"), &pip, Scope::broad()), Err("not_allowed"));
        // Caches 本身不能当一项删
        let library = f.home.join("Library");
        fs::create_dir_all(&library).unwrap();
        assert_eq!(f.guard.entry_check(&library, &library.join("Caches"), Scope::broad()), Err("not_allowed"));
        assert_eq!(f.guard.entry_check(&pip, &pip.join("gone"), Scope::broad()), Err("missing"));
    }

    #[test]
    fn personal_folders_only_allow_narrow_rules() {
        let f = fixture();
        let downloads = f.home.join("Downloads");
        fs::write(downloads.join("a.crdownload"), b"x").unwrap();
        assert!(!f.guard.root_ok(&downloads, Scope::broad()));
        assert!(f.guard.root_ok(&downloads, Scope::narrow()));
        assert_eq!(f.guard.entry_check(&downloads, &downloads.join("a.crdownload"), Scope::narrow()), Ok(()));
        assert!(!f.guard.root_ok(&f.home, Scope::narrow()) || f.guard.entry_check(&f.home, &f.home.join("Documents"), Scope::narrow()).is_err());
        assert_eq!(f.guard.entry_check(&f.home, &f.home.join("Documents"), Scope::narrow()), Err("not_allowed"));
    }

    #[test]
    fn sealed_folders_are_never_touched() {
        let f = fixture();
        assert!(!f.guard.root_ok(&f.home.join(".ssh"), Scope::broad()));
        assert!(!f.guard.root_ok(&f.home.join("repo"), Scope::narrow()));
        // 仓库里下载用的临时文件夹：只能按名字筛着清
        let partial = f.home.join("repo/.shibapack/downloads");
        fs::write(partial.join("abc.part"), b"x").unwrap();
        assert!(!f.guard.root_ok(&partial, Scope::broad()));
        assert!(f.guard.root_ok(&partial, Scope::narrow()));
        assert_eq!(f.guard.entry_check(&partial, &partial.join("abc.part"), Scope::narrow()), Ok(()));
    }

    #[test]
    fn git_repositories_and_browser_secrets_are_kept() {
        let f = fixture();
        assert_eq!(f.guard.entry_check(&f.home.join("proj"), &f.home.join("proj/.git"), Scope::broad()), Err("not_allowed"));
        let cache = f.home.join("Library/Caches/pip");
        fs::create_dir_all(cache.join("repo/.git")).unwrap();
        fs::write(cache.join("Cookies"), b"x").unwrap();
        assert_eq!(f.guard.entry_check(&cache, &cache.join("repo"), Scope::broad()), Err("not_allowed"));
        assert_eq!(f.guard.entry_check(&cache, &cache.join("Cookies"), Scope::broad()), Err("not_allowed"));
        // 隐私痕迹可以删 Cookie，但还是不能删密码
        let private = Scope { narrow: true, private: true, admin: false };
        fs::write(cache.join("Login Data"), b"x").unwrap();
        assert_eq!(f.guard.entry_check(&cache, &cache.join("Cookies"), private), Ok(()));
        assert_eq!(f.guard.entry_check(&cache, &cache.join("Login Data"), private), Err("not_allowed"));
    }

    #[test]
    fn admin_rules_only_reach_their_listed_folders() {
        let f = fixture();
        let system = f.home.join("System/Caches");
        fs::create_dir_all(system.join("vendor")).unwrap();
        let admin = Scope { admin: true, ..Scope::default() };
        assert!(!f.guard.root_ok(&system, admin), "没列出来的不行");
        let guard = f.guard.clone().with_admin_roots(std::slice::from_ref(&system));
        assert!(guard.root_ok(&system, admin));
        assert_eq!(guard.entry_check(&system, &system.join("vendor"), admin), Ok(()));
        // 普通规则用不了管理员的文件夹以外的地方，管理员规则也用不了别的地方
        assert!(!guard.root_ok(&f.home.join("Library/Caches/pip"), admin));
    }

    #[test]
    fn custom_folders_must_be_specific() {
        let f = fixture();
        assert_eq!(f.guard.custom_root(&f.home.join("Library/Logs/App")), Ok(()));
        assert_eq!(f.guard.custom_root(&f.home.join("Downloads")), Err("protected"));
        assert_eq!(f.guard.custom_root(&f.home), Err("protected"));
        assert_eq!(f.guard.custom_root(&f.home.join("repo")), Err("protected"));
        assert_eq!(f.guard.custom_root(&f.home.join("missing")), Err("missing"));
        // 选了装着个人文件夹的上一级
        assert_eq!(f.guard.custom_root(f.home.parent().unwrap()), Err("protected"));
        assert_eq!(f.guard.custom_root(Path::new("/")), Err("too_shallow"));
    }

    #[test]
    fn exclusions_cover_their_contents() {
        let f = fixture();
        let ex = vec![f.home.join("Library/Caches/pip")];
        assert!(f.guard.excluded(&f.home.join("Library/Caches/pip/http"), &ex));
        assert!(!f.guard.excluded(&f.home.join("Library/Caches/pipx"), &ex));
    }
}
