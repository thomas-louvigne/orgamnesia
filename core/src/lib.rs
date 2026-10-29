//! What the backend (`src-tauri`) and the interface (`src`) must agree on: the
//! data they exchange, the settings and their defaults, the way `#tags` are
//! read (`#tags`, `:tags:`), the TODO keywords and the shortcut registry. Plain Rust, no I/O.

pub mod assets;
pub mod config;
pub mod hashtags;
pub mod keybindings;
pub mod layout;
pub mod links;
pub mod model;
pub mod names;
pub mod search;
pub mod settings;
pub mod spell;
pub mod todo;

pub use hashtags::{Hashtags, OrgTags, TagSyntax};
pub use layout::{Frame, Layout, Move};
pub use model::*;
pub use search::{PageHits, ReplaceOutcome, ReplaceSkip, SearchHit, SearchOptions, SearchResults};
pub use settings::{Config, Prefs, Session, Settings, SplitKind};
pub use todo::TodoKeywords;
