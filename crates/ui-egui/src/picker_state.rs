//! Persistent and transient state shared by preset pickers and floating preset panels.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(crate) const MAX_GROUP_DEPTH: usize = 32;

/// Transient rename target for a nested folder. Kept separate from the legacy string-keyed rename
/// state so arbitrary group and folder labels remain lossless.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct FolderRenameState {
    pub(crate) panel: String,
    pub(crate) group: String,
    pub(crate) folder: Vec<String>,
    pub(crate) text: String,
}

/// Panel state (serde, so the control channel can read and drive it).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PresetUi {
    /// Floating panels.
    #[serde(default)]
    pub gradients: bool,
    #[serde(default)]
    pub swatches: bool,
    #[serde(default)]
    pub patterns: bool,
    pub styles: bool,
    pub shapes: bool,
    pub tool_presets: bool,
    pub clone_source: bool,
    /// Thumbnail size per panel (px; the minimum shows a list).
    pub sizes: BTreeMap<String, f32>,
    /// Folders whose open state differs from the default (only the first folder starts open).
    pub toggled: BTreeSet<String>,
    /// Selected preset per panel (by name / pattern id).
    pub selected: BTreeMap<String, String>,
    /// Tool Presets: Current Tool Only.
    pub current_tool_only: bool,
    /// Custom Shape tool: the shape it draws.
    pub custom_shape: String,
    /// Folder/group order per panel, by name (#3658), like the dock's `tabOrder` (#2272). A panel
    /// with no entry keeps the library's order; unknown names are ignored and groups missing from
    /// a list follow it in library order. Saved with the UI state.
    #[serde(rename = "groupOrder", alias = "group_order", deserialize_with = "lenient_group_order", skip_serializing_if = "BTreeMap::is_empty")]
    pub group_order: BTreeMap<String, Vec<String>>,
    /// Parent group for nested asset groups, scoped per picker. Expansion stays keyed by group name,
    /// so moving a group under a collapsed parent never resets the child's own open state.
    #[serde(rename = "groupParent", alias = "group_parent", default, skip_serializing_if = "BTreeMap::is_empty")]
    pub group_parent: BTreeMap<String, BTreeMap<String, String>>,
    /// Visibility of the search row per picker. Missing entries default to visible.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub search_visible: BTreeMap<String, bool>,
    /// Case-insensitive asset-name filter per picker.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub search: BTreeMap<String, String>,
    /// In-place rename: (panel, preset or "group:<name>", text).
    #[serde(skip)]
    pub renaming: Option<(String, String, String)>,
    /// Typed in-place nested-folder rename. Transient and intentionally excluded from serde.
    #[serde(skip)]
    pub(crate) folder_renaming: Option<FolderRenameState>,
}

/// Read `groupOrder` keeping what makes sense: non-object values, non-list entries and non-string
/// names are dropped instead of failing the whole saved UI state.
const MAX_GROUP_ORDER: usize = 10_000;
fn lenient_group_order<'de, D: serde::Deserializer<'de>>(d: D) -> Result<BTreeMap<String, Vec<String>>, D::Error> {
    let v = Value::deserialize(d)?;
    let mut out = BTreeMap::new();
    for (panel, names) in v.as_object().into_iter().flatten() {
        let Some(names) = names.as_array() else { continue };
        out.insert(panel.clone(), names.iter().filter_map(Value::as_str).take(MAX_GROUP_ORDER).map(str::to_owned).collect());
    }
    Ok(out)
}

impl PresetUi {
    pub fn shape(&self) -> &str {
        if self.custom_shape.is_empty() { tl!("Heart") } else { &self.custom_shape }
    }

    /// Display order of the folders for `panel`, as library indices: the stored order first
    /// (names that no longer exist are dropped), then groups missing from it in library order.
    /// Mirrors the dock's per-group tab order (#2272, #3658).
    pub(crate) fn display_order(&self, panel: &str, names: &[String]) -> Vec<usize> {
        let mut out: Vec<usize> = Vec::with_capacity(names.len());
        // Build this once: saved order and library names can both contain thousands of entries.
        // Repeated `position`/`contains` scans made this path quadratic.
        let mut indices: BTreeMap<&str, usize> = BTreeMap::new();
        for (i, name) in names.iter().enumerate() {
            indices.entry(name.as_str()).or_insert(i);
        }
        let mut included = vec![false; names.len()];
        for name in self.group_order.get(panel).into_iter().flatten() {
            if let Some(&i) = indices.get(name.as_str())
                && !included[i]
            {
                included[i] = true;
                out.push(i);
            }
        }
        for (i, is_included) in included.iter().enumerate() {
            if !is_included {
                out.push(i);
            }
        }
        out
    }

