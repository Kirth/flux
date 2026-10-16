use super::state::TabState;
use crate::model::{AppMsg, FluxApp};
use gtk::prelude::*;
use relm4::prelude::*;

impl FluxApp {
    /// Ctrl+Tab: next tab within the active pane's strip (selecting it activates it).
    pub fn handle_next_tab(&mut self) {
        self.step_tab(1);
    }

    pub fn handle_prev_tab(&mut self) {
        self.step_tab(-1);
    }

    fn step_tab(&self, delta: i32) {
        let view = self.tab_view_for(self.active_pane());
        let n = view.n_pages();
        if n < 2 {
            return;
        }
        let pos = view.selected_page().map_or(0, |p| view.page_position(&p));
        view.set_selected_page(&view.nth_page((pos + delta).rem_euclid(n)));
    }

    pub(super) fn wire_tab_signals(&self, tab: &mut TabState, sender: &AsyncComponentSender<Self>) {
        tab.files
            .view
            .set_single_click_activate(self.config.ui.single_click);

        let tab_id = tab.id;

        let s_open = sender.clone();
        tab.files.view.connect_activate(move |_, pos| {
            s_open.input(AppMsg::Open {
                position: Some(pos),
                tab_id: Some(tab_id),
            });
        });

        let s_sel = sender.clone();
        if let Some(selection_model) = tab.files.view.model().and_downcast::<gtk::MultiSelection>()
        {
            selection_model.connect_selection_changed(move |_, _, _| {
                s_sel.input(AppMsg::SelectionChanged {
                    tab_id: Some(tab_id),
                });
            });
        }

        Self::connect_tab_scroller(&tab.scroller, sender);
    }
}
