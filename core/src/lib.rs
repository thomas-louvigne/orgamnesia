//! What the backend (`src-tauri`) and the interface (`src`) must agree on: the
//! data they exchange, the settings and their defaults, the way `#tags` are
//! read (`#tags`, `:tags:`), the TODO keywords and the shortcut registry. Plain Rust, no I/O.

pub mod agenda;
pub mod assets;
pub mod capture;
pub mod complete;
pub mod config;
pub mod edit;
pub mod folding;
pub mod format;
pub mod frontmatter;
pub mod hashtags;
pub mod highlight;
pub mod keybindings;
pub mod layout;
pub mod links;
pub mod model;
pub mod motion;
pub mod names;
pub mod pages;
pub mod org;
pub mod search;
pub mod settings;
pub mod spell;
pub mod table;
pub mod tips;
pub mod tags;
pub mod templates;
pub mod todo;
pub mod tree;

pub use format::{Format, Formats};
pub use hashtags::{Hashtags, OrgTags, TagSyntax};
pub use layout::{Frame, Layout, Move};
pub use model::*;
pub use search::{PageHits, ReplaceOutcome, ReplaceSkip, SearchHit, SearchOptions, SearchResults};
pub use settings::{Config, Prefs, Session, Settings, SplitKind};
pub use todo::TodoKeywords;