    /// Move folder `from` so it is drawn before `before` (`None` = last) for `panel`. The
    /// library's own order is stored as no entry, as the dock does for its tabs.
    pub(crate) fn move_group(&mut self, panel: &str, names: &[String], from: &str, before: Option<&str>) {
        if before == Some(from) {
            return;
        }
        let mut order: Vec<String> = self.display_order(panel, names).into_iter().filter_map(|i| names.get(i).cloned()).collect();
        let Some(at) = order.iter().position(|n| n == from) else { return };
        let moved = order.remove(at);
        let to = before.and_then(|b| order.iter().position(|n| n == b)).unwrap_or(order.len());
        order.insert(to, moved);
        if order.as_slice() == names {
            self.group_order.remove(panel);
        } else {
            self.group_order.insert(panel.to_string(), order);
        }
    }

    /// Make `child` a child of `parent`, or a root group when `parent` is `None`. Reject cycles and
    /// stale names; expansion keys intentionally remain unchanged across the move.
    pub(crate) fn nest_group(&mut self, panel: &str, names: &[String], child: &str, parent: Option<&str>) {
        if !names.iter().any(|n| n == child) || parent.is_some_and(|p| !names.iter().any(|n| n == p) || p == child) {
            return;
        }
        let parents = self.group_parent.entry(panel.to_string()).or_default();
        let mut ancestor = parent;
        while let Some(name) = ancestor {
            if name == child {
                return;
            }
            ancestor = parents.get(name).map(String::as_str);
        }
        match parent {
            Some(parent) => {
                parents.insert(child.to_string(), parent.to_string());
            }
            None => {
                parents.remove(child);
            }
        }
        if parents.is_empty() {
            self.group_parent.remove(panel);
        }
    }

    /// Keep view-only hierarchy and expansion state attached to a group when its asset-library
    /// name changes. The caller invokes this only after the asset rename command succeeds.
    pub(crate) fn rename_group(&mut self, panel: &str, from: &str, to: &str) {
        if from == to {
            return;
        }
        if let Some(parents) = self.group_parent.get_mut(panel) {
            if let Some(parent) = parents.remove(from) {
                parents.insert(to.to_string(), parent);
            }
            for parent in parents.values_mut() {
                if parent == from {
                    *parent = to.to_string();
                }
            }
            if parents.is_empty() {
                self.group_parent.remove(panel);
            }
        }
        if let Some(order) = self.group_order.get_mut(panel) {
            for name in order {
                if name == from {
                    *name = to.to_string();
                }
            }
        }
        let old_prefix = format!("{panel}/{}", escape_hierarchy_key_part(from));
        let new_prefix = format!("{panel}/{}", escape_hierarchy_key_part(to));
        let renamed: Vec<String> = self
            .toggled
            .iter()
            .filter_map(|key| key.strip_prefix(&old_prefix).filter(|tail| tail.is_empty() || tail.starts_with('/')).map(|tail| format!("{new_prefix}{tail}")))
            .collect();
        self.toggled.retain(|key| !key.strip_prefix(&old_prefix).is_some_and(|tail| tail.is_empty() || tail.starts_with('/')));
        self.toggled.extend(renamed);
    }

    /// Detach children of a removed group and discard its ordering/expansion entries.
    pub(crate) fn delete_group(&mut self, panel: &str, name: &str) {
        if let Some(parents) = self.group_parent.get_mut(panel) {
            parents.remove(name);
            parents.retain(|_, parent| parent != name);
            if parents.is_empty() {
                self.group_parent.remove(panel);
            }
        }
        if let Some(order) = self.group_order.get_mut(panel) {
            order.retain(|group| group != name);
            if order.is_empty() {
                self.group_order.remove(panel);
            }
        }
        let prefix = format!("{panel}/{}", escape_hierarchy_key_part(name));
        self.toggled.retain(|key| !key.strip_prefix(&prefix).is_some_and(|tail| tail.is_empty() || tail.starts_with('/')));
    }

