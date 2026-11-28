//! 导入安装包：识别、算哈希、去重，按存储模式复制进仓库或只记路径。

use crate::{
    fileinfo::{self, FileInfo},
    icons,
    identify::{identify, identify_folder, is_installer, normalize_name, same_software, Identity},
    model::*,
    new_id, now,
    store::{clean_path, sanitize, unique_path, Repo, DATA_DIR},
    AtStep, Error, Result,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

mod transfer;
use transfer::{checkpoint, copy_file, remove_staging};

/// 扫描文件夹时最多往下走几层。
const MAX_SCAN_DEPTH: usize = 6;
/// 导入时每新登记这么多个文件，或者离上次保存过了这么久，就先存一次索引。
const SAVE_EVERY_FILES: usize = 20;
const SAVE_EVERY: Duration = Duration::from_secs(30);

/// 导入时手动指定的信息，覆盖从文件名识别的结果。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ImportHint {
    pub name: Option<String>,
    pub version: Option<String>,
    pub platform: Option<Platform>,
    /// 放进哪个分类：新建的软件放进去，已有的软件还没分类的也放进去，已经分好类的不动。
    pub category_id: Option<String>,
    pub auto_classify: Option<bool>,
    /// 导入前已确认的分类预览，按原始路径记录。
    pub category_previews: Option<HashMap<String, crate::classify::ImportCategoryPreview>>,
    /// 当作一个安装包整个导入的文件夹（绿色版软件、带着一堆文件的安装程序）。其余文件夹照旧展开成里面的安装包。
    pub packages: Vec<String>,
    /// 在导入窗口确认的名称和版本，按原始路径记录。
    pub entries: HashMap<String, ImportEntryEdit>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImportEntryEdit {
    pub name: String,
    pub version: String,
}

impl ImportHint {
    /// 当作一个安装包的文件夹，去掉了符号链接。
    pub fn package_set(&self) -> HashSet<PathBuf> {
        self.packages.iter().map(|p| fs::canonicalize(p).map(|p| clean_path(&p)).unwrap_or_else(|_| PathBuf::from(p))).collect()
    }

    pub fn uses_auto_classification(&self, settings: &Settings) -> bool {
        self.category_id.is_none() && self.auto_classify.unwrap_or(settings.auto_classify_on_import)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// collect：正在找文件；import：正在处理第 `current` 个；done：结束。
    pub phase: String,
    pub current: usize,
    pub total: usize,
    pub file: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportItem {
    pub path: String,
    /// imported / duplicate / failed
    pub status: String,
    pub software_id: Option<String>,
    /// 失败时系统或程序给的原话。
    pub message: Option<String>,
    /// 失败在哪一步：read（读取、算校验值）、copy（复制或移动进仓库）、save（登记进索引）。
    pub step: Option<String>,
    /// 失败的原因代码：permission_denied、disk_full、in_use、file_missing、copy_mismatch……归不了类的是 io。
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub imported: usize,
    pub duplicates: usize,
    pub failed: usize,
    pub items: Vec<ImportItem>,
    /// 用户中途取消了：后面的文件没有处理。
    pub cancelled: bool,
    pub classified: usize,
    pub classification_error: Option<String>,
}

/// 一个文件导入后的结果：登记好了，或者是重复的。两种都带着对应软件的 id。
pub enum Outcome {
    Imported(String),
    Duplicate(String),
}

/// 一个准备导入的文件（或当作安装包的文件夹），最慢的一步（算哈希）已经做完。这一步只读文件本身，不碰索引。
pub struct Hashed {
    /// 原始路径，与导入确认窗口返回的路径一致。
    pub source: PathBuf,
    /// 文件的绝对路径（去掉了符号链接）。
    pub abs: PathBuf,
    pub file_name: String,
    pub size: u64,
    pub sha256: String,
    /// 是一个文件夹（整个当作一个安装包）。
    pub is_dir: bool,
    /// 文件自带的产品名、版本号、发布者（exe、msi；文件夹取里面的主程序）。文件名里看不出来时用它补。
    pub info: Option<FileInfo>,
    /// 从文件夹里面的东西看出来的平台（有 .exe 是 Windows，是 .app 是 macOS）。
    pub platform: Option<Platform>,
}

/// 读一个待导入的文件、算哈希。不需要占着仓库，可以在别的操作照常进行时做。
/// 文件夹整个当作一个安装包：哈希按里面每个文件的相对路径和内容算，大小是全部文件加起来。
pub fn hash_source(src: &Path) -> Result<Hashed> {
    hash_source_cancellable(src, &|| false)
}

pub fn hash_source_cancellable(src: &Path, cancelled: &dyn Fn() -> bool) -> Result<Hashed> {
    checkpoint(cancelled)?;
    let meta = fs::metadata(src)?;
    let abs = clean_path(&fs::canonicalize(src)?);
    let file_name = abs.file_name().unwrap_or_default().to_string_lossy().to_string();
    if meta.is_dir() {
        let (size, sha256) = hash_folder_cancellable(&abs, cancelled)?;
        let (info, platform) = folder_info(&abs);
        checkpoint(cancelled)?;
        return Ok(Hashed { source: src.to_path_buf(), abs, file_name, size, sha256, is_dir: true, info, platform });
    }
    if !meta.is_file() {
        return Err(Error::Code("not_a_file"));
    }
    let sha256 = sha256_file_cancellable(&abs, cancelled)?;
    let info = fileinfo::read(&abs);
    checkpoint(cancelled)?;
    Ok(Hashed { source: src.to_path_buf(), abs, file_name, size: meta.len(), sha256, is_dir: false, info, platform: None })
}

/// 文件夹的哈希：按相对路径排好，每个文件的“相对路径 + 内容哈希”串起来再算一次。和文件放在哪无关，
/// 内容一样的文件夹哈希就一样（查重复用）。返回（总大小，哈希）。
pub fn hash_folder(dir: &Path) -> Result<(u64, String)> {
    hash_folder_cancellable(dir, &|| false)
}

pub(crate) fn hash_folder_cancellable(dir: &Path, cancelled: &dyn Fn() -> bool) -> Result<(u64, String)> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(dir).follow_links(false) {
        checkpoint(cancelled)?;
        if let Ok(entry) = entry {
            if entry.file_type().is_file() {
                files.push((to_rel(entry.path().strip_prefix(dir).unwrap_or(entry.path())), entry.into_path()));
            }
        }
    }
    files.sort();
    let mut hasher = Sha256::new();
    let mut size = 0;
    for (rel, path) in &files {
        size += fs::metadata(path)?.len();
        hasher.update(rel.as_bytes());
        hasher.update([0]);
        hasher.update(sha256_file_cancellable(path, cancelled)?.as_bytes());
        hasher.update([0]);
    }
    Ok((size, hex::encode(hasher.finalize())))
}

/// 文件夹里所有文件加起来的大小（校验时用）。
pub fn folder_size(dir: &Path) -> u64 {
    walkdir::WalkDir::new(dir).into_iter().filter_map(|e| e.ok()).filter(|e| e.file_type().is_file()).filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum()
}

pub(crate) fn folder_size_cancellable(dir: &Path, cancelled: &dyn Fn() -> bool) -> Result<u64> {
    let mut size = 0;
    for entry in walkdir::WalkDir::new(dir).follow_links(false) {
        checkpoint(cancelled)?;
        let entry = entry.map_err(std::io::Error::other)?;
        if entry.file_type().is_file() {
            size += entry.metadata().map_err(std::io::Error::other)?.len();
        }
    }
    Ok(size)
}

/// 是不是 macOS 的应用（`.app` 文件夹）。它本身就是一个整体，导入时不往里面找安装包。
pub fn is_app_bundle(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("app")) && path.is_dir()
}

/// 文件夹看起来是不是一个软件（应该整个当作一个安装包）：是 .app；或者里面第一层有程序（.exe）还带着它的
/// 库文件（.dll），这是绿色版软件和“解压后运行 setup”的安装程序的样子；或者第一层就是一个 .app。
/// 一堆安装包放在一起的文件夹（只有 exe、msi、dmg……没有 dll）不算，照旧展开。
pub fn looks_like_package(dir: &Path) -> bool {
    if is_app_bundle(dir) {
        return true;
    }
    let Ok(entries) = fs::read_dir(dir) else { return false };
    let names: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    let ext_is = |p: &PathBuf, ext: &str| p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext));
    let has_exe = names.iter().any(|p| p.is_file() && ext_is(p, "exe"));
    let has_dll = names.iter().any(|p| p.is_file() && ext_is(p, "dll"));
    let apps = names.iter().filter(|p| is_app_bundle(p)).count();
    (has_exe && has_dll) || (apps == 1 && names.iter().filter(|p| is_installer(&p.file_name().unwrap_or_default().to_string_lossy())).count() <= 1)
}

