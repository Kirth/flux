//! Folder info shown on folder icons: thumbnails of a few images/videos inside it,
//! and how full/heavy it is by file type. Direct contents only (no recursion).

use crate::i18n::tr;
use crate::model::{AppMsg, FluxApp};
use crate::utils;
use futures::stream::{self, StreamExt};
use relm4::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

/// How many previews a folder icon shows (a 2×2 grid).
pub const MAX_FOLDER_PREVIEWS: usize = 4;

/// Preview candidates kept per folder; some may fail to thumbnail (corrupt or
/// unsupported files), so the loader tries a few extra before giving up.
const PREVIEW_CANDIDATES: usize = MAX_FOLDER_PREVIEWS * 3;

/// Directory entries examined per folder, so huge folders stay cheap to scan.
const SCAN_LIMIT: usize = 10_000;

/// Coarse file type used to colour a folder's composition bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileCategory {
    Images,
    Video,
    Audio,
    Documents,
    Code,
    Archives,
    Other,
}

impl FileCategory {
    pub const ALL: [FileCategory; 7] = [
        FileCategory::Images,
        FileCategory::Video,
        FileCategory::Audio,
        FileCategory::Documents,
        FileCategory::Code,
        FileCategory::Archives,
        FileCategory::Other,
    ];

    /// CSS class suffix: `flux-cat-<key>`.
    pub fn key(self) -> &'static str {
        match self {
            FileCategory::Images => "images",
            FileCategory::Video => "video",
            FileCategory::Audio => "audio",
            FileCategory::Documents => "documents",
            FileCategory::Code => "code",
            FileCategory::Archives => "archives",
            FileCategory::Other => "other",
        }
    }

    pub fn title(self) -> String {
        tr(match self {
            FileCategory::Images => "Images",
            FileCategory::Video => "Video",
            FileCategory::Audio => "Audio",
            FileCategory::Documents => "Documents",
            FileCategory::Code => "Code & text",
            FileCategory::Archives => "Archives",
            FileCategory::Other => "Other",
        })
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|c| *c == self).unwrap_or(6)
    }
}

/// Classifies a file by extension (no I/O).
pub fn categorize(path: &Path) -> FileCategory {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "pdf" | "doc" | "docx" | "odt" | "ods" | "odp" | "xls" | "xlsx" | "ppt" | "pptx"
        | "epub" | "rtf" => return FileCategory::Documents,
        "ttf" | "otf" | "woff" | "woff2" | "ttc" => return FileCategory::Other,
        "mp3" | "flac" | "ogg" | "opus" | "wav" | "aac" | "m4a" | "wma" | "aiff" => {
            return FileCategory::Audio
        }
        "zip" | "tar" | "gz" | "tgz" | "xz" | "zst" | "bz2" | "7z" | "rar" | "iso" | "deb"
        | "rpm" | "appimage" | "lz4" | "lzma" => return FileCategory::Archives,
        "rs" | "py" | "js" | "mjs" | "ts" | "tsx" | "jsx" | "c" | "h" | "cc" | "cpp" | "hpp"
        | "go" | "java" | "kt" | "rb" | "php" | "sh" | "bash" | "zsh" | "fish" | "lua" | "nix"
        | "toml" | "json" | "yaml" | "yml" | "xml" | "html" | "css" | "scss" | "md" | "txt"
        | "rst" | "csv" | "log" | "ini" | "conf" | "cfg" | "kdl" | "sql" | "tf" | "vue"
        | "svelte" | "zig" | "hs" | "ml" | "ex" | "exs" | "el" | "vim" => {
            return FileCategory::Code
        }
        _ => {}
    }
    match crate::services::loader::is_visual_media_by_ext(path) {
        (true, _) => FileCategory::Images,
        (_, true) => FileCategory::Video,
        _ => FileCategory::Other,
    }
}

