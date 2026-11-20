//! 联网给软件找图标：官网图标、可选的 theSVG 品牌图标、GitHub 仓库所有者头像。

#![cfg_attr(not(feature = "network"), allow(dead_code, unused_imports))]

use crate::summary::Unreachable;
#[cfg(feature = "network")]
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

/// 取回的一张图：文件内容和扩展名。
#[derive(Debug, Clone, PartialEq)]
pub struct WebIcon {
    pub bytes: Vec<u8>,
    pub ext: &'static str,
}

/// 网站图标小于这个边长（像素）时，先试其他来源。
const SHARP: u32 = 48;
/// 一个网站最多下载几张候选图标。
const MAX_TRIES: usize = 4;
const MAX_IMAGE: u64 = 1 << 20;
const MAX_PAGE: u64 = 256 << 10;
#[cfg(feature = "network")]
const MAX_REGISTRY: u64 = 8 << 20;

/// 没有编进联网功能时，什么都取不到。
#[cfg(not(feature = "network"))]
pub fn fetch_web_icon(_homepage: &str, _github_repo: Option<&str>, _the_svg_name: Option<&str>) -> Result<Option<WebIcon>, Unreachable> {
    Err(Unreachable)
}

/// `the_svg_name` 是用于匹配 theSVG 清单的本地软件名称，传入 None 时跳过该来源。
/// `Ok(None)` 表示都问过了、确实没有；只要有请求没得到答复，就返回 `Unreachable`，下次再试。
#[cfg(feature = "network")]
pub fn fetch_web_icon(homepage: &str, github_repo: Option<&str>, the_svg_name: Option<&str>) -> Result<Option<WebIcon>, Unreachable> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .user_agent("ShibaPack/0.1 (desktop app; software icons)")
        .proxy(ureq::Proxy::try_from_env())
        .build();
    let net = Net { agent: ureq::Agent::new_with_config(config), failed: std::cell::Cell::new(false) };

    let homepage = normalize_homepage(homepage);
    let owner = github_repo.and_then(repo_owner).or_else(|| homepage.as_deref().and_then(github_owner)).map(str::to_string);
    // 主页就是 GitHub 时，网站图标是 GitHub 自己的，没有意义
    let site = homepage.filter(|url| !is_github(url));

    let mut blurry: Option<WebIcon> = None;
    if let Some(site) = site {
        for url in net.site_icon_urls(&site).into_iter().take(MAX_TRIES) {
            let Some(icon) = net.image(&url) else { continue };
            if pixel_size(&icon).is_none_or(|size| size >= SHARP) {
                return Ok(Some(icon));
            }
            blurry.get_or_insert(icon);
        }
    }
    if let Some(name) = the_svg_name {
        match the_svg_slug(&net, name) {
            Ok(Some(slug)) => {
                if let Some(icon) = net.image(&format!("https://thesvg.org/icons/{slug}/default.svg")).filter(|icon| icon.ext == "svg") {
                    return Ok(Some(icon));
                }
            }
            Ok(None) => {}
            Err(Unreachable) => net.failed.set(true),
        }
    }
    if let Some(owner) = owner {
        if let Some(icon) = net.image(&format!("https://github.com/{owner}.png?size=128")) {
            return Ok(Some(icon));
        }
    }
    match blurry {
        Some(icon) => Ok(Some(icon)),
        None if net.failed.get() => Err(Unreachable),
        None => Ok(None),
    }
}

#[cfg(feature = "network")]
#[derive(serde::Deserialize)]
struct TheSvgManifest {
    icons: Vec<TheSvgEntry>,
}

#[cfg(feature = "network")]
#[derive(serde::Deserialize)]
struct TheSvgEntry {
    slug: String,
    title: String,
    #[serde(default)]
    aliases: Vec<String>,
}

#[cfg(feature = "network")]
impl TheSvgManifest {
    /// 优先用 slug，避免同名徽章盖掉品牌图标。名称或别名有歧义时跳过。
    fn name_index(self) -> HashMap<String, Option<String>> {
        let mut slugs = HashMap::new();
        let mut names = HashMap::new();
        for TheSvgEntry { slug, title, aliases } in self.icons {
            if slug.is_empty() || !slug.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_') {
                continue;
            }
            Self::index_name(&mut slugs, &slug, &slug);
            for label in std::iter::once(&title).chain(&aliases) {
                Self::index_name(&mut names, label, &slug);
            }
        }
        names.extend(slugs);
        names
    }

    fn index_name(index: &mut HashMap<String, Option<String>>, label: &str, slug: &str) {
        let name = crate::identify::normalize_name(label);
        if name.is_empty() {
            return;
        }
        index.entry(name)
            .and_modify(|match_slug| {
                if match_slug.as_deref() != Some(slug) {
                    *match_slug = None;
                }
            })
            .or_insert_with(|| Some(slug.to_owned()));
    }
}

