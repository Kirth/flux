use super::chart::CategoryChart;
use super::page::{Page, Slot};
use super::sections::{
    build_sections, folder_total_placeholder, folder_total_section, FolderTotal, SectionData, Tab,
};
use crate::i18n::tr;
use crate::utils;
use adw::gio;
use adw::prelude::*;
use relm4::prelude::*;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::Command;

pub struct FileProperties {
    /// Tabs, in [`Tab::ALL`] order; tabs without sections are skipped.
    pages: Vec<Page>,
    /// Page index and slot of the "Calculating…" placeholder for folders.
    total_placeholder: Option<(usize, Slot)>,
    chart: Option<CategoryChart>,
    filename: String,
    app_list: Vec<gio::AppInfo>,
    current_mime: String,
    toast_overlay: adw::ToastOverlay,
}

#[derive(Debug)]
pub enum PropertiesMsg {
    CopyToClipboard(String),
    AppSelected(u32),
    /// The background folder walk finished; replaces the "Calculating…" section.
    FolderTotal(FolderTotal),
}

#[relm4::component(pub)]
impl SimpleComponent for FileProperties {
    type Init = PathBuf;
    type Input = PropertiesMsg;
    type Output = ();

    view! {
        adw::Window {
            set_default_size: (860, 640),
            #[watch] set_title: Some(&format!("{} - {}", tr("Properties"), model.filename)),

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                #[name = "header"]
                adw::HeaderBar {
                    set_show_end_title_buttons: false,
                    set_show_start_title_buttons: false,
                },

                #[local_ref]
                overlay -> adw::ToastOverlay {
                    #[local_ref]
                    stack -> adw::ViewStack {
                        set_vexpand: true,
                    },
                }
            }
        }
    }

    fn init(
        path: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let filename = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let mime_type = utils::media::get_mime_type(&path);

        let is_dir = path.is_dir();
        let mut file_content = Vec::new();
        if !is_dir {
            if let Ok(f) = fs::File::open(&path) {
                let _ = f.take(1024 * 1024).read_to_end(&mut file_content);
            }
        }

        let built = build_sections(&path, &mime_type, &file_content);

        let toast_overlay = adw::ToastOverlay::new();

        let app_group = adw::PreferencesGroup::new();

        let mut all_apps: Vec<gio::AppInfo> = gio::AppInfo::all().into_iter().collect();
        all_apps.sort_by_key(|a| a.name().to_lowercase());

        let app_list_store = gtk::StringList::new(&[]);
        let default_app = gio::AppInfo::default_for_type(&mime_type, false);
        let mut selected_idx = gtk::INVALID_LIST_POSITION;

        for (i, app) in all_apps.iter().enumerate() {
            app_list_store.append(&app.name());
            if let Some(ref def) = default_app {
                if app.equal(def) {
                    selected_idx = i as u32;
                }
            }
        }

        let dropdown = gtk::DropDown::builder()
            .model(&app_list_store)
            .selected(selected_idx)
            .enable_search(true)
            .expression(gtk::PropertyExpression::new(
                gtk::StringObject::static_type(),
                None::<gtk::Expression>,
                "string",
            ))
            .halign(gtk::Align::End)
            .valign(gtk::Align::Center)
            .build();

        let row = adw::ActionRow::builder()
            .title(tr("Open With").as_str())
            .subtitle(format!("{} {}", tr("Default application for"), mime_type))
            .activatable_widget(&dropdown)
            .build();
        row.add_suffix(&dropdown);
        app_group.add(&row);

        let sender_clone = sender.clone();
        dropdown.connect_selected_notify(move |row| {
            sender_clone.input(PropertiesMsg::AppSelected(row.selected()));
        });

        let chart = built.contents_by_category.map(CategoryChart::new);

        // Narrow window: stack each page's two columns.
        let narrow = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            720.0,
            adw::LengthUnit::Px,
        ));

        let stack = adw::ViewStack::new();
        let mut pages = Vec::new();
        let mut total_placeholder = None;
        for tab in Tab::ALL {
            let mut sections: Vec<SectionData> = built
                .sections
                .iter()
                .filter(|(t, _)| *t == tab)
                .map(|(_, s)| s.clone())
                .collect();
            let mut header: Vec<gtk::Widget> = Vec::new();
            let mut placeholder = None;
            if tab == Tab::General {
                if let Some(chart) = &chart {
                    header.push(chart.widget().clone().upcast());
                }
                header.push(app_group.clone().upcast());
                // Folders: the recursive total can take a while, so it's filled in later.
                if is_dir {
                    let idx = sections.len().min(2);
                    sections.insert(idx, folder_total_placeholder());
                    placeholder = Some(idx);
                }
            }
            if sections.is_empty() && header.is_empty() {
                continue;
            }
            let (page, slot) = Page::new(
                &header,
                sections,
                placeholder,
                sender.input_sender(),
                &narrow,
            );
            let (name, title, icon) = match tab {
                Tab::General => ("general", tr("General"), "dialog-information-symbolic"),
                Tab::Details => ("details", tr("Details"), "drive-harddisk-symbolic"),
                Tab::Git => ("git", tr("Git"), "document-open-recent-symbolic"),
            };
            stack.add_titled_with_icon(page.widget(), Some(name), &title, icon);
            if let Some(slot) = slot {
                total_placeholder = Some((pages.len(), slot));
            }
            pages.push(page);
        }

        if is_dir {
            let walk_path = path.clone();
            let walk_sender = sender.input_sender().clone();
            std::thread::spawn(move || {
                let total = folder_total_section(&walk_path);
                let _ = walk_sender.send(PropertiesMsg::FolderTotal(total));
            });
        }

        let model = FileProperties {
            pages,
            total_placeholder,
            chart,
            filename,
            app_list: all_apps,
            current_mime: mime_type,
            toast_overlay: toast_overlay.clone(),
        };

        let overlay = &model.toast_overlay;
        let stack = &stack;

        let widgets = view_output!();

        root.add_breakpoint(narrow);
        // A single tab needs no switcher; the header then shows the window title.
        if model.pages.len() > 1 {
            let switcher = adw::ViewSwitcher::builder()
                .stack(stack)
                .policy(adw::ViewSwitcherPolicy::Wide)
                .build();
            widgets.header.set_title_widget(Some(&switcher));
        }

        // Escape closes the window (it opens as a modal over flux).
        let keys = gtk::EventControllerKey::new();
        let window = root.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == adw::gdk::Key::Escape {
                window.close();
                return gtk::glib::Propagation::Stop;
            }
            gtk::glib::Propagation::Proceed
        });
        root.add_controller(keys);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            PropertiesMsg::FolderTotal(total) => {
                if let Some(chart) = &self.chart {
                    chart.update(total.by_category);
                }
                let (page, slot) = match self.total_placeholder.take() {
                    Some((page, slot)) => (page, Some(slot)),
                    None => (0, None),
                };
                if let Some(page) = self.pages.get_mut(page) {
                    page.replace(slot, total.section);
                }
            }
            PropertiesMsg::CopyToClipboard(text) => {
                if let Some(display) = adw::gdk::Display::default() {
                    display.clipboard().set_text(&text);
                    self.toast_overlay
                        .add_toast(adw::Toast::new(&tr("Copied to clipboard")));
                }
            }
            PropertiesMsg::AppSelected(idx) => {
                if idx == gtk::INVALID_LIST_POSITION {
                    return;
                }
                if let Some(app) = self.app_list.get(idx as usize) {
                    let _ = app.set_as_default_for_type(&self.current_mime);

                    if let Some(id) = app.id() {
                        let _ = Command::new("xdg-mime")
                            .args(["default", id.as_ref(), &self.current_mime])
                            .status();
                    }

                    let name = app.name().to_string();
                    self.toast_overlay.add_toast(adw::Toast::new(&format!(
                        "{} {}",
                        tr("Set as default:"),
                        name
                    )));
                }
            }
        }
    }
}
