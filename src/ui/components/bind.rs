use super::file_item::FileItem;
use super::widgets::FileWidgets;
use crate::i18n::tr;
use crate::ui::constants;
use adw::gdk;
use adw::prelude::*;
use gtk::glib;
use gtk::glib::clone;
use relm4::prelude::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// Formats a byte count into a human-readable string (B / KB / MB / GB).
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1_024;
    const MB: u64 = 1_024 * KB;
    const GB: u64 = 1_024 * MB;
    match bytes {
        b if b >= GB => format!("{:.1} GB", b as f64 / GB as f64),
        b if b >= MB => format!("{:.1} MB", b as f64 / MB as f64),
        b if b >= KB => format!("{:.0} KB", b as f64 / KB as f64),
        b => format!("{b} B"),
    }
}

/// Applies the cut/copy clipboard styling to a bound item's root and card.
pub(crate) fn apply_clipboard_classes(
    root: &gtk::Widget,
    card_box: &gtk::Widget,
    is_cut: bool,
    is_copy: bool,
) {
    for (class, on) in [("flux-card--cut", is_cut), ("flux-card--copy", is_copy)] {
        for w in [root, card_box] {
            if on {
                w.add_css_class(class);
            } else {
                w.remove_css_class(class);
            }
        }
    }
}

/// Restyles the on-screen items of `view` after the cut/copy marks changed.
///
/// Mutating the model instead (remove + insert) would make GTK drop the focused item
/// and scroll back to the top. Rows bound later pick the marks up from their `FileItem`.
pub(crate) fn restyle_clipboard_marks(
    view: &gtk::GridView,
    cut: &std::collections::HashSet<PathBuf>,
    copy: &std::collections::HashSet<PathBuf>,
) {
    let mut next = view.first_child();
    while let Some(list_item) = next {
        next = list_item.next_sibling();
        let Some(root) = list_item.first_child().and_downcast::<gtk::Overlay>() else {
            continue;
        };
        // SAFETY: "flux_path" is only ever set to a PathBuf, in `bind_widgets`.
        let Some(path) = (unsafe {
            root.data::<PathBuf>("flux_path")
                .map(|p| p.as_ref().clone())
        }) else {
            continue;
        };
        if let Some(card_box) = root.child() {
            apply_clipboard_classes(
                root.upcast_ref(),
                &card_box,
                cut.contains(&path),
                copy.contains(&path),
            );
        }
    }
}

