use serde::{Serialize, Serializer};

/// Why a backend operation failed. Commands return it as is: the interface
/// receives its message.
#[derive(Debug)]
pub enum AppError {
    Io(std::io::Error),
    /// No project is open.
    NoProject,
    /// The folder chosen as a project does not exist (or is not a folder).
    VaultNotFound(String),
    /// The path is not a page of the open project.
    NotAPage(String),
    /// A page with this name already exists.
    PageExists(String),
    /// A page name must be non-empty and hold no `/` or `\`.
    InvalidName(String),
    /// The export tool failed or could not be run.
    Export(String),
    /// `settings.json` / `keybindings.json` could not be written.
    Settings(String),
    /// The folder picker could not be shown.
    Dialog,
    /// A git command failed; what it said.
    Git(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e)            => write!(f, "{e}"),
            Self::NoProject        => write!(f, "No project open"),
            Self::VaultNotFound(p) => write!(f, "Project folder not found: {p}"),
            Self::NotAPage(p)      => write!(f, "Not a page of the open project: {p}"),
            Self::PageExists(n)    => write!(f, "Page '{n}' already exists"),
            Self::InvalidName(n)   => write!(f, "Invalid page name: '{n}'"),
            Self::Export(e)        => write!(f, "Export failed: {e}"),
            Self::Settings(e)      => write!(f, "Settings error: {e}"),
            Self::Dialog           => write!(f, "The folder picker could not be shown"),
            Self::Git(e)           => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self { Self::Io(e) }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self { Self::Settings(e.to_string()) }
}

/// Sent to the interface as its message.
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
