//! Space: QuickLook-style preview through `flux-preview` (a separate, resident previewer).

use crate::i18n::tr;
use crate::model::{AppMsg, FluxApp};
use gtk::gio;
use gtk::prelude::*;
use relm4::prelude::*;
use std::ffi::OsString;

/// Most files handed over in one go; ←/→ in the preview step through these.
const MAX_FILES: usize = 2000;

impl FluxApp {
    /// Toggles the preview of the selected file. The other files shown (in display
    /// order, so a filter or search applies) go along, so ←/→ step through the folder.
    pub fn handle_quick_preview(&mut self, sender: &AsyncComponentSender<Self>) {
        let Some(tab) = self.tabs.get(self.active_tab_index) else {
            return;
        };
        let files = &tab.files;
        let selection = &files.selection_model;
        let n = selection.n_items();

        // Only real local files: archive entries, trash and network locations have no
        // path the previewer can read.
        let previewable = |p: &std::path::Path| {
            let s = p.to_string_lossy();
            !s.contains("://") && !crate::services::network::is_network_uri(p)
        };
        let mut paths = Vec::new();
        let mut index = None;
        let first_selected = (0..n).find(|&i| selection.is_selected(i));
        for i in 0..n {
            let Some(item) = files.get_visible(i) else {
                continue;
            };
            let path = item.borrow().path.clone();
            if !previewable(&path) {
                continue;
            }
            if Some(i) == first_selected {
                index = Some(paths.len());
            }
            paths.push(path);
        }
        let Some(index) = index else {
            if first_selected.is_some() {
                sender.input(AppMsg::ShowToast(tr("This item can't be previewed.")));
            }
            return;
        };

        // Keep the command line sane in huge folders: a window around the selection.
        let start = index.saturating_sub(MAX_FILES / 2);
        let end = (start + MAX_FILES).min(paths.len());
        let start = end.saturating_sub(MAX_FILES);

        let mut argv: Vec<OsString> = vec![previewer(), "--index".into()];
        argv.push((index - start).to_string().into());
        argv.extend(paths[start..end].iter().map(|p| p.as_os_str().to_owned()));
        let argv: Vec<&std::ffi::OsStr> = argv.iter().map(|a| a.as_os_str()).collect();

        // The client hands the request to the resident daemon and exits; GIO reaps it.
        if let Err(e) = gio::Subprocess::newv(&argv, gio::SubprocessFlags::NONE) {
            eprintln!("[preview] {e}");
            sender.input(AppMsg::ShowToast(tr(
                "Preview needs flux-preview (not found in PATH).",
            )));
        }
    }
}

/// The previewer to run: `flux-preview` from PATH. Non-release builds honor
/// `FLUX_PREVIEW_DEV` to test a dev build instead: `1` picks the release build in the
/// sibling `flux-preview` checkout, any other value is taken as the binary's path.
fn previewer() -> OsString {
    #[cfg(debug_assertions)]
    if let Some(dev) = std::env::var_os("FLUX_PREVIEW_DEV").filter(|v| !v.is_empty() && v != "0") {
        return if dev == "1" {
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../flux-preview/target/release/flux-preview"
            )
            .into()
        } else {
            dev
        };
    }
    "flux-preview".into()
}
