//! 联网取软件简介：维基百科（按界面语言先中文或先英文）→ GitHub 仓库描述。
//!
//! 只把软件名发出去。取回来的简介由调用方存进索引，之后离线也能显示。

#![cfg_attr(not(feature = "network"), allow(dead_code, unused_imports))]

use crate::{identify::normalize_name, model::Summary, now};
use serde_json::Value;
use std::time::Duration;

const USER_AGENT: &str = "ShibaPack/0.1 (desktop app; software summaries)";
const MAX_CHARS: usize = 260;

/// 没有编进联网功能时，什么都取不到。
#[cfg(not(feature = "network"))]
pub fn fetch_summary(_name: &str, _language: &str) -> Result<Option<Summary>, Unreachable> {
    Err(Unreachable)
}

/// 网络不通，或者对方限流。这种情况不应当记成“没有简介”，下次还要再试。
#[derive(Debug, PartialEq)]
pub struct Unreachable;

/// 一次取简介过程中的网络状态：只要有一个请求没得到正常答复，就不能断定“查不到”。
#[cfg(feature = "network")]
struct Net {
    agent: ureq::Agent,
    failed: std::cell::Cell<bool>,
}

#[cfg(feature = "network")]
impl Net {
    fn get_json(&self, url: &str, accept_language: Option<&str>) -> Option<Value> {
        let mut request = self.agent.get(url).header("Accept", "application/json");
        if let Some(language) = accept_language {
            request = request.header("Accept-Language", language);
        }
        match request.call() {
            Ok(mut response) => response.body_mut().read_json().ok(),
            // 404 是明确的“没有这个词条”
            Err(ureq::Error::StatusCode(404)) => None,
            Err(_) => {
                self.failed.set(true);
                None
            }
        }
    }
}

/// 取一个软件的简介。`Ok(None)` 表示三个来源都查过了、确实没有合适的内容。
#[cfg(feature = "network")]
pub fn fetch_summary(name: &str, language: &str) -> Result<Option<Summary>, Unreachable> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(None);
    }
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .user_agent(USER_AGENT)
        .proxy(ureq::Proxy::try_from_env())
        .build();
    let net = Net { agent: ureq::Agent::new_with_config(config), failed: std::cell::Cell::new(false) };

    let wikis: &[&str] = if language.to_lowercase().starts_with("zh") { &["zh", "en"] } else { &["en"] };
    let found = wikis.iter().find_map(|wiki| from_wikipedia(&net, wiki, name)).or_else(|| from_github(&net, name));
    match found {
        Some(summary) => Ok(Some(summary)),
        None if net.failed.get() => Err(Unreachable),
        None => Ok(None),
    }
}

#[cfg(feature = "network")]
fn from_wikipedia(net: &Net, wiki: &str, name: &str) -> Option<Summary> {
    let base = format!("https://{wiki}.wikipedia.org");
    // 中文维基按这个头返回简体或繁体
    let accept = if wiki == "zh" { "zh-CN" } else { "en" };

    // 先按软件名直接找词条；找不到或者不是软件（比如 Blender 在英文里先是搅拌机），再搜一次
    let mut titles = vec![(name.to_string(), true)];
    let keyword = if wiki == "zh" { "软件" } else { "software" };
    let search = format!(
        "{base}/w/api.php?action=query&list=search&srlimit=3&format=json&srsearch={}",
        encode(&format!("{name} {keyword}"))
    );
    if let Some(found) = net.get_json(&search, None) {
        for hit in found["query"]["search"].as_array().into_iter().flatten() {
            if let Some(title) = hit["title"].as_str() {
                if !titles.iter().any(|(known, _)| known == title) {
                    titles.push((title.to_string(), false));
                }
            }
        }
    }

    for (title, direct) in titles {
        let url = format!("{base}/api/rest_v1/page/summary/{}", encode(&title.replace(' ', "_")));
        let Some(page) = net.get_json(&url, Some(accept)) else { continue };
        let extract = page["extract"].as_str().unwrap_or("").trim();
        let description = page["description"].as_str().unwrap_or("");
        let page_title = page["title"].as_str().unwrap_or(&title);
        // 直接命中的词条允许重定向（WeChat → 微信）；搜出来的词条标题必须和软件名对得上
        let on_topic = direct || title_matches(name, page_title);
        if page["type"] == "standard" && on_topic && !extract.is_empty() && looks_like_software(&format!("{extract} {description}")) {
            return Some(Summary { text: shorten(extract), source: format!("Wikipedia ({wiki})"), fetched_at: now() });
        }
    }
    None
}

