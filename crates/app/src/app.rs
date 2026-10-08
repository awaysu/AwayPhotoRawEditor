//! The main window: folder → thumbnail strip → photo, with the C# layout (top bar,
//! strip, left adjustments, centre viewer, right histogram / info / tools).

use crate::settings::Settings;
use crate::theme;
use crate::viewer::{self, DisplayPipeline, Placement, ViewerState, ZoomMode};
use crate::widgets::{self, Gradient, Histogram, SliderSpec};
use crate::worker::{self, Item, Msg, Worker};
use awpr_core::pipeline::ProcessContext;
use awpr_core::{color, FloatImage, ImageAdjustments, Rotation};
use awpr_gpu::{GpuFrame, GpuPipeline, HistogramJob};
use awpr_photo::loader::DecodeSource;
use awpr_photo::store::{self, PreviewList};
use awpr_photo::{paths, ExifData};
use eframe::egui::{self, Color32, Key, RichText, Vec2};
use eframe::egui_wgpu;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// What the viewer currently shows.
enum Display {
    None,
    /// A frame still on the GPU, drawn from its buffer.
    Gpu { frame: GpuFrame<'static>, bind: Arc<wgpu::BindGroup> },
    /// The CPU fallback's result as a texture.
    Cpu { tex: egui::TextureHandle, size: Vec2 },
}

/// `--shot`: open a folder, wait for the photo, save a screenshot, quit. For checking the
/// UI on machines nobody is sitting at; never writes settings or edits.
pub struct ShotPlan {
    pub folder: String,
    pub out: String,
    pub select: usize,
    /// "exposure=0.5,contrast=20,..." applied in memory after load.
    pub adjust: Option<String>,
    pub started: Instant,
    pub requested: bool,
}

/// Points of height the full layout needs (top bar + strip + left column + margins);
/// the C# build's `Ui.DesignClientHeight`.
const DESIGN_HEIGHT: f32 = 1010.0;

pub struct App {
    zoom_fitted: bool,
    rs: Option<egui_wgpu::RenderState>,
    gpu: Option<&'static GpuPipeline>,
    gpu_status: String,
    font: Option<String>,
    settings: Settings,
    shot: Option<ShotPlan>,
    worker: Worker,
    rx: Receiver<Msg>,

    folder: String,
    preview_list: PreviewList,
    items: Vec<Item>,
    current: Option<usize>,
    scroll_to_current: bool,
    thumbs: HashMap<String, (egui::TextureHandle, u64)>,
    thumb_version: u64,
    thumb_live_due: Option<Instant>,
    cache_progress: Option<(usize, usize, String)>,

    load_version: u64,
    loading: bool,
    adj: ImageAdjustments,
    saved_adj: ImageAdjustments,
    exif: Option<ExifData>,
    source: DecodeSource,
    proxy: Option<Arc<FloatImage>>,
    proxy_gpu: Option<GpuFrame<'static>>,
    display: Display,
    needs_render: bool,
    render_version: u64,
    cpu_inflight: bool,
    hist: Option<Histogram>,
    hist_job: Option<(HistogramJob, Instant)>,
    viewer: ViewerState,
    show_original: bool,
    undo: Vec<ImageAdjustments>,
    redo: Vec<ImageAdjustments>,
    status: String,
    render_note: String,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, shot: Option<ShotPlan>) -> Self {
        let font = theme::install(&cc.egui_ctx);
        let mut settings = Settings::load();
        if std::env::var_os("AWPR_NO_GPU").is_some() {
            settings.use_gpu = false;
        }
        let rs = cc.wgpu_render_state.clone();
        let (mut gpu, mut gpu_status) = (None, "沒有 GPU".to_string());
        if let Some(rs) = &rs {
            DisplayPipeline::install(rs);
            crate::trace(&format!("adapter {:?}, surface format {:?}", rs.adapter.get_info().name, rs.target_format));
            if settings.use_gpu {
                match GpuPipeline::with_device(&rs.adapter.get_info(), rs.device.clone(), rs.queue.clone()) {
                    Ok(p) => {
                        gpu_status = p.status.clone();
                        // Lives as long as the program; frames borrow it.
                        gpu = Some(&*Box::leak(Box::new(p)));
                    }
                    Err(e) => gpu_status = format!("GPU 無法使用，改用 CPU：{e}"),
                }
            } else {
                gpu_status = "已關閉 GPU 加速（CPU 算圖）".into();
            }
        }
        let (worker, rx) = Worker::new(cc.egui_ctx.clone());
        let mut app = Self {
            zoom_fitted: false,
            rs,
            gpu,
            gpu_status,
            font,
            settings,
            shot,
            worker,
            rx,
            folder: String::new(),
            preview_list: PreviewList::default(),
            items: Vec::new(),
            current: None,
            scroll_to_current: false,
            thumbs: HashMap::new(),
            thumb_version: 1,
            thumb_live_due: None,
            cache_progress: None,
            load_version: 0,
            loading: false,
            adj: ImageAdjustments::default(),
            saved_adj: ImageAdjustments::default(),
            exif: None,
            source: DecodeSource::LibRaw,
            proxy: None,
            proxy_gpu: None,
            display: Display::None,
            needs_render: false,
            render_version: 0,
            cpu_inflight: false,
            hist: None,
            hist_job: None,
            viewer: ViewerState::default(),
            show_original: false,
            undo: Vec::new(),
            redo: Vec::new(),
            status: String::new(),
            render_note: String::new(),
        };
        if let Some(s) = &app.shot {
            let f = s.folder.clone();
            app.open_folder(&f);
        } else if !app.settings.last_folder.is_empty() && std::path::Path::new(&app.settings.last_folder).is_dir() {
            let f = app.settings.last_folder.clone();
            app.open_folder(&f);
        }
        app
    }

