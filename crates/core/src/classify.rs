//! 智能分类：根据软件名、发布者和简介，给软件推荐一个分类。
//!
//! 不联网，也不靠模型：一份常见软件的名单，加上一组按类别分好的关键词。
//! 推荐的分类优先用仓库里已有的（按名字对应，再细到名字出现在简介里的子分类）；
//! 仓库里没有合适的，就推荐新建一个。结果只是建议，用户在预览里确认后才会改动。

use crate::{identify::normalize_name, model::*, store::Repo, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 一类软件。
struct Kind {
    zh: &'static str,
    en: &'static str,
    icon: &'static str,
    /// 用户自己建的分类，名字里含这些词就算这一类。
    aliases: &'static [&'static str],
    /// 常见软件，名字去掉空格和标点、转小写后的样子。
    known: &'static [&'static str],
    /// 发布者名字里含这些词就算这一类。
    publishers: &'static [&'static str],
    /// 简介里的关键词和分量。英文按整词比，中文按包含比。
    terms: &'static [(&'static str, u32)],
}

const KINDS: &[Kind] = &[
    Kind {
        zh: "开发工具",
        en: "Development",
        icon: "code",
        aliases: &["开发", "编程", "程序", "代码", "development", "developer", "programming", "coding", "dev", "ide"],
        known: &[
            "visualstudiocode", "vscode", "visualstudio", "git", "githubdesktop", "sourcetree", "fork", "gitkraken", "docker",
            "dockerdesktop", "nodejs", "node", "python", "java", "jdk", "openjdk", "golang", "go", "rust", "rustup", "postman",
            "insomnia", "sublimetext", "sublime", "notepad", "intellijidea", "pycharm", "webstorm", "goland", "clion",
            "datagrip", "rider", "phpstorm", "androidstudio", "xcode", "cursor", "zed", "vim", "macvim", "neovim", "emacs",
            "iterm", "iterm2", "windowsterminal", "warp", "hyper", "dbeaver", "navicat", "tableplus", "mongodbcompass",
            "redisinsight", "anaconda", "miniconda", "cmake", "msys2", "mingw", "godot", "unity", "unityhub", "hbuilderx",
            "微信开发者工具", "apifox",
        ],
        publishers: &["jetbrains", "github", "docker", "python software foundation", "openjs", "sublime hq", "git scm"],
        terms: &[
            ("source code", 3), ("code editor", 3), ("text editor", 2), ("ide", 3), ("integrated development environment", 3),
            ("version control", 3), ("compiler", 2), ("programming", 2), ("programming language", 3), ("developer", 2),
            ("developers", 2), ("sdk", 2), ("runtime", 2), ("interpreter", 2), ("database", 2), ("terminal", 2),
            ("debugger", 2), ("container", 2), ("containers", 2), ("javascript", 2), ("api", 1), ("编程", 3), ("代码", 2),
            ("开发", 2), ("集成开发环境", 3), ("版本控制", 3), ("编译器", 3), ("编程语言", 3), ("数据库", 2),
            ("终端", 2), ("编辑器", 1), ("运行环境", 2), ("解释器", 2), ("容器", 2),
        ],
    },
    Kind {
        zh: "设计创作",
        en: "Design",
        icon: "palette",
        aliases: &["设计", "创作", "图形", "美术", "绘图", "图像", "design", "graphics", "creative", "art", "image"],
        known: &[
            "figma", "sketch", "photoshop", "adobephotoshop", "illustrator", "adobeillustrator", "indesign", "lightroom",
            "affinitydesigner", "affinityphoto", "affinitypublisher", "blender", "gimp", "inkscape", "krita", "canva",
            "pixelmator", "pixelmatorpro", "cinema4d", "maya", "zbrush", "coreldraw", "adobexd", "axure", "axurerp",
            "mastergo", "pixso", "即时设计", "墨刀", "lunacy", "penpot", "paintnet", "darktable", "rawtherapee",
        ],
        publishers: &["adobe", "figma", "serif", "bohemian coding", "blender foundation", "autodesk", "maxon"],
        terms: &[
            ("graphics", 2), ("3d", 2), ("design", 2), ("designer", 2), ("vector", 2), ("image editor", 3),
            ("photo editor", 3), ("photo", 2), ("raster", 2), ("illustration", 2), ("prototyping", 2), ("ui design", 3),
            ("animation", 1), ("rendering", 1), ("modeling", 2), ("modelling", 2), ("painting", 2), ("drawing", 2),
            ("设计", 2), ("图形", 2), ("三维", 2), ("建模", 2), ("图像处理", 3), ("修图", 3), ("绘图", 2), ("绘画", 2),
            ("矢量", 2), ("原型", 2), ("渲染", 1), ("动画", 1),
        ],
    },
    Kind {
        zh: "办公效率",
        en: "Productivity",
        icon: "briefcase",
        aliases: &["办公", "效率", "文档", "笔记", "office", "productivity", "document", "documents", "notes", "work"],
        known: &[
            "microsoftoffice", "office", "word", "microsoftword", "excel", "microsoftexcel", "powerpoint",
            "microsoftpowerpoint", "wps", "wpsoffice", "libreoffice", "onlyoffice", "notion", "obsidian", "evernote",
            "印象笔记", "onenote", "typora", "marktext", "logseq", "joplin", "有道云笔记", "语雀", "xmind", "mindnode",
            "adobeacrobat", "adobeacrobatreader", "acrobatreader", "foxitpdfreader", "foxitreader", "pdfexpert", "zotero",
            "todoist", "ticktick", "滴答清单", "things", "飞书文档",
        ],
        publishers: &["kingsoft", "the document foundation", "notion labs", "dynalist", "evernote", "xmind"],
        terms: &[
            ("note-taking", 3), ("notes", 2), ("markdown", 2), ("office suite", 3), ("word processor", 3),
            ("spreadsheet", 3), ("presentation", 2), ("pdf", 2), ("productivity", 2), ("mind map", 3),
            ("mind mapping", 3), ("knowledge base", 2), ("to-do", 2), ("calendar", 2), ("笔记", 3), ("办公", 3),
            ("文档", 2), ("表格", 2), ("演示", 2), ("幻灯片", 2), ("思维导图", 3), ("知识库", 2), ("待办", 2),
            ("日程", 2), ("文字处理", 3), ("效率", 1),
        ],
    },
    Kind {
        zh: "影音媒体",
        en: "Media",
        icon: "film",
        aliases: &["影音", "媒体", "视频", "音乐", "播放", "多媒体", "media", "video", "audio", "music", "player"],
        known: &[
            "vlc", "vlcmediaplayer", "potplayer", "mpv", "iina", "infuse", "kmplayer", "mpchc", "obs", "obsstudio",
            "handbrake", "davinciresolve", "adobepremierepro", "premierepro", "finalcutpro", "capcut", "剪映", "audacity",
            "spotify", "网易云音乐", "qq音乐", "foobar2000", "musicbee", "plex", "kodi", "shotcut", "kdenlive", "ffmpeg",
            "爱奇艺", "优酷", "腾讯视频", "哔哩哔哩", "bilibili", "screenflow", "camtasia",
        ],
        publishers: &["videolan", "obs project", "blackmagic"],
        terms: &[
            ("media player", 3), ("video", 2), ("audio", 2), ("music", 2), ("player", 1), ("streaming", 2),
            ("video editor", 3), ("video editing", 3), ("recording", 2), ("screencast", 2), ("podcast", 2),
            ("multimedia", 3), ("codec", 2), ("播放器", 3), ("视频", 2), ("音频", 2), ("音乐", 2), ("剪辑", 3),
            ("录屏", 3), ("直播", 2), ("多媒体", 3), ("影音", 3),
        ],
    },
    Kind {
        zh: "网络工具",
        en: "Internet",
        icon: "globe",
        aliases: &["网络", "上网", "浏览器", "下载", "互联网", "网盘", "network", "internet", "browser", "web", "download", "cloud"],
        known: &[
            "chrome", "googlechrome", "firefox", "microsoftedge", "edge", "safari", "opera", "brave", "vivaldi", "arc",
            "thunderbird", "qbittorrent", "transmission", "motrix", "迅雷", "thunder", "internetdownloadmanager", "idm",
            "freedownloadmanager", "filezilla", "cyberduck", "putty", "termius", "rustdesk", "teamviewer", "anydesk",
            "todesk", "向日葵", "clash", "clashforwindows", "clashverge", "v2rayn", "shadowsocks", "wireguard",
            "tailscale", "zerotier", "dropbox", "googledrive", "onedrive", "百度网盘", "baidunetdisk", "坚果云",
            "阿里云盘", "synologydrive",
        ],
        publishers: &["mozilla", "google", "dropbox", "teamviewer"],
        terms: &[
            ("web browser", 3), ("browser", 2), ("download manager", 3), ("bittorrent", 3), ("torrent", 2), ("ftp", 2),
            ("vpn", 3), ("proxy", 2), ("remote desktop", 3), ("cloud storage", 3), ("file hosting", 2),
            ("file synchronization", 2), ("email client", 2), ("浏览器", 3), ("下载", 2), ("网盘", 3), ("远程", 2),
            ("远程控制", 3), ("代理", 2), ("网络", 1), ("邮件", 2), ("云存储", 3),
        ],
    },
    Kind {
        zh: "聊天社交",
        en: "Communication",
        icon: "message",
        aliases: &["聊天", "社交", "通讯", "沟通", "chat", "social", "communication", "messaging"],
        known: &[
            "wechat", "微信", "qq", "tim", "telegram", "telegramdesktop", "discord", "slack", "zoom", "teams",
            "microsoftteams", "skype", "钉钉", "dingtalk", "飞书", "feishu", "lark", "whatsapp", "signal", "line",
            "kakaotalk", "企业微信", "wecom", "腾讯会议", "tencentmeeting", "voovmeeting", "element", "mattermost",
        ],
        publishers: &["tencent", "腾讯", "telegram", "discord", "slack", "zoom"],
        terms: &[
            ("instant messaging", 3), ("messaging", 3), ("messenger", 3), ("chat", 3), ("video conferencing", 3),
            ("voip", 3), ("communication", 2), ("social", 2), ("聊天", 3), ("即时通讯", 3), ("即时通信", 3), ("社交", 3),
            ("通讯", 2), ("会议", 2), ("视频会议", 3),
        ],
    },
    Kind {
        zh: "系统工具",
        en: "Utilities",
        icon: "wrench",
        aliases: &["系统", "工具", "实用", "维护", "system", "utilities", "utility", "tools", "tool"],
        known: &[
            "7zip", "winrar", "bandizip", "theunarchiver", "keka", "peazip", "everything", "ccleaner", "cleanmymac",
            "appcleaner", "rufus", "balenaetcher", "etcher", "ventoy", "cpuz", "gpuz", "crystaldiskinfo",
            "crystaldiskmark", "hwinfo", "hwmonitor", "powertoys", "autohotkey", "raycast", "alfred", "magnet",
            "rectangle", "bartender", "hiddenbar", "istatmenus", "stats", "snipaste", "sharex", "flameshot", "greenshot",
            "ditto", "aida64", "diskgenius", "火绒", "malwarebytes", "bitwarden", "1password", "keepass", "keepassxc",
            "veracrypt", "virtualbox", "vmwareworkstation", "vmwarefusion", "parallelsdesktop", "utm",
        ],
        publishers: &["igor pavlov", "voidtools", "piriform", "sysinternals"],
        terms: &[
            ("file archiver", 3), ("archiver", 3), ("compression", 2), ("utility", 2), ("disk", 2), ("partition", 2),
            ("cleaner", 2), ("uninstaller", 3), ("screenshot", 3), ("clipboard", 3), ("launcher", 2),
            ("window manager", 3), ("virtual machine", 3), ("virtualization", 3), ("hardware", 2), ("system monitor", 3),
            ("password manager", 3), ("antivirus", 3), ("backup", 2), ("file manager", 2), ("bootable", 2), ("压缩", 3),
            ("解压", 3), ("清理", 2), ("卸载", 2), ("截图", 3), ("剪贴板", 3), ("启动器", 2), ("虚拟机", 3), ("硬件", 2),
            ("磁盘", 2), ("分区", 2), ("密码管理", 3), ("杀毒", 3), ("备份", 2), ("文件管理", 2), ("系统", 1), ("工具", 1),
        ],
    },
    Kind {
        zh: "游戏娱乐",
        en: "Games",
        icon: "gamepad",
        aliases: &["游戏", "娱乐", "game", "games", "gaming", "entertainment"],
        known: &[
            "steam", "epicgameslauncher", "epicgames", "goggalaxy", "battlenet", "eaapp", "origin", "ubisoftconnect",
            "wegame", "minecraft", "xbox", "playnite", "retroarch", "yuzu", "ryujinx", "dolphin", "pcsx2", "ppsspp",
        ],
        publishers: &["valve", "epic games", "blizzard", "mojang"],
        terms: &[
            ("video game", 3), ("game", 2), ("games", 2), ("gaming", 3), ("emulator", 2), ("digital distribution", 2),
            ("游戏", 3), ("娱乐", 2), ("模拟器", 2),
        ],
    },
];

