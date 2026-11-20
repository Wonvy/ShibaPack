//! 在线源：查一个软件的最新版本和可以下载的安装包。
//!
//! - Homebrew Cask：macOS 应用，按 cask 名字查，也可以在整份目录里搜索。
//! - GitHub Releases：按 `owner/repo` 查最新发布里的文件。
//! - winget：Windows 应用，按包标识读社区清单。
//!
//! GitHub 对匿名请求限流（每小时 60 次），GitHub Releases 和 winget 都受它影响。

#![cfg_attr(not(feature = "network"), allow(dead_code, unused_imports))]

use crate::{
    identify::{compare_versions, identify, is_installer},
    model::{Asset, Platform, Release, SourceKind},
};
use serde::Serialize;
use serde_json::Value;
use std::{collections::HashMap, path::Path, time::Duration};

const USER_AGENT: &str = "ShibaPack/0.1 (desktop app)";
/// Homebrew 的整份目录有二十来 MB，取一次用七天。
const CATALOG_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 3600);
const CATALOG_FILE: &str = "homebrew-casks.json";

#[derive(Debug, PartialEq)]
pub enum SourceError {
    /// 这个源里没有这个软件，或者标识写错了。
    NotFound,
    /// 标识的格式不对。
    Invalid,
    /// 被限流了，过一会儿再试。
    RateLimited,
    /// 网络不通。
    Unreachable,
}

impl SourceError {
    /// 给界面翻译用的代码。
    pub fn code(&self) -> &'static str {
        match self {
            SourceError::NotFound => "source_not_found",
            SourceError::Invalid => "invalid_reference",
            SourceError::RateLimited => "rate_limited",
            SourceError::Unreachable => "offline",
        }
    }
}

/// 在 Homebrew 目录里搜到的一个软件。
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub reference: String,
    pub name: String,
    pub description: String,
    pub homepage: String,
    pub version: String,
}

/// 把用户填的内容整理成标准的标识：GitHub 的网址变成 `owner/repo`，去掉首尾空白。
pub fn normalize_reference(kind: SourceKind, input: &str) -> Result<String, SourceError> {
    let input = input.trim();
    let ok = |text: &str, extra: &str| !text.is_empty() && text.chars().all(|c| c.is_ascii_alphanumeric() || extra.contains(c));
    match kind {
        SourceKind::Homebrew => {
            let token = input.to_lowercase();
            ok(&token, "-_.@+").then_some(token).ok_or(SourceError::Invalid)
        }
        SourceKind::Github => {
            let path = input.trim_start_matches("https://").trim_start_matches("http://").trim_start_matches("github.com/");
            let mut parts = path.split('/').filter(|p| !p.is_empty());
            match (parts.next(), parts.next()) {
                (Some(owner), Some(repo)) if ok(owner, "-_.") && ok(repo.trim_end_matches(".git"), "-_.") => {
                    Ok(format!("{owner}/{}", repo.trim_end_matches(".git")))
                }
                _ => Err(SourceError::Invalid),
            }
        }
        SourceKind::Winget => {
            (input.contains('.') && !input.starts_with('.') && !input.ends_with('.') && ok(input, "-_.+"))
                .then(|| input.to_string())
                .ok_or(SourceError::Invalid)
        }
    }
}

// ── 解析（不联网，可以直接测试）────────────────────────────────

