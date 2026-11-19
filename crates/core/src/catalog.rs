//! 离线目录：在仓库根目录生成一个 `index.html`。
//!
//! 新装的系统上还没有 ShibaPack 时，用浏览器打开它就能按分类和平台找到安装包并直接点开。
//! 它完全由索引生成，每次保存索引时重写，删掉也没关系。

use crate::{
    identify::compare_versions,
    model::{Library, PackageFile, Platform, Software, Storage},
    store::Repo,
};
use std::fmt::Write as _;

pub const CATALOG_FILE: &str = "index.html";

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// 相对路径或绝对路径 → 浏览器里能点的地址。
fn href(file: &PackageFile) -> String {
    let encode = |segment: &str| -> String {
        segment
            .bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || b"-_.~!$&'()*+,;=:@".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }
            })
            .collect()
    };
    let path = file.path.replace('\\', "/");
    let encoded = path.split('/').map(encode).collect::<Vec<_>>().join("/");
    match file.storage {
        Storage::Managed => encoded,
        // 仓库外的文件用 file:// 绝对地址。Windows 的盘符路径前面要多一个斜杠
        Storage::Indexed if encoded.starts_with('/') => format!("file://{encoded}"),
        Storage::Indexed => format!("file:///{encoded}"),
    }
}

fn platform_name(platform: Platform, zh: bool) -> &'static str {
    match platform {
        Platform::Windows => "Windows",
        Platform::Macos => "macOS",
        Platform::Linux => "Linux",
        Platform::Android => "Android",
        Platform::Ios => "iOS",
        Platform::Cross if zh => "跨平台",
        Platform::Cross => "Cross-platform",
    }
}

fn platform_key(platform: Platform) -> &'static str {
    match platform {
        Platform::Windows => "windows",
        Platform::Macos => "macos",
        Platform::Linux => "linux",
        Platform::Android => "android",
        Platform::Ios => "ios",
        Platform::Cross => "cross",
    }
}

fn size_text(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if value >= 100.0 { format!("{value:.0} {}", UNITS[unit]) } else { format!("{value:.1} {}", UNITS[unit]) }
}

/// "开发工具 / 编辑器" 这样的完整分类名。
fn category_path(lib: &Library, id: &str) -> String {
    let mut names = Vec::new();
    let mut current = lib.categories.iter().find(|c| c.id == id);
    while let Some(category) = current {
        names.insert(0, category.name.clone());
        if names.len() > 16 {
            break;
        }
        current = category.parent_id.as_deref().and_then(|parent| lib.categories.iter().find(|c| c.id == parent));
    }
    names.join(" / ")
}

/// 分类按树的顺序排成一列：父分类后面紧跟它的各级子分类。
fn tree_order<'a>(lib: &'a Library, parent: Option<&str>, out: &mut Vec<&'a str>, depth: usize) {
    if depth > 16 {
        return;
    }
    for category in lib.categories.iter().filter(|c| c.parent_id.as_deref() == parent) {
        out.push(&category.id);
        tree_order(lib, Some(&category.id), out, depth + 1);
    }
}

