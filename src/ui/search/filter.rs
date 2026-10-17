use crate::model::{AppMsg, FluxApp};
use crate::utils;
use crate::utils::search::SizeOp;
use gtk::glib;
use gtk::prelude::*;
use relm4::prelude::*;
use std::sync::atomic::Ordering;

impl FluxApp {
    /// Resets the filter and view layout state when content search or tag search is active.
    pub fn handle_update_filter(&mut self, query: String, sender: &AsyncComponentSender<Self>) {
        crate::hit!("handle_update_filter");
        if query.is_empty() && self.search_just_opened {
            self.search_just_opened = false;
            return;
        }
        let query_lc = query.to_lowercase();
        if query_lc.is_empty() {
            if self.is_content_searching {
                self.reset_from_content_search();
                sender.input(AppMsg::Refresh);
            } else {
                if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
                    tab.files.clear_filters();
                }
                sender.input(AppMsg::Refresh);
            }
            return;
        }

        // Check for content search trigger: starts with ':'
        if query_lc.starts_with(':')
            && !query_lc.starts_with(":tag:")
            && !query_lc.starts_with(":t:")
        {
            return;
        }

        // Check for tag filter (:tag:name, :t:name, #name) -> Global Search
        if let Some((tags, rest_query)) = crate::utils::search::parse_tag_filter(&query_lc) {
            self.filter = query.clone();

            let tag_title = format!("#{}", tags.join(", "));
            if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
                tab.title = tag_title.clone();
                if let Some(page) =
                    crate::ui::tabs::find_tab_page(&self.tab_view, &self.tab_view_right, tab.id)
                {
                    page.set_title(&tag_title);
                }
            }

            let (
                view_clone,
                list_mode,
                list_icon_size,
                current_icon_size,
                max_w,
                spacing,
                show_symlink,
                expand_labels,
                show_empty,
                hidden_extensions,
            ) = {
                let tab = &mut self.tabs[self.active_tab_index];
                tab.files.clear_filters();
                tab.files.clear();

                if self.is_list_mode {
                    tab.files.view.set_min_columns(1);
                    tab.files.view.set_max_columns(1);
                } else {
                    tab.files.view.set_min_columns(1);
                    tab.files.view.set_max_columns(20);
                }

                (
                    tab.files.view.clone(),
                    self.is_list_mode,
                    self.current_list_icon_size,
                    self.current_icon_size,
                    self.config.ui.max_width_chars,
                    self.config.ui.grid_spacing,
                    self.config.ui.show_symlink_emblem,
                    self.config.ui.expand_labels,
                    self.config.ui.show_empty_dir_emblem,
                    self.config.ui.hidden_extensions.clone(),
                )
            };

            let target_tags: Vec<String> = tags.into_iter().map(|t| t.to_lowercase()).collect();
            let filter_text = rest_query.trim().to_lowercase();

            let mut matching_paths = std::collections::BTreeSet::new();

            for tag in &target_tags {
                if let Ok(paths) = self.state_db.get_paths_for_tag(tag) {
                    for path in paths {
                        if !path.exists() {
                            continue;
                        }

                        // Check if file matches all requested tags
                        let file_tags = crate::utils::xattr::read_tags(&path);
                        let file_tags_lc: Vec<String> =
                            file_tags.into_iter().map(|t| t.to_lowercase()).collect();

                        if target_tags.iter().all(|req| file_tags_lc.contains(req)) {
                            matching_paths.insert(path);
                        }
                    }
                }
            }

            let mut media_tasks: Vec<(u32, std::path::PathBuf)> = Vec::new();
            let active_files = &mut self.tabs[self.active_tab_index].files;
            let mut grid_idx: u32 = active_files.len();

            for path in matching_paths {
                if !filter_text.is_empty() {
                    let filename_str = path
                        .file_name()
                        .map(|n| n.to_string_lossy())
                        .unwrap_or_default();
                    if !filename_str.to_lowercase().contains(&filter_text) {
                        continue;
                    }
                }

                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string_lossy().into_owned());