/// 文件名取网址的最后一段；网址里看不出文件名时（比如以 /stable 结尾）用后备的名字。
fn name_from_url(url: &str, fallback: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let last = path.rsplit('/').next().unwrap_or("");
    let decoded = percent_decode(last);
    if is_installer(&decoded) { decoded } else { fallback.to_string() }
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = (bytes[i] == b'%').then(|| bytes.get(i + 1..i + 3)).flatten();
        match hex.and_then(|h| u8::from_str_radix(std::str::from_utf8(h).ok()?, 16).ok()) {
            Some(byte) => {
                out.push(byte);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Homebrew 单个 cask 的 JSON → 版本和安装包。
pub fn parse_homebrew(cask: &Value) -> Option<Release> {
    let token = cask["token"].as_str()?;
    // 版本形如 "1.2.3,456"，逗号后面是内部构建号
    let version = cask["version"].as_str()?.split(',').next()?.to_string();
    let url = cask["url"].as_str()?;
    let checksum = |value: &Value| value.as_str().filter(|s| s.len() == 64).map(str::to_lowercase);

    // 顶层的地址是 Apple 芯片用的；variations 里不带 arm64_ 前缀的是 Intel 机器用的
    let intel = cask["variations"].as_object().and_then(|variations| {
        variations.iter().filter(|(key, _)| !key.starts_with("arm64_")).find_map(|(_, v)| {
            let other = v["url"].as_str().filter(|other| *other != url)?;
            Some((other.to_string(), checksum(&v["sha256"])))
        })
    });

    let asset = |url: &str, sha256: Option<String>, arch: &str| {
        let guessed = identify(url.rsplit('/').next().unwrap_or(""));
        let arch = if arch.is_empty() { guessed.arch } else { arch.to_string() };
        let suffix = if arch.is_empty() { String::new() } else { format!("-{arch}") };
        Asset {
            name: name_from_url(url, &format!("{token}-{version}{suffix}")),
            url: url.to_string(),
            platform: Platform::Macos,
            arch,
            size: None,
            sha256,
        }
    };
    let mut assets = vec![asset(url, checksum(&cask["sha256"]), if intel.is_some() { "arm64" } else { "" })];
    if let Some((intel_url, sha256)) = intel {
        assets.push(asset(&intel_url, sha256, "x64"));
    }
    Some(Release { version, published_at: String::new(), page: cask["homepage"].as_str().unwrap_or("").to_string(), assets })
}

/// GitHub 的一次发布 → 版本和安装包。校验和、签名、源码包这些不算安装包。
pub fn parse_github(release: &Value) -> Option<Release> {
    let tag = release["tag_name"].as_str()?;
    // "v1.2.3"、"release-1.2.3" → "1.2.3"；标签里没有数字版本号时原样使用
    let identified = identify(&format!("x-{tag}.zip")).version;
    let version = if identified.is_empty() { tag.trim_start_matches(['v', 'V']).to_string() } else { identified };
    const SKIP: [&str; 9] = [".sig", ".asc", ".sha256", ".sha512", ".blockmap", ".yml", ".yaml", ".json", ".asar.gz"];
    let assets = release["assets"]
        .as_array()?
        .iter()
        .filter_map(|asset| {
            let name = asset["name"].as_str()?;
            let lower = name.to_lowercase();
            if !is_installer(name) || SKIP.iter().any(|ext| lower.ends_with(ext)) {
                return None;
            }
            let id = identify(name);
            Some(Asset {
                name: name.to_string(),
                url: asset["browser_download_url"].as_str()?.to_string(),
                platform: id.platform,
                arch: id.arch,
                size: asset["size"].as_u64(),
                sha256: asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(str::to_lowercase),
            })
        })
        .collect();
    Some(Release {
        version,
        published_at: release["published_at"].as_str().unwrap_or("").to_string(),
        page: release["html_url"].as_str().unwrap_or("").to_string(),
        assets,
    })
}

/// winget 的安装清单（YAML）里只用得到几个字段，所以不引入完整的 YAML 解析库，按行读。
pub fn parse_winget(id: &str, yaml: &str) -> Option<Release> {
    let unquote = |value: &str| value.trim().trim_matches(['\'', '"']).to_string();
    let mut version = String::new();
    let mut installers: Vec<HashMap<String, String>> = Vec::new();
    let mut in_installers = false;
    for line in yaml.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        let text = line.trim_start();
        if indent == 0 && !text.starts_with('-') {
            in_installers = text.trim_end() == "Installers:";
            if let Some(value) = text.strip_prefix("PackageVersion:") {
                version = unquote(value);
            }
            continue;
        }
        if !in_installers {
            continue;
        }
        // "- Architecture: x64" 开始一个新的安装包；它下面缩进两格的 "键: 值" 属于它，更深的嵌套不管
        let (starts_item, pair) = match text.strip_prefix("- ") {
            Some(rest) if indent == 0 => (true, rest),
            None if indent == 2 => (false, text),
            _ => continue,
        };
        if starts_item {
            installers.push(HashMap::new());
        }
        if let (Some((key, value)), Some(current)) = (pair.split_once(':'), installers.last_mut()) {
            if !value.trim().is_empty() {
                current.insert(key.trim().to_string(), unquote(value));
            }
        }
    }

    let assets: Vec<Asset> = installers
        .iter()
        .filter_map(|installer| {
            let url = installer.get("InstallerUrl")?;
            let arch = match installer.get("Architecture").map(String::as_str) {
                Some(arch @ ("x64" | "x86" | "arm64")) => arch.to_string(),
                _ => String::new(),
            };
            let kind = installer.get("InstallerType").map(String::as_str).unwrap_or("exe");
            let ext = if matches!(kind, "msi" | "wix" | "burn") && url.to_lowercase().contains(".msi") { "msi" } else { "exe" };
            let suffix = if arch.is_empty() { String::new() } else { format!("-{arch}") };
            Some(Asset {
                name: name_from_url(url, &format!("{id}-{version}{suffix}.{ext}")),
                url: url.clone(),
                platform: Platform::Windows,
                arch,
                size: None,
                sha256: installer.get("InstallerSha256").filter(|s| s.len() == 64).map(|s| s.to_lowercase()),
            })
        })
        .collect();
    (!version.is_empty() && !assets.is_empty()).then(|| Release {
        version,
        published_at: String::new(),
        page: format!("https://github.com/microsoft/winget-pkgs/tree/master/{}", winget_path(id)),
        assets,
    })
}

/// 包标识 → 清单仓库里的目录：`Microsoft.VisualStudioCode` → `manifests/m/Microsoft/VisualStudioCode`。
fn winget_path(id: &str) -> String {
    let first = id.chars().next().map(|c| c.to_ascii_lowercase()).unwrap_or('_');
    format!("manifests/{first}/{}", id.replace('.', "/"))
}

/// 在目录里搜索。名字或标识完全一致的排最前，其次是开头一致的，再次是包含的。
pub fn search_candidates(all: &[Candidate], query: &str, limit: usize) -> Vec<Candidate> {
    let squash = |text: &str| -> String { text.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect() };
    let wanted = squash(query);
    if wanted.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<(u8, &Candidate)> = all
        .iter()
        .filter_map(|candidate| {
            let (name, token) = (squash(&candidate.name), squash(&candidate.reference));
            let rank = if name == wanted || token == wanted {
                0
            } else if name.starts_with(&wanted) || token.starts_with(&wanted) {
                1
            } else if name.contains(&wanted) || token.contains(&wanted) {
                2
            } else {
                return None;
            };
            Some((rank, candidate))
        })
        .collect();
    hits.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.name.len().cmp(&b.1.name.len())));
    hits.into_iter().take(limit).map(|(_, candidate)| candidate.clone()).collect()
}

