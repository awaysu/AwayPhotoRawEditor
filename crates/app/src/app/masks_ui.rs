//! 遮罩 tool (處理版本 3): the mask list and controls, the radial handles, brush painting
//! and the red overlay. The weights come from `awpr_core::masks` — the same raster the
//! renderer applies — so the overlay shows exactly where an edit lands.

use super::App;
use crate::i18n::{f, t};
use crate::theme;
use crate::tools::{self, Drag, View, P};
use crate::widgets::{Gradient, SliderSpec};
use awpr_core::{masks, BrushStroke, LocalMask, MaskKind};
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};

/// The overlay is rasterized at most this large (long edge), whatever the proxy.
const OVERLAY_MAX: usize = 1200;

/// A new stroke point only when the pointer moved this many points.
const PAINT_STEP: f64 = 2.0;

impl App {
    pub(super) fn active_mask(&mut self) -> Option<usize> {
        tools::active_mask(&self.adj, &mut self.active_mask)
    }

    pub(super) fn add_mask(&mut self, kind: MaskKind) {
        self.edit_begin();
        self.adj.masks.push(LocalMask { kind, ..Default::default() });
        self.active_mask = self.adj.masks.len() as i32 - 1;
        self.mask_overlay = kind == MaskKind::Brush || self.mask_overlay;
        self.edited();
    }

    pub(super) fn delete_mask(&mut self, index: usize) {
        if index >= self.adj.masks.len() {
            return;
        }
        self.edit_begin();
        self.adj.masks.remove(index);
        self.active_mask = self.adj.masks.len() as i32 - 1;
        self.edited();
    }

    /// The 遮罩 tool's controls.
    pub(super) fn mask_controls(&mut self, ui: &mut egui::Ui, on: bool) {
        let on = on && self.v3_notice(ui);
        let active = self.active_mask();
        // The list.
        if self.adj.masks.is_empty() {
            ui.label(RichText::new(t("（尚無遮罩）")).color(theme::TEXT_FAINT));
        }
        let mut pick = None;
        ui.horizontal_wrapped(|ui| {
            for (i, m) in self.adj.masks.iter().enumerate() {
                let name = match m.kind {
                    MaskKind::Radial => f("放射狀 {0}", &[&(i + 1)]),
                    MaskKind::Brush => f("筆刷 {0}", &[&(i + 1)]),
                };
                if ui.add_enabled(on, egui::Button::new(name).selected(Some(i) == active)).clicked() {
                    pick = Some(i);
                }
            }
        });
        if let Some(i) = pick {
            self.active_mask = i as i32;
        }
        ui.horizontal(|ui| {
            let w = (ui.available_width() - 2.0 * ui.spacing().item_spacing.x) / 3.0;
            // Short labels so three fit the column; the full action on hover.
            if crate::widgets::fixed_button(ui, on, Vec2::new(w, 26.0), format!("+ {}", t("放射狀")), |b| b).on_hover_text(t("新增放射狀")).clicked() {
                self.add_mask(MaskKind::Radial);
            }
            if crate::widgets::fixed_button(ui, on, Vec2::new(w, 26.0), format!("+ {}", t("筆刷")), |b| b).on_hover_text(t("新增筆刷")).clicked() {
                self.add_mask(MaskKind::Brush);
            }
            if crate::widgets::fixed_button(ui, on && active.is_some(), Vec2::new(w, 26.0), t("刪除"), |b| b).on_hover_text(t("刪除遮罩")).clicked() {
                if let Some(i) = active {
                    self.delete_mask(i);
                }
            }
        });
        let active = self.active_mask();
        let has = on && active.is_some();
        ui.horizontal(|ui| {
            let mut show = self.mask_overlay;
            if ui.add_enabled(on, egui::Checkbox::new(&mut show, t("顯示遮罩疊加"))).changed() {
                self.mask_overlay = show;
            }
            let mut inv = active.is_some_and(|i| self.adj.masks[i].invert);
            if ui.add_enabled(has, egui::Checkbox::new(&mut inv, t("反轉"))).changed() {
                if let Some(i) = active {
                    self.edit_begin();
                    self.adj.masks[i].invert = inv;
                    self.edited();
                }
            }
        });
        let kind = active.map(|i| self.adj.masks[i].kind);
        type Field = (SliderSpec, fn(&LocalMask) -> f64, fn(&mut LocalMask, f64));
        let mut shape: Vec<Field> = Vec::new();
        match kind {
            Some(MaskKind::Brush) => {
                shape.push((SliderSpec { min: 1.0, max: 100.0, default: 16.0, bipolar: false, ..SliderSpec::pm100(t("筆刷大小")) }, |m| m.brush_size * 400.0, |m, v| m.brush_size = v / 400.0));
                shape.push((SliderSpec { min: 0.0, default: 50.0, bipolar: false, ..SliderSpec::pm100(t("羽化")) }, |m| m.brush_feather, |m, v| m.brush_feather = v));
                shape.push((SliderSpec { min: 1.0, default: 100.0, bipolar: false, ..SliderSpec::pm100(t("流量")) }, |m| m.brush_flow, |m, v| m.brush_flow = v));
            }
            _ => shape.push((SliderSpec { min: 0.0, default: 50.0, bipolar: false, ..SliderSpec::pm100(t("羽化")) }, |m| m.feather, |m, v| m.feather = v)),
        }
        let fields: [Field; 5] = [
            (SliderSpec { min: -5.0, max: 5.0, decimals: 2, wheel_step: 0.05, ..SliderSpec::pm100(t("曝光")) }, |m| m.exposure, |m, v| m.exposure = v),
            (SliderSpec::pm100(t("對比")), |m| m.contrast, |m, v| m.contrast = v),
            (SliderSpec::pm100(t("亮部")), |m| m.highlights, |m, v| m.highlights = v),
            (SliderSpec::pm100(t("暗部")), |m| m.shadows, |m, v| m.shadows = v),
            (SliderSpec::pm100(t("飽和度")).gradient(Gradient::Saturation), |m| m.saturation, |m, v| m.saturation = v),
        ];
        for (n, (spec, get, set)) in shape.into_iter().chain(fields).enumerate() {
            ui.push_id(("mask_field", n), |ui| {
                self.slider_with(ui, spec, has, |a| active.map_or(spec.default, |i| get(&a.masks[i])), |a, v| {
                    if let Some(i) = active {
                        set(&mut a.masks[i], v);
                    }
                });
            });
        }
        if kind == Some(MaskKind::Brush) {
            let mut erase = self.brush_erase;
            if ui.add_enabled(has, egui::Checkbox::new(&mut erase, t("擦除"))).on_hover_text(t("筆刷改為從遮罩擦掉")).changed() {
                self.brush_erase = erase;
            }
        }
    }

