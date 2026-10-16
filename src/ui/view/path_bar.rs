//! Moves the header's title stack (breadcrumbs / path entry / search) to its own row
//! under the header when the header can't fit it next to its buttons.
//!
//! Not a breakpoint: libadwaita applies one breakpoint at a time and the window already
//! uses two (sidebar auto-hide, compact), and whether it fits depends on the path length
//! and which buttons are showing, not on a fixed window width.

use super::component::FluxAppWidgets;
use adw::prelude::*;
use gtk::glib;
use std::cell::Cell;

/// Extra room needed before moving back up, so it doesn't flap at the boundary.
const HYSTERESIS_PX: i32 = 16;
/// Gaps around the title between the two button groups.
const TITLE_GAPS_PX: i32 = 12;

fn natural_width(widget: &impl IsA<gtk::Widget>) -> i32 {
    widget.measure(gtk::Orientation::Horizontal, -1).1
}

/// The header's internal `GtkCenterBox` (start buttons / title / end buttons).
fn find_center_box(widget: &gtk::Widget) -> Option<gtk::CenterBox> {
    if let Some(center) = widget.downcast_ref::<gtk::CenterBox>() {
        return Some(center.clone());
    }
    let mut child = widget.first_child();
    while let Some(c) = child {
        if let Some(found) = find_center_box(&c) {
            return Some(found);
        }
        child = c.next_sibling();
    }
    None
}

/// Width the header's buttons need, excluding the title. The header's own natural
/// width can't be used: it keeps the title centred, so it reserves twice the wider
/// button group, while it will happily shift the title off-centre when short of room.
fn buttons_width(header: &adw::HeaderBar) -> i32 {
    match find_center_box(header.upcast_ref()) {
        Some(center) => {
            let side = |w: Option<gtk::Widget>| w.map_or(0, |w| natural_width(&w));
            side(center.start_widget()) + side(center.end_widget())
        }
        None => natural_width(header),
    }
}

pub(super) fn setup_path_bar(widgets: &FluxAppWidgets) {
    let header = widgets.header_bar.clone();
    let stack = widgets.header_stack.clone();
    let path_bar = widgets.path_bar.clone();
    let below = Cell::new(false);

    // Measured on the toolbar view: unlike the header it stays mapped when the header
    // is hidden. GTK caches measurements, so this is a few lookups per frame.
    widgets.toolbar_view.add_tick_callback(move |view, _| {
        let avail = view.width();
        if avail <= 0 {
            return glib::ControlFlow::Continue;
        }

        let need = buttons_width(&header) + natural_width(&stack) + TITLE_GAPS_PX;
        let move_below = if below.get() {
            need + HYSTERESIS_PX > avail
        } else {
            need > avail
        };

        if move_below != below.get() {
            below.set(move_below);
            // The breadcrumbs grab focus when mapped; keep whatever had it.
            let focus = view.root().and_then(|r| r.focus());
            if move_below {
                header.set_title_widget(None::<&gtk::Widget>);
                stack.set_hexpand(true);
                path_bar.append(&stack);
            } else {
                path_bar.remove(&stack);
                stack.set_hexpand(false);
                header.set_title_widget(Some(&stack));
            }
            // Show/hide the row now, before restoring focus: showing it maps the
            // breadcrumbs, and that must not happen after the restore.
            path_bar.set_visible(move_below && header.is_visible());
            if let Some(focus) = focus {
                match focus.downcast_ref::<gtk::Text>() {
                    // Don't select a search the user is typing.
                    Some(text) => {
                        text.grab_focus_without_selecting();
                    }
                    None => {
                        focus.grab_focus();
                    }
                }
            }
        }

        let show = below.get() && header.is_visible();
        if path_bar.is_visible() != show {
            path_bar.set_visible(show);
        }
        glib::ControlFlow::Continue
    });
}
