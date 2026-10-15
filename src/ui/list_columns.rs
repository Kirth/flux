//! Detail columns for the list view: the column model, the widths and visibility shared by
//! every row, and the sortable/resizable header shown above the file view.
//!
//! Rows are bound in `FileItem::bind_widgets`, which has no access to the app model, so the
//! column layout lives in a thread-local that rows and the header both read from.

use crate::i18n::tr;
use crate::model::{AppMsg, SortBy};
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use relm4::Sender;
use std::cell::RefCell;
use std::collections::HashMap;

/// An optional detail column, in display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListColumn {
    Size,
    Modified,
    Created,
    Type,
    Owner,
    Permissions,
}

impl ListColumn {
    pub const ALL: [ListColumn; 6] = [
        ListColumn::Size,
        ListColumn::Modified,
        ListColumn::Created,
        ListColumn::Type,
        ListColumn::Owner,
        ListColumn::Permissions,
    ];

    pub fn key(self) -> &'static str {
        match self {
            ListColumn::Size => "size",
            ListColumn::Modified => "modified",
            ListColumn::Created => "created",
            ListColumn::Type => "type",
            ListColumn::Owner => "owner",
            ListColumn::Permissions => "permissions",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.key() == key)
    }

    pub fn title(self) -> String {
        tr(match self {
            ListColumn::Size => "Size",
            ListColumn::Modified => "Modified",
            ListColumn::Created => "Created",
            ListColumn::Type => "Type",
            ListColumn::Owner => "Owner",
            ListColumn::Permissions => "Permissions",
        })
    }

    pub fn default_width(self) -> i32 {
        match self {
            ListColumn::Size => 90,
            ListColumn::Modified | ListColumn::Created => 150,
            ListColumn::Type => 150,
            ListColumn::Owner => 90,
            ListColumn::Permissions => 110,
        }
    }

    /// Sizes read best right-aligned; everything else is left-aligned.
    pub fn xalign(self) -> f32 {
        if self == ListColumn::Size {
            1.0
        } else {
            0.0
        }
    }

    /// The sort a header click applies, or `None` when the column is not sortable.
    pub fn sort_by(self) -> Option<SortBy> {
        match self {
            ListColumn::Size => Some(SortBy::Size),
            ListColumn::Modified => Some(SortBy::Date),
            ListColumn::Type => Some(SortBy::Type),
            ListColumn::Created | ListColumn::Owner | ListColumn::Permissions => None,
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|c| *c == self).unwrap_or(0)
    }
}

pub const MIN_COLUMN_WIDTH: i32 = 48;
pub const MAX_COLUMN_WIDTH: i32 = 600;

/// Text shown for a value the item does not have (search results, remote entries).
pub const MISSING: &str = "—";

struct Header {
    root: gtk::Box,
    name_label: gtk::Label,
    cells: Vec<gtk::Widget>,
    labels: Vec<gtk::Label>,
}

struct State {
    visible: [bool; 6],
    widths: [i32; 6],
    rows: Vec<glib::WeakRef<gtk::Box>>,
    header: Option<Header>,
    /// Last applied (visible, sort, ascending), to skip redundant header updates.
    synced: Option<(bool, SortBy, bool)>,
}

impl Default for State {
    fn default() -> Self {
        let mut visible = [false; 6];
        for key in crate::model::UIConfig::default().list_columns {
            if let Some(col) = ListColumn::from_key(&key) {
                visible[col.index()] = true;
            }
        }
        Self {
            visible,
            widths: ListColumn::ALL.map(ListColumn::default_width),
            rows: Vec::new(),
            header: None,
            synced: None,
        }
    }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Loads column visibility and widths from the config.
pub fn init_from_config(ui: &crate::model::UIConfig) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.visible = [false; 6];
        for key in &ui.list_columns {
            if let Some(col) = ListColumn::from_key(key) {
                s.visible[col.index()] = true;
            }
        }
        for col in ListColumn::ALL {
            s.widths[col.index()] = ui
                .list_column_widths
                .get(col.key())
                .copied()
                .unwrap_or(col.default_width())
                .clamp(MIN_COLUMN_WIDTH, MAX_COLUMN_WIDTH);
        }
    });
    apply_all();
}

pub fn is_visible(col: ListColumn) -> bool {
    STATE.with(|s| s.borrow().visible[col.index()])
}

pub fn width(col: ListColumn) -> i32 {
    STATE.with(|s| s.borrow().widths[col.index()])
}

pub fn set_visible(col: ListColumn, visible: bool) {
    STATE.with(|s| s.borrow_mut().visible[col.index()] = visible);
    apply_all();
}