#[cfg(feature = "network")]
struct CachedTheSvgRegistry {
    names: Result<HashMap<String, Option<String>>, Unreachable>,
    expires_at: Instant,
}

#[cfg(feature = "network")]
fn the_svg_slug(net: &Net, name: &str) -> Result<Option<String>, Unreachable> {
    static REGISTRY: Mutex<Option<CachedTheSvgRegistry>> = Mutex::new(None);
    let name = crate::identify::normalize_name(name);
    if name.is_empty() {
        return Ok(None);
    }
    // 并行查找多个软件时共用一份清单。临时故障缓一会儿再重试。
    let mut cache = REGISTRY.lock().map_err(|_| Unreachable)?;
    if cache.as_ref().is_none_or(|cached| cached.expires_at <= Instant::now()) {
        let names = net.get("https://thesvg.org/api/registry.json")
            .and_then(|mut response| response.body_mut().with_config().limit(MAX_REGISTRY).read_json::<TheSvgManifest>().ok())
            .filter(|manifest| !manifest.icons.is_empty())
            .map(TheSvgManifest::name_index)
            .ok_or(Unreachable);
        let lifetime = Duration::from_secs(if names.is_ok() { 86_400 } else { 60 });
        *cache = Some(CachedTheSvgRegistry { names, expires_at: Instant::now() + lifetime });
    }
    let cached = cache.as_ref().expect("theSVG registry cache was initialized");
    let names = cached.names.as_ref().map_err(|_| Unreachable)?;
    Ok(names.get(&name).cloned().flatten())
}

#[cfg(feature = "network")]
struct Net {
    agent: ureq::Agent,
    failed: std::cell::Cell<bool>,
}

#[cfg(feature = "network")]
impl Net {
    /// 取一个地址。对方明确说没有（4xx）时返回 None；连不上、超时、服务器出错记作失败。
    fn get(&self, url: &str) -> Option<ureq::http::Response<ureq::Body>> {
        match self.agent.get(url).call() {
            Ok(response) => Some(response),
            Err(ureq::Error::StatusCode(code)) if (400..500).contains(&code) && code != 429 => None,
            Err(_) => {
                self.failed.set(true);
                None
            }
        }
    }

    fn image(&self, url: &str) -> Option<WebIcon> {
        let mut response = self.get(url)?;
        let bytes = response.body_mut().with_config().limit(MAX_IMAGE).read_to_vec().ok()?;
        let ext = sniff(&bytes)?;
        Some(WebIcon { bytes, ext })
    }

    /// 主页上声明的图标从大到小，最后是网站根目录的 favicon.ico。
    fn site_icon_urls(&self, homepage: &str) -> Vec<String> {
        use std::io::Read;
        use ureq::ResponseExt;
        let mut urls = Vec::new();
        let mut base = homepage.to_string();
        if let Some(mut response) = self.get(homepage) {
            // 跟着跳转之后的地址才是相对链接的基准
            base = response.get_uri().to_string();
            let mut html = Vec::new();
            if response.body_mut().as_reader().take(MAX_PAGE).read_to_end(&mut html).is_ok() {
                let mut links = icon_links(&String::from_utf8_lossy(&html));
                links.sort_by_key(|link| std::cmp::Reverse(link.1));
                urls.extend(links.into_iter().filter_map(|(href, _)| resolve(&base, &href)));
            }
        }
        if let Some(origin) = origin(&base) {
            urls.push(format!("{origin}/favicon.ico"));
        }
        let mut seen = std::collections::HashSet::new();
        urls.retain(|url| seen.insert(url.clone()));
        urls
    }
}

