//! 图标提取。所有图标统一转成 data URL 返回给前端，
//! 避免前端直接访问本地路径带来的权限与跨平台问题。
//! 提取失败时返回空字符串，前端会回退到中性首字母占位图标。

use std::fs;
use std::path::Path;
#[cfg(target_os = "linux")]
use std::path::PathBuf;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;

const MAX_ICON_BYTES: usize = 512 * 1024;

fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!("data:{};base64,{}", mime, BASE64.encode(bytes))
}

pub fn png_data_url(bytes: &[u8]) -> String {
    data_url("image/png", bytes)
}

/// 把 PNG 原始像素编码为 PNG data URL。
fn encode_png(rgba: &[u8], width: u32, height: u32) -> Option<String> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
    }
    Some(png_data_url(&out))
}

/// 直接读取图片文件（png/ico/svg/jpg/webp）为 data URL。
fn file_icon(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => return None,
    };
    let bytes = fs::read(path).ok()?;
    if bytes.is_empty() || bytes.len() > MAX_ICON_BYTES {
        return None;
    }
    Some(data_url(mime, &bytes))
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

/// 展开注册表路径里常见的环境变量，并去掉首尾引号/空白。
#[cfg(target_os = "windows")]
fn expand_path(raw: &str) -> String {
    let mut s = raw.trim().trim_matches('"').trim().to_string();
    let vars: &[(&str, String)] = &[
        ("%ProgramFiles%", std::env::var("ProgramW6432").or_else(|_| std::env::var("ProgramFiles")).unwrap_or_else(|_| r"C:\Program Files".into())),
        ("%ProgramFiles(x86)%", std::env::var("ProgramFiles(x86)").unwrap_or_else(|_| r"C:\Program Files (x86)".into())),
        ("%SystemRoot%", std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into())),
        ("%windir%", std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into())),
        ("%LOCALAPPDATA%", std::env::var("LOCALAPPDATA").unwrap_or_default()),
        ("%APPDATA%", std::env::var("APPDATA").unwrap_or_default()),
        ("%USERPROFILE%", std::env::var("USERPROFILE").unwrap_or_default()),
        ("%ProgramData%", std::env::var("ProgramData").unwrap_or_default()),
    ];
    for (key, val) in vars {
        if val.is_empty() {
            continue;
        }
        s = s.replace(key, val);
        s = s.replace(&key.to_ascii_lowercase(), val);
    }
    s.trim().trim_end_matches(|c| c == '\\' || c == '/').to_string()
}

/// 解析 DisplayIcon，支持 `path`、`path,index`、`"path",index`、负索引（资源 ID）。
#[cfg(target_os = "windows")]
fn parse_display_icon(raw: &str) -> Option<(String, i32)> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    let (path_part, index) = if let Some(pos) = s.rfind(',') {
        match s[pos + 1..].trim().parse::<i32>() {
            Ok(idx) => (&s[..pos], idx),
            Err(_) => (s, 0),
        }
    } else {
        (s, 0)
    };
    let path = expand_path(path_part);
    if path.is_empty() {
        return None;
    }
    Some((path, index))
}

/// 从卸载命令行里取出可执行文件绝对路径。
#[cfg(target_os = "windows")]
fn exe_from_command(cmd: &str) -> Option<String> {
    let s = cmd.trim();
    if s.is_empty() {
        return None;
    }
    if s.starts_with('"') {
        if let Some(end) = s[1..].find('"') {
            let p = expand_path(&s[1..=end]);
            if p.to_ascii_lowercase().ends_with(".exe") && (p.contains('\\') || p.contains('/')) {
                return Some(p);
            }
        }
    }
    for token in s.split_whitespace() {
        let t = token.trim_matches('"');
        if t.to_ascii_lowercase().ends_with(".exe") && (t.contains('\\') || t.contains('/')) {
            return Some(expand_path(t));
        }
    }
    None
}

