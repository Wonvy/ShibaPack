//! 下载安装包：断点续传、可以取消、下完校验 SHA-256。
//!
//! 下载中的内容写在 `<目录>/<地址的哈希>.part`。中断或取消后这个文件留着，下次从断点接着下。

#![cfg_attr(not(feature = "network"), allow(dead_code, unused_imports))]

use crate::{identify::is_installer, store::{sanitize, unique_path}};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

#[derive(Debug)]
pub enum DownloadError {
    Cancelled,
    /// 下载完的内容和源上公布的哈希对不上。文件已删除。
    HashMismatch,
    NotFound,
    /// 网络不通或中途断开。已下载的部分留着，下次接着下。
    Unreachable,
    Io(std::io::Error),
}

impl DownloadError {
    pub fn code(&self) -> String {
        match self {
            DownloadError::Cancelled => "cancelled".into(),
            DownloadError::HashMismatch => "hash_mismatch".into(),
            DownloadError::NotFound => "source_not_found".into(),
            DownloadError::Unreachable => "offline".into(),
            DownloadError::Io(error) => error.to_string(),
        }
    }
}

impl From<std::io::Error> for DownloadError {
    fn from(error: std::io::Error) -> Self {
        DownloadError::Io(error)
    }
}

#[derive(Debug)]
pub struct Downloaded {
    pub path: PathBuf,
    pub sha256: String,
    pub size: u64,
}

