//! AI 模型：给软件写简介、给一批软件推荐分类。
//!
//! 只用各家都提供的 OpenAI 兼容接口（`/chat/completions`、`/models`），从这台电脑直接请求用户选的服务，
//! 不经过别的服务器。发出去的只有软件名、发布者、主页、安装包的文件名、已有的简介和仓库里的分类名。
//!
//! 服务地址和模型名在仓库的设置里（`settings.ai`）；API Key 由桌面外壳从系统钥匙串里取出来传进来。
//! 结果都只是建议：简介可以重新生成或换回联网取的，分类要用户在预览里确认后才改动。

use crate::{
    classify::{descendants, icon_for, tree_order, NewCategory, Suggestion},
    model::{AiSettings, Category, Library, Software, Summary},
    now,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock,
    },
    time::Duration,
};

const USER_AGENT: &str = "ShibaPack (desktop app)";
/// 一次请求里放多少个软件。太多了回答慢、容易漏；太少了请求次数多，新分类的名字也不容易统一。
const BATCH: usize = 30;
/// 给分类用的简介最多带多少字。
const ABOUT_CHARS: usize = 160;
/// 生成的简介最多留多少字。
const SUMMARY_CHARS: usize = 300;

/// 出错的原因。`code()` 是给界面的短代码，界面按它显示本地化的提示。
#[derive(Debug, Clone, PartialEq)]
pub enum AiError {
    /// 还没设置服务地址。
    NotConfigured,
    /// 地址不是 http(s) 开头。
    BadUrl,
    /// 不是本机的服务，这台电脑上又没有存 API Key。
    NoKey,
    /// 还没选模型。
    NoModel,
    Auth(String),
    Balance(String),
    RateLimited(String),
    ModelNotFound(String),
    /// 地址上没有这个接口（多半是少了或多了 `/v1`）。
    WrongPath(String),
    TooLong(String),
    Region(String),
    Server(u16, String),
    Network(String),
    Timeout,
    /// 回答的格式不对，或者是空的。
    BadAnswer(String),
    /// 模型说不认识这个软件。
    Unknown,
    Cancelled,
}

impl AiError {
    /// `代码` 或 `代码: 服务的原话`。
    pub fn code(&self) -> String {
        let (code, detail) = match self {
            AiError::NotConfigured => ("ai_not_configured", String::new()),
            AiError::BadUrl => ("ai_bad_url", String::new()),
            AiError::NoKey => ("ai_no_key", String::new()),
            AiError::NoModel => ("ai_no_model", String::new()),
            AiError::Auth(m) => ("ai_auth", m.clone()),
            AiError::Balance(m) => ("ai_balance", m.clone()),
            AiError::RateLimited(m) => ("ai_rate_limited", m.clone()),
            AiError::ModelNotFound(m) => ("ai_model_not_found", m.clone()),
            AiError::WrongPath(m) => ("ai_wrong_path", m.clone()),
            AiError::TooLong(m) => ("ai_too_long", m.clone()),
            AiError::Region(m) => ("ai_region", m.clone()),
            AiError::Server(status, m) => ("ai_server", format!("{status} {m}").trim().to_string()),
            AiError::Network(m) => ("ai_network", m.clone()),
            AiError::Timeout => ("ai_timeout", String::new()),
            AiError::BadAnswer(m) => ("ai_bad_answer", m.clone()),
            AiError::Unknown => ("ai_unknown", String::new()),
            AiError::Cancelled => ("cancelled", String::new()),
        };
        if detail.is_empty() {
            code.to_string()
        } else {
            format!("{code}: {detail}")
        }
    }
}

/// 服务在这台电脑上（Ollama、LM Studio）：不需要密钥，内容不出电脑，也不走代理。
pub fn is_local(base_url: &str) -> bool {
    let rest = base_url.trim().split_once("://").map_or(base_url.trim(), |(_, rest)| rest);
    let host = match rest.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(""),
        None => rest.split(['/', ':']).next().unwrap_or(""),
    }
    .to_ascii_lowercase();
    host == "localhost" || host == "127.0.0.1" || host == "::1" || host.ends_with(".localhost")
}

/// 钥匙串里按什么存 API Key：同一个地址的服务共用一把，换了服务再换回来不用重填。
pub fn key_account(base_url: &str) -> String {
    base_url.trim().trim_end_matches('/').to_ascii_lowercase()
}

/// 这次运行里发现不认可选参数（温度、JSON 输出、关掉思考）的“地址|模型”。之后直接不带，省一次失败的请求。
fn plain_models() -> &'static Mutex<HashSet<String>> {
    static PLAIN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    PLAIN.get_or_init(|| Mutex::new(HashSet::new()))
}

/// 一个配置好的 AI 服务。
pub struct Service {
    base: String,
    model: String,
    preset: String,
    key: Option<String>,
    agent: ureq::Agent,
}

