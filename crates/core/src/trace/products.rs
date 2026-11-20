//! 安装程序自己登记的清单：用来确认快照对比出来的改动，补上快照范围以外写的文件。
//!
//! - macOS 的 .pkg：装完会在 /var/db/receipts 留一份安装收据，`pkgutil --files <包 id>` 列出它装的每个文件。
//!   这次对比里新出现的收据就是这个安装包留下的。
//! - Windows 的 .msi：安装包里写着产品码（Property 表的 ProductCode）。装完后 Windows Installer 在注册表
//!   `…\Installer\UserData\<用户>\Components\<组件>` 下按“压缩过的产品码”记着每个组件装到了哪个文件。
//!
//! 清单里的文件已经在改动里的（或者在新增的文件夹里），标成“已确认”；不在的、而且是这次安装期间写的，补成新的改动。

use super::{categorize, file_change, sort_changes, Change, FileMeta, ItemKind, Op, Snapshot};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// 一个登记过的产品。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    /// `pkg` 或 `msi`。
    pub kind: String,
    /// .pkg 的包 id（`com.foo.app.pkg`）或 MSI 的产品码（`{…}`）。
    pub id: String,
    pub name: String,
    pub version: String,
    /// 清单里一共几个文件。
    pub files: u64,
}

/// 安装前从 .msi 里读出的产品信息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MsiInfo {
    pub code: String,
    pub name: String,
    pub version: String,
}

/// 补上的文件最多这么多项（按文件夹合并后），再多的只计数。
const MAX_EXTRA: usize = 2000;

/// .msi 安装包的产品码、名字、版本。不是 .msi 或读不出时返回 None。
pub fn msi_info(installer: &Path) -> Option<MsiInfo> {
    if !installer.extension().is_some_and(|e| e.eq_ignore_ascii_case("msi")) {
        return None;
    }
    let mut package = msi::open(installer).ok()?;
    let rows = package.select_rows(msi::Select::table("Property").columns(&["Property", "Value"])).ok()?;
    let mut info = MsiInfo { code: String::new(), name: String::new(), version: String::new() };
    for row in rows {
        let (Some(key), Some(value)) = (row[0].as_str(), row[1].as_str()) else { continue };
        match key {
            "ProductCode" => info.code = value.trim().to_ascii_uppercase(),
            "ProductName" => info.name = value.trim().to_string(),
            "ProductVersion" => info.version = value.trim().to_string(),
            _ => {}
        }
    }
    (!info.code.is_empty()).then_some(info)
}

/// 装完后：找出安装程序登记的产品，用它们的文件清单确认和补全改动。
/// `started_ms` 是开始监控的时间（从 1970 年起的毫秒），补文件时只要这之后写过的。
pub fn apply(changes: &mut Vec<Change>, before: &Snapshot, msi: Option<&MsiInfo>, started_ms: i64) -> Vec<Product> {
    let mut products = Vec::new();
    for id in new_receipts(changes) {
        if let Some((product, files)) = pkg_receipt(&id) {
            confirm(changes, before, &files, started_ms);
            products.push(product);
        }
    }
    if let Some(info) = msi {
        if let Some(files) = msi_files(&info.code) {
            confirm(changes, before, &files, started_ms);
            products.push(Product { kind: "msi".into(), id: info.code.clone(), name: info.name.clone(), version: info.version.clone(), files: files.len() as u64 });
        }
    }
    sort_changes(changes);
    products
}

/// 这次新出现的 .pkg 安装收据的包 id。
fn new_receipts(changes: &[Change]) -> Vec<String> {
    changes
        .iter()
        .filter(|c| c.op != Op::Removed && c.kind == ItemKind::File)
        .filter_map(|c| {
            let rest = c.path.strip_prefix("/private/var/db/receipts/").or_else(|| c.path.strip_prefix("/var/db/receipts/"))?;
            rest.strip_suffix(".plist").map(str::to_string)
        })
        .collect()
}

/// 读一个 .pkg 安装收据：产品信息和它装的文件（完整路径）。
fn pkg_receipt(id: &str) -> Option<(Product, Vec<PathBuf>)> {
    if !cfg!(target_os = "macos") || id.starts_with('-') {
        return None;
    }
    let run = |args: &[&str]| -> Option<String> {
        let out = std::process::Command::new("/usr/sbin/pkgutil").args(args).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())
    };
    let info = parse_pkg_info(&run(&["--pkg-info", id])?);
    let base = Path::new(&info.volume).join(&info.location);
    let files: Vec<PathBuf> = run(&["--files", id, "--only-files"])?.lines().filter(|l| !l.trim().is_empty()).map(|l| base.join(l)).collect();
    let product = Product { kind: "pkg".into(), id: id.to_string(), name: id.to_string(), version: info.version, files: files.len() as u64 };
    Some((product, files))
}

