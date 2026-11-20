//! ShibaPack 的核心逻辑。这里不依赖 Tauri，所有功能都能用 `cargo test -p shibapack-core` 单独验证。

#[cfg(feature = "network")]
pub mod ai;
pub mod aicache;
pub mod catalog;
pub mod classify;
pub mod cleanup;
pub mod disk;
pub mod download;
pub mod faillog;
pub mod fileinfo;
pub mod icons;
pub mod identify;
pub mod import;
pub mod installed;
pub mod leftovers;
pub mod model;
pub mod ops;
pub mod sources;
pub mod startup;
pub mod store;
pub mod summary;
pub mod system;
pub mod trace;
pub mod webicon;

pub use classify::{Assignment, NewCategory, Suggestion};
pub use import::{ImportHint, ImportItem, ImportReport, Progress};
pub use installed::InstalledApp;
pub use model::*;
pub use ops::{SoftwareEdit, VersionEdit, VersionRef, WebIconQuery};
pub use store::{BackupInfo, MissingFile, Repo, Stats, VerifyReport};

/// 错误。`Code` 是给界面翻译用的短代码（如 `not_found`），其余是系统给出的原始信息。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("invalid_data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Code(&'static str),
    /// 在哪一步出的错（导入时：读取、复制、登记），说明原因时用。
    #[error("{1}")]
    Step(&'static str, Box<Error>),
}

impl Error {
    /// 出错的那一步（没有标出来时是 None）。
    pub fn step(&self) -> Option<&'static str> {
        match self {
            Error::Step(step, _) => Some(step),
            _ => None,
        }
    }

    /// 原因的代码：错误代码本身，或者把系统错误归成几类（没有权限、空间不够、被占用……），
    /// 界面上按它翻译；归不了类的是 `io`，界面上显示系统给的原话。
    pub fn reason(&self) -> &'static str {
        match self {
            Error::Step(_, inner) => inner.reason(),
            Error::Code(code) => code,
            Error::Json(_) => "invalid_data",
            Error::Io(err) => io_reason(err),
        }
    }

    /// 标上是哪一步出的错。已经标过的不再改。
    pub fn at(self, step: &'static str) -> Error {
        match self {
            Error::Step(..) => self,
            other => Error::Step(step, Box::new(other)),
        }
    }
}

/// 系统错误归类。
fn io_reason(err: &std::io::Error) -> &'static str {
    use std::io::ErrorKind::*;
    // Windows：32 文件被别的程序占用，33 一部分被锁住，112 磁盘满，1224 文件映射在用
    #[cfg(windows)]
    if let Some(32 | 33 | 1224) = err.raw_os_error() {
        return "in_use";
    }
    // macOS / Linux：26 文件忙（ETXTBSY），28 磁盘满（ENOSPC），30 只读（EROFS），36/63 文件名太长
    #[cfg(unix)]
    match err.raw_os_error() {
        Some(26) => return "in_use",
        Some(28) => return "disk_full",
        Some(30) => return "read_only",
        Some(36 | 63) => return "name_too_long",
        _ => {}
    }
    match err.kind() {
        PermissionDenied => "permission_denied",
        NotFound => "file_missing",
        StorageFull => "disk_full",
        ReadOnlyFilesystem => "read_only",
        ResourceBusy => "in_use",
        InvalidFilename => "invalid_filename",
        TimedOut | NotConnected | ConnectionReset | ConnectionAborted | NetworkUnreachable | HostUnreachable => "disconnected",
        AlreadyExists => "already_exists",
        _ => "io",
    }
}

/// 给 `Result` 标上出错的那一步。
pub trait AtStep<T> {
    fn at(self, step: &'static str) -> Result<T>;
}

impl<T, E: Into<Error>> AtStep<T> for std::result::Result<T, E> {
    fn at(self, step: &'static str) -> Result<T> {
        self.map_err(|err| err.into().at(step))
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// 当前时间，带时区的 ISO 8601 字符串。
pub fn now() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

/// 新的记录 id。
pub fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_string()
}