/// 文件夹里的主程序：.app 本身；第一层里和文件夹同名的 exe，没有时最大的那个（卸载程序、更新程序不算）；
/// 第一层的一个 .app。
pub fn main_program(dir: &Path) -> Option<PathBuf> {
    if is_app_bundle(dir) {
        return Some(dir.to_path_buf());
    }
    let entries: Vec<PathBuf> = fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).collect();
    if let Some(app) = entries.iter().find(|p| is_app_bundle(p)) {
        return Some(app.clone());
    }
    let folder = normalize_name(&dir.file_name().unwrap_or_default().to_string_lossy());
    let mut exes: Vec<(PathBuf, u64)> = entries
        .into_iter()
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")))
        .filter(|p| {
            let stem = p.file_stem().unwrap_or_default().to_string_lossy().to_lowercase();
            !stem.starts_with("unins") && !stem.contains("uninstall") && !stem.contains("update") && !stem.contains("crash")
        })
        .filter_map(|p| fs::metadata(&p).ok().map(|m| (p, m.len())))
        .collect();
    exes.sort_by(|a, b| b.1.cmp(&a.1));
    let by_name = exes.iter().find(|(p, _)| {
        let stem = normalize_name(&p.file_stem().unwrap_or_default().to_string_lossy());
        !stem.is_empty() && (folder.contains(&stem) || stem.contains(&folder))
    });
    by_name.or(exes.first()).map(|(p, _)| p.clone())
}

/// 文件夹里的主程序带着的信息，和看得出来的平台。
fn folder_info(dir: &Path) -> (Option<FileInfo>, Option<Platform>) {
    let Some(main) = main_program(dir) else { return (None, None) };
    if is_app_bundle(&main) {
        let plist = plist::Value::from_file(main.join("Contents/Info.plist")).ok().and_then(plist::Value::into_dictionary);
        let text = |key: &str| plist.as_ref().and_then(|d| d.get(key)).and_then(plist::Value::as_string).map(|s| s.trim().to_string()).unwrap_or_default();
        let name = Some(text("CFBundleDisplayName")).filter(|s| !s.is_empty()).unwrap_or_else(|| text("CFBundleName"));
        let mut version = text("CFBundleShortVersionString");
        if version.is_empty() {
            version = text("CFBundleVersion");
        }
        let company = crate::installed::publisher_from_copyright(&text("NSHumanReadableCopyright"));
        return (Some(FileInfo { product_name: name, version: fileinfo::clean_version(&version), company }), Some(Platform::Macos));
    }
    (fileinfo::read(&main), Some(Platform::Windows))
}

/// 文件夹当作安装包时用来取图标的文件：.app 的图标、主程序 exe，或者第一层的 .ico / .icns / .png。
pub fn icon_source_in(dir: &Path) -> Option<PathBuf> {
    if let Some(main) = main_program(dir) {
        if is_app_bundle(&main) {
            let plist = plist::Value::from_file(main.join("Contents/Info.plist")).ok().and_then(plist::Value::into_dictionary);
            let declared = plist.as_ref().and_then(|d| d.get("CFBundleIconFile")).and_then(plist::Value::as_string).map(str::to_string);
            if let Some(icon) = crate::installed::bundle_icon(&main, declared.as_deref()) {
                return Some(icon);
            }
        } else {
            return Some(main);
        }
    }
    let entries: Vec<PathBuf> = fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).collect();
    ["ico", "icns", "png"].iter().find_map(|ext| entries.iter().find(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext))).cloned())
}

/// 拖进来的一项，导入前给确认窗口看的样子。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DropEntry {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    /// 文件的大小。文件夹不算（NAS 上的大文件夹要走很久）。
    pub size: Option<u64>,
    /// 文件夹看起来是一个软件（绿色版、.app），建议整个当作一个安装包导入。
    pub package: bool,
    /// 文件夹里能找到的安装包个数（展开导入时会导入的那些）。
    pub installers: usize,
    /// 文件是不是认得的安装包。
    pub installer: bool,
    /// 按文件名和文件属性认出来的软件名、版本号（当作一个安装包导入时）。
    pub guess_name: String,
    pub guess_version: String,
    pub filename_version: String,
    pub metadata_version: String,
    pub matched_software_id: Option<String>,
    pub publisher: String,
    pub icon_data: Option<Arc<str>>,
    /// 普通文件夹选择展开导入时的安装包。
    pub children: Vec<DropEntry>,
}

/// 看一看拖进来的东西：每一项是文件还是文件夹、文件夹像不像一个软件、里面有几个安装包、会被认成什么软件。
pub fn inspect(paths: &[PathBuf], data_dir: &Path, software_names: &[(String, String)], include_children: bool) -> Vec<DropEntry> {
    paths.iter().map(|path| {
        let mut entry = inspect_entry(path, software_names, include_children);
        if entry.is_dir && !is_app_bundle(path) {
            let files = collect_files(std::slice::from_ref(path), data_dir, &HashSet::new());
            entry.installers = files.len();
            if include_children {
                entry.children = files.iter().map(|file| inspect_entry(file, software_names, true)).collect();
            }
        }
        entry
    }).collect()
}

fn inspect_entry(path: &Path, software_names: &[(String, String)], preview: bool) -> DropEntry {
    let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let is_dir = path.is_dir();
    let (info, platform) = if is_dir { folder_info(path) } else { (fileinfo::read(path), None) };
    let mut identity = filename_identity(&name, is_dir);
    let filename_version = identity.version.clone();
    let metadata_version = info.as_ref().map_or_else(String::new, |info| info.version.clone());
    fill_identity(&mut identity, info.as_ref(), platform);
    DropEntry {
        path: path.to_string_lossy().into_owned(),
        is_dir,
        size: (!is_dir).then(|| fs::metadata(path).ok().map(|meta| meta.len())).flatten(),
        package: is_dir && looks_like_package(path),
        installers: 0,
        installer: !is_dir && is_installer(&name),
        matched_software_id: match_import_name(&identity.name, software_names),
        guess_name: identity.name,
        guess_version: identity.version,
        filename_version,
        metadata_version,
        publisher: info.map_or_else(String::new, |info| info.company.trim().to_owned()),
        icon_data: preview.then(|| icon_source(path).and_then(|source| icons::preview_icon(&source))).flatten(),
        name,
        children: Vec::new(),
    }
}