impl Service {
    pub fn new(settings: &AiSettings, key: Option<&str>) -> Result<Service, AiError> {
        let base = settings.base_url.trim().trim_end_matches('/').to_string();
        if base.is_empty() {
            return Err(AiError::NotConfigured);
        }
        let lower = base.to_ascii_lowercase();
        if !(lower.starts_with("http://") || lower.starts_with("https://")) {
            return Err(AiError::BadUrl);
        }
        let local = is_local(&base);
        let key = key.map(str::trim).filter(|k| !k.is_empty()).map(str::to_string);
        if key.is_none() && !local {
            return Err(AiError::NoKey);
        }
        let config = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(15)))
            // 一批软件的分类，慢一点的模型要写上一两分钟
            .timeout_global(Some(Duration::from_secs(180)))
            .http_status_as_error(false)
            .user_agent(USER_AGENT)
            .proxy(if local { None } else { ureq::Proxy::try_from_env() })
            .build();
        Ok(Service {
            base,
            model: settings.model.trim().to_string(),
            preset: settings.preset.trim().to_string(),
            key,
            agent: ureq::Agent::new_with_config(config),
        })
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    fn authorized<B>(&self, request: ureq::RequestBuilder<B>) -> ureq::RequestBuilder<B> {
        match &self.key {
            Some(key) => request.header("Authorization", format!("Bearer {key}")),
            None => request,
        }
    }

    /// 服务上可用的模型，按名字排好。
    pub fn models(&self) -> Result<Vec<String>, AiError> {
        let request = self.authorized(self.agent.get(format!("{}/models", self.base)));
        let body = finish(request.call())?;
        let value: Value = serde_json::from_str(&body).map_err(|_| AiError::BadAnswer(clip(&body, 200)))?;
        let list = value.get("data").or_else(|| value.get("models")).and_then(Value::as_array).ok_or_else(|| AiError::BadAnswer(clip(&body, 200)))?;
        let mut names: Vec<String> = list
            .iter()
            .filter_map(|item| item.get("id").or_else(|| item.get("name")).and_then(Value::as_str))
            // Gemini 的模型名带着 `models/` 前缀
            .map(|name| name.trim_start_matches("models/").to_string())
            .filter(|name| !name.is_empty())
            .collect();
        names.sort_by_key(|name| name.to_lowercase());
        names.dedup();
        Ok(names)
    }

    /// 发一次对话，返回回答的正文（去掉开头的思考过程）。
    ///
    /// 默认带上温度、`json`（要求输出 JSON）和让模型别思考的参数。服务不认其中某个（回 400）时，
    /// 全去掉再试一次，并记住这个模型，之后都不带。不设输出上限：会思考的模型的思考也算在上限里，
    /// 设了容易把回答截断。
    pub fn chat(&self, system: &str, user: &str, json_mode: bool, temperature: f64) -> Result<String, AiError> {
        if self.model.is_empty() {
            return Err(AiError::NoModel);
        }
        let url = format!("{}/chat/completions", self.base);
        let memo = format!("{}|{}", key_account(&self.base), self.model);
        let mut extras = !plain_models().lock().map(|plain| plain.contains(&memo)).unwrap_or(false);
        loop {
            let mut body = json!({
                "model": self.model,
                "messages": [{ "role": "system", "content": system }, { "role": "user", "content": user }],
                "stream": false,
            });
            if extras {
                body["temperature"] = json!(temperature);
                if json_mode {
                    body["response_format"] = json!({ "type": "json_object" });
                }
                if let Some((name, value)) = quiet_param(&self.preset, &self.base) {
                    body[name] = value;
                }
            }
            // 紧凑的 JSON：`send_json` 会排版缩进，白白多出几成字节
            let request = self.authorized(self.agent.post(&url)).header("Content-Type", "application/json");
            match finish(request.send(body.to_string())) {
                Err(AiError::Server(400 | 422, _)) if extras => {
                    extras = false;
                    if let Ok(mut plain) = plain_models().lock() {
                        plain.insert(memo.clone());
                    }
                }
                Err(error) => return Err(error),
                Ok(text) => return parse_reply(&text),
            }
        }
    }
}

/// 让模型别先写一段思考的参数（加在请求体最外层）。写简介、分类都是短任务，思考只会更慢更贵。
/// 按预设或地址认服务，认不出的不发。各家的参数名不一样（据各家文档，2026 年）。
fn quiet_param(preset: &str, base: &str) -> Option<(&'static str, Value)> {
    let url = base.to_ascii_lowercase();
    let at = |host: &str| url.contains(host);
    if preset == "deepseek" || at("api.deepseek.com") || preset == "zhipu" || at("bigmodel.cn") || at("api.z.ai") || preset == "moonshot" || at("moonshot.cn") || at("moonshot.ai") {
        Some(("thinking", json!({ "type": "disabled" })))
    } else if matches!(preset, "qwen" | "siliconflow") || at("dashscope") || at("siliconflow") {
        Some(("enable_thinking", json!(false)))
    } else if matches!(preset, "gemini" | "ollama") || at("generativelanguage.googleapis.com") || at(":11434") {
        Some(("reasoning_effort", json!("none")))
    } else if preset == "openrouter" || at("openrouter.ai") {
        Some(("reasoning", json!({ "effort": "none" })))
    } else {
        None
    }
}