    /// Mask tool press: a radial handle of the selected mask, another radial's centre, or
    /// (selected brush mask) the start of a stroke.
    pub(super) fn begin_mask_drag(&mut self, p: P, v: &View) {
        if !self.adj.is_v3() {
            return;
        }
        if let Some(ai) = self.active_mask() {
            if self.adj.masks[ai].kind == MaskKind::Radial {
                let d = tools::radial_hit(&self.adj.masks[ai], v, p);
                if d != Drag::None {
                    self.drag = d;
                    self.edit_begin();
                    return;
                }
            }
        }
        if let Some(i) = tools::radial_at_point(&self.adj, v, p) {
            self.active_mask = i as i32;
            self.drag = Drag::MaskCenter;
            self.edit_begin();
            return;
        }
        if let Some(ai) = self.active_mask() {
            if self.adj.masks[ai].kind == MaskKind::Brush {
                self.edit_begin();
                let (nx, ny) = v.ctrl_to_norm(p);
                let m = &mut self.adj.masks[ai];
                m.strokes.push(BrushStroke { radius: m.brush_size, feather: m.brush_feather, flow: m.brush_flow, erase: self.brush_erase, points: vec![(nx, ny)] });
                self.drag = Drag::MaskPaint;
                self.paint_last = Some(p);
                self.edited();
            }
        }
    }

    pub(super) fn update_mask_drag(&mut self, p: P, v: &View) {
        let Some(ai) = self.active_mask() else { return };
        match self.drag {
            Drag::MaskPaint => {
                if self.paint_last.is_some_and(|q| tools::dist(p, q) < PAINT_STEP) {
                    return;
                }
                self.paint_last = Some(p);
                let (nx, ny) = v.ctrl_to_norm(p);
                if let Some(s) = self.adj.masks[ai].strokes.last_mut() {
                    s.points.push((nx, ny));
                }
                self.edited();
            }
            d @ (Drag::MaskCenter | Drag::MaskRadiusX | Drag::MaskRadiusY | Drag::MaskRotate) => {
                tools::drag_radial(&mut self.adj.masks[ai], d, p, v);
                self.edited();
            }
            _ => {}
        }
    }

