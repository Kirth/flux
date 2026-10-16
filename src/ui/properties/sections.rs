use super::probes::{
    format_size, get_elf_info, get_git_info, get_shannon_entropy, get_text_metrics,
};
use crate::i18n::tr;
use crate::utils;
use chrono::{DateTime, Local};
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

/// A section title plus its `(label, value)` rows.
pub(super) type SectionData = (String, Vec<(String, String)>);

/// Bytes per [`FileCategory`](crate::services::thumbnails::FileCategory), in `ALL` order.
pub(super) type CategoryBytes = [u64; 7];

/// Which tab of the Properties window a section is shown on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Tab {
    General,
    Details,
    Git,
}

impl Tab {
    pub(super) const ALL: [Tab; 3] = [Tab::General, Tab::Details, Tab::Git];
}

/// Everything [`build_sections`] found: tagged sections and, for folders, the
/// direct-contents breakdown for the chart.
pub(super) struct Built {
    pub sections: Vec<(Tab, SectionData)>,
    pub contents_by_category: Option<CategoryBytes>,
}

/// Result of the recursive folder walk.
#[derive(Debug)]
pub struct FolderTotal {
    pub section: SectionData,
    pub by_category: CategoryBytes,
}

/// Assembles every metadata section for `path`.
///
/// `mime_type` is pre-computed by the caller (it's used elsewhere too), and
/// `file_content` is the first 1 MiB of the file (or empty if unreadable).
pub(super) fn build_sections(path: &Path, mime_type: &str, file_content: &[u8]) -> Built {
    let filename = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let mut sections_data: Vec<(Tab, SectionData)> = Vec::new();
    let mut contents_by_category = None;

    if path.is_dir() {
        sections_data.push((
            Tab::General,
            (
                tr("Folder"),
                vec![
                    (tr("Name"), filename.clone()),
                    (
                        tr("Location"),
                        path.parent()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_default(),
                    ),
                ],
            ),
        ));
        let (contents, by_category) = folder_contents_section(path);
        sections_data.push((Tab::General, contents));
        contents_by_category = Some(by_category);
    } else {
        sections_data.push((
            Tab::General,
            (
                tr("File Identity"),
                vec![
                    (tr("Filename"), filename),
                    (
                        tr("Extension"),
                        path.extension()
                            .map(|e| e.to_string_lossy().to_string())
                            .unwrap_or(tr("None")),
                    ),
                    (tr("MIME Type"), mime_type.to_string()),
                ],
            ),
        ));
    }

    // ── Symlink ─────────────────────────────────────────────────────────────
    if path.is_symlink() {
        if let Ok(target) = fs::read_link(path) {
            sections_data.push((
                Tab::Details,
                (
                    tr("Symlink Info"),
                    vec![
                        (tr("Target"), target.to_string_lossy().to_string()),
                        (tr("Broken"), (!target.exists()).to_string()),
                        (
                            tr("Canonical"),
                            path.canonicalize()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string(),
                        ),
                    ],
                ),
            ));
        }
    }

    // ── Extended attributes ────────────────────────────────────────────────
    if let Ok(attrs) = xattr::list(path) {
        let mut xattrs_list = Vec::new();
        for attr in attrs {
            if let Ok(Some(val)) = xattr::get(path, &attr) {
                xattrs_list.push((
                    attr.to_string_lossy().to_string(),
                    String::from_utf8_lossy(&val).to_string(),
                ));
            }
        }
        if !xattrs_list.is_empty() {
            sections_data.push((Tab::Details, (tr("Extended Attributes"), xattrs_list)));
        }
    }

    // ── System & disk, temporal, security ──────────────────────────────────
    if let Ok(meta) = path.metadata() {
        let mut system_disk = vec![(tr("Full Path"), path.to_string_lossy().to_string())];
        // A folder's own size is just its directory entry; its totals are listed above.
        if !meta.is_dir() {
            system_disk.push((
                tr("Size"),
                format!("{} ({} B)", format_size(meta.len()), meta.len()),
            ));
            system_disk.push((tr("Disk Usage"), format_size(meta.blocks() * 512)));
        }
        system_disk.push((tr("Inode"), meta.ino().to_string()));
        system_disk.push((tr("Device ID"), meta.dev().to_string()));

        if let Ok(mounts) = fs::read_to_string("/proc/self/mounts") {
            let mount_info = mounts
                .lines()
                .rfind(|l| path.starts_with(l.split_whitespace().nth(1).unwrap_or("")));
            if let Some(line) = mount_info {
                let parts: Vec<&str> = line.split_whitespace().collect();
                system_disk.push((tr("Mount Source"), parts[0].to_string()));
                system_disk.push((tr("FS Type"), parts[2].to_string()));
            }
        }
        sections_data.push((Tab::Details, (tr("System & Disk"), system_disk)));

        let fmt_time = |t: std::time::SystemTime| -> String {
            let dt: DateTime<Local> = t.into();
            dt.format("%Y-%m-%d %H:%M:%S").to_string()
        };

        sections_data.push((
            Tab::General,
            (
                tr("Temporal"),
                vec![
                    (
                        tr("Created"),
                        meta.created().map(fmt_time).unwrap_or_default(),
                    ),
                    (
                        tr("Modified"),
                        meta.modified().map(fmt_time).unwrap_or_default(),
                    ),
                    (
                        tr("Accessed"),
                        meta.accessed().map(fmt_time).unwrap_or_default(),
                    ),
                ],
            ),
        ));

        let mut security = vec![
            (
                tr("Permissions"),
                format!(
                    "{}  ({:03o})",
                    crate::ui::list_columns::format_permissions(meta.mode()).unwrap_or_default(),
                    meta.mode() & 0o777
                ),
            ),
            (
                tr("Owner"),
                format!(
                    "{} (uid {}, gid {})",
                    crate::ui::list_columns::owner_name(meta.uid()),
                    meta.uid(),
                    meta.gid()
                ),
            ),
        ];
        if !file_content.is_empty() {
            security.push((
                tr("Entropy"),
                format!("{:.4}", get_shannon_entropy(file_content)),
            ));
            let hash_hex = Sha256::digest(file_content)
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>();
            security.push((tr("SHA256"), hash_hex));
        }
        sections_data.push((Tab::Details, (tr("Security"), security)));
    }

    // ── ELF analysis ────────────────────────────────────────────────────────
    if mime_type.contains("executable") || mime_type.contains("sharedlib") {
        if let Some(elf) = get_elf_info(path) {
            sections_data.push((Tab::Details, (tr("Executable Analysis"), elf)));
        }
    }

    // ── Text content metrics ───────────────────────────────────────────────
    let is_text = mime_type.contains("text")
        || mime_type.contains("javascript")
        || mime_type.contains("json")
        || mime_type.contains("xml");
    let text_exts = [".py", ".rs", ".toml", ".yaml", ".sh", ".md", ".txt"];
    let has_text_ext = path
        .extension()
        .map(|e| text_exts.contains(&e.to_string_lossy().as_ref()))
        .unwrap_or(false);
    if (is_text || has_text_ext) && !file_content.is_empty() {
        if let Some(metrics) = get_text_metrics(file_content) {
            sections_data.push((Tab::General, (tr("Content Metrics"), metrics)));
        }
    }

    // ── Image dimensions ────────────────────────────────────────────────────
    if mime_type.starts_with("image/") {
        if let Ok(dim) = imagesize::size(path) {
            sections_data.push((
                Tab::General,
                (
                    tr("Visual Media"),
                    vec![
                        (tr("Dimensions"), format!("{}x{}", dim.width, dim.height)),
                        (
                            tr("Aspect Ratio"),
                            format!("{:.2}:1", dim.width as f64 / dim.height as f64),
                        ),
                    ],
                ),
            ));
        }
    }

    // ── Git history ─────────────────────────────────────────────────────────
    if let Some(git) = get_git_info(path) {
        sections_data.push((Tab::Git, (tr("Git History"), git)));
    }

    let _ = utils::media::get_mime_type; // keep the `utils` import live for callers

    Built {
        sections: sections_data,
        contents_by_category,
    }
}

