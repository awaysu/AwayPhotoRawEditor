//! 匯出設定 window (the C# `ExportForm`), the background export job with its progress
//! window, and the live watermark overlay on the viewer.

use crate::theme;
use awpr_core::{apply_to_float, FloatImage, ImageAdjustments, ProcessContext};
use awpr_gpu::GpuPipeline;
use awpr_photo::export::{self, ConflictMode, ExportFormat, ExportItem, ExportLocation, ExportSettings, RenameMode};
use awpr_photo::loader::LoaderOptions;
use awpr_photo::watermark::{self, WatermarkColor, WatermarkPosition};
use eframe::egui::{self, Color32, RichText, Vec2};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, OnceLock};

/// What the dialog's footer asked for.
#[derive(PartialEq)]
pub enum DialogAction {
    None,
    /// 儲存設定 (close).
    Save,
    /// 儲存設定並開始轉存.
    Start,
    /// 取消 / window closed.
    Cancel,
}

/// The open 匯出設定 window: a draft of the settings until 儲存.
pub struct ExportDialog {
    pub draft: ExportSettings,
    max_edge: String,
    /// Export every (not hidden) photo instead of the current one.
    pub all: bool,
}

/// The installed font families, scanned once in the background.
pub fn font_list() -> Option<&'static Vec<String>> {
    static LIST: OnceLock<Vec<String>> = OnceLock::new();
    static STARTED: AtomicBool = AtomicBool::new(false);
    if LIST.get().is_none() && !STARTED.swap(true, Ordering::SeqCst) {
        std::thread::spawn(|| {
            let _ = LIST.set(watermark::font_families());
        });
    }
    LIST.get()
}

impl ExportDialog {
    pub fn new(settings: &ExportSettings) -> Self {
        font_list();
        Self { draft: settings.clone(), max_edge: settings.max_long_edge.to_string(), all: false }
    }

