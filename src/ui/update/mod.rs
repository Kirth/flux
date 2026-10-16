use crate::model::{AppMsg, FluxApp};
use gtk::prelude::*;
use relm4::prelude::*;

mod context_menu;
mod dialogs;
mod file_ops;
mod git;
mod loading;
mod navigation;
mod network;
mod search;
mod sidebar;
mod thumbnails;
mod view;
mod window;

impl FluxApp {
    pub fn handle_update(&mut self, message: AppMsg, sender: AsyncComponentSender<Self>) {
        self.route_update(message, sender);
        self.sync_list_view_chrome();
    }

    /// Keeps each pane's list-view header and compact-row class in step with the tab
    /// that pane is showing (the active tab's live state is mirrored on `self`).
    fn sync_list_view_chrome(&self) {
        use crate::ui::tabs::{panes::page_tab_id, Pane};
        for (i, pane) in [Pane::Left, Pane::Right].into_iter().enumerate() {
            let shown = self
                .tab_view_for(pane)
                .selected_page()
                .and_then(|p| self.tab_index_by_id(page_tab_id(&p)));
            let Some(index) = shown else {
                crate::ui::list_columns::sync_header(i, false, self.sort_by, true);
                continue;
            };
            let tab = &self.tabs[index];
            let (list_mode, sort_by, ascending) = if index == self.active_tab_index {
                (self.is_list_mode, self.sort_by, self.sort_ascending)
            } else {
                (tab.is_list_mode, tab.sort_by, tab.sort_ascending)
            };
            let view = &tab.files;
            if view.view.has_css_class("flux-list-mode") != list_mode {
                if list_mode {
                    view.view.add_css_class("flux-list-mode");
                } else {
                    view.view.remove_css_class("flux-list-mode");
                }
            }
            // Content-search hits show snippets instead of columns, so drop the header.
            let showing_snippets = view
                .get(0)
                .is_some_and(|item| item.borrow().search_snippet.is_some());
            let pane_shown = pane == Pane::Left || self.split_view;
            crate::ui::list_columns::sync_header(
                i,
                pane_shown && list_mode && !showing_snippets,
                sort_by,
                ascending,
            );
        }
    }

    fn route_update(&mut self, mut message: AppMsg, sender: AsyncComponentSender<Self>) {
        // Message trail for crash/hang reports, slow-message log (utils/flight.rs).
        let _flight = crate::utils::flight::message(&message);
        crate::hit!("handle_update");

        macro_rules! route {
            ($($m:ident),+ $(,)?) => {
                $(
                    match $m::handle(self, message, &sender) {
                        Ok(()) => return,
                        Err(rest) => message = rest,
                    }
                )+
            };
        }

        route!(
            navigation,
            loading,
            view,
            thumbnails,
            search,
            file_ops,
            dialogs,
            context_menu,
            network,
            git,
            window,
            sidebar,
        );

        eprintln!("[flux] unhandled AppMsg: {message:?}");
    }
}