    fn headless(&self) -> bool {
        self.shot.is_some()
    }

    fn has_photo(&self) -> bool {
        self.current.is_some() && self.proxy.is_some()
    }

    // ---- folder -------------------------------------------------------------------

    fn pick_folder(&mut self) {
        let mut dlg = rfd::FileDialog::new().set_title("選擇相片資料夾");
        if !self.folder.is_empty() {
            dlg = dlg.set_directory(&self.folder);
        }
        if let Some(p) = dlg.pick_folder() {
            let mut p = p.to_string_lossy().into_owned();
            // Picking RAW_TEMP itself means its photo folder.
            if paths::file_name(&p).eq_ignore_ascii_case(paths::RAW_TEMP) {
                if let Some(parent) = std::path::Path::new(&p).parent() {
                    p = parent.to_string_lossy().into_owned();
                }
            }
            self.open_folder(&p);
        }
    }

    fn open_folder(&mut self, path: &str) {
        self.save_current_if_dirty();
        self.worker.folder_gen.fetch_add(1, Ordering::SeqCst);
        self.clear_editor();
        self.folder = path.to_string();
        if !self.headless() {
            self.settings.last_folder = path.to_string();
            self.settings.save();
        }
        paths::cleanup_stale_temp(path);
        self.preview_list = PreviewList::load(path);
        self.thumbs.clear();
        self.rebuild_items();
        if self.items.is_empty() {
            self.status = "此資料夾沒有支援的影像".into();
            self.cache_progress = None;
            return;
        }
        self.cache_progress = Some((0, self.items.len(), String::new()));
        for it in &self.items {
            self.worker.thumbnail(it, None, 0);
        }
        self.worker.generate_caches(self.items.clone(), self.settings.loader_options());
        let first = self.shot.as_ref().map(|s| s.select).unwrap_or(0).min(self.items.len() - 1);
        self.select(first);
    }

    fn close_folder(&mut self) {
        self.save_current_if_dirty();
        self.worker.folder_gen.fetch_add(1, Ordering::SeqCst);
        self.folder.clear();
        self.items.clear();
        self.thumbs.clear();
        self.cache_progress = None;
        self.clear_editor();
        if !self.headless() {
            // Closed on purpose: the next launch stays closed.
            self.settings.last_folder.clear();
            self.settings.save();
        }
    }

    /// Strip items from disk plus preview_list.xml (hidden photos, virtual copies).
    fn rebuild_items(&mut self) {
        let files = paths::images_in_folder(&self.folder);
        let mut all: Vec<(String, i32)> = files.iter().map(|f| (f.clone(), 0)).collect();
        let mut copies = self.preview_list.virtual_copies.clone();
        copies.sort_by_key(|e| e.index);
        for e in copies {
            // Keys hold the absolute path the writing machine used; match by file name
            // so a folder moved or opened from another OS keeps its copies.
            let name = paths::file_name(&e.path.replace('\\', "/"));
            if let Some(at) = all.iter().rposition(|(p, _)| paths::file_name(p) == name) {
                let p = all[at].0.clone();
                all.insert(at + 1, (p, e.index));
            }
        }
        let hidden: std::collections::HashSet<String> = self
            .preview_list
            .hidden
            .iter()
            .map(|k| {
                let (p, n) = store::parse_key(k);
                format!("{}|{n}", paths::file_name(&p.replace('\\', "/")))
            })
            .collect();
        let mut items = Vec::new();
        for (i, (p, copy)) in all.into_iter().enumerate() {
            let is_hidden = hidden.contains(&format!("{}|{copy}", paths::file_name(&p)));
            if is_hidden && !self.settings.show_hidden {
                continue;
            }
            let (a, _, placeholder) = store::load_all(&p, copy);
            let edited = !(placeholder || a.as_ref().is_none_or(store::is_default));
            items.push(Item { key: store::make_key(&p, copy), path: p, copy, number: i + 1, hidden: is_hidden, edited });
        }
        self.items = items;
    }

    // ---- photo --------------------------------------------------------------------

