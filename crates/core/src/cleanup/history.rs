//! 清理记录：每次清理写一份清单（删了哪些路径、多大、是直接删还是进了回收站），放在本机应用数据目录的
//! `cleanup-history/` 里，保留最近 20 次。这是这台电脑自己的事，不放进仓库。

use super::rule::Text;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 留几次。
pub const KEEP: usize = 20;
/// 每一项最多记多少条路径。
const MAX_PATHS: usize = 500;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    /// 文件名（不含 .json），也是这一次的 id。
    #[serde(default)]
    pub id: String,
    pub at: String,
    pub freed: u64,
    pub trashed: u64,
    pub items: Vec<RunItem>,
    /// 跳过了几项（被占用、程序在运行、没权限……）。
    pub skipped: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunItem {
    pub id: String,
    pub name: Text,
    pub freed: u64,
    pub trashed: u64,
    pub removed: usize,
    /// 删掉或移走的路径，最多记 500 条。
    #[serde(default)]
    pub paths: Vec<String>,
    /// 没记下来的还有几条。
    #[serde(default)]
    pub more: usize,
}

impl RunItem {
    pub fn push_path(&mut self, path: &Path) {
        if self.paths.len() < MAX_PATHS {
            self.paths.push(path.to_string_lossy().to_string());
        } else {
            self.more += 1;
        }
    }
}

/// 写下这一次，删掉太旧的。返回这一次的 id。
pub fn record(dir: &Path, run: &Run) -> std::io::Result<String> {
    std::fs::create_dir_all(dir)?;
    let mut id = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    // 一秒里清了两次
    let mut n = 1;
    while dir.join(format!("{id}.json")).exists() {
        n += 1;
        id = format!("{}-{n}", &id[..15]);
    }
    let mut run = run.clone();
    run.id = id.clone();
    std::fs::write(dir.join(format!("{id}.json")), serde_json::to_vec_pretty(&run).map_err(std::io::Error::other)?)?;
    for old in files(dir).into_iter().skip(KEEP) {
        let _ = std::fs::remove_file(old);
    }
    Ok(id)
}

/// 最近的在前。坏掉的文件跳过。`with_paths` 为假时不带路径（列表里用不到，省得传一大堆）。
pub fn list(dir: &Path, with_paths: bool) -> Vec<Run> {
    files(dir)
        .into_iter()
        .take(KEEP)
        .filter_map(|path| {
            let mut run: Run = serde_json::from_slice(&std::fs::read(&path).ok()?).ok()?;
            run.id = path.file_stem()?.to_string_lossy().to_string();
            if !with_paths {
                for item in &mut run.items {
                    item.more += item.paths.len();
                    item.paths.clear();
                }
            }
            Some(run)
        })
        .collect()
}

/// 记录文件，最近的在前（文件名就是时间，倒序排）。
fn files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .map(|read| read.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")).collect())
        .unwrap_or_default();
    paths.sort();
    paths.reverse();
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_latest_runs() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..(KEEP + 3) {
            std::fs::write(dir.path().join(format!("20260101-0000{i:02}.json")), b"{\"at\":\"x\",\"freed\":1,\"trashed\":0,\"items\":[],\"skipped\":0}").unwrap();
        }
        let mut item = RunItem { id: "a".into(), ..Default::default() };
        item.push_path(Path::new("/tmp/x"));
        let id = record(dir.path(), &Run { at: "now".into(), freed: 10, items: vec![item], ..Default::default() }).unwrap();
        let runs = list(dir.path(), false);
        assert_eq!(runs.len(), KEEP);
        assert_eq!(runs[0].id, id);
        assert_eq!(runs[0].freed, 10);
        assert!(runs[0].items[0].paths.is_empty());
        assert_eq!(runs[0].items[0].more, 1);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), KEEP);
        assert_eq!(list(dir.path(), true)[0].items[0].paths, vec!["/tmp/x".to_string()]);
    }
}