fn match_import_name(name: &str, software_names: &[(String, String)]) -> Option<String> {
    let key = normalize_name(name);
    if key.is_empty() {
        return None;
    }
    if let Some((id, _)) = software_names.iter().find(|(_, existing)| normalize_name(existing) == key) {
        return Some(id.clone());
    }
    let mut matches = software_names.iter().filter(|(_, existing)| same_software(name, existing));
    let (id, _) = matches.next()?;
    matches.next().is_none().then(|| id.clone())
}

fn filename_identity(name: &str, is_dir: bool) -> Identity {
    if is_dir { identify_folder(name) } else { identify(name) }
}

fn fill_identity(identity: &mut Identity, info: Option<&FileInfo>, platform: Option<Platform>) {
    if let Some(info) = info {
        if identity.version.is_empty() {
            identity.version = info.version.clone();
        }
        if generic_name(&identity.name) && !info.product_name.trim().is_empty() {
            identity.name = info.product_name.trim().to_string();
        }
    }
    if identity.platform == Platform::Cross {
        if let Some(platform) = platform {
            identity.platform = platform;
        }
    }
}

/// 安装包取图标用的文件：文件本身；文件夹当作安装包时取里面的主程序或图标文件。
pub fn icon_source(path: &Path) -> Option<PathBuf> {
    if path.is_dir() {
        icon_source_in(path)
    } else {
        path.is_file().then(|| path.to_path_buf())
    }
}

/// 打开一个安装包时实际打开的东西：文件本身；文件夹里的主程序（.app 本身也是）；找不到主程序时是文件夹自己。
pub fn launch_target(path: &Path) -> PathBuf {
    if path.is_dir() {
        main_program(path).unwrap_or_else(|| path.to_path_buf())
    } else {
        path.to_path_buf()
    }
}

/// 把整个文件夹复制过去（不跟符号链接，链接本身照原样复制）。
fn copy_dir(src: &Path, dest: &Path, cancelled: &dyn Fn() -> bool) -> Result<()> {
    fs::create_dir_all(dest)?;
    for entry in walkdir::WalkDir::new(src).min_depth(1).follow_links(false) {
        checkpoint(cancelled)?;
        let entry = entry.map_err(std::io::Error::other)?;
        let target = dest.join(entry.path().strip_prefix(src).unwrap_or(entry.path()));
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else if entry.file_type().is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(fs::read_link(entry.path())?, &target)?;
        } else if entry.file_type().is_file() {
            copy_file(entry.path(), &target, cancelled)?;
        } else {
            return Err(Error::Code("not_a_file"));
        }
    }
    Ok(())
}

/// 导入一个文件前做的决定：是重复的（带着已有软件的 id），或者要登记成什么、放在哪。
pub enum Plan {
    Duplicate(String),
    Add(Placement),
}

pub struct Placement {
    /// 从文件名认出（再按手动指定的信息改过）的软件名、版本、平台、架构。
    identity: Identity,
    /// 新建软件时的发布者（文件自带的公司名）。
    publisher: String,
    /// 放进哪个分类（手动指定的）。
    category: Option<String>,
    /// 放在哪个文件夹下：仓库里已经有这个软件时用它现有的名字（可能被改过）。
    folder: String,
    target: Target,
}

enum Target {
    /// 文件本来就在仓库里：就地登记，值是相对路径。
    InRepo(String),
    /// 复制（或移动）进仓库。
    Managed,
    /// 只记录位置。
    Indexed,
}

/// 文件放好之后的样子：存储方式、写进索引的路径、磁盘上的实际位置、是不是这次新放进去的。
pub struct Placed {
    storage: Storage,
    path: String,
    disk: PathBuf,
    created: bool,
}

/// 把展开好的文件列表收集出来：文件原样保留，文件夹里只取安装包。不进隐藏文件夹，也不进仓库自己的数据目录。
/// `packages` 里的文件夹和 .app 整个当作一个安装包，不往里面找。
pub fn collect_files(paths: &[PathBuf], data_dir: &Path, packages: &HashSet<PathBuf>) -> Vec<PathBuf> {
    collect_files_cancellable(paths, data_dir, packages, &|| false).unwrap_or_default()
}

pub fn collect_files_cancellable(paths: &[PathBuf], data_dir: &Path, packages: &HashSet<PathBuf>, cancelled: &dyn Fn() -> bool) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for path in paths {
        checkpoint(cancelled)?;
        let as_package = path.is_dir() && (is_app_bundle(path) || packages.contains(&fs::canonicalize(path).map(|p| clean_path(&p)).unwrap_or_else(|_| path.clone())));
        if as_package {
            files.push(path.clone());
        } else if path.is_dir() {
            let mut walker = walkdir::WalkDir::new(path).max_depth(MAX_SCAN_DEPTH).into_iter().filter_entry(|entry| {
                let hidden = entry.depth() > 0 && entry.file_name().to_string_lossy().starts_with('.');
                !hidden && !entry.path().starts_with(data_dir) && entry.file_name() != DATA_DIR
            });
            while let Some(entry) = walker.next() {
                checkpoint(cancelled)?;
                let Ok(entry) = entry else { continue };
                // 文件夹里的 .app 是一个整体，不往里面走
                if entry.depth() > 0 && entry.file_type().is_dir() && is_app_bundle(entry.path()) {
                    walker.skip_current_dir();
                    files.push(entry.into_path());
                } else if entry.file_type().is_file() && is_installer(&entry.file_name().to_string_lossy()) {
                    files.push(entry.into_path());
                }
            }
        } else {
            files.push(path.clone());
        }
    }
    files.sort();
    files.dedup();
    checkpoint(cancelled)?;
    Ok(files)
}

/// 复制（或移动）进仓库的 `软件名/版本号/原文件名`。复制时先写 `.part` 再改名，中途失败不会留下半个文件。
/// 不碰索引，只需要仓库根目录，可以在不占着仓库的情况下做。
pub fn place_file(root: &Path, hashed: &Hashed, placement: &Placement, move_file: bool) -> Result<Placed> {
    place_file_cancellable(root, hashed, placement, move_file, &|| false)
}

