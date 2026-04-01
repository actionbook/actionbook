pub mod clear;
pub mod delete;
pub mod get;
pub mod list;
pub mod set;

/// Which Web Storage object to target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum StorageKind {
    #[default]
    Local,
    Session,
}

impl StorageKind {
    /// JavaScript global object name.
    pub fn js_object(self) -> &'static str {
        match self {
            StorageKind::Local => "window.localStorage",
            StorageKind::Session => "window.sessionStorage",
        }
    }

    /// Value for the `storage` field in JSON responses.
    pub fn data_name(self) -> &'static str {
        match self {
            StorageKind::Local => "local",
            StorageKind::Session => "session",
        }
    }

    /// CLI command-name fragment.
    pub fn cli_name(self) -> &'static str {
        match self {
            StorageKind::Local => "local-storage",
            StorageKind::Session => "session-storage",
        }
    }
}
