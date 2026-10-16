use super::state::TabState;
use crate::model::{AppMsg, FluxApp};
use gtk::prelude::*;
use relm4::prelude::*;
use std::path::PathBuf;

impl FluxApp {
    pub fn handle_new_tab(
        &mut self,
        target_path: Option<PathBuf>,
        sender: &AsyncComponentSender<Self>,
    ) {
        let path = target_path.unwrap_or_else(|| self.current_path.clone());
        self.open_tab_in_pane(self.active_pane(), path, sender);
    }

    /// Opens `path` in a new, active tab at the end of `pane`'s tab strip.
    pub(crate) fn open_tab_in_pane(
        &mut self,
        pane: super::panes::Pane,
        path: PathBuf,
        sender: &AsyncComponentSender<Self>,
    ) {
        self.next_tab_id += 1;
        let tab_id = self.next_tab_id;

        let mut new_tab = TabState::new(
            tab_id,
            path.clone(),
            self.is_list_mode,
            self.config.ui.default_sort,
        );
        new_tab.pane = pane;

        // Do not mark initialized yet, let load_path or SwitchTab do it
        new_tab.is_initialized = true;

        self.wire_tab_signals(&mut new_tab, sender);
        self.save_active_tab_state();

        let title = new_tab.title.clone();
        let view = self.tab_view_for(pane).clone();
        let page = view.append(&new_tab.scroller);
        page.set_title(&title);
        page.set_icon(Some(&gtk::gio::ThemedIcon::new("folder-symbolic")));
        unsafe {
            page.set_data("flux_tab_id", tab_id);
        }

        self.tabs.push(new_tab);
        self.active_tab_index = self.tabs.len() - 1;

        if view.selected_page().as_ref() != Some(&page) {
            view.set_selected_page(&page);
        }
        self.update_panes();

        self.load_path(path, sender);
    }

    pub fn handle_open_tabs(&mut self, targets: Vec<PathBuf>, sender: &AsyncComponentSender<Self>) {
        if targets.is_empty() {
            return;
        }

        let total = targets.len();
        for (i, path) in targets.into_iter().enumerate() {
            let is_last = i == total - 1;
            self.next_tab_id += 1;
            let tab_id = self.next_tab_id;

            let mut new_tab = TabState::new(
                tab_id,
                path.clone(),
                self.is_list_mode,
                self.config.ui.default_sort,
            );

            // Background tabs stay uninitialized until selected
            new_tab.is_initialized = is_last;

            self.wire_tab_signals(&mut new_tab, sender);

            let pane = self.active_pane();
            new_tab.pane = pane;
            let title = new_tab.title.clone();
            let view = self.tab_view_for(pane).clone();
            let page = view.append(&new_tab.scroller);
            page.set_title(&title);
            page.set_icon(Some(&gtk::gio::ThemedIcon::new("folder-symbolic")));
            unsafe {
                page.set_data("flux_tab_id", tab_id);
            }

            self.tabs.push(new_tab);

            // Only activate and load the last tab
            if is_last {
                self.save_active_tab_state();
                self.active_tab_index = self.tabs.len() - 1;
                view.set_selected_page(&page);
                self.load_path(path, sender);
            }
        }
    }

    pub fn handle_switch_tab(&mut self, index: usize, sender: &AsyncComponentSender<Self>) {
        if index >= self.tabs.len() || index == self.active_tab_index {
            return;
        }

        self.save_active_tab_state();

        self.active_tab_index = index;

        let (target_path, needs_initial_load, thumbs_ready) = {
            let tab = &mut self.tabs[index];
            let needs_load = !tab.is_initialized;
            tab.is_initialized = true;

            self.current_path = tab.current_path.clone();
            self.history = tab.history.clone();
            self.forward_stack = tab.forward_stack.clone();
            self.recent_stack = tab.recent_stack.clone();
            self.filter = tab.filter.clone();
            self.sort_by = tab.sort_by;
            self.sort_ascending = tab.sort_ascending;
            self.is_list_mode = tab.is_list_mode;
            self.selection_status = tab.selection_status.clone();

            (tab.current_path.clone(), needs_load, tab.thumbs_ready)
        };

        // Only update the selected page if it's not already active to avoid recursive notification cycles
        self.select_tab_page(index);

        self.update_breadcrumbs();
        self.sync_sidebar_selection();

        // If the tab was interrupted or hasn't finished its thumbnails, kick it back into gear!
        if !thumbs_ready {
            let sender_clone = sender.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(50), move || {
                sender_clone.input(AppMsg::CheckVisibleThumbnails);
            });
        }