/// 整份 Homebrew 目录（一个 JSON 数组）→ 精简后的候选列表。
pub fn candidates_from_catalog(catalog: &Value) -> Vec<Candidate> {
    let text = |value: &Value| value.as_str().unwrap_or("").to_string();
    catalog
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|cask| {
            let token = cask["token"].as_str()?;
            Some(Candidate {
                reference: token.to_string(),
                name: cask["name"][0].as_str().unwrap_or(token).to_string(),
                description: text(&cask["desc"]),
                homepage: text(&cask["homepage"]),
                version: cask["version"].as_str().unwrap_or("").split(',').next().unwrap_or("").to_string(),
            })
        })
        .collect()
}

// ── 联网 ────────────────────────────────────────────────────

#[cfg(not(feature = "network"))]
pub fn fetch_release(_kind: SourceKind, _reference: &str) -> Result<Release, SourceError> {
    Err(SourceError::Unreachable)
}

#[cfg(not(feature = "network"))]
pub fn search_homebrew(_cache_dir: &Path, _query: &str, _limit: usize) -> Result<Vec<Candidate>, SourceError> {
    Err(SourceError::Unreachable)
}

#[cfg(feature = "network")]
fn agent(response_timeout: u64) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_recv_response(Some(Duration::from_secs(response_timeout)))
        // 读数据卡住超过 60 秒就算失败（比如 20 MB 的 Homebrew 目录下到一半网断了），不一直挂着
        .timeout_per_read(Some(Duration::from_secs(60)))
        .user_agent(USER_AGENT)
        .proxy(ureq::Proxy::try_from_env())
        .build();
    ureq::Agent::new_with_config(config)
}

#[cfg(feature = "network")]
fn get(agent: &ureq::Agent, url: &str) -> Result<ureq::http::Response<ureq::Body>, SourceError> {
    match agent.get(url).header("Accept", "application/json").call() {
        Ok(response) => Ok(response),
        Err(ureq::Error::StatusCode(404)) => Err(SourceError::NotFound),
        Err(ureq::Error::StatusCode(403 | 429)) => Err(SourceError::RateLimited),
        Err(_) => Err(SourceError::Unreachable),
    }
}

