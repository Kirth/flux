use crate::model::FluxApp;
use gtk::prelude::*;

impl FluxApp {
    /// After returning from another app (e.g. one a file was opened in): if the files had
    /// focus when we left, put it back on the selected item. While flux is inactive focus
    /// can drift (niri narrows the window for the new app's column, the grid re-lays its
    /// tiles, focus ends up in the header); the user can't have moved it meanwhile.
    pub fn handle_window_active(&mut self, active: bool, files_focused: bool) {
        if !active {
            self.files_focused_on_leave = files_focused;
            return;
        }
        if std::mem::take(&mut self.files_focused_on_leave) {
            self.refocus_files(|focus, view| {
                focus.is_none_or(|f| {
                    f.ancestor(gtk::GridView::static_type()).as_ref() != Some(view.upcast_ref())
                })
            });
        }
    }

    /// Focus fell to nothing while it was on the files: the focused tile was recycled
    /// (re-layout after a resize, a refresh). Put it back on the selected item.
    pub fn handle_files_focus_lost(&mut self) {
        self.refocus_files(|focus, _| focus.is_none());
    }

    /// On idle (once GTK has settled), focuses the selected item if `needed` says so
    /// for the focus at that time.
    fn refocus_files(
        &self,
        needed: impl Fn(Option<&gtk::Widget>, &gtk::GridView) -> bool + 'static,
    ) {
        let Some(tab) = self.tabs.get(self.active_tab_index) else {
            return;
        };
        let view = tab.files.view.clone();
        let selected = self.get_selected_path();
        gtk::glib::idle_add_local_once(move || {
            let focus = view.root().and_then(|r| r.focus());
            if needed(focus.as_ref(), &view) {
                focus_item(&view, selected.as_deref());
            }
        });
    }
}

/// Focuses the row showing `path` (its list item widget, so the view's keyboard
/// position moves there too), or the view itself if that row isn't on screen.
fn focus_item(view: &gtk::GridView, path: Option<&std::path::Path>) {
    if let Some(path) = path {
        let mut child = view.first_child();
        while let Some(cell) = child {
            // SAFETY: "flux_path" is only ever set to a PathBuf, in `bind_widgets`.
            let shows_path = cell.first_child().is_some_and(|root| unsafe {
                root.data::<std::path::PathBuf>("flux_path")
                    .is_some_and(|p| p.as_ref() == path)
            });
            if shows_path && cell.grab_focus() {
                return;
            }
            child = cell.next_sibling();
        }
    }
    view.grab_focus();
}

/// Watches keyboard focus in the main window: reports when it becomes active or
/// inactive, and when focus falls to nothing right after being on the files (both handled
/// above). With `FLUX_DEBUG_FOCUS=1`, also logs every focus change.
pub fn install_focus_tracking(window: &gtk::Window, sender: relm4::AsyncComponentSender<FluxApp>) {
    use crate::model::AppMsg;
    use std::cell::Cell;

    let debug = std::env::var_os("FLUX_DEBUG_FOCUS").is_some();
    let in_files = |w: &gtk::Widget| w.ancestor(gtk::GridView::static_type()).is_some();

    let s = sender.clone();
    window.connect_is_active_notify(move |w| {
        // Checked right away: by the time the message is handled, focus may have moved.
        let files_focused = gtk::prelude::GtkWindowExt::focus(w).is_some_and(|f| in_files(&f));
        if debug {
            eprintln!("[focus] window active: {}", w.is_active());
        }
        s.input(AppMsg::WindowActive {
            active: w.is_active(),
            files_focused,
        });
    });

    let was_in_files = Cell::new(false);
    window.connect_focus_widget_notify(move |win| match gtk::prelude::GtkWindowExt::focus(win) {
        Some(w) => {
            if debug {
                eprintln!("[focus] {}", describe(&w));
            }
            was_in_files.set(in_files(&w));
        }
        None => {
            if debug {
                eprintln!("[focus] (none)");
            }
            if was_in_files.replace(false) {
                sender.input(AppMsg::FilesFocusLost);
            }
        }
    });
}

/// `GtkButton.flat  in GtkBox < GtkHeaderBar < …`, for the focus log.
fn describe(w: &gtk::Widget) -> String {
    let classes = w.css_classes().join(".");
    let path: Vec<String> = std::iter::successors(w.parent(), |p| p.parent())
        .take(4)
        .map(|p| p.type_().name().to_string())
        .collect();
    format!("{}.{classes}  in {}", w.type_().name(), path.join(" < "))
}