    /// Apply the Brushes panel's header-drop convention to asset groups: the upper third inserts
    /// before the target at its sibling level; the remainder nests into the target. A root group
    /// dropped onto a nested header's edge becomes a sibling of that nested group, not a root.
    pub(crate) fn drop_group_on_header(&mut self, panel: &str, names: &[String], child: &str, target: &str, edge: bool) {
        if child == target || !names.iter().any(|n| n == child) || !names.iter().any(|n| n == target) {
            return;
        }
        if !edge {
            self.nest_group(panel, names, child, Some(target));
            return;
        }
        let target_parent = self.group_parent.get(panel).and_then(|parents| parents.get(target)).cloned();
        let mut ancestor = target_parent.as_deref();
        while let Some(name) = ancestor {
            if name == child {
                return;
            }
            ancestor = self.group_parent.get(panel).and_then(|parents| parents.get(name)).map(String::as_str);
        }
        self.nest_group(panel, names, child, target_parent.as_deref());
        self.move_group(panel, names, child, Some(target));
    }

    /// Depth-first display order. Sibling order follows the existing per-panel order and corrupt
    /// cycles/stale parent references safely fall back to root-level entries.
    pub(crate) fn nested_display_order(&self, panel: &str, names: &[String]) -> Vec<(usize, usize)> {
        let ordered = self.display_order(panel, names);
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        let valid_names: BTreeSet<&str> = names.iter().map(String::as_str).collect();
        let parents = self.group_parent.get(panel);
        // Bucket children by parent in display order. The old recursive implementation scanned
        // the complete ordered list for every visited group, quadratic for a deep/large hierarchy.
        let mut children: BTreeMap<Option<&str>, Vec<usize>> = BTreeMap::new();
        for &i in &ordered {
            let Some(name) = names.get(i) else { continue };
            let parent = parents.and_then(|m| m.get(name)).map(String::as_str).filter(|p| valid_names.contains(p));
            children.entry(parent).or_default().push(i);
        }
        fn visit(
            parent: Option<&str>,
            depth: usize,
            names: &[String],
            children: &BTreeMap<Option<&str>, Vec<usize>>,
            seen: &mut BTreeSet<String>,
            out: &mut Vec<(usize, usize)>,
        ) {
            let Some(siblings) = children.get(&parent) else { return };
            for &i in siblings {
                let Some(name) = names.get(i) else { continue };
                if !seen.insert(name.clone()) {
                    continue;
                }
                out.push((i, depth));
                if depth < MAX_GROUP_DEPTH {
                    visit(Some(name), depth + 1, names, children, seen, out);
                }
            }
        }
        visit(None, 0, names, &children, &mut seen, &mut out);
        // Any malformed cycle is detached for display rather than disappearing.
        for &i in &ordered {
            if let Some(name) = names.get(i)
                && !seen.contains(name)
            {
                out.push((i, 0));
            }
        }
        out
    }
}

/// Escape one path component in persisted picker expansion keys. `%` is escaped first so the
/// encoding is reversible; `/` can then safely remain the component separator.
pub(crate) fn escape_hierarchy_key_part(part: &str) -> String {
    part.replace('%', "%25").replace('/', "%2F")
}

#[cfg(test)]
mod tests {
    use super::PresetUi;

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn display_order_keeps_saved_order_and_appends_unlisted_names() {
        let mut state = PresetUi::default();
        state.group_order.insert("panel".into(), names(&["gone", "third", "third", "first"]));

        assert_eq!(state.display_order("panel", &names(&["first", "second", "third"])), vec![2, 0, 1]);
    }

    #[test]
    fn nested_display_order_keeps_sibling_order_and_detaches_cycles() {
        let mut state = PresetUi::default();
        let names = names(&["root-b", "child", "root-a", "cycle-a", "cycle-b"]);
        state.group_order.insert("panel".into(), names.clone());
        state.group_parent.insert(
            "panel".into(),
            [("child".into(), "root-a".into()), ("cycle-a".into(), "cycle-b".into()), ("cycle-b".into(), "cycle-a".into())].into_iter().collect(),
        );

        assert_eq!(state.nested_display_order("panel", &names), vec![(0, 0), (2, 0), (1, 1), (3, 0), (4, 0)]);
    }
}
