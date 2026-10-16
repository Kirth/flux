//! `FluxApp` operation handlers, grouped by domain.
//!
//! Most modules only add `handle_*` methods to `FluxApp`, `paste` also exposes
//! the shared task-id counter and `perform_file_op`.

pub mod paste;

mod app_action;
pub mod file;
mod git;
mod preview;
mod remote;
mod sidebar;
mod sidebar_tree;
mod tag;
mod task;
mod terminal;
mod toast;
mod undo;
mod view;
pub use view::install_focus_tracking;
mod watcher;

pub(crate) use watcher::debounce_path_event;