pub fn place_file_cancellable(
    root: &Path, hashed: &Hashed, placement: &Placement, move_file: bool, cancelled: &dyn Fn() -> bool,
) -> Result<Placed> {
    checkpoint(cancelled)?;
    let src = &hashed.abs;
    match &placement.target {
        Target::InRepo(rel) => return Ok(Placed { storage: Storage::Managed, path: rel.clone(), disk: src.clone(), created: false }),
        Target::Indexed => {
            return Ok(Placed { storage: Storage::Indexed, path: src.to_string_lossy().to_string(), disk: src.clone(), created: false })
        }
        Target::Managed => {}
    }
    let version = &placement.identity.version;
    let version_dir = if version.is_empty() { "unknown".to_string() } else { sanitize(version) };
    let dir = root.join(sanitize(&placement.folder)).join(version_dir);
    fs::create_dir_all(&dir)?;
    let mut dest = dir.join(&hashed.file_name);
    let managed = |dest: PathBuf, created: bool| {
        let path = dest.strip_prefix(root).map(to_rel).unwrap_or_default();
        Placed { storage: Storage::Managed, path, disk: dest, created }
    };
    if dest.exists() {
        // 之前删了记录但没删掉的同一份安装包可以直接登记。
        match contents_match(&dest, hashed, cancelled) {
            Ok(true) => return Ok(managed(dest, false)),
            Err(err) if err.reason() == "cancelled" => return Err(err),
            _ => dest = unique_path(dest),
        }
    }

    // 同盘改名是原子的。从成功改名起必须登记，避免源文件已经移走却找不到记录。
    checkpoint(cancelled)?;
    if move_file && fs::rename(src, &dest).is_ok() {
        return Ok(managed(dest, true));
    }

    // 复制和跨盘校验只碰本次导入的临时目录。最后一次取消检查过后才提交。
    let staging = dir.join(format!(".import-{}.part", uuid::Uuid::new_v4()));
    fs::create_dir(&staging)?;
    let payload = staging.join("payload");
    let result = (|| -> Result<()> {
        if hashed.is_dir {
            copy_dir(src, &payload, cancelled)?;
        } else {
            copy_file(src, &payload, cancelled)?;
            sync_import_copy(&payload)?;
        }
        if move_file && !contents_match(&payload, hashed, cancelled)? {
            return Err(Error::Code("copy_mismatch"));
        }
        checkpoint(cancelled)?;
        fs::rename(&payload, &dest)?;
        Ok(())
    })();
    remove_staging(&staging);
    result?;
    if move_file {
        if hashed.is_dir {
            let _ = fs::remove_dir_all(src);
        } else {
            let _ = fs::remove_file(src);
        }
    }
    Ok(managed(dest, true))
}

fn contents_match(path: &Path, hashed: &Hashed, cancelled: &dyn Fn() -> bool) -> Result<bool> {
    let meta = fs::metadata(path)?;
    if hashed.is_dir && meta.is_dir() {
        let (size, sha256) = hash_folder_cancellable(path, cancelled)?;
        return Ok(size == hashed.size && sha256 == hashed.sha256);
    }
    Ok(!hashed.is_dir && meta.is_file() && meta.len() == hashed.size
        && sha256_file_cancellable(path, cancelled)? == hashed.sha256)
}

#[cfg(not(windows))]
fn sync_import_copy(path: &Path) -> std::io::Result<()> {
    fs::File::open(path)?.sync_all()
}