    /// Draw the window. Watermark edits are copied into `live` straight away so the main
    /// preview follows them (the C# dialog did the same).
    pub fn show(&mut self, ctx: &egui::Context, live: &mut ExportSettings, current: usize, all: usize) -> DialogAction {
        let mut action = DialogAction::None;
        let mut open = true;
        egui::Window::new("匯出設定")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(1016.0)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_width(1016.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("匯出照片").strong().size(18.0));
                        let n = if self.all { all } else { current };
                        ui.label(RichText::new(format!("共 {n} 張相片將被轉存")).color(theme::TEXT_DIM));
                    });
                    ui.add_space(40.0);
                    ui.label(RichText::new("範圍").color(theme::TEXT_DIM));
                    ui.radio_value(&mut self.all, false, "目前這張");
                    ui.radio_value(&mut self.all, true, format!("資料夾全部（{all} 張，不含隱藏）"));
                });
                ui.separator();
                ui.columns(2, |cols| {
                    self.left_column(&mut cols[0]);
                    self.right_column(&mut cols[1]);
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new("儲存設定").min_size(Vec2::new(100.0, 32.0))).clicked() {
                        action = DialogAction::Save;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new("取消").min_size(Vec2::new(92.0, 32.0))).clicked() {
                            action = DialogAction::Cancel;
                        }
                        let start = egui::Button::new(RichText::new("儲存設定並開始轉存").color(Color32::WHITE)).fill(theme::ACCENT).min_size(Vec2::new(168.0, 32.0));
                        if ui.add(start).clicked() {
                            action = DialogAction::Start;
                        }
                    });
                });
            });
        if !open {
            action = DialogAction::Cancel;
        }
        self.copy_watermark(live);
        if matches!(action, DialogAction::Save | DialogAction::Start) {
            self.commit();
        }
        action
    }

    /// `Commit`: the parsed long edge; an empty sub-folder name falls back to NEW_IMAGE.
    fn commit(&mut self) {
        let s = &mut self.draft;
        s.custom_path = s.custom_path.trim().to_string();
        if s.sub_folder.trim().is_empty() {
            s.sub_folder = "NEW_IMAGE".into();
        }
        s.sub_folder = s.sub_folder.trim().to_string();
        if let Ok(v) = self.max_edge.trim().parse::<i64>() {
            if v >= 0 {
                s.max_long_edge = v;
            }
        }
        s.jpeg_quality = s.jpeg_quality.clamp(50, 100);
    }

    fn copy_watermark(&self, live: &mut ExportSettings) {
        let d = &self.draft;
        live.watermark_enabled = d.watermark_enabled;
        live.watermark_text = d.watermark_text.clone();
        live.watermark_font_name = d.watermark_font_name.clone();
        live.watermark_font_size = d.watermark_font_size;
        live.watermark_transparency = d.watermark_transparency;
        live.watermark_color = d.watermark_color;
        live.watermark_position = d.watermark_position;
        live.watermark_margin = d.watermark_margin;
    }

    fn left_column(&mut self, ui: &mut egui::Ui) {
        let s = &mut self.draft;
        card(ui, "儲存位置", |ui| {
            ui.radio_value(&mut s.location, ExportLocation::Desktop, "桌面");
            ui.radio_value(&mut s.location, ExportLocation::SameAsSource, "同原始照片目錄");
            ui.radio_value(&mut s.location, ExportLocation::Custom, "自己選擇");
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                ui.add(egui::TextEdit::singleline(&mut s.custom_path).desired_width(ui.available_width() - 96.0));
                if ui.add(egui::Button::new("瀏覽").min_size(Vec2::new(84.0, 24.0))).clicked() {
                    let mut dlg = rfd::FileDialog::new().set_title("選擇儲存位置");
                    if !s.custom_path.trim().is_empty() {
                        dlg = dlg.set_directory(s.custom_path.trim());
                    }
                    if let Some(p) = dlg.pick_folder() {
                        s.custom_path = p.to_string_lossy().into_owned();
                        s.location = ExportLocation::Custom;
                    }
                }
            });
        });
        card(ui, "次資料夾", |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut s.use_sub_folder, "儲存至次資料夾");
                ui.add(egui::TextEdit::singleline(&mut s.sub_folder).desired_width(ui.available_width()));
            });
        });
        card(ui, "重新命名", |ui| {
            let names = ["按照原始檔案", "日期時間（IMG 年月日時分秒＋序號）", "數字開始（IMG00001）"];
            let modes = [RenameMode::Original, RenameMode::DateTime, RenameMode::Sequence];
            let cur = modes.iter().position(|m| *m == s.rename).unwrap_or(0);
            egui::ComboBox::from_id_salt("rename").width(ui.available_width()).selected_text(names[cur]).show_ui(ui, |ui| {
                for (m, n) in modes.iter().zip(names) {
                    ui.selectable_value(&mut s.rename, *m, n);
                }
            });
        });
        card(ui, "存檔遇到相同檔名", |ui| {
            ui.radio_value(&mut s.conflict, ConflictMode::AppendNumber, "檔名接續 \"_數字\"，例如 _1, _2...");
            ui.radio_value(&mut s.conflict, ConflictMode::Overwrite, "直接覆蓋");
        });
    }

    fn right_column(&mut self, ui: &mut egui::Ui) {
        let max_edge = &mut self.max_edge;
        let s = &mut self.draft;
        card(ui, "格式與尺寸", |ui| {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("format").width(110.0).selected_text(s.format.label()).show_ui(ui, |ui| {
                    for f in ExportFormat::ALL {
                        ui.selectable_value(&mut s.format, f, f.label());
                    }
                });
                ui.label(RichText::new("符合寬度高度(像素)").color(theme::TEXT_DIM));
                ui.add(egui::TextEdit::singleline(max_edge).desired_width(72.0)).on_hover_text("長邊上限；0 = 原尺寸");
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("解析度（像素/英寸）").color(theme::TEXT_DIM));
                egui::ComboBox::from_id_salt("dpi").width(80.0).selected_text(s.resolution.to_string()).show_ui(ui, |ui| {
                    for v in [100, 200, 300, 400, 500, 600] {
                        ui.selectable_value(&mut s.resolution, v, v.to_string());
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("JPEG 品質").color(theme::TEXT_DIM));
                ui.add_enabled(s.format == ExportFormat::Jpeg, egui::Slider::new(&mut s.jpeg_quality, 50..=100));
            });
            ui.checkbox(&mut s.preserve_exif, "保存 EXIF（相機 / 鏡頭 / 拍攝資訊）");
            ui.checkbox(&mut s.open_explorer_after, "轉檔完成後開啟檔案總管顯示");
            if matches!(s.format, ExportFormat::Tiff | ExportFormat::Png) {
                ui.label(RichText::new("TIFF / PNG 在「RAW 處理精度 16-bit」時輸出 16-bit；全部格式（BMP 除外）嵌入 sRGB 描述檔").size(11.5).color(theme::TEXT_FAINT));
            }
        });
        card(ui, "浮水印", |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut s.watermark_enabled, "啟用浮水印");
                ui.label(RichText::new("文字").color(theme::TEXT_DIM));
                ui.add(egui::TextEdit::singleline(&mut s.watermark_text).desired_width(ui.available_width()));
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("字體").color(theme::TEXT_DIM));
                egui::ComboBox::from_id_salt("wm_font").width(150.0).selected_text(s.watermark_font_name.clone()).show_ui(ui, |ui| match font_list() {
                    Some(list) => {
                        for f in list {
                            ui.selectable_value(&mut s.watermark_font_name, f.clone(), f);
                        }
                    }
                    None => {
                        ui.label("讀取字型中…");
                    }
                });
                ui.label(RichText::new("大小").color(theme::TEXT_DIM));
                ui.add(egui::DragValue::new(&mut s.watermark_font_size).range(6.0..=300.0).max_decimals(0));
                ui.label(RichText::new("顏色").color(theme::TEXT_DIM));
                egui::ComboBox::from_id_salt("wm_color").width(80.0).selected_text(s.watermark_color.label()).show_ui(ui, |ui| {
                    for c in WatermarkColor::ALL {
                        ui.selectable_value(&mut s.watermark_color, c, c.label());
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("透明度").color(theme::TEXT_DIM));
                ui.add(egui::DragValue::new(&mut s.watermark_transparency).range(0..=100));
                ui.label(RichText::new("位置").color(theme::TEXT_DIM));
                egui::ComboBox::from_id_salt("wm_pos").width(80.0).selected_text(s.watermark_position.label()).show_ui(ui, |ui| {
                    for p in WatermarkPosition::ALL {
                        ui.selectable_value(&mut s.watermark_position, p, p.label());
                    }
                });
                ui.label(RichText::new("邊緣").color(theme::TEXT_DIM));
                ui.add(egui::DragValue::new(&mut s.watermark_margin).range(0..=9999));
            });
        });
    }
}

