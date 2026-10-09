//! The ☰ app menu and the windows behind it: 設定 (with 字體大小…), the first-run language
//! choice, 關於 with 檢查更新, the supported-camera list — plus the interface size (C#
//! `Ui.Init`: automatic = min(system scaling, what fits the screen)).

use super::library::Confirm;
use super::{App, DESIGN_HEIGHT};
use crate::i18n::{self, f, t, Lang};
use crate::settings::{FontKind, FontSizes, Settings};
use crate::theme;
use crate::update::{self, UpdateInfo};
use awpr_photo::{library, paths};
use eframe::egui::{self, Color32, RichText, Vec2};
use std::sync::mpsc::Receiver;

/// Fixed interface sizes offered besides 自動 (C# `ScalePercents`).
const SCALE_PERCENTS: [i64; 5] = [100, 125, 150, 175, 200];
/// How many folders 紀錄 lists.
const RECENT_SHOWN: usize = 10;

/// The open 設定 window: a draft until 套用.
pub(super) struct SettingsDraft {
    s: Settings,
    /// 字體大小… open, with its own draft.
    fonts: Option<FontSizes>,
    help: bool,
}

impl SettingsDraft {
    pub(super) fn new(s: &Settings, with_fonts: bool) -> Self {
        Self { s: s.clone(), fonts: with_fonts.then_some(s.font_sizes), help: false }
    }
}

pub(super) enum UpdateState {
    Idle,
    Checking(Receiver<Option<UpdateInfo>>),
    Done(Option<UpdateInfo>),
}

impl App {
    // ---- interface size ---------------------------------------------------------------

    /// Apply 介面大小 (or AWPR_UI_SCALE) as egui's zoom once the monitor size is known, and
    /// again whenever the setting changes.
    pub(super) fn update_ui_scale(&mut self, ctx: &egui::Context) {
        let env = std::env::var("AWPR_UI_SCALE").ok().and_then(|v| v.parse::<f32>().ok()).filter(|v| *v >= 0.5 && *v <= 4.0);
        let want = (self.settings.ui_scale_percent, env.map(|v| (v * 100.0).round() as i64));
        if self.applied_scale == Some(want) {
            return;
        }
        let Some(m) = ctx.input(|i| i.viewport().monitor_size) else { return };
        let zoom = ctx.zoom_factor();
        let native_ppp = ctx.pixels_per_point() / zoom;
        // monitor_size is in points at the current zoom: back to physical pixels.
        let screen_px = m.y * zoom * native_ppp;
        let fit = screen_px / DESIGN_HEIGHT;
        let auto = native_ppp.min(fit);
        self.auto_percent = (auto * 100.0).round() as i64;
        let effective = env.unwrap_or(if self.settings.ui_scale_percent == 0 { auto } else { self.settings.ui_scale_percent as f32 / 100.0 });
        // Bigger than the screen holds: the columns must scroll or blocks get cut off.
        self.force_scroll = effective > fit + 0.01;
        let z = (effective / native_ppp).clamp(0.3, 4.0);
        if (z - zoom).abs() > 0.001 {
            ctx.set_zoom_factor(z);
        }
        crate::trace(&format!("ui scale: native {native_ppp} screen {screen_px}px fit {fit} effective {effective} zoom {z}"));
        self.applied_scale = Some(want);
    }

    /// Language, type sizes and GPU from new settings, live (egui needs no restart).
    fn apply_settings(&mut self, ctx: &egui::Context, new: Settings) {
        let old = std::mem::replace(&mut self.settings, new);
        if old.language != self.settings.language {
            i18n::set_lang(self.settings.language);
            self.font = theme::install_fonts(ctx, self.settings.language);
        }
        if old.font_sizes != self.settings.font_sizes {
            theme::set_font_sizes(ctx, self.settings.font_sizes);
        }
        if old.ui_scale_percent != self.settings.ui_scale_percent {
            self.applied_scale = None;
        }
        if old.use_gpu != self.settings.use_gpu {
            self.set_gpu(self.settings.use_gpu);
        }
        if old.show_hidden != self.settings.show_hidden {
            self.refresh_items_keep_selection();
        }
        if !self.headless() {
            self.settings.save();
        }
    }