#[cfg(feature = "network")]
fn get_json(agent: &ureq::Agent, url: &str) -> Result<Value, SourceError> {
    get(agent, url)?.body_mut().read_json().map_err(|_| SourceError::Unreachable)
}

/// 查一个软件在在线源上的最新版本。
#[cfg(feature = "network")]
pub fn fetch_release(kind: SourceKind, reference: &str) -> Result<Release, SourceError> {
    let reference = normalize_reference(kind, reference)?;
    let agent = agent(20);
    match kind {
        SourceKind::Homebrew => {
            let cask = get_json(&agent, &format!("https://formulae.brew.sh/api/cask/{reference}.json"))?;
            parse_homebrew(&cask).ok_or(SourceError::NotFound)
        }
        SourceKind::Github => {
            let release = get_json(&agent, &format!("https://api.github.com/repos/{reference}/releases/latest"))?;
            parse_github(&release).ok_or(SourceError::NotFound)
        }
        SourceKind::Winget => {
            let path = winget_path(&reference);
            let listing = get_json(&agent, &format!("https://api.github.com/repos/microsoft/winget-pkgs/contents/{path}"))?;
            // 目录下每个版本一个文件夹；不以数字开头的是子产品（比如 Insiders），不算版本
            let version = listing
                .as_array()
                .into_iter()
                .flatten()
                .filter(|entry| entry["type"] == "dir")
                .filter_map(|entry| entry["name"].as_str())
                .filter(|name| name.starts_with(|c: char| c.is_ascii_digit()))
                .max_by(|a, b| compare_versions(a, b))
                .ok_or(SourceError::NotFound)?;
            let raw = format!("https://raw.githubusercontent.com/microsoft/winget-pkgs/master/{path}/{version}/{reference}.installer.yaml");
            let yaml = get(&agent, &raw)?.body_mut().read_to_string().map_err(|_| SourceError::Unreachable)?;
            parse_winget(&reference, &yaml).ok_or(SourceError::NotFound)
        }
    }
}

/// 在 Homebrew 目录里搜索。目录第一次用到时下载，精简后存在 `cache_dir` 里。
#[cfg(feature = "network")]
pub fn search_homebrew(cache_dir: &Path, query: &str, limit: usize) -> Result<Vec<Candidate>, SourceError> {
    let cache = cache_dir.join(CATALOG_FILE);
    let fresh = std::fs::metadata(&cache)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.elapsed().ok())
        .is_some_and(|age| age < CATALOG_MAX_AGE);
    let cached = || std::fs::read(&cache).ok().and_then(|bytes| serde_json::from_slice::<Vec<Candidate>>(&bytes).ok());

    let candidates = match fresh.then(cached).flatten() {
        Some(list) => list,
        None => match download_catalog() {
            Ok(list) => {
                let _ = std::fs::create_dir_all(cache_dir);
                if let Ok(json) = serde_json::to_vec(&list) {
                    let _ = std::fs::write(&cache, json);
                }
                list
            }
            // 取不到新的就凑合用旧的
            Err(error) => cached().ok_or(error)?,
        },
    };
    Ok(search_candidates(&candidates, query, limit))
}

