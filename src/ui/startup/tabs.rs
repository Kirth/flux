use crate::model::{AppMsg, Config, FluxApp};
use adw::prelude::*;
use relm4::prelude::*;
use std::path::PathBuf;

impl FluxApp {
    /// Builds both panes' tab views and strips (the right one stays empty until split
    /// view is turned on) and the initial tab, wiring tab signals.
    pub(super) fn build_tabs(
        start_path: PathBuf,
        config: &Config,
        sender: &AsyncComponentSender<Self>,
    ) -> (
        adw::TabView,
        adw::TabBar,
        adw::TabView,
        adw::TabBar,
        crate::ui::TabState,
    ) {
        let new_bar = |view: &adw::TabView| {
            adw::TabBar::builder()
                .view(view)
                .autohide(true)
                .expand_tabs(false)
                .css_classes(["inline", "flux-tab-bar"])
                .build()
        };
        let tab_view = adw::TabView::new();
        let tab_bar = new_bar(&tab_view);
        let tab_view_right = adw::TabView::new();
        let tab_bar_right = new_bar(&tab_view_right);

        let mut initial_tab = crate::ui::TabState::new(
            1,
            start_path.clone(),
            config.default_list_mode,
            config.ui.default_sort,
        );
        initial_tab.is_initialized = true;

        let tab_id = initial_tab.id;

        initial_tab
            .files
            .view
            .set_single_click_activate(config.ui.single_click);

        let s_open = sender.clone();
        initial_tab.files.view.connect_activate(move |_, pos| {
            s_open.input(AppMsg::Open {
                position: Some(pos),
                tab_id: Some(tab_id),
            });
        });

        let s_sel = sender.clone();
        if let Some(selection_model) = initial_tab
            .files
            .view
            .model()
            .and_downcast::<gtk::MultiSelection>()
        {
            selection_model.connect_selection_changed(move |_, _, _| {
                s_sel.input(AppMsg::SelectionChanged {
                    tab_id: Some(tab_id),
                });
            });
        }

        Self::connect_tab_scroller(&initial_tab.scroller, sender);

        let initial_page = tab_view.append(&initial_tab.scroller);
        initial_page.set_title(&initial_tab.title);
        initial_page.set_icon(Some(&gtk::gio::ThemedIcon::new("folder-symbolic")));
        unsafe {
            initial_page.set_data("flux_tab_id", tab_id);
        }

        use crate::ui::tabs::{panes::wire_tab_view, Pane};
        wire_tab_view(&tab_view, &tab_bar, &tab_view_right, Pane::Left, sender);
        wire_tab_view(
            &tab_view_right,
            &tab_bar_right,
            &tab_view,
            Pane::Right,
            sender,
        );

        (
            tab_view,
            tab_bar,
            tab_view_right,
            tab_bar_right,
            initial_tab,
        )
    }
}