/// 补上协议。不是网址（空的、ftp、文件路径）时返回 None。
fn normalize_homepage(homepage: &str) -> Option<String> {
    let homepage = homepage.trim();
    if homepage.is_empty() || homepage.contains(char::is_whitespace) {
        return None;
    }
    let lower = homepage.to_ascii_lowercase();
    let url = if lower.starts_with("http://") || lower.starts_with("https://") {
        homepage.to_string()
    } else if !lower.contains("://") && host_of(&format!("https://{homepage}")).is_some_and(|host| host.contains('.')) {
        format!("https://{homepage}")
    } else {
        return None;
    };
    host_of(&url).is_some().then_some(url)
}

/// `https://example.com:8080/a?b` → `example.com:8080`（小写）。
fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host = rest[..end].rsplit('@').next()?.to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

/// `https://example.com/a/b` → `https://example.com`。
fn origin(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    (end > 0).then(|| format!("{scheme}://{}", &rest[..end]))
}

fn is_github(url: &str) -> bool {
    host_of(url).is_some_and(|host| host == "github.com" || host == "www.github.com")
}

/// GitHub 用户名只能有字母、数字和连字符。
fn valid_owner(owner: &str) -> bool {
    !owner.is_empty() && owner.len() <= 39 && owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// `owner/repo` → `owner`。
fn repo_owner(reference: &str) -> Option<&str> {
    let owner = reference.trim().split('/').next()?;
    valid_owner(owner).then_some(owner)
}

/// 主页是 `github.com/owner/...` 或 `owner.github.io` 时的仓库主人。
fn github_owner(url: &str) -> Option<&str> {
    let host = host_of(url)?;
    let rest = url.split_once("://")?.1;
    if host == "github.com" || host == "www.github.com" {
        let path = &rest[rest.find('/')? + 1..];
        let owner = path.split(['/', '?', '#']).next()?;
        const NOT_USERS: &[&str] = &["orgs", "sponsors", "topics", "features", "about", "login", "marketplace"];
        return (valid_owner(owner) && !NOT_USERS.contains(&owner.to_ascii_lowercase().as_str())).then_some(owner);
    }
    let sub = host.strip_suffix(".github.io")?;
    let owner = &rest[..sub.len()];
    valid_owner(owner).then_some(owner)
}

/// 网页里 `<link rel="icon">`、`<link rel="apple-touch-icon">` 的地址和大致边长。
/// Safari 的 mask-icon 是单色剪影，不要。
fn icon_links(html: &str) -> Vec<(String, u32)> {
    let lower = html.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(found) = lower[pos..].find("<link") {
        let start = pos + found;
        let end = lower[start..].find('>').map_or(lower.len(), |e| start + e);
        let tag = &html[start..end];
        pos = end;
        let Some(rel) = attr(tag, "rel") else { continue };
        let rel = rel.to_ascii_lowercase();
        let touch = rel.split_whitespace().any(|token| token.starts_with("apple-touch-icon"));
        if !touch && !rel.split_whitespace().any(|token| token == "icon") {
            continue;
        }
        let Some(href) = attr(tag, "href").map(|h| h.trim().replace("&amp;", "&")).filter(|h| !h.is_empty()) else { continue };
        let svg = attr(tag, "type").is_some_and(|t| t.contains("svg")) || href.to_ascii_lowercase().split(['?', '#']).next().unwrap_or("").ends_with(".svg");
        let declared = attr(tag, "sizes").and_then(|sizes| {
            sizes.split_whitespace().filter_map(|size| size.to_ascii_lowercase().split('x').next()?.parse::<u32>().ok()).max()
        });
        let size = match declared {
            Some(size) => size,
            None if svg => 512,
            None if touch => 180,
            None => 32,
        };
        out.push((href, size));
    }
    out
}

/// 标签里一个属性的值，支持双引号、单引号和不带引号的写法。
fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut from = 0;
    while let Some(found) = lower[from..].find(name) {
        let at = from + found;
        from = at + name.len();
        // 要是一个完整的属性名：前面是空白，后面跟着等号
        if at == 0 || !bytes[at - 1].is_ascii_whitespace() {
            continue;
        }
        let rest = lower[from..].trim_start();
        if !rest.starts_with('=') {
            continue;
        }
        let value_at = tag.len() - rest.len() + 1;
        let value = tag[value_at..].trim_start();
        return Some(match value.chars().next() {
            Some(quote @ ('"' | '\'')) => value[1..].split(quote).next().unwrap_or("").to_string(),
            _ => value.split(|c: char| c.is_whitespace() || c == '>').next().unwrap_or("").trim_end_matches('/').to_string(),
        });
    }
    None
}

