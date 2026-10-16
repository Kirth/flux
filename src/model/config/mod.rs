use serde::{Deserialize, Serialize};

mod actions;
mod places;
mod shortcuts;
mod terminal;
mod thumbnails;
mod ui;

pub use actions::{ContextAction, CustomAction, MenuEntry};
pub use places::{CustomPlace, DeviceRename};
pub use shortcuts::ShortcutsConfig;
pub use terminal::TerminalConfig;
pub use thumbnails::ThumbnailTypes;
pub use ui::UIConfig;

/// Top-level configuration structure for persistent application settings.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Config {
    /// Visual and behavioral settings for the application window and widgets.
    pub ui: UIConfig,
    /// A collection of user-defined bookmarks and places for the sidebar.
    #[serde(default)]
    pub sidebar: Vec<CustomPlace>,
    /// Plain jump links shown at the top of the sidebar (`[[quick_links]]`): they
    /// navigate on click and, unlike `sidebar` entries, never expand into a tree.
    #[serde(default)]
    pub quick_links: Vec<CustomPlace>,
    /// Custom keybindings for navigating and managing files.
    #[serde(default)]
    pub shortcuts: ShortcutsConfig,
    #[serde(default)]
    pub network_bookmarks: Vec<crate::services::network::NetworkBookmark>,
    #[serde(default)]
    pub default_list_mode: bool,
}