/// A titled group box (`DarkGroupBox`).
fn card(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(theme::PANEL)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .corner_radius(4.0)
        .inner_margin(egui::Margin { left: 14, right: 14, top: 8, bottom: 10 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).strong());
            ui.add_space(4.0);
            body(ui);
        });
    ui.add_space(8.0);
}

// ---- the job --------------------------------------------------------------------------

enum JobMsg {
    Progress { done: usize, total: usize, name: String },
    Finished(Result<Vec<export::Written>, String>),
}

/// A running export (one background thread).
pub struct ExportJob {
    rx: Receiver<JobMsg>,
    cancel: Arc<AtomicBool>,
    pub done: usize,
    pub total: usize,
    pub name: String,
    pub result: Option<Result<Vec<export::Written>, String>>,
}

impl ExportJob {
    pub fn start(ctx: egui::Context, items: Vec<ExportItem>, settings: ExportSettings, opt: LoaderOptions, gpu: Option<&'static GpuPipeline>) -> Self {
        let (tx, rx): (Sender<JobMsg>, Receiver<JobMsg>) = std::sync::mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let total = items.len();
        let stop = cancel.clone();
        std::thread::Builder::new()
            .name("export".into())
            .spawn(move || {
                // Full resolution: the GPU only when the whole frame fits (16 MP), else CPU.
                let render = |img: &FloatImage, a: &ImageAdjustments, c: &ProcessContext| {
                    if let Some(g) = gpu.filter(|g| g.can_host(img.width, img.height)) {
                        if let Ok(o) = g.apply(img, a, c) {
                            return o;
                        }
                    }
                    apply_to_float(img, a, c)
                };
                let r = export::export_all(&items, &settings, opt, &render, |done, total, name| {
                    let _ = tx.send(JobMsg::Progress { done, total, name: name.to_string() });
                    ctx.request_repaint();
                    !stop.load(Ordering::SeqCst)
                });
                let _ = tx.send(JobMsg::Finished(r));
                ctx.request_repaint();
            })
            .expect("export thread");
        Self { rx, cancel, done: 0, total, name: String::new(), result: None }
    }

