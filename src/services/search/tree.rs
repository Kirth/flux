//! Lays out recursive search results as a tree: each match under the folders it's in,
//! files before subfolders, and chains of folders that only lead to one subfolder
//! merged into one row (`Computer/INZONE/decompiled/resources`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One search result, relative to the searched folder.
pub struct TreeEntry {
    pub rel: PathBuf,
    pub is_dir: bool,
}

/// One row of the laid-out tree.
#[derive(Debug, PartialEq, Eq)]
pub struct TreeRow {
    /// Shown name: a file name, or a folder path when a chain was merged (`a/b/c`).
    pub label: String,
    /// The row's path, relative to the searched folder.
    pub rel: PathBuf,
    pub depth: u32,
    /// Index into the entries for a search result; `None` for a folder row that's
    /// only there to hold results.
    pub result: Option<usize>,
}

#[derive(Default)]
struct Node {
    /// Keyed by lowercase name for case-insensitive order; value keeps the real name.
    dirs: BTreeMap<String, (String, Node)>,
    files: Vec<(String, String, usize)>,
    matched: Option<usize>,
}

fn name_of(c: std::path::Component) -> String {
    c.as_os_str().to_string_lossy().into_owned()
}

pub fn build_tree(entries: &[TreeEntry]) -> Vec<TreeRow> {
    let mut root = Node::default();
    for (idx, entry) in entries.iter().enumerate() {
        let mut components: Vec<String> = entry.rel.components().map(name_of).collect();
        let file = if entry.is_dir { None } else { components.pop() };
        let mut node = &mut root;
        for name in components {
            node = &mut node
                .dirs
                .entry(name.to_lowercase())
                .or_insert_with(|| (name, Node::default()))
                .1;
        }
        match file {
            Some(name) => node.files.push((name.to_lowercase(), name, idx)),
            None => node.matched = Some(idx),
        }
    }
    let mut rows = Vec::new();
    emit(&mut root, Path::new(""), 0, &mut rows);
    rows
}

fn emit(node: &mut Node, rel: &Path, depth: u32, rows: &mut Vec<TreeRow>) {
    node.files.sort();
    for (_, name, idx) in &node.files {
        rows.push(TreeRow {
            label: name.clone(),
            rel: rel.join(name),
            depth,
            result: Some(*idx),
        });
    }
    for (name, child) in node.dirs.values_mut() {
        let mut label = name.clone();
        let mut path = rel.join(&*name);
        let mut cur = child;
        // A folder that isn't a result, holds no files and leads to just one
        // subfolder is merged into that subfolder's row.
        while cur.matched.is_none() && cur.files.is_empty() && cur.dirs.len() == 1 {
            let (next_name, next) = cur.dirs.values_mut().next().expect("one subfolder");
            label = format!("{label}/{next_name}");
            path = path.join(&*next_name);
            cur = next;
        }
        rows.push(TreeRow {
            label,
            rel: path.clone(),
            depth,
            result: cur.matched,
        });
        emit(cur, &path, depth + 1, rows);
    }
}