    fn select(&mut self, index: usize) {
        if index >= self.items.len() || self.current == Some(index) {
            return;
        }
        self.save_current_if_dirty();
        self.clear_photo();
        self.current = Some(index);
        self.scroll_to_current = true;
        self.loading = true;
        self.load_version += 1;
        let item = self.items[index].clone();
        self.status = format!("載入 {}…", item.name());
        self.worker.load_photo(item, self.load_version, self.settings.loader_options());
    }

    fn clear_photo(&mut self) {
        self.proxy = None;
        self.proxy_gpu = None;
        self.display = Display::None;
        self.hist = None;
        self.hist_job = None;
        self.exif = None;
        self.undo.clear();
        self.redo.clear();
        self.show_original = false;
        self.needs_render = false;
        self.adj = ImageAdjustments::default();
        self.saved_adj = self.adj.clone();
    }

    fn clear_editor(&mut self) {
        self.clear_photo();
        self.current = None;
        self.loading = false;
    }

    fn on_loaded(&mut self, l: worker::Loaded) {
        if l.version != self.load_version {
            return; // superseded by a later selection
        }
        self.loading = false;
        self.adj = l.adjustments;
        self.saved_adj = self.adj.clone();
        self.exif = Some(l.exif);
        self.source = l.source;
        self.viewer.reset_fit();
        let Some(proxy) = l.proxy else {
            self.status = "無法讀取這張照片".into();
            return;
        };
        if let Some(gpu) = self.gpu {
            if gpu.can_host(proxy.width, proxy.height) {
                match gpu.upload(&proxy) {
                    Ok(f) => self.proxy_gpu = Some(f),
                    Err(e) => self.status = format!("GPU 上傳失敗，改用 CPU：{e}"),
                }
            }
        }
        self.status = format!("{} · {} x {} · 載入 {} ms", self.items[self.current.unwrap_or(0)].name(), proxy.width, proxy.height, l.millis);
        self.proxy = Some(Arc::new(proxy));
        self.needs_render = true;
        if let Some(s) = &self.shot {
            if let Some(spec) = s.adjust.clone() {
                apply_adjust_spec(&mut self.adj, &spec);
            }
            match std::env::var("AWPR_SHOT_ZOOM").as_deref() {
                Ok("100") => self.viewer.set_mode(ZoomMode::Actual100),
                Ok("200") => self.viewer.set_mode(ZoomMode::Actual200),
                _ => {}
            }
        }
        // The load may just have generated the proxy (and its strip thumbnail).
        if let Some(i) = self.current {
            self.thumb_version += 1;
            self.worker.thumbnail(&self.items[i], Some(self.adj.clone()), self.thumb_version);
        }
    }

    fn save_current_if_dirty(&mut self) {
        let Some(i) = self.current else { return };
        if self.headless() || self.proxy.is_none() || store::value_equals(&self.adj, &self.saved_adj) {
            return;
        }
        let item = self.items[i].clone();
        if let Err(e) = store::save(&item.path, &self.adj, item.copy, None) {
            self.status = format!("無法儲存編輯：{e}");
            return;
        }
        self.saved_adj = self.adj.clone();
        self.items[i].edited = !store::is_default(&self.adj);
    }

    // ---- editing ------------------------------------------------------------------

    /// An edit gesture starts: snapshot for undo.
    fn edit_begin(&mut self) {
        self.undo.push(self.adj.clone());
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.show_original = false;
    }

    fn edited(&mut self) {
        self.needs_render = true;
        self.thumb_live_due = Some(Instant::now() + Duration::from_millis(200));
        if let Some(i) = self.current {
            self.items[i].edited = !store::is_default(&self.adj);
        }
    }

