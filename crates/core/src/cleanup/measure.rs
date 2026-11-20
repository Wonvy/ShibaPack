//! 量大小和删除。都不跟随符号链接、不跨到别的盘。

use std::collections::HashSet;
use std::path::Path;
use std::time::SystemTime;
use walkdir::WalkDir;

/// 一项（根目录下第一层的文件或文件夹）量出来的样子。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Measured {
    /// 占用的磁盘空间。macOS 和 Linux 按实际占用的块算（和系统显示的可用空间对得上），同一个文件的多个硬链接只算一次。
    pub size: u64,
    pub files: u64,
    /// 里面最近一次改动（修改时间和创建时间里取晚的，宁可算新一点）。
    pub newest: Option<SystemTime>,
    /// 里面有套接字或管道：多半是正在运行的程序在用（ssh-agent、tmux），不能删。
    pub in_use: bool,
    /// 有读不到的地方（没权限）。
    pub unreadable: bool,
}

/// 量一项。`seen` 记着已经算过的硬链接，一个根目录下共用一份。
pub fn measure(path: &Path, seen: &mut HashSet<(u64, u64)>) -> Measured {
    let mut out = Measured::default();
    for entry in WalkDir::new(path).follow_links(false).follow_root_links(false).same_file_system(true) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                out.unreadable = true;
                continue;
            }
        };
        let Ok(meta) = entry.metadata() else {
            out.unreadable = true;
            continue;
        };
        let changed = changed_at(&meta);
        if changed > out.newest {
            out.newest = changed;
        }
        let kind = entry.file_type();
        if special_file(&kind) {
            out.in_use = true;
        }
        if kind.is_file() {
            out.files += 1;
            out.size += disk_size(&meta, seen);
        }
    }
    out
}

/// 修改时间和创建时间（Unix 上是状态改变时间）里晚的那个。解压出来的文件修改时间可能很早，但刚刚才放进来。
fn changed_at(meta: &std::fs::Metadata) -> Option<SystemTime> {
    let modified = meta.modified().ok();
    #[cfg(unix)]
    let other = {
        use std::os::unix::fs::MetadataExt;
        u64::try_from(meta.ctime()).ok().map(|secs| SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs))
    };
    #[cfg(not(unix))]
    let other = meta.created().ok();
    modified.max(other)
}

#[cfg(unix)]
fn special_file(kind: &std::fs::FileType) -> bool {
    use std::os::unix::fs::FileTypeExt;
    kind.is_socket() || kind.is_fifo()
}

#[cfg(not(unix))]
fn special_file(_kind: &std::fs::FileType) -> bool {
    false
}

#[cfg(unix)]
fn disk_size(meta: &std::fs::Metadata, seen: &mut HashSet<(u64, u64)>) -> u64 {
    use std::os::unix::fs::MetadataExt;
    if meta.nlink() > 1 && !seen.insert((meta.dev(), meta.ino())) {
        return 0;
    }
    meta.blocks() * 512
}

#[cfg(not(unix))]
fn disk_size(meta: &std::fs::Metadata, _seen: &mut HashSet<(u64, u64)>) -> u64 {
    meta.len()
}

/// 直接删除的结果。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Removed {
    /// 删掉的文件实际腾出的空间（还有别的硬链接的文件不算，删了也不腾空间）。
    pub freed: u64,
    pub files: u64,
    /// 删不掉的文件（被占用、没权限）。
    pub failed: u64,
    /// 删不掉的原因里有“没权限”。
    pub denied: bool,
}

/// 直接删除一项：从最里面开始一个一个删，删不掉的跳过，别的照删。最后这一项本身删得掉就删。
/// 符号链接只删链接本身，不碰它指向的东西。
pub fn remove_tree(path: &Path) -> Removed {
    let mut out = Removed::default();
    for entry in WalkDir::new(path).follow_links(false).follow_root_links(false).same_file_system(true).contents_first(true) {
        let Ok(entry) = entry else {
            out.failed += 1;
            continue;
        };
        let kind = entry.file_type();
        let target = entry.path();
        if kind.is_dir() {
            // 里面有删不掉的文件时它也删不掉，不另算失败
            let _ = std::fs::remove_dir(target);
            continue;
        }
        let meta = std::fs::symlink_metadata(target).ok();
        let worth = meta.as_ref().filter(|m| m.is_file()).map(freed_by).unwrap_or(0);
        match remove_file(target, kind.is_symlink()) {
            Ok(()) => {
                out.freed += worth;
                if kind.is_file() {
                    out.files += 1;
                }
            }
            Err(err) => {
                out.failed += 1;
                out.denied |= err.kind() == std::io::ErrorKind::PermissionDenied;
            }
        }
    }
    out
}