/// 认出了软件名时的分量：一次命中就足以压过关键词。
const KNOWN_WEIGHT: u32 = 10;
const PUBLISHER_WEIGHT: u32 = 4;
/// 只靠关键词时，至少要有这么多分量才给建议。
const MIN_SCORE: u32 = 2;

/// 推荐新建的分类。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewCategory {
    pub name: String,
    pub icon: String,
}

/// 对一个软件的分类建议。`category_id` 和 `new_category` 至多有一个；都没有表示判断不出来。
/// 能读回来：AI 给的建议记在缓存里（见 `aicache`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub software_id: String,
    pub category_id: Option<String>,
    pub new_category: Option<NewCategory>,
    /// 推荐的就是它现在所在的分类。
    pub unchanged: bool,
    /// 名字在内置的常见软件名单里。
    pub known: bool,
    /// 发布者和简介里命中的词，给用户看推荐的依据。
    pub evidence: Vec<String>,
    /// AI 给的一句依据（它认为这是个什么软件）。内置规则给的建议没有。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
}

/// 用户在预览里确认的一条：放进已有分类，或者新建一个分类再放进去。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Assignment {
    pub software_id: String,
    #[serde(default)]
    pub category_id: Option<String>,
    #[serde(default)]
    pub new_category: Option<NewCategory>,
}