/// 读完一次请求的回答：2xx 交出正文，别的按状态码和服务的原话归类。
fn finish(result: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Result<String, AiError> {
    let mut response = result.map_err(network_error)?;
    let status = response.status().as_u16();
    let body = response.body_mut().read_to_string().map_err(network_error)?;
    if (200..300).contains(&status) {
        Ok(body)
    } else {
        Err(failure(status, &body))
    }
}

fn network_error(error: ureq::Error) -> AiError {
    match error {
        ureq::Error::Timeout(_) => AiError::Timeout,
        other => AiError::Network(other.to_string()),
    }
}

/// 把服务返回的错误归类。各家格式不一，按状态码和关键词判断。
fn failure(status: u16, body: &str) -> AiError {
    let message = server_message(body);
    let lower = message.to_lowercase();
    let says = |words: &[&str]| words.iter().any(|word| lower.contains(word));
    if says(&["unsupported_country", "location is not supported", "not available in your", "region is not supported"]) {
        return AiError::Region(message);
    }
    if status == 402 || says(&["insufficient", "balance", "quota", "billing", "arrearage", "余额", "欠费"]) {
        return AiError::Balance(message);
    }
    match status {
        401 | 403 => AiError::Auth(message),
        404 if says(&["model"]) => AiError::ModelNotFound(message),
        404 | 405 => AiError::WrongPath(message),
        429 => AiError::RateLimited(message),
        400 | 413 if says(&["context length", "context_length", "too long", "maximum context", "too many tokens"]) => AiError::TooLong(message),
        400 if says(&["model not exist", "model_not_found", "does not exist", "unknown model", "invalid model"]) => AiError::ModelNotFound(message),
        _ => AiError::Server(status, message),
    }
}

/// 错误回答里给人看的那句话。
fn server_message(body: &str) -> String {
    let value: Option<Value> = serde_json::from_str(body).ok();
    let value = value.map(|v| match v {
        Value::Array(list) => list.into_iter().next().unwrap_or(Value::Null),
        other => other,
    });
    let message = value.as_ref().and_then(|v| {
        [v.pointer("/error/message"), v.get("message"), v.get("error"), v.get("detail")]
            .into_iter()
            .flatten()
            .find_map(Value::as_str)
            .map(str::to_string)
    });
    clip(message.as_deref().unwrap_or(body).trim(), 300)
}

/// 读一次对话的回答。
fn parse_reply(body: &str) -> Result<String, AiError> {
    let value: Value = serde_json::from_str(body).map_err(|_| AiError::BadAnswer(clip(body, 200)))?;
    let text = match value.pointer("/choices/0/message/content") {
        Some(Value::String(text)) => text.clone(),
        // 少数服务把正文分成几段
        Some(Value::Array(parts)) => parts.iter().filter_map(|p| p.get("text").and_then(Value::as_str)).collect(),
        _ => return Err(AiError::BadAnswer(server_message(body))),
    };
    let text = strip_thinking(&text);
    let text = text.trim();
    if text.is_empty() {
        return Err(AiError::BadAnswer(String::new()));
    }
    Ok(text.to_string())
}

/// 有的模型把思考过程写在正文最前面，用 `<think>…</think>` 包着。这部分不是回答。
fn strip_thinking(text: &str) -> String {
    match text.trim_start().strip_prefix("<think>") {
        Some(rest) => rest.find("</think>").map_or(String::new(), |end| rest[end + "</think>".len()..].to_string()),
        None => text.to_string(),
    }
}

/// 最多留 `max` 个字。
fn clip(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((at, _)) => format!("{}…", text[..at].trim_end()),
        None => text.to_string(),
    }
}

// ── 提示词 ───────────────────────────────────────────────────

/// 界面语言在提示词里怎么称呼。
fn language_name(code: &str) -> String {
    let lower = code.to_lowercase();
    let base = lower.split(['-', '_']).next().unwrap_or("");
    let name = match base {
        "zh" if ["tw", "hk", "mo", "hant"].iter().any(|r| lower.contains(r)) => "Traditional Chinese",
        "zh" => "Simplified Chinese",
        "en" => "English",
        "ja" => "Japanese",
        "ko" => "Korean",
        "de" => "German",
        "fr" => "French",
        "es" => "Spanish",
        "pt" => "Portuguese",
        "it" => "Italian",
        "ru" => "Russian",
        _ => return format!("the language with code \"{code}\""),
    };
    name.to_string()
}

/// 用汉字、假名、谚文写的语言：长度按字数算，不按词数。
fn compact(code: &str) -> bool {
    matches!(code.to_lowercase().split(['-', '_']).next(), Some("zh" | "ja" | "ko"))
}

fn base_language(code: &str) -> String {
    code.split(['-', '_']).next().unwrap_or("").to_lowercase()
}

fn describe_prompt(language: &str) -> String {
    let lang = language_name(language);
    let length = if compact(language) { "at most 80 characters" } else { "at most 40 words" };
    format!(
        "You write entries for a software catalog. Describe the given application in {lang}: one or two plain sentences, {length}, \
saying what it is and what people use it for. State only facts you are confident about. No marketing tone, no version numbers, \
no prices, no Markdown, no quotation marks, no heading. If you do not recognize the application, reply with exactly UNKNOWN."
    )
}