/// 把网页里的链接换成完整地址。内嵌的 data: 图片不要。
fn resolve(base: &str, href: &str) -> Option<String> {
    let lower = href.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return Some(href.to_string());
    }
    // data:、javascript: 这类不是网址
    if let Some(colon) = lower.find(':') {
        if colon > 0 && lower[..colon].chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c)) {
            return None;
        }
    }
    if let Some(rest) = href.strip_prefix("//") {
        let scheme = base.split_once("://")?.0;
        return Some(format!("{scheme}://{rest}"));
    }
    let origin = origin(base)?;
    if href.starts_with('/') {
        return Some(format!("{origin}{href}"));
    }
    let path = &base[origin.len()..];
    let path = path.split(['?', '#']).next().unwrap_or("");
    let dir = path.rfind('/').map_or("/", |i| &path[..=i]);
    Some(format!("{origin}{dir}{href}"))
}

/// 从文件内容认出图片格式，返回扩展名。网页、文本之类返回 None。
pub fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("png");
    }
    if bytes.len() > 6 && bytes.starts_with(&[0, 0, 1, 0]) && bytes[4..6] != [0, 0] {
        return Some("ico");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("jpg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("gif");
    }
    if bytes.len() > 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(2048)]).to_ascii_lowercase();
    match (head.find("<svg"), head.find("<html")) {
        (Some(svg), html) if html.is_none_or(|html| svg < html) => Some("svg"),
        _ => None,
    }
}

