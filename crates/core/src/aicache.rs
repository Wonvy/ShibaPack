//! AI 结果的缓存：智能分类的建议、启动项的解释。
//!
//! 存在仓库的 `.shibapack/cache/ai.json`，和 Homebrew 目录的缓存放在一起，可以随时删掉（删了只是下次要重新问 AI）。
//! 同样的问题再问时直接用记下的答案，不再调用 AI；只有用户点了“重新分析”“重新解释”才重新问。
//! AI 写的软件简介不在这里：它们本来就存在索引的 `summaries` 里。

use crate::{classify::Suggestion, model::*};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

const FILE: &str = "ai.json";
/// 缓存里最多记多少条，多了先丢最旧的。
const MAX_ENTRIES: usize = 4000;

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCache {
    /// 智能分类：软件 id → 那次问 AI 的指纹和建议。
    #[serde(default)]
    pub classify: BTreeMap<String, CachedSuggestion>,
    /// 启动项的解释：`系统|类型|内部名称|界面语言` → 解释。
    #[serde(default)]
    pub explain: BTreeMap<String, CachedText>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedSuggestion {
    pub fingerprint: String,
    pub suggestion: Suggestion,
    pub at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedText {
    pub text: String,
    /// 哪个模型写的。
    pub model: String,
    pub at: String,
}

impl AiCache {
    /// 读缓存。没有、坏了都当作空的：缓存丢了不要紧，只是要重新问。
    pub fn load(dir: &Path) -> AiCache {
        std::fs::read(dir.join(FILE)).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
    }

    /// 先写临时文件再换上，写到一半出错也不会把原来的弄坏。
    pub fn save(&mut self, dir: &Path) -> std::io::Result<()> {
        trim(&mut self.classify, |entry| entry.at.clone());
        trim(&mut self.explain, |entry| entry.at.clone());
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join(format!("{FILE}.tmp"));
        std::fs::write(&tmp, serde_json::to_vec(self)?)?;
        std::fs::rename(tmp, dir.join(FILE))
    }
}

/// 超过上限时按时间丢掉最旧的。
fn trim<T>(map: &mut BTreeMap<String, T>, at: impl Fn(&T) -> String) {
    if map.len() <= MAX_ENTRIES {
        return;
    }
    let mut stamps: Vec<(String, String)> = map.iter().map(|(key, value)| (at(value), key.clone())).collect();
    stamps.sort();
    for (_, key) in stamps.into_iter().take(map.len() - MAX_ENTRIES) {
        map.remove(&key);
    }
}

/// 一个软件在 AI 分类时的指纹。问 AI 时用到的东西（名字、发布者、简介、现在的分类、整棵分类树、模型、界面语言）
/// 有一样变了，指纹就不同，要重新问；都没变就用上次的答案。
pub fn classify_fingerprint(lib: &Library, software: &Software, model: &str) -> String {
    let mut hasher = Sha256::new();
    let mut add = |text: &str| {
        hasher.update(text.as_bytes());
        hasher.update([0]);
    };
    add(model);
    add(&lib.settings.language);
    add(&software.name);
    add(&software.publisher);
    add(software.category_id.as_deref().unwrap_or(""));
    add(lib.summaries.get(&software.name).map_or("", |summary| summary.text.as_str()));
    for category in &lib.categories {
        add(&category.id);
        add(&category.name);
        add(category.parent_id.as_deref().unwrap_or(""));
    }
    hex::encode(hasher.finalize())[..24].to_string()
}

/// 启动项解释的键：同一个系统上同一种、同一个名字的项，解释是一样的（换了界面语言要重新写）。
pub fn explain_key(os: &str, source: &str, label: &str, language: &str) -> String {
    format!("{os}|{source}|{label}|{language}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library() -> Library {
        let mut lib = Library::default();
        lib.categories.push(Category { id: "c1".into(), name: "开发工具".into(), parent_id: None, color: None, icon: None });
        let mut software: Software = serde_json::from_value(serde_json::json!({ "id": "s1", "name": "Visual Studio Code", "addedAt": "2026-11-20T10:00:00+08:00" })).unwrap();
        software.publisher = "Microsoft".into();
        lib.software.push(software);
        lib
    }

    #[test]
    fn fingerprint_changes_with_what_the_ai_sees() {
        let lib = library();
        let base = classify_fingerprint(&lib, &lib.software[0], "deepseek-chat");
        assert_eq!(base, classify_fingerprint(&lib, &lib.software[0], "deepseek-chat"));
        assert_ne!(base, classify_fingerprint(&lib, &lib.software[0], "another-model"));

        let mut renamed = lib.clone();
        renamed.categories[0].name = "编程".into();
        assert_ne!(base, classify_fingerprint(&renamed, &renamed.software[0], "deepseek-chat"));

        let mut moved = lib.clone();
        moved.software[0].category_id = Some("c1".into());
        assert_ne!(base, classify_fingerprint(&moved, &moved.software[0], "deepseek-chat"));

        let mut described = lib.clone();
        described.summaries.insert("Visual Studio Code".into(), Summary { text: "代码编辑器".into(), ..Summary::default() });
        assert_ne!(base, classify_fingerprint(&described, &described.software[0], "deepseek-chat"));

        // 收藏、备注这些 AI 看不到的东西变了，不用重新问
        let mut starred = lib.clone();
        starred.software[0].starred = true;
        starred.software[0].note = "常用".into();
        assert_eq!(base, classify_fingerprint(&starred, &starred.software[0], "deepseek-chat"));
    }

    #[test]
    fn saves_and_loads() {
        let dir = tempfile::tempdir().unwrap();
        assert!(AiCache::load(dir.path()).explain.is_empty());
        let mut cache = AiCache::default();
        let text = CachedText { text: "Edge 的更新程序".into(), model: "m".into(), at: "2026-11-20T10:00:00+08:00".into() };
        cache.explain.insert(explain_key("macos", "launchAgent", "com.microsoft.EdgeUpdater.wake", "zh-CN"), text.clone());
        cache.classify.insert(
            "s1".into(),
            CachedSuggestion {
                fingerprint: "abc".into(),
                suggestion: Suggestion {
                    software_id: "s1".into(),
                    category_id: Some("c1".into()),
                    new_category: None,
                    unchanged: false,
                    known: false,
                    evidence: vec![],
                    reason: "代码编辑器".into(),
                },
                at: "2026-11-20T10:00:00+08:00".into(),
            },
        );
        cache.save(dir.path()).unwrap();
        let back = AiCache::load(dir.path());
        assert_eq!(back.explain.values().next(), Some(&text));
        assert_eq!(back.classify["s1"].suggestion.reason, "代码编辑器");
        // 坏了的文件当作空缓存
        std::fs::write(dir.path().join(FILE), b"{not json").unwrap();
        assert!(AiCache::load(dir.path()).classify.is_empty());
    }

    #[test]
    fn trims_oldest() {
        let mut map: BTreeMap<String, CachedText> = BTreeMap::new();
        for i in 0..MAX_ENTRIES + 3 {
            map.insert(format!("k{i}"), CachedText { text: String::new(), model: String::new(), at: format!("2026-01-01T00:00:{:05}", i) });
        }
        trim(&mut map, |entry| entry.at.clone());
        assert_eq!(map.len(), MAX_ENTRIES);
        assert!(!map.contains_key("k0") && !map.contains_key("k2") && map.contains_key("k3"));
    }
}