    /// Switch GPU rendering on or off without a restart.
    fn set_gpu(&mut self, on: bool) {
        if !on {
            self.gpu = None;
            self.proxy_gpu = None;
            self.gpu_status = t("已關閉 GPU 加速（CPU 算圖）").into();
        } else if self.gpu.is_none() {
            if let Some(rs) = &self.rs {
                match awpr_gpu::GpuPipeline::with_device(&rs.adapter.get_info(), rs.device.clone(), rs.queue.clone()) {
                    Ok(p) => {
                        self.gpu_status = p.status.clone();
                        self.gpu = Some(&*Box::leak(Box::new(p)));
                    }
                    Err(e) => self.gpu_status = f("GPU 無法使用，改用 CPU：{0}", &[&e]),
                }
            }
            if let (Some(g), Some(p)) = (self.gpu, &self.proxy) {
                if g.can_host(p.width, p.height) {
                    self.proxy_gpu = g.upload(p).ok();
                }
            }
        }
        self.needs_render = true;
    }

    // ---- ☰ menu -------------------------------------------------------------------------

    /// The ☰ button at the left of the top bar.
    pub(super) fn app_menu_button(&mut self, ui: &mut egui::Ui) {
        let b = egui::Button::new(RichText::new("☰").size(theme::fs(FontKind::MenuGlyph))).frame(false);
        let r = ui.add(b);
        if r.clicked() {
            self.app_menu = Some((r.rect.left_bottom(), true));
        }
    }