fn classify_prompt(language: &str) -> String {
    let lang = language_name(language);
    let examples = match base_language(language).as_str() {
        "zh" => " such as 开发工具, 办公效率, 影音媒体, 网络工具, 系统工具",
        "en" => " such as Development, Productivity, Media, Internet, Utilities",
        _ => "",
    };
    let (label, reason) = if compact(language) { ("2 to 4 characters", "at most 10 characters") } else { ("1 or 2 words", "at most 5 words") };
    format!(
        "You organize a software library into categories.\n\
For each application, choose the best category from the given list, preferring the most specific subcategory that fits. \
If the application's current category already fits, choose it again.\n\
Only when no listed category fits, set \"new\" to the name of a new top-level category: a short, general shelf label in {lang} \
({label}){examples}. Give similar applications the same new name, and reuse a name from \"New categories proposed so far\" when one fits.\n\
If you cannot tell what an application is, set both \"category\" and \"new\" to null.\n\
\"reason\" says what the application is, in {lang}, {reason}.\n\
Answer with JSON only, in exactly this shape, including every application once:\n\
{{\"items\":[{{\"id\":\"a1\",\"category\":\"c2\",\"new\":null,\"reason\":\"...\"}}]}}"
    )
}

// ── 写简介 ───────────────────────────────────────────────────

/// 给一个软件写简介。`name` 是简介存放用的名字（仓库里的软件名，或已安装软件去掉版本号后的名字）；
/// 仓库里有同名软件时，带上它的发布者、主页和安装包文件名，否则用 `publisher`。
pub fn describe(service: &Service, lib: &Library, name: &str, publisher: &str) -> Result<Summary, AiError> {
    describe_with(&mut |system, user| service.chat(system, user, false, 0.3), service.model(), lib, name, publisher)
}

fn describe_with(
    chat: &mut dyn FnMut(&str, &str) -> Result<String, AiError>,
    model: &str,
    lib: &Library,
    name: &str,
    publisher: &str,
) -> Result<Summary, AiError> {
    let language = &lib.settings.language;
    let software = lib.software.iter().find(|s| s.name == name);
    let mut facts = format!("Name: {name}\n");
    let publisher = software.map(|s| s.publisher.trim()).filter(|p| !p.is_empty()).unwrap_or(publisher.trim());
    if !publisher.is_empty() {
        facts += &format!("Publisher: {publisher}\n");
    }
    if let Some(homepage) = software.map(|s| s.homepage.trim()).filter(|h| !h.is_empty()) {
        facts += &format!("Homepage: {homepage}\n");
    }
    let files = software.map(file_names).unwrap_or_default();
    if !files.is_empty() {
        facts += &format!("Installer files: {}\n", files.join(", "));
    }
    // 联网取到的简介（可能是别的语言，可能不完整）：给模型参考，免得认错
    if let Some(existing) = lib.summaries.get(name).filter(|s| !s.text.trim().is_empty() && !s.source.starts_with("AI")) {
        facts += &format!("Existing description (may be in another language): {}\n", clip(existing.text.trim(), 400));
    }
    let answer = chat(&describe_prompt(language), &facts)?;
    let text = clean_description(&answer)?;
    Ok(Summary {
        text,
        source: format!("AI · {model}"),
        fetched_at: now(),
        lang: base_language(language),
        translations: Default::default(),
    })
}

/// 安装包的文件名，最多 6 个，不重复。
fn file_names(software: &Software) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for file in software.versions.iter().flat_map(|v| &v.files) {
        if names.len() >= 6 {
            break;
        }
        if !names.contains(&file.name) {
            names.push(file.name.clone());
        }
    }
    names
}

/// 整理模型写的简介：去掉引号、标题、换行。模型说不认识时返回 `Unknown`。
fn clean_description(answer: &str) -> Result<String, AiError> {
    let mut text = answer.trim();
    for prefix in ["简介：", "简介:", "描述：", "Description:", "description:"] {
        text = text.strip_prefix(prefix).unwrap_or(text).trim_start();
    }
    let text = text.trim_matches(|c: char| matches!(c, '"' | '“' | '”' | '「' | '」' | '\'' | '`') || c.is_whitespace());
    if text.is_empty() || text.to_ascii_uppercase().starts_with("UNKNOWN") {
        return Err(AiError::Unknown);
    }
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    Ok(clip(&joined, SUMMARY_CHARS))
}

// ── 智能分类 ─────────────────────────────────────────────────

/// 模型对一个软件的回答。
#[derive(Debug, Default, PartialEq)]
struct Answer {
    category: Option<String>,
    new: Option<String>,
    reason: String,
}