/// 去掉名称里的版本号与标点，用于在安装目录里匹配主程序 exe。
#[cfg(target_os = "windows")]
fn name_key(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// 从卸载命令里提取 MSI 产品代码（含花括号）。
#[cfg(target_os = "windows")]
fn msi_guid(uninstall_string: &str) -> Option<String> {
    let start = uninstall_string.find('{')?;
    let end = start + uninstall_string[start..].find('}')?;
    let guid = &uninstall_string[start..=end];
    if guid.len() < 12 {
        return None;
    }
    Some(guid.to_string())
}

/// 用 Windows Installer 查询 MSI 产品注册的 ARP 图标路径。
/// 实测大量 MSI 安装项没有 DisplayIcon，但产品属性里带 ProductIcon。
#[cfg(target_os = "windows")]
fn msi_product_icon(guid: &str) -> Option<String> {
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::System::ApplicationInstallationAndServicing::MsiGetProductInfoW;

    let product: Vec<u16> = guid.encode_utf16().chain(std::iter::once(0)).collect();
    let attribute: Vec<u16> = "ProductIcon"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut buf = vec![0u16; 1024];
    let mut len = buf.len() as u32;
    let ret = unsafe {
        MsiGetProductInfoW(
            PCWSTR(product.as_ptr()),
            PCWSTR(attribute.as_ptr()),
            Some(PWSTR(buf.as_mut_ptr())),
            Some(&mut len),
        )
    };
    if ret != 0 {
        return None;
    }
    let n = (len as usize).min(buf.len());
    let path = String::from_utf16_lossy(&buf[..n])
        .trim_matches('\0')
        .trim()
        .to_string();
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

/// 受限递归收集安装目录下的 exe（很多程序主程序在 bin/ 等子目录里）。
#[cfg(target_os = "windows")]
fn collect_exes(dir: &Path, depth: usize, out: &mut Vec<std::path::PathBuf>) {
    if out.len() >= 24 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if out.len() >= 24 {
            break;
        }
        let p = entry.path();
        let name = p
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if p.is_file() {
            if name.ends_with(".exe")
                && !name.contains("unins")
                && !name.contains("uninstall")
                && !name.contains("setup")
                && !name.contains("crash")
                && !name.contains("helper")
            {
                out.push(p);
            }
        } else if depth > 0 && p.is_dir() {
            collect_exes(&p, depth - 1, out);
        }
    }
}

/// 按优先级生成图标候选路径：
/// DisplayIcon → MSI 产品图标 → 安装目录主程序（含子目录）→ 卸载程序。
#[cfg(target_os = "windows")]
fn icon_candidates(
    display_icon: &str,
    install_location: &str,
    uninstall_string: &str,
    display_name: &str,
) -> Vec<(String, i32)> {
    let mut out: Vec<(String, i32)> = Vec::new();
    if let Some((path, index)) = parse_display_icon(display_icon) {
        out.push((path, index));
    }

    // MSI 安装的程序大多没有 DisplayIcon，用产品代码查注册的产品图标
    if let Some(guid) = msi_guid(uninstall_string) {
        if let Some(path) = msi_product_icon(&guid) {
            out.push((path, 0));
        }
    }

    let location = expand_path(install_location);
    if !location.is_empty() && Path::new(&location).is_dir() {
        let key = name_key(display_name);
        let mut exes: Vec<std::path::PathBuf> = Vec::new();
        collect_exes(Path::new(&location), 2, &mut exes);
        let score = |p: &std::path::PathBuf| {
            let stem = p
                .file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_lowercase();
            let stem_key = name_key(&stem);
            if !stem_key.is_empty()
                && !key.is_empty()
                && (key.contains(&stem_key) || stem_key.contains(&key))
            {
                0
            } else {
                1
            }
        };
        exes.sort_by_key(score);
        for p in exes.into_iter().take(2) {
            out.push((p.to_string_lossy().to_string(), 0));
        }
    }

    if let Some(path) = exe_from_command(uninstall_string) {
        out.push((path, 0));
    }

    // 去重，避免同一路径反复尝试
    let mut seen = std::collections::HashSet::new();
    out.retain(|(p, _)| seen.insert(p.to_ascii_lowercase()));
    out
}

/// HICON → PNG data URL。调用方负责销毁 HICON。
#[cfg(target_os = "windows")]
fn hicon_to_png(hicon: windows::Win32::UI::WindowsAndMessaging::HICON) -> Option<String> {
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, HGDIOBJ, BITMAP,
        BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetIconInfo, ICONINFO};

    let mut info = ICONINFO::default();
    let ok = unsafe { GetIconInfo(hicon, &mut info) }.is_ok();
    if !ok || info.hbmColor.0.is_null() {
        if !info.hbmMask.0.is_null() {
            let _ = unsafe { DeleteObject(HGDIOBJ(info.hbmMask.0)) };
        }
        return None;
    }

    let mut bitmap = BITMAP::default();
    let wrote = unsafe {
        GetObjectW(
            HGDIOBJ(info.hbmColor.0),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bitmap as *mut _ as *mut _),
        )
    };
    if wrote == 0 {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
            let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
        }
        return None;
    }
    let w = bitmap.bmWidth;
    let h = bitmap.bmHeight;
    if w <= 0 || h <= 0 || w > 512 || h > 512 {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
            let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
        }
        return None;
    }

    let hdc = unsafe { CreateCompatibleDC(None) };
    if hdc.0.is_null() {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
            let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
        }
        return None;
    }

    let mut bi = BITMAPINFO::default();
    bi.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: w,
        biHeight: -h, // 负值表示自顶向下
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    let got = unsafe {
        GetDIBits(
            hdc,
            info.hbmColor,
            0,
            h as u32,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bi,
            DIB_RGB_COLORS,
        )
    };
    unsafe {
        let _ = DeleteDC(hdc);
        let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
        let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
    }
    if got == 0 {
        return None;
    }

    // DIB 为 BGRA，转为 RGBA。
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    // 某些图标 GetDIBits 不保留 alpha，全部为 0 时回退为不透明，避免完全不可见。
    if pixels.chunks_exact(4).all(|p| p[3] == 0) {
        for pixel in pixels.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
    }
    encode_png(&pixels, w as u32, h as u32)
}

