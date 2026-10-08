//! The photo viewer: zoom / pan (the C# `ImageViewer` behaviour) and the two ways of
//! putting pixels on screen —
//!
//! * GPU: a fragment shader reads the rendered frame's storage buffer directly, so a
//!   slider change goes compute → screen without ever leaving the device;
//! * CPU fallback: the rendered image as an egui texture.

use crate::theme;
use awpr_gpu::display;
use eframe::egui::{self, Pos2, Rect, Vec2};
use eframe::egui_wgpu;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ZoomMode {
    Fit,
    /// 1 image pixel : 1 physical screen pixel.
    Actual100,
    Actual200,
    Custom,
}

/// Zoom / pan state. Scales are physical screen pixels per image pixel, so 100% stays
/// 1:1 on any display scaling (judging sharpness and noise needs that).
pub struct ViewerState {
    pub mode: ZoomMode,
    scale: f32,
    /// The image point (image pixels) at the centre of the view.
    center: Pos2,
    image_size: Vec2,
    press: Option<Pos2>,
    panned: bool,
}

/// Movement under this (points) is a click, not a pan.
const PAN_THRESHOLD: f32 = 4.0;

impl Default for ViewerState {
    fn default() -> Self {
        Self { mode: ZoomMode::Fit, scale: 1.0, center: Pos2::ZERO, image_size: Vec2::ZERO, press: None, panned: false }
    }
}

/// Where the image lands this frame.
pub struct Placement {
    /// The viewer area (points).
    pub view: Rect,
    /// The whole image's rectangle (points); may extend past `view`.
    pub image: Rect,
    /// Physical pixels per image pixel.
    pub scale_px: f32,
}

impl ViewerState {
    pub fn reset_fit(&mut self) {
        self.mode = ZoomMode::Fit;
    }

    pub fn set_mode(&mut self, mode: ZoomMode) {
        self.mode = mode;
        if mode != ZoomMode::Custom {
            self.center = (self.image_size / 2.0).to_pos2();
        }
    }

    pub fn zoom_percent(&self) -> f32 {
        self.scale * 100.0
    }

    fn fit_scale(&self, view: Rect, ppp: f32) -> f32 {
        if self.image_size.x <= 0.0 || self.image_size.y <= 0.0 {
            return 1.0;
        }
        ((view.width() * ppp) / self.image_size.x).min((view.height() * ppp) / self.image_size.y).min(1.0)
    }

    fn clamp_center(&mut self, view: Rect, ppp: f32) {
        // Keep the image from leaving the view: centre it on an axis that fits.
        let half = view.size() * ppp / (2.0 * self.scale);
        for (c, size, h) in [(&mut self.center.x, self.image_size.x, half.x), (&mut self.center.y, self.image_size.y, half.y)] {
            *c = if size <= 2.0 * h { size / 2.0 } else { c.clamp(h, size - h) };
        }
    }