/// `term` 在 `text`（已转小写）里出现了没有。英文要求整词，中文按包含。
fn mentions(text: &str, term: &str) -> bool {
    if !term.is_ascii() {
        return text.contains(term);
    }
    let bytes = text.as_bytes();
    let boundary = |i: usize| i >= bytes.len() || !bytes[i].is_ascii_alphanumeric();
    let mut from = 0;
    while let Some(found) = text[from..].find(term) {
        let start = from + found;
        let end = start + term.len();
        let before_ok = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        if before_ok && boundary(end) {
            return true;
        }
        from = start + term.len().max(1);
        while from < text.len() && !text.is_char_boundary(from) {
            from += 1;
        }
    }
    false
}

/// 名字对得上内置名单：完全一样；或者名单里的名字够长（5 个字符以上），软件名以它开头（"Docker Desktop"）。
/// 短名字只认完全一样的，免得 "Linear" 被当成 "LINE"。
fn is_known(key: &str, known: &str) -> bool {
    key == known || (known.chars().count() >= 5 && key.starts_with(known))
}

/// 判断一个软件是哪一类。返回 (类别, 是否认出了名字, 命中的词)。
fn guess(name: &str, publisher: &str, summary: &str) -> Option<(&'static Kind, bool, Vec<String>)> {
    let key = normalize_name(name);
    let publisher_lower = publisher.to_lowercase();
    let text = format!("{name} {summary}").to_lowercase();

    let mut best: Option<(&'static Kind, u32, bool, Vec<String>)> = None;
    for kind in KINDS {
        let known = !key.is_empty() && kind.known.iter().any(|k| is_known(&key, k));
        let mut score = if known { KNOWN_WEIGHT } else { 0 };
        let mut evidence = Vec::new();
        if !publisher_lower.is_empty() && kind.publishers.iter().any(|p| publisher_lower.contains(p)) {
            score += PUBLISHER_WEIGHT;
            evidence.push(publisher.trim().to_string());
        }
        for (term, weight) in kind.terms {
            if mentions(&text, term) {
                score += weight;
                evidence.push((*term).to_string());
            }
        }
        // 分数相同时，先列出的类别优先（比较常见）
        if score > best.as_ref().map_or(0, |b| b.1) {
            best = Some((kind, score, known, evidence));
        }
    }
    best.filter(|(_, score, known, _)| *known || *score >= MIN_SCORE).map(|(kind, _, known, evidence)| (kind, known, evidence))
}

/// 给一个新分类挑图标：名字对得上某一类就用那一类的图标，否则用文件夹。AI 分类用。
#[cfg_attr(not(feature = "network"), allow(dead_code))]
pub(crate) fn icon_for(name: &str) -> &'static str {
    let probe = Category { id: String::new(), name: name.to_string(), parent_id: None, color: None, icon: None };
    KINDS.iter().find(|kind| category_matches(&probe, kind)).map_or("folder", |kind| kind.icon)
}