    fn do_undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.adj, prev));
            self.edited();
        }
    }

    fn do_redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.adj, next));
            self.edited();
        }
    }

    fn reset_basic_color_detail(&mut self) {
        self.edit_begin();
        let d = ImageAdjustments::default();
        let a = &mut self.adj;
        (a.exposure, a.contrast, a.highlights, a.shadows, a.whites, a.blacks) = (d.exposure, d.contrast, d.highlights, d.shadows, d.whites, d.blacks);
        (a.temperature, a.tint, a.vibrance, a.saturation) = (d.temperature, d.tint, d.vibrance, d.saturation);
        (a.sharpening, a.noise_reduction, a.vignette) = (d.sharpening, d.noise_reduction, d.vignette);
        self.edited();
    }

    fn reset_all(&mut self) {
        self.edit_begin();
        let v = self.adj.pipeline_version;
        self.adj = ImageAdjustments { pipeline_version: v, ..Default::default() };
        self.edited();
    }

    /// 拍攝時設定: the as-shot white balance.
    fn as_shot(&mut self) {
        let shot = self.exif.as_ref().and_then(|e| {
            e.camera.as_ref().and_then(color::as_shot).or_else(|| e.has_as_shot_white_balance().then_some((e.color_temperature, 0.0)))
        });
        if let Some((k, t)) = shot {
            self.edit_begin();
            self.adj.temperature = k.clamp(color::MIN_KELVIN, color::MAX_KELVIN);
            self.adj.tint = t;
            self.edited();
        }
    }

    fn rotate(&mut self, clockwise: bool) {
        self.edit_begin();
        self.adj.rotation = match (self.adj.rotation, clockwise) {
            (Rotation::R0, true) | (Rotation::R180, false) => Rotation::R90,
            (Rotation::R90, true) | (Rotation::R270, false) => Rotation::R180,
            (Rotation::R180, true) | (Rotation::R0, false) => Rotation::R270,
            (Rotation::R270, true) | (Rotation::R90, false) => Rotation::R0,
        };
        self.edited();
    }

    // ---- rendering ----------------------------------------------------------------

    fn process_context(&self) -> ProcessContext {
        ProcessContext {
            camera: self.exif.as_ref().and_then(|e| e.camera.clone()),
            white_balance_reference: self.source.white_balance_reference(),
            ..Default::default()
        }
    }

    /// The adjustments actually rendered: 對照原圖 swaps in neutral values but keeps the
    /// geometry, so the comparison lines up.
    fn render_adjustments(&self) -> ImageAdjustments {
        if !self.show_original {
            return self.adj.clone();
        }
        let a = &self.adj;
        ImageAdjustments {
            pipeline_version: a.pipeline_version,
            distortion: a.distortion,
            crop_aspect_ratio: a.crop_aspect_ratio.clone(),
            crop_angle: a.crop_angle,
            crop_x: a.crop_x,
            crop_y: a.crop_y,
            crop_width: a.crop_width,
            crop_height: a.crop_height,
            rotation: a.rotation,
            ..Default::default()
        }
    }

    fn render(&mut self) {
        self.needs_render = false;
        let adj = self.render_adjustments();
        let ctx = self.process_context();
        if let (Some(gpu), Some(src), Some(rs)) = (self.gpu, self.proxy_gpu.as_ref(), self.rs.as_ref()) {
            let t0 = Instant::now();
            match gpu.render(src, &adj, &ctx) {
                Ok(frame) => {
                    let bind = Arc::new(DisplayPipeline::bind(rs, frame.buffer()));
                    self.hist_job = Some((gpu.histogram(&frame), t0));
                    self.display = Display::Gpu { frame, bind };
                    return;
                }
                Err(e) => {
                    self.status = format!("GPU 算圖失敗，改用 CPU：{e}");
                    self.proxy_gpu = None;
                }
            }
        }
        let Some(proxy) = self.proxy.clone() else { return };
        if self.cpu_inflight {
            // One CPU render at a time; the latest values go next.
            self.needs_render = true;
            return;
        }
        self.cpu_inflight = true;
        self.render_version += 1;
        self.render_note = "CPU 算圖中…".into();
        self.worker.cpu_render(proxy, adj, ctx, self.render_version);
    }

    // ---- messages ------------------------------------------------------------------

    fn drain(&mut self, ctx: &egui::Context) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Thumb { key, image, version } => {
                    if self.thumbs.get(&key).is_some_and(|(_, v)| *v > version) {
                        continue;
                    }
                    let tex = ctx.load_texture(format!("thumb:{key}"), image, egui::TextureOptions::LINEAR);
                    self.thumbs.insert(key, (tex, version));
                }
                Msg::Loaded(l) => self.on_loaded(*l),
                Msg::CacheProgress { gen, done, total, name } => {
                    if gen == self.worker.folder_gen.load(Ordering::SeqCst) {
                        self.cache_progress = if done >= total { None } else { Some((done, total, name)) };
                    }
                }
                Msg::CpuRendered { version, image } => {
                    self.cpu_inflight = false;
                    if version == self.render_version && self.proxy.is_some() {
                        self.hist = Some(Histogram::from_float(&image.data));
                        let size = Vec2::new(image.width as f32, image.height as f32);
                        // Exact pixels from 100 % up (judging sharpness), smoothed below.
                        let filter = if matches!(self.viewer.mode, ZoomMode::Actual100 | ZoomMode::Actual200) { egui::TextureOptions::NEAREST } else { egui::TextureOptions::LINEAR };
                        let tex = ctx.load_texture("photo", worker::to_color_image(&image), filter);
                        self.display = Display::Cpu { tex, size };
                        self.render_note = "CPU 算圖".into();
                    }
                }
            }
        }
        if let Some(gpu) = self.gpu {
            gpu.poll();
            if let Some((job, t0)) = &self.hist_job {
                if let Some(bins) = job.take() {
                    // Submit → histogram ready: how long the GPU took for this render.
                    self.render_note = format!("GPU 算圖 {} ms", t0.elapsed().as_millis());
                    self.hist = Some(Histogram { bins });
                    self.hist_job = None;
                } else {
                    ctx.request_repaint();
                }
            }
        }
        if let Some(due) = self.thumb_live_due {
            let now = Instant::now();
            if now >= due {
                self.thumb_live_due = None;
                if let Some(i) = self.current {
                    self.thumb_version += 1;
                    self.worker.thumbnail(&self.items[i], Some(self.adj.clone()), self.thumb_version);
                }
            } else {
                ctx.request_repaint_after(due - now);
            }
        }
    }

    fn keyboard(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || self.headless() {
            return;
        }
        let (left, right, undo, redo, compare, refresh, open) = ctx.input(|i| {
            let cmd = i.modifiers.command;
            (
                i.key_pressed(Key::ArrowLeft),
                i.key_pressed(Key::ArrowRight),
                cmd && i.key_pressed(Key::Z) && !i.modifiers.shift,
                cmd && (i.key_pressed(Key::Y) || (i.modifiers.shift && i.key_pressed(Key::Z))),
                i.key_pressed(Key::Backslash),
                i.key_pressed(Key::F5),
                cmd && i.key_pressed(Key::O),
            )
        });
        if let Some(c) = self.current {
            if left && c > 0 {
                self.select(c - 1);
            }
            if right && c + 1 < self.items.len() {
                self.select(c + 1);
            }
        }
        if undo {
            self.do_undo();
        }
        if redo {
            self.do_redo();
        }
        if compare && self.has_photo() {
            self.show_original = !self.show_original;
            self.needs_render = true;
        }
        if refresh && !self.folder.is_empty() {
            let f = self.folder.clone();
            self.open_folder(&f);
        }
        if open {
            self.pick_folder();
        }
    }

    // ---- layout -------------------------------------------------------------------

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.add_space(8.0);
            ui.label(RichText::new("AwayPhotoRawEditor").strong().size(20.0));
            ui.label(RichText::new("Rust 預覽版").size(12.0).color(theme::TEXT_FAINT));
            ui.add_space(16.0);
            if ui.button("📁 開啟資料夾").clicked() {
                self.pick_folder();
            }
            let has_folder = !self.folder.is_empty();
            if ui.add_enabled(has_folder, egui::Button::new("重新整理")).clicked() {
                let f = self.folder.clone();
                self.open_folder(&f);
            }
            if ui.add_enabled(has_folder, egui::Button::new("關閉資料夾")).clicked() {
                self.close_folder();
            }
            ui.add_space(10.0);
            ui.label(RichText::new(&self.folder).color(theme::TEXT_DIM));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(10.0);
                ui.label(RichText::new(&self.gpu_status).size(12.0).color(theme::TEXT_FAINT));
                if let Some((done, total, name)) = &self.cache_progress {
                    ui.add(egui::ProgressBar::new(*done as f32 / (*total).max(1) as f32).desired_width(140.0).show_percentage());
                    ui.label(RichText::new(format!("產生快取 {done}/{total} {name}")).size(12.0).color(theme::TEXT_DIM));
                }
            });
        });
    }

    fn strip(&mut self, ui: &mut egui::Ui) {
        if self.items.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("開啟一個相片資料夾開始編輯（Ctrl+O）").color(theme::TEXT_FAINT));
            });
            return;
        }
        let mut clicked = None;
        let scroll_to = std::mem::take(&mut self.scroll_to_current);
        egui::ScrollArea::horizontal().auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                for (i, it) in self.items.iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(176.0, 136.0), egui::Sense::click());
                    if ui.is_rect_visible(rect) {
                        let p = ui.painter();
                        let selected = self.current == Some(i);
                        p.rect_filled(rect, 3.0, if selected { Color32::from_rgb(0x2F, 0x3E, 0x52) } else { theme::WINDOW });
                        let img_area = egui::Rect::from_min_size(rect.min + Vec2::new(2.0, 2.0), Vec2::new(172.0, 115.0));
                        if let Some((tex, _)) = self.thumbs.get(&it.key) {
                            let s = tex.size_vec2();
                            let k = (img_area.width() / s.x).min(img_area.height() / s.y);
                            let r = egui::Rect::from_center_size(img_area.center(), s * k);
                            p.image(tex.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), if it.hidden { Color32::from_gray(110) } else { Color32::WHITE });
                        } else {
                            p.rect_filled(img_area, 2.0, theme::PANEL);
                        }
                        let tag = p.layout_no_wrap(format!("#{}", it.number), egui::FontId::proportional(12.0), Color32::WHITE);
                        let tag_rect = egui::Rect::from_min_size(img_area.min, tag.size() + Vec2::new(8.0, 2.0));
                        p.rect_filled(tag_rect, 2.0, Color32::from_black_alpha(150));
                        p.galley(tag_rect.min + Vec2::new(4.0, 1.0), tag, Color32::WHITE);
                        if it.edited {
                            p.circle_filled(egui::pos2(img_area.max.x - 8.0, img_area.min.y + 8.0), 4.5, theme::EDITED);
                        }
                        p.text(
                            egui::pos2(rect.center().x, rect.max.y - 9.0),
                            egui::Align2::CENTER_CENTER,
                            truncate(&it.name(), 26),
                            egui::FontId::proportional(11.5),
                            if selected { theme::TEXT } else { theme::TEXT_DIM },
                        );
                        if selected {
                            p.rect_stroke(rect, 3.0, egui::Stroke::new(2.0, theme::ACCENT), egui::StrokeKind::Inside);
                        }
                    }
                    if resp.clicked() {
                        clicked = Some(i);
                    }
                    if scroll_to && self.current == Some(i) {
                        ui.scroll_to_rect(rect, Some(egui::Align::Center));
                    }
                }
            });
        });
        if let Some(i) = clicked {
            self.select(i);
        }
    }

    fn slider(&mut self, ui: &mut egui::Ui, spec: SliderSpec, get: fn(&mut ImageAdjustments) -> &mut f64) {
        let enabled = self.has_photo();
        let mut v = *get(&mut self.adj);
        let r = widgets::adjust_slider(ui, &spec, &mut v, enabled);
        if r.began {
            self.edit_begin();
        }
        if r.changed {
            *get(&mut self.adj) = v;
            self.edited();
        }
    }

    fn left_column(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        let legacy = self.adj.is_legacy_pipeline();
        theme::section(ui, "基本調整", |ui| {
            // v1 exposure is ±5 true EV; legacy photos keep their ±2 slider.
            let lim = if legacy { 2.0 } else { 5.0 };
            let exposure = SliderSpec { min: -lim, max: lim, decimals: 2, wheel_step: 0.05, ..SliderSpec::pm100("曝光") };
            self.slider(ui, exposure, |a| &mut a.exposure);
            self.slider(ui, SliderSpec::pm100("對比"), |a| &mut a.contrast);
            self.slider(ui, SliderSpec::pm100("亮部"), |a| &mut a.highlights);
            self.slider(ui, SliderSpec::pm100("暗部"), |a| &mut a.shadows);
            self.slider(ui, SliderSpec::pm100("白色"), |a| &mut a.whites);
            self.slider(ui, SliderSpec::pm100("黑色"), |a| &mut a.blacks);
        });
        ui.add_space(4.0);
        theme::section(ui, "色彩", |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("白平衡").color(theme::TEXT_DIM));
                if ui.add_enabled(self.has_photo(), egui::Button::new("拍攝時設定")).clicked() {
                    self.as_shot();
                }
            });
            let is_raw = self.current.is_none_or(|i| paths::is_raw(&self.items[i].path));
            if is_raw {
                let t = SliderSpec {
                    min: color::MIN_KELVIN,
                    max: color::MAX_KELVIN,
                    default: 5200.0,
                    bipolar: false,
                    wheel_step: 50.0,
                    ..SliderSpec::pm100("色溫")
                }
                .gradient(Gradient::Temperature);
                self.slider(ui, t, |a| &mut a.temperature);
            } else {
                // No camera Kelvin scale: a 0-centred ±100 warm/cool scale (±3000 K).
                let enabled = self.has_photo();
                let mut v = (self.adj.temperature - 5200.0) / 30.0;
                let spec = SliderSpec::pm100("色溫").gradient(Gradient::Temperature);
                let r = widgets::adjust_slider(ui, &spec, &mut v, enabled);
                if r.began {
                    self.edit_begin();
                }
                if r.changed {
                    self.adj.temperature = 5200.0 + v * 30.0;
                    self.edited();
                }
            }
            self.slider(ui, SliderSpec::pm100("色調").gradient(Gradient::Tint), |a| &mut a.tint);
            self.slider(ui, SliderSpec::pm100("鮮豔度").gradient(Gradient::Saturation), |a| &mut a.vibrance);
            self.slider(ui, SliderSpec::pm100("飽和度").gradient(Gradient::Saturation), |a| &mut a.saturation);
        });
        ui.add_space(4.0);
        theme::section(ui, "細節", |ui| {
            self.slider(ui, SliderSpec::pm100("銳利度"), |a| &mut a.sharpening);
            self.slider(ui, SliderSpec::pm100("暗角"), |a| &mut a.vignette);
            let nr = SliderSpec { min: 0.0, bipolar: false, ..SliderSpec::pm100("降噪") };
            self.slider(ui, nr, |a| &mut a.noise_reduction);
        });
        ui.add_space(8.0);
        if ui.add_enabled(self.has_photo(), egui::Button::new("基本／色彩／細節 重設").min_size(Vec2::new(ui.available_width(), 30.0))).clicked() {
            self.reset_basic_color_detail();
        }
    }

    fn right_column(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        theme::section(ui, "直方圖", |ui| widgets::histogram(ui, self.hist.as_ref()));
        ui.add_space(4.0);
        theme::section(ui, "照片資訊", |ui| match &self.exif {
            None => {
                ui.label(RichText::new("尚未選擇照片").color(theme::TEXT_FAINT));
            }
            Some(e) => {
                let rows = [
                    ("相機", format!("{} {}", e.camera_make, e.camera_model).trim().to_string()),
                    ("鏡頭", e.lens.clone()),
                    ("ISO", e.iso.clone()),
                    ("光圈", e.aperture.clone()),
                    ("快門", e.shutter.clone()),
                    ("焦段", e.focal_length.clone()),
                    ("曝光補償", e.exposure_bias.clone()),
                    ("白平衡", e.white_balance.clone()),
                    ("測光", e.metering_mode.clone()),
                    ("日期", e.date_taken.clone()),
                    ("尺寸", e.dimensions_display()),
                    ("檔案大小", e.file_size_display()),
                ];
                egui::Grid::new("exif").num_columns(2).spacing([10.0, 3.0]).show(ui, |ui| {
                    for (k, v) in rows {
                        ui.label(RichText::new(k).size(12.5).color(theme::TEXT_DIM));
                        ui.add(egui::Label::new(v).truncate());
                        ui.end_row();
                    }
                });
                if self.adj.is_legacy_pipeline() {
                    ui.label(RichText::new("· 舊版處理").size(12.0).color(theme::EDITED));
                }
            }
        });
        ui.add_space(4.0);
        theme::section(ui, "工具", |ui| {
            let on = self.has_photo();
            ui.horizontal(|ui| {
                if ui.add_enabled(on, egui::Button::new("⟲ 左轉")).clicked() {
                    self.rotate(false);
                }
                if ui.add_enabled(on, egui::Button::new("⟳ 右轉")).clicked() {
                    self.rotate(true);
                }
            });
            self.slider(ui, SliderSpec::pm100("廣角變形"), |a| &mut a.distortion);
            let n_grad = self.adj.gradients.len();
            let n_heal = self.adj.heal_spots.len();
            let cropped = self.adj.crop_width < 1.0 || self.adj.crop_height < 1.0 || self.adj.crop_angle != 0.0;
            ui.label(
                RichText::new(format!(
                    "裁切{}、漸層 {n_grad} 個、修護 {n_heal} 點（照常套用；互動編輯在下一步）",
                    if cropped { "已設定" } else { "無" }
                ))
                .size(12.0)
                .color(theme::TEXT_FAINT),
            );
        });
    }

    fn right_bottom(&mut self, ui: &mut egui::Ui) {
        let on = self.has_photo();
        let w = ui.available_width();
        ui.add_space(6.0);
        if ui.add_enabled(on, egui::Button::new("全部重設").min_size(Vec2::new(w, 30.0))).clicked() {
            self.reset_all();
        }
        ui.horizontal(|ui| {
            let half = (w - 6.0) / 2.0;
            if ui.add_enabled(on && !self.undo.is_empty(), egui::Button::new("恢復上一步").min_size(Vec2::new(half, 30.0))).clicked() {
                self.do_undo();
            }
            if ui.add_enabled(on && !self.redo.is_empty(), egui::Button::new("重做").min_size(Vec2::new(half, 30.0))).clicked() {
                self.do_redo();
            }
        });
    }

    fn viewer_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.add_space(6.0);
            let on = self.has_photo();
            for (label, mode) in [("適合", ZoomMode::Fit), ("100%", ZoomMode::Actual100), ("200%", ZoomMode::Actual200)] {
                if ui.add_enabled(on, egui::Button::new(label).selected(self.viewer.mode == mode)).clicked() {
                    self.viewer.set_mode(mode);
                }
            }
            if ui.add_enabled(on, egui::Button::new("對照原圖").selected(self.show_original)).clicked() {
                self.show_original = !self.show_original;
                self.needs_render = true;
            }
            if on {
                ui.label(RichText::new(format!("{:.0}%", self.viewer.zoom_percent())).color(theme::TEXT_DIM));
            }
            ui.add_space(12.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                ui.label(RichText::new(&self.render_note).size(12.0).color(theme::TEXT_FAINT));
                ui.add(egui::Label::new(RichText::new(&self.status).size(12.0).color(theme::TEXT_DIM)).truncate());
            });
        });
    }

    fn viewer(&mut self, ui: &mut egui::Ui) {
        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        ui.painter().rect_filled(rect, 0.0, theme::VIEWER);
        let size = match &self.display {
            Display::None => None,
            Display::Gpu { frame, .. } => Some(Vec2::new(frame.width as f32, frame.height as f32)),
            Display::Cpu { size, .. } => Some(*size),
        };
        let Some(size) = size else {
            let msg = if self.loading { "載入中…" } else if self.items.is_empty() { "" } else { "沒有可顯示的照片" };
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, msg, egui::FontId::proportional(16.0), theme::TEXT_FAINT);
            return;
        };
        let pl: Placement = self.viewer.update(ui, &resp, size);
        match &self.display {
            Display::Gpu { bind, .. } => viewer::paint_gpu(ui, &pl, bind, size),
            Display::Cpu { tex, .. } => viewer::paint_texture(ui, &pl, tex),
            Display::None => {}
        }
        if self.show_original {
            ui.painter().text(rect.left_top() + Vec2::new(12.0, 10.0), egui::Align2::LEFT_TOP, "原圖", egui::FontId::proportional(14.0), Color32::WHITE);
        }
    }

    // ---- --shot -------------------------------------------------------------------

    fn shot_step(&mut self, ctx: &egui::Context) {
        let Some(s) = &mut self.shot else { return };
        let shot_event = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = shot_event {
            crate::trace("screenshot event");
            let [w, h] = img.size;
            let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            let r = image::save_buffer(&s.out, &rgba, w as u32, h as u32, image::ExtendedColorType::Rgba8);
            eprintln!("shot: {} ({w}x{h}) {:?}", s.out, r.err());
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let ready = !self.loading && self.hist.is_some() && self.cache_progress.is_none() && !self.needs_render && self.thumb_live_due.is_none();
        let waited = s.started.elapsed();
        if !s.requested && ((ready && waited > Duration::from_millis(1500)) || waited > Duration::from_secs(120)) {
            s.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n - 1).collect::<String>() + "…"
    }
}

