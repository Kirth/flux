//! Explorer-style expansion of sidebar rows into their subfolders.
//!
//! The tree is kept flattened inside the sidebar factory: a row's subfolders are the
//! contiguous run of rows that directly follow it with a greater `depth`.

use crate::model::{AppMsg, FluxApp};
use crate::ui::{
    best_ancestor_root, is_virtual_sidebar_path, list_sidebar_children, SidebarChild,
    SidebarPlaceInit,
};
use adw::prelude::*;
use relm4::factory::DynamicIndex;
use std::path::PathBuf;

impl FluxApp {
    /// Returns the index just past the subtree rooted at row `i`.
    fn sidebar_subtree_end(&self, i: usize) -> usize {
        let Some(depth) = self.sidebar.get(i).map(|p| p.depth) else {
            return i;
        };
        let mut end = i + 1;
        while self.sidebar.get(end).is_some_and(|p| p.depth > depth) {
            end += 1;
        }
        end
    }

    /// Expands or collapses the row the chevron was clicked on.
    pub fn handle_sidebar_toggle_expand(&mut self, index: DynamicIndex) {
        let i = index.current_index();
        match self.sidebar.get(i) {
            Some(p) if p.expanded => self.collapse_sidebar_row(i),
            Some(p) if p.is_expandable() => self.expand_sidebar_row(i),
            _ => {}
        }
    }

    /// Unfolds the row that was clicked to navigate, leaving it open if it already is.
    pub fn handle_sidebar_expand(&mut self, index: DynamicIndex) {
        let i = index.current_index();
        if self
            .sidebar
            .get(i)
            .is_some_and(|p| !p.expanded && p.is_expandable())
        {
            self.expand_sidebar_row(i);
        }
    }

    fn expand_sidebar_row(&mut self, i: usize) {
        let Some(place) = self.sidebar.guard().get_mut(i).map(|p| {
            p.expanded = true;
            (p.index.clone(), p.path.clone())
        }) else {
            return;
        };
        self.sidebar_expanded.insert(place.1.clone());
        self.load_sidebar_children(place.0, place.1);
    }

    fn collapse_sidebar_row(&mut self, i: usize) {
        let end = self.sidebar_subtree_end(i);
        let mut guard = self.sidebar.guard();
        for j in (i + 1..end).rev() {
            guard.remove(j);
        }
        if let Some(p) = guard.get_mut(i) {
            p.expanded = false;
            self.sidebar_expanded.remove(&p.path);
            if self
                .sidebar_reveal_target
                .as_ref()
                .is_some_and(|t| t.starts_with(&p.path))
            {
                self.sidebar_reveal_target = None;
            }
        }
    }

    /// Enumerates the subfolders of `path` off the UI thread.
    fn load_sidebar_children(&self, index: DynamicIndex, path: PathBuf) {
        let Some(sender) = crate::model::SENDER.get().cloned() else {
            return;
        };
        let show_hidden = self.show_hidden;
        relm4::spawn_blocking(move || {
            let children = list_sidebar_children(&path, show_hidden);
            let _ = sender.send(AppMsg::SidebarChildrenLoaded {
                index,
                parent: path,
                children,
            });
        });
    }

    /// Re-expands every root row the user had expanded before a rebuild.
    pub(crate) fn restore_sidebar_expansion(&mut self, from: usize) {
        let mut to_load = Vec::new();
        {
            let mut guard = self.sidebar.guard();
            for i in from..guard.len() {
                let Some(p) = guard.get_mut(i) else { break };
                if p.depth == 0 && p.is_expandable() && self.sidebar_expanded.contains(&p.path) {
                    p.expanded = true;
                    to_load.push((p.index.clone(), p.path.clone()));
                }
            }
        }
        for (index, path) in to_load {
            self.load_sidebar_children(index, path);
        }
    }

    /// Inserts freshly enumerated subfolders under their parent row.
    pub fn handle_sidebar_children_loaded(
        &mut self,
        index: DynamicIndex,
        parent: PathBuf,
        children: Vec<SidebarChild>,
    ) {
        let i = index.current_index();
        let depth = match self.sidebar.get(i) {
            Some(p) if p.path == parent && p.expanded => p.depth,
            _ => return,
        };
        let end = self.sidebar_subtree_end(i);

        let mut to_load = Vec::new();
        {
            let mut guard = self.sidebar.guard();
            for j in (i + 1..end).rev() {
                guard.remove(j);
            }
            if let Some(p) = guard.get_mut(i) {
                p.has_children = Some(!children.is_empty());
                p.expanded = !children.is_empty();
            }
            for (k, child) in children.into_iter().enumerate() {
                let at = i + 1 + k;
                let expand = child.has_children && self.sidebar_expanded.contains(&child.path);
                let child_index = guard.insert(
                    at,
                    SidebarPlaceInit {
                        name: child.name,
                        icon: "folder-symbolic".to_string(),
                        path: child.path.clone(),
                        depth: depth + 1,
                        is_tree_child: true,
                        has_children: Some(child.has_children),
                        ..Default::default()
                    },
                );
                if expand {
                    if let Some(p) = guard.get_mut(at) {
                        p.expanded = true;
                    }
                    to_load.push((child_index, child.path));
                }
            }
        }

        for (index, path) in to_load {
            self.load_sidebar_children(index, path);
        }
        self.advance_sidebar_reveal();
    }

