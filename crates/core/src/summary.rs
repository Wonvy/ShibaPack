//! 联网取软件简介：维基百科（按界面语言先中文或先英文）→ Homebrew 的软件说明 → GitHub 仓库描述。
//! 简介不是界面语言时可以再联网翻译（Google 翻译，不通时用 MyMemory）。
//!
//! 取简介只把软件名发出去，翻译只把简介原文发出去。结果由调用方存进索引，之后离线也能显示。

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

#[cfg(not(feature = "network"))]
pub fn translate(_text: &str, _from: &str, _to: &str) -> Result<String, Unreachable> {
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
    let found = wikis
        .iter()
        .find_map(|wiki| from_wikipedia(&net, wiki, name))
        .or_else(|| from_homebrew(&net, name))
        .or_else(|| from_github(&net, name));
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
            return Some(Summary { text: shorten(extract), source: format!("Wikipedia ({wiki})"), fetched_at: now(), lang: wiki.to_string(), ..Summary::default() });
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
    let text = shorten(description);
    Some(Summary { lang: detect_language(&text).to_string(), text, source: "GitHub".to_string(), fetched_at: now(), ..Summary::default() })
}

/// Homebrew 上的一句话说明。按软件名猜它的标识（Visual Studio Code → visual-studio-code），
/// 先找图形界面的软件（cask），再找命令行工具（formula）。说明是英文的。
#[cfg(feature = "network")]
fn from_homebrew(net: &Net, name: &str) -> Option<Summary> {
    let token = homebrew_token(name)?;
    for kind in ["cask", "formula"] {
        let url = format!("https://formulae.brew.sh/api/{kind}/{token}.json");
        let Some(found) = net.get_json(&url, None) else { continue };
        let desc = found["desc"].as_str().map(str::trim).filter(|d| !d.is_empty());
        if let Some(desc) = desc {
            // 说明只是半句话（“Open-source code editor”），补个句号读起来完整些
            let mut text = shorten(desc);
            if !text.ends_with(['.', '!', '?', '…']) {
                text.push('.');
            }
            return Some(Summary { text, source: "Homebrew".to_string(), fetched_at: now(), lang: "en".into(), ..Summary::default() });
        }
    }
    None
}

/// 软件名按 Homebrew 的习惯变成标识：小写，空格换成 `-`，只留字母、数字和 `-`。认不出时（比如中文名）返回 None。
fn homebrew_token(name: &str) -> Option<String> {
    let token: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_whitespace() || c == '_' { '-' } else { c })
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '.' || *c == '+')
        .collect();
    let token = token.trim_matches('-').to_string();
    (token.len() >= 2 && token.chars().any(|c| c.is_ascii_alphabetic())).then_some(token)
}

/// 猜一段文字是什么语言：有假名是日文，汉字占了两成以上是中文，否则当英文。
pub fn detect_language(text: &str) -> &'static str {
    let (mut han, mut kana, mut letters) = (0usize, 0usize, 0usize);
    for c in text.chars() {
        match c {
            '\u{3040}'..='\u{30ff}' => kana += 1,
            '\u{4e00}'..='\u{9fff}' | '\u{3400}'..='\u{4dbf}' => han += 1,
            c if c.is_alphabetic() => letters += 1,
            _ => {}
        }
    }
    if kana > 0 {
        "ja"
    } else if han * 5 >= (han + letters).max(1) {
        "zh"
    } else {
        "en"
    }
}

/// 简介原文的语言。旧索引里没记，就按文字猜。
pub fn summary_language(summary: &Summary) -> String {
    if summary.lang.is_empty() { detect_language(&summary.text).to_string() } else { summary.lang.clone() }
}

/// 语言代码的主体部分：zh-CN → zh，en-US → en。
fn base_language(code: &str) -> String {
    code.split(['-', '_']).next().unwrap_or("").to_lowercase()
}

/// 简介要不要翻译成界面语言 `language`：有内容、和界面不是同一种语言、还没有译文。
pub fn needs_translation(summary: &Summary, language: &str) -> bool {
    !summary.text.trim().is_empty()
        && base_language(&summary_language(summary)) != base_language(language)
        && !summary.translations.contains_key(language)
}

