//! "Space by Type" donut chart for folders, coloured like the folder weight bars.

use super::probes::format_size;
use super::sections::CategoryBytes;
use crate::i18n::tr;
use crate::services::thumbnails::FileCategory;
use adw::prelude::*;
use std::cell::Cell;
use std::f64::consts::{FRAC_PI_2, TAU};
use std::rc::Rc;

/// Fallbacks for the `@flux_cat_*` colours in `style.css`, in `FileCategory::ALL` order.
const FALLBACK: [&str; 7] = [
    "#b48ead", "#d08770", "#88c0d0", "#ebcb8b", "#81a1c1", "#bf616a", "#808080",
];

pub(super) struct CategoryChart {
    root: adw::PreferencesGroup,
    data: Rc<Cell<CategoryBytes>>,
    area: gtk::DrawingArea,
    legend: gtk::Grid,
}

impl CategoryChart {
    /// Chart of a folder's direct contents; [`Self::update`] swaps in the recursive totals.
    pub(super) fn new(by_category: CategoryBytes) -> Self {
        let root = adw::PreferencesGroup::builder()
            .title(tr("Space by Type"))
            .description(tr("Direct contents · calculating subfolders…"))
            .build();

        let data = Rc::new(Cell::new(by_category));
        let area = gtk::DrawingArea::builder()
            .content_width(160)
            .content_height(160)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .build();
        {
            let data = data.clone();
            area.set_draw_func(move |area, cr, w, h| draw_donut(area, cr, w, h, data.get()));
        }

        let legend = gtk::Grid::builder()
            .column_spacing(12)
            .row_spacing(4)
            .valign(gtk::Align::Center)
            .build();

        let card = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(24)
            .css_classes(["card", "flux-prop-chart"])
            .build();
        card.append(&area);
        card.append(&legend);
        root.add(&card);

        let chart = Self {
            root,
            data,
            area,
            legend,
        };
        chart.fill_legend();
        chart
    }

    pub(super) fn widget(&self) -> &adw::PreferencesGroup {
        &self.root
    }

    /// Shows the recursive (including subfolders) breakdown.
    pub(super) fn update(&self, by_category: CategoryBytes) {
        self.data.set(by_category);
        self.root.set_description(Some(&tr("Including subfolders")));
        self.fill_legend();
        self.area.queue_draw();
    }

    fn fill_legend(&self) {
        while let Some(child) = self.legend.first_child() {
            self.legend.remove(&child);
        }
        let parts = sorted_parts(self.data.get());
        let total: u64 = parts.iter().map(|p| p.1).sum();
        if total == 0 {
            let empty = gtk::Label::builder()
                .label(tr("No files"))
                .css_classes(["dim-label"])
                .build();
            self.legend.attach(&empty, 0, 0, 1, 1);
            return;
        }
        for (row, (cat, bytes)) in parts.into_iter().enumerate() {
            let row = row as i32;
            let swatch = gtk::Box::builder()
                .valign(gtk::Align::Center)
                .css_classes([
                    "flux-prop-swatch".to_string(),
                    format!("flux-cat-{}", cat.key()),
                ])
                .build();
            let name = gtk::Label::builder()
                .label(cat.title())
                .xalign(0.0)
                .width_chars(12)
                .build();
            let size = gtk::Label::builder()
                .label(format_size(bytes))
                .xalign(1.0)
                .css_classes(["numeric"])
                .build();
            let pct = gtk::Label::builder()
                .label(percent(bytes, total))
                .xalign(1.0)
                .width_chars(6)
                .css_classes(["dim-label", "numeric"])
                .build();
            self.legend.attach(&swatch, 0, row, 1, 1);
            self.legend.attach(&name, 1, row, 1, 1);
            self.legend.attach(&size, 2, row, 1, 1);
            self.legend.attach(&pct, 3, row, 1, 1);
        }
    }
}

/// `12.3%`, or `<0.1%` for slivers that would otherwise read as nothing.
fn percent(bytes: u64, total: u64) -> String {
    let pct = bytes as f64 * 100.0 / total as f64;
    if pct < 0.1 {
        "<0.1%".to_string()
    } else {
        format!("{pct:.1}%")
    }
}

/// Non-empty categories, largest first.
fn sorted_parts(by_category: CategoryBytes) -> Vec<(FileCategory, u64)> {
    let mut parts: Vec<(FileCategory, u64)> = FileCategory::ALL
        .iter()
        .copied()
        .zip(by_category)
        .filter(|(_, b)| *b > 0)
        .collect();
    parts.sort_by_key(|p| std::cmp::Reverse(p.1));
    parts
}

#[allow(deprecated)] // StyleContext::lookup_color: the only way to read @define-color values
fn category_color(area: &gtk::DrawingArea, i: usize) -> gtk::gdk::RGBA {
    let key = FileCategory::ALL[i].key();
    area.style_context()
        .lookup_color(&format!("flux_cat_{key}"))
        .unwrap_or_else(|| FALLBACK[i].parse().unwrap_or(gtk::gdk::RGBA::BLACK))
}

#[allow(deprecated)] // Widget::color needs GTK 4.10; flux targets 4.8
fn fg_color(area: &gtk::DrawingArea) -> gtk::gdk::RGBA {
    area.style_context().color()
}

fn draw_donut(
    area: &gtk::DrawingArea,
    cr: &gtk::cairo::Context,
    w: i32,
    h: i32,
    by_category: CategoryBytes,
) {
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    let radius = cx.min(cy) - 2.0;
    let ring = radius * 0.28;
    let mid = radius - ring / 2.0;
    cr.set_line_width(ring);

    let parts = sorted_parts(by_category);
    let total: u64 = parts.iter().map(|p| p.1).sum();
    let fg = fg_color(area);

    if total == 0 {
        cr.set_source_rgba(fg.red() as f64, fg.green() as f64, fg.blue() as f64, 0.12);
        cr.arc(cx, cy, mid, 0.0, TAU);
        let _ = cr.stroke();
    } else {
        // Small gaps between slices, unless one category fills the ring.
        let gap = if parts.len() > 1 { 1.5 / mid } else { 0.0 };
        let mut start = -FRAC_PI_2;
        for (cat, bytes) in &parts {
            let sweep = *bytes as f64 / total as f64 * TAU;
            let i = FileCategory::ALL.iter().position(|c| c == cat).unwrap_or(6);
            let c = category_color(area, i);
            cr.set_source_rgba(
                c.red() as f64,
                c.green() as f64,
                c.blue() as f64,
                c.alpha() as f64,
            );
            if sweep > gap * 2.0 {
                cr.arc(cx, cy, mid, start + gap, start + sweep - gap);
                let _ = cr.stroke();
            }
            start += sweep;
        }
    }

    // Total in the middle.
    let text = if total == 0 {
        tr("Empty")
    } else {
        format_size(total)
    };
    let layout = area.create_pango_layout(Some(&text));
    let mut font = area.pango_context().font_description().unwrap_or_default();
    font.set_weight(gtk::pango::Weight::Bold);
    layout.set_font_description(Some(&font));
    let (tw, th) = layout.pixel_size();
    cr.set_source_rgba(fg.red() as f64, fg.green() as f64, fg.blue() as f64, 0.9);
    cr.move_to(cx - tw as f64 / 2.0, cy - th as f64 / 2.0);
    pangocairo::functions::show_layout(cr, &layout);
}
