//! Order-preserving projection and traversal for nested asset-browser folders.
//!
//! This module owns hierarchy shape and traversal only. Consumers retain collapse state,
//! folder presentation, item rendering, and domain actions.

/// Maximum folder depth rendered by the shared browser. This matches the preset import/storage
/// limit while keeping recursive projection and traversal bounded for malformed external data.
pub(crate) const MAX_HIERARCHY_DEPTH: usize = 32;

/// An item or nested folder in the projected hierarchy.
#[derive(Debug)]
pub enum HierarchyNode<'a, T> {
    /// An item that belongs directly at this level of the hierarchy.
    Preset(&'a T),
    /// A nested folder, kept in the position of its first descendant.
    Folder(HierarchyFolder<'a, T>),
}

/// A folder node in the shared, order-preserving hierarchy projection.
#[derive(Debug)]
pub struct HierarchyFolder<'a, T> {
    /// Full path from the item's group root to this folder.
    pub path: Vec<String>,
    /// Items and subfolders in their original library order.
    pub children: Vec<HierarchyNode<'a, T>>,
    /// Number of items in this folder subtree, including nested folders.
    pub count: usize,
}

/// Project ordered items and their folder paths into a hierarchy, truncating paths at `max_depth`.
/// Paths deeper than `max_depth` are truncated before grouping, so the view cannot recurse beyond
/// the consumer's supported nesting limit. Item references remain borrowed from the source library.
pub(crate) fn hierarchy_tree<'a, T>(items: Vec<(&'a [String], &'a T)>, max_depth: usize) -> Vec<HierarchyNode<'a, T>> {
    let bounded = items.into_iter().map(|(path, item)| (&path[..path.len().min(max_depth)], item)).collect();
    hierarchy_level(bounded, &[])
}

/// Shared ordered traversal for nested hierarchy views. Consumers provide their item-run renderer,
/// folder presentation/interaction, and collapse policy; traversal owns recursive sequencing.
/// Adjacent presets are batched into one `draw_items` call and flushed before each folder header;
/// children are traversed only when `is_open` returns true and `max_depth` has not been reached.
type HierarchyOpen<'a, 'b, T> = dyn FnMut(&HierarchyFolder<'a, T>) -> bool + 'b;
type HierarchyDrawFolder<'a, 'b, T> = dyn FnMut(&mut egui::Ui, &HierarchyFolder<'a, T>, bool, usize) + 'b;
type HierarchyDrawItems<'a, 'b, T> = dyn FnMut(&mut egui::Ui, &[&'a T], usize) + 'b;

pub(crate) fn draw_hierarchy<'a, 'b, T>(
    ui: &mut egui::Ui,
    nodes: &[HierarchyNode<'a, T>],
    depth: usize,
    max_depth: usize,
    is_open: &mut HierarchyOpen<'a, 'b, T>,
    draw_folder: &mut HierarchyDrawFolder<'a, 'b, T>,
    draw_items: &mut HierarchyDrawItems<'a, 'b, T>,
) {
    let mut run: Vec<&'a T> = Vec::new();
    let flush = |ui: &mut egui::Ui, run: &mut Vec<&'a T>, draw_items: &mut dyn FnMut(&mut egui::Ui, &[&'a T], usize)| {
        if !run.is_empty() {
            draw_items(ui, run, depth);
            run.clear();
        }
    };
    for node in nodes {
        match node {
            HierarchyNode::Preset(item) => run.push(*item),
            HierarchyNode::Folder(folder) => {
                flush(ui, &mut run, draw_items);
                let open = is_open(folder);
                draw_folder(ui, folder, open, depth);
                if open && depth < max_depth {
                    draw_hierarchy(ui, &folder.children, depth + 1, max_depth, is_open, draw_folder, draw_items);
                }
            }
        }
    }
    flush(ui, &mut run, draw_items);
}

type HierarchyEntry<'a, T> = (&'a [String], &'a T);

/// Build one level of the tree while retaining the first-seen order of items and folders.
fn hierarchy_level<'a, T>(items: Vec<HierarchyEntry<'a, T>>, prefix: &[String]) -> Vec<HierarchyNode<'a, T>> {
    enum Slot {
        Item(usize),
        Folder(usize),
    }
    let mut slots = Vec::new();
    let mut folders: Vec<(&String, Vec<HierarchyEntry<'a, T>>)> = Vec::new();
    for (i, (path, item)) in items.iter().enumerate() {
        match path.split_first() {
            None => slots.push(Slot::Item(i)),
            Some((name, rest)) => match folders.iter_mut().position(|(n, _)| *n == name) {
                Some(k) => {
                    if let Some((_, v)) = folders.get_mut(k) {
                        v.push((rest, *item));
                    }
                }
                None => {
                    slots.push(Slot::Folder(folders.len()));
                    folders.push((name, vec![(rest, *item)]));
                }
            },
        }
    }
    type NamedFolderEntries<'a, T> = (&'a String, Vec<HierarchyEntry<'a, T>>);
    let mut folders: Vec<Option<NamedFolderEntries<'a, T>>> = folders.into_iter().map(Some).collect();
    slots
        .into_iter()
        .filter_map(|slot| match slot {
            Slot::Item(i) => items.get(i).map(|(_, item)| HierarchyNode::Preset(*item)),
            Slot::Folder(i) => {
                let (name, sub) = folders.get_mut(i)?.take()?;
                let path: Vec<String> = prefix.iter().cloned().chain(std::iter::once(name.clone())).collect();
                let count = sub.len();
                let children = hierarchy_level(sub, &path);
                Some(HierarchyNode::Folder(HierarchyFolder { path, children, count }))
            }
        })
        .collect()
}