/// Rows for a breakdown of bytes per file category, largest first.
fn category_rows(by_category: &CategoryBytes) -> Vec<(String, String)> {
    use crate::services::thumbnails::FileCategory;
    let mut parts: Vec<(FileCategory, u64)> = FileCategory::ALL
        .iter()
        .zip(by_category)
        .map(|(c, b)| (*c, *b))
        .filter(|(_, b)| *b > 0)
        .collect();
    parts.sort_by_key(|p| std::cmp::Reverse(p.1));
    parts
        .into_iter()
        .map(|(c, b)| (format!("  {}", c.title()), format_size(b)))
        .collect()
}

/// Direct contents of a folder: entry count, size of the files in it, and by type.
fn folder_contents_section(path: &Path) -> (SectionData, CategoryBytes) {
    let stats = crate::services::thumbnails::scan_folder(path, true, false).stats;
    let more = if stats.truncated { "≥" } else { "" };
    let mut rows = vec![
        (tr("Items"), format!("{more}{}", stats.items)),
        (
            tr("Files size"),
            format!(
                "{more}{} ({} B)",
                format_size(stats.total_bytes),
                stats.total_bytes
            ),
        ),
    ];
    rows.extend(category_rows(&stats.by_category));
    ((tr("Contents"), rows), stats.by_category)
}

/// Placeholder shown until [`folder_total_section`] finishes.
pub(super) fn folder_total_placeholder() -> SectionData {
    (
        tr("Total (including subfolders)"),
        vec![(tr("Size"), tr("Calculating…"))],
    )
}

/// Everything below a folder, walked recursively (symlinks not followed). Stops after
/// a couple of million entries or ~20 s so huge trees still return; then values are
/// marked with "≥". Runs off the main thread.
pub(super) fn folder_total_section(path: &Path) -> FolderTotal {
    use crate::services::thumbnails::{categorize, FileCategory};
    const MAX_ENTRIES: u64 = 2_000_000;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);

    let (mut files, mut dirs, mut bytes, mut disk) = (0u64, 0u64, 0u64, 0u64);
    let mut by_category = [0u64; 7];
    let mut truncated = false;
    let mut stack = vec![path.to_path_buf()];
    'walk: while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if files + dirs >= MAX_ENTRIES || std::time::Instant::now() > deadline {
                truncated = true;
                break 'walk;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                dirs += 1;
                stack.push(entry.path());
            } else {
                files += 1;
                bytes += meta.len();
                disk += meta.blocks() * 512;
                let cat = categorize(&entry.path());
                if let Some(i) = FileCategory::ALL.iter().position(|c| *c == cat) {
                    by_category[i] += meta.len();
                }
            }
        }
    }

    let more = if truncated { "≥" } else { "" };
    let mut rows = vec![
        (
            tr("Size"),
            format!("{more}{} ({more}{} B)", format_size(bytes), bytes),
        ),
        (tr("Disk Usage"), format!("{more}{}", format_size(disk))),
        (tr("Files"), format!("{more}{files}")),
        (tr("Folders"), format!("{more}{dirs}")),
    ];
    rows.extend(category_rows(&by_category));
    FolderTotal {
        section: (tr("Total (including subfolders)"), rows),
        by_category,
    }
}
