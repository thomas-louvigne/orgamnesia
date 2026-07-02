use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum AppError {
    Io(String),
    VaultNotFound(String),
    ExportError(String),
    SettingsError(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::Io(e)           => write!(f, "IO error: {e}"),
            AppError::VaultNotFound(e) => write!(f, "Vault not found: {e}"),
            AppError::ExportError(e)  => write!(f, "Export error: {e}"),
            AppError::SettingsError(e) => write!(f, "Settings error: {e}"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<AppError> for String {
    fn from(e: AppError) -> Self {
        e.to_string()
    }
}
