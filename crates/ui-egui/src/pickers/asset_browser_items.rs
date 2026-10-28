//! Shared item-row presentation and accessibility for asset browsers.
//!
//! Asset adapters paint their own previews; this module keeps list feedback and accessibility
//! metadata consistent without coupling those previews to the shared browser's input handling.

use egui::Rect;

use crate::theme::Tokens;

/// Attach selection semantics for screen readers to a custom item renderer.
pub(crate) fn item_info(response: &egui::Response, label: &str, selected: bool) {
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, label));
}

/// Paint shared selected/hovered feedback for a list row.
pub(crate) fn list_row(ui: &egui::Ui, response: &egui::Response, rect: Rect, label: &str, selected: bool) {
    item_info(response, label, selected);
    let tokens = Tokens::get(ui.ctx());
    if selected {
        ui.painter().rect_filled(rect, 0.0, tokens.row_selected);
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 0.0, tokens.hover.gamma_multiply(0.5));
    }
}
