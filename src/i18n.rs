#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Lang {
    #[default]
    Fr,
    En,
}

impl Lang {
    pub fn from_code(s: &str) -> Self {
        match s {
            "en" => Self::En,
            _ => Self::Fr,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Fr => "fr",
        }
    }
}

thread_local! {
    /// The language of the interface, for what is worded outside the components
    /// (the backend's errors, the crash message). Kept by `App`.
    static CURRENT: std::cell::Cell<Lang> = std::cell::Cell::new(Lang::default());
}

pub fn set_current(lang: Lang) {
    CURRENT.with(|c| c.set(lang));
}

pub fn current() -> Lang {
    CURRENT.with(|c| c.get())
}

mod panels;
mod settings;
mod editing;
mod git;
mod dialogs;
mod status;

/// The text of `key` in `lang`; the key itself when it has none.
pub fn t(key: &'static str, lang: Lang) -> &'static str {
    [panels::t, settings::t, editing::t, git::t, dialogs::t, status::t]
        .iter().find_map(|t| t(key, lang)).unwrap_or(key)
}

#[cfg(test)]
mod tests;