        if let Some(entry) = self.header_path_entry.upgrade() {
            entry.set_text(&self.current_path.to_string_lossy());
            entry.set_position(entry.text_length() as i32);
        }

        if needs_initial_load {
            self.load_path(target_path, sender);
        } else {
            // Immediately evaluate visible items and trigger pending thumbnail tasks for the active tab
            self.check_visible_thumbnails(sender);
        }
    }

    /// Ctrl+W (`None`) closes the active tab through its tab view, which then reports
    /// back via `CloseTabId`; `Some(index)` closes that tab the same way.
    pub fn handle_close_tab(
        &mut self,
        index_opt: Option<usize>,
        _sender: &AsyncComponentSender<Self>,
    ) {
        let index = index_opt.unwrap_or(self.active_tab_index);
        if self.tabs.len() <= 1 {
            if let Some(win) = gtk::Application::default().active_window() {
                win.close();
            }
            return;
        }
        if let (Some(tab), Some(page)) = (self.tabs.get(index), self.tab_page(index)) {
            self.tab_view_for(tab.pane).close_page(&page);
        }
    }

    /// Removes a closed tab from the model and activates whichever tab its pane (or
    /// the other pane, if it emptied) now shows.
    pub fn handle_close_tab_id(&mut self, id: u64, sender: &AsyncComponentSender<Self>) {
        let Some(index) = self.tab_index_by_id(id) else {
            return;
        };
        let was_active = index == self.active_tab_index;
        let pane = self.tabs[index].pane;
        let active_id = self.tabs.get(self.active_tab_index).map(|t| t.id);
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            return;
        }
        self.ensure_panes_not_empty();

        if !was_active {
            if let Some(i) = active_id.and_then(|id| self.tab_index_by_id(id)) {
                self.active_tab_index = i;
            }
            self.update_panes();
            return;
        }

        // The closed tab was active: its state is gone, so load the new one directly.
        let next_id = [pane, pane.other()]
            .into_iter()
            .find_map(|p| self.tab_view_for(p).selected_page())
            .map(|p| super::panes::page_tab_id(&p));
        self.active_tab_index = next_id.and_then(|id| self.tab_index_by_id(id)).unwrap_or(0);

        let (target_path, needs_initial_load) = {
            let tab = &mut self.tabs[self.active_tab_index];
            let needs_load = !tab.is_initialized;
            tab.is_initialized = true;

            self.current_path = tab.current_path.clone();
            self.history = tab.history.clone();
            self.forward_stack = tab.forward_stack.clone();
            self.recent_stack = tab.recent_stack.clone();
            self.filter = tab.filter.clone();
            self.sort_by = tab.sort_by;
            self.sort_ascending = tab.sort_ascending;
            self.is_list_mode = tab.is_list_mode;
            self.selection_status = tab.selection_status.clone();

            (tab.current_path.clone(), needs_load)
        };
        self.select_tab_page(self.active_tab_index);
        self.update_panes();

        self.update_breadcrumbs();
        self.sync_sidebar_selection();

        if let Some(entry) = self.header_path_entry.upgrade() {
            entry.set_text(&self.current_path.to_string_lossy());
            entry.set_position(entry.text_length() as i32);
        }

        if needs_initial_load {
            self.load_path(target_path, sender);
        }
    }

    /// Copies the live view state (mirrored in `FluxApp`) back into the active tab.
    pub(crate) fn save_active_tab_state(&mut self) {
        if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
            tab.current_path = self.current_path.clone();
            tab.history = self.history.clone();
            tab.forward_stack = self.forward_stack.clone();
            tab.recent_stack = self.recent_stack.clone();
            tab.filter = self.filter.clone();
            tab.sort_by = self.sort_by;
            tab.sort_ascending = self.sort_ascending;
            tab.is_list_mode = self.is_list_mode;
            tab.selection_status = self.selection_status.clone();
            tab.scroll_offset = tab.scroller.vadjustment().value();
        }
    }
}