#[cfg(feature = "network")]
fn from_github(net: &Net, name: &str) -> Option<Summary> {
    let url = format!("https://api.github.com/search/repositories?per_page=5&q={}", encode(&format!("{name} in:name")));
    let found = net.get_json(&url, None)?;
    let key = normalize_name(name);
    // 只认仓库名和软件名完全对得上的，免得张冠李戴
    let description = found["items"].as_array()?.iter().find_map(|repo| {
        let same = normalize_name(repo["name"].as_str().unwrap_or("")) == key;
        repo["description"].as_str().map(str::trim).filter(|text| same && !text.is_empty())
    })?;
    Some(Summary { text: shorten(description), source: "GitHub".to_string(), fetched_at: now() })
}

/// 词条标题里要完整地出现软件名的每个词："Blender (software)" 对得上 "Blender"，"GitHub" 对不上 "Git"。
fn title_matches(name: &str, title: &str) -> bool {
    let words = |text: &str| -> Vec<String> {
        text.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(String::from).collect()
    };
    let (wanted, have) = (words(name), words(title));
    !wanted.is_empty() && wanted.iter().all(|word| have.contains(word))
}

fn looks_like_software(text: &str) -> bool {
    // 英文按整词比，免得 "app" 命中 "appliance"
    const WORDS: &[&str] = &[
        "software", "application", "applications", "app", "program", "editor", "browser", "client", "utility",
        "tool", "toolkit", "player", "freeware", "framework", "compiler", "messaging", "runtime", "interpreter",
        "emulator", "ide", "archiver", "launcher",
    ];
    const PHRASES: &[&str] = &[
        "operating system", "open-source", "open source", "video game", "软件", "应用", "程序", "编辑器", "浏览器",
        "客户端", "工具", "播放器", "操作系统", "开源", "游戏", "即时通讯", "即时通信", "运行环境", "解释器",
        "軟體", "應用", "瀏覽器",
    ];
    let lower = text.to_lowercase();
    PHRASES.iter().any(|phrase| lower.contains(phrase))
        || lower.split(|c: char| !c.is_ascii_alphanumeric()).any(|word| WORDS.contains(&word))
}

/// 太长的简介截到最近的一句话结尾。
fn shorten(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= MAX_CHARS {
        return text;
    }
    let head: String = text.chars().take(MAX_CHARS).collect();
    match head.rfind(['。', '！', '？', '.', '!', '?']) {
        // 截得太短（比如第一句里的缩写点）就不按句子截了
        Some(end) if head[..end].chars().count() > MAX_CHARS / 3 => {
            let stop = end + head[end..].chars().next().map_or(1, char::len_utf8);
            head[..stop].to_string()
        }
        _ => format!("{}…", head.trim_end()),
    }
}

/// URL 里的百分号编码。
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(encode("Visual Studio Code"), "Visual%20Studio%20Code");
        assert_eq!(encode("微信"), "%E5%BE%AE%E4%BF%A1");
        assert!(title_matches("Blender", "Blender (software)"));
        assert!(title_matches("Visual Studio Code", "Visual Studio Code"));
        assert!(!title_matches("Git", "GitHub"));
        assert!(!title_matches("Visual Studio Code", "Visual Studio"));
        assert!(looks_like_software("Git is a distributed version control software system"));
        assert!(looks_like_software("微信是腾讯公司推出的即时通讯软件"));
        assert!(!looks_like_software("A blender is a kitchen and laboratory appliance used to mix food"));
    }

    #[test]
    fn shortening() {
        assert_eq!(shorten("Short  text.\nSecond line."), "Short text. Second line.");
        let long = format!("{} End of first. {}", "word ".repeat(30), "tail ".repeat(60));
        let short = shorten(&long);
        assert!(short.ends_with("End of first."), "{short}");
        let unbroken = "字".repeat(400);
        let cut = shorten(&unbroken);
        assert_eq!(cut.chars().count(), MAX_CHARS + 1);
        assert!(cut.ends_with('…'));
    }

    /// 需要联网，平时不跑：`cargo test -p shibapack-core -- --ignored summaries`
    #[test]
    #[ignore]
    #[cfg(feature = "network")]
    fn summaries_from_the_network() {
        for (name, language) in [("Git", "zh-CN"), ("Blender", "en-US"), ("Obsidian", "zh-CN"), ("7-Zip", "en-US")] {
            let summary = fetch_summary(name, language);
            eprintln!("{name} [{language}] → {summary:?}\n");
            assert!(matches!(summary, Ok(Some(_))), "{name} 应当能取到简介");
        }
        let blender = fetch_summary("Blender", "en-US").unwrap().unwrap();
        assert!(!blender.text.to_lowercase().contains("kitchen"), "Blender 应当是三维软件，不是搅拌机");
        assert_eq!(fetch_summary("zzqx-no-such-software-7781", "en-US"), Ok(None));
    }
}