/// 用 AI 给一批软件推荐分类。只读，不改动任何东西；`progress(已完成, 总数)` 在每批回来后调用。
pub fn suggest_categories(
    service: &Service,
    lib: &Library,
    ids: &[String],
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<Vec<Suggestion>, AiError> {
    suggest_with(&mut |system, user| service.chat(system, user, true, 0.1), lib, ids, cancel, progress)
}

fn suggest_with(
    chat: &mut dyn FnMut(&str, &str) -> Result<String, AiError>,
    lib: &Library,
    ids: &[String],
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<Vec<Suggestion>, AiError> {
    let language = &lib.settings.language;
    // 分类按树的顺序编成 c1、c2……：省字数，也免得模型抄错长 id
    let mut ordered = Vec::new();
    tree_order(&lib.categories, None, 0, &mut ordered);
    let mut lines = Vec::new();
    let mut short_ids: HashMap<String, String> = HashMap::new();
    let mut by_id: HashMap<&str, String> = HashMap::new();
    let mut path: Vec<&str> = Vec::new();
    for (i, (category, depth)) in ordered.iter().enumerate() {
        path.truncate(*depth);
        path.push(category.name.trim());
        let short = format!("c{}", i + 1);
        lines.push(format!("{short}: {}", path.join(" / ")));
        by_id.insert(category.id.as_str(), short.clone());
        short_ids.insert(short, category.id.clone());
    }
    let categories = if lines.is_empty() { "(none yet)".to_string() } else { lines.join("\n") };

    let software: Vec<&Software> = ids.iter().filter_map(|id| lib.software.iter().find(|s| &s.id == id)).collect();
    let total = software.len();
    let mut answers: HashMap<&str, Answer> = HashMap::new();
    let mut proposed: Vec<String> = Vec::new();
    progress(0, total);
    for (batch, chunk) in software.chunks(BATCH).enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(AiError::Cancelled);
        }
        let offset = batch * BATCH;
        let apps: Vec<String> = chunk
            .iter()
            .enumerate()
            .map(|(i, s)| app_line(lib, s, offset + i + 1, s.category_id.as_deref().and_then(|c| by_id.get(c)).map(String::as_str)))
            .collect();
        let mut user = format!("Categories (id: path):\n{categories}\n\n");
        if !proposed.is_empty() {
            user += &format!("New categories proposed so far: {}\n\n", proposed.join(", "));
        }
        user += &format!("Applications (one JSON object per line):\n{}", apps.join("\n"));
        let answer = chat(&classify_prompt(language), &user)?;
        let parsed = parse_classes(&answer);
        if parsed.is_empty() {
            return Err(AiError::BadAnswer(clip(answer.trim(), 200)));
        }
        for (short, item) in parsed {
            let Some(index) = short.strip_prefix('a').and_then(|n| n.parse::<usize>().ok()) else { continue };
            let Some(software) = index.checked_sub(offset + 1).and_then(|i| chunk.get(i)) else { continue };
            if let Some(name) = &item.new {
                if !proposed.iter().any(|p| p.eq_ignore_ascii_case(name)) {
                    proposed.push(name.clone());
                }
            }
            answers.insert(software.id.as_str(), item);
        }
        progress((offset + chunk.len()).min(total), total);
    }

    Ok(software
        .iter()
        .map(|software| {
            let mut suggestion = Suggestion {
                software_id: software.id.clone(),
                category_id: None,
                new_category: None,
                unchanged: false,
                known: false,
                evidence: Vec::new(),
                reason: String::new(),
            };
            let Some(answer) = answers.get(software.id.as_str()) else { return suggestion };
            suggestion.reason = answer.reason.clone();
            // 说要新建的分类其实已经有了（同名），就用已有的
            let existing = |name: &str| {
                let key = name.trim().to_lowercase();
                lib.categories.iter().find(|c| c.name.trim().to_lowercase() == key).map(|c| c.id.clone())
            };
            let target = answer.category.as_ref().and_then(|c| short_ids.get(c).cloned()).or_else(|| answer.new.as_deref().and_then(existing));
            match target {
                Some(id) => suggestion.category_id = Some(keep_if_inside(&lib.categories, software, id)),
                None => {
                    suggestion.new_category =
                        answer.new.as_ref().map(|name| NewCategory { name: name.clone(), icon: icon_for(name).to_string() });
                }
            }
            suggestion.unchanged = suggestion.category_id.is_some() && suggestion.category_id == software.category_id;
            suggestion
        })
        .collect())
}

/// 软件已经在推荐的分类或它的某个下级里：保持原样，不往上挪。
fn keep_if_inside(categories: &[Category], software: &Software, target: String) -> String {
    let Some(current) = software.category_id.as_deref() else { return target };
    let mut children = Vec::new();
    descendants(categories, &target, &mut children, 0);
    if current == target || children.iter().any(|c| c.id == current) {
        current.to_string()
    } else {
        target
    }
}

/// 发给模型的一个软件：一行 JSON。有简介就带简介，没有就带几个安装包的文件名帮它认。
fn app_line(lib: &Library, software: &Software, index: usize, current: Option<&str>) -> String {
    let mut item = json!({ "id": format!("a{index}"), "name": software.name.trim() });
    if !software.publisher.trim().is_empty() {
        item["publisher"] = json!(software.publisher.trim());
    }
    if let Some(current) = current {
        item["current"] = json!(current);
    }
    let summary = lib.summaries.get(&software.name).map(|s| s.translations.get(&lib.settings.language).unwrap_or(&s.text).trim()).unwrap_or("");
    if summary.is_empty() {
        let files: Vec<String> = file_names(software).into_iter().take(2).collect();
        if !files.is_empty() {
            item["files"] = json!(files);
        }
    } else {
        item["about"] = json!(clip(summary, ABOUT_CHARS));
    }
    item.to_string()
}

