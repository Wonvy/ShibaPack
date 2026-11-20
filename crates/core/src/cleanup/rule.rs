//! 清理规则：写在 `rules/*.toml` 里，编进程序。每条规则说清楚清哪里、清什么、风险多大、为什么能删。
//!
//! 一条规则在每个系统上各写各的根目录（路径格式见 `vars`），清的是根目录下第一层的文件和文件夹：
//! - `names`：只清名字符合这些通配的（`*.part`）。写了它的规则叫“按名字筛”，才能用下载文件夹这类个人文件夹当根目录。
//! - `keep`：这些名字的不清。
//! - `olderThanDays`：只清这么多天里都没动过的（看里面最新的那个文件）。
//! - `ownOnly`：只清属于当前用户的（Linux 的 /tmp 是大家共用的）。
//!
//! 安全档（safe）是删了会自己重新生成的，默认勾选，直接删除；可恢复（recoverable）和高影响（high）默认不勾，移到回收站。
//! 隐私痕迹（privacy）一律默认不勾、直接删除（进回收站等于没删）。要管理员权限的（`admin`）默认不勾，
//! 只能用下面 `ADMIN_ROOTS` 里列出来的系统文件夹，清理时一起交给提权的进程做。

use super::vars::{self, Os};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    System,
    Browser,
    Apps,
    Development,
    Installers,
    Privacy,
    Trash,
    Custom,
}