    pub(super) fn app_menu_ui(&mut self, ctx: &egui::Context) {
        let Some((pos, just_opened)) = self.app_menu else { return };
        let has_folder = !self.folder.is_empty();
        let hidden = if has_folder { self.preview_list.hidden.len() } else { 0 };
        enum Act {
            Open,
            Close,
            ClearCache,
            RestoreHidden,
            Refresh,
            Recent(String),
            ClearRecent,
            Settings,
            Presets,
            ExportXmp,
            ImportXmp,
            Cameras,
            About,
            Quit,
        }
        let mut act = None;
        let area = egui::Area::new(egui::Id::new("app_menu")).order(egui::Order::Foreground).fixed_pos(pos).constrain(true).show(ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                ui.set_min_width(260.0);
                let mut item = |ui: &mut egui::Ui, text: &str, enabled: bool, a: Act| {
                    if ui.add_enabled(enabled, egui::Button::new(text).frame(false).min_size(Vec2::new(250.0, 22.0))).clicked() {
                        act = Some(a);
                    }
                };
                item(ui, t("開啟資料夾…"), true, Act::Open);
                item(ui, t("關閉資料夾"), has_folder, Act::Close);
                item(ui, t("關閉資料夾並刪除快取縮圖"), has_folder, Act::ClearCache);
                let restore = if hidden > 0 { f("還原已隱藏的照片（{0} 張）", &[&hidden]) } else { t("還原已隱藏的照片").to_string() };
                item(ui, &restore, hidden > 0, Act::RestoreHidden);
                item(ui, t("重新整理資料夾  (F5)"), has_folder, Act::Refresh);
                egui::CollapsingHeader::new(t("紀錄")).id_salt("menu_recent").show(ui, |ui| {
                    if self.settings.recent_folders.is_empty() {
                        ui.add_enabled(false, egui::Label::new(t("（尚無開啟紀錄）")));
                    } else {
                        for p in self.settings.recent_folders.iter().take(RECENT_SHOWN) {
                            item(ui, p, true, Act::Recent(p.clone()));
                        }
                        ui.separator();
                        item(ui, t("清除紀錄"), true, Act::ClearRecent);
                    }
                });
                ui.separator();
                item(ui, t("設定…"), true, Act::Settings);
                item(ui, t("編輯風格檔…"), true, Act::Presets);
                if self.settings.xmp_support {
                    let photo = self.has_photo();
                    item(ui, t("匯出 XMP"), photo, Act::ExportXmp);
                    item(ui, t("匯入 XMP"), photo, Act::ImportXmp);
                }
                ui.separator();
                item(ui, t("支援RAW檔相機列表"), true, Act::Cameras);
                item(ui, t("關於"), true, Act::About);
                item(ui, t("結束"), true, Act::Quit);
            });
        });
        let outside = ctx.input(|i| i.pointer.any_pressed()) && !area.response.contains_pointer();
        if just_opened {
            self.app_menu = Some((pos, false));
        } else if outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.app_menu = None;
        }
        let Some(a) = act else { return };
        self.app_menu = None;
        match a {
            Act::Open => self.pick_folder(),
            Act::Close => self.close_folder(),
            Act::ClearCache => self.confirm = Some(Confirm::ClearCache),
            Act::RestoreHidden => self.restore_hidden(),
            Act::Refresh => {
                let folder = self.folder.clone();
                self.open_folder(&folder);
                self.status = t("已重新整理資料夾").into();
            }
            Act::Recent(p) => {
                if std::path::Path::new(&p).is_dir() {
                    self.open_folder(&p);
                } else {
                    self.status = f("資料夾已不存在：\n{0}", &[&p]).replace('\n', " ");
                }
            }
            Act::ClearRecent => {
                self.settings.recent_folders.clear();
                if !self.headless() {
                    self.settings.save();
                }
            }
            Act::Settings => self.open_settings(),
            Act::Presets => self.preset_editor = Some(crate::presets_ui::PresetEditor::new(&self.presets)),
            Act::ExportXmp => self.export_xmp_selected(),
            Act::ImportXmp => self.import_xmp_selected(),
            Act::Cameras => self.cameras = Some((String::new(), awpr_core::libraw::camera_list())),
            Act::About => self.about = Some(UpdateState::Idle),
            Act::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
    }

    /// 還原已隱藏的照片: un-hide everything in this folder.
    fn restore_hidden(&mut self) {
        let n = self.preview_list.hidden.len();
        if n == 0 {
            self.status = t("沒有已隱藏的照片").into();
            return;
        }
        self.preview_list.hidden.clear();
        if !self.headless() {
            let _ = self.preview_list.save(&self.folder);
        }
        self.refresh_items_keep_selection();
        self.status = f("已還原 {0} 張隱藏的照片", &[&n]);
    }

    /// 關閉資料夾並刪除快取縮圖 (after the confirmation).
    pub(super) fn clear_cache_confirmed(&mut self) {
        let folder = self.folder.clone();
        self.close_folder();
        if self.headless() || folder.is_empty() {
            return;
        }
        let n = library::delete_cache_files(&folder);
        self.status = f("已關閉資料夾並刪除快取縮圖（{0} 個檔案）", &[&n]);
    }

    // ---- 設定 ----------------------------------------------------------------------------

    pub(super) fn open_settings(&mut self) {
        self.settings_dlg = Some(SettingsDraft::new(&self.settings, false));
    }

    fn gpu_state_text(&self, enabled: bool) -> String {
        if !enabled {
            t("GPU：已停用（設定）").into()
        } else if let Some(g) = self.gpu {
            format!("{}{}", t("GPU："), g.status)
        } else if self.settings.use_gpu && !self.gpu_status.is_empty() {
            // Why the GPU is not in use (the reason the toolbar used to show).
            self.gpu_status.clone()
        } else {
            t("GPU：未偵測到可用裝置，使用 CPU").into()
        }
    }

    pub(super) fn settings_ui(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.settings_dlg.take() else { return };
        let mut done: Option<bool> = None;
        let gpu_line = self.gpu_state_text(d.s.use_gpu);
        let auto = self.auto_percent;
        egui::Window::new(t("設定")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            ui.set_width(520.0);
            let section = |ui: &mut egui::Ui, text: &str| {
                ui.add_space(6.0);
                ui.label(RichText::new(text).strong().color(theme::ACCENT));
            };
            section(ui, t("語言"));
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("set_lang").width(260.0).selected_text(d.s.language.display_name()).show_ui(ui, |ui| {
                    for l in Lang::ALL {
                        ui.selectable_value(&mut d.s.language, l, l.display_name());
                    }
                });
                ui.label(RichText::new(t("變更後立即套用")).size(theme::fs(FontKind::Small)).color(theme::TEXT_FAINT));
            });
            section(ui, t("介面大小"));
            ui.horizontal(|ui| {
                let label = |p: i64| if p == 0 { format!("{}　—　{auto}%", t("自動（依螢幕大小）")) } else { format!("{p}%") }; // i18n-ignore: not language text
                egui::ComboBox::from_id_salt("set_scale").width(260.0).selected_text(label(d.s.ui_scale_percent)).show_ui(ui, |ui| {
                    for p in std::iter::once(0).chain(SCALE_PERCENTS) {
                        ui.selectable_value(&mut d.s.ui_scale_percent, p, label(p));
                    }
                });
                ui.label(RichText::new(t("變更後立即套用")).size(theme::fs(FontKind::Small)).color(theme::TEXT_FAINT));
            });
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(t("字體大小…")).min_size(Vec2::new(130.0, 28.0))).clicked() {
                    d.fonts = Some(d.s.font_sizes);
                }
                let fs = d.s.font_sizes;
                let summary = if fs == FontSizes::default() { t("預設比例").to_string() } else { f("已自訂：一般 {0}px、區塊標題 {1}px、小字 {2}px", &[&fs.normal, &fs.section_title, &fs.small]) };
                ui.label(RichText::new(summary).size(theme::fs(FontKind::Small)).color(theme::TEXT_FAINT));
            });
            section(ui, t("一般選項"));
            ui.checkbox(&mut d.s.use_libraw, t("使用 LibRaw"));
            ui.horizontal(|ui| {
                ui.label(t("RAW 處理精度"));
                let names = [t("8-bit（省空間）"), t("16-bit（高精度）")];
                egui::ComboBox::from_id_salt("set_precision").width(190.0).selected_text(names[d.s.high_precision as usize]).show_ui(ui, |ui| {
                    ui.selectable_value(&mut d.s.high_precision, false, names[0]);
                    ui.selectable_value(&mut d.s.high_precision, true, names[1]);
                });
                if ui.link(t("說明")).clicked() {
                    d.help = true;
                }
            });
            ui.checkbox(&mut d.s.show_thumbnail_number, t("在縮圖左上顯示編號 (#1, #2 …)"));
            ui.checkbox(&mut d.s.show_column_scroll_bars, t("顯示捲軸（視窗過矮時左右欄可捲動）"));
            ui.checkbox(&mut d.s.use_gpu, t("使用 GPU 加速算圖（偵測不到或失敗時自動改用 CPU）"));
            ui.checkbox(&mut d.s.xmp_support, t("支援 XMP"));
            ui.label(RichText::new(t("開啟後縮圖右鍵選單與選單會出現匯出／匯入 XMP")).size(theme::fs(FontKind::Small)).color(theme::TEXT_FAINT));
            ui.add_space(4.0);
            let small = |ui: &mut egui::Ui, s: String| ui.label(RichText::new(s).size(theme::fs(FontKind::Small)).color(theme::TEXT_FAINT));
            let lib = if awpr_core::libraw::available() { f("LibRaw {0} 已載入", &[&libraw_version()]) } else { t("LibRaw 無法使用（將使用相機內嵌預覽）").to_string() };
            small(ui, lib);
            small(ui, gpu_line.clone());
            ui.add_space(8.0);
            ui.separator();
            ui.horizontal(|ui| {
                // Back to the defaults in the window only (套用 saves); the language is
                // the user's identity, not a preference, so it stays.
                if ui.add(egui::Button::new(t("恢復預設")).min_size(Vec2::new(210.0, 30.0))).clicked() {
                    let keep = d.s.language;
                    let recent = std::mem::take(&mut d.s.recent_folders);
                    let last = std::mem::take(&mut d.s.last_folder);
                    let hidden = d.s.show_hidden;
                    d.s = Settings { language: keep, recent_folders: recent, last_folder: last, show_hidden: hidden, ..Default::default() };
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new(t("取消")).min_size(Vec2::new(96.0, 30.0))).clicked() {
                        done = Some(false);
                    }
                    let ok = egui::Button::new(RichText::new(t("套用")).color(Color32::WHITE)).fill(theme::ACCENT).min_size(Vec2::new(88.0, 30.0));
                    if ui.add(ok).clicked() {
                        done = Some(true);
                    }
                });
            });
        });
        if d.help {
            let mut close = false;
            egui::Window::new(t("RAW 處理精度")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::new(0.0, 30.0)).show(ctx, |ui| {
                ui.set_max_width(460.0);
                ui.label(t(RAW_PRECISION_HELP));
                if ui.button(t("確定")).clicked() {
                    close = true;
                }
            });
            if close {
                d.help = false;
            }
        }
        if let Some(mut draft) = d.fonts.take() {
            match font_sizes_ui(ctx, &mut draft) {
                Some(true) => d.s.font_sizes = draft,
                Some(false) => {}
                None => d.fonts = Some(draft),
            }
        }
        match done {
            Some(true) => self.apply_settings(ctx, d.s),
            Some(false) => {}
            None => self.settings_dlg = Some(d),
        }
    }

    // ---- first run --------------------------------------------------------------------

    /// The language choice on the very first start (no settings anywhere yet). Closing it
    /// accepts the selection, so it is never asked twice.
    pub(super) fn first_run_ui(&mut self, ctx: &egui::Context) {
        let Some(mut pick) = self.first_run else { return };
        let mut ok = false;
        let mut open = true;
        egui::Window::new("AwayPhotoRawEditor").open(&mut open).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            ui.set_width(480.0);
            ui.label(RichText::new("選擇語言　/　Select Language").strong().size(theme::scaled(20.0))); // i18n-ignore: shown before a language exists
            ui.label(RichText::new("請選擇介面語言，稍後可在「設定」中變更。").color(theme::TEXT_DIM)); // i18n-ignore
            ui.label(RichText::new("Choose your interface language. You can change it later in Settings.").color(theme::TEXT_DIM));
            ui.add_space(8.0);
            egui::Grid::new("first_run_langs").num_columns(2).spacing([10.0, 10.0]).show(ui, |ui| {
                for (i, l) in Lang::ALL.into_iter().enumerate() {
                    let sel = pick == l;
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(232.0, 52.0), egui::Sense::click());
                    let p = ui.painter();
                    p.rect_filled(rect, 6.0, if sel { Color32::from_rgb(0x2F, 0x3E, 0x52) } else if resp.hovered() { theme::BUTTON_HOVER } else { theme::BUTTON });
                    p.rect_stroke(rect, 6.0, egui::Stroke::new(if sel { 2.0 } else { 1.0 }, if sel { theme::ACCENT } else { theme::BORDER }), egui::StrokeKind::Inside);
                    p.text(rect.min + Vec2::new(14.0, 8.0), egui::Align2::LEFT_TOP, l.display_name(), egui::FontId::proportional(theme::scaled(16.0)), theme::TEXT);
                    p.text(rect.min + Vec2::new(14.0, 31.0), egui::Align2::LEFT_TOP, l.english_name(), egui::FontId::proportional(theme::scaled(12.0)), theme::TEXT_DIM);
                    if resp.clicked() {
                        pick = l;
                    }
                    if resp.double_clicked() {
                        ok = true;
                    }
                    if i % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let b = egui::Button::new(RichText::new("確定　/　OK").color(Color32::WHITE)).fill(theme::ACCENT).min_size(Vec2::new(140.0, 32.0)); // i18n-ignore
                if ui.add(b).clicked() {
                    ok = true;
                }
            });
        });
        self.first_run = Some(pick);
        if ok || !open || ctx.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Escape)) {
            self.first_run = None;
            let mut s = self.settings.clone();
            s.language = pick;
            self.settings.language = Lang::ZhTw; // so apply_settings sees the change
            i18n::set_lang(Lang::ZhTw);
            self.apply_settings(ctx, s);
        }
    }

    // ---- 關於 ----------------------------------------------------------------------------

    pub(super) fn about_ui(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.about.take() else { return };
        if let UpdateState::Checking(rx) = &state {
            if let Ok(r) = rx.try_recv() {
                state = UpdateState::Done(r);
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
        let mut close = false;
        let body = theme::fs(FontKind::AboutBody);
        egui::Window::new(t("關於")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            ui.set_width(540.0);
            let text = |ui: &mut egui::Ui, s: &str| ui.label(RichText::new(s).size(body));
            ui.label(RichText::new("AwayPhotoRawEditor").strong().size(theme::fs(FontKind::AboutTitle)));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                text(ui, &format!("{}{}", t("版本："), env!("CARGO_PKG_VERSION")));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let checking = matches!(state, UpdateState::Checking(_));
                    let label = if checking { t("檢查中…") } else { t("檢查更新") };
                    if ui.add_enabled(!checking, egui::Button::new(label)).clicked() {
                        let (tx, rx) = std::sync::mpsc::channel();
                        let c = ctx.clone();
                        std::thread::spawn(move || {
                            let _ = tx.send(update::fetch(env!("CARGO_PKG_VERSION")));
                            c.request_repaint();
                        });
                        state = UpdateState::Checking(rx);
                    }
                });
            });
            if let UpdateState::Done(r) = &state {
                egui::Frame::new().fill(theme::VIEWER).corner_radius(4.0).inner_margin(8).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    match r {
                        None => {
                            ui.label(t("無法連線到更新伺服器，請稍後再試。"));
                        }
                        Some(i) if !i.update_available => {
                            ui.label(f("目前已是最新版本（{0}）。", &[&env!("CARGO_PKG_VERSION")]));
                        }
                        Some(i) => {
                            ui.label(f("有新版本可以下載。\n\n目前版本：{0}\n最新版本：v{1}", &[&env!("CARGO_PKG_VERSION"), &i.latest_version]));
                            if !i.notes.is_empty() {
                                ui.add_space(4.0);
                                egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| ui.label(RichText::new(&i.notes).color(theme::TEXT_DIM)));
                            }
                            if ui.button(t("開啟下載頁")).clicked() {
                                ctx.open_url(egui::OpenUrl::new_tab(&i.page_url));
                            }
                        }
                    }
                });
            }
            ui.add_space(6.0);
            text(ui, &format!("{}{}", t("編譯時間："), env!("AWPR_BUILD_TIME")));
            ui.add_space(6.0);
            text(ui, &format!("{} Chih-Wei Su (Awaysu)　awaysu@gmail.com", t("作者:"))); // i18n-ignore: not language text
            ui.add_space(6.0);
            text(ui, t("下載:"));
            ui.hyperlink_to(RichText::new(update::PAGE_URL).size(body), update::PAGE_URL);
            text(ui, "Source Code:");
            ui.hyperlink_to(RichText::new("https://github.com/awaysu/AwayPhotoRawEditor").size(body), "https://github.com/awaysu/AwayPhotoRawEditor");
            ui.add_space(6.0);
            text(ui, t("第三方元件:"));
            for line in [
                format!("LibRaw {} (CDDL-1.0)", libraw_version()),
                "egui / eframe 0.36 · wgpu 30 (MIT / Apache-2.0)".into(),
                "image 0.25 · tiff 0.11 · ab_glyph 0.2 (MIT / Apache-2.0)".into(),
                "ureq 3 · serde_json 1 · quick-xml · rfd · rayon · chrono · trash (MIT / Apache-2.0)".into(),
                "Ubuntu Font (Ubuntu Font Licence 1.0)".into(),
            ] {
                ui.label(RichText::new(format!("  {line}")).size(theme::fs(FontKind::Small)).color(theme::TEXT_DIM));
            }
            ui.label(RichText::new(t("完整清單見 THIRD-PARTY-NOTICES.md")).size(theme::fs(FontKind::Small)).color(theme::TEXT_FAINT));
            ui.add_space(6.0);
            text(ui, &format!("{}BSD 3-Clause　© 2026 Chih-Wei Su (Awaysu)", t("授權："))); // i18n-ignore: not language text
            ui.add_space(6.0);
            ui.label(RichText::new(t("歡迎自由修改成你自己的版本，只希望你能在你的「關於」視窗中提及來源是這裡（AwayPhotoRawEditor / Awaysu）。")).size(body).color(theme::TEXT_DIM));
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let ok = egui::Button::new(RichText::new(t("確定")).color(Color32::WHITE)).fill(theme::ACCENT).min_size(Vec2::new(88.0, 30.0));
                if ui.add(ok).clicked() {
                    close = true;
                }
            });
        });
        if !close {
            self.about = Some(state);
        }
    }

    // ---- 支援 RAW 檔相機列表 ---------------------------------------------------------------

    pub(super) fn cameras_ui(&mut self, ctx: &egui::Context) {
        let Some((mut filter, list)) = self.cameras.take() else { return };
        let mut open = true;
        egui::Window::new(t("支援RAW檔相機列表")).open(&mut open).collapsible(false).resizable(true).default_size(Vec2::new(460.0, 560.0)).pivot(egui::Align2::CENTER_CENTER).default_pos(ctx.content_rect().center()).show(ctx, |ui| {
            ui.label(RichText::new(f("LibRaw {0} 支援 {1} 種相機", &[&libraw_version(), &list.len()])).color(theme::TEXT_DIM));
            ui.horizontal(|ui| {
                ui.label(t("搜尋"));
                ui.add(egui::TextEdit::singleline(&mut filter).desired_width(ui.available_width()));
            });
            ui.separator();
            let needle = filter.to_lowercase();
            let shown: Vec<&String> = list.iter().filter(|c| needle.is_empty() || c.to_lowercase().contains(&needle)).collect();
            let row = ui.text_style_height(&egui::TextStyle::Body);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, row, shown.len(), |ui, range| {
                for c in &shown[range] {
                    ui.label(c.as_str());
                }
            });
        });
        if open {
            self.cameras = Some((filter, list));
        }
    }
}