/// 删掉这个文件能腾出多少空间。
#[cfg(unix)]
fn freed_by(meta: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    if meta.nlink() > 1 { 0 } else { meta.blocks() * 512 }
}

#[cfg(not(unix))]
fn freed_by(meta: &std::fs::Metadata) -> u64 {
    meta.len()
}

fn remove_file(path: &Path, symlink: bool) -> std::io::Result<()> {
    let result = std::fs::remove_file(path);
    match result {
        // Windows 上指向文件夹的链接、联接点要用 remove_dir 删
        Err(_) if symlink => std::fs::remove_dir(path),
        // Windows 上只读的文件删不掉：去掉只读再删
        #[cfg(windows)]
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
            let Ok(meta) = std::fs::metadata(path) else { return Err(err) };
            let mut perms = meta.permissions();
            if !perms.readonly() {
                return Err(err);
            }
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            std::fs::set_permissions(path, perms)?;
            std::fs::remove_file(path)
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn measures_files_and_newest_change() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("a/b")).unwrap();
        fs::write(dir.path().join("a/x.bin"), vec![1u8; 5000]).unwrap();
        fs::write(dir.path().join("a/b/y.bin"), vec![1u8; 9000]).unwrap();
        let measured = measure(&dir.path().join("a"), &mut HashSet::new());
        assert_eq!(measured.files, 2);
        assert!(measured.size >= 14_000, "{}", measured.size);
        assert!(measured.newest.is_some());
        assert!(!measured.in_use && !measured.unreadable);
    }

    #[cfg(unix)]
    #[test]
    fn hard_links_count_once_and_free_nothing() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("a")).unwrap();
        fs::write(dir.path().join("a/x.bin"), vec![1u8; 8192]).unwrap();
        fs::hard_link(dir.path().join("a/x.bin"), dir.path().join("a/y.bin")).unwrap();
        let measured = measure(&dir.path().join("a"), &mut HashSet::new());
        assert_eq!(measured.files, 2);
        assert!(measured.size >= 8192 && measured.size < 16384, "{}", measured.size);
        // 外面还有一个链接：删了也不腾空间
        fs::hard_link(dir.path().join("a/x.bin"), dir.path().join("keep.bin")).unwrap();
        let removed = remove_tree(&dir.path().join("a"));
        assert_eq!(removed.freed, 0);
        assert!(dir.path().join("keep.bin").exists());
    }

    #[cfg(unix)]
    #[test]
    fn sockets_mark_the_entry_in_use() {
        let dir = tempfile::tempdir().unwrap();
        let _listener = std::os::unix::net::UnixListener::bind(dir.path().join("agent.sock")).unwrap();
        assert!(measure(dir.path(), &mut HashSet::new()).in_use);
    }

    #[cfg(unix)]
    #[test]
    fn removal_does_not_follow_links() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("precious.txt"), b"keep").unwrap();
        fs::create_dir_all(dir.path().join("cache")).unwrap();
        fs::write(dir.path().join("cache/junk.bin"), vec![0u8; 4096]).unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("cache/link")).unwrap();
        let removed = remove_tree(&dir.path().join("cache"));
        assert_eq!(removed.failed, 0);
        assert_eq!(removed.files, 1);
        assert!(removed.freed >= 4096);
        assert!(!dir.path().join("cache").exists());
        assert!(outside.path().join("precious.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_entry_is_removed_as_a_link() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("precious.txt"), vec![1u8; 4096]).unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("entry")).unwrap();
        assert_eq!(measure(&dir.path().join("entry"), &mut HashSet::new()).size, 0, "不量链接指向的东西");
        let removed = remove_tree(&dir.path().join("entry"));
        assert_eq!(removed.failed, 0);
        assert!(std::fs::symlink_metadata(dir.path().join("entry")).is_err());
        assert!(outside.path().join("precious.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn locked_files_are_skipped_and_the_rest_removed() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("cache");
        fs::create_dir_all(root.join("locked")).unwrap();
        fs::write(root.join("locked/a.bin"), b"a").unwrap();
        fs::write(root.join("free.bin"), b"b").unwrap();
        // 文件夹不可写：里面的文件删不掉
        fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o555)).unwrap();
        let removed = remove_tree(&root);
        fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(removed.files, 1);
        assert_eq!(removed.failed, 1);
        assert!(removed.denied);
        assert!(root.join("locked/a.bin").exists());
        assert!(!root.join("free.bin").exists());
    }
}