#[cfg(windows)]
fn sync_import_copy(path: &Path) -> std::io::Result<()> {
    // FlushFileBuffers 需要可写句柄。fs::copy 会保留源文件的只读属性。
    let permissions = fs::metadata(path)?.permissions();
    if permissions.readonly() {
        let mut writable = permissions.clone();
        writable.set_readonly(false);
        fs::set_permissions(path, writable)?;
    }
    fs::OpenOptions::new().write(true).open(path)?.sync_all()?;
    if permissions.readonly() {
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

/// 认不出是什么软件时文件名里常见的词：这种时候用文件自带的产品名。
fn generic_name(name: &str) -> bool {
    matches!(normalize_name(name).as_str(), "" | "setup" | "install" | "installer" | "update" | "app" | "program" | "main" | "launcher" | "portable")
}

impl Repo {
    /// 导入一批文件或文件夹（托管时复制进仓库）。见 `import_paths_with`。
    pub fn import_paths(
        &mut self,
        paths: &[PathBuf],
        mode: Storage,
        hint: &ImportHint,
        on_progress: &mut dyn FnMut(Progress),
    ) -> Result<ImportReport> {
        self.import_paths_with(paths, mode, false, hint, on_progress)
    }

    /// 导入一批文件或文件夹。文件夹会被展开成里面的安装包。分批保存索引。
    ///
    /// `move_files` 只在托管模式下起作用：放进仓库后原文件就不留了。同一个磁盘上直接移动；
    /// 跨磁盘先复制，核对内容一模一样再删原文件。认出是重复的文件不动，原样留在原处。
    ///
    /// 这里一直占着仓库。桌面应用里导入大批文件时不用它，而是把 `hash_source`、`plan_import`、
    /// `place_file`、`register` 分开调用，只在第二、四步短暂地占着仓库。
    pub fn import_paths_with(
        &mut self,
        paths: &[PathBuf],
        mode: Storage,
        move_files: bool,
        hint: &ImportHint,
        on_progress: &mut dyn FnMut(Progress),
    ) -> Result<ImportReport> {
        let _writer = self.begin_write()?;
        on_progress(Progress { phase: "collect".into(), current: 0, total: 0, file: String::new() });
        let files = collect_files(paths, &self.data_dir(), &hint.package_set());
        let total = files.len();
        let mut known = self.known_paths();
        let mut report = ImportReport::default();
        let mut saver = BatchSaver::new();
        for (i, file) in files.iter().enumerate() {
            let label = file.file_name().unwrap_or_default().to_string_lossy().to_string();
            on_progress(Progress { phase: "import".into(), current: i + 1, total, file: label });
            let outcome = self.import_one(file, mode, move_files, hint, &mut known);
            report.add(file, outcome);
            saver.maybe_save(self, report.imported)?;
        }
        saver.finish(self, report.imported)?;
        on_progress(Progress { phase: "done".into(), current: total, total, file: String::new() });
        Ok(report)
    }

    /// 已经登记过的文件路径。重新扫描同一个文件夹时，这些文件不用再算一遍哈希。
    pub fn known_paths(&self) -> HashSet<PathBuf> {
        self.lib.software.iter().flat_map(|s| &s.versions).flat_map(|v| &v.files).map(|f| self.abs_path(f)).collect()
    }

    /// `known` 里有这个文件时，它已经登记过：返回它所属软件的 id。
    pub fn already_known(&self, src: &Path, known: &HashSet<PathBuf>) -> Option<String> {
        let abs = clean_path(&fs::canonicalize(src).ok()?);
        known.contains(&abs).then(|| self.owner_of(&abs).unwrap_or_default())
    }

    fn import_one(&mut self, src: &Path, mode: Storage, move_files: bool, hint: &ImportHint, known: &mut HashSet<PathBuf>) -> Result<Outcome> {
        if let Some(owner) = self.already_known(src, known) {
            return Ok(Outcome::Duplicate(owner));
        }
        let hashed = hash_source(src).at("read")?;
        match self.plan_import(&hashed, mode, hint) {
            Plan::Duplicate(owner) => Ok(Outcome::Duplicate(owner)),
            Plan::Add(placement) => {
                let placed = place_file(self.root(), &hashed, &placement, move_files).at("copy")?;
                self.register(&hashed, placement, placed, known).at("save")
            }
        }
    }

    /// 认软件名和版本、查重复，决定这个文件登记成什么、放在哪。只读索引。
    pub fn plan_import(&self, hashed: &Hashed, mode: Storage, hint: &ImportHint) -> Plan {
        let mut identity = filename_identity(&hashed.file_name, hashed.is_dir);
        fill_identity(&mut identity, hashed.info.as_ref(), hashed.platform);
        let publisher = hashed.info.as_ref().map_or_else(String::new, |info| info.company.trim().to_string());
        let edit = hint.entries.get(hashed.source.to_string_lossy().as_ref());
        let name = edit.map(|edit| edit.name.as_str()).or(hint.name.as_deref());
        let version = edit.map(|edit| edit.version.as_str()).or(hint.version.as_deref());
        if let Some(name) = name.map(str::trim).filter(|n| !n.is_empty()) {
            identity.name = name.to_string();
        }
        if let Some(version) = version.map(str::trim) {
            identity.version = version.to_string();
        }
        if let Some(platform) = hint.platform {
            identity.platform = platform;
        }
        if let Some(owner) = self.duplicate_of(&identity, &hashed.sha256) {
            return Plan::Duplicate(owner);
        }
        let key = normalize_name(&identity.name);
        let folder = self.lib.software.iter().find(|s| normalize_name(&s.name) == key).map_or(identity.name.clone(), |s| s.name.clone());
        let target = if let Ok(rel) = hashed.abs.strip_prefix(self.root()) {
            Target::InRepo(to_rel(rel))
        } else if mode == Storage::Managed {
            Target::Managed
        } else {
            Target::Indexed
        };
        let category = hint.category_id.clone().filter(|id| self.lib.categories.iter().any(|c| &c.id == id));
        Plan::Add(Placement { identity, publisher, category, folder, target })
    }

    /// 同一个软件、同一个版本里已经有内容相同的文件：返回那个软件的 id。
    fn duplicate_of(&self, identity: &Identity, sha256: &str) -> Option<String> {
        let key = normalize_name(&identity.name);
        let software = self.lib.software.iter().find(|s| normalize_name(&s.name) == key)?;
        let version = software.versions.iter().find(|v| v.version == identity.version)?;
        version.files.iter().any(|f| f.sha256 == sha256).then(|| software.id.clone())
    }

    /// 把放好的文件登记进索引（不保存）。放文件的这段时间里如果别处已经登记了同样的内容，
    /// 就当重复处理，这次新放进仓库的那份删掉（内容已经在仓库里了）。
    pub fn register(&mut self, hashed: &Hashed, placement: Placement, placed: Placed, known: &mut HashSet<PathBuf>) -> Result<Outcome> {
        let Placement { identity, publisher, category, .. } = placement;
        if let Some(owner) = self.duplicate_of(&identity, &hashed.sha256) {
            if placed.created && placed.disk != hashed.abs {
                if placed.disk.is_dir() {
                    let _ = fs::remove_dir_all(&placed.disk);
                } else {
                    let _ = fs::remove_file(&placed.disk);
                }
                self.remove_empty_dirs(placed.disk.parent());
            }
            return Ok(Outcome::Duplicate(owner));
        }
        let added_at = now();
        let file = PackageFile {
            id: new_id(),
            name: placed.disk.file_name().unwrap_or_default().to_string_lossy().to_string(),
            platform: identity.platform,
            arch: identity.arch,
            size: hashed.size,
            sha256: hashed.sha256.clone(),
            storage: placed.storage,
            path: placed.path,
            added_at: added_at.clone(),
        };
        let key = normalize_name(&identity.name);
        let s = match self.lib.software.iter().position(|s| normalize_name(&s.name) == key) {
            Some(s) => s,
            None => {
                self.lib.software.push(Software {
                    id: new_id(),
                    name: identity.name,
                    publisher: publisher.clone(),
                    homepage: String::new(),
                    note: String::new(),
                    category_id: category.clone(),
                    starred: false,
                    icon: None,
                    icon_data: None,
                    default_version_id: None,
                    added_at: added_at.clone(),
                    versions: Vec::new(),
                    subscription: None,
                });
                self.lib.software.len() - 1
            }
        };
        let software = &mut self.lib.software[s];
        // 放进指定的分类：已有的软件只在还没分类时放进去，已经分好类的不动
        if software.category_id.is_none() {
            software.category_id = category;
        }
        // 发布者还空着时用文件自带的公司名补上
        if software.publisher.trim().is_empty() && !publisher.is_empty() {
            software.publisher = publisher;
        }
        let v = match software.versions.iter().position(|v| v.version == identity.version) {
            Some(v) => v,
            None => {
                software.versions.push(Version { id: new_id(), version: identity.version, note: String::new(), added_at, files: Vec::new() });
                software.versions.len() - 1
            }
        };
        software.versions[v].files.push(file);
        let software_id = software.id.clone();
        // 软件还没有图标（或只有联网取的）时，从刚导入的文件里取一个
        if crate::ops::needs_local_icon(&self.lib.software[s]) {
            if let Some(icon) = icon_source(&placed.disk).and_then(|source| self.icon_from(&software_id, &source)) {
                self.set_local_icon(s, icon);
            }
        }
        known.insert(placed.disk);
        known.insert(hashed.abs.clone());
        Ok(Outcome::Imported(software_id))
    }

    fn owner_of(&self, abs: &Path) -> Option<String> {
        self.lib.software.iter().find_map(|s| {
            s.versions.iter().flat_map(|v| &v.files).any(|f| self.abs_path(f) == abs).then(|| s.id.clone())
        })
    }
}

impl ImportReport {
    /// 记下一个文件的导入结果。
    pub fn add(&mut self, file: &Path, outcome: Result<Outcome>) {
        let path = file.to_string_lossy().to_string();
        let item = match outcome {
            Ok(Outcome::Imported(id)) => {
                self.imported += 1;
                ImportItem { path, status: "imported".into(), software_id: Some(id), message: None, step: None, reason: None }
            }
            Ok(Outcome::Duplicate(id)) => {
                self.duplicates += 1;
                ImportItem { path, status: "duplicate".into(), software_id: Some(id), message: None, step: None, reason: None }
            }
            Err(err) => {
                self.failed += 1;
                ImportItem {
                    path,
                    status: "failed".into(),
                    software_id: None,
                    message: Some(err.to_string()),
                    step: err.step().map(str::to_string),
                    reason: Some(err.reason().to_string()),
                }
            }
        };
        self.items.push(item);
    }
}

/// 导入大批文件时分批保存：每新登记 20 个文件或每过 30 秒存一次。
/// 中途崩溃或断电，已经复制进仓库的文件大多也已登记。
pub struct BatchSaver {
    saved: usize,
    last: Instant,
}

impl BatchSaver {
    pub fn new() -> Self {
        Self { saved: 0, last: Instant::now() }
    }

    /// `imported` 是到目前为止新登记的文件数。到了该存的时候就存。
    pub fn maybe_save(&mut self, repo: &mut Repo, imported: usize) -> Result<()> {
        if imported > self.saved && (imported - self.saved >= SAVE_EVERY_FILES || self.last.elapsed() >= SAVE_EVERY) {
            repo.save()?;
            (self.saved, self.last) = (imported, Instant::now());
        }
        Ok(())
    }

    /// 导入结束：还有没存的就存一次。
    pub fn finish(&mut self, repo: &mut Repo, imported: usize) -> Result<()> {
        if imported > self.saved {
            repo.save()?;
            self.saved = imported;
        }
        Ok(())
    }
}

impl Default for BatchSaver {
    fn default() -> Self {
        Self::new()
    }
}

/// 仓库内的相对路径，统一用 `/`，换个系统打开也能用。
fn to_rel(rel: &Path) -> String {
    rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}

pub fn sha256_file(path: &Path) -> Result<String> {
    sha256_file_cancellable(path, &|| false)
}

pub(crate) fn sha256_file_cancellable(path: &Path, cancelled: &dyn Fn() -> bool) -> Result<String> {
    checkpoint(cancelled)?;
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        checkpoint(cancelled)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_can_stop_between_blocks_of_one_file() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("Large-1.0.exe");
        fs::write(&source, vec![7; 4 * 1024 * 1024]).unwrap();
        let checks = std::cell::Cell::new(0);
        let hash = sha256_file_cancellable(&source, &|| {
            checks.set(checks.get() + 1);
            checks.get() >= 3
        });
        assert!(matches!(hash, Err(Error::Code("cancelled"))));
        assert_eq!(fs::metadata(&source).unwrap().len(), 4 * 1024 * 1024);
    }

    #[test]
    fn cancelled_copy_removes_staging_without_touching_the_source_or_index() {
        for folder in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let repo = Repo::open(&temp.path().join("repo")).unwrap();
            let source = temp.path().join(if folder { "Large-1.0" } else { "Large-1.0.exe" });
            let data = if folder {
                fs::create_dir(&source).unwrap();
                source.join("large.bin")
            } else {
                source.clone()
            };
            fs::write(&data, vec![7; 8 * 1024 * 1024]).unwrap();
            let hashed = hash_source(&source).unwrap();
            let Plan::Add(placement) = repo.plan_import(&hashed, Storage::Managed, &ImportHint::default()) else {
                panic!("fixture should be a new package");
            };
            let copied = std::cell::Cell::new(false);
            let result = place_file_cancellable(repo.root(), &hashed, &placement, false, &|| {
                copied.set(copied.get() || walkdir::WalkDir::new(repo.root()).into_iter().flatten().any(|entry| {
                    (entry.file_name() == "large.bin" || entry.file_name() == "payload")
                        && entry.file_type().is_file() && entry.metadata().is_ok_and(|meta| meta.len() > 0)
                }));
                copied.get()
            });
            assert!(copied.get(), "the copy must have started");
            assert!(matches!(result, Err(Error::Code("cancelled"))));
            assert_eq!(fs::metadata(&data).unwrap().len(), hashed.size);
            assert!(walkdir::WalkDir::new(repo.root()).into_iter().flatten().all(|entry| {
                !entry.file_name().to_string_lossy().ends_with(".part") && entry.file_name() != source.file_name().unwrap()
            }));
            assert!(Repo::open(repo.root()).unwrap().lib.software.is_empty());
            assert!(place_file(repo.root(), &hashed, &placement, false).is_ok());
        }
    }

    fn import(repo: &mut Repo, paths: &[PathBuf], mode: Storage) -> ImportReport {
        repo.import_paths(paths, mode, &ImportHint::default(), &mut |_| {}).unwrap()
    }

    #[test]
    fn import_preview_shows_both_versions_and_honors_confirmed_identity() {
        let root = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        let app = source.path().join("Docker Desktop-2.4.1.app");
        fs::create_dir_all(app.join("Contents")).unwrap();
        let mut plist = plist::Dictionary::new();
        plist.insert("CFBundleShortVersionString".into(), "2.5.0".into());
        plist::Value::Dictionary(plist).to_file_xml(app.join("Contents/Info.plist")).unwrap();
        let setup = source.path().join("Git-2.51.0.exe");
        fs::write(&setup, b"test installer").unwrap();

        let mut repo = Repo::open(root.path()).unwrap();
        let docker_id = repo.ensure_software("Docker", "").unwrap();
        let names = vec![(docker_id.clone(), "Docker".into())];
        let drops = inspect(&[source.path().to_path_buf()], &repo.data_dir(), &names, true);
        assert_eq!(drops[0].installers, 2);
        let entry = &drops[0].children[0];
        assert_eq!(entry.guess_name, "Docker Desktop");
        assert_eq!(entry.matched_software_id.as_deref(), Some(docker_id.as_str()));
        assert_eq!(entry.filename_version, "2.4.1");
        assert_eq!(entry.metadata_version, "2.5.0");
        let hint = ImportHint {
            entries: HashMap::from([(entry.path.clone(), ImportEntryEdit { name: "Docker".into(), version: entry.metadata_version.clone() })]),
            ..Default::default()
        };
        let report = repo.import_paths(&[app, setup], Storage::Managed, &hint, &mut |_| {}).unwrap();
        assert_eq!(report.imported, 2, "{:?}", report.items);
        assert_eq!(repo.lib.software.len(), 2);
        assert_eq!(repo.lib.software[0].versions[0].version, "2.5.0");
        assert_eq!(repo.lib.software[1].versions[0].version, "2.51.0");
        let git_id = repo.lib.software[1].id.clone();
        repo.lib.categories.push(Category { id: "manual".into(), name: "Manual".into(), parent_id: None, color: None, icon: None });
        repo.lib.software[0].category_id = Some("manual".into());
        let suggestions = repo.suggest_categories(&[docker_id.clone(), git_id]);
        assert_eq!(repo.apply_import_classification(suggestions).unwrap(), 1);
        assert_eq!(repo.lib.software[0].category_id.as_deref(), Some("manual"));
        assert!(repo.lib.software[1].category_id.is_some());
        assert_eq!(identify_folder("Tool-1.2.3").version, "1.2.3");
        assert_eq!(match_import_name("Docker Pro", &names), None);
        let ambiguous = [(docker_id, "Docker".into()), ("other".into(), "Docker Desktop".into())];
        assert_eq!(match_import_name("Docker Desktop", &ambiguous).as_deref(), Some("other"));
        assert_eq!(match_import_name("Docker Desktop Edition", &ambiguous), None);
    }

    #[test]
    fn managed_import_copies_and_groups() {
        let repo_dir = tempfile::tempdir().unwrap();
        let src_dir = tempfile::tempdir().unwrap();
        let win = src_dir.path().join("Obsidian-1.7.7-x64.exe");
        let mac = src_dir.path().join("Obsidian-1.7.7.dmg");
        let old = src_dir.path().join("Obsidian-1.6.0.dmg");
        fs::write(&win, b"windows build").unwrap();
        fs::write(&mac, b"mac build").unwrap();
        fs::write(&old, b"old mac build").unwrap();

        let mut repo = Repo::open(repo_dir.path()).unwrap();
        let report = import(&mut repo, &[win.clone(), mac, old], Storage::Managed);
        assert_eq!((report.imported, report.duplicates, report.failed), (3, 0, 0));

        assert_eq!(repo.lib.software.len(), 1);
        let software = &repo.lib.software[0];
        assert_eq!(software.name, "Obsidian");
        assert_eq!(software.versions.len(), 2);
        let v177 = software.versions.iter().find(|v| v.version == "1.7.7").unwrap();
        assert_eq!(v177.files.len(), 2);
        let file = v177.files.iter().find(|f| f.platform == Platform::Windows).unwrap();
        assert_eq!(file.path, "Obsidian/1.7.7/Obsidian-1.7.7-x64.exe");
        assert_eq!(file.arch, "x64");
        assert!(repo.abs_path(file).is_file());
        assert!(win.is_file(), "原文件应当保留");

        // 再导入同一个文件：重复，不产生新记录
        let again = import(&mut repo, &[win], Storage::Managed);
        assert_eq!((again.imported, again.duplicates), (0, 1));
        assert_eq!(repo.stats().files, 3);
    }

    #[cfg(windows)]
    #[test]
    fn managed_import_can_flush_a_read_only_installer_copy() {
        let repo_dir = tempfile::tempdir().unwrap();
        let src_dir = tempfile::tempdir().unwrap();
        let source = src_dir.path().join("Everything-1.4.1.1024.x64-Setup.exe");
        let contents = b"test installer";
        fs::write(&source, contents).unwrap();
        let writable = fs::metadata(&source).unwrap().permissions();
        let mut read_only = writable.clone();
        read_only.set_readonly(true);
        fs::set_permissions(&source, read_only).unwrap();

        let mut repo = Repo::open(repo_dir.path()).unwrap();
        let report = import(&mut repo, std::slice::from_ref(&source), Storage::Managed);
        assert_eq!((report.imported, report.duplicates, report.failed), (1, 0, 0), "{:?}", report.items);
        let file = &repo.lib.software[0].versions[0].files[0];
        let destination = repo.abs_path(file);
        assert_eq!(fs::read(&destination).unwrap(), contents);
        assert!(fs::metadata(&source).unwrap().permissions().readonly());
        assert!(fs::metadata(&destination).unwrap().permissions().readonly());
        assert_eq!(fs::read_dir(destination.parent().unwrap()).unwrap().count(), 1);
        fs::set_permissions(&source, writable.clone()).unwrap();
        fs::set_permissions(destination, writable).unwrap();
    }

    #[test]
    fn indexed_import_keeps_file_in_place() {
        let repo_dir = tempfile::tempdir().unwrap();
        let src_dir = tempfile::tempdir().unwrap();
        let file = src_dir.path().join("7-Zip 24.09 x64.exe");
        fs::write(&file, b"7z").unwrap();

        let mut repo = Repo::open(repo_dir.path()).unwrap();
        let report = import(&mut repo, &[file.clone()], Storage::Indexed);
        assert_eq!(report.imported, 1);
        let record = &repo.lib.software[0].versions[0].files[0];
        assert_eq!(record.storage, Storage::Indexed);
        assert_eq!(repo.abs_path(record), clean_path(&fs::canonicalize(&file).unwrap()));
        assert!(!repo.root().join("7-Zip").exists(), "索引模式不应复制文件");

        // 重新扫描：按路径直接判定为已登记
        let again = import(&mut repo, &[file], Storage::Indexed);
        assert_eq!((again.imported, again.duplicates), (0, 1));
    }

    #[test]
    fn scanning_a_folder_takes_only_installers() {
        let repo_dir = tempfile::tempdir().unwrap();
        let src_dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(src_dir.path().join("dev/.cache")).unwrap();
        fs::write(src_dir.path().join("dev/Git-2.47.1-64-bit.exe"), b"git").unwrap();
        fs::write(src_dir.path().join("dev/.cache/hidden-1.0.exe"), b"hidden").unwrap();
        fs::write(src_dir.path().join("readme.txt"), b"text").unwrap();
        fs::write(src_dir.path().join("vlc-3.0.21.dmg"), b"vlc").unwrap();

        let mut repo = Repo::open(repo_dir.path()).unwrap();
        let mut phases = Vec::new();
        let report = repo
            .import_paths(&[src_dir.path().to_path_buf()], Storage::Indexed, &ImportHint::default(), &mut |p| {
                phases.push(p.phase)
            })
            .unwrap();
        assert_eq!(report.imported, 2);
        let mut names: Vec<_> = repo.lib.software.iter().map(|s| s.name.as_str()).collect();
        names.sort();
        assert_eq!(names, ["Git", "vlc"]);
        assert_eq!(phases.first().map(String::as_str), Some("collect"));
        assert_eq!(phases.last().map(String::as_str), Some("done"));
    }

    #[test]
    fn files_already_inside_the_repo_are_registered_in_place() {
        let repo_dir = tempfile::tempdir().unwrap();
        let mut repo = Repo::open(repo_dir.path()).unwrap();
        let inside = repo.root().join("Blender").join("4.3.2");
        fs::create_dir_all(&inside).unwrap();
        fs::write(inside.join("blender-4.3.2-windows-x64.msi"), b"blender").unwrap();

        let root = repo.root().to_path_buf();
        let report = import(&mut repo, &[root], Storage::Indexed);
        assert_eq!(report.imported, 1);
        let record = &repo.lib.software[0].versions[0].files[0];
        assert_eq!(record.storage, Storage::Managed);
        assert_eq!(record.path, "Blender/4.3.2/blender-4.3.2-windows-x64.msi");
    }

    /// 桌面应用里导入是分步做的，复制文件时不占着仓库。这期间别处登记了同样的内容：
    /// 登记时认出是重复的，这次新复制进仓库的那份删掉，不留一个没登记的文件。
    #[test]
    fn registering_after_a_concurrent_import_counts_as_duplicate() {
        let repo_dir = tempfile::tempdir().unwrap();
        let src = tempfile::tempdir().unwrap();
        let mut repo = Repo::open(repo_dir.path()).unwrap();
        let file = src.path().join("Tool-1.0.exe");
        fs::write(&file, b"tool").unwrap();
        let mut known = repo.known_paths();
        let hashed = hash_source(&file).unwrap();
        let Plan::Add(placement) = repo.plan_import(&hashed, Storage::Managed, &ImportHint::default()) else { panic!("应当是新文件") };

        // 复制期间，同样内容的另一个文件被导入了
        let other = src.path().join("Tool-1.0-x64.exe");
        fs::write(&other, b"tool").unwrap();
        assert_eq!(import(&mut repo, &[other], Storage::Managed).imported, 1);

        let placed = place_file(repo.root(), &hashed, &placement, false).unwrap();
        let copy = placed.disk.clone();
        assert!(copy.is_file());
        assert!(matches!(repo.register(&hashed, placement, placed, &mut known).unwrap(), Outcome::Duplicate(_)));
        assert!(!copy.exists(), "多复制的那份应当删掉");
        assert_eq!(repo.lib.software[0].versions[0].files.len(), 1);
    }

    /// 移动进仓库：原文件不留；认出是重复的文件原样留在原处。
    #[test]
    fn move_mode_removes_originals_but_keeps_duplicates() {
        let repo_dir = tempfile::tempdir().unwrap();
        let src = tempfile::tempdir().unwrap();
        let mut repo = Repo::open(repo_dir.path()).unwrap();
        let first = src.path().join("Tool-1.0.exe");
        fs::write(&first, b"tool").unwrap();
        let report = repo.import_paths_with(&[first.clone()], Storage::Managed, true, &ImportHint::default(), &mut |_| {}).unwrap();
        assert_eq!(report.imported, 1);
        assert!(!first.exists(), "原文件应当移走");
        let stored = repo.abs_path(&repo.lib.software[0].versions[0].files[0]);
        assert_eq!(fs::read(&stored).unwrap(), b"tool");

        // 同样内容的文件再导入一次：是重复的，不移动、不删除
        let again = src.path().join("Tool-1.0-x64.exe");
        fs::write(&again, b"tool").unwrap();
        let report = repo.import_paths_with(&[again.clone()], Storage::Managed, true, &ImportHint::default(), &mut |_| {}).unwrap();
        assert_eq!(report.duplicates, 1);
        assert!(again.exists(), "重复的文件留在原处");

        // 只记录位置时不管这个开关，原文件不动
        let indexed = src.path().join("Other-2.0.exe");
        fs::write(&indexed, b"other").unwrap();
        repo.import_paths_with(&[indexed.clone()], Storage::Indexed, true, &ImportHint::default(), &mut |_| {}).unwrap();
        assert!(indexed.exists());
    }

    #[test]
    fn move_import_leaves_the_source_alone_when_another_writer_holds_the_lock() {
        let repo_dir = tempfile::tempdir().unwrap();
        let src = tempfile::tempdir().unwrap();
        let mut first = Repo::open(repo_dir.path()).unwrap();
        let mut second = Repo::open(repo_dir.path()).unwrap();
        let source = src.path().join("Tool-1.0.exe");
        fs::write(&source, b"tool").unwrap();

        let writer = first.begin_write().unwrap();
        let result = second.import_paths_with(std::slice::from_ref(&source), Storage::Managed, true, &ImportHint::default(), &mut |_| {});
        assert!(matches!(result, Err(Error::Code("repo_busy"))));
        assert_eq!(fs::read(&source).unwrap(), b"tool");

        drop(writer);
        let report = second.import_paths_with(&[source.clone()], Storage::Managed, true, &ImportHint::default(), &mut |_| {}).unwrap();
        assert_eq!(report.imported, 1);
        assert!(!source.exists());
    }

    #[test]
    fn hint_overrides_identification() {
        let repo_dir = tempfile::tempdir().unwrap();
        let src_dir = tempfile::tempdir().unwrap();
        let file = src_dir.path().join("setup.exe");
        fs::write(&file, b"x").unwrap();

        let mut repo = Repo::open(repo_dir.path()).unwrap();
        let hint = ImportHint { name: Some("微信".into()), version: Some("3.9.12".into()), ..Default::default() };
        repo.import_paths(&[file], Storage::Managed, &hint, &mut |_| {}).unwrap();
        let software = &repo.lib.software[0];
        assert_eq!(software.name, "微信");
        assert_eq!(software.versions[0].files[0].path, "微信/3.9.12/setup.exe");
    }

    /// 一个绿色版软件的文件夹：主程序加它的库文件，还有子文件夹。
    fn portable_folder(parent: &Path) -> PathBuf {
        let dir = parent.join("Tool");
        fs::create_dir_all(dir.join("data")).unwrap();
        fs::write(dir.join("Tool.exe"), b"main program").unwrap();
        fs::write(dir.join("helper.dll"), b"library").unwrap();
        fs::write(dir.join("data/config.ini"), b"[x]").unwrap();
        dir
    }

    #[test]
    fn a_folder_marked_as_package_is_imported_whole() {
        let repo_dir = tempfile::tempdir().unwrap();
        let src_dir = tempfile::tempdir().unwrap();
        let dir = portable_folder(src_dir.path());
        assert!(looks_like_package(&dir));
        assert!(!looks_like_package(src_dir.path()), "装着一个软件文件夹的上一层不算");
        assert_eq!(main_program(&dir), Some(dir.join("Tool.exe")));

        let mut repo = Repo::open(repo_dir.path()).unwrap();
        repo.lib.categories.push(Category { id: "tools".into(), name: "工具".into(), parent_id: None, color: None, icon: None });
        let hint = ImportHint { packages: vec![dir.to_string_lossy().to_string()], category_id: Some("tools".into()), ..Default::default() };
        let report = repo.import_paths(&[dir.clone()], Storage::Managed, &hint, &mut |_| {}).unwrap();
        assert_eq!((report.imported, report.failed), (1, 0), "{:?}", report.items.iter().map(|i| &i.message).collect::<Vec<_>>());
        let software = &repo.lib.software[0];
        assert_eq!(software.name, "Tool");
        assert_eq!(software.category_id.as_deref(), Some("tools"));
        let file = &software.versions[0].files[0];
        assert_eq!(file.path, "Tool/unknown/Tool");
        assert_eq!(file.size, (b"main program".len() + b"library".len() + b"[x]".len()) as u64);
        assert_eq!(file.platform, Platform::Windows);
        assert!(repo.root().join("Tool/unknown/Tool/data/config.ini").is_file());
        assert!(dir.join("Tool.exe").is_file(), "复制模式不动原文件夹");
        assert!(repo.verify().problems.is_empty(), "文件夹的大小按里面的文件加起来核对");
        assert_eq!(launch_target(&repo.root().join("Tool/unknown/Tool")), repo.root().join("Tool/unknown/Tool/Tool.exe"));

        // 同一个文件夹再导入一次是重复的
        let again = repo.import_paths(&[dir.clone()], Storage::Managed, &hint, &mut |_| {}).unwrap();
        assert_eq!(again.duplicates, 1);
        // 没有标成安装包时照旧展开：里面的 Tool.exe 当作一个安装包
        let expanded = collect_files(&[dir.clone()], &repo.data_dir(), &HashSet::new());
        assert_eq!(expanded, vec![dir.join("Tool.exe")]);
    }

    #[test]
    fn app_bundles_in_a_scanned_folder_are_whole_packages() {
        let repo_dir = tempfile::tempdir().unwrap();
        let src_dir = tempfile::tempdir().unwrap();
        let app = src_dir.path().join("Foo.app");
        fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        fs::write(app.join("Contents/MacOS/Foo"), b"binary").unwrap();
        // .app 里面的 exe（比如带着的 Windows 工具）不能被当成单独的安装包
        fs::write(app.join("Contents/MacOS/tool.exe"), b"inner").unwrap();
        let mut plist = plist::Dictionary::new();
        plist.insert("CFBundleName".into(), "Foo".into());
        plist.insert("CFBundleShortVersionString".into(), "2.1".into());
        plist.insert("NSHumanReadableCopyright".into(), "Copyright © 2024 Foo Labs".into());
        plist::Value::Dictionary(plist).to_file_xml(app.join("Contents/Info.plist")).unwrap();
        fs::write(src_dir.path().join("Bar-1.0.dmg"), b"dmg").unwrap();

        let mut repo = Repo::open(repo_dir.path()).unwrap();
        let mut files = collect_files(&[src_dir.path().to_path_buf()], &repo.data_dir(), &HashSet::new());
        files.sort();
        assert_eq!(files, vec![src_dir.path().join("Bar-1.0.dmg"), app.clone()]);

        let report = import(&mut repo, &[src_dir.path().to_path_buf()], Storage::Indexed);
        assert_eq!((report.imported, report.failed), (2, 0));
        let foo = repo.lib.software.iter().find(|s| s.name == "Foo").unwrap();
        assert_eq!(foo.versions[0].version, "2.1", "版本号从 Info.plist 里来");
        assert_eq!(foo.publisher, "Foo Labs");
        assert_eq!(foo.versions[0].files[0].platform, Platform::Macos);
    }

    #[test]
    fn file_properties_fill_in_a_missing_version() {
        let repo_dir = tempfile::tempdir().unwrap();
        let src_dir = tempfile::tempdir().unwrap();
        let file = src_dir.path().join("setup.msi");
        crate::fileinfo::write_test_msi(&file, "WinThumbsPreloader", "1.1.0.0", "Dmitry Bruhov");
        let mut repo = Repo::open(repo_dir.path()).unwrap();
        let report = import(&mut repo, &[file], Storage::Managed);
        assert_eq!(report.imported, 1);
        let software = &repo.lib.software[0];
        assert_eq!(software.name, "WinThumbsPreloader", "文件名是 setup 这种时用属性里的产品名");
        assert_eq!(software.publisher, "Dmitry Bruhov");
        assert_eq!(software.versions[0].version, "1.1.0", "文件名里没有版本号时用属性里的");
        assert_eq!(software.versions[0].files[0].path, "WinThumbsPreloader/1.1.0/setup.msi");
    }

    #[test]
    fn failures_say_which_step_and_why() {
        let repo_dir = tempfile::tempdir().unwrap();
        let mut repo = Repo::open(repo_dir.path()).unwrap();
        let report = import(&mut repo, &[PathBuf::from("/nonexistent/Tool-1.0.exe")], Storage::Managed);
        assert_eq!(report.failed, 1);
        let item = &report.items[0];
        assert_eq!(item.step.as_deref(), Some("read"));
        assert_eq!(item.reason.as_deref(), Some("file_missing"));
        assert!(item.message.as_deref().is_some_and(|m| !m.is_empty()));
    }
}
