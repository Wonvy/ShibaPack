//! ShibaPack 的核心逻辑。这里不依赖 Tauri，所有功能都能用 `cargo test -p shibapack-core` 单独验证。

pub mod catalog;
pub mod classify;
pub mod download;
pub mod icons;
pub mod identify;
pub mod import;
pub mod installed;
pub mod model;
pub mod ops;
pub mod sources;
pub mod store;
pub mod summary;
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
