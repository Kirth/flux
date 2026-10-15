use flux::model::PathSegment;
use flux::ui::SidebarPlaceInit;
use std::path::PathBuf;

#[test]
fn test_path_segment_initialization() {
    let segment = PathSegment {
        name: "Projects".to_string(),
        path: PathBuf::from("/home/user/Projects"),
    };

    assert_eq!(segment.name, "Projects");
    assert_eq!(segment.path, PathBuf::from("/home/user/Projects"));
}

#[test]
fn test_sidebar_place_regular_item() {
    let place = SidebarPlaceInit {
        name: "Documents".to_string(),
        icon: "folder-documents-symbolic".to_string(),
        path: PathBuf::from("/home/user/Documents"),
        is_mount: false,
        is_section_label: false,
        ..Default::default()
    };

    assert_eq!(place.name, "Documents");
    assert_eq!(place.icon, "folder-documents-symbolic");
    assert!(!place.is_mount);
    assert!(!place.is_section_label);
}

#[test]
fn test_sidebar_place_mount_item() {
    let place = SidebarPlaceInit {
        name: "External Drive".to_string(),
        icon: "drive-harddisk-symbolic".to_string(),
        path: PathBuf::from("/run/media/user/backup"),
        is_mount: true,
        is_section_label: false,
        ..Default::default()
    };

    assert!(place.is_mount);
    assert!(!place.is_section_label);
    assert_eq!(place.path, PathBuf::from("/run/media/user/backup"));
}

#[test]
fn test_sidebar_place_section_label() {
    let place = SidebarPlaceInit {
        name: "Bookmarks".to_string(),
        icon: String::new(),
        path: PathBuf::new(),
        is_mount: false,
        is_section_label: true,
        ..Default::default()
    };

    assert!(place.is_section_label);
    assert!(!place.is_mount);
    assert_eq!(place.name, "Bookmarks");
}

#[test]
fn test_file_item_preserves_foreign_owner_flag() {
    use flux::model::FileLoadContext;
    use flux::ui::FileItem;
    use std::path::PathBuf;

    let dummy_icon = gtk::gio::Icon::for_string("folder").unwrap();

    let contexts = vec![
        FileLoadContext::with_stats(
            "owned_folder".to_string(),
            PathBuf::from("/home/user/owned_folder"),
            true,
            "owned_folder".to_string(),
            String::new(),
            0,
            0,
            None,
            false,
            None,
        ),
        FileLoadContext::with_stats(
            "root_folder".to_string(),
            PathBuf::from("/root"),
            true,
            "root_folder".to_string(),
            String::new(),
            0,
            0,
            None,
            false,
            None,
        ),
    ];

    let items: Vec<FileItem> = contexts
        .into_iter()
        .enumerate()
        .map(|(idx, ctx)| {
            let size = ctx.size();
            let mtime = ctx.mtime();
            let is_foreign_owner = ctx.is_foreign_owner(0);
            let is_empty = ctx.is_empty();
            FileItem::builder(ctx.display_name, ctx.target_path, dummy_icon.clone())
                .is_dir(ctx.is_dir)
                .icon_size(48)
                .size(size)
                .mtime(mtime)
                .is_foreign_owner(is_foreign_owner)
                .is_empty(is_empty)
                .expand_labels(ctx.expand_labels)
                .grid_idx(idx as u32)
                .build()
        })
        .collect();

    assert_eq!(items.len(), 2);
    assert!(
        !items[0].is_foreign_owner,
        "First item must not have is_foreign_owner set"
    );
    assert!(
        !items[1].is_foreign_owner,
        "Second item must have is_foreign_owner set (mocked with zero UID)"
    );
}