pub fn set_width(col: ListColumn, px: i32) {
    STATE.with(|s| {
        s.borrow_mut().widths[col.index()] = px.clamp(MIN_COLUMN_WIDTH, MAX_COLUMN_WIDTH)
    });
    apply_all();
}

/// Tracks a row's cell box so width/visibility changes reach it while it is alive.
pub fn register_row(cells: &gtk::Box) {
    STATE.with(|s| s.borrow_mut().rows.push(cells.downgrade()));
}

fn apply_layout(cells: &[gtk::Widget], visible: &[bool; 6], widths: &[i32; 6]) {
    for (i, cell) in cells.iter().enumerate().take(6) {
        cell.set_visible(visible[i]);
        if cell.width_request() != widths[i] {
            cell.set_size_request(widths[i], -1);
        }
    }
}

fn children(container: &gtk::Box) -> Vec<gtk::Widget> {
    let mut out = Vec::with_capacity(6);
    let mut next = container.first_child();
    while let Some(w) = next {
        next = w.next_sibling();
        out.push(w);
    }
    out
}

/// Applies the current column layout to one row's cell box.
pub fn apply_to_row(cells: &gtk::Box) {
    STATE.with(|s| {
        let s = s.borrow();
        apply_layout(&children(cells), &s.visible, &s.widths);
    });
}

/// Re-applies the layout to the header and every live row, pruning destroyed rows.
pub fn apply_all() {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let (visible, widths) = (s.visible, s.widths);
        s.rows.retain(|weak| match weak.upgrade() {
            Some(cells) => {
                apply_layout(&children(&cells), &visible, &widths);
                true
            }
            None => false,
        });
        if let Some(header) = &s.header {
            apply_layout(&header.cells, &visible, &widths);
        }
    });
}

/// Shows/hides the header and updates its sort arrow. Cheap when nothing changed.
pub fn sync_header(visible: bool, sort_by: SortBy, ascending: bool) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.synced == Some((visible, sort_by, ascending)) {
            return;
        }
        s.synced = Some((visible, sort_by, ascending));
        let Some(header) = &s.header else {
            return;
        };
        header.root.set_visible(visible);
        let arrow = if ascending { " ↑" } else { " ↓" };
        let title = |t: String, active: bool| if active { format!("{t}{arrow}") } else { t };
        header
            .name_label
            .set_label(&title(tr("Name"), sort_by == SortBy::Name));
        for (col, label) in ListColumn::ALL.iter().zip(&header.labels) {
            label.set_label(&title(col.title(), col.sort_by() == Some(sort_by)));
        }
    });
}

/// Builds the column header; there is one per window, above the tab view.
pub fn build_header(sender: Sender<AppMsg>) -> gtk::Box {
    let root = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(LIST_CELL_SPACING)
        .css_classes(["flux-list-header"])
        .visible(false)
        .build();

    let name_label = gtk::Label::builder()
        .label(tr("Name"))
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    let name_button = gtk::Button::builder()
        .child(&name_label)
        .hexpand(true)
        .css_classes(["flat", "flux-list-header-button"])
        .build();
    {
        let sender = sender.clone();
        name_button.connect_clicked(move |_| {
            let _ = sender.send(AppMsg::SortByColumn(SortBy::Name));
        });
    }
    root.append(&name_button);

    // Detail cells sit flush; each cell's left padding (CSS) holds its resize handle.
    let cells_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .build();
    root.append(&cells_box);

    let mut cells = Vec::new();
    let mut labels = Vec::new();
    for col in ListColumn::ALL {
        // The handle overlays the cell's left edge so the cell keeps exactly the
        // column width, matching the row cells below it.
        // hexpand(false) stops the button's expand flag from widening the cell.
        let cell = gtk::Overlay::builder()
            .hexpand(false)
            .css_classes(["flux-list-header-cell"])
            .build();

        let handle = gtk::Box::builder()
            .width_request(LIST_CELL_PAD)
            .halign(gtk::Align::Start)
            .css_classes(["flux-list-resize"])
            .cursor(&gdk::Cursor::from_name("col-resize", None).unwrap())
            .build();
        connect_resize(&handle, col, sender.clone());

        let label = gtk::Label::builder()
            .label(col.title())
            .xalign(col.xalign())
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .width_chars(1)
            .max_width_chars(1)
            .build();
        let button = gtk::Button::builder()
            .child(&label)
            .hexpand(true)
            .css_classes(["flat", "flux-list-header-button"])
            .build();
        if let Some(sort) = col.sort_by() {
            let sender = sender.clone();
            button.connect_clicked(move |_| {
                let _ = sender.send(AppMsg::SortByColumn(sort));
            });
        } else {
            button.set_can_target(false);
        }
        cell.set_child(Some(&button));
        cell.add_overlay(&handle);

        cells_box.append(&cell);
        cells.push(cell.upcast());
        labels.push(label);
    }

    let right_click = gtk::GestureClick::builder()
        .button(gdk::BUTTON_SECONDARY)
        .build();
    {
        let sender = sender.clone();
        right_click.connect_pressed(move |gesture, _, x, y| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            if let Some(widget) = gesture.widget() {
                show_column_menu(&widget, x, y, sender.clone());
            }
        });
    }
    root.add_controller(right_click);

    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.header = Some(Header {
            root: root.clone(),
            name_label,
            cells,
            labels,
        });
        s.synced = None;
    });
    apply_all();
    root
}

