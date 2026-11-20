//! Windows 安装包自带的产品名、版本号、发布者：exe 的版本信息资源（文件属性里“详细信息”那一页），
//! msi 的 Property 表（ProductName、ProductVersion、Manufacturer）。
//!
//! 文件名里看不出版本号时用它补上：比如 `WinThumbsPreloader.exe` 文件名里没有版本，属性里的“产品版本”是 1.1.0。
//! 读过的按 SHA-256 记在仓库的 `.shibapack/cache/fileinfo.json`（读不出来的也记），同一个文件不再读第二遍。
//! 解析 exe 和 msi 都不依赖 Windows，在任何系统上都能读。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    /// 产品名（没有时用文件说明）。
    #[serde(default)]
    pub product_name: String,
    /// 整理过的版本号：产品版本优先，没有时用文件版本。
    #[serde(default)]
    pub version: String,
    /// 公司名 / 发布者。
    #[serde(default)]
    pub company: String,
}

impl FileInfo {
    fn is_empty(&self) -> bool {
        self.product_name.is_empty() && self.version.is_empty() && self.company.is_empty()
    }
}

/// 读一个文件自带的信息。不是 exe / msi、或者读不出来时返回 `None`。
pub fn read(path: &Path) -> Option<FileInfo> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    let info = match ext.as_str() {
        "exe" => read_exe(path),
        "msi" => read_msi(path),
        _ => None,
    }?;
    (!info.is_empty()).then_some(info)
}

/// 这个文件名是不是可能带着自己的信息（exe、msi）。
pub fn has_info(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".exe") || lower.ends_with(".msi")
}

fn read_exe(path: &Path) -> Option<FileInfo> {
    let file = std::fs::File::open(path).ok()?;
    // 安全性：映射期间文件被别的程序改写会读到变化的内容，这里只读不写，最坏结果是读不出信息
    let map = unsafe { memmap2::Mmap::map(&file).ok()? };
    let pe = pelite::PeFile::from_bytes(&map[..]).ok()?;
    let info = pe.resources().ok()?.version_info().ok()?;
    let langs = info.translation();
    // 有的 exe 带好几种语言的字符串：挑第一个有产品名或版本的
    let pick = |key: &str| {
        langs.iter().find_map(|lang| info.value(*lang, key).map(|v| v.trim().trim_end_matches('\0').trim().to_string()).filter(|v| !v.is_empty()))
    };
    let version = pick("ProductVersion").map(|v| clean_version(&v)).filter(|v| !v.is_empty()).or_else(|| pick("FileVersion").map(|v| clean_version(&v)));
    let fixed = info.fixed().map(|f| {
        let v = f.dwProductVersion;
        clean_version(&format!("{}.{}.{}.{}", v.Major, v.Minor, v.Patch, v.Build))
    });
    Some(FileInfo {
        product_name: pick("ProductName").or_else(|| pick("FileDescription")).unwrap_or_default(),
        version: version.filter(|v| !v.is_empty()).or(fixed).unwrap_or_default(),
        company: pick("CompanyName").unwrap_or_default(),
    })
}

fn read_msi(path: &Path) -> Option<FileInfo> {
    let mut package = msi::open(path).ok()?;
    let rows = package.select_rows(msi::Select::table("Property").columns(&["Property", "Value"])).ok()?;
    let mut info = FileInfo::default();
    for row in rows {
        let (Some(key), Some(value)) = (row[0].as_str(), row[1].as_str()) else { continue };
        let value = value.trim().to_string();
        match key {
            "ProductName" => info.product_name = value,
            "ProductVersion" => info.version = clean_version(&value),
            "Manufacturer" => info.company = value,
            _ => {}
        }
    }
    Some(info)
}

/// 把属性里的版本号整理成平常的写法：`1, 2, 0, 0` → `1.2`，`1.1.0.0` → `1.1.0`，去掉后面的说明文字。
/// 全是 0 或者没有数字时返回空字符串。
pub fn clean_version(raw: &str) -> String {
    let normalized = raw.trim().replace(", ", ".").replace(',', ".");
    let head: String = normalized.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    let mut parts: Vec<&str> = head.split('.').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() || parts.iter().all(|p| p.chars().all(|c| c == '0')) {
        return String::new();
    }
    // 第四段（构建号）是 0 时不要；第三段也是 0、前面只有两段时保留成 x.y.0 的习惯写法
    while parts.len() > 3 && parts.last().is_some_and(|p| p.chars().all(|c| c == '0')) {
        parts.pop();
    }
    parts.join(".")
}

