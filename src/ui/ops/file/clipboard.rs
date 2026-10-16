use crate::model::{AppMsg, FluxApp};
use adw::prelude::*;
use relm4::prelude::*;
use std::collections::HashSet;
use std::path::PathBuf;

impl FluxApp {
    /// Handles clipboard Copy and Cut actions by populating standard GTK Clipboard providers.
    pub fn handle_copy_or_cut(&mut self, is_cut: bool, sender: &AsyncComponentSender<Self>) {
        self.handle_clipboard_action(is_cut);

        // Update the marks in place: removing/re-inserting the selected items would make
        // GTK drop the focused item and scroll the view back to the top.
        let selection: HashSet<PathBuf> = self.get_selection().into_iter().collect();
        let none = HashSet::new();
        let (cut, copy) = if is_cut {
            (&selection, &none)
        } else {
            (&none, &selection)
        };
        for tab in &self.tabs {
            for i in 0..tab.files.len() {
                if let Some(wrapper) = tab.files.get(i) {
                    let mut item = wrapper.borrow_mut();
                    let is_selected = selection.contains(&item.path);
                    item.is_cut = is_cut && is_selected;
                    item.is_copy = !is_cut && is_selected;
                }
            }
            crate::ui::components::restyle_clipboard_marks(&tab.files.view, cut, copy);
        }

        let cmd = if is_cut {
            "builtin::cut"
        } else {
            "builtin::copy"
        };
        if let Some(toast) = self
            .menu_actions
            .iter()
            .find(|a| a.command == cmd)
            .and_then(|a| a.toast.clone())
        {
            sender.input(AppMsg::ShowToast(toast));
        }
    }

    /// Cancels a pending cut: clears the cut marks and, if flux still owns the clipboard,
    /// empties it so a later paste cannot move the files. Copy marks are left alone.
    /// Returns whether there was a cut to cancel.
    pub fn handle_cancel_cut(&mut self) -> bool {
        let mut had_cut = false;
        for tab in &self.tabs {
            for i in 0..tab.files.len() {
                if let Some(wrapper) = tab.files.get(i) {
                    let mut item = wrapper.borrow_mut();
                    had_cut |= item.is_cut;
                    item.is_cut = false;
                }
            }
        }
        if !had_cut {
            return false;
        }

        let none = HashSet::new();
        for tab in &self.tabs {
            let copy: HashSet<PathBuf> = (0..tab.files.len())
                .filter_map(|i| tab.files.get(i))
                .filter(|w| w.borrow().is_copy)
                .map(|w| w.borrow().path.clone())
                .collect();
            crate::ui::components::restyle_clipboard_marks(&tab.files.view, &none, &copy);
        }

        if let Some(display) = gtk::gdk::Display::default() {
            let clipboard = display.clipboard();
            if clipboard.is_local() {
                let _ = clipboard.set_content(None::<&gtk::gdk::ContentProvider>);
            }
        }
        true
    }

    /// Opens the Properties window for `path` as a modal over the main window.
    pub fn open_properties_window(&self, path: std::path::PathBuf) {
        let mut properties = crate::ui::FileProperties::builder().launch(path).detach();
        // Keep the component alive with its window; dropping the controller would shut
        // it down and take its factory-built sections with it.
        properties.detach_runtime();
        let window = properties.widget();
        if let Some(parent) = self
            .header_widget
            .as_ref()
            .and_then(|w| w.root())
            .and_downcast::<gtk::Window>()
        {
            window.set_transient_for(Some(&parent));
            window.set_modal(true);
        }
        window.present();
    }

    /// Copies the absolute paths of selected items to the clipboard.
    /// Resolves symlinks to their canonical target path when available.
    pub fn handle_copy_path(&self, sender: &AsyncComponentSender<Self>) {
        let selection = self.get_selection();
        if selection.is_empty() {
            sender.input(AppMsg::ShowToast(crate::i18n::tr("No items selected.")));
            return;
        }

        let paths: Vec<String> = selection
            .iter()
            .map(|p| {
                let resolved = p.canonicalize().unwrap_or_else(|_| p.clone());
                let s = resolved.to_string_lossy();
                if s.contains(' ') {
                    format!("'{}'", s.replace('\'', "'\\'\\'"))
                } else {
                    s.into_owned()
                }
            })
            .collect();
        let text = paths.join(" ");

        if let Some(display) = gtk::gdk::Display::default() {
            display.clipboard().set_text(&text);
            sender.input(AppMsg::ShowToast(crate::i18n::tr(
                "Paths copied to clipboard.",
            )));
        }
    }
}