/// 分类名里含某一类的别名，或者和这一类的名字一样。
fn category_matches(category: &Category, kind: &Kind) -> bool {
    let name = category.name.trim().to_lowercase();
    name == kind.zh || name == kind.en.to_lowercase() || kind.aliases.iter().any(|alias| mentions(&name, alias))
}

/// 一个分类的全部下级，按树的顺序。
pub(crate) fn descendants<'a>(categories: &'a [Category], id: &str, out: &mut Vec<&'a Category>, depth: usize) {
    if depth > 16 {
        return;
    }
    for child in categories.iter().filter(|c| c.parent_id.as_deref() == Some(id)) {
        out.push(child);
        descendants(categories, &child.id, out, depth + 1);
    }
}

/// 分类按树的顺序排成一列，同时记下层级。
pub(crate) fn tree_order<'a>(categories: &'a [Category], parent: Option<&str>, depth: usize, out: &mut Vec<(&'a Category, usize)>) {
    if depth > 16 {
        return;
    }
    for category in categories.iter().filter(|c| c.parent_id.as_deref() == parent) {
        out.push((category, depth));
        tree_order(categories, Some(&category.id), depth + 1, out);
    }
}

/// 新仓库使用和智能分类一致的一级分类。
pub(crate) fn default_categories(language: &str) -> Vec<Category> {
    let zh = language.to_lowercase().starts_with("zh");
    KINDS.iter().map(|kind| Category {
        id: crate::new_id(),
        name: if zh { kind.zh } else { kind.en }.into(),
        parent_id: None,
        color: None,
        icon: Some(kind.icon.into()),
    }).collect()
}

