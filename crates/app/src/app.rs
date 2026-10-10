//! The main window: folder → thumbnail strip → photo, with the C# layout (top bar,
//! strip, left adjustments, centre viewer, right histogram / info / tools).

use crate::i18n::{self, f, t, Lang};
use crate::export_ui::{DialogAction, ExportDialog, ExportJob, Scope, WatermarkOverlay};
use crate::settings::Settings;
use crate::theme;
use crate::tools::{self, Drag, HealMode, ToolMode, View, P};
use crate::viewer::{self, DisplayPipeline, Placement, ViewerState, ZoomMode};
use crate::widgets::{self, Gradient, Histogram, SliderSpec};
use crate::worker::{self, Item, Msg, Worker};
use awpr_core::pipeline::{ProcessContext, SourceKind};
use awpr_core::{color, FloatImage, ImageAdjustments, LinearGradient, Rotation};
use awpr_gpu::{GpuFrame, GpuPipeline, HistogramJob};
use awpr_photo::export::{ExportItem, ExportSettings};
use awpr_photo::presets::PresetCollection;
use awpr_photo::edits;
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

mod dialogs;
mod library;
mod masks_ui;
use dialogs::{SettingsDraft, UpdateState};
use library::{Confirm, UndoStep};

/// What the viewer currently shows.
enum Display {
    None,
    /// A frame still on the GPU, drawn from its buffer.
    Gpu { frame: GpuFrame<'static>, bind: Arc<wgpu::BindGroup> },
    /// The CPU fallback's result as a texture.
    Cpu { tex: egui::TextureHandle, size: Vec2 },
}

/// The 色彩 section's pages.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ColorTab {
    Basic,
    Hsl,
    Curves,
}

/// Width of the left and right columns (logical points; 468 px at 150 %).
const SIDE_W: f32 = 312.0;

/// HSL band names, in `ImageAdjustments::hsl_*` order.
const HSL_BANDS: [&str; 8] = ["紅", "橙", "黃", "綠", "青", "藍", "紫", "洋紅"];

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
pub(crate) const DESIGN_HEIGHT: f32 = 1010.0;

pub struct App {
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
    /// Right-button drag on the strip, and an offset to apply to it on the next frame.
    strip_pan: library::StripPan,
    strip_offset: Option<f32>,
    /// --shot AWPR_SHOT_STRIP_SCROLL: held every frame (the strip has no width at first).
    shot_strip_offset: Option<f32>,
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
    /// The open photo's source: LibRaw's linear camera RGB (處理版本 3 RAW) or encoded.
    source_kind: SourceKind,
    /// The delivered source's primaries, and whether the file itself is Display P3.
    source_primaries: awpr_core::SourcePrimaries,
    p3_file: bool,
    /// Why the open photo could not be decoded (shown in 照片資訊).
    decode_error: Option<String>,
    /// Photos nothing could decode (their strip tiles say so).
    undecodable: std::collections::HashSet<String>,
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
    undo: Vec<UndoStep>,
    redo: Vec<UndoStep>,
    status: String,
    render_note: String,

    // ---- tools (the C# ImageViewer overlays) ----
    tool: ToolMode,
    heal_mode: HealMode,
    /// `ImageAdjustments.ActiveGradientIndex`: runtime only, never saved.
    active_gradient: i32,
    /// The heal spot last placed / edited: 大小 and 仿製／修補 apply to it.
    active_spot: i32,
    drag: Drag,
    drag_spot: usize,
    /// Normalized pointer at the press, and the crop box then (whole-box moves).
    drag_start: (f64, f64),
    crop_start: [f64; 4],
    drag_last: Option<egui::Pos2>,
    wb_picker: bool,
    /// 自訂 picked in the 比例 list this session (W, H); None = the list follows the
    /// stored ratio.
    crop_custom: Option<(u32, u32)>,
    /// The gradient a right-click landed on (its 刪除 menu).
    grad_menu: Option<usize>,

    // ---- 色彩 section pages (處理版本 3) ----
    color_tab: ColorTab,
    /// HSL page: 0 色相, 1 飽和度, 2 明度.
    hsl_tab: usize,
    /// 曲線 page: 0 RGB, 1 紅, 2 綠, 3 藍; the point being dragged.
    curve_channel: usize,
    curve_drag: Option<usize>,

    // ---- 遮罩 tool (masks_ui.rs) ----
    /// The selected mask (runtime only, like the gradient index).
    active_mask: i32,
    mask_overlay: bool,
    /// 擦除: new brush strokes remove from the mask.
    brush_erase: bool,
    /// Last painted point (screen), so strokes are not oversampled.
    paint_last: Option<P>,
    /// The red overlay texture and the mask shape it shows.
    mask_overlay_tex: Option<(u64, egui::TextureHandle)>,

    // ---- export ----
    export_settings: ExportSettings,
    /// The open 匯出設定 window, and the settings before it opened (取消 restores the
    /// watermark, which the window previews live).
    export_dlg: Option<(ExportDialog, ExportSettings)>,
    export_job: Option<ExportJob>,
    wm_overlay: WatermarkOverlay,

    // ---- photo management (library.rs) ----
    /// Selected strip positions; `current` is the one being edited.
    selected: std::collections::BTreeSet<usize>,
    /// Where a Shift-click range starts.
    anchor: Option<usize>,
    /// The batch edit session: the other selected photos and the adjustments when it began.
    sync: Option<(Vec<Item>, ImageAdjustments)>,
    /// 複製照片設定 (memory only, like the C# build) and which photo it came from.
    copied: Option<ImageAdjustments>,
    copy_source: Option<String>,
    presets: PresetCollection,
    preset_choice: String,
    preset_editor: Option<crate::presets_ui::PresetEditor>,
    /// The thumbnail menu: item, position, opened this frame.
    strip_menu: Option<(usize, egui::Pos2, bool)>,
    confirm: Option<Confirm>,

