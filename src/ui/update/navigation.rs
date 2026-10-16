use crate::model::{AppMsg, FluxApp};
use relm4::prelude::*;

pub(super) fn handle(
    app: &mut FluxApp,
    msg: AppMsg,
    sender: &AsyncComponentSender<FluxApp>,
) -> Result<(), AppMsg> {
    match msg {
        AppMsg::Navigate(path) => {
            app.stop_video_preview();
            app.handle_navigate(path, sender);
        }
        AppMsg::GoBack => {
            app.stop_video_preview();
            app.handle_go_back(sender);
        }
        AppMsg::GoForward => {
            app.stop_video_preview();
            app.handle_go_forward(sender);
            sender.input(AppMsg::Refresh);
        }
        AppMsg::SyncPathEntry => {}
        AppMsg::ToggleLocationPanel => {
            app.toggle_sidebar_right_panel(crate::model::RightPanelType::Location, sender)
        }
        AppMsg::ToggleConnectPanel => {
            app.toggle_sidebar_right_panel(crate::model::RightPanelType::Connect, sender)
        }
        AppMsg::JumpToRecent(rank) => {
            let target_index = if rank == 0 { 0 } else { rank - 1 };
            if let Some(target_path) = app.recent_stack.get(target_index).cloned() {
                if rank != 0 && target_path == app.current_path {
                    return Ok(());
                }
                sender.input(AppMsg::Navigate(target_path));
            }
        }
        AppMsg::ClearRecents => app.handle_clear_recents(sender),
        AppMsg::NewTab(target_path) => app.handle_new_tab(target_path, sender),
        AppMsg::OpenTabs(targets) => app.handle_open_tabs(targets, sender),
        AppMsg::Open { position, tab_id } => app.handle_open(position, tab_id, sender),
        AppMsg::SwitchTab(index) => app.handle_switch_tab(index, sender),
        AppMsg::ActivateTab(id) => app.handle_activate_tab(id, sender),
        AppMsg::CloseTabId(id) => app.handle_close_tab_id(id, sender),
        AppMsg::TabAttached { id, pane } => app.handle_tab_attached(id, pane, sender),
        AppMsg::DropOnTab { id, paths } => app.handle_drop_on_tab(id, paths, sender),
        AppMsg::ToggleSplitView => app.handle_toggle_split_view(sender),
        AppMsg::FocusPane(pane) => app.handle_focus_pane(pane, sender),
        AppMsg::SwitchPane => app.handle_switch_pane(sender),
        AppMsg::CloseTab(index_opt) => app.handle_close_tab(index_opt, sender),
        AppMsg::NextTab => app.handle_next_tab(),
        AppMsg::PrevTab => app.handle_prev_tab(),
        AppMsg::Refresh => {
            app.active_video_preview = None;
            app.folder_cache.clear();
            app.handle_refresh_path(sender);
        }
        AppMsg::EnterArchive(archive_path) => {
            app.stop_video_preview();
            app.handle_enter_archive(archive_path, sender)
        }
        AppMsg::LoadArchiveWithPassword {
            archive_path,
            prefix,
            password,
        } => {
            app.archive_locked = false;
            app.load_archive(archive_path, prefix, Some(password), sender);
            app.update_breadcrumbs();
        }
        AppMsg::NavigateNetwork => app.handle_navigate_network(sender),
        AppMsg::NavigateTag(tag) => app.handle_navigate_tag(tag, sender),
        other => return Err(other),
    }
    Ok(())
}