/// Dragging a column's left edge to the left widens it.
fn connect_resize(handle: &gtk::Box, col: ListColumn, sender: Sender<AppMsg>) {
    let drag = gtk::GestureDrag::new();
    let start = std::rc::Rc::new(std::cell::Cell::new(0));
    {
        let start = start.clone();
        drag.connect_drag_begin(move |g, _, _| {
            g.set_state(gtk::EventSequenceState::Claimed);
            start.set(width(col));
        });
    }
    {
        let start = start.clone();
        drag.connect_drag_update(move |_, dx, _| {
            set_width(col, start.get() - dx as i32);
        });
    }
    drag.connect_drag_end(move |_, _, _| {
        let _ = sender.send(AppMsg::SetListColumnWidth(
            col.key().to_string(),
            width(col),
        ));
    });
    handle.add_controller(drag);
}

fn show_column_menu(widget: &gtk::Widget, x: f64, y: f64, sender: Sender<AppMsg>) {
    let menu_model = gio::Menu::new();
    let group = gio::SimpleActionGroup::new();
    for col in ListColumn::ALL {
        menu_model.append(Some(&col.title()), Some(&format!("listcols.{}", col.key())));
        let action =
            gio::SimpleAction::new_stateful(col.key(), None, &is_visible(col).to_variant());
        let sender = sender.clone();
        action.connect_activate(move |_, _| {
            let _ = sender.send(AppMsg::ToggleListColumn(col.key().to_string()));
        });
        group.add_action(&action);
    }
    widget.insert_action_group("listcols", Some(&group));

    let menu = gtk::PopoverMenu::from_model(Some(&menu_model));
    menu.set_has_arrow(false);
    menu.set_parent(widget);
    menu.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
    menu.connect_closed(|m| {
        let m = m.clone();
        glib::idle_add_local_once(move || m.unparent());
    });
    menu.popup();
}

/// Gap between the name and the first detail cell, in rows and in the header.
pub const LIST_CELL_SPACING: i32 = 10;
/// Left padding inside each detail cell (`.flux-list-cell` / header label in CSS); the
/// header's resize handle occupies it, so it doubles as the drag target.
const LIST_CELL_PAD: i32 = 8;

/// Builds the per-row cell box (one label per [`ListColumn`], in order), plus the
/// folder composition mini bar that sits under the Size label.
pub fn build_row_cells() -> (gtk::Box, Vec<gtk::Label>, gtk::Box) {
    let cells = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .valign(gtk::Align::Center)
        .css_classes(["flux-list-columns"])
        .visible(false)
        .build();
    let labels: Vec<gtk::Label> = ListColumn::ALL
        .iter()
        .map(|col| {
            gtk::Label::builder()
                .xalign(col.xalign())
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .width_chars(1)
                .max_width_chars(1)
                .css_classes(["flux-list-cell"])
                .build()
        })
        .collect();

    let size_bar = super::components::build_composition_bar();
    size_bar.set_halign(gtk::Align::End);
    size_bar.add_css_class("flux-folder-bar--mini");
    size_bar.set_visible(false);

    // One top-level child per column, so `apply_layout` sizes the Size wrapper.
    for (col, label) in ListColumn::ALL.iter().zip(&labels) {
        if *col == ListColumn::Size {
            let wrapper = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .valign(gtk::Align::Center)
                .build();
            wrapper.append(label);
            wrapper.append(&size_bar);
            cells.append(&wrapper);
        } else {
            cells.append(label);
        }
    }
    register_row(&cells);
    (cells, labels, size_bar)
}

// ── Cell formatters ─────────────────────────────────────────────────────────

