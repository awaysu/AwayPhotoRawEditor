//! Classic dark palette (the C# `Theme.ClassicDark` colours) and the CJK UI font.

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId, TextStyle};
use std::sync::Arc;

pub const WINDOW: Color32 = Color32::from_rgb(0x1E, 0x1E, 0x1E);
pub const PANEL: Color32 = Color32::from_rgb(0x25, 0x25, 0x25);
pub const TOOLBAR: Color32 = Color32::from_rgb(0x2D, 0x2D, 0x2D);
pub const VIEWER: Color32 = Color32::from_rgb(0x14, 0x14, 0x14);
pub const BORDER: Color32 = Color32::from_rgb(0x3A, 0x3A, 0x3E);
pub const TEXT: Color32 = Color32::from_rgb(0xE0, 0xE0, 0xE4);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0xA8, 0xA8, 0xB0);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x74, 0x74, 0x7C);
pub const ACCENT: Color32 = Color32::from_rgb(0x3D, 0x8B, 0xF2);
pub const TRACK: Color32 = Color32::from_rgb(0x44, 0x44, 0x4A);
pub const EDITED: Color32 = Color32::from_rgb(0xF2, 0xB1, 0x3D);
pub const BUTTON: Color32 = Color32::from_rgb(0x38, 0x38, 0x3E);
pub const BUTTON_HOVER: Color32 = Color32::from_rgb(0x46, 0x46, 0x4E);

/// Candidate CJK fonts per platform (file, face index inside a .ttc). The first that
/// exists wins; egui's built-in fonts have no Chinese glyphs.
fn cjk_candidates() -> &'static [(&'static str, u32)] {
    if cfg!(windows) {
        // Microsoft JhengHei UI (index 1 of msjh.ttc) — the C# build's UI font.
        &[(r"C:\Windows\Fonts\msjh.ttc", 1), (r"C:\Windows\Fonts\msjh.ttf", 0), (r"C:\Windows\Fonts\mingliu.ttc", 0)]
    } else if cfg!(target_os = "macos") {
        &[
            ("/System/Library/Fonts/PingFang.ttc", 2),
            ("/System/Library/Fonts/Hiragino Sans GB.ttc", 0),
            ("/System/Library/Fonts/STHeiti Medium.ttc", 0),
            ("/Library/Fonts/Arial Unicode.ttf", 0),
            ("/System/Library/Fonts/Supplemental/Arial Unicode.ttf", 0),
        ]
    } else {
        &[
            ("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", 3),
            ("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc", 3),
            ("/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc", 3),
            ("/usr/share/fonts/truetype/wqy/wqy-microhei.ttc", 0),
            ("/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc", 0),
            ("/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf", 0),
        ]
    }
}

/// Install the CJK font and the dark visuals. Returns the font file used (or None).
pub fn install(ctx: &egui::Context) -> Option<String> {
    let mut fonts = FontDefinitions::default();
    let mut used = None;
    for &(path, index) in cjk_candidates() {
        if let Ok(bytes) = std::fs::read(path) {
            let mut data = FontData::from_owned(bytes);
            data.index = index;
            fonts.font_data.insert("cjk".into(), Arc::new(data));
            // Latin first (egui's font has nicer digits), CJK as the fallback for the rest.
            fonts.families.entry(FontFamily::Proportional).or_default().push("cjk".into());
            fonts.families.entry(FontFamily::Monospace).or_default().push("cjk".into());
            used = Some(path.to_string());
            break;
        }
    }
    ctx.set_fonts(fonts);

    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| apply_style(style));
    used
}

fn apply_style(style: &mut egui::Style) {
    style.text_styles = [
        (TextStyle::Small, FontId::proportional(12.0)),
        (TextStyle::Body, FontId::proportional(14.0)),
        (TextStyle::Button, FontId::proportional(14.0)),
        (TextStyle::Heading, FontId::proportional(16.0)),
        (TextStyle::Monospace, FontId::monospace(12.0)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(10.0, 4.0);
    let v = &mut style.visuals;
    *v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = Color32::from_rgb(0x1A, 0x1A, 0x1C);
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = ACCENT;
    v.widgets.inactive.weak_bg_fill = BUTTON;
    v.widgets.inactive.bg_fill = BUTTON;
    v.widgets.hovered.weak_bg_fill = BUTTON_HOVER;
    v.widgets.hovered.bg_fill = BUTTON_HOVER;
    v.widgets.active.weak_bg_fill = ACCENT;
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, BORDER);
}

/// A section box with a bold title (`SectionPanel`).
pub fn section<R>(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(PANEL)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .corner_radius(4.0)
        .inner_margin(egui::Margin { left: 10, right: 10, top: 6, bottom: 8 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(title).strong().size(15.0));
            ui.add_space(2.0);
            body(ui)
        })
        .inner
}
