use adw::prelude::*;

use crate::i18n::tr;
use relm4::factory::FactoryComponent;
use relm4::prelude::*;

/// One compact `key  value` line; clicking (or activating) it copies the value.
pub struct PropertyRow {
    key: String,
    value: String,
}

#[relm4::factory(pub)]
impl FactoryComponent for PropertyRow {
    type Init = (String, String);
    type Input = ();
    type Output = String;
    type ParentWidget = adw::PreferencesGroup;
    type CommandOutput = ();

    view! {
        // A PreferencesRow (not a plain Box) so the group puts it in its boxed list.
        adw::PreferencesRow {
            set_title: &self.key,
            set_use_markup: false,
            set_activatable: true,
            set_tooltip_text: Some(&tr("Click to copy")),
            add_css_class: "flux-prop-row",
            connect_activate[sender, val = self.value.clone()] => move |_| {
                let _ = sender.output(val.clone());
            },
            add_controller = gtk::GestureClick {
                connect_released[sender, val = self.value.clone()] => move |_, _, _, _| {
                    let _ = sender.output(val.clone());
                },
            },

            #[wrap(Some)]
            set_child = &gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 12,

                gtk::Label {
                    set_label: &self.key,
                    set_xalign: 0.0,
                    set_yalign: 0.0,
                    set_width_chars: 11,
                    set_max_width_chars: 16,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    add_css_class: "dim-label",
                    add_css_class: "flux-prop-key",
                },

                // Plain text: values like paths or "System & Disk" aren't markup.
                gtk::Label {
                    set_label: &self.value,
                    set_use_markup: false,
                    set_hexpand: true,
                    set_xalign: 0.0,
                    set_wrap: true,
                    set_wrap_mode: gtk::pango::WrapMode::WordChar,
                    // Keeps long paths/hashes from forcing the column wider.
                    set_width_chars: 1,
                    add_css_class: "flux-prop-value",
                },

                gtk::Image {
                    set_icon_name: Some("edit-copy-symbolic"),
                    set_valign: gtk::Align::Center,
                    add_css_class: "flux-prop-copy",
                },
            },
        }
    }

    fn init_model(init: Self::Init, _: &DynamicIndex, _: FactorySender<Self>) -> Self {
        Self {
            key: init.0,
            value: init.1,
        }
    }
}