/// "exposure=0.5,contrast=20" → the adjustments (shot mode only).
fn apply_adjust_spec(a: &mut ImageAdjustments, spec: &str) {
    for part in spec.split(',') {
        let Some((k, v)) = part.split_once('=') else { continue };
        let Ok(v) = v.trim().parse::<f64>() else { continue };
        match k.trim() {
            "exposure" => a.exposure = v,
            "contrast" => a.contrast = v,
            "highlights" => a.highlights = v,
            "shadows" => a.shadows = v,
            "whites" => a.whites = v,
            "blacks" => a.blacks = v,
            "temperature" => a.temperature = v,
            "tint" => a.tint = v,
            "vibrance" => a.vibrance = v,
            "saturation" => a.saturation = v,
            "sharpening" => a.sharpening = v,
            "nr" => a.noise_reduction = v,
            "vignette" => a.vignette = v,
            "distortion" => a.distortion = v,
            "rotate" => {
                a.rotation = match v as i32 {
                    90 => Rotation::R90,
                    180 => Rotation::R180,
                    270 => Rotation::R270,
                    _ => Rotation::R0,
                }
            }
            _ => {}
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if !self.zoom_fitted {
            if let Some(m) = ctx.input(|i| i.viewport().monitor_size) {
                self.zoom_fitted = true;
                let native_ppp = ctx.pixels_per_point() / ctx.zoom_factor();
                // monitor_size is in points at the current zoom; convert to native points.
                let height = m.y * ctx.zoom_factor();
                let zoom = (height / DESIGN_HEIGHT).clamp(0.6, 1.0);
                if (zoom - ctx.zoom_factor()).abs() > 0.01 {
                    ctx.set_zoom_factor(zoom);
                }
                crate::trace(&format!("monitor {:?} native ppp {native_ppp} zoom {zoom}", m));
            }
        }
        crate::trace(&format!(
            "frame: loading={} proxy={} hist={} cache={:?}",
            self.loading,
            self.proxy.is_some(),
            self.hist.is_some(),
            self.cache_progress.as_ref().map(|c| (c.0, c.1))
        ));
        self.drain(&ctx);
        self.keyboard(&ctx);
        if self.needs_render && self.proxy.is_some() {
            self.render();
        }

        egui::Panel::top("top").exact_size(52.0).frame(egui::Frame::new().fill(theme::TOOLBAR)).show(ui, |ui| self.top_bar(ui));
        egui::Panel::top("strip")
            .exact_size(144.0)
            .frame(egui::Frame::new().fill(theme::WINDOW).inner_margin(egui::Margin::symmetric(0, 4)))
            .show(ui, |ui| self.strip(ui));
        egui::Panel::left("left")
            .exact_size(330.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(theme::WINDOW).inner_margin(egui::Margin::symmetric(10, 0)))
            .show(ui, |ui| self.left_column(ui));
        egui::Panel::right("right")
            .exact_size(320.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(theme::WINDOW).inner_margin(egui::Margin::symmetric(10, 0)))
            .show(ui, |ui| {
                egui::Panel::bottom("right_bottom").exact_size(80.0).frame(egui::Frame::new().fill(theme::WINDOW)).show(ui, |ui| self.right_bottom(ui));
                egui::CentralPanel::no_frame().show(ui, |ui| self.right_column(ui));
            });
        egui::CentralPanel::no_frame().show(ui, |ui| {
            egui::Panel::bottom("viewer_bar").exact_size(36.0).frame(egui::Frame::new().fill(theme::TOOLBAR)).show(ui, |ui| self.viewer_toolbar(ui));
            egui::CentralPanel::no_frame().show(ui, |ui| self.viewer(ui));
        });
        self.shot_step(&ctx);
        let _ = &self.font;
    }

    fn on_exit(&mut self) {
        crate::trace("on_exit");
        self.save_current_if_dirty();
    }
}
