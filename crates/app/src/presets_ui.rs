//! 編輯風格檔 window (the C# `PresetEditorForm`): every preset on the left, the 基本 /
//! 色彩 / 細節 sliders for the selected one on the right. Edits are committed when the
//! selection changes and when the window closes; a built-in edited back to its default
//! loses its override.

use crate::i18n::{f, t, tr};
use crate::theme;
use crate::widgets::{self, Gradient, SliderSpec};
use awpr_core::{color, ImageAdjustments};
use awpr_photo::presets::{self, PresetCollection};
use eframe::egui::{self, Color32, RichText, Vec2};

enum Ask {
    /// 還原全部 from this file.
    Restore(std::path::PathBuf),
    /// 恢復預設.
    Reset,
}

pub struct PresetEditor {
    cur: Option<String>,
    adj: ImageAdjustments,
    baseline: ImageAdjustments,
    new_name: String,
    message: Option<String>,
    ask: Option<Ask>,
}

impl PresetEditor {
    pub fn new(presets: &PresetCollection) -> Self {
        let mut e = Self { cur: None, adj: ImageAdjustments::default(), baseline: ImageAdjustments::default(), new_name: String::new(), message: None, ask: None };
        let first = Self::names(presets).into_iter().next();
        if let Some(n) = first {
            e.select(presets, &n);
        }
        e
    }

    /// The editable presets: the built-ins except 預設時設定, then the custom ones.
    fn names(presets: &PresetCollection) -> Vec<String> {
        presets::BUILT_IN_NAMES.iter().filter(|n| **n != presets::DEFAULT_NAME).map(|s| s.to_string()).chain(presets.custom_names()).collect()
    }

    fn select(&mut self, presets: &PresetCollection, name: &str) {
        self.cur = Some(name.to_string());
        self.adj = presets.effective(name);
        self.baseline = self.adj.clone();
    }

    /// Store the edited preset (only when it changed). True when the collection changed.
    fn commit(&mut self, presets: &mut PresetCollection) -> bool {
        let Some(name) = &self.cur else { return false };
        if awpr_photo::store::value_equals(&self.adj, &self.baseline) {
            return false;
        }
        presets.commit(name, &self.adj);
        self.baseline = self.adj.clone();
        true
    }

