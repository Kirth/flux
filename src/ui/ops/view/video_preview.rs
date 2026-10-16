use crate::i18n::tr;
use crate::model::{AppMsg, FluxApp};
use crate::utils;
use adw::gio::prelude::*;
use adw::prelude::*;
use gtk::glib;
use relm4::prelude::*;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const VIDEO_PREVIEW_LAUNCH_LIMIT: usize = 5;
const VIDEO_PREVIEW_LAUNCH_WINDOW: Duration = Duration::from_secs(1);
const VIDEO_PREVIEW_COOLDOWN: Duration = Duration::from_secs(5);

/// Whether GStreamer can play video in this process.
///
/// GtkMediaFile's player aborts the whole process (a fatal `g_error`) when GStreamer
/// has no `playbin3`, e.g. when flux is started on NixOS without
/// `GST_PLUGIN_SYSTEM_PATH_1_0`, so previews must check before creating one.
fn video_playback_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        let ok = gstreamer::init().is_ok() && gstreamer::ElementFactory::find("playbin3").is_some();
        if !ok {
            eprintln!(
                "[flux] GStreamer 'playbin3' not found - video previews disabled \
                 (is GST_PLUGIN_SYSTEM_PATH_1_0 set?)"
            );
            if let Some(s) = crate::model::SENDER.get() {
                let _ = s.send(AppMsg::ShowToast(tr(
                    "Video previews unavailable: GStreamer plugins not found",
                )));
            }
        }
        ok
    })
}

impl FluxApp {
    /// Finds the grid cell widget that is currently showing the given file path.
    ///
    /// Looks up the item by the `flux_path` data key that `FileItem::bind` stores on
    /// the overlay root and card box.
    ///
    /// NOTE: do not rewrite this to match on `widget_name()`. `bind()` intentionally
    /// never sets the widget name (see the warning in `components.rs`), so a name
    /// lookup always returns `None` and video previews silently do nothing.
    pub fn find_widget_by_path(&self, path: &std::path::Path) -> Option<gtk::Widget> {
        fn search(widget: &gtk::Widget, target: &std::path::Path) -> Option<gtk::Widget> {
            let hit = unsafe {
                widget
                    .data::<PathBuf>("flux_path")
                    .is_some_and(|p| p.as_ref() == target)
            };
            if hit {
                return Some(widget.clone());
            }
            let mut child = widget.first_child();
            while let Some(c) = child {
                if let Some(found) = search(&c, target) {
                    return Some(found);
                }
                child = c.next_sibling();
            }
            None
        }

        let active_view = self
            .tabs
            .get(self.active_tab_index)
            .map(|t| &t.files.view)?;
        search(active_view.as_ref(), path)
    }

    /// The icon image of a grid cell found by [`Self::find_widget_by_path`].
    ///
    /// Previews play as this image's paintable, so they occupy exactly the thumbnail's
    /// box. A separate `gtk::Video` would size itself to the video's resolution and
    /// reflow the whole grid.
    fn preview_image(cell: &gtk::Widget) -> Option<gtk::Image> {
        // SAFETY: "icon_widget" is only ever set to a gtk::Image, in the item setup.
        unsafe {
            cell.data::<gtk::Image>("icon_widget")
                .map(|p| p.as_ref().clone())
        }
    }

    fn playing_stream(image: &gtk::Image) -> Option<gtk::MediaStream> {
        image.paintable().and_downcast::<gtk::MediaStream>()
    }

    /// The preview stream attached to a cell, shown or still waiting for its first frame.
    fn preview_stream(cell: &gtk::Widget) -> Option<gtk::MediaStream> {
        // SAFETY: "preview_stream" is only ever set to a gtk::MediaStream, below.
        unsafe {
            cell.data::<gtk::MediaStream>("preview_stream")
                .map(|p| p.as_ref().clone())
        }
    }

    pub fn stop_video_preview(&mut self) {
        if let Some(source_id) = self.video_preview_source.take() {
            source_id.remove();
        }

        let Some(active_path) = self.active_video_preview.take() else {
            return;
        };

        if let Some(widget) = self.find_widget_by_path(&active_path) {
            // SAFETY: data keys are only ever set with these types, in this file.
            let (stream, restore) = unsafe {
                (
                    widget.steal_data::<gtk::MediaStream>("preview_stream"),
                    widget.steal_data::<Option<gtk::gdk::Paintable>>("preview_restore"),
                )
            };
            if let Some(stream) = stream {
                stream.pause();
            }
            if let Some(image) = Self::preview_image(&widget) {
                // Only put the thumbnail back if the video had replaced it already.
                if Self::playing_stream(&image).is_some() {
                    image.set_paintable(restore.flatten().as_ref());
                }
            }
        }
    }