impl FileItem {
    /// Fills the list-mode detail cells and applies the shared column layout.
    fn bind_list_columns(&self, widgets: &FileWidgets) {
        use crate::ui::list_columns::{self as cols, ListColumn, MISSING};

        for (col, cell) in ListColumn::ALL.iter().zip(&widgets.column_labels) {
            let mut tooltip = None;
            let text = match col {
                ListColumn::Size if self.is_dir => {
                    if self.size == 1 {
                        tr("1 item")
                    } else {
                        format!("{} {}", self.size, tr("items"))
                    }
                }
                ListColumn::Size => format_size(self.size),
                ListColumn::Modified => {
                    tooltip = cols::format_exact_date(self.mtime);
                    cols::format_list_date(self.mtime).unwrap_or_else(|| MISSING.into())
                }
                ListColumn::Created => {
                    tooltip = cols::format_exact_date(self.created);
                    cols::format_list_date(self.created).unwrap_or_else(|| MISSING.into())
                }
                ListColumn::Type if self.is_symlink => {
                    // Describe what the link points at, not the link's own name.
                    let target = std::fs::canonicalize(&self.path).ok();
                    let target_name = target
                        .as_ref()
                        .and_then(|t| t.file_name())
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| self.name.clone());
                    let target_is_dir = target.as_ref().is_some_and(|t| t.is_dir());
                    cols::type_description(&target_name, target_is_dir)
                }
                ListColumn::Type => cols::type_description(&self.name, self.is_dir),
                ListColumn::Owner => self.uid.map_or_else(|| MISSING.into(), cols::owner_name),
                ListColumn::Permissions => {
                    cols::format_permissions(self.mode).unwrap_or_else(|| MISSING.into())
                }
            };
            cell.set_label(&text);
            cell.set_tooltip_text(tooltip.as_deref());
        }
        cols::apply_to_row(&widgets.columns_box);
        widgets.columns_box.set_visible(true);
    }

    /// Updates the item's widgets with current data from the [FileItem] model.
    ///
    /// Synchronizes labels, icons, thumbnails, and visibility states (e.g., rename entry).
    pub(super) fn bind_widgets(&mut self, widgets: &mut FileWidgets, root: &mut gtk::Overlay) {
        crate::hit!("bind");
        widgets.label.set_label(&self.display_label);

        apply_clipboard_classes(
            root.upcast_ref(),
            widgets.card_box.upcast_ref(),
            self.is_cut,
            self.is_copy,
        );

        if self.is_list_mode {
            crate::hit!("bind_layout_list");
            // Compact horizontal row: small icon on the left, filename fills the rest.
            widgets
                .card_box
                .set_orientation(gtk::Orientation::Horizontal);
            widgets.card_box.set_halign(gtk::Align::Fill);
            widgets.card_box.set_valign(gtk::Align::Center);
            widgets.card_box.set_hexpand(true);
            widgets.card_box.set_vexpand(false);
            widgets
                .card_box
                .set_spacing(crate::ui::list_columns::LIST_CELL_SPACING);

            widgets.icon_widget.set_pixel_size(self.icon_size);
            widgets
                .icon_widget
                .set_size_request(self.icon_size, self.icon_size);
            widgets.icon_widget.set_valign(gtk::Align::Center);
            widgets.icon_widget.set_halign(gtk::Align::Start);

            widgets
                .video_widget
                .set_size_request(self.icon_size, self.icon_size);

            widgets
                .preview_stack
                .set_size_request(self.icon_size, self.icon_size);
            widgets.preview_stack.set_valign(gtk::Align::Center);
            widgets.preview_stack.set_halign(gtk::Align::Start);
            widgets.icon_overlay.set_valign(gtk::Align::Center);
            widgets.icon_overlay.set_halign(gtk::Align::Start);
            widgets.preview_stack.set_visible_child_name("icon");
            widgets.label.set_halign(gtk::Align::Start);
            widgets.label.set_hexpand(true);
            widgets.label.set_justify(gtk::Justification::Left);
            widgets.label.set_width_chars(-1);
            widgets.label.set_max_width_chars(-1);
            // Long names ellipsize in the middle so the detail columns stay aligned.
            widgets.label.set_wrap(false);
            widgets
                .label
                .set_ellipsize(gtk::pango::EllipsizeMode::Middle);

            if let Some(ref old_provider) = widgets.scale_css_provider {
                widgets.label.style_context().remove_provider(old_provider);
                widgets
                    .info_label
                    .style_context()
                    .remove_provider(old_provider);
                widgets.scale_css_provider = None;
            }

            if self.scale_font_with_icons && self.icon_size > 0 {
                const DEFAULT_LIST_ICON_BASELINE: f64 = 64.0;

                let ratio = self.icon_size as f64 / DEFAULT_LIST_ICON_BASELINE;

                // Allow a broader clamp so size differences between 16px and 48px+ are visible
                let scale_factor = ratio.clamp(0.8, 2.0);

                let label_attrs = gtk::pango::AttrList::new();
                label_attrs.insert(gtk::pango::AttrFloat::new_scale(scale_factor));
                widgets.label.set_attributes(Some(&label_attrs));

                let info_attrs = gtk::pango::AttrList::new();
                info_attrs.insert(gtk::pango::AttrFloat::new_scale(scale_factor));
                widgets.info_label.set_attributes(Some(&info_attrs));
            } else {
                widgets.label.set_attributes(None);
                widgets.info_label.set_attributes(None);
            }

            // The name fills whatever the detail columns leave; it never scrolls.
            widgets
                .label_scroller
                .set_hscrollbar_policy(gtk::PolicyType::Never);
            widgets.label_scroller.set_propagate_natural_width(false);
            widgets.label_scroller.set_propagate_natural_height(true);
            widgets.label_scroller.set_valign(gtk::Align::Center);
            widgets.label_scroller.set_hexpand(true);

            // Stack and its label-box fill the scroller's viewport.
            widgets.stack.set_halign(gtk::Align::Fill);
            widgets.stack.set_valign(gtk::Align::Center);
            widgets.stack.set_hexpand(true);

            // Get the label container (the Box inside the Stack) and make it start-aligned.
            if let Some(label_box) = widgets.stack.child_by_name(constants::VIEW_LABEL) {
                if let Some(box_widget) = label_box.downcast_ref::<gtk::Box>() {
                    box_widget.set_halign(gtk::Align::Start);
                    box_widget.set_valign(gtk::Align::Center);
                    box_widget.set_hexpand(true);
                    box_widget.set_size_request(-1, -1);
                }
            }

            // Invalidate cached measurements across the scroller boundary to force size renegotiation
            widgets.label.queue_resize();
            widgets.info_label.queue_resize();
            widgets.label_scroller.queue_resize();
            root.queue_resize();

            crate::hit!("bind_info_label");
            if let Some(ref snippet) = self.search_snippet {
                // Content-search hits show their matching line instead of detail columns.
                widgets.columns_box.set_visible(false);
                widgets.info_label.set_halign(gtk::Align::End);
                widgets.info_label.set_xalign(1.0);
                widgets.info_label.set_hexpand(false);
                widgets.info_label.set_tooltip_text(None::<&str>);
                if self.line_number > 0 {
                    widgets
                        .info_label
                        .set_label(&format!("L:{}  {}", self.line_number, snippet));
                } else {
                    widgets.info_label.set_label(snippet);
                }
                widgets.info_label.set_visible(true);
            } else {
                widgets.info_label.set_visible(false);
                widgets.columns_box.set_visible(false);
                self.bind_list_columns(widgets);
            }
        } else {
            crate::hit!("bind_layout_grid");
            widgets.card_box.set_orientation(gtk::Orientation::Vertical);
            widgets.card_box.set_halign(gtk::Align::Fill);
            widgets.card_box.set_valign(gtk::Align::Fill);
            widgets.card_box.set_hexpand(false);
            widgets.card_box.set_vexpand(false);
            widgets.card_box.set_spacing(0);
            widgets.icon_widget.set_pixel_size(self.icon_size);
            widgets.icon_widget.set_size_request(-1, -1);
            widgets.icon_widget.set_valign(gtk::Align::End);
            widgets.icon_widget.set_halign(gtk::Align::Center);
            widgets
                .video_widget
                .set_size_request(self.icon_size, self.icon_size);
            widgets.preview_stack.set_size_request(-1, -1);
            widgets.preview_stack.set_valign(gtk::Align::End);
            widgets.preview_stack.set_halign(gtk::Align::Center);
            widgets.icon_overlay.set_valign(gtk::Align::End);
            widgets.icon_overlay.set_halign(gtk::Align::Center);
            widgets.preview_stack.set_visible_child_name("icon");
            widgets.label.set_halign(gtk::Align::Center);
            widgets.label.set_hexpand(false);
            widgets.label.set_justify(gtk::Justification::Center);
            widgets.label.set_max_width_chars(self.max_width_chars);
            widgets.label.set_width_chars(self.grid_spacing);

            if self.scale_font_with_icons && self.icon_size > 0 {
                let base_size = if self.default_icon_size > 0 {
                    self.default_icon_size as f64
                } else {
                    96.0f64
                };
                let scale_factor = (self.icon_size as f64 / base_size).clamp(0.8, 1.3);
                let attrs = gtk::pango::AttrList::new();
                attrs.insert(gtk::pango::AttrFloat::new_scale(scale_factor));
                widgets.label.set_attributes(Some(&attrs));
            } else {
                widgets.label.set_attributes(None);
            }

            if self.expand_labels {
                widgets.label.set_wrap(true);
                widgets.label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                widgets.label.set_ellipsize(gtk::pango::EllipsizeMode::None);
            } else {
                widgets.label.set_wrap(false);
                widgets.label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            }

            // Restore scroller to grid-mode defaults: no scrollbar, propagate natural size.
            widgets
                .label_scroller
                .set_hscrollbar_policy(gtk::PolicyType::Never);
            widgets.label_scroller.set_propagate_natural_width(true);
            widgets.label_scroller.set_valign(gtk::Align::Start);
            widgets.label_scroller.set_hexpand(false);

            // Reset Stack alignment for grid mode (centered horizontally, anchored to start vertically).
            widgets.stack.set_halign(gtk::Align::Center);
            widgets.stack.set_valign(gtk::Align::Start);
            widgets.stack.set_hexpand(false);
            if let Some(label_box) = widgets.stack.child_by_name(constants::VIEW_LABEL) {
                if let Some(box_widget) = label_box.downcast_ref::<gtk::Box>() {
                    box_widget.set_halign(gtk::Align::Center);
                    box_widget.set_valign(gtk::Align::Start);
                    box_widget.set_hexpand(false);
                    box_widget.set_size_request(-1, -1);
                }
            }

            // Always hide the info label in grid mode.
            widgets.info_label.set_visible(false);
        }

        // WARNING: never use `set_widget_name(&self.path.to_string_lossy())` here.
        // `bind()` runs on every visible item on each scroll/zoom/layout pass.
        // Setting the widget name dirties the CSS node and forces GTK's style
        // engine to re-run a full CSS selector match for the entire grid on
        // every bind, which caused severe scrollbar lag in GridView mode with
        // thousands of items. Opaque widget data via `set_data` bypasses the
        // CSS engine entirely and is the correct, high-performance approach.
        crate::hit!("bind_flux_path_data");
        unsafe {
            root.set_data("flux_path", self.path.clone());
            widgets.card_box.set_data("flux_path", self.path.clone());
        }

        crate::hit!("bind_emblem_classes");
        if self.is_foreign_owner {
            root.add_css_class("flux-card--restricted");
            widgets.card_box.add_css_class("flux-card--restricted");
            widgets.lock_icon.set_visible(true);
        } else {
            root.remove_css_class("flux-card--restricted");
            widgets.card_box.remove_css_class("flux-card--restricted");
            widgets.lock_icon.set_visible(false);
        }

        let show_indicator = self.show_symlink_emblem && self.is_symlink;

        if show_indicator {
            if self.is_broken_symlink {
                root.add_css_class("flux-card--broken-symlink");
                root.remove_css_class("flux-card--symlink");
                widgets.card_box.add_css_class("flux-card--broken-symlink");
                widgets.card_box.remove_css_class("flux-card--symlink");
            } else {
                root.add_css_class("flux-card--symlink");
                root.remove_css_class("flux-card--broken-symlink");
                widgets.card_box.add_css_class("flux-card--symlink");
                widgets
                    .card_box
                    .remove_css_class("flux-card--broken-symlink");
            }
        } else {
            root.remove_css_class("flux-card--symlink");
            root.remove_css_class("flux-card--broken-symlink");
            widgets.card_box.remove_css_class("flux-card--symlink");
            widgets
                .card_box
                .remove_css_class("flux-card--broken-symlink");
        }

        if self.is_dir && self.show_empty_dir_emblem {
            if self.is_empty {
                root.add_css_class("flux-card--empty");
                widgets.card_box.add_css_class("flux-card--empty");
            } else {
                root.remove_css_class("flux-card--empty");
                widgets.card_box.remove_css_class("flux-card--empty");
            }
        } else {
            root.remove_css_class("flux-card--empty");
            widgets.card_box.remove_css_class("flux-card--empty");
        }

        let emblem = if show_indicator && self.is_broken_symlink {
            Some(("!", "flux-file-emblem--broken"))
        } else if show_indicator {
            Some(("↗", "flux-file-emblem--link"))
        } else if self.is_dir && self.show_empty_dir_emblem && self.is_empty {
            Some(("∅", "flux-file-emblem--empty"))
        } else {
            None
        };
        if let Some((glyph, class_name)) = emblem {
            widgets.file_emblem.set_label(glyph);
            widgets
                .file_emblem
                .set_css_classes(&["flux-file-emblem", class_name]);
            widgets.file_emblem.set_visible(true);
        } else {
            widgets.file_emblem.set_visible(false);
        }

        crate::hit!("bind_git_badge");
        if let Some((emblem, class_name)) = self.git_status.badge_info() {
            widgets.git_badge.set_label(emblem);
            widgets
                .git_badge
                .set_css_classes(&["flux-git-badge", class_name]);
            widgets.git_badge.set_visible(true);
        } else {
            widgets.git_badge.set_visible(false);
        }

        if self.is_editing {
            crate::hit!("bind_rename_entry");
            let entry = match widgets.stack.child_by_name(constants::VIEW_ENTRY) {
                Some(w) => w.downcast::<gtk::Entry>().unwrap(),
                None => {
                    let entry = gtk::Entry::builder()
                        .halign(gtk::Align::Center)
                        .css_classes([constants::RENAME_ENTRY_CLASS])
                        .build();

                    // Store the path directly as widget data on the entry itself
                    unsafe {
                        entry.set_data("target_rename_path", self.path.clone());
                    }

                    let submitted = std::rc::Rc::new(std::cell::Cell::new(false));

                    let focus_ctrl = gtk::EventControllerFocus::new();
                    let submitted_focus = submitted.clone();
                    focus_ctrl.connect_leave(move |_| {
                        if !submitted_focus.get() {
                            if let Some(s) = crate::model::SENDER.get() {
                                s.send(crate::model::AppMsg::Refresh).ok();
                            }
                        }
                    });
                    entry.add_controller(focus_ctrl);

                    let key_ctrl = gtk::EventControllerKey::new();
                    key_ctrl.set_propagation_phase(gtk::PropagationPhase::Capture);

                    let submitted_key = submitted.clone();
                    key_ctrl.connect_key_pressed(clone!(
                        #[weak]
                        entry,
                        #[upgrade_or]
                        glib::Propagation::Proceed,
                        move |_, keyval, _, _| {
                            if keyval == gdk::Key::Escape {
                                submitted_key.set(true);
                                if let Some(s) = crate::model::SENDER.get() {
                                    s.send(crate::model::AppMsg::Refresh).ok();
                                    return glib::Propagation::Stop;
                                }
                            } else if keyval == gdk::Key::Return || keyval == gdk::Key::KP_Enter {
                                submitted_key.set(true);
                                let target_path = unsafe {
                                    entry
                                        .data::<PathBuf>("target_rename_path")
                                        .map(|p| p.as_ref().clone())
                                };

                                if let Some(old_path) = target_path {
                                    let new_name = entry.text().to_string();
                                    if let Some(s) = crate::model::SENDER.get() {
                                        s.send(crate::model::AppMsg::PerformRename(
                                            old_path, new_name,
                                        ))
                                        .ok();
                                        return glib::Propagation::Stop;
                                    }
                                }
                            }
                            glib::Propagation::Proceed
                        }
                    ));
                    entry.add_controller(key_ctrl);

                    widgets.stack.add_named(&entry, Some(constants::VIEW_ENTRY));
                    entry
                }
            };

            // Update the data on existing entries if reused by the factory
            unsafe {
                entry.set_data("target_rename_path", self.path.clone());
            }

            widgets.stack.set_visible_child_name(constants::VIEW_ENTRY);
            entry.set_text(&self.name);
            let dot_pos = self
                .name
                .rfind('.')
                .map(|byte_idx| self.name[..byte_idx].chars().count())
                .unwrap_or_else(|| self.name.chars().count());
            entry.select_region(0, dot_pos as i32);

            gtk::glib::idle_add_local_once(clone!(
                #[weak]
                entry,
                move || {
                    entry.grab_focus();
                }
            ));
        } else {
            crate::hit!("bind_rename_teardown");
            // Reclaim memory if a temporary Entry was previously instantiated
            if let Some(existing_entry) = widgets.stack.child_by_name(constants::VIEW_ENTRY) {
                widgets.stack.remove(&existing_entry);
            }
            // Ensure the label is visible when not editing.
            widgets.stack.set_visible_child_name(constants::VIEW_LABEL);
        }

        if let Some(ref texture) = self.thumbnail {
            crate::hit!("bind_thumbnail_paint");
            widgets.icon_widget.set_paintable(Some(texture));
        } else {
            crate::hit!("bind_icon_paint");
            widgets.icon_widget.set_pixel_size(self.icon_size);

            // Custom file-path icons must be loaded as textures and scaled ourselves,
            // because GtkIconTheme::lookup_by_gicon returns a paintable at the file's
            // natural size for GFileIcon, which the Image widget then paints verbatim
            // (stretching the grid cell's width).
            // NOTE: This is necessary, otherwise the grid layout will be inconsistent and the icon will
            // not match the requested size.
            let custom_path = self
                .icon
                .downcast_ref::<gtk::gio::FileIcon>()
                .and_then(|fi| fi.file().path());

            if let Some(path) = custom_path {
                if let Ok(texture) = gdk::Texture::from_filename(&path) {
                    widgets.icon_widget.set_paintable(Some(&texture));
                } else {
                    // fall through to theme lookup
                    let display = root.display();
                    let icon_theme = gtk::IconTheme::for_display(&display);
                    let scale = root.scale_factor().max(1);
                    let paintable = icon_theme.lookup_by_gicon(
                        &self.icon,
                        self.icon_size,
                        scale,
                        gtk::TextDirection::None,
                        gtk::IconLookupFlags::empty(),
                    );
                    widgets.icon_widget.set_paintable(Some(&paintable));
                }
            } else {
                let display = root.display();
                let icon_theme = gtk::IconTheme::for_display(&display);
                let scale = root.scale_factor().max(1);
                let paintable = icon_theme.lookup_by_gicon(
                    &self.icon,
                    self.icon_size,
                    scale,
                    gtk::TextDirection::None,
                    gtk::IconLookupFlags::empty(),
                );
                widgets.icon_widget.set_paintable(Some(&paintable));
            }
        }

        if self.disable_drag_and_drop {
            crate::hit!("bind_dnd_disabled");
            widgets
                .drag_source
                .set_content(None::<&gdk::ContentProvider>);
            widgets.drop_target.set_actions(gdk::DragAction::empty());
        } else {
            crate::hit!("bind_dnd_payload");
            let file = gtk::gio::File::for_path(&self.path);
            let uri = format!("{}\r\n", file.uri());
            let file_list_provider = gdk::ContentProvider::for_bytes(
                "text/uri-list",
                &glib::Bytes::from(uri.as_bytes()),
            );
            let file_provider = gdk::ContentProvider::for_value(&file.to_value());
            let content = gdk::ContentProvider::new_union(&[file_list_provider, file_provider]);
            widgets.drag_source.set_content(Some(&content));

            widgets.drop_target.set_actions(if self.is_dir {
                gdk::DragAction::COPY | gdk::DragAction::MOVE
            } else {
                gdk::DragAction::empty()
            });
        }

        crate::hit!("bind_path_cells");
        // Update the shared cell with the current path so gestures can read it
        *self.active_path.borrow_mut() = Some(self.path.clone());

        // Store a clone of the Rc in the widget data so gestures can access the cell
        unsafe {
            root.set_data("active_path_cell", Rc::downgrade(&self.active_path));
            root.set_data("grid_item_index", self.grid_idx);
        }
    }

    /// Clears the per-cell lazy-thumbnail guard so the next item bound to this
    /// recycled widget cell can request its own thumbnail without being suppressed.
    pub(super) fn unbind_widgets(&mut self, widgets: &mut FileWidgets, root: &mut gtk::Overlay) {
        crate::hit!("unbind");
        // A video preview plays as the icon's paintable; stop it before the cell is reused,
        // and forget the thumbnail it would restore (it belongs to the previous item).
        if let Some(stream) = widgets
            .icon_widget
            .paintable()
            .and_downcast::<gtk::MediaStream>()
        {
            stream.pause();
        }
        // SAFETY: "preview_restore" is only ever set to Option<gdk::Paintable>.
        unsafe {
            let _ = root.steal_data::<Option<gdk::Paintable>>("preview_restore");
        }

        if let Some(ref provider) = widgets.scale_css_provider {
            widgets.label.style_context().remove_provider(provider);
            widgets.info_label.style_context().remove_provider(provider);
            widgets.scale_css_provider = None;
        }

        root.remove_css_class("flux-card--cut");
        root.remove_css_class("flux-card--copy");
        root.remove_css_class("flux-card--restricted");
        widgets.card_box.remove_css_class("flux-card--cut");
        widgets.card_box.remove_css_class("flux-card--copy");
        widgets.card_box.remove_css_class("flux-card--restricted");

        widgets.preview_stack.set_visible_child_name("icon");

        widgets.git_badge.set_visible(false);
        widgets.git_badge.set_css_classes(&["flux-git-badge"]);
        widgets.file_emblem.set_visible(false);

        // Release the texture on both the widget and the item model
        widgets.icon_widget.set_paintable(None::<&gdk::Paintable>);
        widgets.icon_widget.clear();

        widgets
            .drag_source
            .set_content(None::<&gdk::ContentProvider>);
        widgets.drag_source.set_icon(None::<&gdk::Paintable>, 0, 0);
        widgets.label.set_text("");
        widgets.label.set_attributes(None);
        widgets.info_label.set_text("");
        widgets.info_label.set_attributes(None);
        widgets.info_label.set_tooltip_text(None::<&str>);
        widgets.info_label.set_visible(false);

        root.remove_css_class("flux-card--symlink");
        root.remove_css_class("flux-card--broken-symlink");
        root.remove_css_class("flux-card--empty");
        widgets.card_box.remove_css_class("flux-card--symlink");
        widgets
            .card_box
            .remove_css_class("flux-card--broken-symlink");
        widgets.card_box.remove_css_class("flux-card--empty");

        if let Some(existing_entry) = widgets.stack.child_by_name(constants::VIEW_ENTRY) {
            widgets.stack.remove(&existing_entry);
        }
        *self.active_path.borrow_mut() = None;
        unsafe {
            let _ = root.steal_data::<std::rc::Weak<RefCell<Option<PathBuf>>>>("active_path_cell");
            let _ = root.steal_data::<u32>("lazy_thumb_requested");
            let _ = root.steal_data::<u32>("grid_item_index");
            let _ = root.steal_data::<PathBuf>("flux_path");
            let _ = widgets.card_box.steal_data::<PathBuf>("flux_path");
        }
    }
}