impl Category {
    pub fn name(self) -> &'static str {
        match self {
            Category::System => "system",
            Category::Browser => "browser",
            Category::Apps => "apps",
            Category::Development => "development",
            Category::Installers => "installers",
            Category::Privacy => "privacy",
            Category::Trash => "trash",
            Category::Custom => "custom",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Risk {
    /// 删了会自己重新生成。默认勾选，直接删除。
    Safe,
    /// 删了要重新下载或者丢掉一些记录。默认不勾，移到回收站。
    Recoverable,
    /// 删了找不回来或者代价大。默认不勾，确认时要再勾一次。
    High,
}

/// 两种语言的文字。别的语言用英文。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Text {
    pub zh: String,
    pub en: String,
}

impl Text {
    pub fn new(zh: &str, en: &str) -> Text {
        Text { zh: zh.into(), en: en.into() }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerOs<T> {
    #[serde(default)]
    pub macos: T,
    #[serde(default)]
    pub windows: T,
    #[serde(default)]
    pub linux: T,
}

impl<T> PerOs<T> {
    pub fn get(&self, os: Os) -> &T {
        match os {
            Os::Macos => &self.macos,
            Os::Windows => &self.windows,
            Os::Linux => &self.linux,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rule {
    /// `类别.名字`，只用小写字母、数字、`.`、`-`。
    pub id: String,
    pub category: Category,
    pub risk: Risk,
    /// 这些缓存属于哪个应用。界面上用它的图标，“请先关闭”时显示这个名字。
    #[serde(default)]
    pub app: Option<String>,
    /// 清理前要先关掉的程序（进程名，不分大小写）。macOS 上第一个也是退出它时用的应用名。
    #[serde(default)]
    pub processes: PerOs<Vec<String>>,
    pub roots: PerOs<Vec<String>>,
    #[serde(default)]
    pub names: Vec<String>,
    #[serde(default)]
    pub keep: Vec<String>,
    #[serde(default)]
    pub older_than_days: Option<u32>,
    #[serde(default)]
    pub own_only: bool,
    /// 要管理员权限：清理时交给提权的进程做（Windows 弹 UAC，macOS 输入密码）。
    #[serde(default)]
    pub admin: bool,
    /// 直接删除，不进回收站（隐私痕迹用：进回收站等于没删）。
    #[serde(default)]
    pub permanent: bool,
    /// 这些文件是浏览器的 Cookie 数据库：有保留名单时只删名单以外的网站，不删整个文件。
    #[serde(default)]
    pub cookies: bool,
    /// 这个文件是 Firefox 的 places.sqlite：历史和书签在一起，只删里面的浏览记录，书签留着，文件不删。
    #[serde(default)]
    pub history_db: bool,
    /// 不删文件，改为运行这条系统自带的命令（只用于要管理员权限的规则，比如 Windows 的传递优化缓存）。
    #[serde(default)]
    pub command: PerOs<String>,
    pub name: Text,
    /// 这是什么、为什么会变大。
    pub about: Text,
    /// 删了以后会怎样。
    pub effect: Text,
    /// 为什么能删：官方文档或者程序自己的说明。
    pub evidence: String,
}

impl Rule {
    /// 只按名字筛着清一部分。
    pub fn narrow(&self) -> bool {
        !self.names.is_empty()
    }

    /// 直接删除还是移到回收站。
    pub fn deletes(&self) -> bool {
        self.permanent || self.risk == Risk::Safe
    }

    /// 这个系统上要运行的命令（不删文件时）。
    pub fn command_for(&self, os: Os) -> Option<&str> {
        Some(self.command.get(os).as_str()).filter(|c| !c.trim().is_empty())
    }

    /// 这个名字要不要清：符合 `names`（没写就都符合），不在 `keep` 里。
    pub fn wants(&self, name: &str) -> bool {
        (self.names.is_empty() || self.names.iter().any(|p| vars::glob(p, name))) && !self.keep.iter().any(|p| vars::glob(p, name))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleFile {
    rule: Vec<Rule>,
}

const SOURCES: &[(&str, &str)] = &[
    ("system.toml", include_str!("rules/system.toml")),
    ("admin.toml", include_str!("rules/admin.toml")),
    ("browser.toml", include_str!("rules/browser.toml")),
    ("apps.toml", include_str!("rules/apps.toml")),
    ("development.toml", include_str!("rules/development.toml")),
    ("installers.toml", include_str!("rules/installers.toml")),
    ("privacy.toml", include_str!("rules/privacy.toml")),
];

/// 要管理员权限的规则只能清这些系统文件夹。带 `true` 的可以整个清空里面的东西，别的只能按名字筛着清。
pub const ADMIN_ROOTS: &[(Os, &str, bool)] = &[
    (Os::Macos, "${system_library}/Caches", true),
    (Os::Windows, "${windows}/Temp", true),
    (Os::Windows, "${windows}/SoftwareDistribution/Download", true),
    (Os::Windows, "${windows}/Minidump", false),
    (Os::Windows, "${windows}", false),
    (Os::Windows, "${program_data}/Microsoft/Windows/WER/ReportArchive", true),
    (Os::Windows, "${program_data}/Microsoft/Windows/WER/ReportQueue", true),
    (Os::Windows, "${windows}/ServiceProfiles/NetworkService/AppData/Local/Microsoft/Windows/DeliveryOptimization/Cache", true),
];

/// 编进程序的全部规则。规则文件有错时直接停下：单元测试会先发现。
pub fn catalog() -> &'static [Rule] {
    static CATALOG: OnceLock<Vec<Rule>> = OnceLock::new();
    CATALOG.get_or_init(|| load(SOURCES).unwrap_or_else(|err| panic!("清理规则有错：{err}")))
}

fn load(sources: &[(&str, &str)]) -> Result<Vec<Rule>, String> {
    let mut rules = Vec::new();
    let mut ids = HashSet::new();
    for (file, text) in sources {
        let parsed: RuleFile = toml::from_str(text).map_err(|e| format!("{file}：{e}"))?;
        for rule in parsed.rule {
            check(&rule).map_err(|e| format!("{file} {}：{e}", rule.id))?;
            if !ids.insert(rule.id.clone()) {
                return Err(format!("{file}：规则 {} 重复", rule.id));
            }
            rules.push(rule);
        }
    }
    Ok(rules)
}

/// 一条规则写得对不对。
pub fn check(rule: &Rule) -> Result<(), String> {
    let prefix = format!("{}.", rule.category.name());
    if !rule.id.starts_with(&prefix) || !rule.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-') {
        return Err(format!("id 要以 {prefix} 开头，只用小写字母、数字、. 和 -"));
    }
    if matches!(rule.category, Category::Trash | Category::Custom) {
        return Err("回收站和自定义文件夹不用规则写".into());
    }
    let mut any = false;
    for os in Os::ALL {
        for root in rule.roots.get(os) {
            vars::check(root, os)?;
            any = true;
            let (name, tail) = vars::split(root).unwrap_or_default();
            if rule.admin {
                // 要管理员权限的只能清列出来的系统文件夹
                let allowed = ADMIN_ROOTS.iter().find(|(o, t, _)| *o == os && t == root);
                match allowed {
                    None => return Err(format!("{root}：要管理员权限的规则只能用 ADMIN_ROOTS 里的文件夹")),
                    Some((_, _, false)) if !rule.narrow() => return Err(format!("{root}：这个系统文件夹只能按名字筛着清")),
                    _ => {}
                }
                continue;
            }
            if vars::SYSTEM.contains(&name) {
                return Err(format!("{root}：系统文件夹只能给要管理员权限的规则用"));
            }
            // 不按名字筛的规则不能直接清空个人文件夹或者一整类数据的总文件夹
            if !rule.narrow() && tail.is_empty() && vars::BROAD.contains(&name) {
                return Err(format!("{root}：不按名字筛时不能直接清这个文件夹"));
            }
        }
        if !rule.command.get(os).trim().is_empty() && (!rule.admin || os != Os::Windows) {
            return Err("command 只用于 Windows 上要管理员权限的规则".into());
        }
        if rule.processes.get(os).iter().any(|p| p.trim().is_empty()) {
            return Err("进程名不能是空的".into());
        }
    }
    if !any {
        return Err("没有根目录".into());
    }
    for pattern in rule.names.iter().chain(&rule.keep) {
        if pattern.is_empty() || pattern.contains('/') || pattern.contains('\\') {
            return Err(format!("名字通配 {pattern:?} 不能是空的，也不能带路径"));
        }
        if pattern.chars().all(|c| c == '*' || c == '?') {
            return Err(format!("名字通配 {pattern:?} 什么都匹配，不如不写"));
        }
    }
    if rule.older_than_days == Some(0) {
        return Err("olderThanDays 至少是 1".into());
    }
    if rule.category == Category::Privacy && (!rule.narrow() || !rule.permanent || rule.risk == Risk::Safe) {
        return Err("隐私痕迹要按名字筛、直接删除，而且不能是安全档（不默认勾选）".into());
    }
    if rule.permanent && rule.category != Category::Privacy {
        return Err("permanent 只用于隐私痕迹".into());
    }
    if rule.cookies && rule.category != Category::Privacy {
        return Err("cookies 只用于隐私痕迹".into());
    }
    if rule.history_db && (rule.category != Category::Privacy || rule.names != ["places.sqlite"]) {
        return Err("historyDb 只用于隐私痕迹，而且只能是 places.sqlite".into());
    }
    for text in [&rule.name, &rule.about, &rule.effect] {
        if text.zh.trim().is_empty() || text.en.trim().is_empty() {
            return Err("名字和说明要有中英文".into());
        }
    }
    if !rule.evidence.starts_with("https://") {
        return Err("evidence 要是一个 https 链接".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_rules_are_valid() {
        let rules = load(SOURCES).unwrap();
        assert!(rules.len() >= 15, "只有 {} 条", rules.len());
        for os in Os::ALL {
            assert!(rules.iter().any(|r| !r.roots.get(os).is_empty()), "{os:?} 没有规则");
        }
        // 浏览器都要求先关掉
        for rule in rules.iter().filter(|r| r.category == Category::Browser) {
            for os in Os::ALL {
                assert!(rule.roots.get(os).is_empty() || !rule.processes.get(os).is_empty(), "{} 在 {os:?} 上没写进程名", rule.id);
            }
        }
    }

    fn sample() -> Rule {
        Rule {
            id: "development.demo".into(),
            category: Category::Development,
            risk: Risk::Safe,
            app: None,
            processes: PerOs::default(),
            roots: PerOs { macos: vec!["${user_cache}/demo".into()], windows: vec![], linux: vec![] },
            names: vec![],
            keep: vec![],
            older_than_days: None,
            own_only: false,
            admin: false,
            permanent: false,
            cookies: false,
            history_db: false,
            command: PerOs::default(),
            name: Text::new("演示", "Demo"),
            about: Text::new("说明", "About"),
            effect: Text::new("结果", "Effect"),
            evidence: "https://example.com".into(),
        }
    }

    #[test]
    fn rejects_rules_that_could_empty_whole_folders() {
        assert!(check(&sample()).is_ok());
        let mut rule = sample();
        rule.roots.macos = vec!["${downloads}".into()];
        assert!(check(&rule).is_err());
        rule.names = vec!["*.crdownload".into()];
        assert!(check(&rule).is_ok(), "按名字筛的可以");
        let mut rule = sample();
        rule.roots.macos = vec!["${user_cache}".into()];
        assert!(check(&rule).is_err());
        let mut rule = sample();
        rule.names = vec!["*".into()];
        assert!(check(&rule).is_err());
        let mut rule = sample();
        rule.id = "browser.demo".into();
        assert!(check(&rule).is_err(), "id 和类别对不上");
        let mut rule = sample();
        rule.roots.windows = vec!["${user_cache}/x".into()];
        assert!(check(&rule).is_err(), "Windows 没有 user_cache");
        let mut rule = sample();
        rule.evidence = String::new();
        assert!(check(&rule).is_err());
    }

    #[test]
    fn system_folders_need_admin_and_the_allow_list() {
        let mut rule = sample();
        rule.roots = PerOs { macos: vec!["${system_library}/Caches".into()], windows: vec![], linux: vec![] };
        assert!(check(&rule).is_err(), "没标 admin 不能用系统文件夹");
        rule.admin = true;
        assert!(check(&rule).is_ok());
        rule.roots.macos = vec!["${system_library}/Preferences".into()];
        assert!(check(&rule).is_err(), "不在名单里的系统文件夹不行");
        let mut rule = sample();
        rule.admin = true;
        rule.roots = PerOs { macos: vec![], windows: vec!["${windows}".into()], linux: vec![] };
        assert!(check(&rule).is_err(), "Windows 文件夹本身只能按名字筛");
        rule.names = vec!["*.dmp".into()];
        assert!(check(&rule).is_ok());
    }

    #[test]
    fn privacy_rules_are_narrow_permanent_and_never_default() {
        let mut rule = sample();
        rule.id = "privacy.demo".into();
        rule.category = Category::Privacy;
        rule.risk = Risk::High;
        rule.names = vec!["History".into()];
        assert!(check(&rule).is_err(), "隐私痕迹要直接删除");
        rule.permanent = true;
        assert!(check(&rule).is_ok());
        rule.risk = Risk::Safe;
        assert!(check(&rule).is_err(), "隐私痕迹不能是安全档");
        let mut rule = sample();
        rule.permanent = true;
        assert!(check(&rule).is_err(), "别的类别不能跳过回收站");
    }

    #[test]
    fn names_and_keep_filter_entries() {
        let mut rule = sample();
        rule.names = vec!["*.part".into(), "*.crdownload".into()];
        rule.keep = vec!["keep*".into()];
        assert!(rule.wants("a.PART"));
        assert!(!rule.wants("a.dmg"));
        assert!(!rule.wants("keep.part"));
    }
}