/// 从 exe/dll 中用 Win32 API 提取图标；依次尝试指定索引、0、-1。
#[cfg(target_os = "windows")]
fn exe_icon(path: &str, preferred_index: i32) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ExtractIconExW;
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, HICON};

    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let mut indices = vec![preferred_index, 0, -1];
    indices.dedup();
    for index in indices {
        let mut hicon = HICON::default();
        let extracted = unsafe {
            ExtractIconExW(PCWSTR(wide.as_ptr()), index, Some(&mut hicon), None, 1)
        };
        if extracted == 0 || hicon.0.is_null() {
            continue;
        }
        let url = hicon_to_png(hicon);
        let _ = unsafe { DestroyIcon(hicon) };
        if url.is_some() {
            return url;
        }
    }
    None
}

/// 兜底：用 Shell 的文件关联图标（对没有图标资源或索引异常的文件有效）。
#[cfg(target_os = "windows")]
fn shell_file_icon(path: &str) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_NORMAL, FILE_FLAGS_AND_ATTRIBUTES};
    use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_FLAGS, SHGFI_ICON};
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, HICON};

    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let mut info = SHFILEINFOW::default();
    let attrs: FILE_FLAGS_AND_ATTRIBUTES = FILE_ATTRIBUTE_NORMAL;
    let ret = unsafe {
        SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            attrs,
            Some(&mut info as *mut _),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_FLAGS(SHGFI_ICON.0),
        )
    };
    if ret == 0 || info.hIcon.0.is_null() {
        return None;
    }
    let url = hicon_to_png(info.hIcon);
    let _ = unsafe { DestroyIcon(HICON(info.hIcon.0)) };
    url
}

/// 已安装软件图标：DisplayIcon 缺失时从安装目录/卸载程序推断。
/// 实测约 1/3 的注册表项没有 DisplayIcon，这是"很多程序没图标"的主因。
#[cfg(target_os = "windows")]
pub fn windows_icon(
    display_icon: &str,
    install_location: &str,
    uninstall_string: &str,
    display_name: &str,
) -> String {
    let candidates = icon_candidates(display_icon, install_location, uninstall_string, display_name);
    let mut existing: Option<&str> = None;
    for (path, index) in &candidates {
        let p = Path::new(path);
        if let Some(url) = file_icon(p) {
            return url;
        }
        if let Some(url) = exe_icon(path, *index) {
            return url;
        }
        if existing.is_none() && p.exists() {
            existing = Some(path.as_str());
        }
    }
    // 所有候选都拿不到图标资源时，退回系统文件关联图标
    if let Some(path) = existing {
        return shell_file_icon(path).unwrap_or_default();
    }
    String::new()
}

/// 安装包文件图标：用于导入后立刻在列表里显示真实图标。
#[cfg(target_os = "windows")]
pub fn package_icon(path: &Path) -> String {
    if let Some(url) = file_icon(path) {
        return url;
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(ext.as_str(), "exe" | "dll" | "scr" | "msi" | "msix" | "appx") {
        let s = path.to_string_lossy().to_string();
        if let Some(url) = exe_icon(&s, 0) {
            return url;
        }
        return shell_file_icon(&s).unwrap_or_default();
    }
    String::new()
}

#[cfg(not(target_os = "windows"))]
pub fn package_icon(_path: &Path) -> String {
    String::new()
}

// ---------------------------------------------------------------------------
// macOS
// ---------------------------------------------------------------------------

/// 用 macOS 自带的 sips 把 icns 转成 png。
#[cfg(target_os = "macos")]
fn icns_to_png(icns: &Path) -> Option<String> {
    use std::process::{Command, Stdio};

    let tmp = std::env::temp_dir().join(format!("shibapack-icon-{}.png", uuid::Uuid::new_v4()));
    let status = Command::new("sips")
        .arg("-s")
        .arg("format")
        .arg("png")
        .arg(icns)
        .arg("--out")
        .arg(&tmp)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()?;
    if !status.success() {
        let _ = fs::remove_file(&tmp);
        return None;
    }
    let bytes = fs::read(&tmp).ok();
    let _ = fs::remove_file(&tmp);
    bytes.and_then(|b| if b.is_empty() { None } else { Some(png_data_url(&b)) })
}

#[cfg(target_os = "macos")]
fn plist_string(xml: &str, key: &str) -> String {
    let pattern = format!(
        r"(?s)<key>\s*{}</key>\s*<string>\s*(.*?)\s*</string>",
        regex::escape(key)
    );
    let Ok(re) = regex::Regex::new(&pattern) else {
        return String::new();
    };
    re.captures(xml)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_default()
}

#[cfg(target_os = "macos")]
pub fn macos_icon(app_path: &Path, plist_xml: &str) -> String {
    let mut name = plist_string(plist_xml, "CFBundleIconFile");
    if name.is_empty() {
        name = "AppIcon".to_string();
    }
    if !name.to_ascii_lowercase().ends_with(".icns") {
        name.push_str(".icns");
    }
    let resources = app_path.join("Contents/Resources");
    let icns = resources.join(&name);
    if let Some(url) = file_icon(&icns) {
        return url;
    }
    // 兜底：扫描 Resources 下第一个 icns。
    if let Ok(entries) = fs::read_dir(&resources) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("icns") {
                if let Some(url) = icns_to_png(&p) {
                    return url;
                }
            }
        }
    }
    icns_to_png(&icns).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Linux