    fn sidebar_listbox(&self) -> Option<gtk::ListBox> {
        self.sidebar
            .widget()
            .downcast_ref::<gtk::ListBox>()
            .cloned()
    }

    fn select_sidebar_row(&self, i: Option<usize>) {
        let Some(listbox) = self.sidebar_listbox() else {
            return;
        };
        match i.and_then(|i| listbox.row_at_index(i as i32)) {
            Some(row) => listbox.select_row(Some(&row)),
            None => listbox.unselect_all(),
        }
    }

    /// Highlights the row for `current_path`, expanding the tree towards it when it
    /// lives below one of the sidebar's root places.
    pub fn sync_sidebar_selection(&mut self) {
        self.sidebar_reveal_target = None;
        let current = self.current_path.clone();
        let canonical = current.canonicalize().ok();

        let is_dir_root = |p: &crate::ui::SidebarPlace| {
            p.depth == 0 && !p.is_section_label && !is_virtual_sidebar_path(&p.path)
        };

        // Exact match: prefer the configured places, then any already-expanded subfolder.
        let root_match = self.sidebar.iter().position(|p| {
            !p.is_section_label
                && p.depth == 0
                && (p.path == current
                    || canonical
                        .as_ref()
                        .is_some_and(|c| p.path.canonicalize().ok().as_ref() == Some(c)))
        });
        let exact = root_match.or_else(|| self.sidebar.iter().position(|p| p.path == current));
        if exact.is_some() {
            self.select_sidebar_row(exact);
            return;
        }

        self.select_sidebar_row(None);
        let Some(canonical) = canonical else {
            return;
        };

        let roots: Vec<(usize, PathBuf)> = self
            .sidebar
            .iter()
            .enumerate()
            .filter(|(_, p)| is_dir_root(p))
            .filter_map(|(i, p)| p.path.canonicalize().ok().map(|c| (i, c)))
            .collect();
        let Some(root) =
            best_ancestor_root(roots.iter().map(|(i, p)| (*i, p.as_path())), &canonical)
        else {
            return;
        };
        let root_canonical = &roots.iter().find(|(i, _)| *i == root).unwrap().1;
        let Ok(rel) = canonical.strip_prefix(root_canonical) else {
            return;
        };
        let Some(root_path) = self.sidebar.get(root).map(|p| p.path.clone()) else {
            return;
        };

        // Express the target under the root's own (possibly symlinked) path, since
        // child rows are built by joining names onto it.
        self.sidebar_reveal_target = Some(root_path.join(rel));
        self.advance_sidebar_reveal();
    }

    /// Walks the tree towards `sidebar_reveal_target`, expanding the next unexpanded
    /// ancestor. Continues from `handle_sidebar_children_loaded` once its children arrive.
    fn advance_sidebar_reveal(&mut self) {
        let Some(target) = self.sidebar_reveal_target.clone() else {
            return;
        };

        let roots = self
            .sidebar
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                p.depth == 0 && !p.is_section_label && !is_virtual_sidebar_path(&p.path)
            })
            .map(|(i, p)| (i, p.path.as_path()));
        let Some(mut i) = best_ancestor_root(roots, &target) else {
            self.sidebar_reveal_target = None;
            return;
        };

        loop {
            let Some(place) = self.sidebar.get(i) else {
                return;
            };
            if place.path == target {
                self.sidebar_reveal_target = None;
                self.select_sidebar_row(Some(i));
                return;
            }
            if !place.expanded {
                if place.is_expandable() {
                    self.expand_sidebar_row(i);
                } else {
                    self.sidebar_reveal_target = None;
                }
                return;
            }
            let depth = place.depth;
            let end = self.sidebar_subtree_end(i);
            match (i + 1..end).find(|&j| {
                self.sidebar
                    .get(j)
                    .is_some_and(|c| c.depth == depth + 1 && target.starts_with(&c.path))
            }) {
                Some(j) => i = j,
                // Children are still loading (or the folder is filtered out, e.g. hidden).
                None => return,
            }
        }
    }

    /// Left/Right arrow handling on the sidebar: collapse/expand or move to parent/child.
    pub fn handle_sidebar_key_nav(&mut self, expand: bool) {
        let Some(listbox) = self.sidebar_listbox() else {
            return;
        };
        let Some(i) = listbox.selected_row().map(|r| r.index() as usize) else {
            return;
        };
        let Some(place) = self.sidebar.get(i) else {
            return;
        };
        let depth = place.depth;

        if expand {
            if !place.expanded {
                if place.is_expandable() {
                    self.expand_sidebar_row(i);
                }
            } else if self.sidebar.get(i + 1).is_some_and(|c| c.depth > depth) {
                self.select_sidebar_row(Some(i + 1));
            }
        } else if place.expanded {
            self.collapse_sidebar_row(i);
        } else if depth > 0 {
            let parent = (0..i)
                .rev()
                .find(|&j| self.sidebar.get(j).is_some_and(|p| p.depth < depth));
            self.select_sidebar_row(parent);
        }
    }
}