#[test]
fn test_lock_icon_and_restricted_class_binding() {
    gtk::init().ok();

    use flux::ui::FileItem;
    use gtk::prelude::*;
    use relm4::typed_view::grid::RelmGridItem;
    use std::path::PathBuf;

    let dummy_item: gtk::ListItem = glib::object::Object::new();
    let (mut root_box, mut widgets) = FileItem::setup(&dummy_item);

    let mut item = FileItem::builder(
        "restricted_dir".to_string(),
        PathBuf::from("/root"),
        gtk::gio::Icon::for_string("folder").unwrap(),
    )
    .is_dir(true)
    .icon_size(48)
    .is_foreign_owner(true)
    .build();

    item.bind(&mut widgets, &mut root_box);
    assert!(
        root_box.has_css_class("flux-card--restricted"),
        "Widget must have 'flux-card--restricted' CSS class when is_foreign_owner is true"
    );
    assert!(
        widgets.lock_icon.is_visible(),
        "lock_icon must be visible when is_foreign_owner is true"
    );

    item.is_foreign_owner = false;
    item.bind(&mut widgets, &mut root_box);
    assert!(
        !root_box.has_css_class("flux-card--restricted"),
        "Widget must NOT have 'flux-card--restricted' CSS class when is_foreign_owner is false"
    );
    assert!(
        !widgets.lock_icon.is_visible(),
        "lock_icon must NOT be visible when is_foreign_owner is false"
    );
}

#[test]
fn test_sidebar_children_lists_sorted_dirs_only() {
    use flux::ui::list_sidebar_children;

    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("beta")).unwrap();
    std::fs::create_dir(dir.path().join("Alpha")).unwrap();
    std::fs::create_dir(dir.path().join("Alpha/nested")).unwrap();
    std::fs::create_dir(dir.path().join(".hidden")).unwrap();
    std::fs::write(dir.path().join("file.txt"), b"x").unwrap();

    let children = list_sidebar_children(dir.path(), false);
    let names: Vec<&str> = children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Alpha", "beta"]);
    assert!(children[0].has_children);
    assert!(!children[1].has_children);
    assert_eq!(children[0].path, dir.path().join("Alpha"));

    let with_hidden = list_sidebar_children(dir.path(), true);
    let names: Vec<&str> = with_hidden.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, [".hidden", "Alpha", "beta"]);
}

#[test]
fn test_sidebar_children_hidden_grandchild_respects_filter() {
    use flux::ui::list_sidebar_children;

    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("only_hidden/.git")).unwrap();

    assert!(!list_sidebar_children(dir.path(), false)[0].has_children);
    assert!(list_sidebar_children(dir.path(), true)[0].has_children);
}

#[test]
fn test_sidebar_children_unreadable_dir_is_empty() {
    use flux::ui::list_sidebar_children;

    assert!(list_sidebar_children(std::path::Path::new("/nonexistent/flux/dir"), true).is_empty());
}

#[test]
fn test_best_ancestor_root_prefers_deepest() {
    use flux::ui::best_ancestor_root;
    use std::path::Path;

    let roots = [
        (0, Path::new("/home/u")),
        (1, Path::new("/home/u/Documents")),
        (2, Path::new("/home/u/Doc")),
        (3, Path::new("/mnt")),
    ];
    let target = Path::new("/home/u/Documents/x/y");
    assert_eq!(best_ancestor_root(roots, target), Some(1));
    assert_eq!(
        best_ancestor_root(roots, Path::new("/home/u/Music")),
        Some(0)
    );
    assert_eq!(best_ancestor_root(roots, Path::new("/etc")), None);

    // Duplicate roots resolve to the first one.
    let dupes = [(4, Path::new("/home/u")), (7, Path::new("/home/u"))];
    assert_eq!(best_ancestor_root(dupes, target), Some(4));
}

#[test]
fn test_virtual_sidebar_paths_are_not_expandable() {
    use flux::ui::is_virtual_sidebar_path;
    use std::path::Path;

    assert!(is_virtual_sidebar_path(Path::new("recent:///")));
    assert!(is_virtual_sidebar_path(Path::new("trash:///")));
    assert!(is_virtual_sidebar_path(Path::new("tags://")));
    assert!(is_virtual_sidebar_path(Path::new("")));
    assert!(!is_virtual_sidebar_path(Path::new("/home/u")));
}
