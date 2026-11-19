//! 从文件里取图标：macOS 的 .icns、Windows 的 exe / dll / ico、Linux 的 png / svg。
//!
//! 取出来的图标原样保存（png、ico、svg 浏览器内核都能直接显示），不做重新编码。

use std::{
    fs,
    path::{Path, PathBuf},
};

const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

/// 取出 `source` 的图标，写到 `dest_dir/<stem>.<扩展名>`，返回写出的路径。取不到时返回 `None`。
pub fn extract_icon(source: &Path, dest_dir: &Path, stem: &str) -> Option<PathBuf> {
    let ext = source.extension()?.to_string_lossy().to_ascii_lowercase();
    let (bytes, out_ext) = match ext.as_str() {
        "icns" => (icns_png(source)?, "png"),
        "exe" | "dll" => (pe_icon(source)?, "ico"),
        "ico" | "png" | "svg" => (fs::read(source).ok()?, ext.as_str()),
        _ => return None,
    };
    if bytes.is_empty() {
        return None;
    }
    fs::create_dir_all(dest_dir).ok()?;
    let dest = dest_dir.join(format!("{stem}.{out_ext}"));
    fs::write(&dest, bytes).ok()?;
    Some(dest)
}

/// 已经取过的图标（任何一种扩展名）。
pub fn cached_icon(dest_dir: &Path, stem: &str) -> Option<PathBuf> {
    ["png", "ico", "svg"].iter().map(|ext| dest_dir.join(format!("{stem}.{ext}"))).find(|p| p.is_file())
}

/// .icns 是一个容器，里面按尺寸放了多张图，新一些的格式直接就是 PNG。挑一张大小合适的。
fn icns_png(path: &Path) -> Option<Vec<u8>> {
    let data = fs::read(path).ok()?;
    pick_icns_png(&data).or_else(|| convert_with_sips(path))
}

fn pick_icns_png(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 8 || &data[..4] != b"icns" {
        return None;
    }
    // 128、256、512 像素优先；64 太糊、1024 太大，放最后
    const ORDER: [&[u8; 4]; 7] = [b"ic07", b"ic13", b"ic08", b"ic09", b"ic14", b"ic12", b"ic10"];
    let mut found: Vec<(&[u8], &[u8])> = Vec::new();
    let mut pos = 8;
    while pos + 8 <= data.len() {
        let kind = &data[pos..pos + 4];
        let len = u32::from_be_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]]) as usize;
        if len < 8 || pos + len > data.len() {
            break;
        }
        let body = &data[pos + 8..pos + len];
        if body.starts_with(PNG_MAGIC) {
            found.push((kind, body));
        }
        pos += len;
    }
    ORDER.iter().find_map(|want| found.iter().find(|(kind, _)| *kind == &want[..]).map(|(_, body)| body.to_vec()))
}

/// 老格式的 .icns 里没有 PNG。macOS 自带的 sips 能转，别的系统上就放弃。
#[cfg(target_os = "macos")]
fn convert_with_sips(path: &Path) -> Option<Vec<u8>> {
    let out = std::env::temp_dir().join(format!("shibapack-{}.png", crate::new_id()));
    let status = std::process::Command::new("/usr/bin/sips")
        .args(["-s", "format", "png", "-Z", "128"])
        .arg(path)
        .arg("--out")
        .arg(&out)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()?;
    let bytes = if status.success() { fs::read(&out).ok() } else { None };
    let _ = fs::remove_file(&out);
    bytes.filter(|b| b.starts_with(PNG_MAGIC))
}

#[cfg(not(target_os = "macos"))]
fn convert_with_sips(_path: &Path) -> Option<Vec<u8>> {
    None
}

/// 从 exe / dll 的资源里取第一组图标，拼成一个 .ico 文件的内容。
/// 用内存映射读文件，几百 MB 的安装包也只会碰到文件头和资源段。
fn pe_icon(path: &Path) -> Option<Vec<u8>> {
    let file = fs::File::open(path).ok()?;
    // 安全性：映射期间文件被别的程序改写会读到变化的内容，这里只读不写，最坏结果是取图标失败
    let map = unsafe { memmap2::Mmap::map(&file).ok()? };
    let pe = pelite::PeFile::from_bytes(&map[..]).ok()?;
    let resources = pe.resources().ok()?;
    let (_, group) = resources.icons().filter_map(Result::ok).next()?;
    let mut ico = Vec::new();
    group.write(&mut ico).ok()?;
    // 至少要有文件头和一个图标条目
    (ico.len() > 22).then_some(ico)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn icns_with(entries: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
        let mut body = Vec::new();
        for (kind, data) in entries {
            body.extend_from_slice(*kind);
            body.extend_from_slice(&((data.len() + 8) as u32).to_be_bytes());
            body.extend_from_slice(data);
        }
        let mut out = b"icns".to_vec();
        out.extend_from_slice(&((body.len() + 8) as u32).to_be_bytes());
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn picks_the_preferred_png_entry() {
        let png = |tag: u8| [PNG_MAGIC, &[tag]].concat();
        let data = icns_with(&[(b"il32", b"old rle data"), (b"ic10", &png(10)), (b"ic08", &png(8)), (b"ic07", &png(7))]);
        assert_eq!(pick_icns_png(&data).unwrap().last(), Some(&7));

        let only_big = icns_with(&[(b"ic10", &png(10))]);
        assert_eq!(pick_icns_png(&only_big).unwrap().last(), Some(&10));
    }

    #[test]
    fn rejects_non_icns_and_truncated_data() {
        assert!(pick_icns_png(b"not an icns file").is_none());
        assert!(pick_icns_png(&icns_with(&[(b"il32", b"rle only")])).is_none());
        let mut broken = icns_with(&[(b"ic07", PNG_MAGIC)]);
        broken.truncate(broken.len() - 3);
        assert!(pick_icns_png(&broken).is_none());
    }

    #[test]
    fn copies_plain_image_files() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("app.png");
        fs::write(&source, PNG_MAGIC).unwrap();
        let out = extract_icon(&source, &dir.path().join("icons"), "abc").unwrap();
        assert_eq!(out.file_name().unwrap(), "abc.png");
        assert_eq!(cached_icon(&dir.path().join("icons"), "abc"), Some(out));
        assert!(extract_icon(&dir.path().join("notes.txt"), dir.path(), "x").is_none());
    }

    /// 用这台机器上真实的 exe 检查图标提取。找不到样本时跳过。
    #[test]
    fn extracts_icon_from_a_real_exe() {
        let candidates = [
            "/Library/Developer/CommandLineTools/Library/Frameworks/Python3.framework/Versions/3.9/lib/python3.9/site-packages/pip/_vendor/distlib/t64.exe",
            "C:\\Windows\\explorer.exe",
        ];
        let Some(sample) = candidates.iter().map(Path::new).find(|p| p.is_file()) else {
            eprintln!("没有可用的 exe 样本，跳过");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let out = extract_icon(sample, dir.path(), "sample").expect("这个 exe 应当带图标");
        let bytes = fs::read(out).unwrap();
        // .ico 文件头：保留字 0、类型 1、图标数量 ≥ 1
        assert_eq!(&bytes[..4], &[0, 0, 1, 0]);
        assert!(u16::from_le_bytes([bytes[4], bytes[5]]) >= 1);
    }
}
