/// Collection of GTK widgets utilized by a [FileItem] within the grid view.
pub struct FileWidgets {
    pub card_box: gtk::Box,
    pub icon_widget: gtk::Image,
    pub video_widget: gtk::Video,
    pub preview_stack: gtk::Stack,
    /// Wraps `preview_stack` so `file_emblem` can sit on the icon's corner.
    pub icon_overlay: gtk::Overlay,
    pub file_emblem: gtk::Label,
    /// Folder previews, weight bar and stats line (also stored on the root widget).
    pub folder_info: FolderInfoWidgets,
    /// List-mode detail cells, one label per `list_columns::ListColumn` in order.
    pub columns_box: gtk::Box,
    pub column_labels: Vec<gtk::Label>,
    pub lock_icon: gtk::Image,
    pub label: gtk::Label,
    pub stack: gtk::Stack,
    pub drag_source: gtk::DragSource,
    pub drop_target: gtk::DropTarget,
    pub info_label: gtk::Label,
    pub label_scroller: gtk::ScrolledWindow,
    pub scale_css_provider: Option<gtk::CssProvider>,
    pub git_badge: gtk::Label,
}

/// Widgets that show what's inside a folder; cloned onto the item root as
/// `"folder_info"` data so live updates can reach an on-screen cell.
#[derive(Clone)]
pub struct FolderInfoWidgets {
    /// 2×2 grid of `gtk::Picture`s showing media inside the folder.
    pub preview: gtk::Grid,
    /// Grid mode: composition bar under the icon.
    pub bar: gtk::Box,
    /// Grid mode: "1.2 GB · 340 items" under the name.
    pub stats_label: gtk::Label,
    /// List mode: the Size column label and the mini bar under it.
    pub size_label: gtk::Label,
    pub size_bar: gtk::Box,
}