/// How full and heavy a folder is, by its direct contents.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FolderStats {
    /// Bytes of regular files directly inside the folder.
    pub total_bytes: u64,
    /// Entries directly inside the folder (files and subfolders).
    pub items: u64,
    /// Bytes per [`FileCategory`], indexed like `FileCategory::ALL`.
    pub by_category: [u64; 7],
    /// The scan stopped at its entry limit; real numbers are larger.
    pub truncated: bool,
}

impl FolderStats {
    pub fn bytes_of(&self, cat: FileCategory) -> u64 {
        self.by_category[cat.index()]
    }
}

/// Result of [`scan_folder`].
#[derive(Debug, Default)]
pub struct FolderScan {
    pub previews: Vec<PathBuf>,
    pub stats: FolderStats,
}

/// One pass over `dir`'s direct entries: preview candidates (sorted images/videos, a
/// few more than [`MAX_FOLDER_PREVIEWS`], only if `want_previews`) and [`FolderStats`].
///
/// Sizes come from `DirEntry::metadata` (no symlink follow); unreadable dirs yield an
/// empty scan. PDFs and fonts get thumbnails in flux but aren't used as previews.
pub fn scan_folder(dir: &Path, show_hidden: bool, want_previews: bool) -> FolderScan {
    let mut scan = FolderScan::default();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return scan;
    };
    let mut media: Vec<(String, PathBuf)> = Vec::new();
    for (n, entry) in entries.flatten().enumerate() {
        if n >= SCAN_LIMIT {
            scan.stats.truncated = true;
            break;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        scan.stats.items += 1;
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let path = entry.path();
        let category = categorize(&path);
        scan.stats.total_bytes += meta.len();
        scan.stats.by_category[category.index()] += meta.len();
        if want_previews && matches!(category, FileCategory::Images | FileCategory::Video) {
            media.push((name.to_lowercase(), path));
        }
    }
    media.sort_by(|a, b| a.0.cmp(&b.0));
    scan.previews = media
        .into_iter()
        .take(PREVIEW_CANDIDATES)
        .map(|(_, p)| p)
        .collect();
    scan
}

/// Pixel widths of each category's segment in a bar of `max_px`.
///
/// The filled length grows logarithmically with total size: 1 KB or less is empty,
/// 100 GB or more fills the bar; anything non-empty gets at least 3 px. The filled
/// length is split across categories in proportion to their bytes.
pub fn bar_segments(stats: &FolderStats, max_px: i32) -> [i32; 7] {
    let mut widths = [0; 7];
    if stats.total_bytes == 0 || max_px <= 0 {
        return widths;
    }
    let frac = (((stats.total_bytes as f64).log10() - 3.0) / 8.0).clamp(0.0, 1.0);
    let filled = ((max_px as f64 * frac).round() as i32).clamp(3.min(max_px), max_px);

    let mut used = 0;
    let mut largest = 0;
    for (i, &bytes) in stats.by_category.iter().enumerate() {
        widths[i] = (filled as f64 * bytes as f64 / stats.total_bytes as f64).floor() as i32;
        used += widths[i];
        if bytes > stats.by_category[largest] {
            largest = i;
        }
    }
    // Rounding leftovers go to the dominant category so the bar keeps its length.
    widths[largest] += filled - used;
    widths
}

/// `"1.2 GB · 340 items"`, `"12 items"` when there are no file bytes, `"Empty"` for
/// nothing at all; prefixed with `≥` when the scan was truncated.
pub fn format_stats_line(stats: &FolderStats) -> String {
    let more = if stats.truncated { "≥" } else { "" };
    let items = if stats.items == 1 {
        tr("1 item")
    } else {
        format!("{more}{} {}", stats.items, tr("items"))
    };
    if stats.items == 0 {
        tr("Empty")
    } else if stats.total_bytes == 0 {
        items
    } else {
        format!(
            "{more}{} · {items}",
            gtk::glib::format_size(stats.total_bytes)
        )
    }
}