    /// Outlines, handles, the brush cursor and (optionally) the red overlay of the
    /// selected mask.
    pub(super) fn paint_masks(&mut self, ui: &egui::Ui, painter: &egui::Painter, v: &View, image: egui::Rect) {
        let active = self.active_mask();
        if self.mask_overlay {
            if let Some(i) = active {
                self.paint_overlay(ui, painter, i, v, image);
            }
        }
        let pt = |p: P| egui::pos2(p.x as f32, p.y as f32);
        for (i, m) in self.adj.masks.iter().enumerate() {
            if m.kind != MaskKind::Radial {
                continue;
            }
            let sel = Some(i) == active;
            let col = if sel { Color32::WHITE } else { Color32::from_white_alpha(120) };
            let outer: Vec<egui::Pos2> = tools::radial_outline(m, v, 1.0, 96).into_iter().map(pt).collect();
            painter.add(egui::Shape::line(outer, Stroke::new(if sel { 1.5 } else { 1.0 }, col)));
            if sel && m.feather > 0.0 {
                // Where the fall-off starts.
                let inner: Vec<egui::Pos2> = tools::radial_outline(m, v, 1.0 - m.feather / 100.0, 96).into_iter().map(pt).collect();
                for seg in inner.chunks(4).step_by(2) {
                    painter.add(egui::Shape::line(seg.to_vec(), Stroke::new(1.0, Color32::from_white_alpha(140))));
                }
            }
            let [c, hx, hy, rot] = tools::radial_handles(m, v);
            painter.circle(pt(c), if sel { 6.0 } else { 4.5 }, if sel { Color32::WHITE } else { Color32::from_white_alpha(160) }, Stroke::new(1.0, Color32::from_black_alpha(160)));
            if sel {
                painter.line_segment([pt(hx), pt(rot)], Stroke::new(1.0, Color32::from_white_alpha(140)));
                for h in [hx, hy] {
                    painter.circle(pt(h), 5.0, Color32::from_rgb(0xF2, 0xC2, 0x3A), Stroke::new(1.0, Color32::from_black_alpha(160)));
                }
                painter.circle(pt(rot), 5.0, Color32::from_rgb(0x4A, 0x90, 0xE2), Stroke::new(1.0, Color32::from_black_alpha(160)));
            }
        }
        // Brush cursor: the brush's radius and its soft edge.
        if let Some(i) = active {
            let m = &self.adj.masks[i];
            if m.kind == MaskKind::Brush && self.adj.is_v3() {
                if let Some(pos) = ui.input(|inp| inp.pointer.hover_pos()).filter(|p| image.contains(*p)) {
                    let r = (m.brush_size * tools::long_px(v)) as f32;
                    let col = if self.brush_erase { Color32::from_rgb(0xE0, 0x60, 0x60) } else { Color32::WHITE };
                    painter.circle_stroke(pos, r, Stroke::new(1.2, col));
                    painter.circle_stroke(pos, r * (1.0 - m.brush_feather as f32 / 100.0), Stroke::new(1.0, Color32::from_white_alpha(110)));
                }
            }
        }
    }

    fn paint_overlay(&mut self, ui: &egui::Ui, painter: &egui::Painter, i: usize, v: &View, image: egui::Rect) {
        let (w, h) = (v.w as usize, v.h as usize);
        let k = (w.max(h) as f64 / OVERLAY_MAX as f64).max(1.0);
        let (ow, oh) = (((w as f64 / k).round() as usize).max(1), ((h as f64 / k).round() as usize).max(1));
        let m = &self.adj.masks[i];
        let key = masks::shape_key(m, ow, oh);
        if self.mask_overlay_tex.as_ref().is_none_or(|(k2, _)| *k2 != key) {
            let wts = masks::weights(m, ow, oh);
            let rgba: Vec<u8> = wts.iter().flat_map(|&a| [255u8, 40, 40, (a * 130.0) as u8]).collect();
            let img = egui::ColorImage::from_rgba_unmultiplied([ow, oh], &rgba);
            let tex = ui.ctx().load_texture("mask_overlay", img, egui::TextureOptions::LINEAR);
            self.mask_overlay_tex = Some((key, tex));
        }
        if let Some((_, tex)) = &self.mask_overlay_tex {
            painter.image(tex.id(), image, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        }
    }
}