                let is_dir = path.is_dir();
                let meta = std::fs::metadata(&path).ok();
                let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                let mtime = meta
                    .as_ref()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);

                let icon = utils::icon::get_icon_for_path(&path, is_dir);

                if !is_dir {
                    let (is_img, is_vid) = utils::media::is_visual_media(&path);
                    if is_img || is_vid {
                        media_tasks.push((grid_idx, path.clone()));
                    }
                }

                let is_empty = if is_dir && show_empty {
                    FluxApp::is_dir_empty(&path)
                } else {
                    false
                };

                // Apply hidden_extensions to the display label, matching the
                // behavior of the main directory loader.
                let display_label =
                    crate::utils::helpers::format_display_label(&name, is_dir, &hidden_extensions);

                active_files.append(
                    crate::ui::FileItem::builder(name, path.clone(), icon)
                        .display_label(display_label)
                        .is_dir(is_dir)
                        .icon_size(if list_mode {
                            list_icon_size
                        } else {
                            current_icon_size
                        })
                        .size(size)
                        .mtime(mtime)
                        .is_empty(is_empty)
                        .expand_labels(expand_labels)
                        .is_list_mode(list_mode)
                        .grid_idx(active_files.len())
                        .max_width_chars(max_w)
                        .grid_spacing(spacing)
                        .show_symlink_emblem(show_symlink)
                        .build(),
                );
                grid_idx += 1;
            }

            let session_id = self.load_id.fetch_add(1, Ordering::SeqCst) + 1;
            self.spawn_thumbnail_loader(
                media_tasks,
                session_id,
                self.active_tab_index,
                sender.clone(),
            );

            glib::idle_add_local_once(move || {
                if let Some(model) = view_clone
                    .model()
                    .and_then(|m| m.downcast::<gtk::MultiSelection>().ok())
                {
                    model.unselect_all();
                }
            });
            return;
        }

        let q = crate::utils::search::parse_query(&query);

        // Globs (`*.iso`, `.webm`) search the subfolders too: lock into list mode as
        // soon as one is typed.
        if q.is_deep() {
            if !self.search_saved_layout {
                self.saved_list_mode = self.is_list_mode;
                self.saved_max_columns = self.tabs[self.active_tab_index].files.view.max_columns();
                self.search_saved_layout = true;
            }
            self.is_list_mode = true;
            let active_view = &self.tabs[self.active_tab_index].files.view;
            active_view.set_min_columns(1);
            active_view.set_max_columns(1);
            self.sync_list_mode();

            self.filter = query.clone();
            if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
                tab.files.clear_filters();
            }
            // Wait for something after the wildcard ("*." isn't worth a tree walk yet).
            let glob_text: String = q.globs.join(" ").replace(['*', '?', '[', ']'], "");
            if !glob_text.chars().any(|c| c.is_alphanumeric()) {
                return;
            }

            // Keeps the index fresh and prunes entries for deleted files.
            let clean_query = glob_text.trim_start_matches('.').trim().to_string();
            if crate::services::search::indexer::is_ready() && clean_query.len() >= 3 {
                crate::services::search::indexer::request_delta_scan();
                if let Ok(index) = crate::services::search::indexer::SearchIndex::open_or_create() {
                    if let Ok(matches) = index.query_in(&clean_query, Some(&self.current_path), 300)
                    {
                        let stale_paths: Vec<String> = matches
                            .into_iter()
                            .filter(|p| !p.exists())
                            .map(|p| p.to_string_lossy().to_string())
                            .collect();
                        if !stale_paths.is_empty() {
                            crate::services::search::indexer::prune_paths(stale_paths);
                        }
                    }
                }
            }

            // The walk filters by size and date itself; a range is walked as "larger
            // than its low end" and narrowed by the view filter below.
            let size_bytes = q.size.as_ref().map(|op| match *op {
                SizeOp::Gt(v) => (true, v),
                SizeOp::Lt(v) => (false, v),
                SizeOp::Range(l, r) => (true, l.min(r).saturating_sub(1)),
            });
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64);
            let date_seconds = q
                .modified_after
                .map(|t| now.saturating_sub(t).max(0) as u64);
            // Name words and exact ranges are checked per result before it's shown.
            sender.input(AppMsg::StartAdvancedSearch(
                crate::services::search::AdvancedSearchParams {
                    patterns: q.globs.clone(),
                    size_bytes,
                    date_seconds,
                    include_hidden: self.show_hidden,
                    result_filter: Some(Box::new(q)),
                    ..Default::default()
                },
            ));
            return;
        }

        // Everything else filters the current folder: name terms (fuzzy), size, date.
        self.filter = query.clone();
        let tab = &mut self.tabs[self.active_tab_index];
        tab.files.clear_filters();
        let view = tab.files.view.clone();
        glib::idle_add_local_once(move || {
            if let Some(model) = view
                .model()
                .and_then(|m| m.downcast::<gtk::MultiSelection>().ok())
            {
                model.unselect_all();
                // A lone match is selected so Enter in the search entry opens it.
                if model.n_items() == 1 {
                    model.select_item(0, true);
                }
            }
        });
        if !q.is_empty() {
            tab.files.add_filter(move |item| item.shown_by(&q));
        }
    }
}
