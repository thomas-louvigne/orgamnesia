use serde::{Serialize, Serializer};

/// Why a backend operation failed. Commands return it as is: the interface
/// receives its code and details, and words it in its language.
#[derive(Debug)]
pub enum AppError {
    Io(std::io::Error),
    /// No project is open.
    NoProject,
    /// The folder chosen as a project does not exist.
    VaultNotFound(String),
    /// The folder chosen as a project is not a folder.
    NotAFolder(String),
    /// The path is not a page of the open project.
    NotAPage(String),
    /// A page with this name already exists.
    PageExists(String),
    /// A page name must be non-empty and hold no `/` or `\`.
    InvalidName(String),
    /// Org-bloom failed; what it said.
    OrgBloom(String),
    /// The org-bloom command was not found (the one tried).
    OrgBloomNotFound(String),
    /// A program could not be run, and why.
    CannotRun { program: String, error: String },
    /// A folder to show does not exist.
    FolderNotFound(String),
    /// `settings.json` / `keybindings.json` could not be written.
    Settings(String),
    /// The folder picker could not be shown.
    Dialog,
    /// A git command failed; what it said.
    Git(String),
    /// The project's repository has no remote to pull from or push to.
    NoRemote,
    /// The file added as an image is not one (by its extension).
    NotAnImage(String),
    /// No spelling dictionary found for this language.
    NoDictionary(String),
    /// A spelling dictionary could not be read, and why.
    BadDictionary(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e)            => write!(f, "{e}"),
            Self::NoProject        => write!(f, "No project open"),
            Self::VaultNotFound(p) => write!(f, "Project folder not found: {p}"),
            Self::NotAFolder(p)    => write!(f, "Not a folder: {p}"),
            Self::NotAPage(p)      => write!(f, "Not a page of the open project: {p}"),
            Self::PageExists(n)    => write!(f, "Page '{n}' already exists"),
            Self::InvalidName(n)   => write!(f, "Invalid page name: '{n}'"),
            Self::OrgBloom(e)      => write!(f, "{e}"),
            Self::OrgBloomNotFound(c) => write!(f, "'{c}' not found: install org-bloom, or set its path in the settings"),
            Self::CannotRun { program, error } => write!(f, "cannot run '{program}': {error}"),
            Self::FolderNotFound(p) => write!(f, "Folder not found: {p}"),
            Self::Settings(e)      => write!(f, "Settings error: {e}"),
            Self::Dialog           => write!(f, "The folder picker could not be shown"),
            Self::Git(e)           => write!(f, "{e}"),
            Self::NoRemote         => write!(f, "no remote: add one with git remote add origin <url>"),
            Self::NotAnImage(p)    => write!(f, "Not an image: {p}"),
            Self::NoDictionary(l)  => write!(f, "No spelling dictionary for {l}"),
            Self::BadDictionary(e) => write!(f, "Unreadable spelling dictionary: {e}"),
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

impl AppError {
    /// What the interface gets: a code it has a text for, the detail that goes
    /// with it (a path, a name, what a tool said), and for `cannot_run` the cause.
    fn parts(&self) -> (&'static str, Option<String>, Option<String>) {
        let some = |s: &String| Some(s.clone());
        match self {
            Self::Io(e)            => ("io", Some(e.to_string()), None),
            Self::NoProject        => ("no_project", None, None),
            Self::VaultNotFound(p) => ("vault_not_found", some(p), None),
            Self::NotAFolder(p)    => ("not_a_folder", some(p), None),
            Self::NotAPage(p)      => ("not_a_page", some(p), None),
            Self::PageExists(n)    => ("page_exists", some(n), None),
            Self::InvalidName(n)   => ("invalid_name", some(n), None),
            Self::OrgBloom(e)      => ("tool_output", some(e), None),
            Self::OrgBloomNotFound(c) => ("org_bloom_not_found", some(c), None),
            Self::CannotRun { program, error } => ("cannot_run", some(program), some(error)),
            Self::FolderNotFound(p) => ("folder_not_found", some(p), None),
            Self::Settings(e)      => ("settings", some(e), None),
            Self::Dialog           => ("dialog", None, None),
            Self::Git(e)           => ("tool_output", some(e), None),
            Self::NoRemote         => ("no_remote", None, None),
            Self::NotAnImage(p)    => ("not_an_image", some(p), None),
            Self::NoDictionary(l)  => ("no_dictionary", some(l), None),
            Self::BadDictionary(e) => ("bad_dictionary", some(e), None),
        }
    }
}

/// Sent to the interface as `{ code, detail, cause }`.
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let (code, detail, cause) = self.parts();
        let mut st = s.serialize_struct("AppError", 3)?;
        st.serialize_field("code", code)?;
        st.serialize_field("detail", &detail)?;
        st.serialize_field("cause", &cause)?;
        st.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sent_as_code_and_detail() {
        let json = |e: AppError| serde_json::to_string(&e).unwrap();
        assert_eq!(json(AppError::PageExists("Idée".into())), r#"{"code":"page_exists","detail":"Idée","cause":null}"#);
        assert_eq!(json(AppError::NoProject), r#"{"code":"no_project","detail":null,"cause":null}"#);
        assert_eq!(
            json(AppError::CannotRun { program: "xdg-open".into(), error: "denied".into() }),
            r#"{"code":"cannot_run","detail":"xdg-open","cause":"denied"}"#
        );
    }
}
