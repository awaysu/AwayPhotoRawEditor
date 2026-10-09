//! Classic dark palette (the C# `Theme.ClassicDark` colours) and the CJK UI font.

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId, TextStyle};
use crate::i18n::Lang;
use crate::settings::{FontKind, FontSizes};
use std::sync::{Arc, RwLock};

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

/// One font face: file and face index inside a .ttc.
type Face = (std::path::PathBuf, u32);

/// macOS keeps PingFang in the font asset store under a hashed folder.
fn pingfang() -> Option<std::path::PathBuf> {
    let base = std::path::Path::new("/System/Library/AssetsV2");
    for e in std::fs::read_dir(base).ok()?.flatten() {
        if !e.file_name().to_string_lossy().starts_with("com_apple_MobileAsset_Font") {
            continue;
        }
        for a in std::fs::read_dir(e.path()).ok()?.flatten() {
            let p = a.path().join("AssetData/PingFang.ttc");
            if p.exists() {
                return Some(p);
            }
        }
    }
    None
}

/// The CJK faces for a language, best first (`Theme.FontFamily` per language: 微軟正黑體,
/// 微軟雅黑, Yu Gothic UI, Malgun Gothic on Windows; 蘋方 / Hiragino / Apple SD Gothic
/// Neo on macOS; Noto Sans CJK on Linux).
fn faces_for(lang: Lang) -> Vec<Face> {
    let p = |s: &str, i: u32| (std::path::PathBuf::from(s), i);
    if cfg!(windows) {
        let fonts = std::path::PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| "C:/Windows".into())).join("Fonts");
        let f = |n: &str, i: u32| (fonts.join(n), i);
        match lang {
            Lang::ZhCn => vec![f("msyh.ttc", 1), f("msyh.ttf", 0)],
            Lang::Ja => vec![f("YuGothM.ttc", 1), f("meiryo.ttc", 0), f("msgothic.ttc", 0)],
            Lang::Ko => vec![f("malgun.ttf", 0)],
            _ => vec![f("msjh.ttc", 1), f("msjh.ttf", 0), f("mingliu.ttc", 0)],
        }
    } else if cfg!(target_os = "macos") {
        let pf = pingfang();
        let mut v = Vec::new();
        match lang {
            Lang::Ja => v.push(p("/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc", 0)), // i18n-ignore: not language text
            Lang::Ko => v.push(p("/System/Library/Fonts/AppleSDGothicNeo.ttc", 0)),
            Lang::ZhCn => {
                v.extend(pf.map(|f| (f, 3)));
                v.push(p("/System/Library/Fonts/STHeiti Medium.ttc", 1));
            }
            _ => {
                v.extend(pf.map(|f| (f, 2)));
                v.push(p("/System/Library/Fonts/STHeiti Medium.ttc", 0));
            }
        }
        v.push(p("/System/Library/Fonts/Hiragino Sans GB.ttc", 0));
        v
    } else {
        let idx = match lang {
            Lang::Ja => 0,
            Lang::Ko => 1,
            Lang::ZhCn => 2,
            _ => 3,
        };
        vec![
            p("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", idx),
            p("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc", idx),
            p("/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc", idx),
            p("/usr/share/fonts/truetype/wqy/wqy-microhei.ttc", 0),
        ]
    }
}

/// Install the UI fonts for a language: egui's Latin font first, then the language's CJK
/// face, then one face of each other CJK script (the language lists and file names must
/// still draw). Returns the main CJK file used. Callable at any time: switching language
/// swaps the fonts without a restart.
pub fn install_fonts(ctx: &egui::Context, lang: Lang) -> Option<String> {
    let mut fonts = FontDefinitions::default();
    let mut used = None;
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    let order = std::iter::once(lang).chain([Lang::ZhTw, Lang::ZhCn, Lang::Ja, Lang::Ko].into_iter().filter(move |l| *l != lang));
    for (n, l) in order.enumerate() {
        let Some((face, bytes)) = faces_for(l).into_iter().find_map(|f| std::fs::read(&f.0).ok().map(|b| (f, b))) else { continue };
        // One file (Noto CJK, PingFang) holds every script: the first face of it is enough
        // as a fallback.
        if n > 0 && files.contains(&face.0) {
            continue;
        }
        let name = format!("cjk{n}");
        let mut data = FontData::from_owned(bytes);
        data.index = face.1;
        fonts.font_data.insert(name.clone(), Arc::new(data));
        fonts.families.entry(FontFamily::Proportional).or_default().push(name.clone());
        fonts.families.entry(FontFamily::Monospace).or_default().push(name);
        if used.is_none() {
            used = Some(face.0.to_string_lossy().into_owned());
        }
        files.push(face.0);
    }
    ctx.set_fonts(fonts);
    used
}