/// 读模型的分类回答。容忍代码块包裹、前后多余的话、顶层直接是数组。返回 (软件编号, 回答)。
fn parse_classes(answer: &str) -> Vec<(String, Answer)> {
    let start = answer.find(['{', '[']);
    let end = answer.rfind(['}', ']']);
    let Some(value) = start.zip(end).filter(|(s, e)| s < e).and_then(|(s, e)| serde_json::from_str::<Value>(&answer[s..=e]).ok()) else {
        return Vec::new();
    };
    let items = match &value {
        Value::Array(items) => items,
        other => match ["items", "results", "apps"].iter().find_map(|k| other.get(*k).and_then(Value::as_array)) {
            Some(items) => items,
            None => return Vec::new(),
        },
    };
    let text = |item: &Value, key: &str| {
        item.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("null") && !s.eq_ignore_ascii_case("none"))
            .map(str::to_string)
    };
    items
        .iter()
        .filter_map(|item| {
            let id = text(item, "id")?;
            Some((
                id,
                Answer {
                    category: text(item, "category"),
                    new: text(item, "new").map(|name| clip(&name, 24)),
                    reason: text(item, "reason").map(|r| clip(&r, 40)).unwrap_or_default(),
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PackageFile, Platform, Storage, Version};
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;

    fn software(id: &str, name: &str, category: Option<&str>) -> Software {
        Software {
            id: id.into(),
            name: name.into(),
            publisher: String::new(),
            homepage: String::new(),
            note: String::new(),
            category_id: category.map(String::from),
            starred: false,
            icon: None,
            default_version_id: None,
            added_at: String::new(),
            versions: Vec::new(),
            subscription: None,
        }
    }

    fn category(id: &str, name: &str, parent: Option<&str>) -> Category {
        Category { id: id.into(), name: name.into(), parent_id: parent.map(String::from), color: None, icon: None }
    }

    #[test]
    fn local_services_are_recognized() {
        assert!(is_local("http://localhost:11434/v1"));
        assert!(is_local("http://127.0.0.1:1234/v1/"));
        assert!(is_local("http://[::1]:8080"));
        assert!(!is_local("https://api.deepseek.com"));
        assert!(!is_local("https://localhost.example.com/v1"));
        assert_eq!(key_account(" https://API.deepseek.com/ "), "https://api.deepseek.com");
    }

    #[test]
    fn errors_are_classified() {
        assert!(matches!(failure(401, r#"{"error":{"message":"Invalid API key"}}"#), AiError::Auth(m) if m == "Invalid API key"));
        assert!(matches!(failure(402, "{}"), AiError::Balance(_)));
        assert!(matches!(failure(400, r#"{"error":{"message":"Insufficient Balance"}}"#), AiError::Balance(_)));
        assert!(matches!(failure(404, r#"{"error":{"message":"The model `x` does not exist"}}"#), AiError::ModelNotFound(_)));
        assert!(matches!(failure(404, "Not Found"), AiError::WrongPath(_)));
        assert!(matches!(failure(429, "slow down"), AiError::RateLimited(_)));
        assert!(matches!(failure(400, r#"{"message":"maximum context length is 8192 tokens"}"#), AiError::TooLong(_)));
        assert_eq!(failure(500, "boom").code(), "ai_server: 500 boom");
        assert_eq!(AiError::Timeout.code(), "ai_timeout");
    }

    #[test]
    fn replies_drop_thinking() {
        let body = |content: &str| json!({ "choices": [{ "message": { "content": content } }] }).to_string();
        assert_eq!(parse_reply(&body("<think>hmm</think>\n\nOK")).unwrap(), "OK");
        assert_eq!(parse_reply(&body("  plain  ")).unwrap(), "plain");
        assert!(matches!(parse_reply(&body("<think>never ends")), Err(AiError::BadAnswer(_))));
        assert!(matches!(parse_reply("{}"), Err(AiError::BadAnswer(_))));
    }

    #[test]
    fn descriptions_are_cleaned() {
        assert_eq!(clean_description("“VLC 是一款开源的跨平台媒体播放器。”").unwrap(), "VLC 是一款开源的跨平台媒体播放器。");
        assert_eq!(clean_description("Description: A\n code   editor.").unwrap(), "A code editor.");
        assert_eq!(clean_description("UNKNOWN"), Err(AiError::Unknown));
        assert_eq!(clean_description("unknown."), Err(AiError::Unknown));
        assert_eq!(clean_description("x".repeat(400).as_str()).unwrap().chars().count(), SUMMARY_CHARS + 1);
    }

    #[test]
    fn describe_sends_facts_and_records_the_model() {
        let mut lib = Library::default();
        let mut vlc = software("s1", "VLC", None);
        vlc.publisher = "VideoLAN".into();
        vlc.versions.push(Version {
            id: "v".into(),
            version: "3.0".into(),
            note: String::new(),
            added_at: String::new(),
            files: vec![PackageFile {
                id: "f".into(),
                name: "vlc-3.0-win64.exe".into(),
                platform: Platform::Windows,
                arch: "x64".into(),
                size: 1,
                sha256: String::new(),
                storage: Storage::Managed,
                path: String::new(),
                added_at: String::new(),
            }],
        });
        lib.software.push(vlc);
        lib.summaries.insert("VLC".into(), Summary { text: "VLC is a media player.".into(), source: "wikipedia".into(), ..Summary::default() });
        let mut seen = String::new();
        let summary = describe_with(
            &mut |system, user| {
                assert!(system.contains("Simplified Chinese"));
                seen = user.to_string();
                Ok("VLC 是开源的媒体播放器。".into())
            },
            "deepseek-chat",
            &lib,
            "VLC",
            "",
        )
        .unwrap();
        assert!(seen.contains("Publisher: VideoLAN") && seen.contains("vlc-3.0-win64.exe") && seen.contains("VLC is a media player."));
        assert_eq!(summary.source, "AI · deepseek-chat");
        assert_eq!(summary.lang, "zh");
        assert!(!crate::summary::needs_translation(&summary, "zh-CN"));

        lib.settings.language = "en-US".into();
        let unknown = describe_with(&mut |_, _| Ok("UNKNOWN".into()), "m", &lib, "Mystery", "Acme");
        assert_eq!(unknown, Err(AiError::Unknown));
    }

    #[test]
    fn answers_are_parsed_leniently() {
        let fenced = "好的：\n```json\n{\"items\":[{\"id\":\"a1\",\"category\":\"c2\",\"new\":null,\"reason\":\"代码编辑器\"},{\"id\":\"a2\",\"category\":\"null\",\"new\":\"办公效率\"}]}\n```";
        let parsed = parse_classes(fenced);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0], ("a1".into(), Answer { category: Some("c2".into()), new: None, reason: "代码编辑器".into() }));
        assert_eq!(parsed[1].1.category, None);
        assert_eq!(parsed[1].1.new.as_deref(), Some("办公效率"));
        assert_eq!(parse_classes(r#"[{"id":"a1","category":"c1"}]"#).len(), 1);
        assert!(parse_classes("I can't help with that.").is_empty());
    }

    #[test]
    fn suggestions_map_back_to_categories() {
        let mut lib = Library::default();
        lib.categories = vec![category("dev", "开发工具", None), category("ed", "编辑器", Some("dev")), category("sys", "系统工具", None)];
        lib.software = vec![
            software("code", "Visual Studio Code", None),
            software("py", "Python", Some("ed")),
            software("notion", "Notion", None),
            software("obs", "Obsidian", None),
            software("zip", "7-Zip", Some("sys")),
            software("x", "Mystery", None),
        ];
        let ids: Vec<String> = lib.software.iter().map(|s| s.id.clone()).collect();
        let mut prompts = Vec::new();
        let mut steps = Vec::new();
        let out = suggest_with(
            &mut |_, user| {
                prompts.push(user.to_string());
                Ok(json!({ "items": [
                    { "id": "a1", "category": "c2", "reason": "代码编辑器" },
                    { "id": "a2", "category": "c1" },
                    { "id": "a3", "new": "办公效率" },
                    { "id": "a4", "new": "办公效率" },
                    { "id": "a5", "new": "系统工具" },
                    { "id": "a6", "category": null, "new": null },
                    { "id": "a99", "category": "c1" },
                    { "id": "a1x", "category": "c1" },
                ]})
                .to_string())
            },
            &lib,
            &ids,
            &AtomicBool::new(false),
            &mut |done, total| steps.push((done, total)),
        )
        .unwrap();
        assert!(prompts[0].contains("c2: 开发工具 / 编辑器"), "{}", prompts[0]);
        assert!(prompts[0].contains(r#""current":"c2""#), "Python 现在在 c2");
        assert_eq!(steps, [(0, 6), (6, 6)]);

        assert_eq!(out[0].category_id.as_deref(), Some("ed"));
        assert_eq!(out[0].reason, "代码编辑器");
        assert_eq!(out[1].category_id.as_deref(), Some("ed"), "已经在开发工具的下级里，不挪回上一级");
        assert!(out[1].unchanged);
        assert_eq!(out[2].new_category, Some(NewCategory { name: "办公效率".into(), icon: "briefcase".into() }));
        assert_eq!(out[4].category_id.as_deref(), Some("sys"), "要新建的分类已经有了就用已有的");
        assert!(out[4].unchanged && out[4].new_category.is_none());
        assert!(out[5].category_id.is_none() && out[5].new_category.is_none());
    }

    #[test]
    fn later_batches_see_earlier_new_names_and_can_be_cancelled() {
        let mut lib = Library::default();
        lib.software = (0..BATCH + 3).map(|i| software(&format!("s{i}"), &format!("App {i}"), None)).collect();
        let ids: Vec<String> = lib.software.iter().map(|s| s.id.clone()).collect();
        let mut prompts = Vec::new();
        let out = suggest_with(
            &mut |_, user| {
                prompts.push(user.to_string());
                let first = if prompts.len() == 1 { 1 } else { BATCH + 1 };
                Ok(json!({ "items": [{ "id": format!("a{first}"), "new": "工具" }] }).to_string())
            },
            &lib,
            &ids,
            &AtomicBool::new(false),
            &mut |_, _| {},
        )
        .unwrap();
        assert_eq!(prompts.len(), 2);
        assert!(prompts[1].contains("New categories proposed so far: 工具"));
        assert!(prompts[1].contains(&format!("\"id\":\"a{}\"", BATCH + 1)));
        assert!(out[0].new_category.is_some() && out[BATCH].new_category.is_some());
        assert!(out[1].new_category.is_none());

        let cancelled = suggest_with(&mut |_, _| unreachable!(), &lib, &ids, &AtomicBool::new(true), &mut |_, _| {});
        assert_eq!(cancelled.unwrap_err(), AiError::Cancelled);
        let garbage = suggest_with(&mut |_, _| Ok("sorry".into()), &lib, &ids, &AtomicBool::new(false), &mut |_, _| {});
        assert!(matches!(garbage, Err(AiError::BadAnswer(_))));
    }

    /// 本机起一个假的服务：按顺序回给定的（状态码，正文），记下收到的请求。
    fn fake_service(replies: Vec<(u16, String)>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://127.0.0.1:{}/v1", listener.local_addr().unwrap().port());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let record = seen.clone();
        std::thread::spawn(move || {
            for (status, body) in replies {
                let Ok((stream, _)) = listener.accept() else { return };
                let mut reader = BufReader::new(stream);
                let mut head = String::new();
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap_or(0);
                    }
                    head.push_str(&line);
                }
                let mut payload = vec![0; length];
                reader.read_exact(&mut payload).unwrap();
                record.lock().unwrap().push(format!("{head}\n{}", String::from_utf8_lossy(&payload)));
                let mut stream = reader.into_inner();
                let reply = format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                stream.write_all(reply.as_bytes()).unwrap();
            }
        });
        (address, seen)
    }

    #[test]
    fn talks_to_an_openai_compatible_service() {
        let ok = json!({ "choices": [{ "message": { "content": "OK" } }] }).to_string();
        let models = json!({ "data": [{ "id": "b-model" }, { "id": "models/a-model" }, { "id": "b-model" }] }).to_string();
        let (address, seen) = fake_service(vec![
            (400, r#"{"error":{"message":"Unknown parameter: enable_thinking"}}"#.into()),
            (200, ok.clone()),
            (200, ok),
            (200, models),
            (401, r#"{"error":{"message":"bad key"}}"#.into()),
        ]);
        let settings = AiSettings { preset: "qwen".into(), base_url: format!("{address}/"), model: "m-test".into() };
        let service = Service::new(&settings, Some(" sk-test ")).unwrap();
        assert_eq!(service.chat("sys", "ping", true, 0.1).unwrap(), "OK");
        assert_eq!(service.chat("sys", "ping again", true, 0.1).unwrap(), "OK");
        assert_eq!(service.models().unwrap(), ["a-model", "b-model"]);
        assert!(matches!(service.chat("sys", "x", false, 0.1), Err(AiError::Auth(m)) if m == "bad key"));

        let seen = seen.lock().unwrap();
        assert!(seen[0].starts_with("POST /v1/chat/completions"));
        assert!(seen[0].to_lowercase().contains("authorization: bearer sk-test"));
        assert!(seen[0].contains("\"enable_thinking\":false") && seen[0].contains("json_object"), "{}", seen[0]);
        assert!(!seen[1].contains("enable_thinking") && !seen[1].contains("temperature"), "服务不认可选参数时全去掉再试");
        assert!(!seen[2].contains("enable_thinking"), "记住了这个模型不认可选参数");
        assert!(seen[3].starts_with("GET /v1/models"));
    }

    #[test]
    fn remote_services_need_a_key() {
        let remote = AiSettings { preset: "deepseek".into(), base_url: "https://api.deepseek.com".into(), model: "deepseek-chat".into() };
        assert_eq!(Service::new(&remote, None).err(), Some(AiError::NoKey));
        assert_eq!(Service::new(&remote, Some("  ")).err(), Some(AiError::NoKey));
        let local = AiSettings { preset: "ollama".into(), base_url: "http://localhost:11434/v1".into(), model: String::new() };
        let service = Service::new(&local, None).unwrap();
        assert_eq!(service.chat("s", "u", false, 0.1).err(), Some(AiError::NoModel));
        assert_eq!(Service::new(&AiSettings::default(), None).err(), Some(AiError::NotConfigured));
        let bad = AiSettings { base_url: "api.example.com".into(), ..AiSettings::default() };
        assert_eq!(Service::new(&bad, Some("k")).err(), Some(AiError::BadUrl));
    }
}