/// 从响应头的 Content-Disposition 里取文件名。
fn name_from_disposition(header: &str) -> Option<String> {
    // filename*=UTF-8''%E5%BE%AE%E4%BF%A1.exe 优先，其次 filename="x.exe"
    let extended = header.split(';').map(str::trim).find_map(|part| part.strip_prefix("filename*="));
    if let Some(value) = extended {
        let encoded = value.rsplit("''").next().unwrap_or(value);
        let mut bytes = Vec::new();
        let raw = encoded.as_bytes();
        let mut i = 0;
        while i < raw.len() {
            let hex = (raw[i] == b'%').then(|| raw.get(i + 1..i + 3)).flatten();
            match hex.and_then(|h| u8::from_str_radix(std::str::from_utf8(h).ok()?, 16).ok()) {
                Some(byte) => {
                    bytes.push(byte);
                    i += 3;
                }
                None => {
                    bytes.push(raw[i]);
                    i += 1;
                }
            }
        }
        let name = String::from_utf8_lossy(&bytes).trim().to_string();
        if !name.is_empty() {
            return Some(name);
        }
    }
    let plain = header.split(';').map(str::trim).find_map(|part| part.strip_prefix("filename="))?;
    let name = plain.trim_matches('"').trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// 文件名的来源依次是：响应头、跳转后的最终地址、调用方给的后备名字。
fn pick_name(disposition: Option<&str>, final_url: &str, fallback: &str) -> String {
    let from_url = || {
        let path = final_url.split(['?', '#']).next().unwrap_or("");
        let last = path.rsplit('/').next().unwrap_or("");
        is_installer(last).then(|| last.to_string())
    };
    let name = disposition.and_then(name_from_disposition).or_else(from_url).unwrap_or_else(|| fallback.to_string());
    // 只留文件名本身，不让服务器给的名字带着路径
    sanitize(name.rsplit(['/', '\\']).next().unwrap_or(&name))
}

#[cfg(not(feature = "network"))]
pub fn download(
    _url: &str,
    _dir: &Path,
    _fallback_name: &str,
    _expected_sha256: Option<&str>,
    _cancel: &AtomicBool,
    _on_progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<Downloaded, DownloadError> {
    Err(DownloadError::Unreachable)
}

/// 把 `url` 下载到 `dir` 里。`on_progress(已下载, 总大小)` 会被频繁调用，由调用方决定多久刷新一次界面。
#[cfg(feature = "network")]
pub fn download(
    url: &str,
    dir: &Path,
    fallback_name: &str,
    expected_sha256: Option<&str>,
    cancel: &AtomicBool,
    on_progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<Downloaded, DownloadError> {
    use ureq::ResponseExt;

    fs::create_dir_all(dir)?;
    let key = hex::encode(Sha256::digest(url.as_bytes()));
    let part = dir.join(format!("{}.part", &key[..16]));
    // 和 .part 放在一起的是服务器当时给的版本标记（ETag 或 Last-Modified）。续传时带上它（If-Range），
    // 服务器上的文件换过就会整个重发，不会把旧文件的前半截和新文件的后半截拼在一起
    let validator_file = dir.join(format!("{}.part.etag", &key[..16]));

    // 不能设总超时（大文件下多久都正常），但每次读数据最多等 60 秒：网络卡死时报错，而不是一直挂着，
    // 后面排队的下载和“取消”也就不会跟着卡住
    let config = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .timeout_per_read(Some(Duration::from_secs(60)))
        .user_agent("ShibaPack/0.1 (desktop app)")
        .proxy(ureq::Proxy::try_from_env())
        .build();
    let agent = ureq::Agent::new_with_config(config);

    let saved_validator = fs::read_to_string(&validator_file).ok().filter(|v| !v.trim().is_empty());
    let request = |from: u64| {
        let mut request = agent.get(url);
        if from > 0 {
            request = request.header("Range", format!("bytes={from}-"));
            if let Some(validator) = &saved_validator {
                request = request.header("If-Range", validator.trim());
            }
        }
        request.call()
    };
    // 没有版本标记就没法确认服务器上还是同一个文件，从头下
    let mut existing = match &saved_validator {
        Some(_) => fs::metadata(&part).map(|meta| meta.len()).unwrap_or(0),
        None => 0,
    };
    let mut response = match request(existing) {
        // 断点超出了文件范围（多半是上次其实已经下完）：从头再来
        Err(ureq::Error::StatusCode(416)) if existing > 0 => {
            existing = 0;
            request(0)
        }
        other => other,
    }
    .map_err(|error| match error {
        ureq::Error::StatusCode(404) => DownloadError::NotFound,
        _ => DownloadError::Unreachable,
    })?;

    // 服务器不支持续传、或者文件已经换过时会直接返回整个文件（200），那就丢掉已有的部分
    let resumed = existing > 0 && response.status().as_u16() == 206;
    if !resumed {
        // 记下这次的版本标记，下次续传用。弱 ETag（W/ 开头）不能用于 If-Range，退而用 Last-Modified
        let header = |name: &str| response.headers().get(name).and_then(|v| v.to_str().ok()).map(str::trim).map(String::from);
        let validator = header("etag").filter(|etag| !etag.starts_with("W/")).or_else(|| header("last-modified"));
        match validator {
            Some(validator) => fs::write(&validator_file, validator)?,
            None => {
                let _ = fs::remove_file(&validator_file);
            }
        }
    }
    let disposition = response.headers().get("content-disposition").and_then(|v| v.to_str().ok()).map(String::from);
    let file_name = pick_name(disposition.as_deref(), &response.get_uri().to_string(), fallback_name);
    let remaining = response.body().content_length();

    let mut hasher = Sha256::new();
    let mut received = 0u64;
    let mut buffer = vec![0u8; 256 * 1024];
    let mut file = if resumed {
        // 接着下之前，先把已有的部分算进哈希
        let mut done = fs::File::open(&part)?;
        loop {
            let n = done.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
            received += n as u64;
        }
        fs::OpenOptions::new().append(true).open(&part)?
    } else {
        fs::File::create(&part)?
    };
    let total = remaining.map(|rest| rest + received);
    on_progress(received, total);

    let mut reader = response.body_mut().as_reader();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(DownloadError::Cancelled);
        }
        let n = reader.read(&mut buffer).map_err(|_| DownloadError::Unreachable)?;
        if n == 0 {
            break;
        }
        file.write_all(&buffer[..n])?;
        hasher.update(&buffer[..n]);
        received += n as u64;
        on_progress(received, total);
    }
    file.sync_all()?;
    drop(file);
    if total.is_some_and(|total| received < total) {
        return Err(DownloadError::Unreachable);
    }

    let sha256 = hex::encode(hasher.finalize());
    let _ = fs::remove_file(&validator_file);
    if expected_sha256.is_some_and(|expected| !expected.eq_ignore_ascii_case(&sha256)) {
        let _ = fs::remove_file(&part);
        return Err(DownloadError::HashMismatch);
    }
    let path = unique_path(dir.join(file_name));
    fs::rename(&part, &path)?;
    Ok(Downloaded { path, sha256, size: received })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        net::TcpListener,
        sync::{atomic::AtomicUsize, Arc},
    };

    #[test]
    fn file_names() {
        assert_eq!(name_from_disposition(r#"attachment; filename="VSCode-darwin-arm64.zip""#).as_deref(), Some("VSCode-darwin-arm64.zip"));
        assert_eq!(name_from_disposition("attachment; filename*=UTF-8''%E5%BE%AE%E4%BF%A1.exe; filename=wx.exe").as_deref(), Some("微信.exe"));
        assert_eq!(name_from_disposition("inline"), None);
        assert_eq!(pick_name(None, "https://cdn.example.com/a/b/Tool-1.2.dmg?token=1", "fallback"), "Tool-1.2.dmg");
        assert_eq!(pick_name(None, "https://example.com/download/stable", "tool-1.2-arm64"), "tool-1.2-arm64");
        assert_eq!(pick_name(Some(r#"attachment; filename="../../evil.exe""#), "", "x"), "evil.exe");
    }

    /// 本机上的小服务器：只为测试，不访问外网。返回地址和“被请求了几次”的计数。
    /// `etag` 是服务器上这个文件现在的版本标记；请求带的 If-Range 和它对不上时，照规矩返回整个文件。
    fn serve(body: &'static [u8], disposition: Option<&'static str>, ranges: bool, etag: Option<&'static str>) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                counter.fetch_add(1, Ordering::SeqCst);
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let (mut start, mut if_range) = (None, None);
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    let lower = line.to_lowercase();
                    if let Some(range) = lower.strip_prefix("range: bytes=") {
                        start = range.trim().trim_end_matches('-').parse::<usize>().ok();
                    }
                    if lower.starts_with("if-range:") {
                        if_range = Some(line["if-range:".len()..].trim().to_string());
                    }
                }
                let same_file = if_range.as_deref().is_none_or(|v| Some(v) == etag);
                let (status, slice) = match start {
                    Some(from) if ranges && same_file && from <= body.len() => ("206 Partial Content", &body[from..]),
                    _ => ("200 OK", body),
                };
                let mut extra = disposition.map(|d| format!("Content-Disposition: {d}\r\n")).unwrap_or_default();
                if let Some(etag) = etag {
                    extra.push_str(&format!("ETag: {etag}\r\n"));
                }
                let head = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n", slice.len());
                let mut stream = stream;
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(slice);
            }
        });
        (format!("http://{address}"), hits)
    }

    const BODY: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz-this-is-the-installer-payload";

    fn sha(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    #[test]
    #[cfg(feature = "network")]
    fn downloads_verifies_and_names_the_file() {
        let (base, _) = serve(BODY, Some(r#"attachment; filename="Tool-2.0-x64.exe""#), true, Some("\"v1\""));
        let dir = tempfile::tempdir().unwrap();
        let mut last = (0, None);
        let done = download(&format!("{base}/get/latest"), dir.path(), "fallback.exe", Some(&sha(BODY)), &AtomicBool::new(false), &mut |r, t| last = (r, t)).unwrap();
        assert_eq!(done.path.file_name().unwrap(), "Tool-2.0-x64.exe");
        assert_eq!(fs::read(&done.path).unwrap(), BODY);
        assert_eq!((done.size, done.sha256.as_str()), (BODY.len() as u64, sha(BODY).as_str()));
        assert_eq!(last, (BODY.len() as u64, Some(BODY.len() as u64)));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1, "不应留下 .part 文件");
    }

    /// 假装上次下到一半断了：留下前 `len` 个字节，以及当时服务器给的版本标记。
    fn leave_part(dir: &Path, url: &str, bytes: &[u8], etag: Option<&str>) {
        let key = hex::encode(Sha256::digest(url.as_bytes()));
        fs::write(dir.join(format!("{}.part", &key[..16])), bytes).unwrap();
        if let Some(etag) = etag {
            fs::write(dir.join(format!("{}.part.etag", &key[..16])), etag).unwrap();
        }
    }

    #[test]
    #[cfg(feature = "network")]
    fn resumes_from_a_partial_file() {
        let (base, hits) = serve(BODY, None, true, Some("\"v1\""));
        let dir = tempfile::tempdir().unwrap();
        let url = format!("{base}/files/Tool-2.0.dmg");
        leave_part(dir.path(), &url, &BODY[..20], Some("\"v1\""));

        let mut first = None;
        let done = download(&url, dir.path(), "x", Some(&sha(BODY)), &AtomicBool::new(false), &mut |r, _| {
            first.get_or_insert(r);
        })
        .unwrap();
        assert_eq!(first, Some(20), "应当从第 20 个字节接着下");
        assert_eq!(fs::read(&done.path).unwrap(), BODY);
        assert_eq!(done.path.file_name().unwrap(), "Tool-2.0.dmg");
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1, "下完不应留下 .part 和版本标记");
    }

    /// 地址没变、文件换过（比如 Homebrew 里不带版本号的下载地址）：不能把旧文件的前半截接上新文件。
    #[test]
    #[cfg(feature = "network")]
    fn starts_over_when_the_file_changed_upstream() {
        let (base, _) = serve(BODY, None, true, Some("\"v2\""));
        let dir = tempfile::tempdir().unwrap();
        let url = format!("{base}/stable");
        leave_part(dir.path(), &url, b"OLD-VERSION-BYTES-xx", Some("\"v1\""));
        let mut first = None;
        let done = download(&url, dir.path(), "tool.dmg", None, &AtomicBool::new(false), &mut |r, _| {
            first.get_or_insert(r);
        })
        .unwrap();
        assert_eq!(first, Some(0), "文件换过就从头下");
        assert_eq!(fs::read(&done.path).unwrap(), BODY);
    }

    /// 没有记下版本标记（比如旧版本留下的 .part）：确认不了是不是同一个文件，从头下。
    #[test]
    #[cfg(feature = "network")]
    fn starts_over_without_a_saved_validator() {
        let (base, _) = serve(BODY, None, true, None);
        let dir = tempfile::tempdir().unwrap();
        let url = format!("{base}/Tool.exe");
        leave_part(dir.path(), &url, b"stale bytes from an older file", None);
        let done = download(&url, dir.path(), "x", Some(&sha(BODY)), &AtomicBool::new(false), &mut |_, _| {}).unwrap();
        assert_eq!(fs::read(&done.path).unwrap(), BODY);
    }

    #[test]
    #[cfg(feature = "network")]
    fn restarts_when_the_server_ignores_ranges() {
        let (base, _) = serve(BODY, None, false, Some("\"v1\""));
        let dir = tempfile::tempdir().unwrap();
        let url = format!("{base}/Tool.exe");
        leave_part(dir.path(), &url, b"stale bytes from an older file", Some("\"v1\""));
        let done = download(&url, dir.path(), "x", Some(&sha(BODY)), &AtomicBool::new(false), &mut |_, _| {}).unwrap();
        assert_eq!(fs::read(&done.path).unwrap(), BODY);
    }

    #[test]
    #[cfg(feature = "network")]
    fn rejects_a_wrong_hash_and_honours_cancel() {
        let (base, _) = serve(BODY, None, true, None);
        let dir = tempfile::tempdir().unwrap();
        let url = format!("{base}/Tool.exe");
        let wrong = "0".repeat(64);
        let result = download(&url, dir.path(), "x", Some(&wrong), &AtomicBool::new(false), &mut |_, _| {});
        assert!(matches!(result, Err(DownloadError::HashMismatch)));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0, "哈希不对的文件应当删掉");

        let result = download(&url, dir.path(), "x", None, &AtomicBool::new(true), &mut |_, _| {});
        assert!(matches!(result, Err(DownloadError::Cancelled)));

        let missing = download("http://127.0.0.1:9/nothing.exe", dir.path(), "x", None, &AtomicBool::new(false), &mut |_, _| {});
        assert!(matches!(missing, Err(DownloadError::Unreachable)));
    }
}
