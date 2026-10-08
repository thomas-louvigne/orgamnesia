//! What the backend (`src-tauri`) and the interface (`src`) must agree on: the
//! data they exchange, the settings and their defaults, the way `#tags` are
//! read and the shortcut registry. Plain Rust, no I/O.

pub mod hashtags;
pub mod keybindings;
pub mod model;
pub mod names;
pub mod settings;

pub use hashtags::Hashtags;
pub use model::*;
pub use settings::{Prefs, Session, Settings, SplitKind};