#[derive(Deserialize)]
pub struct ImportCategoryTarget {
    pub name: String,
    pub publisher: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCategoryPreview {
    pub category_id: Option<String>,
    pub new_category: Option<NewCategory>,
}

struct CategorySubject<'a> {
    id: &'a str,
    name: &'a str,
    publisher: &'a str,
    category_id: Option<&'a str>,
}

impl Repo {
    /// 给一批软件推荐分类。只读，不改动任何东西。
    pub fn suggest_categories(&self, software_ids: &[String]) -> Vec<Suggestion> {
        let mut ordered = Vec::new();
        tree_order(&self.lib.categories, None, 0, &mut ordered);

        software_ids
            .iter()
            .filter_map(|id| self.lib.software.iter().find(|s| &s.id == id))
            .map(|software| self.category_suggestion(&ordered, CategorySubject {
                id: &software.id,
                name: &software.name,
                publisher: &software.publisher,
                category_id: software.category_id.as_deref(),
            }))
            .collect()
    }

    /// 按导入预览里的名称给出分类。加入已有软件时保留它的分类。
    pub fn preview_import_categories(&self, targets: &[ImportCategoryTarget]) -> Vec<ImportCategoryPreview> {
        let mut ordered = Vec::new();
        tree_order(&self.lib.categories, None, 0, &mut ordered);
        let mut by_name = BTreeMap::new();
        for software in &self.lib.software {
            by_name.entry(normalize_name(&software.name)).or_insert(software);
        }
        targets.iter().map(|target| {
            let existing = by_name.get(&normalize_name(&target.name)).copied();
            if let Some(category_id) = existing.and_then(|software| software.category_id.clone()) {
                return ImportCategoryPreview { category_id: Some(category_id), new_category: None };
            }
            let name = existing.map_or(target.name.trim(), |software| software.name.as_str());
            let publisher = existing.map(|software| software.publisher.as_str())
                .filter(|publisher| !publisher.trim().is_empty()).unwrap_or(&target.publisher);
            let suggestion = self.category_suggestion(&ordered, CategorySubject { id: "", name, publisher, category_id: None });
            ImportCategoryPreview { category_id: suggestion.category_id, new_category: suggestion.new_category }
        }).collect()
    }

