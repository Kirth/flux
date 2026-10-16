use crate::model::{AppMsg, FluxApp};
use crate::ui::constants;
use crate::utils;
use adw::gdk;
use adw::gio::prelude::*;
use adw::prelude::*;
use gtk::gio;
use relm4::prelude::*;
use std::path::PathBuf;

impl FluxApp {
    /// Registers actions for quick transfer operations (move and copy) from the context menu.
    pub fn setup_quick_transfer_actions(&self, sender: &AsyncComponentSender<Self>) {
        let q_move =
            gio::SimpleAction::new("quick-transfer-move", Some(&String::static_variant_type()));
        let s_move = sender.clone();
        q_move.connect_activate(move |_, param| {
            if let Some(dest_str) = param.and_then(|p| p.str()) {
                let parts: Vec<&str> = dest_str.splitn(2, ':').collect();
                if parts.len() == 2 {
                    let dest = PathBuf::from(parts[1]);
                    s_move.input(AppMsg::PerformQuickTransfer { dest, is_cut: true });
                }
            }
        });
        self.action_group.add_action(&q_move);

        let q_copy =
            gio::SimpleAction::new("quick-transfer-copy", Some(&String::static_variant_type()));
        let s_copy = sender.clone();
        q_copy.connect_activate(move |_, param| {
            if let Some(dest_str) = param.and_then(|p| p.str()) {
                let parts: Vec<&str> = dest_str.splitn(2, ':').collect();
                if parts.len() == 2 {
                    let dest = PathBuf::from(parts[1]);
                    s_copy.input(AppMsg::PerformQuickTransfer {
                        dest,
                        is_cut: false,
                    });
                }
            }
        });
        self.action_group.add_action(&q_copy);
    }

    /// Dispatches opening for a specific grid position or current multi-selection.
    pub fn handle_open(
        &mut self,
        position: Option<u32>,
        tab_id: Option<u64>,
        sender: &AsyncComponentSender<Self>,
    ) {
        self.stop_video_preview();

        let modifiers = gdk::Display::default()
            .and_then(|d| d.default_seat())
            .and_then(|s| s.keyboard())
            .map(|k| k.modifier_state())
            .unwrap_or(gdk::ModifierType::empty());

        if modifiers.intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK) {
            return;
        }

        let resolved_tab_idx = tab_id
            .and_then(|id| self.tabs.iter().position(|t| t.id == id))
            .unwrap_or(self.active_tab_index);

        let active_files = match self.tabs.get(resolved_tab_idx) {
            Some(tab) => &tab.files,
            None => return,
        };

        let items: Vec<(PathBuf, bool)> = if let Some(pos) = position {
            let query_lc = self.filter.to_lowercase();

            if self.filter.is_empty() || self.is_content_searching {
                active_files
                    .get(pos)
                    .map(|w| {
                        let item = w.borrow();
                        vec![(item.path.clone(), item.is_dir)]
                    })
                    .unwrap_or_default()
            } else if let Some((tags, rest)) = crate::utils::search::parse_tag_filter(&query_lc) {
                if let Some(wrapper) = active_files.get(pos) {
                    let item = wrapper.borrow();
                    vec![(item.path.clone(), item.is_dir)]
                } else {
                    let rest_clean = rest.trim().to_lowercase();
                    let target_tags: Vec<String> =
                        tags.into_iter().map(|t| t.to_lowercase()).collect();
                    let mut match_count = 0u32;
                    let mut found = None;

                    for i in 0..active_files.len() {
                        if let Some(wrapper) = active_files.get(i) {
                            let item = wrapper.borrow();
                            let name_ok = rest_clean.is_empty()
                                || item.name.to_lowercase().contains(&rest_clean);

                            if name_ok {
                                let file_tags = crate::utils::xattr::read_tags(&item.path);
                                let file_tags_lc: Vec<String> =
                                    file_tags.into_iter().map(|t| t.to_lowercase()).collect();

                                if target_tags.iter().all(|req| file_tags_lc.contains(req)) {
                                    if match_count == pos {
                                        found = Some((item.path.clone(), item.is_dir));
                                        break;
                                    }
                                    match_count += 1;
                                }
                            }
                        }
                    }
                    found.map(|f| vec![f]).unwrap_or_default()
                }
            } else {
                // The nth item the search box's filter shows.
                let query = crate::utils::search::parse_query(&self.filter);
                let mut match_count = 0u32;
                let mut found = None;
                for i in 0..active_files.len() {
                    if let Some(wrapper) = active_files.get(i) {
                        let item = wrapper.borrow();
                        if item.shown_by(&query) {
                            if match_count == pos {
                                found = Some((item.path.clone(), item.is_dir));
                                break;
                            }
                            match_count += 1;
                        }
                    }
                }
                found.map(|f| vec![f]).unwrap_or_default()
            }
        } else {
            self.get_selection_with_meta_for_tab(resolved_tab_idx)
        };

