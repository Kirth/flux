use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[test]
fn test_thumbnail_session_invalidation() {
    let load_id = Arc::new(AtomicU64::new(1));
    let initial_session = load_id.load(Ordering::Acquire);

    // Simulate session increment on directory navigation
    let new_session = load_id.fetch_add(1, Ordering::SeqCst) + 1;

    assert_ne!(initial_session, new_session);
    assert_eq!(load_id.load(Ordering::Acquire), 2);
}

#[test]
fn test_folder_preview_sources_picks_sorted_media_only() {
    use flux::services::thumbnails::scan_folder;

    let dir = tempfile::tempdir().unwrap();
    for name in [
        "b.png",
        "A.jpg",
        "clip.mp4",
        "notes.txt",
        "doc.pdf",
        "font.ttf",
        ".hidden.png",
        "e.webp",
    ] {
        std::fs::write(dir.path().join(name), b"x").unwrap();
    }
    std::fs::create_dir(dir.path().join("sub.png")).unwrap(); // a directory, not media

    let names = |paths: Vec<std::path::PathBuf>| -> Vec<String> {
        paths
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    };

    assert_eq!(
        names(scan_folder(dir.path(), false, true).previews),
        ["A.jpg", "b.png", "clip.mp4", "e.webp"]
    );
    assert_eq!(
        names(scan_folder(dir.path(), true, true).previews),
        [".hidden.png", "A.jpg", "b.png", "clip.mp4", "e.webp"]
    );
    assert!(scan_folder(dir.path(), false, false).previews.is_empty());
    assert!(scan_folder(&dir.path().join("missing"), false, true)
        .previews
        .is_empty());
}

#[test]
fn test_categorize_by_extension() {
    use flux::services::thumbnails::{categorize, FileCategory};
    use std::path::Path;

    let cases = [
        ("a.JPG", FileCategory::Images),
        ("a.mkv", FileCategory::Video),
        ("a.flac", FileCategory::Audio),
        ("a.pdf", FileCategory::Documents),
        ("a.rs", FileCategory::Code),
        ("README.md", FileCategory::Code),
        ("a.tar.zst", FileCategory::Archives),
        ("a.ttf", FileCategory::Other),
        ("Makefile", FileCategory::Other),
    ];
    for (name, want) in cases {
        assert_eq!(categorize(Path::new(name)), want, "{name}");
    }
}

#[test]
fn test_scan_folder_stats() {
    use flux::services::thumbnails::{scan_folder, FileCategory};

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.png"), vec![0u8; 1000]).unwrap();
    std::fs::write(dir.path().join("b.mp4"), vec![0u8; 3000]).unwrap();
    std::fs::write(dir.path().join("c.rs"), vec![0u8; 500]).unwrap();
    std::fs::write(dir.path().join(".hidden.zip"), vec![0u8; 9000]).unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();

    let stats = scan_folder(dir.path(), false, false).stats;
    assert_eq!(stats.items, 4); // a.png, b.mp4, c.rs, sub
    assert_eq!(stats.total_bytes, 4500);
    assert_eq!(stats.bytes_of(FileCategory::Images), 1000);
    assert_eq!(stats.bytes_of(FileCategory::Video), 3000);
    assert_eq!(stats.bytes_of(FileCategory::Code), 500);
    assert_eq!(stats.bytes_of(FileCategory::Archives), 0);
    assert!(!stats.truncated);

    let with_hidden = scan_folder(dir.path(), true, false).stats;
    assert_eq!(with_hidden.items, 5);
    assert_eq!(with_hidden.bytes_of(FileCategory::Archives), 9000);
}

#[test]
fn test_bar_segments_log_scale_and_split() {
    use flux::services::thumbnails::{bar_segments, FolderStats};

    let stats = |bytes: [u64; 7]| FolderStats {
        total_bytes: bytes.iter().sum(),
        items: 1,
        by_category: bytes,
        truncated: false,
    };
    let total = |w: [i32; 7]| w.iter().sum::<i32>();

    assert_eq!(bar_segments(&FolderStats::default(), 100), [0; 7]);
    // Tiny but non-empty still shows a sliver.
    assert_eq!(total(bar_segments(&stats([10, 0, 0, 0, 0, 0, 0]), 100)), 3);
    // 1 GB is 6/8 of the way from 1 KB to 100 GB on the log scale.
    assert_eq!(
        total(bar_segments(&stats([1_000_000_000, 0, 0, 0, 0, 0, 0]), 100)),
        75
    );
    // 100 GB and beyond fill the bar.
    assert_eq!(
        total(bar_segments(
            &stats([0, 200_000_000_000, 0, 0, 0, 0, 0]),
            100
        )),
        100
    );
    // Split proportionally; leftovers go to the biggest category.
    let w = bar_segments(&stats([0, 750_000_000, 250_000_000, 0, 0, 0, 0]), 100);
    assert_eq!(total(w), 75);
    assert!(w[1] > w[2] && w[2] > 0, "{w:?}");
}

#[test]
fn test_format_stats_line() {
    use flux::services::thumbnails::{format_stats_line, FolderStats};

    assert_eq!(format_stats_line(&FolderStats::default()), "Empty");
    let only_dirs = FolderStats {
        items: 3,
        ..Default::default()
    };
    assert_eq!(format_stats_line(&only_dirs), "3 items");
    let full = FolderStats {
        items: 340,
        total_bytes: 1_200_000_000,
        ..Default::default()
    };
    assert_eq!(format_stats_line(&full), "1.2\u{a0}GB · 340 items");
    let truncated = FolderStats {
        truncated: true,
        ..full
    };
    assert_eq!(format_stats_line(&truncated), "≥1.2\u{a0}GB · ≥340 items");
}