    pub fn sync_video_preview(&mut self) {
        crate::hit!("sync_video_preview");
        if !self.config.ui.autoplay_video_previews {
            self.stop_video_preview();
            return;
        }

        let now = Instant::now();

        if self
            .video_preview_cooldown_until
            .is_some_and(|until| now < until)
        {
            self.stop_video_preview();
            return;
        }
        self.video_preview_cooldown_until = None;

        let selection = self.get_selection();
        if selection.len() != 1 {
            self.stop_video_preview();
            return;
        }

        let selected_path = selection[0].clone();
        let (_, is_vid) = utils::media::is_visual_media(&selected_path);
        if !is_vid || !video_playback_available() {
            self.stop_video_preview();
            return;
        }

        if self.active_video_preview.as_ref() == Some(&selected_path) {
            let still_playing = self
                .find_widget_by_path(&selected_path)
                .is_some_and(|w| Self::preview_stream(&w).is_some());

            if still_playing {
                return;
            }
        }

        self.stop_video_preview();

        if self.find_widget_by_path(&selected_path).is_some() {
            self.handle_trigger_video_preview(selected_path);
        } else {
            let target_path = selected_path;
            let source_id =
                glib::timeout_add_local_once(std::time::Duration::from_millis(100), move || {
                    if let Some(s) = crate::model::SENDER.get() {
                        let _ = s.send(AppMsg::TriggerVideoPreview(target_path));
                    }
                });
            self.video_preview_source = Some(source_id);
        }
    }

    pub fn handle_trigger_video_preview(&mut self, path: std::path::PathBuf) {
        crate::hit!("handle_trigger_video_preview");
        self.video_preview_source = None;

        let now = Instant::now();

        if self
            .video_preview_cooldown_until
            .is_some_and(|until| now < until)
        {
            return;
        }

        while self
            .video_preview_launches
            .front()
            .is_some_and(|t| now.duration_since(*t) > VIDEO_PREVIEW_LAUNCH_WINDOW)
        {
            self.video_preview_launches.pop_front();
        }

        if self.video_preview_launches.len() >= VIDEO_PREVIEW_LAUNCH_LIMIT {
            self.video_preview_cooldown_until = Some(now + VIDEO_PREVIEW_COOLDOWN);
            self.video_preview_launches.clear();
            self.stop_video_preview();

            if let Some(s) = crate::model::SENDER.get() {
                let msg = tr("Video preview throttled: too many quick selections, waiting {}s")
                    .replace("{}", &VIDEO_PREVIEW_COOLDOWN.as_secs().to_string());
                let _ = s.send(AppMsg::ShowToast(msg));
            }

            return;
        }

        self.video_preview_launches.push_back(now);

        let selection = self.get_selection();
        if selection.len() != 1 || selection[0] != path {
            return;
        }

        // Ensure any active preview is fully torn down first
        self.stop_video_preview();

        self.active_video_preview = Some(path.clone());

        // Resolve real disk path: if it's an archive virtual path, extract to a tempfile first
        let resolved_file = if let Some((archive_path, inner_path)) =
            crate::services::archive::parse_archive_uri(&path.to_string_lossy())
        {
            match crate::services::archive::extract_entry_to_tempfile(
                &archive_path,
                &inner_path,
                None,
            ) {
                Ok(tmp) => {
                    let tmp_path = tmp.path().to_path_buf();
                    // Keep the temp file alive until preview stops or app exits
                    if let Ok(file) = tmp.keep() {
                        let (_, path_buf) = file;
                        crate::services::archive::register_temp_file(path_buf.clone());
                        Some(gtk::gio::File::for_path(path_buf))
                    } else {
                        Some(gtk::gio::File::for_path(tmp_path))
                    }
                }
                Err(_) => None,
            }
        } else {
            Some(gtk::gio::File::for_path(&path))
        };

        let Some(gfile) = resolved_file else {
            return;
        };

        if !video_playback_available() {
            return;
        }

        if let Some(child) = self.find_widget_by_path(&path) {
            let Some(image) = Self::preview_image(&child) else {
                return;
            };
            let media_file = gtk::MediaFile::for_file(&gfile);
            media_file.set_muted(true);
            media_file.set_loop(true);

            // Keep the thumbnail so stopping the preview can put it back.
            if let Some(old_stream) = Self::preview_stream(&child) {
                old_stream.pause();
            }
            if Self::playing_stream(&image).is_none() {
                // SAFETY: read back with the same type in `stop_video_preview` / unbind.
                unsafe {
                    child.set_data::<Option<gtk::gdk::Paintable>>(
                        "preview_restore",
                        image.paintable(),
                    );
                }
            }
            // SAFETY: read back as gtk::MediaStream in `preview_stream` / stop / unbind.
            unsafe {
                child.set_data::<gtk::MediaStream>("preview_stream", media_file.clone().upcast());
            }

            // Swap the thumbnail for the video only once the first frame is decoded;
            // swapping immediately shows an empty icon until playback has started.
            let first_frame: std::rc::Rc<std::cell::RefCell<Option<glib::SignalHandlerId>>> =
                Default::default();
            let handler = media_file.connect_invalidate_contents({
                let image = image.downgrade();
                let cell = child.downgrade();
                let first_frame = first_frame.clone();
                move |stream| {
                    if let Some(id) = first_frame.borrow_mut().take() {
                        stream.disconnect(id);
                    }
                    let (Some(image), Some(cell)) = (image.upgrade(), cell.upgrade()) else {
                        return;
                    };
                    // Still this cell's current preview (not stopped or recycled)?
                    let current = Self::preview_stream(&cell)
                        .is_some_and(|s| s.upcast_ref::<gtk::gdk::Paintable>() == stream);
                    if current {
                        image.set_paintable(Some(stream));
                    }
                }
            });
            first_frame.replace(Some(handler));
            media_file.play();
        }
    }
}
