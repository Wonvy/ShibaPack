//! 把正在进行的监控存到磁盘：有的安装程序要重启电脑才能装完，重启后打开 ShibaPack 还能接着对比。
//!
//! 存在应用自己的数据文件夹（只和这台电脑有关，不进仓库）：
//! - `pending.json`：装的是什么、范围、开始时间……
//! - `baseline.json`：基线快照。完成或放弃时两个一起删掉。

use super::{products::MsiInfo, Depth, FileMeta, Scope, Snapshot};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

const PENDING: &str = "pending.json";
const BASELINE: &str = "baseline.json";

/// 一次没完成的监控（不含基线）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    pub software_id: String,
    pub version_id: String,
    pub software_name: String,
    pub version: String,
    pub file_name: String,
    pub depth: Depth,
    pub scope: Scope,
    /// 运行的安装程序，用来看它还在不在跑。
    pub installer: PathBuf,
    /// 记录存到哪（仓库的 traces 文件夹）。
    pub data_dir: PathBuf,
    pub started_at: String,
    /// 开始的时间，从 1970 年起的毫秒。
    pub started_ms: i64,
    pub baseline_ms: u64,
    #[serde(default)]
    pub msi: Option<MsiInfo>,
    /// 开始时“重启后替换文件”的登记（Windows），用来看安装程序有没有要求重启。
    #[serde(default)]
    pub reboot_marker: Option<u64>,
}

/// 基线在磁盘上的样子：文件是 [路径, 是不是文件夹, 是不是包, 大小, 修改时间]。
#[derive(Serialize, Deserialize)]
struct BaselineFile {
    files: Vec<(PathBuf, bool, bool, u64, i64)>,
    keys: HashMap<String, HashMap<String, u64>>,
    entries: u64,
    elapsed_ms: u64,
}

/// 存下这次监控和它的基线（先写临时文件再换上）。
pub fn save(dir: &Path, pending: &Pending, baseline: &Snapshot) -> Result<()> {
    fs::create_dir_all(dir)?;
    let file = BaselineFile {
        files: baseline.files.iter().map(|(p, m)| (p.clone(), m.dir, m.bundle, m.size, m.mtime)).collect(),
        keys: baseline.keys.clone(),
        entries: baseline.entries,
        elapsed_ms: baseline.elapsed_ms,
    };
    write_atomic(&dir.join(BASELINE), |out| serde_json::to_writer(out, &file).map_err(Error::from))?;
    write_atomic(&dir.join(PENDING), |out| serde_json::to_writer_pretty(out, pending).map_err(Error::from))?;
    Ok(())
}

fn write_atomic(path: &Path, write: impl FnOnce(&mut BufWriter<fs::File>) -> Result<()>) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    let mut out = BufWriter::new(fs::File::create(&tmp)?);
    write(&mut out)?;
    out.flush()?;
    drop(out);
    fs::rename(&tmp, path)?;
    Ok(())
}

/// 上次没完成的监控。没有或读不了时返回 None。
pub fn load(dir: &Path) -> Option<Pending> {
    let bytes = fs::read(dir.join(PENDING)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// 读出存下的基线。
pub fn load_baseline(dir: &Path) -> Result<Snapshot> {
    let reader = BufReader::new(fs::File::open(dir.join(BASELINE)).map_err(|_| Error::Code("trace_baseline_missing"))?);
    let file: BaselineFile = serde_json::from_reader(reader)?;
    Ok(Snapshot {
        files: file.files.into_iter().map(|(p, dir, bundle, size, mtime)| (p, FileMeta { dir, bundle, size, mtime })).collect(),
        keys: file.keys,
        entries: file.entries,
        elapsed_ms: file.elapsed_ms,
    })
}

/// 完成或放弃：两个文件都删掉。
pub fn clear(dir: &Path) {
    let _ = fs::remove_file(dir.join(PENDING));
    let _ = fs::remove_file(dir.join(BASELINE));
}

/// “重启后替换文件”的登记现在的样子（摘要）。没有登记时是 None；不是 Windows 也是 None。
pub fn reboot_marker() -> Option<u64> {
    #[cfg(windows)]
    {
        use std::hash::{Hash, Hasher};
        use winreg::{enums::*, RegKey};
        let key = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(r"SYSTEM\CurrentControlSet\Control\Session Manager", KEY_READ).ok()?;
        let value = key.get_raw_value("PendingFileRenameOperations").ok()?;
        if value.bytes.iter().all(|b| *b == 0) {
            return None;
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        value.bytes.hash(&mut hasher);
        Some(hasher.finish())
    }
    #[cfg(not(windows))]
    None
}

/// 开始以后有了新的“重启后替换文件”登记：安装程序要求重启。
pub fn reboot_required(start: Option<u64>) -> bool {
    reboot_marker().is_some_and(|now| Some(now) != start)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::{diff, snapshot, Root};
    use std::sync::atomic::AtomicBool;

    #[test]
    fn baseline_survives_a_round_trip() {
        let data = tempfile::tempdir().unwrap();
        let watched = tempfile::tempdir().unwrap();
        fs::write(watched.path().join("a"), "1").unwrap();
        let scope = Scope { roots: vec![Root { path: watched.path().to_path_buf(), depth: 2 }], registry: vec![], exclude: vec![] };
        let baseline = snapshot(&scope, &AtomicBool::new(false), &|_| {}).unwrap();
        let pending = Pending {
            software_id: "s".into(),
            version_id: "v".into(),
            software_name: "Foo".into(),
            version: "1".into(),
            file_name: "foo.msi".into(),
            depth: Depth::Standard,
            scope: scope.clone(),
            installer: "/x/foo.msi".into(),
            data_dir: "/repo/.shibapack/traces".into(),
            started_at: "2026-01-01T00:00:00+08:00".into(),
            started_ms: 1,
            baseline_ms: 2,
            msi: Some(MsiInfo { code: "{X}".into(), name: "Foo".into(), version: "1".into() }),
            reboot_marker: None,
        };
        save(data.path(), &pending, &baseline).unwrap();
        assert_eq!(load(data.path()), Some(pending));
        let back = load_baseline(data.path()).unwrap();
        assert_eq!(back.entries, baseline.entries);
        assert!(diff(&baseline, &back).0.is_empty());
        clear(data.path());
        assert!(load(data.path()).is_none());
        assert!(load_baseline(data.path()).is_err());
    }
}