    // ---- settings, language, about (dialogs.rs) ----
    /// The first-run language choice, with the language picked so far.
    first_run: Option<Lang>,
    settings_dlg: Option<SettingsDraft>,
    about: Option<UpdateState>,
    /// 支援RAW檔相機列表: filter text and the list.
    cameras: Option<(String, Vec<String>)>,
    /// The ☰ menu: position, opened this frame.
    app_menu: Option<(egui::Pos2, bool)>,
    /// The interface size last applied (percent setting, AWPR_UI_SCALE in percent).
    applied_scale: Option<(i64, Option<i64>)>,
    /// The interface is bigger than the screen: the columns must scroll.
    force_scroll: bool,
    /// What 自動 gives on this screen, in percent.
    auto_percent: i64,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, shot: Option<ShotPlan>) -> Self {
        let first_run = Settings::is_first_run();
        let mut settings = Settings::load();
        if let Ok(code) = std::env::var("AWPR_LANG") {
            // Memory only: one run in another language (screenshots).
            settings.language = Lang::from_locale(&code);
        }
        let first_run = if shot.is_some() {
            (std::env::var("AWPR_SHOT_DLG").as_deref() == Ok("firstrun")).then(Lang::guess_from_system)
        } else {
            first_run.then(Lang::guess_from_system)
        };
        i18n::set_lang(settings.language);
        awpr_photo::text::set_translator(i18n::tr);
        let font = theme::install(&cc.egui_ctx, settings.language, settings.font_sizes);
        if std::env::var_os("AWPR_NO_GPU").is_some() {
            settings.use_gpu = false;
        }
        if shot.is_some() && std::env::var_os("AWPR_SHOT_SHOW_HIDDEN").is_some() {
            settings.show_hidden = true; // memory only: the hidden badge in a screenshot
        }
        if shot.is_some() && std::env::var_os("AWPR_SHOT_XMP").is_some() {
            settings.xmp_support = true; // memory only: the XMP menu items in a screenshot
        }
        // --shot: the strip scrolled this far (its ◀ ▶ arrows).
        let shot_strip_offset = std::env::var("AWPR_SHOT_STRIP_SCROLL").ok().filter(|_| shot.is_some()).and_then(|v| v.parse::<f32>().ok());
        let rs = cc.wgpu_render_state.clone();
        let (mut gpu, mut gpu_status) = (None, t("沒有 GPU").to_string());
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
                    Err(e) => gpu_status = f("GPU 無法使用，改用 CPU：{0}", &[&e]),
                }
            } else {
                gpu_status = t("已關閉 GPU 加速（CPU 算圖）").into();
            }
        }
        let (worker, rx) = Worker::new(cc.egui_ctx.clone());
        let mut app = Self {
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
            strip_pan: library::StripPan::default(),
            strip_offset: None,
            shot_strip_offset,
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
            source_kind: SourceKind::Encoded,
            source_primaries: awpr_core::SourcePrimaries::Srgb,
            p3_file: false,
            decode_error: None,
            undecodable: Default::default(),
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
            tool: ToolMode::None,
            heal_mode: HealMode::Clone,
            active_gradient: -1,
            active_spot: -1,
            drag: Drag::None,
            drag_spot: 0,
            drag_start: (0.0, 0.0),
            crop_start: [0.0; 4],
            drag_last: None,
            wb_picker: false,
            crop_custom: None,
            grad_menu: None,
            color_tab: ColorTab::Basic,
            hsl_tab: 0,
            curve_channel: 0,
            curve_drag: None,
            active_mask: -1,
            mask_overlay: false,
            brush_erase: false,
            paint_last: None,
            mask_overlay_tex: None,
            export_settings: ExportSettings::load(),
            export_dlg: None,
            export_job: None,
            wm_overlay: WatermarkOverlay::default(),
            selected: Default::default(),
            anchor: None,
            sync: None,
            copied: None,
            copy_source: None,
            presets: PresetCollection::load(),
            preset_choice: awpr_photo::presets::DEFAULT_NAME.to_string(),
            preset_editor: None,
            strip_menu: None,
            confirm: None,
            first_run,
            settings_dlg: None,
            about: None,
            cameras: None,
            app_menu: None,
            applied_scale: None,
            force_scroll: false,
            auto_percent: 100,
        };
        crate::export_ui::font_list();
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

    /// 顯示捲軸, or forced when the interface is bigger than the screen.
    fn scroll_bars(&self) -> egui::scroll_area::ScrollBarVisibility {
        if self.settings.show_column_scroll_bars || self.force_scroll {
            egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded
        } else {
            egui::scroll_area::ScrollBarVisibility::AlwaysHidden
        }
    }

    fn has_photo(&self) -> bool {
        self.current.is_some() && self.proxy.is_some()
    }

    // ---- folder -------------------------------------------------------------------

    fn pick_folder(&mut self) {
        let mut dlg = rfd::FileDialog::new().set_title(t("選擇相片資料夾"));
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
            dialogs::remember_folder(&mut self.settings, path);
            self.settings.save();
        }
        paths::cleanup_stale_temp(path);
        self.preview_list = PreviewList::load(path);
        self.thumbs.clear();
        self.rebuild_items();
        if self.items.is_empty() {
            self.status = t("此資料夾沒有支援的影像").into();
            self.cache_progress = None;
            return;
        }
        self.cache_progress = Some((0, self.items.len(), String::new()));
        for it in &self.items {
            self.worker.thumbnail(it, None, 0);
        }
        self.worker.generate_caches(self.items.clone(), self.settings.loader_options());
        let first = self.shot.as_ref().map(|s| s.select).unwrap_or(0).min(self.items.len() - 1);
        self.selected = [first].into();
        self.anchor = Some(first);
        self.copy_source = None;
        self.select(first);
    }

    fn close_folder(&mut self) {
        self.save_current_if_dirty();
        self.worker.folder_gen.fetch_add(1, Ordering::SeqCst);
        self.folder.clear();
        self.items.clear();
        self.selected.clear();
        self.anchor = None;
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
        self.status = format!("{}{}", t("載入中… "), item.name());
        self.worker.load_photo(item, self.load_version, self.settings.loader_options(), None);
    }

    /// The open photo's source no longer fits its adjustments' version: a 處理版本 3 RAW
    /// still on the encoded proxy (upgraded, pasted, imported, redone), or an older version
    /// on the linear one (pasted, undone).
    pub(crate) fn needs_source_reload(&self) -> bool {
        let Some(i) = self.current else { return false };
        // A Display P3 file: version 3 reads it as P3, older versions as converted sRGB.
        if self.p3_file && self.adj.is_v3() != (self.source_primaries == awpr_core::SourcePrimaries::DisplayP3) {
            return true;
        }
        match self.source_kind {
            // Only once the linear proxy exists (the background builder makes it; until then
            // version 3 renders from the 8-bit proxy).
            SourceKind::Encoded => self.v3_proxy_possible() && awpr_photo::loader::proxy_v3_ready(&self.items[i].path),
            SourceKind::LinearCamera { .. } => !self.adj.is_v3(),
        }
    }

    /// The open photo is a 處理版本 3 RAW that can have a linear proxy.
    fn v3_proxy_possible(&self) -> bool {
        let Some(i) = self.current else { return false };
        self.adj.is_v3() && awpr_photo::loader::linear_capable(&self.items[i].path, self.exif.as_ref().and_then(|e| e.camera.as_ref()), self.settings.loader_options())
    }

    /// Still showing a 處理版本 3 RAW from its 8-bit proxy: the linear one is building, or
    /// is built but not switched to yet (the message is on its way, or the reload runs).
    pub(crate) fn waiting_for_v3_proxy(&self) -> bool {
        self.source_kind == SourceKind::Encoded
            && self.v3_proxy_possible()
            && std::env::var_os("AWPR_SHOT_HOLD_V3").is_none()
    }

    /// Load the open photo's source again for the adjustments in memory.
    pub(crate) fn reload_source(&mut self) {
        let Some(i) = self.current else { return };
        crate::trace(&format!("reload source for {} (version {})", self.items[i].name(), self.adj.pipeline_version + 1));
        self.loading = true;
        self.load_version += 1;
        self.status = format!("{}{}", t("載入中… "), self.items[i].name());
        self.worker.load_photo(self.items[i].clone(), self.load_version, self.settings.loader_options(), Some(self.adj.clone()));
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
        self.sync = None;
        self.show_original = false;
        self.needs_render = false;
        self.adj = ImageAdjustments::default();
        self.saved_adj = self.adj.clone();
        self.active_gradient = -1;
        self.active_spot = -1;
        self.active_mask = -1;
        self.mask_overlay_tex = None;
        self.drag = Drag::None;
        self.crop_custom = None;
        self.grad_menu = None;
    }

    /// No photo any more: also drop the tool and the picker (`ResetPanelsToDefault`).
    fn clear_editor(&mut self) {
        self.clear_photo();
        self.current = None;
        self.loading = false;
        self.tool = ToolMode::None;
        self.wb_picker = false;
    }

    fn on_loaded(&mut self, l: worker::Loaded) {
        if l.version != self.load_version {
            return; // superseded by a later selection
        }
        crate::trace(&format!("loaded{}: source {:?}", if l.reload { " (reload)" } else { "" }, l.source_kind));
        self.loading = false;
        // A reload changes only the source: the adjustments in memory (perhaps edited while it
        // loaded), the undo history and the saved state stay. If their version moved again
        // meanwhile, `render` reloads once more.
        if !l.reload {
            self.adj = l.adjustments;
            self.saved_adj = self.adj.clone();
            self.viewer.reset_fit();
        }
        self.exif = Some(l.exif);
        self.source = l.source;
        self.source_kind = l.source_kind;
        self.source_primaries = l.source_primaries;
        self.p3_file = l.p3_file;
        self.decode_error = l.decode_error.clone().filter(|s| !s.is_empty());
        self.proxy_gpu = None;
        let Some(proxy) = l.proxy else {
            self.status = match &self.decode_error {
                Some(reason) => format!("{} · {reason}", t("無法讀取這張照片")),
                None => t("無法讀取這張照片").into(),
            };
            if let Some(i) = self.current {
                self.undecodable.insert(self.items[i].path.clone());
            }
            return;
        };
        if let Some(gpu) = self.gpu {
            if gpu.can_host(proxy.width, proxy.height) {
                match gpu.upload(&proxy) {
                    Ok(f) => self.proxy_gpu = Some(f),
                    Err(e) => self.status = f("GPU 上傳失敗，改用 CPU：{0}", &[&e]),
                }
            }
        }
        self.status = f("{0} · {1} x {2} · 載入 {3} ms", &[&self.items[self.current.unwrap_or(0)].name(), &proxy.width, &proxy.height, &l.millis]);
        self.proxy = Some(Arc::new(proxy));
        self.needs_render = true;
        if l.reload {
            return;
        }
        if let Some(s) = &self.shot {
            if let Some(spec) = s.adjust.clone() {
                apply_adjust_spec(&mut self.adj, &spec);
            }
            match std::env::var("AWPR_SHOT_COLOR_TAB").as_deref() {
                Ok("hsl") => self.color_tab = ColorTab::Hsl,
                Ok("curves") => self.color_tab = ColorTab::Curves,
                _ => {}
            }
            if let Ok(v) = std::env::var("AWPR_SHOT_HSL_TAB") {
                self.hsl_tab = v.parse::<usize>().unwrap_or(0).min(2);
            }
            if let Ok(v) = std::env::var("AWPR_SHOT_CURVE_CHANNEL") {
                self.curve_channel = v.parse::<usize>().unwrap_or(0).min(3);
            }
            match std::env::var("AWPR_SHOT_ZOOM").as_deref() {
                Ok("100") => self.viewer.set_mode(ZoomMode::Actual100),
                Ok("200") => self.viewer.set_mode(ZoomMode::Actual200),
                _ => {}
            }
            if let Ok(t) = std::env::var("AWPR_SHOT_TOOL") {
                self.shot_tool(&t);
            }
            if let Ok(text) = std::env::var("AWPR_SHOT_WM") {
                // Memory only: the live watermark preview in a screenshot.
                self.export_settings.watermark_enabled = true;
                self.export_settings.watermark_text = text;
            }
            if let Ok(sel) = std::env::var("AWPR_SHOT_SELECT") {
                let picks: std::collections::BTreeSet<usize> = sel.split(',').filter_map(|v| v.trim().parse::<usize>().ok()).filter(|&v| v >= 1 && v <= self.items.len()).map(|v| v - 1).collect();
                if !picks.is_empty() {
                    self.selected = picks;
                }
            }
            if std::env::var("AWPR_SHOT_MENU").is_ok() {
                let i = self.selected.iter().next().copied().unwrap_or(0);
                self.strip_menu = Some((i, egui::pos2(60.0 + i as f32 * 180.0, 150.0), true));
            }
            match std::env::var("AWPR_SHOT_DLG").as_deref() {
                Ok("export") => self.open_export(false),
                Ok("presets") => self.preset_editor = Some(crate::presets_ui::PresetEditor::new(&self.presets)),
                Ok("settings") => self.settings_dlg = Some(SettingsDraft::new(&self.settings, false)),
                Ok("close") => self.confirm = Some(Confirm::CloseFolder),
                Ok("settings-custom") => {
                    let mut s = self.settings.clone();
                    s.ui_scale_percent = 95;
                    self.settings_dlg = Some(SettingsDraft::new(&s, false));
                }
                Ok("fonts") => self.settings_dlg = Some(SettingsDraft::new(&self.settings, true)),
                Ok("about") => self.about = Some(UpdateState::Idle),
                Ok("cameras") => self.cameras = Some((String::new(), awpr_core::libraw::camera_list())),
                Ok("menu") => self.app_menu = Some((egui::pos2(8.0, 46.0), true)),
                _ => {}
            }
        }
        // Shot spec switched the photo to 處理版本 3: it needs the linear source.
        if self.needs_source_reload() {
            self.reload_source();
        }
        // The load may just have generated the proxy (and its strip thumbnail).
        if let Some(i) = self.current {
            self.thumb_version += 1;
            self.worker.thumbnail(&self.items[i], Some(self.adj.clone()), self.thumb_version);
        }
    }

    fn save_current_if_dirty(&mut self) {
        self.flush_batch_sync();
        let Some(i) = self.current else { return };
        if self.headless() || self.proxy.is_none() || store::value_equals(&self.adj, &self.saved_adj) {
            return;
        }
        let item = self.items[i].clone();
        if let Err(e) = store::save(&item.path, &self.adj, item.copy, None) {
            self.status = format!("{}{e}", t("儲存調整失敗："));
            return;
        }
        self.saved_adj = self.adj.clone();
        self.items[i].edited = !store::is_default(&self.adj);
    }

    // ---- editing ------------------------------------------------------------------

    fn edited(&mut self) {
        self.needs_render = true;
        self.thumb_live_due = Some(Instant::now() + Duration::from_millis(200));
        if let Some(i) = self.current {
            self.items[i].edited = !store::is_default(&self.adj);
        }
        // Provisional badges for the batch targets (final once flushed).
        if let Some((targets, _)) = &self.sync {
            for t in targets {
                if let Some(it) = self.items.iter_mut().find(|x| x.key == t.key) {
                    it.edited = true;
                }
            }
        }
    }

    /// 基本／色彩／細節 重設 (every selected photo).
    fn reset_basic_color_detail(&mut self) {
        self.reset_selected(edits::reset_basic_color_detail);
    }

    /// 全部重設 (every selected photo).
    fn reset_all(&mut self) {
        self.reset_selected(edits::reset_all);
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

    // ---- tools --------------------------------------------------------------------

    fn is_raw(&self) -> bool {
        self.current.is_none_or(|i| paths::is_raw(&self.items[i].path))
    }

    /// 裁切／漸層／修護 tab (or none): also switches the skip-geometry view.
    fn set_tool(&mut self, mode: ToolMode) {
        self.tool = mode;
        self.wb_picker = false;
        self.drag = Drag::None;
        self.grad_menu = None;
        self.needs_render = true;
    }

    fn active_gradient(&mut self) -> Option<usize> {
        tools::active_gradient(&self.adj, &mut self.active_gradient)
    }

    fn active_spot(&self) -> Option<usize> {
        usize::try_from(self.active_spot).ok().filter(|&i| i < self.adj.heal_spots.len())
    }

    /// 新增線性漸層: the default gradient (upper middle), selected.
    fn add_gradient(&mut self) {
        self.edit_begin();
        self.adj.gradients.push(LinearGradient::default());
        self.active_gradient = self.adj.gradients.len() as i32 - 1;
        self.edited();
    }

    fn clear_gradients(&mut self) {
        self.edit_begin();
        self.adj.gradients.clear();
        self.active_gradient = -1;
        self.edited();
    }

    fn delete_gradient(&mut self, index: usize) {
        if index >= self.adj.gradients.len() {
            return;
        }
        self.edit_begin();
        self.adj.gradients.remove(index);
        self.active_gradient = self.adj.gradients.len() as i32 - 1;
        self.edited();
    }

    fn delete_heal_spot(&mut self, index: usize) {
        if index >= self.adj.heal_spots.len() {
            return;
        }
        self.edit_begin();
        self.adj.heal_spots.remove(index);
        // Fall back to the previous spot.
        self.active_spot = self.adj.heal_spots.len() as i32 - 1;
        self.edited();
    }

    fn clear_heal(&mut self) {
        self.edit_begin();
        self.adj.heal_spots.clear();
        self.edited();
    }

    /// 仿製／修補 for new spots; converts the active spot too (`SetHealMode`).
    fn set_heal_mode(&mut self, mode: HealMode) {
        self.heal_mode = mode;
        let Some(i) = self.active_spot() else { return };
        let inpaint = mode == HealMode::Inpaint;
        if self.adj.heal_spots[i].use_inpaint != inpaint {
            self.edit_begin();
            self.adj.heal_spots[i].use_inpaint = inpaint;
            self.edited();
        }
    }

    /// A 比例 choice: store it and fit the largest centred box of that ratio.
    fn set_crop_aspect(&mut self, aspect: String) {
        self.edit_begin();
        if let Some(p) = &self.proxy {
            tools::apply_crop_aspect(&mut self.adj, &aspect, p.width as f64, p.height as f64);
        }
        self.adj.crop_aspect_ratio = aspect;
        self.edited();
    }

    fn reset_crop(&mut self) {
        self.edit_begin();
        tools::reset_crop_geometry(&mut self.adj);
        self.crop_custom = None;
        self.edited();
    }

    /// The picker clicked at (nx, ny) of the displayed image: sample the proxy there
    /// (`OnWhiteBalancePicked`).
    fn pick_white_balance(&mut self, nx: f64, ny: f64) {
        let Some(p) = self.proxy.clone() else { return };
        let x = ((nx * p.width as f64) as i64).clamp(0, p.width as i64 - 1) as usize;
        let y = ((ny * p.height as f64) as i64).clamp(0, p.height as i64 - 1) as usize;
        let i = p.index(x, y);
        let camera = self.exif.as_ref().and_then(|e| e.camera.as_ref());
        let (t, tint) = match (self.source_kind, camera) {
            // Linear camera RGB: the multipliers that neutralise the patch, directly.
            (SourceKind::LinearCamera { .. }, Some(cam)) => {
                match awpr_core::v3::neutralizing_mul(p.data[i], p.data[i + 1], p.data[i + 2]).and_then(|m| color::cam_mul_to_kelvin_tint(cam, &m)) {
                    Some(v) => v,
                    None => return,
                }
            }
            _ => tools::estimate_white_balance(p.data[i], p.data[i + 1], p.data[i + 2], self.adj.is_legacy_pipeline(), camera),
        };
        self.edit_begin();
        self.adj.temperature = tools::clamp_temp_for_current(t, self.is_raw());
        self.adj.tint = tint;
        self.wb_picker = false;
        self.edited();
    }

    /// Left button pressed on the viewer with a tool selected.
    fn begin_tool_drag(&mut self, p: P, v: &View) {
        match self.tool {
            ToolMode::Crop => {
                let d = tools::crop_hit_test(&tools::crop_ctrl_rect(&self.adj, v), p);
                if d != Drag::None {
                    self.drag = d;
                    self.drag_start = v.ctrl_to_norm(p);
                    self.crop_start = [self.adj.crop_x, self.adj.crop_y, self.adj.crop_width, self.adj.crop_height];
                    self.edit_begin();
                }
            }
            ToolMode::Gradient => {
                // The active gradient's rotate / range handles first…
                if let Some(ai) = self.active_gradient() {
                    let d = tools::gradient_handle_hit(&self.adj.gradients[ai], v, p);
                    if d != Drag::None {
                        self.drag = d;
                        self.edit_begin();
                        return;
                    }
                }
                // …then any white dot selects that gradient and starts a move. Clicking
                // empty space creates nothing (新增線性漸層 does).
                if let Some(i) = tools::gradient_at_point(&self.adj, v, p) {
                    self.active_gradient = i as i32;
                    self.drag = Drag::GradCenter;
                    self.edit_begin();
                }
            }
            ToolMode::Heal => {
                if let Some((i, d)) = tools::heal_hit(&self.adj, v, p) {
                    self.drag = d;
                    self.drag_spot = i;
                    self.active_spot = i as i32;
                    self.edit_begin();
                    return;
                }
                let (nx, ny) = v.ctrl_to_norm(p);
                self.edit_begin();
                self.adj.heal_spots.push(tools::new_heal_spot(nx, ny, self.adj.heal_size, self.heal_mode));
                let i = self.adj.heal_spots.len() - 1;
                self.drag = Drag::HealTarget;
                self.drag_spot = i;
                self.active_spot = i as i32;
                self.edited();
            }
            ToolMode::Mask => self.begin_mask_drag(p, v),
            ToolMode::None => {}
        }
    }

    fn update_tool_drag(&mut self, p: P, v: &View) {
        let (nx, ny) = v.ctrl_to_norm(p);
        match self.drag {
            Drag::None => return,
            Drag::MaskCenter | Drag::MaskRadiusX | Drag::MaskRadiusY | Drag::MaskRotate | Drag::MaskPaint => return self.update_mask_drag(p, v),
            Drag::CropMove => tools::move_crop(&mut self.adj, self.crop_start, nx - self.drag_start.0, ny - self.drag_start.1),
            Drag::CropL | Drag::CropR | Drag::CropT | Drag::CropB | Drag::CropTL | Drag::CropTR | Drag::CropBL | Drag::CropBR => {
                tools::drag_crop_handle(&mut self.adj, self.drag, nx, ny, v.w, v.h)
            }
            Drag::GradCenter | Drag::GradRange | Drag::GradRotate => {
                if let Some(ai) = self.active_gradient() {
                    tools::drag_gradient(&mut self.adj.gradients[ai], self.drag, p, v);
                }
            }
            Drag::HealTarget => {
                if let Some(s) = self.adj.heal_spots.get_mut(self.drag_spot) {
                    (s.target_x, s.target_y) = (nx, ny);
                }
            }
            Drag::HealSource => {
                if let Some(s) = self.adj.heal_spots.get_mut(self.drag_spot) {
                    (s.source_x, s.source_y) = (nx, ny);
                }
            }
        }
        self.edited();
    }

    /// `--shot` with AWPR_SHOT_TOOL: open the tool, with sample geometry when the photo
    /// has none, so the overlay shows up in the screenshot (memory only, never saved).
    fn shot_tool(&mut self, name: &str) {
        let mode = match name {
            "crop" => ToolMode::Crop,
            "gradient" => ToolMode::Gradient,
            "heal" => ToolMode::Heal,
            "mask-radial" | "mask-brush" => ToolMode::Mask,
            _ => return,
        };
        self.set_tool(mode);
        let a = &mut self.adj;
        match mode {
            ToolMode::Crop if a.crop_x <= 0.0 && a.crop_y <= 0.0 && a.crop_width >= 1.0 && a.crop_height >= 1.0 => {
                (a.crop_x, a.crop_y, a.crop_width, a.crop_height) = (0.12, 0.1, 0.7, 0.75);
                a.crop_angle = 4.0;
            }
            ToolMode::Gradient if a.gradients.is_empty() => {
                a.gradients.push(LinearGradient { center_y: 0.22, angle: 8.0, range: 0.2, exposure: -1.0, ..Default::default() });
                a.gradients.push(LinearGradient { center_y: 0.85, angle: 180.0, exposure: 0.5, ..Default::default() });
                self.active_gradient = 0;
            }
            ToolMode::Mask if a.masks.is_empty() => {
                if name == "mask-radial" {
                    a.masks.push(awpr_core::LocalMask { center_x: 0.42, center_y: 0.55, radius_x: 0.22, radius_y: 0.13, angle: -12.0, feather: 60.0, exposure: 0.9, saturation: 25.0, ..Default::default() });
                } else {
                    let stroke = |pts: &[(f64, f64)]| awpr_core::BrushStroke { radius: 0.035, feather: 60.0, flow: 100.0, erase: false, points: pts.to_vec() };
                    a.masks.push(awpr_core::LocalMask {
                        kind: awpr_core::MaskKind::Brush,
                        exposure: -0.8,
                        saturation: -40.0,
                        strokes: vec![stroke(&[(0.08, 0.86), (0.3, 0.8), (0.55, 0.84), (0.8, 0.78), (0.95, 0.82)]), stroke(&[(0.15, 0.93), (0.6, 0.92), (0.9, 0.94)])],
                        ..Default::default()
                    });
                }
                self.active_mask = 0;
                self.mask_overlay = std::env::var_os("AWPR_SHOT_NO_OVERLAY").is_none();
            }
            ToolMode::Heal if a.heal_spots.is_empty() => {
                let mut s = tools::new_heal_spot(0.62, 0.45, 20.0, HealMode::Clone);
                s.source_x = 0.5;
                a.heal_spots.push(s);
                a.heal_spots.push(tools::new_heal_spot(0.3, 0.65, 15.0, HealMode::Inpaint));
                self.active_spot = 0;
            }
            _ => {}
        }
    }

    // ---- export -------------------------------------------------------------------

    /// The photos an export covers, never hidden ones.
    fn export_items(&self, scope: Scope) -> Vec<ExportItem> {
        let pick = |it: &worker::Item| (!it.hidden).then(|| ExportItem { path: it.path.clone(), copy: it.copy });
        match scope {
            Scope::All => self.items.iter().filter_map(pick).collect(),
            Scope::Selected => self.selected_items().iter().filter_map(pick).collect(),
            Scope::Current => self.current.and_then(|i| pick(&self.items[i])).into_iter().collect(),
        }
    }

    /// 匯出…: the selection when several photos are selected (or asked for), else the
    /// current photo.
    fn open_export(&mut self, selection: bool) {
        if self.export_job.is_some() || !self.has_photo() {
            return;
        }
        let scope = if selection || self.selected.len() > 1 { Scope::Selected } else { Scope::Current };
        self.export_dlg = Some((ExportDialog::new(&self.export_settings, scope), self.export_settings.clone()));
    }

    fn export_windows(&mut self, ctx: &egui::Context) {
        let counts = [self.export_items(Scope::Current).len(), self.export_items(Scope::Selected).len(), self.export_items(Scope::All).len()];
        if let Some((dlg, before)) = &mut self.export_dlg {
            match dlg.show(ctx, &mut self.export_settings, counts) {
                DialogAction::None => {}
                DialogAction::Cancel => {
                    self.export_settings = before.clone();
                    self.export_dlg = None;
                }
                act @ (DialogAction::Save | DialogAction::Start) => {
                    self.export_settings = dlg.draft.clone();
                    let all = dlg.scope;
                    self.export_dlg = None;
                    if !self.headless() {
                        self.export_settings.save();
                    }
                    if act == DialogAction::Start {
                        self.start_export(ctx, all);
                    }
                }
            }
        }
        if let Some(job) = &mut self.export_job {
            job.poll();
            match job.result.take() {
                Some(r) => {
                    self.status = match r {
                        Ok(w) if w.is_empty() => t("匯出已取消").into(),
                        Ok(w) => {
                            let dir = w[0].path.parent().map(|p| p.display().to_string()).unwrap_or_default();
                            let note = if job.cancelled() { t("（已取消其餘）") } else { "" };
                            format!("{}{note}", f("已匯出 {0} 張到 {1}", &[&w.len(), &dir]))
                        }
                        Err(e) => e,
                    };
                    self.export_job = None;
                }
                None => job.show(ctx),
            }
        }
    }

    fn start_export(&mut self, ctx: &egui::Context, all: Scope) {
        // The XML is what the export reads.
        self.save_current_if_dirty();
        let items = self.export_items(all);
        if items.is_empty() {
            self.status = t("沒有可匯出的照片（隱藏的照片不會匯出）").into();
            return;
        }
        self.export_job = Some(ExportJob::start(ctx.clone(), items, self.export_settings.clone(), self.settings.loader_options(), self.gpu));
    }

    // ---- rendering ----------------------------------------------------------------

    fn process_context(&self) -> ProcessContext {
        ProcessContext {
            // Gradient / heal overlays live in the pre-geometry frame; the crop overlay
            // sees distortion, 90° rotation and the straighten angle but not the crop.
            skip_geometry: matches!(self.tool, ToolMode::Gradient | ToolMode::Heal | ToolMode::Mask),
            skip_crop_rect: self.tool == ToolMode::Crop,
            camera: self.exif.as_ref().and_then(|e| e.camera.clone()),
            white_balance_reference: self.source.white_balance_reference(),
            source_kind: self.source_kind,
            source_primaries: self.source_primaries,
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
        // Never render one version's maths from the other version's source.
        if self.needs_source_reload() {
            if !self.loading {
                self.reload_source();
            }
            return;
        }
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
                    self.status = f("GPU 算圖失敗，改用 CPU：{0}", &[&e]);
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
        self.render_note = t("CPU 算圖中…").into();
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
                Msg::Undecodable { path } => {
                    self.undecodable.insert(path);
                }
                Msg::ProxyV3Ready { path } => {
                    crate::trace(&format!("ProxyV3Ready {}", paths::file_name(&path)));
                    // Its thumbnails now render from the linear source; the open photo
                    // switches to it (`render` sees the source no longer fits).
                    let current = self.current.map(|i| self.items[i].key.clone());
                    for it in self.items.iter().filter(|it| it.path == path) {
                        self.thumb_version += 1;
                        let adj = (Some(&it.key) == current.as_ref()).then(|| self.adj.clone());
                        self.worker.thumbnail(it, adj, self.thumb_version);
                    }
                    if self.current.is_some_and(|i| self.items[i].path == path) {
                        self.needs_render = true;
                    }
                }
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
                        self.render_note = t("CPU 算圖").into();
                    }
                }
            }
        }
        if let Some(gpu) = self.gpu {
            gpu.poll();
            if let Some((job, t0)) = &self.hist_job {
                if let Some(bins) = job.take() {
                    // Submit → histogram ready: how long the GPU took for this render.
                    self.render_note = f("GPU 算圖 {0} ms", &[&t0.elapsed().as_millis()]);
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
        let (select_all, delete_file) = ctx.input(|i| (i.modifiers.command && i.key_pressed(Key::A), i.modifiers.shift && i.key_pressed(Key::Delete)));
        if select_all {
            self.select_all();
        }
        if delete_file {
            if let Some(c) = self.current {
                self.delete_item(c);
            }
        }
        let (left, right, undo, redo, compare, refresh, open, delete, escape) = ctx.input(|i| {
            let cmd = i.modifiers.command;
            (
                i.key_pressed(Key::ArrowLeft),
                i.key_pressed(Key::ArrowRight),
                cmd && i.key_pressed(Key::Z) && !i.modifiers.shift,
                cmd && (i.key_pressed(Key::Y) || (i.modifiers.shift && i.key_pressed(Key::Z))),
                i.key_pressed(Key::Backslash),
                i.key_pressed(Key::F5),
                cmd && i.key_pressed(Key::O),
                i.key_pressed(Key::Delete) && !i.modifiers.shift,
                i.key_pressed(Key::Escape),
            )
        });
        // Esc: cancel the picker first, then the tool.
        if escape {
            if self.wb_picker {
                self.wb_picker = false;
            } else if self.tool != ToolMode::None {
                self.set_tool(ToolMode::None);
            }
        }
        // Delete removes the selected gradient / heal spot while that tool is open. With
        // nothing selected it is left for 隱藏照片 (TASK-004; the C# Delete key), so that
        // must only run when this did not take the key.
        if delete && self.has_photo() && self.drag == Drag::None {
            let taken = match self.tool {
                ToolMode::Gradient => self.active_gradient().map(|i| self.delete_gradient(i)).is_some(),
                ToolMode::Heal => self.active_spot().map(|i| self.delete_heal_spot(i)).is_some(),
                ToolMode::Mask => self.active_mask().map(|i| self.delete_mask(i)).is_some(),
                _ => false,
            };
            // Otherwise Delete = 隱藏且不輸出 (the C# key).
            if !taken {
                self.hide_selected();
            }
        }
        if let Some(c) = self.current {
            let step = if left && c > 0 { Some(c - 1) } else if right && c + 1 < self.items.len() { Some(c + 1) } else { None };
            if let Some(n) = step {
                self.selected = [n].into();
                self.anchor = Some(n);
                self.select(n);
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

    /// Toolbar button height: 1.6 × a normal button (1.1.0's 2× less a fifth).
    fn top_button_h(ui: &egui::Ui) -> f32 {
        let text = ui.fonts_mut(|f| f.row_height(&egui::TextStyle::Button.resolve(ui.style())));
        1.6 * (text + 2.0 * ui.spacing().button_padding.y).max(ui.spacing().interact_size.y)
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let bh = Self::top_button_h(ui);
        let button = |text: &str| egui::Button::new(text).min_size(Vec2::new(0.0, bh));
        ui.horizontal_centered(|ui| {
            ui.add_space(8.0);
            self.app_menu_button(ui);
            ui.label(RichText::new("AwayPhotoRawEditor").strong().size(theme::fs(crate::settings::FontKind::Logo)));
            ui.label(RichText::new(concat!("v", env!("CARGO_PKG_VERSION"))).size(theme::scaled(12.0)).color(theme::TEXT_FAINT));
            ui.add_space(16.0);
            if ui.add(button(t("📁  開啟資料夾"))).clicked() {
                self.pick_folder();
            }
            let has_folder = !self.folder.is_empty();
            if ui.add_enabled(has_folder, button(t("重新整理"))).clicked() {
                let f = self.folder.clone();
                self.open_folder(&f);
            }
            if ui.add_enabled(has_folder, button(t("關閉資料夾"))).clicked() {
                self.confirm = Some(Confirm::CloseFolder);
            }
            let show = self.settings.show_hidden;
            if ui.add_enabled(has_folder, button(t("顯示隱藏的照片")).selected(show)).on_hover_text(t("不顯示隱藏／顯示全部")).clicked() {
                self.set_show_hidden(!show);
            }
            ui.add_space(10.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // 匯出… at the far end: wide and blue. (The GPU status moved to 設定.)
                ui.add_space(10.0);
                let can_export = self.has_photo() && self.export_job.is_none() && self.export_dlg.is_none();
                let export = egui::Button::new(RichText::new(t("匯出…")).color(Color32::WHITE)).fill(theme::ACCENT).min_size(Vec2::new(160.0, bh));
                if ui.add_enabled(can_export, export).clicked() {
                    self.open_export(false);
                }
                ui.add_space(6.0);
                if let Some((done, total, name)) = &self.cache_progress {
                    ui.add(egui::ProgressBar::new(*done as f32 / (*total).max(1) as f32).desired_width(140.0).show_percentage());
                    ui.label(RichText::new(f("產生快取 {0}/{1} {2}", &[done, total, name])).size(theme::scaled(12.0)).color(theme::TEXT_DIM));
                }
                // The folder path gets whatever the status leaves; a long one keeps its
                // end (`…/photos/2026`), like the C# path label. Hover shows all of it.
                ui.add_space(12.0);
                let room = ui.available_width();
                if room > 20.0 && !self.folder.is_empty() {
                    let font = egui::TextStyle::Body.resolve(ui.style());
                    let shown = elide_left(ui, &self.folder, &font, room);
                    ui.allocate_ui_with_layout(Vec2::new(room, ui.available_height()), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(egui::Label::new(RichText::new(shown).color(theme::TEXT_DIM)).wrap_mode(egui::TextWrapMode::Extend)).on_hover_text(&self.folder);
                    });
                }
            });
        });
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

    /// A slider over any value of the adjustments (gradients, negated angle…).
    fn slider_with(&mut self, ui: &mut egui::Ui, spec: SliderSpec, enabled: bool, get: impl Fn(&ImageAdjustments) -> f64, set: impl FnOnce(&mut ImageAdjustments, f64)) -> bool {
        let mut v = get(&self.adj);
        let r = widgets::adjust_slider(ui, &spec, &mut v, enabled);
        if r.began {
            self.edit_begin();
        }
        if r.changed {
            set(&mut self.adj, v);
            self.edited();
        }
        r.changed
    }

    fn left_column(&mut self, ui: &mut egui::Ui) {
        // Long translations are cut, never widen the column.
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
        ui.add_space(4.0);
        let legacy = self.adj.is_legacy_pipeline();
        theme::section(ui, t("基本調整"), |ui| {
            // v1 exposure is ±5 true EV; legacy photos keep their ±2 slider.
            let lim = if legacy { 2.0 } else { 5.0 };
            let exposure = SliderSpec { min: -lim, max: lim, decimals: 2, wheel_step: 0.05, ..SliderSpec::pm100(t("曝光")) };
            self.slider(ui, exposure, |a| &mut a.exposure);
            self.slider(ui, SliderSpec::pm100(t("對比")), |a| &mut a.contrast);
            self.slider(ui, SliderSpec::pm100(t("亮部")), |a| &mut a.highlights);
            self.slider(ui, SliderSpec::pm100(t("暗部")), |a| &mut a.shadows);
            self.slider(ui, SliderSpec::pm100(t("白色")), |a| &mut a.whites);
            self.slider(ui, SliderSpec::pm100(t("黑色")), |a| &mut a.blacks);
            // 處理版本 3 RAWs only: rebuilds clipped channels and rolls white off softly.
            if self.adj.is_v3() && matches!(self.source_kind, SourceKind::LinearCamera { .. }) {
                let spec = SliderSpec { min: 0.0, bipolar: false, ..SliderSpec::pm100(t("高光復原")) };
                self.slider(ui, spec, |a| &mut a.highlight_recovery);
            }
        });
        ui.add_space(4.0);
        theme::section(ui, t("色彩"), |ui| {
            // The three tabs at their own widths, squeezed (names cut) only when the row
            // would not fit: never wider than the column.
            ui.horizontal(|ui| {
                let tabs = [(ColorTab::Basic, t("基本")), (ColorTab::Hsl, "HSL"), (ColorTab::Curves, t("曲線"))];
                let font = egui::TextStyle::Button.resolve(ui.style());
                let pad = 2.0 * ui.spacing().button_padding.x + 4.0;
                let widths = widgets::share_widths(tabs.map(|(_, name)| widgets::text_width(ui, name, &font) + pad), ui.available_width() - 2.0 * ui.spacing().item_spacing.x);
                for ((tab, name), w) in tabs.into_iter().zip(widths) {
                    let sel = self.color_tab == tab;
                    if widgets::fixed_button(ui, true, Vec2::new(w, 24.0), name, |b| b.selected(sel).frame_when_inactive(sel)).clicked() {
                        self.color_tab = tab;
                    }
                }
            });
            ui.add_space(2.0);
            match self.color_tab {
                ColorTab::Basic => self.color_basic(ui),
                ColorTab::Hsl => self.color_hsl(ui),
                ColorTab::Curves => self.color_curves(ui),
            }
        });
        ui.add_space(4.0);
        theme::section(ui, t("細節"), |ui| {
            self.slider(ui, SliderSpec::pm100(t("銳利度")), |a| &mut a.sharpening);
            self.slider(ui, SliderSpec::pm100(t("暗角")), |a| &mut a.vignette);
            let nr = SliderSpec { min: 0.0, bipolar: false, ..SliderSpec::pm100(t("降噪")) };
            self.slider(ui, nr, |a| &mut a.noise_reduction);
        });
        ui.add_space(8.0);
        if widgets::wide_button(ui, self.has_photo(), 30.0, t("基本／色彩／細節 重設"), |b| b).clicked() {
            self.reset_basic_color_detail();
        }
        ui.add_space(8.0);
        self.preset_panel(ui);
    }

    /// 處理版本 3 tools on an older photo: say how to get them. True when they can be used.
    fn v3_notice(&self, ui: &mut egui::Ui) -> bool {
        if self.adj.is_v3() || !self.has_photo() {
            return self.has_photo();
        }
        // Two labels, so a narrow column breaks before the arrow, never inside the quote.
        let note = |ui: &mut egui::Ui, s: &str| ui.add(egui::Label::new(RichText::new(s).color(theme::TEXT_DIM).size(theme::scaled(12.0))).wrap());
        note(ui, t("需要處理版本 3"));
        note(ui, t("→ 縮圖按右鍵「升級處理版本」"));
        false
    }

    /// HSL: 8 bands × 色相／飽和度／明度 (OkLCh), one quantity per page.
    fn color_hsl(&mut self, ui: &mut egui::Ui) {
        let enabled = self.v3_notice(ui);
        ui.horizontal(|ui| {
            for (i, name) in [t("色相"), t("飽和度"), t("明度")].into_iter().enumerate() {
                if ui.selectable_label(self.hsl_tab == i, name).clicked() {
                    self.hsl_tab = i;
                }
            }
        });
        let tab = self.hsl_tab;
        for b in 0..8 {
            let spec = SliderSpec::pm100(t(HSL_BANDS[b]));
            let get = move |a: &ImageAdjustments| match tab {
                0 => a.hsl_hue[b],
                1 => a.hsl_saturation[b],
                _ => a.hsl_luminance[b],
            };
            let set = move |a: &mut ImageAdjustments, v: f64| match tab {
                0 => a.hsl_hue[b] = v,
                1 => a.hsl_saturation[b] = v,
                _ => a.hsl_luminance[b] = v,
            };
            widgets::slider_scope(ui, |ui| {
                ui.push_id(("hsl", tab, b), |ui| {
                    self.slider_with(ui, spec, enabled, get, set);
                });
            });
        }
    }

    /// 曲線: RGB plus R / G / B point curves.
    fn color_curves(&mut self, ui: &mut egui::Ui) {
        let enabled = self.v3_notice(ui);
        ui.horizontal(|ui| {
            for (i, name) in ["RGB", t("紅"), t("綠"), t("藍")].into_iter().enumerate() {
                if ui.selectable_label(self.curve_channel == i, name).clicked() {
                    self.curve_channel = i;
                    self.curve_drag = None;
                }
            }
        });
        let ch = self.curve_channel;
        let color = [Color32::from_gray(220), Color32::from_rgb(235, 80, 80), Color32::from_rgb(80, 200, 100), Color32::from_rgb(90, 140, 255)][ch];
        let mut pts = match ch {
            0 => self.adj.curve_rgb.clone(),
            1 => self.adj.curve_red.clone(),
            2 => self.adj.curve_green.clone(),
            _ => self.adj.curve_blue.clone(),
        };
        let r = widgets::curve_editor(ui, ui.id().with(("curve", ch)), &mut pts, color, enabled, &mut self.curve_drag);
        if r.began {
            self.edit_begin();
        }
        if r.changed {
            self.set_curve(ch, pts);
        }
        // The hint on its own line (it may be cut in a long language), the button whole.
        ui.add(egui::Label::new(RichText::new(t("點兩下或按右鍵刪除控制點")).color(theme::TEXT_FAINT).size(theme::scaled(11.0))).truncate());
        if widgets::wide_button(ui, enabled, 24.0, t("重設曲線"), |b| b).clicked() {
            self.edit_begin();
            self.set_curve(ch, Vec::new());
        }
    }

    fn set_curve(&mut self, ch: usize, pts: Vec<(f64, f64)>) {
        // A curve back on the diagonal is stored as "no curve".
        let pts = if awpr_core::v3::curve_is_identity(&pts) { Vec::new() } else { pts };
        match ch {
            0 => self.adj.curve_rgb = pts,
            1 => self.adj.curve_red = pts,
            2 => self.adj.curve_green = pts,
            _ => self.adj.curve_blue = pts,
        }
        self.edited();
    }

    /// The 色彩 page every version has: white balance, vibrance, saturation.
    fn color_basic(&mut self, ui: &mut egui::Ui) {
        {
            // One row in the narrow column, everything at its own width (the label is the
            // short "WB:" form); squeezed only if a translation is still too long.
            ui.horizontal(|ui| {
                let font = egui::TextStyle::Body.resolve(ui.style());
                let bfont = egui::TextStyle::Button.resolve(ui.style());
                let pad = 2.0 * ui.spacing().button_padding.x + 4.0;
                let [label_w, pick_w, shot_w] = widgets::share_widths(
                    [widgets::text_width(ui, t("白平衡："), &font), widgets::text_width(ui, t("滴管"), &bfont) + pad, widgets::text_width(ui, t("拍攝時設定"), &bfont) + pad],
                    ui.available_width() - 2.0 * ui.spacing().item_spacing.x,
                );
                widgets::fixed_text(ui, label_w, 24.0, t("白平衡："), font, theme::TEXT_DIM).on_hover_text(t("白平衡"));
                let on = self.has_photo();
                let picker = self.wb_picker;
                if widgets::fixed_button(ui, on, Vec2::new(pick_w, 24.0), t("滴管"), |b| b.selected(picker)).on_hover_text(t("點擊畫面上的中性灰色區域設定白平衡（Esc 取消）")).clicked() {
                    self.wb_picker = !self.wb_picker;
                }
                if widgets::fixed_button(ui, on, Vec2::new(shot_w, 24.0), t("拍攝時設定"), |b| b).clicked() {
                    self.as_shot();
                }
            });
            if self.is_raw() {
                let temp = SliderSpec {
                    min: color::MIN_KELVIN,
                    max: color::MAX_KELVIN,
                    default: 5200.0,
                    bipolar: false,
                    wheel_step: 50.0,
                    ..SliderSpec::pm100(t("色溫"))
                }
                .gradient(Gradient::Temperature);
                self.slider(ui, temp, |a| &mut a.temperature);
            } else {
                // No camera Kelvin scale: a 0-centred ±100 warm/cool scale (±3000 K).
                let enabled = self.has_photo();
                let mut v = (self.adj.temperature - 5200.0) / 30.0;
                let spec = SliderSpec::pm100(t("色溫")).gradient(Gradient::Temperature);
                let r = widgets::adjust_slider(ui, &spec, &mut v, enabled);
                if r.began {
                    self.edit_begin();
                }
                if r.changed {
                    self.adj.temperature = 5200.0 + v * 30.0;
                    self.edited();
                }
            }
            self.slider(ui, SliderSpec::pm100(t("色調")).gradient(Gradient::Tint), |a| &mut a.tint);
            self.slider(ui, SliderSpec::pm100(t("鮮豔度")).gradient(Gradient::Saturation), |a| &mut a.vibrance);
            self.slider(ui, SliderSpec::pm100(t("飽和度")).gradient(Gradient::Saturation), |a| &mut a.saturation);
        }
    }

    fn right_column(&mut self, ui: &mut egui::Ui) {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
        ui.add_space(4.0);
        // One section for both (1.1.1): the histogram and the photo info keep their full
        // size whether or not a tool is open; when the tool panel does not fit, the column
        // scrolls.
        theme::section(ui, t("直方圖與照片資訊"), |ui| {
            widgets::histogram(ui, self.hist.as_ref(), 110.0);
            ui.add_space(6.0);
            match &self.exif {
                None => {
                    ui.label(RichText::new(t("尚未選擇照片")).color(theme::TEXT_FAINT));
                }
                Some(e) => {
                    let rows = [
                        (t("相機"), format!("{} {}", e.camera_make, e.camera_model).trim().to_string()),
                        (t("鏡頭"), e.lens.clone()),
                        ("ISO", e.iso.clone()),
                        (t("光圈"), e.aperture.clone()),
                        (t("快門"), e.shutter.clone()),
                        (t("焦段"), e.focal_length.clone()),
                        (t("曝光補償"), e.exposure_bias.clone()),
                        (t("白平衡"), e.white_balance.clone()),
                        (t("測光"), e.metering_mode.clone()),
                        (t("日期"), e.date_taken.clone()),
                        (t("尺寸"), e.dimensions_display()),
                        (t("檔案大小"), e.file_size_display()),
                    ];
                    // The label column is measured (the C# ExifView), at most 45 % of the
                    // column; whatever does not fit is cut with "…" (full text on hover).
                    let key_font = egui::FontId::proportional(theme::scaled(12.5));
                    let key_w = rows.iter().map(|(k, _)| widgets::text_width(ui, k, &key_font)).fold(0.0f32, f32::max).min(ui.available_width() * 0.45);
                    let row_h = ui.text_style_height(&egui::TextStyle::Body);
                    ui.spacing_mut().item_spacing.y = 1.0;
                    for (k, v) in rows {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 10.0;
                            widgets::fixed_text(ui, key_w, row_h, k, key_font.clone(), theme::TEXT_DIM);
                            ui.add(egui::Label::new(v).truncate());
                        });
                    }
                    if let Some(reason) = &self.decode_error {
                        ui.add(egui::Label::new(RichText::new(format!("· {reason}")).size(theme::scaled(12.0)).color(theme::EDITED)).wrap());
                    }
                }
            }
        });
        ui.add_space(4.0);
        theme::section(ui, t("工具"), |ui| self.tools_panel(ui));
    }

    /// 工具: 裁切／漸層／修護 tabs (click the open one again to close it) over the
    /// selected tool's controls; with no tool open only the tabs show.
    fn tools_panel(&mut self, ui: &mut egui::Ui) {
        let on = self.has_photo();
        // Four tabs in a row, or two rows of two when a label would not fit (long
        // translations): a tab name is never cut.
        let tabs_fit = |ui: &egui::Ui, labels: &[&str]| {
            let w = (ui.available_width() - 3.0 * ui.spacing().item_spacing.x) / 4.0;
            let font = egui::TextStyle::Button.resolve(ui.style());
            labels.iter().all(|l| ui.fonts_mut(|f| f.layout_no_wrap(l.to_string(), font.clone(), Color32::WHITE).size().x) + 2.0 * ui.spacing().button_padding.x <= w)
        };
        let four = tabs_fit(ui, &[t("裁切"), t("漸層"), t("修護"), t("遮罩")]);
        let per_row = if four { 4 } else { 2 };
        let w = (ui.available_width() - (per_row as f32 - 1.0) * ui.spacing().item_spacing.x) / per_row as f32;
        let tabs = [
                (t("裁切"), ToolMode::Crop, t("拖曳邊、角或整個框；角度滑桿即時拉直")),
                (t("漸層"), ToolMode::Gradient, t("白點：選取／移動　黃點：範圍　藍點：旋轉\nDelete 或右鍵白點：刪除")),
                (t("修護"), ToolMode::Heal, t("點擊加入修護點，拖曳圓圈移動（虛線圈＝取樣處）\nDelete 或右鍵：刪除")),
                (t("遮罩"), ToolMode::Mask, t("放射狀：白點移動、黃點半徑、藍點旋轉\n筆刷：在畫面上拖曳塗抹\nDelete：刪除選取的遮罩")),
        ];
        for row in tabs.chunks(per_row) {
            ui.horizontal(|ui| {
                for (label, mode, hint) in row {
                    let sel = self.tool == *mode;
                    if widgets::fixed_button(ui, on, Vec2::new(w, 28.0), *label, |b| b.selected(sel)).on_hover_text(*hint).clicked() {
                        self.set_tool(if self.tool == *mode { ToolMode::None } else { *mode });
                    }
                }
            });
        }
        ui.add_space(4.0);
        let enabled = on && self.tool != ToolMode::None;
        match self.tool {
            ToolMode::Gradient => self.gradient_controls(ui, enabled),
            ToolMode::Heal => self.heal_controls(ui, enabled),
            ToolMode::Mask => self.mask_controls(ui, enabled),
            ToolMode::Crop => self.crop_controls(ui, enabled),
            // No tool: just the tabs (no locked stand-in), so the column never scrolls.
            ToolMode::None => {}
        }
    }

    fn crop_controls(&mut self, ui: &mut egui::Ui, on: bool) {
        const NAMES: [&str; 6] = ["原始", "3:2", "4:3", "16:9", "1:1", "自訂"];
        const VALUES: [&str; 6] = ["Original", "3:2", "4:3", "16:9", "1:1", "Custom"];
        ui.horizontal(|ui| {
            // Fits the narrow column: the label as long as it is (capped), the W:H boxes
            // narrow, the combo box whatever is left.
            let gap = ui.spacing().item_spacing.x;
            let font = egui::TextStyle::Body.resolve(ui.style());
            let label_w = widgets::text_width(ui, t("比例"), &font).min(64.0);
            let colon_w = widgets::text_width(ui, ":", &font);
            let num_w = 30.0;
            ui.spacing_mut().interact_size.x = num_w; // a DragValue is never narrower than this
            let combo_w = (ui.available_width() - label_w - 2.0 * num_w - colon_w - 4.0 * gap).max(40.0);
            widgets::fixed_text(ui, label_w, 20.0, t("比例"), font.clone(), theme::TEXT_DIM);
            let cur = if self.crop_custom.is_some() { 5 } else { tools::aspect_index(&self.adj.crop_aspect_ratio) };
            let mut sel = cur;
            // In a box of exactly combo_w, so a long name ("Benutzerdefiniert") is cut.
            ui.allocate_ui(Vec2::new(combo_w, ui.spacing().interact_size.y), |ui| {
                ui.add_enabled_ui(on, |ui| {
                    egui::ComboBox::from_id_salt("crop_aspect").width(combo_w).truncate().selected_text(t(NAMES[cur])).show_ui(ui, |ui| {
                        for (i, name) in NAMES.iter().enumerate() {
                            ui.selectable_value(&mut sel, i, t(name));
                        }
                    });
                });
            });
            // 自訂 W:H, restored from a stored "W:H" that is not one of the presets.
            let (mut cw, mut ch) = self.crop_custom.unwrap_or_else(|| custom_ratio(&self.adj.crop_aspect_ratio).unwrap_or((3, 2)));
            let custom = on && sel == 5;
            let r1 = ui.add_enabled_ui(custom, |ui| ui.add_sized([num_w, 20.0], egui::DragValue::new(&mut cw).range(1..=99))).inner;
            ui.label(":");
            let r2 = ui.add_enabled_ui(custom, |ui| ui.add_sized([num_w, 20.0], egui::DragValue::new(&mut ch).range(1..=99))).inner;
            if sel != cur {
                if sel == 5 {
                    // C#: 自訂 stores the W:H numbers, never the word "Custom".
                    self.crop_custom = Some((cw, ch));
                    self.set_crop_aspect(format!("{cw}:{ch}"));
                } else {
                    self.crop_custom = None;
                    self.set_crop_aspect(VALUES[sel].to_string());
                }
            } else if custom && (r1.changed() || r2.changed()) {
                self.crop_custom = Some((cw, ch));
                self.set_crop_aspect(format!("{cw}:{ch}"));
            }
        });
        // 角度 shows −CropAngle (the user wanted the direction flipped; the stored value
        // keeps its meaning, so old XML is unaffected). 0.0 − x keeps −0 out.
        let angle = SliderSpec { min: -45.0, max: 45.0, decimals: 1, wheel_step: 0.5, ..SliderSpec::pm100(t("角度")) };
        self.slider_with(ui, angle, on, |a| 0.0 - a.crop_angle, |a, v| a.crop_angle = 0.0 - v);
        self.slider_with(ui, SliderSpec::pm100(t("廣角變形")), on, |a| a.distortion, |a, v| a.distortion = v);
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            let half = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
            if widgets::fixed_button(ui, on, Vec2::new(half, 28.0), t("照片左轉90度"), |b| b).clicked() {
                self.rotate(false);
            }
            if widgets::fixed_button(ui, on, Vec2::new(half, 28.0), t("照片右轉90度"), |b| b).clicked() {
                self.rotate(true);
            }
        });
        if widgets::wide_button(ui, on, 28.0, t("裁切重設"), |b| b).clicked() {
            self.reset_crop();
        }
    }

    fn gradient_controls(&mut self, ui: &mut egui::Ui, on: bool) {
        // The sliders edit the selected gradient; locked when there is none.
        let active = self.active_gradient();
        let has = on && active.is_some();
        let lim = if self.adj.is_legacy_pipeline() { 2.0 } else { 5.0 };
        let fields: [(SliderSpec, fn(&LinearGradient) -> f64, fn(&mut LinearGradient, f64)); 5] = [
            (SliderSpec { min: -lim, max: lim, decimals: 2, wheel_step: 0.05, ..SliderSpec::pm100(t("曝光")) }, |g| g.exposure, |g, v| g.exposure = v),
            (SliderSpec::pm100(t("對比")), |g| g.contrast, |g, v| g.contrast = v),
            (SliderSpec::pm100(t("亮部")), |g| g.highlights, |g, v| g.highlights = v),
            (SliderSpec::pm100(t("暗部")), |g| g.shadows, |g, v| g.shadows = v),
            (SliderSpec::pm100(t("飽和度")).gradient(Gradient::Saturation), |g| g.saturation, |g, v| g.saturation = v),
        ];
        for (spec, get, set) in fields {
            self.slider_with(ui, spec, has, |a| active.map_or(spec.default, |i| get(&a.gradients[i])), |a, v| {
                if let Some(i) = active {
                    set(&mut a.gradients[i], v);
                }
            });
        }
        ui.add_space(6.0);
        if widgets::wide_button(ui, on, 28.0, RichText::new(t("新增線性漸層")).color(Color32::WHITE), |b| b.fill(theme::ACCENT)).clicked() {
            self.add_gradient();
        }
        if widgets::wide_button(ui, on, 28.0, t("漸層重設（清除全部）"), |b| b).clicked() {
            self.clear_gradients();
        }
    }

    fn heal_controls(&mut self, ui: &mut egui::Ui, on: bool) {
        ui.horizontal(|ui| {
            let half = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
            for (label, mode) in [(t("仿製"), HealMode::Clone), (t("修補"), HealMode::Inpaint)] {
                let sel = self.heal_mode == mode;
                if widgets::fixed_button(ui, on, Vec2::new(half, 28.0), label, |b| b.selected(sel)).clicked() {
                    self.set_heal_mode(mode);
                }
            }
        });
        // 大小 sets the brush for new spots and live-resizes the active one.
        let size = SliderSpec { min: 0.0, max: 50.0, default: 10.0, bipolar: false, ..SliderSpec::pm100(t("大小")) };
        let spot = self.active_spot();
        self.slider_with(ui, size, on, |a| a.heal_size, |a, v| {
            a.heal_size = v;
            if let Some(i) = spot {
                tools::resize_heal_spot(&mut a.heal_spots[i], v);
            }
        });
        ui.add_space(2.0);
        if widgets::wide_button(ui, on, 28.0, t("修護重設"), |b| b).clicked() {
            self.clear_heal();
        }
    }

    fn right_bottom(&mut self, ui: &mut egui::Ui) {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
        let on = self.has_photo();
        let w = ui.available_width();
        ui.add_space(6.0);
        if widgets::wide_button(ui, on, 30.0, t("全部重設"), |b| b).clicked() {
            self.reset_all();
        }
        ui.horizontal(|ui| {
            let half = (w - 6.0) / 2.0;
            if widgets::fixed_button(ui, on && !self.undo.is_empty(), Vec2::new(half, 30.0), t("恢復上一步"), |b| b).clicked() {
                self.do_undo();
            }
            if widgets::fixed_button(ui, on && !self.redo.is_empty(), Vec2::new(half, 30.0), t("重做"), |b| b).clicked() {
                self.do_redo();
            }
        });
    }

    fn viewer_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.add_space(6.0);
            let on = self.has_photo();
            for (label, mode) in [(t("適合"), ZoomMode::Fit), ("100%", ZoomMode::Actual100), ("200%", ZoomMode::Actual200)] {
                if ui.add_enabled(on, egui::Button::new(label).selected(self.viewer.mode == mode)).clicked() {
                    self.viewer.set_mode(mode);
                }
            }
            if ui.add_enabled(on, egui::Button::new(t("對照原圖")).selected(self.show_original)).clicked() {
                self.show_original = !self.show_original;
                self.needs_render = true;
            }
            if on {
                ui.label(RichText::new(format!("{:.0}%", self.viewer.zoom_percent())).color(theme::TEXT_DIM));
            }
            ui.add_space(12.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                ui.label(RichText::new(&self.render_note).size(theme::scaled(12.0)).color(theme::TEXT_FAINT));
                ui.add(egui::Label::new(RichText::new(&self.status).size(theme::scaled(12.0)).color(theme::TEXT_DIM)).truncate());
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
            let msg = if self.loading { t("載入中…") } else if self.items.is_empty() { "" } else { t("沒有可顯示的照片") };
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, msg, egui::FontId::proportional(theme::scaled(16.0)), theme::TEXT_FAINT);
            return;
        };
        // A tool or the picker owns the left button; the middle button still pans.
        let left_free = self.tool == ToolMode::None && !self.wb_picker;
        let pl: Placement = self.viewer.update(ui, &resp, size, left_free);
        match &self.display {
            Display::Gpu { bind, .. } => viewer::paint_gpu(ui, &pl, bind, size),
            Display::Cpu { tex, .. } => viewer::paint_texture(ui, &pl, tex),
            Display::None => {}
        }
        let v = View {
            ox: pl.image.min.x as f64,
            oy: pl.image.min.y as f64,
            scale: (pl.image.width() / size.x) as f64,
            w: size.x as f64,
            h: size.y as f64,
        };
        if self.has_photo() {
            self.tool_input(ui, &resp, &v);
        }
        let painter = ui.painter().with_clip_rect(pl.view);
        match self.tool {
            ToolMode::Crop => self.paint_crop(&painter, &v, pl.image),
            ToolMode::Gradient => self.paint_gradients(&painter, &v, pl.view),
            ToolMode::Heal => self.paint_heal(&painter, &v),
            ToolMode::Mask => self.paint_masks(ui, &painter, &v, pl.image),
            ToolMode::None => {}
        }
        // The export watermark, live (the C# preview drew it whenever it was enabled).
        if let (Some(p), Some(e)) = (&self.proxy, &self.exif) {
            let full_long = e.width.max(e.height) as f64;
            let ratio = if full_long > 0.0 { p.width.max(p.height) as f64 / full_long } else { 1.0 };
            self.wm_overlay.paint(ui, &painter, &self.export_settings, pl.image, (size.x as usize, size.y as usize), ratio, pl.scale_px as f64);
        }
        if self.wb_picker {
            painter.text(egui::pos2(rect.center().x, rect.min.y + 10.0), egui::Align2::CENTER_TOP, t("點擊中性灰色區域設定白平衡"), egui::FontId::proportional(theme::scaled(14.0)), theme::TEXT);
        }
        if self.show_original {
            ui.painter().text(rect.left_top() + Vec2::new(12.0, 10.0), egui::Align2::LEFT_TOP, t("原圖"), egui::FontId::proportional(theme::scaled(14.0)), Color32::WHITE);
        }
    }

    /// Pointer input for the picker and the tools (the C# viewer's mouse handlers).
    fn tool_input(&mut self, ui: &egui::Ui, resp: &egui::Response, v: &View) {
        let (pressed, down, right, pos) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.secondary_pressed(), i.pointer.interact_pos()));
        let Some(pos) = pos else { return };
        let p = P::new(pos.x as f64, pos.y as f64);
        let over = resp.contains_pointer();
        if pressed && over {
            if self.wb_picker {
                let (nx, ny) = v.ctrl_to_norm(p);
                self.pick_white_balance(nx, ny);
                return;
            }
            self.begin_tool_drag(p, v);
            self.drag_last = Some(pos);
        } else if self.drag != Drag::None {
            if !down {
                self.drag = Drag::None;
            } else if self.drag_last != Some(pos) {
                self.drag_last = Some(pos);
                self.update_tool_drag(p, v);
            }
        }
        if right && over {
            match self.tool {
                ToolMode::Heal => {
                    if let Some(i) = tools::heal_target_at(&self.adj, v, p) {
                        self.delete_heal_spot(i);
                    }
                }
                ToolMode::Gradient => self.grad_menu = tools::gradient_at_point(&self.adj, v, p),
                _ => {}
            }
        }
        if self.tool == ToolMode::Gradient {
            resp.context_menu(|ui| match self.grad_menu {
                Some(i) => {
                    if ui.button(t("刪除此線性漸層")).clicked() {
                        self.delete_gradient(i);
                        self.grad_menu = None;
                        ui.close();
                    }
                }
                None => ui.close(),
            });
        }
        // Cursor feedback: which crop handle a press would grab; elsewhere over the
        // photo a pointing hand (a closed hand while panning or dragging a handle).
        if over || self.drag != Drag::None {
            let d = if self.drag != Drag::None {
                self.drag
            } else if self.tool == ToolMode::Crop {
                tools::crop_hit_test(&tools::crop_ctrl_rect(&self.adj, v), p)
            } else {
                Drag::None
            };
            let icon = match d {
                _ if self.wb_picker => Some(egui::CursorIcon::Crosshair),
                Drag::CropTL | Drag::CropBR => Some(egui::CursorIcon::ResizeNwSe),
                Drag::CropTR | Drag::CropBL => Some(egui::CursorIcon::ResizeNeSw),
                Drag::CropL | Drag::CropR => Some(egui::CursorIcon::ResizeHorizontal),
                Drag::CropT | Drag::CropB => Some(egui::CursorIcon::ResizeVertical),
                Drag::CropMove => Some(egui::CursorIcon::Move),
                Drag::None if resp.dragged() => Some(egui::CursorIcon::Grabbing),
                Drag::None | Drag::MaskPaint => Some(egui::CursorIcon::PointingHand),
                _ => Some(egui::CursorIcon::Grabbing),
            };
            if let Some(icon) = icon {
                ui.ctx().set_cursor_icon(icon);
            }
        }
    }

    /// Crop box: dimmed outside, rule-of-thirds lines, corner and edge handles.
    fn paint_crop(&self, painter: &egui::Painter, v: &View, full: egui::Rect) {
        let b = tools::crop_ctrl_rect(&self.adj, v);
        let r = egui::Rect::from_min_max(egui::pos2(b.left as f32, b.top as f32), egui::pos2(b.right as f32, b.bottom as f32));
        let dim = Color32::from_black_alpha(140);
        for band in [
            egui::Rect::from_min_max(full.min, egui::pos2(full.max.x, r.min.y)),
            egui::Rect::from_min_max(egui::pos2(full.min.x, r.max.y), full.max),
            egui::Rect::from_min_max(egui::pos2(full.min.x, r.min.y), egui::pos2(r.min.x, r.max.y)),
            egui::Rect::from_min_max(egui::pos2(r.max.x, r.min.y), egui::pos2(full.max.x, r.max.y)),
        ] {
            if band.width() > 0.0 && band.height() > 0.0 {
                painter.rect_filled(band, 0.0, dim);
            }
        }
        painter.rect_stroke(r, 0.0, egui::Stroke::new(1.5, Color32::WHITE), egui::StrokeKind::Middle);
        let thin = egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 120));
        for i in 1..3 {
            let x = r.min.x + r.width() * i as f32 / 3.0;
            let y = r.min.y + r.height() * i as f32 / 3.0;
            painter.line_segment([egui::pos2(x, r.min.y), egui::pos2(x, r.max.y)], thin);
            painter.line_segment([egui::pos2(r.min.x, y), egui::pos2(r.max.x, y)], thin);
        }
        // Corner handles bigger than the edge ones, matching their bigger grab zone.
        for c in [r.left_top(), r.right_top(), r.left_bottom(), r.right_bottom()] {
            painter.rect_filled(egui::Rect::from_center_size(c, Vec2::splat(10.0)), 0.0, Color32::WHITE);
        }
        for c in [r.center_top(), r.center_bottom(), r.left_center(), r.right_center()] {
            painter.rect_filled(egui::Rect::from_center_size(c, Vec2::splat(6.0)), 0.0, Color32::WHITE);
        }
    }

    fn paint_gradients(&mut self, painter: &egui::Painter, v: &View, view: egui::Rect) {
        let active = self.active_gradient();
        let len = view.width().max(view.height()) as f64;
        // Inactive gradients first, the active one on top.
        for (i, g) in self.adj.gradients.iter().enumerate() {
            if Some(i) != active {
                paint_gradient(painter, g, v, len, false);
            }
        }
        if let Some(i) = active {
            paint_gradient(painter, &self.adj.gradients[i], v, len, true);
        }
    }

    fn paint_heal(&self, painter: &egui::Painter, v: &View) {
        let active = self.active_spot();
        for (i, s) in self.adj.heal_spots.iter().enumerate() {
            let r = tools::heal_radius_px(s, v) as f32;
            let t = v.norm_to_ctrl(s.target_x, s.target_y);
            let tc = egui::pos2(t.x as f32, t.y as f32);
            // The selected spot (what 大小 / 仿製／修補 / Delete act on) draws thicker.
            let width = if Some(i) == active { 2.6 } else { 1.6 };
            painter.circle_stroke(tc, r, egui::Stroke::new(width, Color32::from_rgba_unmultiplied(90, 200, 120, 230)));
            if !s.use_inpaint {
                let sp = v.norm_to_ctrl(s.source_x, s.source_y);
                let sc = egui::pos2(sp.x as f32, sp.y as f32);
                let pen = egui::Stroke::new(1.4, Color32::from_rgba_unmultiplied(120, 180, 240, 200));
                let ring: Vec<egui::Pos2> = (0..=64).map(|k| sc + Vec2::angled(k as f32 / 64.0 * std::f32::consts::TAU) * r).collect();
                painter.extend(egui::Shape::dashed_line(&ring, pen, 4.2, 1.4));
                painter.extend(egui::Shape::dashed_line(&[sc, tc], pen, 4.2, 1.4));
            }
        }
    }

    // ---- --shot -------------------------------------------------------------------

    fn shot_step(&mut self, ctx: &egui::Context) {
        let waiting_v3 = self.waiting_for_v3_proxy();
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
        let ready = !self.loading && self.hist.is_some() && self.cache_progress.is_none() && !self.needs_render && self.hist_job.is_none() && self.thumb_live_due.is_none() && !waiting_v3;
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

/// `text` cut from the left ("…tail") to fit `max` points.
fn elide_left(ui: &egui::Ui, text: &str, font: &egui::FontId, max: f32) -> String {
    let width = |s: &str| ui.painter().layout_no_wrap(s.to_string(), font.clone(), Color32::WHITE).size().x;
    if width(text) <= max {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    // The longest tail that fits after the ellipsis (binary search on its length).
    let (mut lo, mut hi) = (0, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let tail: String = chars[chars.len() - mid..].iter().collect();
        if width(&format!("…{tail}")) <= max {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let tail: String = chars[chars.len() - lo..].iter().collect();
    format!("…{tail}")
}

/// A stored "W:H" that is not one of the 比例 presets → the 自訂 numbers (1–99).
fn custom_ratio(aspect: &str) -> Option<(u32, u32)> {
    if tools::aspect_index(aspect) != 5 {
        return None;
    }
    let (w, h) = aspect.split_once(':')?;
    let (w, h) = (w.trim().parse::<f64>().ok()?, h.trim().parse::<f64>().ok()?);
    Some((w.round().clamp(1.0, 99.0) as u32, h.round().clamp(1.0, 99.0) as u32))
}

/// One gradient: its line, and for the active one the range band, the yellow range
/// handle, the blue rotate handle and the white position handle (`DrawOneGradient`).
fn paint_gradient(painter: &egui::Painter, g: &LinearGradient, v: &View, len: f64, active: bool) {
    let pt = |p: P| egui::pos2(p.x as f32, p.y as f32);
    let c = v.norm_to_ctrl(g.center_x, g.center_y);
    let (ux, uy) = tools::grad_axis(g);
    let (lx, ly) = (-uy, ux); // the line runs across the axis
    let through = |p: P| [pt(P::new(p.x - lx * len, p.y - ly * len)), pt(P::new(p.x + lx * len, p.y + ly * len))];
    let yellow = |a: u8| Color32::from_rgba_unmultiplied(255, 220, 60, a);
    painter.line_segment(through(c), egui::Stroke::new(if active { 1.6 } else { 1.2 }, yellow(if active { 230 } else { 100 })));
    let hr = 10.0; // handle radius (doubled 5 → 10 in v1.0.18)
    if !active {
        // A white dot so an inactive gradient can be clicked to select it.
        painter.circle_filled(pt(c), hr, Color32::from_rgba_unmultiplied(255, 255, 255, 200));
        return;
    }
    let d = g.range * v.h * v.scale;
    let c1 = P::new(c.x + ux * d, c.y + uy * d);
    let c2 = P::new(c.x - ux * d, c.y - uy * d);
    let dash = egui::Stroke::new(1.0, yellow(120));
    painter.extend(egui::Shape::dashed_line(&through(c1), dash, 3.0, 1.0));
    painter.extend(egui::Shape::dashed_line(&through(c2), dash, 3.0, 1.0));
    painter.circle_filled(pt(c1), hr, yellow(255));
    let rot = pt(tools::rotate_handle_pos(c, g));
    painter.line_segment([pt(c), rot], egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 200, 255, 200)));
    paint_rotate_icon(painter, rot, hr);
    painter.circle_filled(pt(c), 12.0, Color32::WHITE);
}

/// The blue rotate handle: a dark disc with a blue ring and a clockwise arc arrow.
fn paint_rotate_icon(painter: &egui::Painter, c: egui::Pos2, r: f32) {
    let blue = Color32::from_rgb(120, 200, 255);
    painter.circle(c, r, Color32::from_rgba_unmultiplied(25, 35, 50, 200), egui::Stroke::new(1.0, blue));
    let ar = r * 0.55;
    // GDI DrawArc(start 60°, sweep 250°), y down.
    let arc: Vec<egui::Pos2> = (0..=32).map(|k| c + Vec2::angled((60.0 + 250.0 * k as f32 / 32.0).to_radians()) * ar).collect();
    painter.add(egui::Shape::line(arc, egui::Stroke::new(2.0, blue)));
    let th = 310f32.to_radians();
    let end = c + Vec2::angled(th) * ar;
    let t = Vec2::new(-th.sin(), th.cos()); // clockwise tangent
    let n = Vec2::new(-t.y, t.x);
    let s = r * 0.35;
    painter.add(egui::Shape::convex_polygon(vec![end + t * s * 1.8, end + n * s, end - n * s], blue, egui::Stroke::NONE));
}

/// "exposure=0.5,contrast=20" → the adjustments (shot mode only). 處理版本 3:
/// `version=3`, `hlr=` (高光復原), `hue0..7=` / `sat0..7=` / `lum0..7=` (HSL bands 紅…洋紅),
/// `curve=1` (an S curve on RGB plus warm red / cool blue curves).
fn apply_adjust_spec(a: &mut ImageAdjustments, spec: &str) {
    for part in spec.split(',') {
        let Some((k, v)) = part.split_once('=') else { continue };
        let Ok(v) = v.trim().parse::<f64>() else { continue };
        let k = k.trim();
        let band = |p: &str| k.strip_prefix(p).and_then(|n| n.parse::<usize>().ok()).filter(|&n| n < 8);
        if let Some(b) = band("hue") {
            a.hsl_hue[b] = v;
            continue;
        }
        if let Some(b) = band("sat") {
            a.hsl_saturation[b] = v;
            continue;
        }
        if let Some(b) = band("lum") {
            a.hsl_luminance[b] = v;
            continue;
        }
        match k {
            "version" => a.pipeline_version = (v as i32 - 1).clamp(0, ImageAdjustments::CURRENT_PIPELINE_VERSION),
            "hlr" => a.highlight_recovery = v,
            "curve" if v != 0.0 => {
                a.curve_rgb = vec![(0.0, 0.0), (0.25, 0.18), (0.75, 0.84), (1.0, 1.0)];
                a.curve_red = vec![(0.0, 0.0), (0.5, 0.55), (1.0, 1.0)];
                a.curve_blue = vec![(0.0, 0.03), (0.5, 0.46), (1.0, 1.0)];
            }
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
        self.update_ui_scale(&ctx);
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

        let top_h = Self::top_button_h(ui) + 16.0;
        egui::Panel::top("top").exact_size(top_h).frame(egui::Frame::new().fill(theme::TOOLBAR)).show(ui, |ui| self.top_bar(ui));
        // The right column runs from the toolbar to the bottom; the strip spans only the
        // left column and the viewer, so the tools get the full height.
        egui::Panel::right("right")
            .exact_size(SIDE_W)
            .resizable(false)
            .frame(egui::Frame::new().fill(theme::WINDOW).inner_margin(egui::Margin::symmetric(4, 0)))
            .show(ui, |ui| {
                egui::Panel::bottom("right_bottom").exact_size(80.0).frame(egui::Frame::new().fill(theme::WINDOW)).show(ui, |ui| self.right_bottom(ui));
                egui::CentralPanel::no_frame().show(ui, |ui| {
                    // The tool panels (遮罩 above all) can be taller than the column: its
                    // scroll bar shows whenever that happens, whatever 顯示捲軸 says.
                    let bars = egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded;
                    ui.style_mut().spacing.scroll = theme::side_scroll_style();
                    let to_end = self.headless() && std::env::var("AWPR_SHOT_SCROLL").as_deref() == Ok("bottom");
                    egui::ScrollArea::vertical().id_salt("right_scroll").auto_shrink([false, false]).scroll_bar_visibility(bars).show(ui, |ui| {
                        self.right_column(ui);
                        if to_end {
                            ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                        }
                    });
                });
            });
        egui::Panel::top("strip")
            .exact_size(144.0)
            .frame(egui::Frame::new().fill(theme::WINDOW).inner_margin(egui::Margin::symmetric(0, 4)))
            .show(ui, |ui| self.strip(ui));
        egui::Panel::left("left")
            .exact_size(SIDE_W)
            .resizable(false)
            .frame(egui::Frame::new().fill(theme::WINDOW).inner_margin(egui::Margin::symmetric(4, 0)))
            .show(ui, |ui| {
                let bars = self.scroll_bars();
                if bars != egui::scroll_area::ScrollBarVisibility::AlwaysHidden {
                    ui.style_mut().spacing.scroll = theme::side_scroll_style();
                }
                egui::ScrollArea::vertical().id_salt("left_scroll").auto_shrink([false, false]).scroll_bar_visibility(bars).show(ui, |ui| self.left_column(ui));
            });
        egui::CentralPanel::no_frame().show(ui, |ui| {
            egui::Panel::bottom("viewer_bar").exact_size(36.0).frame(egui::Frame::new().fill(theme::TOOLBAR)).show(ui, |ui| self.viewer_toolbar(ui));
            egui::CentralPanel::no_frame().show(ui, |ui| self.viewer(ui));
        });
        self.export_windows(&ctx);
        self.strip_menu_ui(&ctx);
        self.app_menu_ui(&ctx);
        self.settings_ui(&ctx);
        self.about_ui(&ctx);
        self.cameras_ui(&ctx);
        self.confirm_ui(&ctx);
        self.first_run_ui(&ctx);
        if let Some(mut ed) = self.preset_editor.take() {
            let headless = self.headless();
            let keep = ed.show(&ctx, &mut self.presets, &mut |p: &PresetCollection| {
                if !headless {
                    let _ = p.save();
                }
            });
            if keep {
                self.preset_editor = Some(ed);
            }
        }
        self.shot_step(&ctx);
        let _ = &self.font;
    }

    fn on_exit(&mut self) {
        crate::trace("on_exit");
        self.save_current_if_dirty();
    }
}