    pub fn poll(&mut self) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                JobMsg::Progress { done, total, name } => (self.done, self.total, self.name) = (done, total, name),
                JobMsg::Finished(r) => self.result = Some(r),
            }
        }
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    /// The progress window (第 n / 總數、檔名、取消).
    pub fn show(&mut self, ctx: &egui::Context) {
        egui::Window::new("匯出中").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            ui.set_width(420.0);
            let n = (self.done + 1).min(self.total.max(1));
            ui.label(format!("第 {n} / {} 張　{}", self.total, self.name));
            ui.add(egui::ProgressBar::new(self.done as f32 / self.total.max(1) as f32).show_percentage());
            ui.add_space(4.0);
            let label = if self.cancelled() { "取消中…（目前這張完成後停止）" } else { "取消" };
            if ui.add_enabled(!self.cancelled(), egui::Button::new(label)).clicked() {
                self.cancel.store(true, Ordering::SeqCst);
            }
        });
    }
}

// ---- live watermark on the viewer -------------------------------------------------------

/// The watermark as a texture for the viewer, re-rasterised only when it changes.
#[derive(Default)]
pub struct WatermarkOverlay {
    key: String,
    tex: Option<(egui::TextureHandle, f64, f64, usize, usize)>,
}

impl WatermarkOverlay {
    /// Paint the watermark over the displayed frame. `image` = the frame's screen rect,
    /// `frame` its size in pixels, `ratio` = preview pixels per full-resolution pixel,
    /// `scale_px` = physical pixels per frame pixel.
    pub fn paint(&mut self, ui: &egui::Ui, painter: &egui::Painter, s: &ExportSettings, image: egui::Rect, frame: (usize, usize), ratio: f64, scale_px: f64) {
        let spec = s.watermark();
        if !spec.is_visible() || ratio <= 0.0 || scale_px <= 0.0 {
            return;
        }
        let size_frame = (spec.font_size * ratio).max(4.0);
        let size_px = size_frame * scale_px;
        let key = format!("{}|{}|{:.2}|{:?}|{}", spec.text, spec.font_name, size_px, spec.color, spec.transparency);
        if key != self.key || self.tex.is_none() {
            self.key = key;
            self.tex = watermark::render_text(&spec.text, &spec.font_name, size_px).map(|m| {
                let [r, g, b] = spec.color.rgb();
                let a = spec.opacity();
                let pixels: Vec<Color32> = m.coverage.iter().map(|&c| Color32::from_rgba_unmultiplied(r, g, b, (c * a * 255.0).round() as u8)).collect();
                let img = egui::ColorImage { size: [m.width, m.height], pixels, source_size: Vec2::new(m.width as f32, m.height as f32) };
                let tex = ui.ctx().load_texture("watermark", img, egui::TextureOptions::LINEAR);
                (tex, m.box_w, m.box_h, m.box_x, m.box_y)
            });
        }
        let Some((tex, box_w, box_h, box_x, box_y)) = &self.tex else { return };
        // Position in frame pixels, exactly as the export computes it at its own size.
        let margin = awpr_core::buffer::round_half_even(spec.margin as f64 * ratio);
        let (x, y) = watermark::layout_origin(frame.0, frame.1, box_w / scale_px, box_h / scale_px, spec.position, margin);
        let pt_per_frame_px = image.width() as f64 / frame.0 as f64;
        let ppp = ui.ctx().pixels_per_point() as f64;
        let min = egui::pos2(
            (image.min.x as f64 + x * pt_per_frame_px - *box_x as f64 / ppp) as f32,
            (image.min.y as f64 + y * pt_per_frame_px - *box_y as f64 / ppp) as f32,
        );
        let size = tex.size_vec2() / ppp as f32;
        painter.image(tex.id(), egui::Rect::from_min_size(min, size), egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
    }
}