        if items.is_empty() {
            return;
        }

        self.activate_items(items, sender);
    }

    /// Handles explicit primary item activation (e.g. Return / Double-click).
    pub fn handle_activate(&mut self, sender: &AsyncComponentSender<Self>) {
        self.stop_video_preview();

        let modifiers = gdk::Display::default()
            .and_then(|d| d.default_seat())
            .and_then(|s| s.keyboard())
            .map(|k| k.modifier_state())
            .unwrap_or(gdk::ModifierType::empty());

        let is_selecting =
            modifiers.intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK);
        if is_selecting {
            return;
        }

        let items = self.get_selection_with_meta();
        if items.is_empty() {
            return;
        }

        self.activate_items(items, sender);
    }

    /// Launches selection using a Desktop Entry ID.
    pub fn handle_launch_with_app(&self, app_id: String) {
        if let Some(app_info) = gio_unix::DesktopAppInfo::new(&app_id) {
            let selection = self.get_selection();
            if selection.is_empty() {
                return;
            }

            let files: Vec<gio::File> = selection.into_iter().map(gio::File::for_path).collect();
            let context = gdk::Display::default().map(|display| display.app_launch_context());
            let launch_result = app_info.launch(&files, context.as_ref());

            if let Err(e) = launch_result {
                eprintln!("[Launch Error] {:?}: {}", app_info.display_name(), e);
            }
        }
    }

    /// Clears selected or all entries from GTK recents.
    pub fn handle_clear_recents(&mut self, sender: &AsyncComponentSender<Self>) {
        let selection = self.get_selection();
        let result = if selection.is_empty() {
            Self::remove_recents(None)
        } else {
            Self::remove_recents(Some(&selection))
        };

        match result {
            Ok(()) => {
                sender.input(AppMsg::Refresh);
                self.recents_has_selection = false;
            }
            Err(e) => {
                sender.input(AppMsg::ShowToast(format!("Failed to clear recents: {}", e)));
            }
        }
    }

    /// Selects the item at `path` in the active view if it isn't selected yet.
    fn select_clicked_item(&self, path: &std::path::Path) {
        let Some(tab) = self.tabs.get(self.active_tab_index) else {
            return;
        };
        let files = &tab.files;
        let selection = &files.selection_model;
        // Visible positions: the selection model sits on the filtered list.
        let Some(pos) = (0..selection.n_items()).find(|&i| {
            files
                .get_visible(i)
                .is_some_and(|w| w.borrow().path.as_path() == path)
        }) else {
            return;
        };
        if !selection.is_selected(pos) {
            selection.select_item(pos, true);
        }
        files.view.grab_focus();
    }

    /// Resolves target selection coordinates and triggers background MIME calculation for the context menu.
    pub fn handle_prepare_context_menu(
        &self,
        x: f64,
        y: f64,
        path: Option<PathBuf>,
        sender: &AsyncComponentSender<Self>,
    ) {
        // Right-clicking an item selects it (and focuses the view), unless it's already
        // part of the selection: then the menu acts on the whole selection, as usual.
        if let Some(clicked) = &path {
            self.select_clicked_item(clicked);
        }
        let resolved_path = path.or_else(|| self.get_selected_path());

        let sender_ctx = sender.clone();
        relm4::spawn_blocking(move || {
            let mime = resolved_path
                .as_ref()
                .map(|p| utils::media::get_mime_type(p))
                .unwrap_or_else(|| constants::MIME_DIR.to_string());

            sender_ctx.input(AppMsg::ShowContextMenu {
                x,
                y,
                path: resolved_path,
                mime,
            });
        });
    }
}
