//! Commander-style split view: two panes side by side, each with its own tab strip.
//!
//! Tabs live in `FluxApp::tabs` regardless of pane; each `TabState` records its pane,
//! and pages are found by their `"flux_tab_id"` data rather than by position (the two
//! `adw::TabView`s have independent orders). The active tab's live state is mirrored
//! in `FluxApp` as before; the active pane is simply the active tab's pane.

use crate::model::{AppMsg, FluxApp};
use adw::prelude::*;
use gtk::{gdk, glib};
use relm4::prelude::*;
use std::cell::Cell;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pane {
    #[default]
    Left,
    Right,
}

impl Pane {
    pub fn other(self) -> Pane {
        match self {
            Pane::Left => Pane::Right,
            Pane::Right => Pane::Left,
        }
    }

    fn index(self) -> usize {
        match self {
            Pane::Left => 0,
            Pane::Right => 1,
        }
    }
}

thread_local! {
    /// Mirrors `FluxApp::split_view` for key handlers that have no model access.
    static SPLIT_ACTIVE: Cell<bool> = const { Cell::new(false) };
}

/// Whether split view is on (for input handlers deciding whether Tab switches panes).
pub fn split_active() -> bool {
    SPLIT_ACTIVE.with(Cell::get)
}

/// The flux tab id stored on a tab page.
pub fn page_tab_id(page: &adw::TabPage) -> u64 {
    // SAFETY: "flux_tab_id" is only ever set to a u64, when pages are created.
    unsafe {
        page.data::<u64>("flux_tab_id")
            .map(|r| *r.as_ref())
            .unwrap_or(0)
    }
}

/// Finds the page for tab `id` in either pane's view.
pub fn find_tab_page(left: &adw::TabView, right: &adw::TabView, id: u64) -> Option<adw::TabPage> {
    [left, right].into_iter().find_map(|view| {
        (0..view.n_pages())
            .map(|i| view.nth_page(i))
            .find(|page| page_tab_id(page) == id)
    })
}

/// Wires one pane's tab view and tab bar to the app: selecting, closing, moving tabs
/// between panes, and dropping files onto a tab.
pub(crate) fn wire_tab_view(
    view: &adw::TabView,
    bar: &adw::TabBar,
    other: &adw::TabView,
    pane: Pane,
    sender: &AsyncComponentSender<FluxApp>,
) {
    {
        let sender = sender.clone();
        view.connect_selected_page_notify(move |tv| {
            if let Some(page) = tv.selected_page() {
                sender.input(AppMsg::ActivateTab(page_tab_id(&page)));
            }
        });
    }
    {
        let sender = sender.clone();
        let other = other.downgrade();
        view.connect_close_page(move |tv, page| {
            let total = tv.n_pages() + other.upgrade().map_or(0, |o| o.n_pages());
            if total <= 1 {
                if let Some(win) = relm4::main_application().active_window() {
                    win.close();
                }
                return glib::Propagation::Stop;
            }
            let id = page_tab_id(page);
            tv.close_page_finish(page, true);
            sender.input(AppMsg::CloseTabId(id));
            glib::Propagation::Stop
        });
    }
    {
        // Tabs dragged in from the other pane's strip.
        let sender = sender.clone();
        view.connect_page_attached(move |_, page, _| {
            // New pages are attached before their id is set; only transfers matter here.
            let id = page_tab_id(page);
            if id != 0 {
                sender.input(AppMsg::TabAttached { id, pane });
            }
        });
    }

    // Files dragged from the grid onto a tab move into that tab's folder.
    bar.setup_extra_drop_target(
        gdk::DragAction::MOVE | gdk::DragAction::COPY,
        &[gdk::FileList::static_type()],
    );
    {
        let sender = sender.clone();
        bar.connect_extra_drag_drop(move |_, page, value| {
            let Ok(files) = value.get::<gdk::FileList>() else {
                return false;
            };
            let paths: Vec<PathBuf> = files.files().iter().filter_map(|f| f.path()).collect();
            if paths.is_empty() {
                return false;
            }
            sender.input(AppMsg::DropOnTab {
                id: page_tab_id(page),
                paths,
            });
            true
        });
    }
}

impl FluxApp {
    pub(crate) fn tab_view_for(&self, pane: Pane) -> &adw::TabView {
        match pane {
            Pane::Left => &self.tab_view,
            Pane::Right => &self.tab_view_right,
        }
    }

    pub(crate) fn tab_index_by_id(&self, id: u64) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    /// The page showing `self.tabs[index]`, in whichever pane it lives.
    pub(crate) fn tab_page(&self, index: usize) -> Option<adw::TabPage> {
        let id = self.tabs.get(index)?.id;
        find_tab_page(&self.tab_view, &self.tab_view_right, id)
    }

    pub(crate) fn active_pane(&self) -> Pane {
        self.tabs
            .get(self.active_tab_index)
            .map_or(Pane::Left, |t| t.pane)
    }

    /// Selects `self.tabs[index]` in its pane's view, if it isn't already.
    pub(crate) fn select_tab_page(&self, index: usize) {
        let Some(tab) = self.tabs.get(index) else {
            return;
        };
        let view = self.tab_view_for(tab.pane);
        if let Some(page) = find_tab_page(&self.tab_view, &self.tab_view_right, tab.id) {
            if view.selected_page().as_ref() != Some(&page) {
                view.set_selected_page(&page);
            }
        }
    }