    fn category_suggestion(&self, ordered: &[(&Category, usize)], subject: CategorySubject<'_>) -> Suggestion {
        let CategorySubject { id, name, publisher, category_id } = subject;
        let summary = self.lib.summaries.get(name).map(|s| s.text.as_str()).unwrap_or("");
        let mut suggestion = Suggestion {
            software_id: id.to_owned(),
            category_id: None,
            new_category: None,
            unchanged: false,
            known: false,
            evidence: Vec::new(),
            reason: String::new(),
        };
        let Some((kind, known, evidence)) = guess(name, publisher, summary) else { return suggestion };
        suggestion.known = known;
        suggestion.evidence = evidence;

        let matching = ordered.iter().filter(|(category, _)| category_matches(category, kind)).min_by_key(|(_, depth)| *depth);
        if let Some((category, _)) = matching {
            let mut children = Vec::new();
            descendants(&self.lib.categories, &category.id, &mut children, 0);
            let already = category_id == Some(category.id.as_str()) || children.iter().any(|c| Some(c.id.as_str()) == category_id);
            suggestion.category_id = if already {
                category_id.map(str::to_owned)
            } else {
                let text = format!("{name} {summary}").to_lowercase();
                let refined = children.into_iter().rev().find(|child| {
                    let child_name = child.name.trim().to_lowercase();
                    child_name.chars().count() >= 2 && mentions(&text, &child_name)
                });
                Some(refined.unwrap_or(category).id.clone())
            };
        } else {
            let label = if self.lib.settings.language.to_lowercase().starts_with("zh") { kind.zh } else { kind.en };
            suggestion.new_category = Some(NewCategory { name: label.into(), icon: kind.icon.into() });
        }
        suggestion.unchanged = suggestion.category_id.is_some() && suggestion.category_id.as_deref() == category_id;
        suggestion
    }

    /// 导入时只给未分类的软件应用建议，保留用户已有的分类。
    pub fn apply_import_classification(&mut self, suggestions: Vec<Suggestion>) -> Result<usize> {
        let assignments = suggestions.into_iter()
            .filter(|suggestion| self.lib.software.iter().any(|s| s.id == suggestion.software_id && s.category_id.is_none()))
            .map(|suggestion| Assignment {
                software_id: suggestion.software_id,
                category_id: suggestion.category_id,
                new_category: suggestion.new_category,
            })
            .collect();
        self.apply_classification(assignments)
    }