/// LibRaw's version without the "-Release" suffix.
fn libraw_version() -> String {
    awpr_core::libraw::version().trim_end_matches("-Release").to_string()
}

/// The RAW 處理精度 help (the C# `L.RawPrecisionHelp`; the full Chinese text is the key).
const RAW_PRECISION_HELP: &str = "8-bit：RAW 解碼成每色 256 階再處理，預覽快取每張約 3–5 MB。一般調整足夠。\n\n16-bit：RAW 解碼成每色 65536 階（以浮點處理），預覽快取每張約 35 MB、第一次開資料夾較慢。大幅拉曝光或暗部時不會出現色階斷層（banding）。\n\n兩者的運算與輸出都一樣（float 運算、8-bit 輸出），差別只在 RAW 解碼保留多少資訊；對 JPG 等非 RAW 沒有影響。\n\n變更後請用「關閉資料夾並刪除快取縮圖」重新產生預覽快取，舊快取不會自動更新。";

/// 字體大小: each of the twelve sizes with what it is used for, its pixel value and a live
/// preview. Some(true) = 確定, Some(false) = 取消, None = still open.
fn font_sizes_ui(ctx: &egui::Context, draft: &mut FontSizes) -> Option<bool> {
    let rows: [(FontKind, &'static str, &'static str); 12] = [
        (FontKind::Small, "小字", "縮圖檔名、EXIF 欄位名、提示文字"),
        (FontKind::Mono, "等寬數值", "直方圖下方 RGB 平均值"),
        (FontKind::Normal, "一般（預設）", "滑桿標籤與數值、下拉、輸入框、按鈕"),
        (FontKind::SectionTitle, "區塊標題", "基本調整／色彩／工具 等區塊標題"),
        (FontKind::AboutBody, "關於內文", "關於視窗的內文"),
        (FontKind::FolderGlyph, "資料夾圖示", "選擇資料夾清單的圖示"),
        (FontKind::IconGlyph, "小圖示按鈕", "白平衡滴管等圖示鈕"),
        (FontKind::ProgressTitle, "進度視窗標題", "產生快取／轉存進度視窗"),
        (FontKind::DialogTitle, "對話框標題", "匯出照片／設定 視窗標題"),
        (FontKind::AboutTitle, "關於標題", "關於視窗的標題"),
        (FontKind::Logo, "左上程式名稱", "頂端「AwayPhotoRawEditor」"),
        (FontKind::MenuGlyph, "選單圖示", "左上角的 ☰ 選單鈕"),
    ];
    let mut result = None;
    egui::Window::new(t("字體大小")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::new(0.0, 20.0)).show(ctx, |ui| {
        ui.set_width(640.0);
        ui.label(RichText::new(t("數值為 100% 顯示比例下的像素；介面大小會再等比縮放。調太大時部分固定寬度的標籤可能被截字")).size(theme::fs(FontKind::Small)).color(theme::TEXT_FAINT));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(392.0);
            ui.add_sized([60.0, 18.0], egui::Label::new(RichText::new(t("像素")).strong().color(theme::ACCENT)));
            ui.label(RichText::new(t("預覽")).strong().color(theme::ACCENT));
        });
        egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
            for (k, name, detail) in rows {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(380.0);
                        ui.label(t(name));
                        ui.label(RichText::new(t(detail)).size(theme::fs(FontKind::Small)).color(theme::TEXT_FAINT));
                    });
                    let mut v = draft.get(k);
                    if ui.add_sized([60.0, 24.0], egui::DragValue::new(&mut v).range(FontSizes::MIN_PX..=FontSizes::MAX_PX)).changed() {
                        draft.set(k, v);
                    }
                    // The size this setting gives, against the defaults' look.
                    let pt = 14.0 * draft.get(k) as f32 / 15.0;
                    ui.label(RichText::new(t("樣本 Ag")).size(pt));
                });
                ui.add_space(2.0);
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new(t("恢復預設")).min_size(Vec2::new(110.0, 30.0))).clicked() {
                *draft = FontSizes::default();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new(t("取消")).min_size(Vec2::new(92.0, 30.0))).clicked() {
                    result = Some(false);
                }
                let ok = egui::Button::new(RichText::new(t("確定")).color(Color32::WHITE)).fill(theme::ACCENT).min_size(Vec2::new(88.0, 30.0));
                if ui.add(ok).clicked() {
                    result = Some(true);
                }
            });
        });
    });
    result
}

/// Recent folders shown first in 紀錄 when opening one (kept on the settings).
pub(super) fn remember_folder(s: &mut Settings, folder: &str) {
    if !paths::file_name(folder).is_empty() {
        s.push_recent_folder(folder);
    }
}
