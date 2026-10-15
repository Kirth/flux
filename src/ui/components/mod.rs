//! Reusable UI building blocks: grid file items, sidebar rows and breadcrumb segments.

mod bind;
mod breadcrumb;
mod builder;
mod file_item;
mod grid_item;
mod setup;
mod sidebar;
mod widgets;

pub(crate) use bind::{apply_folder_info_to_cell, build_composition_bar, restyle_clipboard_marks};
pub use file_item::FileItem;
pub use sidebar::{
    best_ancestor_root, is_virtual_sidebar_path, list_sidebar_children, SidebarChild, SidebarMsg,
    SidebarPlace, SidebarPlaceInit,
};
