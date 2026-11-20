//! 从文件里取图标，并把 Windows 的 ICO 转成跨平台 WebView 都能显示的 PNG。

use base64::{engine::general_purpose::STANDARD, Engine};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
const MAX_ICON_BYTES: u64 = 4 * 1024 * 1024;

/// 取出 `source` 的图标，写到 `dest_dir/<stem>.<扩展名>`，返回写出的路径。取不到时返回 `None`。
pub fn extract_icon(source: &Path, dest_dir: &Path, stem: &str) -> Option<PathBuf> {
    extract_icon_at(source, dest_dir, stem, 0)
}

pub(crate) fn extract_icon_at(source: &Path, dest_dir: &Path, stem: &str, index: i32) -> Option<PathBuf> {
    let ext = source.extension()?.to_string_lossy().to_ascii_lowercase();
    let (bytes, out_ext) = match ext.as_str() {
        "icns" => (icns_png(source)?, "png"),
        "exe" | "dll" => (ico_png(&pe_icon(source, index)?)?, "png"),
        "msi" => {
            let bytes = msi_icon(source)?;
            if bytes.starts_with(b"MZ") {
                (ico_png(&pe_icon_bytes(&bytes, 0)?)?, "png")
            } else {
                image_bytes(bytes)?
            }
        }
        "ico" | "png" | "svg" | "jpg" | "jpeg" | "gif" | "webp" => image_bytes(read_image(source)?)?,
        _ => return None,
    };
    fs::create_dir_all(dest_dir).ok()?;
    let dest = dest_dir.join(format!("{stem}.{out_ext}"));
    let tmp = dest_dir.join(format!("{}.tmp", crate::new_id()));
    if fs::write(&tmp, bytes).and_then(|_| fs::rename(&tmp, &dest)).is_err() {
        let _ = fs::remove_file(&tmp);
        return None;
    }
    Some(dest)
}

/// 已经取过的图标（任何一种扩展名）。
pub fn cached_icon(dest_dir: &Path, stem: &str) -> Option<PathBuf> {
    ["png", "svg", "jpg", "gif", "webp"].iter().map(|ext| dest_dir.join(format!("{stem}.{ext}"))).find(|p| p.is_file())
}

pub(crate) fn portable_icon(path: &Path) -> Option<Arc<str>> {
    let (bytes, ext) = image_bytes(read_image(path)?)?;
    let subtype = match ext {
        "svg" => "svg+xml",
        "jpg" => "jpeg",
        other => other,
    };
    Some(format!("data:image/{subtype};base64,{}", STANDARD.encode(bytes)).into())
}

fn read_image(path: &Path) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path).ok()?.take(MAX_ICON_BYTES + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= MAX_ICON_BYTES).then_some(bytes)
}

fn image_bytes(bytes: Vec<u8>) -> Option<(Vec<u8>, &'static str)> {
    match crate::webicon::sniff(&bytes)? {
        "ico" => Some((ico_png(&bytes)?, "png")),
        ext => Some((bytes, ext)),
    }
}

fn ico_png(bytes: &[u8]) -> Option<Vec<u8>> {
    let directory = ico::IconDir::read(std::io::Cursor::new(bytes)).ok()?;
    let mut entries: Vec<_> = directory.entries().iter().collect();
    entries.sort_by_key(|entry| {
        let size = entry.width().min(entry.height());
        (size < 64, size.abs_diff(64), std::cmp::Reverse(entry.bits_per_pixel()))
    });
    entries.into_iter().find_map(|entry| {
        let icon = entry.decode().ok()?;
        let mut png = Vec::new();
        icon.write_png(&mut png).ok()?;
        Some(png)
    })
}

/// MSI 的 ARPPRODUCTICON 属性指向 Icon 表中的产品图标流。
fn msi_icon(path: &Path) -> Option<Vec<u8>> {
    let mut package = msi::open(path).ok()?;
    let name = package.select_rows(msi::Select::table("Property").columns(&["Property", "Value"]))
        .ok()?
        .filter(|row| row[0].as_str() == Some("ARPPRODUCTICON"))
        .find_map(|row| row[1].as_str().map(str::to_owned))?;
    let mut bytes = Vec::new();
    package.read_stream(&format!("Icon.{name}")).ok()?.take(MAX_ICON_BYTES + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= MAX_ICON_BYTES).then_some(bytes)
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

/// 从 exe / dll 的资源里按序号（负值为资源 ID）取一组图标，拼成 .ico。
/// 用内存映射读文件，几百 MB 的安装包也只会碰到文件头和资源段。
fn pe_icon(path: &Path, index: i32) -> Option<Vec<u8>> {
    let file = fs::File::open(path).ok()?;
    // 安全性：映射期间文件被别的程序改写会读到变化的内容，这里只读不写，最坏结果是取图标失败
    let map = unsafe { memmap2::Mmap::map(&file).ok()? };
    pe_icon_bytes(&map, index)
}

fn pe_icon_bytes(bytes: &[u8], index: i32) -> Option<Vec<u8>> {
    let pe = pelite::PeFile::from_bytes(bytes).ok()?;
    let resources = pe.resources().ok()?;
    let mut groups = resources.icons();
    let (_, group) = if index < 0 {
        groups.filter_map(Result::ok).find(|(name, _)| *name == pelite::resources::Name::Id(index.unsigned_abs()))?
    } else {
        groups.nth(index as usize)?.ok()?
    };
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

    #[test]
    fn extracts_the_msi_product_icon_as_png() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("app.msi");
        crate::fileinfo::write_test_msi(&source, "App", "1.0", "Acme");
        let mut package = msi::open_rw(&source).unwrap();
        package.insert_rows(msi::Insert::into("Property").row(vec!["ARPPRODUCTICON".into(), "app.ico".into()])).unwrap();

        let image = ico::IconImage::from_rgba_data(1, 1, vec![12, 34, 56, 255]);
        let mut directory = ico::IconDir::new(ico::ResourceType::Icon);
        directory.add_entry(ico::IconDirEntry::encode(&image).unwrap());
        let mut bytes = Vec::new();
        directory.write(&mut bytes).unwrap();
        package.write_stream("Icon.app.ico").unwrap().write_all(&bytes).unwrap();
        package.flush().unwrap();
        drop(package);

        let extracted = extract_icon(&source, dir.path(), "app").unwrap();
        assert_eq!(extracted.extension().unwrap(), "png");
        let png = ico::IconImage::read_png(std::io::BufReader::new(fs::File::open(extracted).unwrap())).unwrap();
        assert_eq!(png.rgba_data(), image.rgba_data());
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
        assert!(bytes.starts_with(PNG_MAGIC));
    }
}