    /// 按用户确认的结果分类：需要新建的分类按名字只建一个，然后一次性保存。返回实际改动的软件数。
    pub fn apply_classification(&mut self, assignments: Vec<Assignment>) -> Result<usize> {
        let mut created: BTreeMap<String, String> = BTreeMap::new();
        let mut changed = 0;
        for assignment in assignments {
            let Some(s) = self.lib.software.iter().position(|x| x.id == assignment.software_id) else { continue };
            let target = match (assignment.category_id, assignment.new_category) {
                (Some(id), _) if self.lib.categories.iter().any(|c| c.id == id) => id,
                (_, Some(new)) if !new.name.trim().is_empty() => {
                    let name = new.name.trim().to_string();
                    let key = name.to_lowercase();
                    // 同名的分类已经有了（包括这一批里刚建的），就用它
                    let existing = self.lib.categories.iter().find(|c| c.name.trim().to_lowercase() == key).map(|c| c.id.clone());
                    match existing.or_else(|| created.get(&key).cloned()) {
                        Some(id) => id,
                        None => {
                            let id = crate::new_id();
                            self.lib.categories.push(Category { id: id.clone(), name, parent_id: None, color: None, icon: Some(new.icon) });
                            created.insert(key, id.clone());
                            id
                        }
                    }
                }
                _ => continue,
            };
            if self.lib.software[s].category_id.as_deref() != Some(target.as_str()) {
                self.lib.software[s].category_id = Some(target);
                changed += 1;
            }
        }
        if changed > 0 || !created.is_empty() {
            self.save()?;
        }
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> (tempfile::TempDir, Repo) {
        let dir = tempfile::tempdir().unwrap();
        let mut repo = Repo::open(dir.path()).unwrap();
        repo.lib.categories.clear();
        (dir, repo)
    }

    fn add(repo: &mut Repo, name: &str, publisher: &str, summary: &str) -> String {
        let id = crate::new_id();
        repo.lib.software.push(Software {
            id: id.clone(),
            name: name.into(),
            publisher: publisher.into(),
            homepage: String::new(),
            note: String::new(),
            category_id: None,
            starred: false,
            icon: None,
            icon_data: None,
            default_version_id: None,
            added_at: String::new(),
            versions: Vec::new(),
            subscription: None,
        });
        if !summary.is_empty() {
            repo.lib.summaries.insert(name.into(), Summary { text: summary.into(), source: "x".into(), ..Summary::default() });
        }
        id
    }

    fn category(repo: &mut Repo, id: &str, name: &str, parent: Option<&str>) {
        repo.lib.categories.push(Category { id: id.into(), name: name.into(), parent_id: parent.map(String::from), color: None, icon: None });
    }

    fn new_name(s: &Suggestion) -> Option<&str> {
        s.new_category.as_ref().map(|c| c.name.as_str())
    }

    #[test]
    fn word_matching() {
        assert!(mentions("a free ide for everyone", "ide"));
        assert!(!mentions("it provides a video player", "ide"));
        assert!(mentions("a code editor.", "code editor"));
        assert!(mentions("obsidian是笔记软件", "笔记"));
        assert!(mentions("3d computer graphics", "3d"));
        assert!(!mentions("graphicsx", "graphics"));
    }

    #[test]
    fn known_software_without_categories_proposes_new_ones() {
        let (_d, mut repo) = repo();
        let ids = vec![
            add(&mut repo, "Visual Studio Code", "Microsoft", ""),
            add(&mut repo, "Docker Desktop", "", ""),
            add(&mut repo, "微信", "腾讯", ""),
            add(&mut repo, "7-Zip", "Igor Pavlov", ""),
            add(&mut repo, "VLC", "VideoLAN", ""),
            add(&mut repo, "Mystery Tool 2000", "", ""),
        ];
        let out = repo.suggest_categories(&ids);
        let names: Vec<_> = out.iter().map(new_name).collect();
        assert_eq!(names, [Some("开发工具"), Some("开发工具"), Some("聊天社交"), Some("系统工具"), Some("影音媒体"), None]);
        assert!(out[0].known && out[5].category_id.is_none());
        let linear = add(&mut repo, "Linear", "", "");
        assert_eq!(new_name(&repo.suggest_categories(&[linear])[0]), None, "Linear 不是 LINE");

        repo.lib.settings.language = "en-US".into();
        assert_eq!(new_name(&repo.suggest_categories(&ids[..1])[0]), Some("Development"));
    }

    #[test]
    fn keywords_in_summaries_decide_unknown_software() {
        let (_d, mut repo) = repo();
        let shot = add(&mut repo, "Shotty", "", "A free, cross-platform screenshot tool with annotation.");
        let notes = add(&mut repo, "Inkdrop", "", "Inkdrop 是一款面向程序员的 Markdown 笔记应用。");
        let vague = add(&mut repo, "Foo", "", "It provides a nice experience.");
        let out = repo.suggest_categories(&[shot, notes, vague]);
        assert_eq!(new_name(&out[0]), Some("系统工具"));
        assert!(out[0].evidence.contains(&"screenshot".to_string()));
        assert_eq!(new_name(&out[1]), Some("办公效率"), "笔记和 markdown 应当压过“程序员”");
        assert_eq!(new_name(&out[2]), None, "provides 里的 ide 不能算数");
    }

    #[test]
    fn existing_categories_are_reused_and_refined() {
        let (_d, mut repo) = repo();
        category(&mut repo, "dev", "编程开发", None);
        category(&mut repo, "editors", "编辑器", Some("dev"));
        category(&mut repo, "media", "Video & Music", None);
        let code = add(&mut repo, "Visual Studio Code", "", "Visual Studio Code 是微软开发的跨平台源代码编辑器。");
        let git = add(&mut repo, "Git", "", "git 是一个分布式版本控制软件。");
        let vlc = add(&mut repo, "VLC", "", "");
        repo.lib.software.iter_mut().find(|s| s.id == git).unwrap().category_id = Some("dev".into());

        let out = repo.suggest_categories(&[code, git, vlc]);
        assert_eq!(out[0].category_id.as_deref(), Some("editors"), "简介里有“编辑器”，应当细到子分类");
        assert_eq!(out[1].category_id.as_deref(), Some("dev"));
        assert!(out[1].unchanged, "已经在推荐的分类里");
        assert_eq!(out[2].category_id.as_deref(), Some("media"), "英文名的分类也要认得");
        assert!(out.iter().all(|s| s.new_category.is_none()));

        // 已经放在下级分类里的（Python 在“运行环境”），不应被挪回上一级
        category(&mut repo, "runtime", "运行环境", Some("dev"));
        let python = add(&mut repo, "Python", "", "");
        repo.lib.software.iter_mut().find(|s| s.id == python).unwrap().category_id = Some("runtime".into());
        let kept = &repo.suggest_categories(&[python])[0];
        assert_eq!(kept.category_id.as_deref(), Some("runtime"));
        assert!(kept.unchanged);

        let before = repo.lib.clone();
        let targets = ["python", "GitHub Desktop", "Notion", "Unrecognized App"].map(|name| ImportCategoryTarget {
            name: name.into(),
            publisher: String::new(),
        });
        let previews = repo.preview_import_categories(&targets);
        assert_eq!(previews[0].category_id.as_deref(), Some("runtime"));
        assert_eq!(previews[1].category_id.as_deref(), Some("dev"));
        assert_eq!(previews[2].new_category.as_ref().map(|category| category.name.as_str()), Some("办公效率"));
        assert!(previews[3].category_id.is_none() && previews[3].new_category.is_none());
        assert_eq!(repo.lib, before);
    }

    #[test]
    fn applying_creates_each_new_category_once() {
        let (_d, mut repo) = repo();
        category(&mut repo, "sys", "系统工具", None);
        let a = add(&mut repo, "Notion", "", "");
        let b = add(&mut repo, "Obsidian", "", "");
        let c = add(&mut repo, "7-Zip", "", "");
        let new = || Some(NewCategory { name: "办公效率".into(), icon: "briefcase".into() });
        let assignments = vec![
            Assignment { software_id: a.clone(), category_id: None, new_category: new() },
            Assignment { software_id: b.clone(), category_id: None, new_category: new() },
            Assignment { software_id: c.clone(), category_id: Some("sys".into()), new_category: None },
            Assignment { software_id: "gone".into(), category_id: Some("sys".into()), new_category: None },
            Assignment { software_id: c.clone(), category_id: Some("nope".into()), new_category: None },
        ];
        assert_eq!(repo.apply_classification(assignments).unwrap(), 3);
        let office: Vec<_> = repo.lib.categories.iter().filter(|x| x.name == "办公效率").collect();
        assert_eq!(office.len(), 1, "同名分类只建一个");
        assert_eq!(office[0].icon.as_deref(), Some("briefcase"));
        let reopened = Repo::open(repo.root()).unwrap();
        let category_of = |id: &str| reopened.lib.software.iter().find(|s| s.id == id).unwrap().category_id.clone();
        assert_eq!(category_of(&a), Some(office[0].id.clone()));
        assert_eq!(category_of(&b), Some(office[0].id.clone()));
        assert_eq!(category_of(&c).as_deref(), Some("sys"));

        // 同名分类已经存在（大小写不同）时不再新建
        let d = add(&mut repo, "Typora", "", "");
        let again = vec![Assignment { software_id: d, category_id: None, new_category: Some(NewCategory { name: "  办公效率 ".into(), icon: "x".into() }) }];
        repo.apply_classification(again).unwrap();
        assert_eq!(repo.lib.categories.iter().filter(|x| x.name == "办公效率").count(), 1);
    }
}
