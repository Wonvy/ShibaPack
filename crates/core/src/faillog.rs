//! 失败日志：导入、更新、启动项、AI 这些操作失败时记一笔，方便事后查原因。
//!
//! 每行一条 JSON，存在应用自己的日志文件夹里（只和这台电脑有关，不放进可能几台电脑共用的仓库）。
//! 文件超过 1 MB 时把它改名成 `failures.1.log`（原来的 `.1` 删掉），再从头记，最多留两份。

use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

const FILE: &str = "failures.log";
const OLD: &str = "failures.1.log";
const MAX_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FailureEntry {
    /// 什么时候，ISO 8601。
    pub time: String,
    /// 哪一块：import、update、startup、ai、ui……
    pub area: String,
    /// 出了什么错（给人看的一句话，或者错误代码）。
    pub message: String,
    /// 相关的东西：文件路径、启动项、版本号。
    #[serde(default)]
    pub detail: String,
}

/// 记一条。写不进去就算了：记日志失败不能让原来的操作再失败一次。
pub fn append(dir: &Path, area: &str, message: &str, detail: &str) {
    let _ = append_inner(dir, &FailureEntry { time: crate::now(), area: area.to_string(), message: message.trim().to_string(), detail: detail.to_string() });
}

fn append_inner(dir: &Path, entry: &FailureEntry) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let path = dir.join(FILE);
    if fs::metadata(&path).map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
        let _ = fs::remove_file(dir.join(OLD));
        fs::rename(&path, dir.join(OLD))?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(file, "{}", serde_json::to_string(entry)?)
}

/// 最近的 `limit` 条，新的在前。
pub fn read(dir: &Path, limit: usize) -> Vec<FailureEntry> {
    let mut entries = Vec::new();
    for name in [FILE, OLD] {
        let Ok(text) = fs::read_to_string(dir.join(name)) else { continue };
        for line in text.lines().rev() {
            if entries.len() >= limit {
                return entries;
            }
            if let Ok(entry) = serde_json::from_str::<FailureEntry>(line) {
                entries.push(entry);
            }
        }
    }
    entries
}

/// 清空（两份都删）。
pub fn clear(dir: &Path) -> std::io::Result<()> {
    for name in [FILE, OLD] {
        match fs::remove_file(dir.join(name)) {
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => return Err(err),
            _ => {}
        }
    }
    Ok(())
}

/// 日志文件的路径（打开文件夹时用）。
pub fn path(dir: &Path) -> std::path::PathBuf {
    dir.join(FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_reads_newest_first_and_clears() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read(dir.path(), 10).is_empty());
        append(dir.path(), "import", "not_a_file", "/tmp/a.exe");
        append(dir.path(), "update", "Permission denied", "0.4.0");
        let entries = read(dir.path(), 10);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].area, "update");
        assert_eq!(entries[1].detail, "/tmp/a.exe");
        assert_eq!(read(dir.path(), 1).len(), 1);
        // 坏掉的行跳过
        fs::write(dir.path().join(FILE), "{oops\n").unwrap();
        append(dir.path(), "ai", "ai_timeout", "");
        assert_eq!(read(dir.path(), 10).len(), 1);
        clear(dir.path()).unwrap();
        assert!(read(dir.path(), 10).is_empty());
        clear(dir.path()).unwrap();
    }

    #[test]
    fn rotates_when_large() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE), "x".repeat(MAX_BYTES as usize + 10)).unwrap();
        append(dir.path(), "import", "after rotation", "");
        assert!(dir.path().join(OLD).exists());
        assert_eq!(read(dir.path(), 10)[0].message, "after rotation");
    }
}