/// 读过的文件信息：SHA-256 → 信息（`None` 是读过但没有）。可以随时删掉，删了只是要重新读。
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfoCache {
    #[serde(default)]
    pub files: BTreeMap<String, Option<FileInfo>>,
}

const CACHE_FILE: &str = "fileinfo.json";

impl InfoCache {
    pub fn load(dir: &Path) -> InfoCache {
        std::fs::read(dir.join(CACHE_FILE)).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join(format!("{CACHE_FILE}.tmp"));
        std::fs::write(&tmp, serde_json::to_vec(self)?)?;
        std::fs::rename(tmp, dir.join(CACHE_FILE))
    }

    /// 查缓存，没有就读文件并记下。第二个值表示这次是不是真的读了文件。
    pub fn get_or_read(&mut self, sha256: &str, path: &Path) -> (Option<FileInfo>, bool) {
        if let Some(known) = self.files.get(sha256) {
            return (known.clone(), false);
        }
        if !path.is_file() {
            return (None, false);
        }
        let info = read(path);
        self.files.insert(sha256.to_string(), info.clone());
        (info, true)
    }
}

/// 测试用：造一个真的 MSI，只有 Property 表。
#[cfg(test)]
pub(crate) fn write_test_msi(path: &Path, name: &str, version: &str, maker: &str) {
    let file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(path).unwrap();
    let mut package = msi::Package::create(msi::PackageType::Installer, file).unwrap();
    package
        .create_table("Property", vec![msi::Column::build("Property").primary_key().string(72), msi::Column::build("Value").string(0)])
        .unwrap();
    package
        .insert_rows(msi::Insert::into("Property").rows(vec![
            vec![msi::Value::from("ProductName"), msi::Value::from(name)],
            vec![msi::Value::from("ProductVersion"), msi::Value::from(version)],
            vec![msi::Value::from("Manufacturer"), msi::Value::from(maker)],
        ]))
        .unwrap();
    package.flush().unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_versions() {
        assert_eq!(clean_version("1.1.0"), "1.1.0");
        assert_eq!(clean_version("1.1.0.0"), "1.1.0");
        assert_eq!(clean_version("2, 4, 1, 0"), "2.4.1");
        assert_eq!(clean_version("24.09"), "24.09");
        assert_eq!(clean_version("3.0.21.0 (x64)"), "3.0.21");
        assert_eq!(clean_version("10.0.19041.1"), "10.0.19041.1");
        assert_eq!(clean_version("0.0.0.0"), "");
        assert_eq!(clean_version("beta"), "");
    }

    #[test]
    fn reads_real_exe() {
        // 本机 Python 自带的 pip 启动器是一个真实的 Windows exe（和 icons 的测试用的同一个）
        let path = Path::new("/Library/Developer/CommandLineTools/Library/Frameworks/Python3.framework/Versions/3.9/lib/python3.9/site-packages/pip/_vendor/distlib/t64.exe");
        if !path.exists() {
            return;
        }
        // 这个启动器没有版本信息：读不出来时是 None，不能 panic
        let _ = read(path);
        assert!(read(Path::new("/nonexistent.exe")).is_none());
        assert!(read(Path::new("/etc/hosts")).is_none());
    }

    #[test]
    fn reads_msi_properties() {
        // 用同一个库造一个真的 MSI：只有 Property 表
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Tool-setup.msi");
        write_test_msi(&path, "WinThumbsPreloader", "1.1.0.0", "Dmitry Bruhov");
        let info = read(&path).unwrap();
        assert_eq!(info, FileInfo { product_name: "WinThumbsPreloader".into(), version: "1.1.0".into(), company: "Dmitry Bruhov".into() });
    }

    #[test]
    fn cache_reads_once() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("not-really.exe");
        std::fs::write(&file, b"MZ but not a real exe").unwrap();
        let mut cache = InfoCache::default();
        assert_eq!(cache.get_or_read("abc", &file), (None, true));
        assert_eq!(cache.get_or_read("abc", &file), (None, false), "第二次直接用记下的");
        cache.files.insert("def".into(), Some(FileInfo { product_name: "X".into(), version: "1.0".into(), company: "Y".into() }));
        cache.save(dir.path()).unwrap();
        let back = InfoCache::load(dir.path());
        assert_eq!(back.files.len(), 2);
        assert_eq!(back.files["def"].as_ref().unwrap().version, "1.0");
    }
}
