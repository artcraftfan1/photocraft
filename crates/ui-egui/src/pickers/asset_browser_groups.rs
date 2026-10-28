//! Shared group and folder header presentation for asset browsers.
//!
//! The browser owns expansion state and hierarchy traversal; brush and preset consumers share
//! this header's hit target and visual treatment while retaining their own group actions.

use egui::{Align2, Rect, Sense, pos2, vec2};

use crate::pickers::picker_state::PresetUi;
use crate::theme::Tokens;

/// Initial expansion policy for a browser group, with explicit user toggles overriding the default.
pub(crate) fn group_open(st: &PresetUi, panel: &str, index: usize, name: &str, all_groups_open: bool) -> bool {
    (index == 0 || all_groups_open) != st.toggled.contains(&format!("{panel}/{}", crate::pickers::picker_state::escape_hierarchy_key_part(name)))
}

/// Shared collapsible group/folder row. The caller owns toggle state and context actions.
pub(crate) fn header(ui: &mut egui::Ui, name: &str, open: bool, depth: usize, count: Option<usize>, height: f32) -> (Rect, egui::Response) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click_and_drag());
    paint(ui, rect, open, name, depth, response.hovered() || response.dragged(), count);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::CollapsingHeader, true, name));
    (rect, response)
}

/// Paint a group/folder header; nesting is conveyed by indentation rather than an accent bar.
pub(crate) fn paint(ui: &egui::Ui, rect: Rect, open: bool, name: &str, depth: usize, hovered: bool, count: Option<usize>) {
    let tokens = Tokens::get(ui.ctx());
    let indent = 14.0 * depth.min(4) as f32;
    ui.painter().rect_filled(rect, tokens.radius_sm, if hovered { tokens.hover } else { tokens.field });
    let x = rect.left() + 6.0 + indent;
    crate::icons::paint(
        ui,
        Rect::from_center_size(pos2(x + 6.0, rect.center().y), vec2(12.0, 12.0)),
        if open { "chevron-down" } else { "chevron-right" },
        10.0,
        tokens.text_dim,
    );
    crate::icons::paint(
        ui,
        Rect::from_center_size(pos2(x + 24.0, rect.center().y), vec2(14.0, 14.0)),
        if open { "folder-open" } else { "folder" },
        12.0,
        tokens.icon,
    );
    ui.painter().text(
        pos2(x + 36.0, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        if depth == 0 { crate::theme::semibold(11.5) } else { crate::theme::medium(11.5) },
        tokens.text,
    );
    if let Some(count) = count {
        ui.painter().text(pos2(rect.right() - 8.0, rect.center().y), Align2::RIGHT_CENTER, count.to_string(), crate::theme::medium(11.0), tokens.text_faint);
    }
}