/// Install fonts, the dark visuals and the type sizes.
pub fn install(ctx: &egui::Context, lang: Lang, sizes: FontSizes) -> Option<String> {
    let used = install_fonts(ctx, lang);
    ctx.set_theme(egui::Theme::Dark);
    set_font_sizes(ctx, sizes);
    used
}

// ---- type sizes ------------------------------------------------------------------------

static SIZES: RwLock<Option<FontSizes>> = RwLock::new(None);

/// The look the layout was drawn for, in points, at the default sizes.
fn base_points(k: FontKind) -> f32 {
    match k {
        FontKind::Small | FontKind::Mono => 12.0,
        FontKind::Normal | FontKind::AboutBody | FontKind::FolderGlyph | FontKind::IconGlyph => 14.0,
        FontKind::SectionTitle => 15.0,
        FontKind::ProgressTitle => 16.0,
        FontKind::DialogTitle => 18.0,
        FontKind::Logo | FontKind::MenuGlyph => 20.0,
        FontKind::AboutTitle => 22.0,
    }
}

/// A size category in points: the base look scaled by the user's pixel size against the
/// default one (字體大小… tunes each category; 介面大小 scales everything on top).
pub fn fs(k: FontKind) -> f32 {
    let s = SIZES.read().ok().and_then(|g| *g).unwrap_or_default();
    base_points(k) * s.get(k) as f32 / FontSizes::default().get(k) as f32
}

/// A literal size from the layout code, mapped to its category and scaled with it.
pub fn scaled(points: f32) -> f32 {
    let k = if points < 13.0 {
        FontKind::Small
    } else if points < 14.6 {
        FontKind::Normal
    } else if points < 15.6 {
        FontKind::SectionTitle
    } else if points < 17.0 {
        FontKind::ProgressTitle
    } else if points < 19.0 {
        FontKind::DialogTitle
    } else if points < 21.0 {
        FontKind::Logo
    } else {
        FontKind::AboutTitle
    };
    points * fs(k) / base_points(k)
}

pub fn set_font_sizes(ctx: &egui::Context, sizes: FontSizes) {
    if let Ok(mut g) = SIZES.write() {
        *g = Some(sizes);
    }
    ctx.all_styles_mut(apply_style);
}

fn apply_style(style: &mut egui::Style) {
    style.text_styles = [
        (TextStyle::Small, FontId::proportional(fs(FontKind::Small))),
        (TextStyle::Body, FontId::proportional(fs(FontKind::Normal))),
        (TextStyle::Button, FontId::proportional(fs(FontKind::Normal))),
        (TextStyle::Heading, FontId::proportional(fs(FontKind::ProgressTitle))),
        (TextStyle::Monospace, FontId::monospace(fs(FontKind::Mono))),
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
/// The side columns' scroll bar: solid (never over a slider) but thin, so the narrow
/// columns keep their room.
pub fn side_scroll_style() -> egui::style::ScrollStyle {
    egui::style::ScrollStyle { bar_inner_margin: 2.0, ..egui::style::ScrollStyle::solid() }
}

pub fn section<R>(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(PANEL)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .corner_radius(4.0)
        .inner_margin(egui::Margin { left: 5, right: 5, top: 6, bottom: 8 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(title).strong().size(fs(FontKind::SectionTitle)));
            ui.add_space(2.0);
            body(ui)
        })
        .inner
}