/// 翻译服务认的语言代码：中文要分简繁（zh-CN / zh-TW），别的只要主体部分。
fn service_language(code: &str) -> String {
    let base = base_language(code);
    if base != "zh" {
        return base;
    }
    let lower = code.to_lowercase();
    if lower.contains("tw") || lower.contains("hk") || lower.contains("hant") { "zh-TW".into() } else { "zh-CN".into() }
}

/// 把 `text` 从 `from` 翻译成 `to`（都是 zh-CN、en 这样的语言代码）。先用 Google 翻译，不通再用 MyMemory。
#[cfg(feature = "network")]
pub fn translate(text: &str, from: &str, to: &str) -> Result<String, Unreachable> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .user_agent(USER_AGENT)
        .proxy(ureq::Proxy::try_from_env())
        .build();
    let net = Net { agent: ureq::Agent::new_with_config(config), failed: std::cell::Cell::new(false) };
    let (from, to) = (service_language(from), service_language(to));

    let google = format!(
        "https://translate.googleapis.com/translate_a/single?client=gtx&dt=t&sl={}&tl={}&q={}",
        encode(&from),
        encode(&to),
        encode(text)
    );
    // 返回的是按句子切开的数组：[[["译文", "原文", …], …], …]
    let from_google = net.get_json(&google, None).and_then(|found| {
        let pieces: String = found[0].as_array()?.iter().filter_map(|piece| piece[0].as_str()).collect();
        Some(pieces.trim().to_string()).filter(|t| !t.is_empty())
    });
    if let Some(done) = from_google {
        return Ok(done);
    }

    let mymemory = format!("https://api.mymemory.translated.net/get?q={}&langpair={}", encode(text), encode(&format!("{from}|{to}")));
    let from_mymemory = net.get_json(&mymemory, None).and_then(|found| {
        let ok = found["responseStatus"].as_u64() == Some(200) || found["responseStatus"].as_str() == Some("200");
        let done = found["responseData"]["translatedText"].as_str()?.trim().to_string();
        // 超了免费额度时它会把提示当译文返回
        (ok && !done.is_empty() && !done.starts_with("MYMEMORY WARNING")).then_some(done)
    });
    from_mymemory.ok_or(Unreachable)
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
    fn languages() {
        assert_eq!(detect_language("Git is a distributed version control system."), "en");
        assert_eq!(detect_language("微信是腾讯公司推出的即时通讯软件"), "zh");
        assert_eq!(detect_language("Visual Studio Code 是微软开发的跨平台源代码编辑器"), "zh");
        assert_eq!(detect_language("ブレンダーは3DCGソフトです"), "ja");
        let en = Summary { text: "A code editor.".into(), lang: "en".into(), ..Summary::default() };
        assert!(needs_translation(&en, "zh-CN"));
        assert!(!needs_translation(&en, "en-US"));
        let mut done = en.clone();
        done.translations.insert("zh-CN".into(), "一个代码编辑器。".into());
        assert!(!needs_translation(&done, "zh-CN"));
        // 旧索引里没记语言：按文字猜
        let old = Summary { text: "微信是即时通讯软件".into(), ..Summary::default() };
        assert!(!needs_translation(&old, "zh-CN") && needs_translation(&old, "en"));
        assert!(!needs_translation(&Summary::default(), "zh-CN"), "空简介不用翻");
        assert_eq!((service_language("zh-CN"), service_language("zh-TW"), service_language("en-US"), service_language("ja-JP")), ("zh-CN".into(), "zh-TW".into(), "en".into(), "ja".into()));
        assert_eq!(homebrew_token("Visual Studio Code").as_deref(), Some("visual-studio-code"));
        assert_eq!(homebrew_token("微信"), None);
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

    /// 需要联网，平时不跑：`cargo test -p shibapack-core -- --ignored translation`
    #[test]
    #[ignore]
    #[cfg(feature = "network")]
    fn translation_from_the_network() {
        let done = translate("Git is a distributed version control system.", "en", "zh-CN").unwrap();
        eprintln!("→ {done}");
        assert_eq!(detect_language(&done), "zh");
        // Homebrew 能补上维基百科上没有的软件
        let summary = fetch_summary("Rectangle", "zh-CN").unwrap().unwrap();
        eprintln!("Rectangle → {summary:?}");
    }
}