// ---------------------------------------------------------------------------

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;

    #[test]
    fn parse_display_icon_handles_quotes_index_and_env() {
        let (path, index) = parse_display_icon(r#""C:\Program Files\App\app.exe",0"#).unwrap();
        assert_eq!(path, r"C:\Program Files\App\app.exe");
        assert_eq!(index, 0);
        // 负索引是资源 ID，必须保留
        assert_eq!(parse_display_icon(r"C:\App\app.exe,-101").unwrap().1, -101);
        // 环境变量需要展开
        let (path, _) = parse_display_icon(r"%ProgramFiles%\App\app.exe,0").unwrap();
        assert!(!path.contains("%ProgramFiles%"));
    }

    #[test]
    fn exe_from_command_extracts_quoted_path() {
        assert_eq!(
            exe_from_command(r#""C:\Program Files\Foo\unins000.exe" /SILENT"#).as_deref(),
            Some(r"C:\Program Files\Foo\unins000.exe")
        );
        // MsiExec 这类没有绝对路径的命令行不应被当成图标来源
        assert!(exe_from_command("MsiExec.exe /X{ABC-123}").is_none());
    }

    #[test]
    #[ignore = "手动运行：统计已安装程序图标覆盖率"]
    fn icon_coverage_report() {
        let list = crate::platform::installed_apps();
        let total = list.len();
        let with_icon = list.iter().filter(|p| !p.icon.is_empty()).count();
        println!(
            "[icon coverage] {}/{} ({}%)",
            with_icon,
            total,
            if total == 0 { 0 } else { with_icon * 100 / total }
        );
        assert!(with_icon > 0);
    }

    #[test]
    fn icon_candidates_falls_back_to_install_location() {
        let dir = std::env::temp_dir().join(format!("shibapack-icon-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("MyApp.exe"), b"fake").unwrap();
        fs::write(dir.join("unins000.exe"), b"fake").unwrap();
        let candidates = icon_candidates("", dir.to_str().unwrap(), "", "MyApp 2.0");
        assert!(
            candidates.iter().any(|(p, _)| p.ends_with("MyApp.exe")),
            "缺少 DisplayIcon 时应从安装目录推断主程序"
        );
        assert!(
            !candidates.iter().any(|(p, _)| p.ends_with("unins000.exe")),
            "不应把卸载程序当作主程序图标"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

#[cfg(target_os = "linux")]
pub fn linux_icon(icon: &str) -> String {
    if icon.trim().is_empty() {
        return String::new();
    }
    let p = Path::new(icon);
    if p.is_absolute() {
        return file_icon(p).unwrap_or_default();
    }

    let mut dirs = vec![
        PathBuf::from("/usr/share/pixmaps"),
        PathBuf::from("/usr/local/share/pixmaps"),
    ];
    for size in ["48x48", "64x64", "32x32", "128x128", "scalable"] {
        for kind in ["apps", "categories", "mimetypes"] {
            dirs.push(PathBuf::from(format!("/usr/share/icons/hicolor/{}/{}", size, kind)));
        }
    }
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local/share/pixmaps"));
        dirs.push(home.join(".local/share/icons/hicolor/48x48/apps"));
        dirs.push(home.join(".local/share/icons/hicolor/scalable/apps"));
    }

    for dir in dirs {
        for ext in ["png", "svg"] {
            let candidate = dir.join(format!("{}.{}", icon, ext));
            if candidate.exists() {
                if let Some(url) = file_icon(&candidate) {
                    return url;
                }
            }
        }
    }
    String::new()
}