/// 图片的边长（像素）。认不出的格式返回 None；矢量图当作足够大。
pub fn pixel_size(icon: &WebIcon) -> Option<u32> {
    let b = &icon.bytes;
    match icon.ext {
        "png" if b.len() >= 24 => Some(u32::from_be_bytes([b[16], b[17], b[18], b[19]])),
        "ico" if b.len() >= 6 => {
            let count = u16::from_le_bytes([b[4], b[5]]) as usize;
            (0..count)
                .filter_map(|i| b.get(6 + 16 * i).map(|&w| if w == 0 { 256 } else { u32::from(w) }))
                .max()
        }
        "gif" if b.len() >= 8 => Some(u32::from(u16::from_le_bytes([b[6], b[7]]))),
        "svg" => Some(512),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_icon_links() {
        let html = r##"<html><head>
            <link rel="stylesheet" href="/a.css">
            <LINK REL="shortcut icon" HREF="/favicon.ico">
            <link rel=icon type="image/png" sizes="16x16 32x32" href="/img/fav-32.png?v=2&amp;x=1" />
            <link rel="apple-touch-icon" href='touch.png'>
            <link rel="mask-icon" href="/mask.svg" color="#000">
            <link href="/logo.svg" rel="icon">
        </head></html>"##;
        let links = icon_links(html);
        assert_eq!(
            links,
            vec![
                ("/favicon.ico".to_string(), 32),
                ("/img/fav-32.png?v=2&x=1".to_string(), 32),
                ("touch.png".to_string(), 180),
                ("/logo.svg".to_string(), 512),
            ]
        );
    }

    #[test]
    fn reads_attributes_exactly() {
        let tag = r#"<link data-href="/wrong.png" href = "/right.png" rel="icon""#;
        assert_eq!(attr(tag, "href").as_deref(), Some("/right.png"));
        assert_eq!(attr("<link rel=icon href=/x.png/", "href").as_deref(), Some("/x.png"));
        assert_eq!(attr("<link rel=icon>", "href"), None);
    }

    #[test]
    fn resolves_relative_links() {
        let base = "https://example.com/app/index.html?x=1";
        assert_eq!(resolve(base, "https://cdn.example.com/i.png").as_deref(), Some("https://cdn.example.com/i.png"));
        assert_eq!(resolve(base, "//cdn.example.com/i.png").as_deref(), Some("https://cdn.example.com/i.png"));
        assert_eq!(resolve(base, "/i.png").as_deref(), Some("https://example.com/i.png"));
        assert_eq!(resolve(base, "img/i.png").as_deref(), Some("https://example.com/app/img/i.png"));
        assert_eq!(resolve("https://example.com", "i.png").as_deref(), Some("https://example.com/i.png"));
        assert_eq!(resolve(base, "data:image/png;base64,AAAA"), None);
        assert_eq!(resolve(base, "mailto:x"), None);
        assert_eq!(resolve(base, "img/a:b.png").as_deref(), Some("https://example.com/app/img/a:b.png"));
    }

    #[test]
    fn finds_github_owners() {
        assert_eq!(repo_owner("neovim/neovim"), Some("neovim"));
        assert_eq!(repo_owner("bad owner/x"), None);
        assert_eq!(github_owner("https://github.com/obsidianmd/obsidian-releases"), Some("obsidianmd"));
        assert_eq!(github_owner("https://www.github.com/sponsors/someone"), None);
        assert_eq!(github_owner("https://Owner-1.github.io/project/"), Some("Owner-1"));
        assert_eq!(github_owner("https://example.com/github.com/x"), None);
        assert!(is_github("https://github.com/a/b"));
        assert!(!is_github("https://a.github.io"));
    }

    #[test]
    fn normalizes_homepages() {
        assert_eq!(normalize_homepage(" https://obsidian.md/ ").as_deref(), Some("https://obsidian.md/"));
        assert_eq!(normalize_homepage("obsidian.md").as_deref(), Some("https://obsidian.md"));
        assert_eq!(normalize_homepage(""), None);
        assert_eq!(normalize_homepage("ftp://example.com"), None);
        assert_eq!(normalize_homepage("not a url"), None);
        assert_eq!(normalize_homepage("localhost"), None);
        assert_eq!(origin("https://a.com:8080/x?y").as_deref(), Some("https://a.com:8080"));
    }

    #[test]
    fn recognizes_images_and_sizes() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&[0, 0, 0, 180, 0, 0, 0, 180]);
        assert_eq!(sniff(&png), Some("png"));
        assert_eq!(pixel_size(&WebIcon { bytes: png, ext: "png" }), Some(180));

        // 两张图：16 和 256（宽度字节 0 表示 256）
        let mut ico = vec![0, 0, 1, 0, 2, 0];
        ico.extend_from_slice(&[16; 16]);
        ico.extend_from_slice(&[0; 16]);
        assert_eq!(sniff(&ico), Some("ico"));
        assert_eq!(pixel_size(&WebIcon { bytes: ico, ext: "ico" }), Some(256));

        assert_eq!(sniff(b"<?xml version=\"1.0\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\"/>"), Some("svg"));
        assert_eq!(sniff(b"<!doctype html><html><body><svg></svg></body></html>"), None);
        assert_eq!(sniff(b"Not Found"), None);
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpg"));
    }

    #[cfg(feature = "network")]
    #[test]
    fn the_svg_matches_full_names_and_unique_aliases() {
        let manifest: TheSvgManifest = serde_json::from_str(r#"{"icons":[
            {"slug":"visual-studio-code","title":"Visual Studio Code","aliases":["VS Code","Editor"]},
            {"slug":"vscode-badge","title":"Visual Studio Code"},
            {"slug":"another-editor","title":"Another Editor","aliases":["Editor"]},
            {"slug":"7zip","title":"7Zip"},
            {"slug":"../bad","title":"Bad"}
        ]}"#).unwrap();
        let index = manifest.name_index();
        let lookup = |name| index.get(&crate::identify::normalize_name(name)).and_then(Option::as_deref);
        assert_eq!(lookup("Visual Studio Code"), Some("visual-studio-code"));
        assert_eq!(lookup("VS Code"), Some("visual-studio-code"));
        assert_eq!(lookup("7-Zip"), Some("7zip"));
        assert_eq!(lookup("Visual Studio"), None);
        assert_eq!(lookup("Editor"), None);
        assert_eq!(lookup("Bad"), None);
    }

    #[cfg(feature = "network")]
    #[test]
    #[ignore = "需要网络"]
    fn the_svg_icons_from_the_network() {
        let icon = fetch_web_icon("", None, Some("Obsidian")).expect("theSVG 可访问").expect("应当有品牌图标");
        assert_eq!(icon.ext, "svg");
    }

    /// 真的去网上取一次。没有网络时跳过。
    #[cfg(feature = "network")]
    #[test]
    #[ignore = "需要网络"]
    fn icons_from_the_network() {
        let site = fetch_web_icon("https://obsidian.md", None, None).expect("网络").expect("应当有网站图标");
        assert!(pixel_size(&site).is_none_or(|size| size >= SHARP), "{:?}", pixel_size(&site));
        let avatar = fetch_web_icon("", Some("neovim/neovim"), None).expect("网络").expect("应当有头像");
        assert!(["png", "jpg"].contains(&avatar.ext));
    }
}