    /// Handle input over `resp` and work out this frame's placement for an image of
    /// `size` (image pixels).
    pub fn update(&mut self, ui: &egui::Ui, resp: &egui::Response, size: Vec2) -> Placement {
        let ppp = ui.ctx().pixels_per_point();
        let view = resp.rect;
        if size != self.image_size {
            // A different photo or a crop / rotation changed the frame: recentre.
            self.image_size = size;
            self.center = (size / 2.0).to_pos2();
        }
        match self.mode {
            ZoomMode::Fit => {
                self.scale = self.fit_scale(view, ppp);
                self.center = (size / 2.0).to_pos2();
            }
            ZoomMode::Actual100 => self.scale = 1.0,
            ZoomMode::Actual200 => self.scale = 2.0,
            ZoomMode::Custom => {}
        }

        let to_image = |s: &Self, p: Pos2| s.center + (p - view.center()) * ppp / s.scale;

        // Left button: click cycles Fit → 100% → 200% → Fit around the clicked point,
        // drag pans (the C# viewer). Panning keeps the zoom mode so the cycle continues.
        if resp.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_down()) {
            if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
                match self.press {
                    None => {
                        self.press = Some(p);
                        self.panned = false;
                    }
                    Some(start) => {
                        if self.panned || (p - start).length() > PAN_THRESHOLD {
                            self.panned = true;
                            let d = ui.input(|i| i.pointer.delta());
                            self.center -= d * ppp / self.scale;
                            if self.mode == ZoomMode::Fit {
                                // Fit already shows everything; nothing to pan.
                                self.center = (size / 2.0).to_pos2();
                            }
                        }
                    }
                }
            }
        } else if let Some(p) = self.press.take() {
            if !self.panned && resp.hovered() {
                let anchor = to_image(self, p);
                let next = match self.mode {
                    ZoomMode::Fit => ZoomMode::Actual100,
                    ZoomMode::Actual100 => ZoomMode::Actual200,
                    ZoomMode::Actual200 | ZoomMode::Custom => ZoomMode::Fit,
                };
                self.zoom_about(next, anchor, p, view, ppp);
            }
            self.panned = false;
        }

        // Wheel: free zoom about the pointer.
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                if let Some(p) = ui.input(|i| i.pointer.hover_pos()) {
                    let anchor = to_image(self, p);
                    let factor = (scroll / 200.0).exp();
                    let fit = self.fit_scale(view, ppp);
                    self.scale = (self.scale * factor).clamp(fit.min(0.05), 8.0);
                    self.mode = ZoomMode::Custom;
                    self.center = anchor - (p - view.center()) * ppp / self.scale;
                }
            }
        }

        self.clamp_center(view, ppp);
        let size_pt = size * self.scale / ppp;
        let min = view.center() - (self.center.to_vec2() * self.scale / ppp);
        Placement { view, image: Rect::from_min_size(min, size_pt), scale_px: self.scale }
    }

    fn zoom_about(&mut self, mode: ZoomMode, anchor: Pos2, screen: Pos2, view: Rect, ppp: f32) {
        self.mode = mode;
        self.scale = match mode {
            ZoomMode::Fit => self.fit_scale(view, ppp),
            ZoomMode::Actual100 => 1.0,
            ZoomMode::Actual200 => 2.0,
            ZoomMode::Custom => self.scale,
        };
        // Keep the clicked image point under the pointer.
        self.center = anchor - (screen - view.center()) * ppp / self.scale;
        if mode == ZoomMode::Fit {
            self.center = (self.image_size / 2.0).to_pos2();
        }
    }
}

// ---- GPU display -------------------------------------------------------------------

/// The shared viewer pipeline, kept in egui-wgpu's callback resources.
pub struct DisplayPipeline(display::DisplayPipeline);

impl DisplayPipeline {
    pub fn install(rs: &egui_wgpu::RenderState) {
        let dp = display::DisplayPipeline::new(&rs.device, rs.target_format);
        rs.renderer.write().callback_resources.insert(DisplayPipeline(dp));
    }

    /// A bind group drawing `buffer` (an RGBA f32 frame).
    pub fn bind(rs: &egui_wgpu::RenderState, buffer: &wgpu::Buffer) -> wgpu::BindGroup {
        let r = rs.renderer.read();
        let dp: &DisplayPipeline = r.callback_resources.get().expect("display pipeline installed");
        dp.0.bind(&rs.device, buffer)
    }
}

struct FrameCallback {
    bind: Arc<wgpu::BindGroup>,
    uniform: display::ViewUniform,
}

impl egui_wgpu::CallbackTrait for FrameCallback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(dp) = resources.get::<DisplayPipeline>() {
            dp.0.write_uniform(queue, self.uniform);
        }
        Vec::new()
    }

    fn paint(&self, _info: egui::PaintCallbackInfo, pass: &mut wgpu::RenderPass<'static>, resources: &egui_wgpu::CallbackResources) {
        let Some(dp) = resources.get::<DisplayPipeline>() else { return };
        dp.0.draw(pass, &self.bind);
    }
}

/// Paint a GPU frame of `size` (image pixels) at `pl`.
pub fn paint_gpu(ui: &egui::Ui, pl: &Placement, bind: &Arc<wgpu::BindGroup>, size: Vec2) {
    let ppp = ui.ctx().pixels_per_point();
    // Display-encoded like the image (only edge pixels half off the image use it).
    let bg = theme::VIEWER;
    let uniform = display::ViewUniform::new(
        size.x as usize,
        size.y as usize,
        [pl.image.min.x * ppp, pl.image.min.y * ppp],
        pl.scale_px,
        [bg.r() as f32 / 255.0, bg.g() as f32 / 255.0, bg.b() as f32 / 255.0],
        false,
    );
    // Only the part of the image inside the view is drawn; egui's own background shows
    // around it.
    let rect = pl.image.intersect(pl.view);
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    ui.painter().add(egui_wgpu::Callback::new_paint_callback(rect, FrameCallback { bind: bind.clone(), uniform }));
}

/// Paint the CPU-rendered texture at `pl`.
pub fn paint_texture(ui: &egui::Ui, pl: &Placement, tex: &egui::TextureHandle) {
    let painter = ui.painter().with_clip_rect(pl.view);
    painter.image(tex.id(), pl.image, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), egui::Color32::WHITE);
}