/// `ls -l` style mode string, e.g. `drwxr-xr-x`. Returns `None` for unknown (0) modes.
pub fn format_permissions(mode: u32) -> Option<String> {
    if mode == 0 {
        return None;
    }
    let kind = match mode & 0o170000 {
        0o040000 => 'd',
        0o120000 => 'l',
        0o020000 => 'c',
        0o060000 => 'b',
        0o010000 => 'p',
        0o140000 => 's',
        _ => '-',
    };
    let bit = |mask: u32, c: char| if mode & mask != 0 { c } else { '-' };
    let exec =
        |x: u32, special: u32, set: char, unset: char| match (mode & x != 0, mode & special != 0) {
            (true, true) => set,
            (false, true) => unset,
            (true, false) => 'x',
            (false, false) => '-',
        };
    Some(
        [
            kind,
            bit(0o400, 'r'),
            bit(0o200, 'w'),
            exec(0o100, 0o4000, 's', 'S'),
            bit(0o040, 'r'),
            bit(0o020, 'w'),
            exec(0o010, 0o2000, 's', 'S'),
            bit(0o004, 'r'),
            bit(0o002, 'w'),
            exec(0o001, 0o1000, 't', 'T'),
        ]
        .iter()
        .collect(),
    )
}

/// Human date for a list cell: "Today, 18:06", "Yesterday, 09:12" or the locale date.
pub fn format_list_date(ts: i64) -> Option<String> {
    if ts <= 0 {
        return None;
    }
    let file_dt = glib::DateTime::from_unix_local(ts).ok()?;
    let now = glib::DateTime::now_local().ok()?;
    let same_day = |a: &glib::DateTime, b: &glib::DateTime| {
        a.year() == b.year() && a.month() == b.month() && a.day_of_month() == b.day_of_month()
    };
    let time = file_dt.format("%H:%M").ok()?;
    if same_day(&file_dt, &now) {
        return Some(format!("{}, {}", tr("Today"), time));
    }
    if now.add_days(-1).is_ok_and(|y| same_day(&file_dt, &y)) {
        return Some(format!("{}, {}", tr("Yesterday"), time));
    }
    file_dt.format("%x %H:%M").ok().map(|s| s.to_string())
}

/// Exact timestamp for a date cell tooltip.
pub fn format_exact_date(ts: i64) -> Option<String> {
    (ts > 0)
        .then(|| glib::DateTime::from_unix_local(ts).ok())
        .flatten()
        .and_then(|dt| dt.format("%x %X").ok())
        .map(|s| s.to_string())
}

thread_local! {
    static TYPE_CACHE: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
    static OWNER_CACHE: RefCell<HashMap<u32, String>> = RefCell::new(HashMap::new());
}

/// Human-readable kind ("Folder", "Rust source code", ...), guessed from the name and
/// cached per extension.
pub fn type_description(name: &str, is_dir: bool) -> String {
    if is_dir {
        return tr("Folder");
    }
    // Extension-less names (Makefile, README) are guessed individually.
    let key = std::path::Path::new(name)
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
        .unwrap_or_else(|| name.to_string());
    TYPE_CACHE.with(|c| {
        c.borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                let (content_type, _) = gio::content_type_guess(Some(name), None);
                if !gio::content_type_is_unknown(&content_type) {
                    return gio::content_type_get_description(&content_type).to_string();
                }
                // Unknown to the MIME database: "TF file", like other file managers.
                match std::path::Path::new(name).extension() {
                    Some(ext) => format!("{} {}", ext.to_string_lossy().to_uppercase(), tr("file")),
                    None => tr("File"),
                }
            })
            .clone()
    })
}

/// User name for a uid, falling back to the numeric id; cached per uid.
pub fn owner_name(uid: u32) -> String {
    OWNER_CACHE.with(|c| {
        c.borrow_mut()
            .entry(uid)
            .or_insert_with(|| lookup_user_name(uid).unwrap_or_else(|| uid.to_string()))
            .clone()
    })
}

fn lookup_user_name(uid: u32) -> Option<String> {
    let mut buf = vec![0 as libc::c_char; 1024];
    let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut result: *mut libc::passwd = std::ptr::null_mut();
    loop {
        // SAFETY: all pointers reference live, correctly sized buffers owned by this frame.
        let rc =
            unsafe { libc::getpwuid_r(uid, &mut pwd, buf.as_mut_ptr(), buf.len(), &mut result) };
        if rc == libc::ERANGE && buf.len() < 1 << 16 {
            buf.resize(buf.len() * 2, 0);
            continue;
        }
        if rc != 0 || result.is_null() || pwd.pw_name.is_null() {
            return None;
        }
        // SAFETY: on success pw_name points into `buf` as a NUL-terminated string.
        let name = unsafe { std::ffi::CStr::from_ptr(pwd.pw_name) };
        return Some(name.to_string_lossy().into_owned());
    }
}
