//! Status and label formatting helpers.

use crate::i18n::tr;
use relm4::prelude::*;
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Computes the right-side status string containing active filters and free volume space.
pub fn format_right_status(current_path: &Path, extension_filter: Option<&[String]>) -> String {
    let mut parts = Vec::new();

    if let Some(patterns) = extension_filter {
        if !patterns.is_empty() {
            parts.push(format!("[filter: {}]", patterns.join(", ")));
        }
    }

    if current_path.is_absolute() {
        if let Some(free_bytes) = free_space(current_path) {
            parts.push(format!(
                "{} {}",
                gtk::glib::format_size(free_bytes),
                tr("free")
            ));
        }
    }

    parts.join(" · ")
}

struct FreeSpace {
    path: PathBuf,
    bytes: Option<u64>,
    checked: Option<Instant>,
    in_flight: Option<PathBuf>,
}

static FREE_SPACE: Mutex<FreeSpace> = Mutex::new(FreeSpace {
    path: PathBuf::new(),
    bytes: None,
    checked: None,
    in_flight: None,
});

/// Free space on the volume holding `path`, as last measured off the UI thread.
///
/// This runs on every view update, and `statvfs` blocks for as long as the filesystem does
/// (a FUSE driver busy with thumbnail reads, a hung network mount), so it is measured on a
/// worker thread every few seconds and the cached value is shown meanwhile.
fn free_space(path: &Path) -> Option<u64> {
    const REFRESH: Duration = Duration::from_secs(5);

    let mut cache = FREE_SPACE.lock().unwrap_or_else(|e| e.into_inner());
    if cache.path != path {
        cache.path = path.to_path_buf();
        cache.bytes = None;
        cache.checked = None;
    }
    let stale = cache.checked.is_none_or(|t| t.elapsed() > REFRESH);
    if stale && cache.in_flight.as_deref() != Some(path) {
        cache.in_flight = Some(path.to_path_buf());
        let path = path.to_path_buf();
        std::thread::spawn(move || {
            let bytes = statvfs_free(&path);
            let mut cache = FREE_SPACE.lock().unwrap_or_else(|e| e.into_inner());
            if cache.in_flight.as_ref() == Some(&path) {
                cache.in_flight = None;
            }
            if cache.path == path {
                cache.bytes = bytes;
                cache.checked = Some(Instant::now());
                drop(cache);
                if let Some(sender) = crate::model::SENDER.get() {
                    let _ = sender.send(crate::model::AppMsg::FreeSpaceReady);
                }
            }
        });
    }
    cache.bytes
}

fn statvfs_free(path: &Path) -> Option<u64> {
    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) } != 0 {
        return None;
    }
    let stat = unsafe { stat.assume_init() };
    Some(stat.f_bsize * stat.f_bavail)
}

/// Checks if a paste operation is recursive (pasting a folder into itself or a subfolder).
#[allow(dead_code)]
pub fn is_recursive_paste(src: &Path, dest_dir: &Path) -> bool {
    dest_dir.starts_with(src)
}

/// Returns the label to display for a file in the grid or list.
///
/// If `name`'s extension is in `hidden_extensions` (case-insensitive, optional leading dot),
/// or if `hidden_extensions` contains `*`, returns the stem only. Otherwise returns `name`.
pub fn format_display_label(name: &str, is_dir: bool, hidden_extensions: &[String]) -> String {
    if is_dir || hidden_extensions.is_empty() {
        return name.to_string();
    }

    let hide_all = hidden_extensions.iter().any(|h| {
        let trimmed = h.trim();
        trimmed == "*" || trimmed == ".*"
    });

    let mut patterns: Vec<String> = hidden_extensions
        .iter()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty() && s != "*" && s != ".*")
        .collect();

    patterns.sort_by_key(|a| std::cmp::Reverse(a.len()));

    let name_lc = name.to_ascii_lowercase();

    let glob_matchers: Vec<globset::GlobMatcher> = patterns
        .iter()
        .filter(|c| c.contains('*') || c.contains('?') || c.contains('['))
        .filter_map(|clean| {
            let pat = if clean.starts_with('*') {
                clean.clone()
            } else if clean.starts_with('.') {
                format!("*{}", clean)
            } else {
                format!("*.{}", clean)
            };
            globset::GlobBuilder::new(&pat)
                .case_insensitive(true)
                .literal_separator(false)
                .build()
                .ok()
                .map(|g| g.compile_matcher())
        })
        .collect();

    for matcher in &glob_matchers {
        if matcher.is_match(&name_lc) {
            for (byte_idx, _) in name.match_indices('.') {
                if byte_idx > 0 && byte_idx < name.len() {
                    let ext_slice = &name_lc[byte_idx..];
                    let test_str = format!("x{}", ext_slice);
                    if matcher.is_match(&test_str) {
                        return name[..byte_idx].to_string();
                    }
                }
            }
        }
    }

    for clean in &patterns {
        if clean.contains('*') || clean.contains('?') || clean.contains('[') {
            continue;
        }

        let suffix = if clean.starts_with('.') {
            clean.clone()
        } else {
            format!(".{}", clean)
        };

        if suffix.is_empty() || suffix == "." {
            continue;
        }

        if name_lc.ends_with(&suffix) {
            let remaining_len = name.len().saturating_sub(suffix.len());
            if remaining_len > 0 && name.is_char_boundary(remaining_len) {
                return name[..remaining_len].to_string();
            }
        }
    }

    if hide_all {
        return std::path::Path::new(name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(name)
            .to_string();
    }

    name.to_string()
}
