//! Dark neon theme matching the MixTree design concept (spec §11).

use egui::{Color32, Rounding, Stroke, Visuals};

pub const BG: Color32 = Color32::from_rgb(0x05, 0x07, 0x0d);
pub const BG2: Color32 = Color32::from_rgb(0x0a, 0x0e, 0x18);
pub const PANEL: Color32 = Color32::from_rgb(0x0c, 0x11, 0x1d);
pub const PANEL2: Color32 = Color32::from_rgb(0x0f, 0x16, 0x26);
pub const LINE: Color32 = Color32::from_rgb(0x1a, 0x23, 0x35);
pub const LINE2: Color32 = Color32::from_rgb(0x24, 0x31, 0x49);
pub const TEXT: Color32 = Color32::from_rgb(0xe6, 0xed, 0xf7);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x8a, 0x97, 0xad);
pub const RED: Color32 = Color32::from_rgb(0xff, 0x2d, 0x55);
pub const RED_SOFT: Color32 = Color32::from_rgb(0xff, 0x6b, 0x84);
pub const CYAN: Color32 = Color32::from_rgb(0x2d, 0x9c, 0xff);
pub const GREEN: Color32 = Color32::from_rgb(0x2e, 0xcc, 0x71);
pub const ORANGE: Color32 = Color32::from_rgb(0xff, 0x8c, 0x1a);
pub const PURPLE: Color32 = Color32::from_rgb(0xa8, 0x55, 0xf7);
pub const GREY: Color32 = Color32::from_rgb(0x9a, 0xa5, 0xb1);

/// Parses a `#rrggbb` hex color (falls back to grey on malformed input).
pub fn hex(s: &str) -> Color32 {
    let s = s.trim_start_matches('#');
    if s.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&s[0..2], 16),
            u8::from_str_radix(&s[2..4], 16),
            u8::from_str_radix(&s[4..6], 16),
        ) {
            return Color32::from_rgb(r, g, b);
        }
    }
    GREY
}

/// Applies the global dark theme to the egui context.
pub fn apply(ctx: &egui::Context) {
    let mut v = Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = BG;
    v.faint_bg_color = PANEL2;
    v.override_text_color = Some(TEXT);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    v.widgets.inactive.bg_fill = PANEL2;
    v.widgets.inactive.weak_bg_fill = PANEL2;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, LINE2);
    v.widgets.hovered.bg_fill = LINE;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, CYAN);
    v.widgets.active.bg_fill = LINE2;
    v.selection.bg_fill = Color32::from_rgba_unmultiplied(0x2d, 0x9c, 0xff, 60);
    v.selection.stroke = Stroke::new(1.0, CYAN);
    v.widgets.inactive.rounding = Rounding::same(6.0);
    v.widgets.hovered.rounding = Rounding::same(6.0);
    v.widgets.active.rounding = Rounding::same(6.0);
    ctx.set_visuals(v);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    ctx.set_style(style);
}
