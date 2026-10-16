use super::section::MetadataSection;
use super::sections::SectionData;
use super::window::PropertiesMsg;
use adw::prelude::*;
use relm4::factory::FactoryVecDeque;
use relm4::prelude::*;

/// Column (`true` = right) and index of a section within a [`Page`].
pub(super) type Slot = (bool, usize);

/// One tab of the Properties window: optional full-width widgets on top, then the
/// sections split in reading order over two columns (stacked when the window is narrow).
pub(super) struct Page {
    left: FactoryVecDeque<MetadataSection>,
    right: FactoryVecDeque<MetadataSection>,
    root: gtk::ScrolledWindow,
}

impl Page {
    /// Builds the page; returns where section `placeholder` (an index into `sections`)
    /// ended up so it can be replaced later. `narrow` gets a setter that stacks the columns.
    pub(super) fn new(
        header: &[gtk::Widget],
        sections: Vec<SectionData>,
        placeholder: Option<usize>,
        sender: &relm4::Sender<PropertiesMsg>,
        narrow: &adw::Breakpoint,
    ) -> (Self, Option<Slot>) {
        let column = || {
            FactoryVecDeque::builder()
                .launch(gtk::Box::new(gtk::Orientation::Vertical, 12))
                .forward(sender, PropertiesMsg::CopyToClipboard)
        };
        let (mut left, mut right) = (column(), column());

        // Split in order so the stacked layout still reads top to bottom. A section
        // weighs its rows plus its header; the placeholder fills in later, so it's
        // counted as a typical filled-in folder total.
        let weight = |i: usize, rows: usize| {
            if placeholder == Some(i) {
                8
            } else {
                rows + 1
            }
        };
        let total: usize = sections
            .iter()
            .enumerate()
            .map(|(i, s)| weight(i, s.1.len()))
            .sum();
        let mut slot = None;
        {
            let (mut lg, mut rg) = (left.guard(), right.guard());
            let mut left_weight = 0;
            for (i, (title, items)) in sections.into_iter().enumerate() {
                let to_right = left_weight * 2 >= total && !lg.is_empty();
                if placeholder == Some(i) {
                    slot = Some((to_right, if to_right { rg.len() } else { lg.len() }));
                }
                if to_right {
                    rg.push_back((title, items));
                } else {
                    left_weight += weight(i, items.len());
                    lg.push_back((title, items));
                }
            }
        }

        let columns = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .homogeneous(true)
            .spacing(12)
            .valign(gtk::Align::Start)
            .build();
        columns.append(left.widget());
        columns.append(right.widget());
        narrow.add_setter(
            &columns,
            "orientation",
            Some(&gtk::Orientation::Vertical.to_value()),
        );

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();
        for widget in header {
            content.append(widget);
        }
        content.append(&columns);

        let root = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(
                &adw::Clamp::builder()
                    .maximum_size(1100)
                    .child(&content)
                    .build(),
            )
            .build();

        (Self { left, right, root }, slot)
    }

    pub(super) fn widget(&self) -> &gtk::ScrolledWindow {
        &self.root
    }

    /// Swaps the section at `slot` for `section` (appends it if the slot is gone).
    pub(super) fn replace(&mut self, slot: Option<Slot>, section: SectionData) {
        let column = match slot {
            Some((true, _)) => &mut self.right,
            _ => &mut self.left,
        };
        let mut guard = column.guard();
        match slot {
            Some((_, idx)) if idx < guard.len() => {
                guard.remove(idx);
                guard.insert(idx, section);
            }
            _ => {
                guard.push_back(section);
            }
        }
    }
}
