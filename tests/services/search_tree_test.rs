use flux::services::search::{build_tree, TreeEntry};
use std::path::PathBuf;

fn layout(entries: &[(&str, bool)]) -> Vec<(String, u32, bool)> {
    let entries: Vec<TreeEntry> = entries
        .iter()
        .map(|(p, is_dir)| TreeEntry {
            rel: PathBuf::from(p),
            is_dir: *is_dir,
        })
        .collect();
    build_tree(&entries)
        .into_iter()
        .map(|r| (r.label, r.depth, r.result.is_some()))
        .collect()
}

fn row(label: &str, depth: u32, result: bool) -> (String, u32, bool) {
    (label.to_string(), depth, result)
}

#[test]
fn single_folder_chains_merge_into_one_row() {
    let rows = layout(&[
        ("Computer/INZONE/decompiled/resources/a.png", false),
        ("Computer/INZONE/decompiled/resources/b.png", false),
    ]);
    assert_eq!(
        rows,
        vec![
            row("Computer/INZONE/decompiled/resources", 0, false),
            row("a.png", 1, true),
            row("b.png", 1, true),
        ]
    );
}

#[test]
fn chains_split_where_they_branch() {
    let rows = layout(&[
        ("a/b/c/x.png", false),
        ("a/b/d/y.png", false),
        ("a/b/z.png", false),
    ]);
    assert_eq!(
        rows,
        vec![
            row("a/b", 0, false),
            row("z.png", 1, true),
            row("c", 1, false),
            row("x.png", 2, true),
            row("d", 1, false),
            row("y.png", 2, true),
        ]
    );
}

#[test]
fn files_come_first_and_order_ignores_case() {
    let rows = layout(&[
        ("beta/1.png", false),
        ("Alpha/2.png", false),
        ("loose.png", false),
    ]);
    assert_eq!(
        rows,
        vec![
            row("loose.png", 0, true),
            row("Alpha", 0, false),
            row("2.png", 1, true),
            row("beta", 0, false),
            row("1.png", 1, true),
        ]
    );
}

#[test]
fn a_matching_folder_stops_the_merge_and_is_a_result() {
    let rows = layout(&[("a/b", true), ("a/b/c/x.png", false)]);
    assert_eq!(
        rows,
        vec![
            row("a/b", 0, true),
            row("c", 1, false),
            row("x.png", 2, true),
        ]
    );
}

#[test]
fn similar_names_stay_separate() {
    let rows = layout(&[("a/b/x.png", false), ("a/b c/y.png", false)]);
    assert_eq!(
        rows,
        vec![
            row("a", 0, false),
            row("b", 1, false),
            row("x.png", 2, true),
            row("b c", 1, false),
            row("y.png", 2, true),
        ]
    );
}
