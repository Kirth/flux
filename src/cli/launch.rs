use super::shortcuts::setup_shortcuts;
use super::watcher::setup_config_watcher;
use crate::model::{AppInit, AppMsg, FluxApp};
use adw::prelude::*;
use adw::{gio, glib};
use relm4::prelude::*;
use std::path::PathBuf;

#[allow(clippy::too_many_arguments)]
pub(super) fn launch_main_app(
    start_path: PathBuf,
    open_archive: Option<PathBuf>,
    quick_list: Option<Vec<PathBuf>>,
    tag_search: Option<String>,
    no_sidebar: bool,
    no_header: bool,
    no_statusbar: bool,
) {
    // Defer non-critical CSS/Theme loading and dependency checks by 150ms.
    glib::timeout_add_local(std::time::Duration::from_millis(150), move || {
        crate::utils::helpers::load_custom_css();
        crate::utils::helpers::load_custom_background_images();
        setup_config_watcher();
        std::thread::spawn(crate::utils::deps::check_optional_deps);
        glib::ControlFlow::Break
    });

    // --- MAIN APP HANDLER ---
    // NOTE:
    // Setting application_id with .flags(gio::ApplicationFlags...) in the
    // builder triggers a synchronous D-Bus handshake and Wayland compositor
    // lookup that blocks the main thread for around ~200ms.
    let base_app = adw::Application::builder().build();

    assert!(
        base_app.application_id().is_none(),
        "\n\n[flux] STARTUP REGRESSION: application_id is set on the main adw::Application.\n\
     This triggers a synchronous D-Bus name acquisition and Wayland compositor\n\
     lookup on the main thread, adding ~200ms to startup time.\n\
     Remove .application_id(...) from the adw::Application::builder() call.\n"
    );

    assert!(
        !base_app.flags().contains(gio::ApplicationFlags::NON_UNIQUE),
        "\n\n[flux] STARTUP REGRESSION: NON_UNIQUE flag is set on the main adw::Application.\n\
     This triggers a synchronous D-Bus handshake and Wayland compositor lookup\n\
     on the main thread, adding ~200ms to startup time.\n\
     Remove .flags(gio::ApplicationFlags::NON_UNIQUE) from the adw::Application::builder() call.\n"
    );

    setup_shortcuts(&base_app);

    // relm4 adds and shows the main window before the async init has built (and
    // sized) its content. Size it here, before it's mapped:
    // - its minimum, else it maps with libadwaita's default 360×200 and a tiling
    //   compositor's narrower tile gets a too-wide first frame;
    // - its default size, else setting it later (on a mapped window) acts as a resize
    //   request that overrides the compositor's tile size until its next configure.
    base_app.connect_window_added(|_, window| {
        if window.is::<adw::Window>() && !window.is_mapped() && window.transient_for().is_none() {
            window.set_size_request(200, 200);
            let ui = crate::utils::load_config().ui;
            window.set_default_size(ui.startup_window_width, ui.startup_window_height);
        }
    });

    let app: RelmApp<AppMsg> = RelmApp::from_app(base_app);
    app.allow_multiple_instances(true);
    app.with_args(vec![]).run_async::<FluxApp>(AppInit {
        start_path,
        open_archive,
        quick_list,
        tag_search,
        no_sidebar,
        no_header,
        no_statusbar,
    });
}
