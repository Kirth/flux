use crate::model::{AppMsg, FluxApp, SortBy};
use crate::ui::constants;
use crate::utils;
use relm4::prelude::*;

impl FluxApp {
    /// Toggles between grid card layout and compact list view.
    pub fn handle_toggle_list_mode(&mut self) {
        self.is_list_mode = !self.is_list_mode;
        self.saved_list_mode = self.is_list_mode;
        self.config.default_list_mode = self.is_list_mode;
        utils::save_config(&self.config);

        let is_list = self.is_list_mode;
        let new_icon_size = if is_list {
            self.current_list_icon_size
        } else {
            self.current_icon_size
        };

        let active_tab = match self.tabs.get_mut(self.active_tab_index) {
            Some(t) => t,
            None => return,
        };

        if is_list {
            active_tab.files.view.set_min_columns(1);
            active_tab.files.view.set_max_columns(1);
        } else {
            active_tab.files.view.set_min_columns(1);
            active_tab.files.view.set_max_columns(20);
        }

        for i in 0..active_tab.files.len() {
            if let Some(item_wrapper) = active_tab.files.get(i) {
                let mut item = item_wrapper.borrow().clone();
                item.is_list_mode = is_list;
                item.icon_size = new_icon_size;
                active_tab.files.remove(i);
                active_tab.files.insert(i, item);
            }
        }

        self.sync_list_mode();
    }

    /// Toggles between ascending and descending sort order.
    pub fn handle_toggle_sort_order(&mut self, sender: &AsyncComponentSender<Self>) {
        self.sort_ascending = !self.sort_ascending;
        let _ = self.state_db.save_view(
            &self.current_path,
            &format!("{:?}", self.sort_by),
            !self.sort_ascending,
            self.current_icon_size as u32,
            self.config.ui.folders_first,
        );
        sender.input(AppMsg::Refresh);
    }

    /// List-view header click: sorts by `sort`, or flips the direction if already sorted by it.
    pub fn handle_sort_by_column(&mut self, sort: SortBy, sender: &AsyncComponentSender<Self>) {
        if self.sort_by == sort {
            self.sort_ascending = !self.sort_ascending;
        } else {
            self.sort_by = sort;
            self.sort_ascending = true;
        }
        let _ = self.state_db.save_view(
            &self.current_path,
            &format!("{:?}", self.sort_by),
            !self.sort_ascending,
            self.current_icon_size as u32,
            self.config.ui.folders_first,
        );
        sender.input(AppMsg::Refresh);
    }

    /// Shows or hides a list-view detail column and persists the choice.
    pub fn handle_toggle_list_column(&mut self, key: &str) {
        use crate::ui::list_columns::{self, ListColumn};
        let Some(col) = ListColumn::from_key(key) else {
            return;
        };
        let visible = !list_columns::is_visible(col);
        self.config.ui.list_columns.retain(|k| k != key);
        if visible {
            self.config.ui.list_columns.push(key.to_string());
        }
        utils::save_config(&self.config);
        list_columns::set_visible(col, visible);
    }

    /// Persists a list-view column width after the user finished dragging it.
    pub fn handle_set_list_column_width(&mut self, key: String, px: i32) {
        self.config.ui.list_column_widths.insert(key, px);
        utils::save_config(&self.config);
    }

    /// Cycles through available sorting criteria (Name -> Date -> Size -> Type).
    pub fn handle_cycle_sort(&mut self, sender: &AsyncComponentSender<Self>) {
        self.sort_by = match self.sort_by {
            SortBy::Name => SortBy::Date,
            SortBy::Date => SortBy::Size,
            SortBy::Size => SortBy::Type,
            SortBy::Type => SortBy::Name,
        };
        let _ = self.state_db.save_view(
            &self.current_path,
            &format!("{:?}", self.sort_by),
            !self.sort_ascending,
            self.current_icon_size as u32,
            self.config.ui.folders_first,
        );
        sender.input(AppMsg::Refresh);
    }

    /// Toggles "Folders First" sorting priority for the current directory view.
    pub fn handle_cycle_folder_priority(&mut self, sender: &AsyncComponentSender<Self>) {
        let path = self.current_path.clone();
        let current_state = if let Ok(Some((_, _, _, ff))) = self.state_db.get_view(&path) {
            ff
        } else {
            self.config.ui.folders_first
        };
        let new_state = !current_state;

        let _ = self.state_db.save_view(
            &path,
            &format!("{:?}", self.sort_by),
            false,
            self.current_icon_size as u32,
            new_state,
        );
        self.load_path(path, sender);
    }

    /// Adjusts icon scale (Ctrl+scroll, Ctrl +/−), targeting only the active view mode.
    ///
    /// Grid mode updates `current_icon_size` and `config.ui.grid_icon_size` (one size for
    /// every folder); list mode updates `current_list_icon_size` and
    /// `config.ui.list_icon_size`. Only items of the current mode change, so switching
    /// modes always restores the independent size.
    pub fn handle_zoom(&mut self, delta: f64) {
        crate::hit!("handle_zoom");
        if delta == 0.0 {
            return;
        }
        let sizes = constants::CRISP_ICON_SIZES;
        let current = if self.is_list_mode {
            self.current_list_icon_size
        } else if self.current_icon_size > 0 {
            self.current_icon_size
        } else {
            self.config.ui.effective_grid_icon_size()
        };
        let current_idx = sizes
            .iter()
            .position(|&s| s >= current)
            .unwrap_or(sizes.len() - 1);
        // Scrolling down (positive delta) shrinks.
        let new_idx = if delta < 0.0 {
            (current_idx + 1).min(sizes.len() - 1)
        } else {
            current_idx.saturating_sub(1)
        };
        self.set_icon_size(sizes[new_idx]);
    }

    /// Ctrl 0: back to the configured size for the active view mode.
    pub fn handle_zoom_reset(&mut self) {
        if self.is_list_mode {
            self.set_icon_size(crate::model::UIConfig::default().list_icon_size);
        } else {
            self.config.ui.grid_icon_size = 0;
            self.set_icon_size(self.config.ui.effective_grid_icon_size());
        }
    }

    /// Applies and saves `size` for the active view mode, refreshing its items.
    fn set_icon_size(&mut self, size: i32) {
        let list = self.is_list_mode;
        let current = if list {
            self.current_list_icon_size
        } else {
            self.current_icon_size
        };
        if list {
            self.current_list_icon_size = size;
            self.config.ui.list_icon_size = size;
        } else {
            self.current_icon_size = size;
            // Kept at 0 after a reset to the default, so a changed default applies.
            if self.config.ui.grid_icon_size != 0
                || size != self.config.ui.effective_grid_icon_size()
            {
                self.config.ui.grid_icon_size = size;
            }
        }
        utils::save_config(&self.config);
        if size == current {
            return;
        }

        let active_files = &mut self.tabs[self.active_tab_index].files;
        for i in 0..active_files.len() {
            if let Some(item_wrapper) = active_files.get(i) {
                if item_wrapper.borrow().is_list_mode == list {
                    let mut item = item_wrapper.borrow().clone();
                    item.icon_size = size;
                    active_files.remove(i);
                    active_files.insert(i, item);
                }
            }
        }
    }
}