/// Per-category breakdown for tooltips, largest first: one `"Images  820 MB"` per line.
pub fn breakdown_tooltip(stats: &FolderStats) -> String {
    let mut parts: Vec<(FileCategory, u64)> = FileCategory::ALL
        .iter()
        .map(|c| (*c, stats.bytes_of(*c)))
        .filter(|(_, b)| *b > 0)
        .collect();
    parts.sort_by_key(|p| std::cmp::Reverse(p.1));
    let mut lines = vec![format_stats_line(stats)];
    lines.extend(
        parts
            .into_iter()
            .map(|(c, b)| format!("{}  {}", c.title(), gtk::glib::format_size(b))),
    );
    lines.join("\n")
}

impl FluxApp {
    /// Scans each folder in `folder_tasks` in the background (sharing the thumbnail
    /// semaphore) for previews and stats, reported via `AppMsg::FolderInfoReady`.
    pub fn spawn_folder_info_loader(
        &self,
        folder_tasks: Vec<(u32, PathBuf)>,
        current_session: u64,
        tab_index: usize,
        sender: AsyncComponentSender<Self>,
    ) {
        if folder_tasks.is_empty() {
            return;
        }
        let want_previews = self.config.ui.show_folder_previews && self.config.ui.show_thumbnails;
        let want_stats = self.config.ui.show_folder_stats;
        let session_arc = self.load_id.clone();
        let show_hidden = self.show_hidden;
        let max_threads = self.config.ui.thumbnail_threads.max(1);
        let sem = self.thumbnail_manager.get_semaphore(max_threads);

        relm4::spawn(async move {
            stream::iter(folder_tasks)
                .map(|(grid_idx, dir)| {
                    let sender = sender.clone();
                    let session_arc = session_arc.clone();
                    let sem = sem.clone();
                    async move {
                        let stale = || session_arc.load(Ordering::Acquire) != current_session;
                        if stale() {
                            return;
                        }
                        let Ok(_permit) = sem.acquire().await else {
                            return;
                        };
                        if stale() {
                            return;
                        }

                        let scan_dir = dir.clone();
                        let scan = tokio::task::spawn_blocking(move || {
                            scan_folder(&scan_dir, show_hidden, want_previews)
                        })
                        .await
                        .unwrap_or_default();

                        let mut previews = Vec::with_capacity(scan.previews.len());
                        for source in scan.previews {
                            if stale() {
                                return;
                            }
                            if previews.len() == MAX_FOLDER_PREVIEWS {
                                break;
                            }
                            if let Some(t) = utils::media::get_or_create_thumbnail(&source).await {
                                previews.push(t);
                            }
                        }

                        let stats = want_stats.then_some(scan.stats);
                        if (previews.is_empty() && stats.is_none()) || stale() {
                            return;
                        }
                        let _ = sender.input_sender().send(AppMsg::FolderInfoReady {
                            grid_idx,
                            path: dir,
                            previews,
                            stats,
                            load_id: current_session,
                            tab_index,
                        });
                    }
                })
                .buffer_unordered(max_threads)
                .collect::<Vec<()>>()
                .await;
        });
    }

    /// Stores a folder's previews and stats and paints them if its cell is on screen.
    ///
    /// Updates the item in place (no remove/insert), so selection and scroll are kept.
    pub fn handle_folder_info_ready(
        &mut self,
        grid_idx: u32,
        path: PathBuf,
        previews: Vec<gtk::gdk::Texture>,
        stats: Option<FolderStats>,
        load_id: u64,
        tab_index: usize,
    ) {
        if load_id != self.load_id.load(Ordering::SeqCst) {
            return;
        }
        let Some(tab) = self.tabs.get(tab_index) else {
            return;
        };
        let Some(wrapper) = tab.files.get(grid_idx) else {
            return;
        };
        {
            let mut item = wrapper.borrow_mut();
            if item.path != path {
                return;
            }
            item.folder_previews = previews;
            item.folder_stats = stats;
        }

        if tab_index == self.active_tab_index {
            if let Some(cell) = self.find_widget_by_path(&path) {
                crate::ui::apply_folder_info_to_cell(&cell, &wrapper.borrow());
            }
        }
    }
}