    /// Draw the window; false once it is closed. `save` persists the collection.
    pub fn show(&mut self, ctx: &egui::Context, presets: &mut PresetCollection, save: &mut dyn FnMut(&PresetCollection)) -> bool {
        let mut open = true;
        let mut close = false;
        let mut changed = false;
        egui::Window::new(t("編輯風格檔")).open(&mut open).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                // ---- left: the list and the buttons ----
                ui.vertical(|ui| {
                    ui.set_width(240.0);
                    egui::Frame::new().fill(theme::VIEWER).stroke(egui::Stroke::new(1.0, theme::BORDER)).show(ui, |ui| {
                        ui.set_min_height(380.0);
                        ui.set_width(240.0);
                        egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                            for n in Self::names(presets) {
                                let text = if presets::is_builtin(&n) { tr(&n) } else { format!("{n}{}", t("（自訂）")) };
                                let sel = self.cur.as_deref() == Some(n.as_str());
                                if ui.add(egui::Button::selectable(sel, text).min_size(Vec2::new(232.0, 28.0))).clicked() && !sel {
                                    changed |= self.commit(presets);
                                    self.select(presets, &n);
                                }
                            }
                        });
                    });
                    ui.add_space(6.0);
                    ui.label(RichText::new(t("新增自訂風格檔")).color(theme::TEXT_DIM));
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.new_name).desired_width(150.0));
                        if widgets::fixed_button(ui, true, Vec2::new(78.0, 26.0), t("新增"), |b| b).clicked() {
                            let name = self.new_name.trim().to_string();
                            if name.is_empty() {
                                self.message = Some(t("請先輸入自訂風格檔名稱").into());
                            } else if presets::is_builtin(&name) || presets.custom_names().contains(&name) {
                                self.message = Some(f("已有名為「{0}」的風格檔，請換一個名稱", &[&name]));
                            } else {
                                self.commit(presets);
                                // From the values shown now.
                                presets.set(&name, &self.adj);
                                changed = true;
                                self.new_name.clear();
                                self.select(presets, &name);
                            }
                        }
                    });
                    ui.label(RichText::new(t("「新增」以目前顯示的設定建立\n修改會自動儲存")).size(theme::scaled(11.5)).color(theme::TEXT_FAINT));
                    let custom = self.cur.as_deref().is_some_and(|n| !presets::is_builtin(n));
                    if widgets::fixed_button(ui, custom, Vec2::new(240.0, 28.0), t("刪除這個自訂風格檔"), |b| b).clicked() {
                        if let Some(n) = self.cur.take() {
                            presets.remove(&n);
                            changed = true;
                            if let Some(first) = Self::names(presets).into_iter().next() {
                                self.select(presets, &first);
                            }
                        }
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if widgets::fixed_button(ui, true, Vec2::new(116.0, 30.0), t("備份全部"), |b| b).clicked() {
                            changed |= self.commit(presets);
                            if let Some(p) = rfd::FileDialog::new().set_title(t("風格檔備份")).add_filter(t("風格檔備份"), &["xml"]).set_file_name("AwayPhotoRawEditor_Presets.xml").save_file() {
                                self.message = Some(match presets.export_to(&p) {
                                    Ok(()) => f("已備份全部風格檔至：\n{0}", &[&p.display()]),
                                    Err(e) => format!("{}{e}", t("備份失敗：")),
                                });
                            }
                        }
                        if widgets::fixed_button(ui, true, Vec2::new(116.0, 30.0), t("還原全部"), |b| b).clicked() {
                            if let Some(p) = rfd::FileDialog::new().set_title(t("風格檔備份")).add_filter(t("風格檔備份"), &["xml"]).pick_file() {
                                self.ask = Some(Ask::Restore(p));
                            }
                        }
                    });
                    if widgets::fixed_button(ui, true, Vec2::new(240.0, 30.0), t("恢復預設"), |b| b).clicked() {
                        self.ask = Some(Ask::Reset);
                    }
                });
                ui.add_space(12.0);
                // ---- right: the three slider groups ----
                ui.vertical(|ui| {
                    ui.set_width(310.0);
                    let a = &mut self.adj;
                    theme::section(ui, t("基本調整"), |ui| {
                        let exposure = SliderSpec { min: -5.0, max: 5.0, decimals: 2, wheel_step: 0.05, ..SliderSpec::pm100(t("曝光")) };
                        slider(ui, exposure, &mut a.exposure);
                        slider(ui, SliderSpec::pm100(t("對比")), &mut a.contrast);
                        slider(ui, SliderSpec::pm100(t("亮部")), &mut a.highlights);
                        slider(ui, SliderSpec::pm100(t("暗部")), &mut a.shadows);
                        slider(ui, SliderSpec::pm100(t("白色")), &mut a.whites);
                        slider(ui, SliderSpec::pm100(t("黑色")), &mut a.blacks);
                    });
                    ui.add_space(4.0);
                    theme::section(ui, t("色彩"), |ui| {
                        let temp = SliderSpec { min: color::MIN_KELVIN, max: color::MAX_KELVIN, default: 5200.0, bipolar: false, wheel_step: 50.0, ..SliderSpec::pm100(t("色溫")) }.gradient(Gradient::Temperature);
                        slider(ui, temp, &mut a.temperature);
                        slider(ui, SliderSpec::pm100(t("色調")).gradient(Gradient::Tint), &mut a.tint);
                        slider(ui, SliderSpec::pm100(t("鮮豔度")).gradient(Gradient::Saturation), &mut a.vibrance);
                        slider(ui, SliderSpec::pm100(t("飽和度")).gradient(Gradient::Saturation), &mut a.saturation);
                        ui.label(RichText::new(t("套用風格檔時維持照片目前的色溫／色調")).size(theme::scaled(11.5)).color(theme::TEXT_FAINT));
                    });
                    ui.add_space(4.0);
                    theme::section(ui, t("細節"), |ui| {
                        slider(ui, SliderSpec::pm100(t("銳利度")), &mut a.sharpening);
                        slider(ui, SliderSpec::pm100(t("暗角")), &mut a.vignette);
                        slider(ui, SliderSpec { min: 0.0, bipolar: false, ..SliderSpec::pm100(t("降噪")) }, &mut a.noise_reduction);
                    });
                    ui.add_space(8.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        let b = egui::Button::new(RichText::new(t("關閉")).color(Color32::WHITE)).fill(theme::ACCENT).min_size(Vec2::new(80.0, 30.0));
                        if ui.add(b).clicked() {
                            close = true;
                        }
                    });
                });
            });
        });
        self.ask_ui(ctx, presets, &mut changed);
        if let Some(m) = self.message.clone() {
            egui::Window::new(t("編輯風格檔")).id(egui::Id::new("preset_editor_message")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::new(0.0, 40.0)).show(ctx, |ui| {
                ui.label(m);
                if ui.button(t("確定")).clicked() {
                    self.message = None;
                }
            });
        }
        let closing = close || !open;
        if closing {
            changed |= self.commit(presets);
        }
        if changed {
            save(presets);
        }
        !closing
    }

    fn ask_ui(&mut self, ctx: &egui::Context, presets: &mut PresetCollection, changed: &mut bool) {
        let Some(ask) = &self.ask else { return };
        let (title, text) = match ask {
            Ask::Restore(_) => (t("風格檔備份"), t("還原將以備份內容取代現有的所有風格檔設定。\n確定要還原？")),
            Ask::Reset => (t("恢復預設"), t("將刪除所有自訂風格檔，並把所有內建風格檔恢復為預設值。\n確定要恢復預設？")),
        };
        let mut answer = None;
        egui::Window::new(title).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::new(0.0, 40.0)).show(ctx, |ui| {
            ui.label(text);
            ui.horizontal(|ui| {
                if ui.button(t("確定")).clicked() {
                    answer = Some(true);
                }
                if ui.button(t("取消")).clicked() {
                    answer = Some(false);
                }
            });
        });
        match answer {
            Some(true) => {
                match self.ask.take() {
                    Some(Ask::Restore(p)) => match PresetCollection::import_from(&p) {
                        Ok(Some(c)) => {
                            *presets = c;
                            self.message = Some(t("已從備份還原風格檔").into());
                        }
                        Ok(None) => self.message = Some(t("這不是有效的風格檔備份檔").into()),
                        Err(e) => self.message = Some(format!("{}{e}", t("還原失敗："))),
                    },
                    Some(Ask::Reset) => *presets = PresetCollection::default(),
                    None => {}
                }
                // Pending edits are dropped on purpose: the restored / default values win.
                *changed = true;
                if let Some(first) = Self::names(presets).into_iter().next() {
                    self.select(presets, &first);
                }
            }
            Some(false) => self.ask = None,
            None => {}
        }
    }
}

fn slider(ui: &mut egui::Ui, spec: SliderSpec, v: &mut f64) {
    let _ = widgets::adjust_slider(ui, &spec, v, true);
}