    /// Activates tab `id` if its page is (still) the selected one in its strip.
    ///
    /// Selection-change notifications are queued, so by the time one is handled the
    /// strip may have moved on (e.g. several tabs transferred at once). Acting on a
    /// stale one would re-select an old page, whose notification would undo the next
    /// one, and so on forever.
    pub fn handle_activate_tab(&mut self, id: u64, sender: &AsyncComponentSender<Self>) {
        let Some(index) = self.tab_index_by_id(id) else {
            return;
        };
        let view = self.tab_view_for(self.tabs[index].pane);
        let is_selected = view
            .selected_page()
            .is_some_and(|page| page_tab_id(&page) == id);
        if !is_selected {
            return;
        }
        self.handle_switch_tab(index, sender);
        self.update_panes();
    }

    /// Clicking into the other pane (or pressing Tab) makes its selected tab active.
    pub fn handle_focus_pane(&mut self, pane: Pane, sender: &AsyncComponentSender<Self>) {
        if !self.split_view || pane == self.active_pane() {
            return;
        }
        if let Some(page) = self.tab_view_for(pane).selected_page() {
            self.handle_activate_tab(page_tab_id(&page), sender);
        }
    }

    pub fn handle_switch_pane(&mut self, sender: &AsyncComponentSender<Self>) {
        let other = self.active_pane().other();
        self.handle_focus_pane(other, sender);
    }

    /// A tab was dragged into `pane`'s strip from the other one.
    pub fn handle_tab_attached(
        &mut self,
        id: u64,
        pane: Pane,
        sender: &AsyncComponentSender<Self>,
    ) {
        if let Some(index) = self.tab_index_by_id(id) {
            self.tabs[index].pane = pane;
        }
        self.ensure_panes_not_empty();
        // The dropped tab is selected in its new strip; make it the active tab.
        if let Some(page) = self.tab_view_for(pane).selected_page() {
            if page_tab_id(&page) == id {
                self.handle_activate_tab(id, sender);
                return;
            }
        }
        self.update_panes();
    }

    /// Turns split view on (opening the current folder in the right pane if it has no
    /// tabs yet) or off (moving the right pane's tabs into the left strip).
    pub fn handle_toggle_split_view(&mut self, sender: &AsyncComponentSender<Self>) {
        if self.split_view {
            // Pages can't be transferred while a tab drag is still settling; retry.
            if self.tab_view.is_transferring_page() || self.tab_view_right.is_transferring_page() {
                let sender = sender.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(150), move || {
                    sender.input(AppMsg::ToggleSplitView);
                });
                return;
            }
            self.split_view = false;
            if !self.merge_right_into_left() {
                self.split_view = true;
            }
        } else {
            self.split_view = true;
            if let Some(paned) = &self.panes_paned {
                let width = paned.width();
                if width > 0 {
                    paned.set_position(width / 2);
                }
            }
            if self.tab_view_right.n_pages() == 0 {
                let path = self.current_path.clone();
                self.open_tab_in_pane(Pane::Right, path, sender);
            } else if let Some(page) = self.tab_view_right.selected_page() {
                self.handle_activate_tab(page_tab_id(&page), sender);
            }
        }
        self.update_panes();
    }

    /// Moves every right-pane tab to the end of the left strip. Returns false if a
    /// transfer was refused (e.g. mid tab-drag), leaving the rest where they are.
    fn merge_right_into_left(&mut self) -> bool {
        while self.tab_view_right.n_pages() > 0 {
            let before = self.tab_view_right.n_pages();
            let page = self.tab_view_right.nth_page(0);
            let id = page_tab_id(&page);
            self.tab_view_right
                .transfer_page(&page, &self.tab_view, self.tab_view.n_pages());
            if self.tab_view_right.n_pages() == before {
                return false;
            }
            if let Some(index) = self.tab_index_by_id(id) {
                self.tabs[index].pane = Pane::Left;
            }
        }
        self.select_tab_page(self.active_tab_index);
        true
    }

    /// Keeps the layout valid after a tab moved or closed: an empty right pane ends
    /// split view; an empty left pane takes over the right pane's tabs.
    pub(crate) fn ensure_panes_not_empty(&mut self) {
        if !self.split_view {
            if self.tab_view_right.n_pages() > 0 && !self.merge_right_into_left() {
                self.split_view = true;
            }
            return;
        }
        if self.tab_view_right.n_pages() == 0 || self.tab_view.n_pages() == 0 {
            self.split_view = false;
            if !self.merge_right_into_left() {
                self.split_view = true;
            }
        }
    }

    /// Applies split/active-pane state to the widgets.
    pub(crate) fn update_panes(&self) {
        SPLIT_ACTIVE.with(|s| s.set(self.split_view));
        let active = self.active_pane();
        if let Some(panes) = &self.pane_boxes {
            for (i, pane_box) in panes.iter().enumerate() {
                let is_active = self.split_view && i == active.index();
                if is_active {
                    pane_box.add_css_class("flux-pane-active");
                } else {
                    pane_box.remove_css_class("flux-pane-active");
                }
            }
            panes[1].set_visible(self.split_view);
        }
        if let Some(paned) = &self.panes_paned {
            if self.split_view {
                paned.add_css_class("flux-split");
            } else {
                paned.remove_css_class("flux-split");
            }
        }
        // In split view each pane always shows its strip so you can see what it holds.
        self.tab_bar.set_autohide(!self.split_view);
        self.tab_bar_right.set_autohide(!self.split_view);
    }

    /// Files dropped onto a tab move into that tab's folder.
    pub fn handle_drop_on_tab(
        &mut self,
        id: u64,
        paths: Vec<PathBuf>,
        sender: &AsyncComponentSender<Self>,
    ) {
        let Some(index) = self.tab_index_by_id(id) else {
            return;
        };
        let dest = if index == self.active_tab_index {
            self.current_path.clone()
        } else {
            self.tabs[index].current_path.clone()
        };
        sender.input(AppMsg::HandleDrop {
            source_paths: paths,
            dest_path: dest,
        });
    }
}