fn software_html(lib: &Library, software: &Software, zh: bool) -> String {
    let mut versions: Vec<_> = software.versions.iter().filter(|v| !v.files.is_empty()).collect();
    versions.sort_by(|a, b| compare_versions(&b.version, &a.version));
    let platforms: Vec<&str> = {
        let mut keys: Vec<&str> = versions.iter().flat_map(|v| &v.files).map(|f| platform_key(f.platform)).collect();
        keys.sort_unstable();
        keys.dedup();
        keys
    };
    let summary = lib.summaries.get(&software.name).map(|s| s.text.as_str()).filter(|t| !t.is_empty());
    let mut out = String::new();
    let _ = write!(
        out,
        r#"<article data-name="{}" data-platforms="{}"><h3>{}</h3>"#,
        escape(&format!("{} {}", software.name, software.publisher).to_lowercase()),
        platforms.join(" "),
        escape(&software.name),
    );
    if !software.publisher.is_empty() {
        let _ = write!(out, r#"<p class="by">{}</p>"#, escape(&software.publisher));
    }
    if let Some(text) = summary {
        let _ = write!(out, r#"<p class="about">{}</p>"#, escape(text));
    }
    for version in versions {
        let label = if version.version.is_empty() { if zh { "未知版本" } else { "Unknown version" } } else { &version.version };
        let _ = write!(out, r#"<div class="version"><span class="v">{}</span><ul>"#, escape(label));
        for file in &version.files {
            let arch = if file.arch.is_empty() { String::new() } else { format!(" · {}", escape(&file.arch)) };
            let _ = write!(
                out,
                r#"<li data-platform="{}"><a href="{}">{}</a><span class="meta">{}{} · {}</span></li>"#,
                platform_key(file.platform),
                escape(&href(file)),
                escape(&file.name),
                platform_name(file.platform, zh),
                arch,
                size_text(file.size),
            );
        }
        out.push_str("</ul></div>");
    }
    out.push_str("</article>");
    out
}

/// 生成整页 HTML。`generated_at` 是页脚显示的生成时间。
pub fn render(lib: &Library, generated_at: &str) -> String {
    let zh = lib.settings.language.to_lowercase().starts_with("zh");
    let t = |zh_text: &'static str, en_text: &'static str| if zh { zh_text } else { en_text };

    // 按分类树的顺序分组，没有分类的放最后
    let mut sections = String::new();
    let mut ordered: Vec<(String, Vec<&Software>)> = Vec::new();
    let mut order: Vec<&str> = Vec::new();
    tree_order(lib, None, &mut order, 0);
    let by_name = |a: &&Software, b: &&Software| a.name.to_lowercase().cmp(&b.name.to_lowercase());
    // 只列有安装包的软件：只关联了在线源、还没下载过的，在这里没有东西可点
    let listed: Vec<&Software> = lib.software.iter().filter(|s| s.versions.iter().any(|v| !v.files.is_empty())).collect();
    for id in order {
        let mut items: Vec<&Software> = listed.iter().copied().filter(|s| s.category_id.as_deref() == Some(id)).collect();
        items.sort_by(by_name);
        if !items.is_empty() {
            ordered.push((category_path(lib, id), items));
        }
    }
    let mut rest: Vec<&Software> = listed
        .iter()
        .copied()
        .filter(|s| s.category_id.as_deref().is_none_or(|id| !lib.categories.iter().any(|c| c.id == id)))
        .collect();
    rest.sort_by(by_name);
    if !rest.is_empty() {
        ordered.push((t("未分类", "Uncategorized").to_string(), rest));
    }
    let mut total_files = 0;
    for (title, items) in &ordered {
        let _ = write!(sections, r#"<section><h2>{}</h2><div class="grid">"#, escape(title));
        for software in items {
            total_files += software.versions.iter().map(|v| v.files.len()).sum::<usize>();
            sections.push_str(&software_html(lib, software, zh));
        }
        sections.push_str("</div></section>");
    }

    let present: Vec<Platform> = [Platform::Windows, Platform::Macos, Platform::Linux, Platform::Android, Platform::Ios, Platform::Cross]
        .into_iter()
        .filter(|p| lib.software.iter().flat_map(|s| &s.versions).flat_map(|v| &v.files).any(|f| f.platform == *p))
        .collect();
    let mut filters = format!(r#"<button data-p="" class="on">{}</button>"#, t("全部", "All"));
    for platform in present {
        let _ = write!(filters, r#"<button data-p="{}">{}</button>"#, platform_key(platform), platform_name(platform, zh));
    }

    TEMPLATE
        .replace("{{lang}}", if zh { "zh-CN" } else { "en" })
        .replace("{{title}}", t("安装包目录", "Installer catalog"))
        .replace("{{search}}", t("搜索软件…", "Search software…"))
        .replace("{{filters}}", &filters)
        .replace("{{sections}}", &sections)
        .replace("{{empty}}", t("没有符合条件的软件", "Nothing matches"))
        .replace(
            "{{footer}}",
            &escape(&if zh {
                format!("{} 个软件，{total_files} 个安装包 · 由 ShibaPack 生成于 {generated_at}", listed.len())
            } else {
                format!("{} apps, {total_files} installers · generated by ShibaPack on {generated_at}", listed.len())
            }),
        )
}

impl Repo {
    /// 重写根目录的离线目录。失败不算错误：它只是索引的一份副本。
    pub(crate) fn write_catalog(&self) {
        let path = self.root().join(CATALOG_FILE);
        // 根目录里如果本来就有一个别人的 index.html，不碰它
        let existing = std::fs::read(&path).ok();
        let ours = existing.as_deref().is_none_or(|bytes| bytes.windows(MARKER.len()).any(|w| w == MARKER.as_bytes()));
        if !ours {
            return;
        }
        if !self.lib.settings.offline_catalog {
            // 关掉这个功能后，把以前生成的那份也收走，免得它越来越过时
            if existing.is_some() {
                let _ = std::fs::remove_file(&path);
            }
            return;
        }
        let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
        let _ = std::fs::write(path, render(&self.lib, &stamp));
    }
}

/// 页面里的这个标记说明它是 ShibaPack 生成的，可以放心覆盖或删除。
const MARKER: &str = r#"<meta name="generator" content="ShibaPack">"#;

const TEMPLATE: &str = r##"<!doctype html>
<html lang="{{lang}}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="generator" content="ShibaPack">
<title>{{title}}</title>
<style>
:root { color-scheme: light dark; --bg:#fff; --fg:#1b1c1f; --fg2:#5c5f67; --fg3:#8a8d95; --line:rgba(0,0,0,.1); --card:#f6f6f7; --accent:#1f6fe0; }
@media (prefers-color-scheme: dark) { :root { --bg:#0f1012; --fg:#ececee; --fg2:#a4a7ae; --fg3:#70747c; --line:rgba(255,255,255,.1); --card:#18191c; --accent:#5aa2ff; } }
* { box-sizing: border-box; }
body { margin:0; background:var(--bg); color:var(--fg); font:14px/1.5 -apple-system,BlinkMacSystemFont,"Segoe UI","PingFang SC","Microsoft YaHei",sans-serif; }
header { position:sticky; top:0; z-index:1; display:flex; flex-wrap:wrap; gap:10px 14px; align-items:center; padding:12px 20px; background:var(--bg); border-bottom:1px solid var(--line); }
h1 { margin:0 8px 0 0; font-size:16px; }
input { flex:1; min-width:160px; max-width:320px; height:32px; padding:0 10px; border:1px solid var(--line); border-radius:7px; background:var(--card); color:inherit; font:inherit; outline:none; }
input:focus { border-color:var(--accent); }
.filters { display:flex; flex-wrap:wrap; gap:4px; }
button { height:28px; padding:0 10px; border:1px solid var(--line); border-radius:7px; background:none; color:var(--fg2); font:inherit; font-size:13px; cursor:pointer; }
button.on { background:var(--accent); border-color:var(--accent); color:#fff; }
main { max-width:1180px; margin:0 auto; padding:4px 20px 40px; }
h2 { margin:26px 0 10px; padding-bottom:6px; border-bottom:1px solid var(--line); font-size:13px; font-weight:600; color:var(--fg2); }
.grid { display:grid; grid-template-columns:repeat(auto-fill,minmax(300px,1fr)); gap:10px; }
article { padding:12px 14px; border:1px solid var(--line); border-radius:10px; background:var(--card); min-width:0; }
h3 { margin:0; font-size:14px; }
.by { margin:0; color:var(--fg3); font-size:12px; }
.about { margin:6px 0 0; color:var(--fg2); font-size:12.5px; }
.version { margin-top:10px; }
.v { font-size:12px; font-weight:600; color:var(--fg2); font-variant-numeric:tabular-nums; }
ul { list-style:none; margin:2px 0 0; padding:0; }
li { display:flex; flex-wrap:wrap; align-items:baseline; gap:0 8px; padding:3px 0; border-top:1px solid var(--line); }
a { color:var(--accent); text-decoration:none; overflow-wrap:anywhere; }
a:hover { text-decoration:underline; }
.meta { margin-left:auto; color:var(--fg3); font-size:12px; white-space:nowrap; }
#empty { display:none; padding:60px 0; text-align:center; color:var(--fg3); }
footer { padding:0 20px 30px; text-align:center; color:var(--fg3); font-size:12px; }
[hidden] { display:none !important; }
</style>
</head>
<body>
<header><h1>{{title}}</h1><input id="q" type="search" placeholder="{{search}}" autocomplete="off"><div class="filters" id="f">{{filters}}</div></header>
<main>{{sections}}<p id="empty">{{empty}}</p></main>
<footer>{{footer}}</footer>
<script>
(function () {
  var query = document.getElementById("q"), filters = document.getElementById("f"), platform = "";
  function apply() {
    var text = query.value.trim().toLowerCase(), shown = 0;
    document.querySelectorAll("section").forEach(function (section) {
      var any = false;
      section.querySelectorAll("article").forEach(function (card) {
        var ok = (!text || card.dataset.name.indexOf(text) >= 0) && (!platform || card.dataset.platforms.split(" ").indexOf(platform) >= 0);
        card.hidden = !ok;
        // 选了平台时，只留下那个平台的文件
        card.querySelectorAll("li").forEach(function (row) { row.hidden = !!platform && row.dataset.platform !== platform; });
        card.querySelectorAll(".version").forEach(function (v) { v.hidden = !v.querySelector("li:not([hidden])"); });
        if (ok) { any = true; shown++; }
      });
      section.hidden = !any;
    });
    document.getElementById("empty").style.display = shown ? "none" : "block";
  }
  query.addEventListener("input", apply);
  filters.addEventListener("click", function (event) {
    var button = event.target.closest("button");
    if (!button) return;
    platform = button.dataset.p;
    filters.querySelectorAll("button").forEach(function (b) { b.classList.toggle("on", b === button); });
    apply();
  });
  apply();
})();
</script>
</body>
</html>
"##;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn file(name: &str, platform: Platform, storage: Storage, path: &str) -> PackageFile {
        PackageFile { id: name.into(), name: name.into(), platform, arch: "x64".into(), size: 5 * 1024 * 1024, sha256: String::new(), storage, path: path.into(), added_at: String::new() }
    }

    fn sample() -> Library {
        let mut lib = Library::default();
        lib.categories = vec![
            Category { id: "dev".into(), name: "开发工具".into(), parent_id: None, color: None, icon: None },
            Category { id: "ed".into(), name: "编辑器".into(), parent_id: Some("dev".into()), color: None, icon: None },
        ];
        let software = |name: &str, category: Option<&str>, versions: Vec<Version>| Software {
            id: name.into(), name: name.into(), publisher: "ACME <Inc>".into(), homepage: String::new(), note: String::new(),
            category_id: category.map(String::from), tag_ids: vec![], icon: None, default_version_id: None, added_at: String::new(), versions, subscription: None,
        };
        let version = |v: &str, files: Vec<PackageFile>| Version { id: v.into(), version: v.into(), note: String::new(), added_at: String::new(), files };
        lib.software = vec![
            software("VS Code", Some("ed"), vec![
                version("1.9", vec![file("old.exe", Platform::Windows, Storage::Managed, "VS Code/1.9/old.exe")]),
                version("1.10", vec![
                    file("Setup 版.exe", Platform::Windows, Storage::Managed, "VS Code/1.10/Setup 版.exe"),
                    file("Code.dmg", Platform::Macos, Storage::Indexed, "/Volumes/NAS/Code.dmg"),
                    file("Code.msi", Platform::Windows, Storage::Indexed, r"D:\Soft\Code.msi"),
                ]),
            ]),
            software("Loose Tool", None, vec![version("", vec![file("tool.zip", Platform::Cross, Storage::Managed, "Loose Tool/unknown/tool.zip")])]),
        ];
        // 只关联了在线源、一个安装包都没有的软件，不应出现在目录里
        lib.software.push(software("Empty Shell", None, vec![]));
        lib.summaries.insert("VS Code".into(), Summary { text: "编辑器 & 更多".into(), source: "x".into(), fetched_at: String::new() });
        lib
    }

    #[test]
    fn renders_links_groups_and_escapes() {
        let html = render(&sample(), "2026-11-19 12:00");
        assert!(html.contains(MARKER));
        assert!(html.contains("<h2>开发工具 / 编辑器</h2>"));
        assert!(html.contains("<h2>未分类</h2>"));
        // 托管文件用相对地址，空格和中文都转义；仓库外的文件用 file:// 地址
        assert!(html.contains(r#"href="VS%20Code/1.10/Setup%20%E7%89%88.exe""#), "{html}");
        assert!(html.contains(r#"href="file:///Volumes/NAS/Code.dmg""#));
        assert!(html.contains(r#"href="file:///D:/Soft/Code.msi""#));
        assert!(html.contains("ACME &lt;Inc&gt;") && html.contains("编辑器 &amp; 更多"));
        // 新版本排在旧版本前面
        assert!(html.find(">1.10<").unwrap() < html.find(">1.9<").unwrap());
        assert!(html.contains("未知版本") && html.contains("5.0 MB"));
        assert!(html.contains(r#"data-p="macos""#) && !html.contains(r#"data-p="linux""#));
        assert!(html.contains("2 个软件，5 个安装包"));
        assert!(!html.contains("Empty Shell"));
    }

    #[test]
    fn repo_writes_and_removes_the_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let mut repo = Repo::open(dir.path()).unwrap();
        let page = dir.path().join(CATALOG_FILE);
        assert!(page.is_file(), "打开仓库时就应当生成");
        repo.lib.settings.offline_catalog = false;
        repo.save().unwrap();
        assert!(!page.exists(), "关掉后应当删除生成的那份");
        // 不是 ShibaPack 生成的 index.html 不能动：关着的时候不删，开着的时候不覆盖
        std::fs::write(&page, "<html>mine</html>").unwrap();
        repo.save().unwrap();
        repo.lib.settings.offline_catalog = true;
        repo.save().unwrap();
        assert_eq!(std::fs::read_to_string(&page).unwrap(), "<html>mine</html>");
    }
}