#[cfg(feature = "network")]
fn download_catalog() -> Result<Vec<Candidate>, SourceError> {
    let mut response = get(&agent(60), "https://formulae.brew.sh/api/cask.json")?;
    // 默认只肯读 10 MB，这份目录比它大
    let reader = response.body_mut().with_config().limit(96 * 1024 * 1024).reader();
    let catalog: Value = serde_json::from_reader(reader).map_err(|_| SourceError::Unreachable)?;
    let list = candidates_from_catalog(&catalog);
    if list.is_empty() { Err(SourceError::Unreachable) } else { Ok(list) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn references() {
        use SourceKind::*;
        assert_eq!(normalize_reference(Homebrew, " Visual-Studio-Code "), Ok("visual-studio-code".into()));
        assert_eq!(normalize_reference(Github, "https://github.com/obsidianmd/obsidian-releases/"), Ok("obsidianmd/obsidian-releases".into()));
        assert_eq!(normalize_reference(Github, "BurntSushi/ripgrep.git"), Ok("BurntSushi/ripgrep".into()));
        assert_eq!(normalize_reference(Winget, "Microsoft.VisualStudioCode"), Ok("Microsoft.VisualStudioCode".into()));
        assert_eq!(normalize_reference(Github, "just-a-name"), Err(SourceError::Invalid));
        assert_eq!(normalize_reference(Winget, "NoDot"), Err(SourceError::Invalid));
        assert_eq!(normalize_reference(Homebrew, "bad name/../x"), Err(SourceError::Invalid));
        assert_eq!(winget_path("Microsoft.VisualStudio.2022.Community"), "manifests/m/Microsoft/VisualStudio/2022/Community");
    }

    #[test]
    fn homebrew_cask() {
        let cask = json!({
            "token": "visual-studio-code",
            "version": "1.147.0",
            "homepage": "https://code.visualstudio.com/",
            "url": "https://update.code.visualstudio.com/1.147.0/darwin-arm64/stable",
            "sha256": "149d944feb599b076b48406b174e1c3cd1d510e9d673854777a20a6f4116a59a",
            "variations": {
                "arm64_big_sur": { "url": "https://update.code.visualstudio.com/1.147.0/darwin-arm64/stable" },
                "sequoia": { "url": "https://update.code.visualstudio.com/1.147.0/darwin/stable", "sha256": "A91677B96C1A73DE366F54CB4A497C8095459E07E95457FFAB985820F93FB298" }
            }
        });
        let release = parse_homebrew(&cask).unwrap();
        assert_eq!(release.version, "1.147.0");
        assert_eq!(release.assets.len(), 2);
        assert_eq!((release.assets[0].arch.as_str(), release.assets[0].name.as_str()), ("arm64", "visual-studio-code-1.147.0-arm64"));
        assert_eq!(release.assets[1].arch, "x64");
        assert_eq!(release.assets[1].sha256.as_deref(), Some("a91677b96c1a73de366f54cb4a497c8095459e07e95457ffab985820f93fb298"));

        // 只有一个地址、版本带构建号、不校验哈希的 cask
        let simple = json!({ "token": "obsidian", "version": "1.7.7,123", "url": "https://example.com/dl/Obsidian-1.7.7.dmg", "sha256": "no_check" });
        let release = parse_homebrew(&simple).unwrap();
        assert_eq!(release.version, "1.7.7");
        assert_eq!(release.assets.len(), 1);
        assert_eq!(release.assets[0].name, "Obsidian-1.7.7.dmg");
        assert_eq!(release.assets[0].sha256, None);
    }

    #[test]
    fn github_release() {
        let release = json!({
            "tag_name": "v1.15.1",
            "published_at": "2026-11-12T15:43:18Z",
            "html_url": "https://github.com/o/r/releases/tag/v1.15.1",
            "assets": [
                { "name": "Obsidian-1.15.1.exe", "size": 339406872u64, "digest": "sha256:218C7D30", "browser_download_url": "https://github.com/o/r/releases/download/v1.15.1/Obsidian-1.15.1.exe" },
                { "name": "Obsidian-1.15.1-arm64.AppImage", "size": 10, "digest": null, "browser_download_url": "https://x/a.AppImage" },
                { "name": "obsidian-1.15.1.asar.gz", "size": 10, "browser_download_url": "https://x/a.asar.gz" },
                { "name": "latest.yml", "size": 10, "browser_download_url": "https://x/latest.yml" },
                { "name": "Obsidian-1.15.1.exe.sig", "size": 10, "browser_download_url": "https://x/sig" }
            ]
        });
        let parsed = parse_github(&release).unwrap();
        assert_eq!(parsed.version, "1.15.1");
        let names: Vec<_> = parsed.assets.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Obsidian-1.15.1.exe", "Obsidian-1.15.1-arm64.AppImage"]);
        assert_eq!(parsed.assets[0].platform, Platform::Windows);
        assert_eq!(parsed.assets[0].sha256.as_deref(), Some("218c7d30"));
        assert_eq!((parsed.assets[1].platform, parsed.assets[1].arch.as_str()), (Platform::Linux, "arm64"));
    }

    #[test]
    fn winget_manifest() {
        let yaml = "# Created with komac\nPackageIdentifier: 7zip.7zip\nPackageVersion: '24.09'\nScope: machine\nFileExtensions:\n- '001'\n- 7z\nInstallers:\n- Architecture: x86\n  InstallerType: exe\n  InstallerUrl: https://example.com/files/7z2409.exe\n  InstallerSha256: E35E4374100B52E697E002859AEFDD5533BCBF4118E5D2210FAE6DE318947C41\n  InstallModes:\n  - interactive\n  - silent\n- Architecture: x64\n  InstallerType: wix\n  InstallerUrl: https://example.com/files/7z2409-x64.msi\n  InstallerSha256: 0000000000000000000000000000000000000000000000000000000000000000\n- Architecture: arm64\n  InstallerUrl: https://example.com/download?id=42\nManifestType: installer\nManifestVersion: 1.12.0\n";
        let release = parse_winget("7zip.7zip", yaml).unwrap();
        assert_eq!(release.version, "24.09");
        assert_eq!(release.assets.len(), 3);
        assert_eq!((release.assets[0].name.as_str(), release.assets[0].arch.as_str()), ("7z2409.exe", "x86"));
        assert_eq!(release.assets[0].sha256.as_deref(), Some("e35e4374100b52e697e002859aefdd5533bcbf4118e5d2210fae6de318947c41"));
        assert_eq!(release.assets[1].name, "7z2409-x64.msi");
        // 网址里看不出文件名：用标识、版本和架构拼一个
        assert_eq!(release.assets[2].name, "7zip.7zip-24.09-arm64.exe");
        assert!(parse_winget("x.y", "PackageVersion: 1.0\n").is_none());
    }

    #[test]
    fn catalog_search() {
        let catalog = json!([
            { "token": "visual-studio-code", "name": ["Microsoft Visual Studio Code", "VS Code"], "desc": "Open-source code editor", "homepage": "https://code.visualstudio.com/", "version": "1.147.0" },
            { "token": "visual-studio-code@insiders", "name": ["Microsoft Visual Studio Code - Insiders"], "desc": "", "homepage": "", "version": "1.148.0,abc" },
            { "token": "code-notes", "name": ["Code Notes"], "desc": "", "homepage": "", "version": "1.2.4" },
            { "token": "obsidian", "name": ["Obsidian"], "desc": "Knowledge base", "homepage": "", "version": "1.15.1" }
        ]);
        let all = candidates_from_catalog(&catalog);
        assert_eq!(all.len(), 4);
        assert_eq!(all[1].version, "1.148.0");
        let hits: Vec<_> = search_candidates(&all, "Visual Studio Code", 10).into_iter().map(|c| c.reference).collect();
        assert_eq!(hits, ["visual-studio-code", "visual-studio-code@insiders"]);
        assert_eq!(search_candidates(&all, "obsidian", 10)[0].reference, "obsidian");
        assert_eq!(search_candidates(&all, "code", 10).len(), 3);
        assert!(search_candidates(&all, "  ", 10).is_empty());
    }

    /// 需要联网，平时不跑：`cargo test -p shibapack-core -- --ignored online_sources`
    #[test]
    #[ignore]
    #[cfg(feature = "network")]
    fn online_sources() {
        let brew = fetch_release(SourceKind::Homebrew, "visual-studio-code").unwrap();
        eprintln!("homebrew → {} {:?}", brew.version, brew.assets.iter().map(|a| (&a.name, &a.arch)).collect::<Vec<_>>());
        assert!(!brew.version.is_empty() && !brew.assets.is_empty());

        let github = fetch_release(SourceKind::Github, "obsidianmd/obsidian-releases").unwrap();
        eprintln!("github → {} {:?}", github.version, github.assets.iter().map(|a| (&a.name, a.platform)).collect::<Vec<_>>());
        assert!(github.assets.iter().any(|a| a.platform == Platform::Windows && a.sha256.is_some()));

        let winget = fetch_release(SourceKind::Winget, "7zip.7zip").unwrap();
        eprintln!("winget → {} {:?}", winget.version, winget.assets.iter().map(|a| (&a.name, &a.arch)).collect::<Vec<_>>());
        assert!(compare_versions(&winget.version, "24.09").is_ge());
        assert!(winget.assets.iter().all(|a| a.platform == Platform::Windows && a.sha256.is_some()));

        assert_eq!(fetch_release(SourceKind::Homebrew, "zzqx-no-such-cask-7781"), Err(SourceError::NotFound));

        let cache = tempfile::tempdir().unwrap();
        let hits = search_homebrew(cache.path(), "obsidian", 5).unwrap();
        eprintln!("search → {:?}", hits.iter().map(|c| (&c.reference, &c.version)).collect::<Vec<_>>());
        assert_eq!(hits[0].reference, "obsidian");
        assert!(cache.path().join(CATALOG_FILE).is_file(), "目录应当被缓存下来");
    }
}