#[derive(Debug, Default, PartialEq)]
pub struct PkgInfo {
    pub volume: String,
    pub location: String,
    pub version: String,
}

/// `pkgutil --pkg-info` 的输出：`volume: /`、`location: Applications`、`version: 1.2`……
pub fn parse_pkg_info(text: &str) -> PkgInfo {
    let mut info = PkgInfo { volume: "/".into(), ..Default::default() };
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else { continue };
        let value = value.trim().to_string();
        match key.trim() {
            "volume" if !value.is_empty() => info.volume = value,
            "location" => info.location = value,
            "version" => info.version = value,
            _ => {}
        }
    }
    info
}

/// 用清单确认改动：清单里的文件在某项改动里（就是它，或者在新增的文件夹里）的，那项标成已确认、不算噪音。
/// 不在任何一项里、还在磁盘上、而且是开始监控后写的，按所在文件夹合并后补成新的改动（同样已确认）。
pub fn confirm(changes: &mut Vec<Change>, before: &Snapshot, files: &[PathBuf], started_ms: i64) {
    // 按路径排好，二分找最近的上级
    let mut index: Vec<(PathBuf, usize)> = changes
        .iter()
        .enumerate()
        .filter(|(_, c)| matches!(c.kind, ItemKind::File | ItemKind::Folder))
        .map(|(i, c)| (PathBuf::from(&c.path), i))
        .collect();
    index.sort();
    let covering = |file: &Path| -> Option<usize> {
        let mut probe = Some(file);
        while let Some(path) = probe {
            if let Ok(at) = index.binary_search_by(|(p, _)| p.as_path().cmp(path)) {
                return Some(index[at].1);
            }
            probe = path.parent();
        }
        None
    };

    let mut extra: BTreeMap<PathBuf, (u64, u64)> = BTreeMap::new(); // 文件夹 → 大小、个数
    let mut singles: Vec<(PathBuf, u64, bool)> = Vec::new();
    for file in files {
        if let Some(i) = covering(file) {
            changes[i].confirmed = true;
            changes[i].noise = false;
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(file) else { continue };
        let written = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .is_some_and(|d| d.as_millis() as i64 >= started_ms);
        if !written {
            continue;
        }
        let Some(parent) = file.parent() else { continue };
        // 上级文件夹在快照范围里、拍基线时就在：这个文件是新加的；否则说不清，算“里面有改动”
        let known_parent = before.files.contains_key(parent);
        let is_new = known_parent && !before.files.contains_key(file);
        if known_parent {
            singles.push((file.clone(), meta.len(), is_new));
        } else {
            let entry = extra.entry(parent.to_path_buf()).or_default();
            entry.0 += meta.len();
            entry.1 += 1;
        }
    }

    let mut added = 0;
    for (path, size, is_new) in singles {
        if added >= MAX_EXTRA {
            break;
        }
        let meta = FileMeta { dir: false, bundle: false, size, mtime: 0 };
        let mut change = file_change(if is_new { Op::Added } else { Op::Modified }, &path, &meta, Some(size));
        change.confirmed = true;
        change.noise = false;
        changes.push(change);
        added += 1;
    }
    // 同一个上级下的文件夹合并到最上面那一层
    let mut merged: Vec<(PathBuf, u64, u64)> = Vec::new();
    for (dir, (size, count)) in extra {
        if let Some(last) = merged.last_mut() {
            if dir.starts_with(&last.0) {
                last.1 += size;
                last.2 += count;
                continue;
            }
        }
        merged.push((dir, size, count));
    }
    for (dir, size, count) in merged {
        if added >= MAX_EXTRA {
            break;
        }
        let text = dir.to_string_lossy().to_string();
        changes.push(Change {
            op: Op::Modified,
            kind: ItemKind::Folder,
            category: categorize(&text, ItemKind::Folder, false),
            noise: false,
            path: text,
            value: None,
            data: None,
            size: Some(size),
            count: Some(count),
            confirmed: true,
        });
        added += 1;
    }
}

/// 压缩过的 GUID：Windows Installer 在注册表里记产品码、组件码用的写法。
/// 前三段整段倒过来，后两段每个字节的两位对调，去掉括号和横线。
/// `{AC76BA86-7AD7-1033-7B44-AC0F074E4100}` → `68AB67CA7DA73301B744CAF070E41400`
pub fn packed_guid(guid: &str) -> Option<String> {
    let hex: String = guid.chars().filter(|c| c.is_ascii_hexdigit()).collect::<String>().to_ascii_uppercase();
    if hex.len() != 32 {
        return None;
    }
    let rev = |s: &str| s.chars().rev().collect::<String>();
    let mut out = rev(&hex[0..8]) + &rev(&hex[8..12]) + &rev(&hex[12..16]);
    for pair in hex.as_bytes()[16..].chunks(2) {
        out.push(pair[1] as char);
        out.push(pair[0] as char);
    }
    Some(out)
}

/// MSI 产品装的文件：Windows Installer 的组件登记里，值名是这个产品压缩码的那些。
/// 注册表组件（`01:\…` 这样的）和空的跳过。产品没装上（找不到任何组件）时返回 None。
#[cfg(windows)]
fn msi_files(code: &str) -> Option<Vec<PathBuf>> {
    use winreg::{enums::*, RegKey};
    let packed = packed_guid(code)?;
    let user_data = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Installer\UserData", KEY_READ)
        .ok()?;
    let mut files = Vec::new();
    let mut found = false;
    for sid in user_data.enum_keys().flatten() {
        let Ok(components) = user_data.open_subkey_with_flags(format!(r"{sid}\Components"), KEY_READ) else { continue };
        for component in components.enum_keys().flatten() {
            let Ok(key) = components.open_subkey_with_flags(&component, KEY_READ) else { continue };
            let Ok(path) = key.get_value::<String, _>(&packed) else { continue };
            found = true;
            let path = path.trim();
            let bytes = path.as_bytes();
            // 注册表组件写成 `01:\…`、`02:\…`
            if path.is_empty() || (bytes.len() > 2 && bytes[0].is_ascii_digit() && bytes[1].is_ascii_digit() && bytes[2] == b':') {
                continue;
            }
            // 驱动器号后面有时是 `?`（`C?\…`，表示可以挪到别的盘）
            let path = if bytes.len() > 1 && bytes[1] == b'?' { format!("{}:{}", &path[..1], &path[2..]) } else { path.to_string() };
            files.push(PathBuf::from(path));
        }
    }
    found.then_some(files)
}

#[cfg(not(windows))]
fn msi_files(_code: &str) -> Option<Vec<PathBuf>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::{diff, snapshot, Category, Root, Scope};
    use std::{fs, sync::atomic::AtomicBool};

    #[test]
    fn packs_guids() {
        assert_eq!(packed_guid("{AC76BA86-7AD7-1033-7B44-AC0F074E4100}").as_deref(), Some("68AB67CA7DA73301B744CAF070E41400"));
        assert_eq!(packed_guid("nope"), None);
    }

    #[test]
    fn parses_pkg_info() {
        let text = "package-id: com.foo.pkg\nversion: 1.2.3\nvolume: /\nlocation: Applications\ninstall-time: 1700000000\n";
        assert_eq!(parse_pkg_info(text), PkgInfo { volume: "/".into(), location: "Applications".into(), version: "1.2.3".into() });
    }

    #[test]
    fn finds_new_receipts() {
        let c = |path: &str| Change {
            op: Op::Added,
            kind: ItemKind::File,
            path: path.into(),
            value: None,
            data: None,
            size: None,
            count: None,
            category: Category::Uninstall,
            noise: false,
            confirmed: false,
        };
        let changes = vec![c("/private/var/db/receipts/com.foo.pkg.plist"), c("/private/var/db/receipts/com.foo.pkg.bom"), c("/Applications/x")];
        assert_eq!(new_receipts(&changes), vec!["com.foo.pkg".to_string()]);
    }

    #[test]
    fn confirms_and_fills_in_files_outside_the_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("deep/a/b")).unwrap();
        let scope = Scope { roots: vec![Root { path: root.to_path_buf(), depth: 1 }], registry: vec![], exclude: vec![] };
        let cancel = AtomicBool::new(false);
        let before = snapshot(&scope, &cancel, &|_| {}).unwrap();
        let started = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64 - 1000;

        fs::create_dir_all(root.join("NewApp")).unwrap();
        fs::write(root.join("NewApp/x"), "1").unwrap();
        fs::write(root.join("top.txt"), "12").unwrap();
        // 快照只看一层，深处写的文件对比不出来
        fs::write(root.join("deep/a/b/lib.dylib"), "123").unwrap();
        let after = snapshot(&scope, &cancel, &|_| {}).unwrap();
        let (mut changes, _) = diff(&before, &after);
        assert!(!changes.iter().any(|c| c.path.contains("lib.dylib")));

        let files = vec![root.join("NewApp/x"), root.join("top.txt"), root.join("deep/a/b/lib.dylib"), root.join("missing")];
        confirm(&mut changes, &before, &files, started);
        let app = changes.iter().find(|c| c.path.ends_with("NewApp")).unwrap();
        assert!(app.confirmed);
        assert!(changes.iter().find(|c| c.path.ends_with("top.txt")).unwrap().confirmed);
        let deep = changes.iter().find(|c| c.path.ends_with("b")).expect("deep folder");
        assert_eq!((deep.op, deep.count, deep.size), (Op::Modified, Some(1), Some(3)));
        assert!(!changes.iter().any(|c| c.path.ends_with("missing")));
    }
}
