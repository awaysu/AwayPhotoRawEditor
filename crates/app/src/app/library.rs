//! Photo management on the strip (the C# `ThumbnailStrip` and the `MainForm` handlers
//! behind it): multi-selection and the batch edit session, the thumbnail menu, hiding,
//! virtual copies, deleting, copy / paste settings, 升級處理版本 and the preset panel.

use crate::i18n::{f, t, tr};
use super::App;
use crate::theme;
use crate::worker::Item;
use awpr_core::ImageAdjustments;
use awpr_photo::{edits, library, presets, store, xmp};
use eframe::egui::{self, Color32, RichText, Vec2};

/// One undo step: the current photo's prior state, plus — for a step that opened a batch
/// edit — every other affected photo's prior state, so the whole step reverts together.
pub(super) struct UndoStep {
    pub current: ImageAdjustments,
    pub others: Option<Vec<(Item, ImageAdjustments)>>,
}

/// How many undo steps are kept (C#: 80).
const UNDO_LIMIT: usize = 80;

/// A question waiting for 確定 / 取消.
pub(super) enum Confirm {
    DeleteFile(Item),
    Upgrade(Vec<Item>),
    /// 關閉資料夾並刪除快取縮圖.
    ClearCache,
}

/// What a thumbnail-menu click asks for (run after the menu is drawn).
enum MenuAction {
    SelectAll,
    Invert,
    DeselectAll,
    ApplyPreset(String),
    Copy(usize),
    Paste,
    Upgrade,
    ExportXmp,
    ImportXmp,
    VirtualCopy(usize),
    Hide,
    Unhide,
    Delete(usize),
    ShowHidden(bool),
    Export,
}

impl App {
    // ---- selection --------------------------------------------------------------------

    /// The selected photos in strip order (the current one when nothing else is).
    pub(super) fn selected_items(&self) -> Vec<Item> {
        let v: Vec<Item> = self.selected.iter().filter_map(|&i| self.items.get(i).cloned()).collect();
        if v.is_empty() {
            self.current.and_then(|i| self.items.get(i).cloned()).into_iter().collect()
        } else {
            v
        }
    }

    /// The selected photos other than the current one (empty unless a multi-selection).
    fn selected_others(&self) -> Vec<Item> {
        if self.selected.len() <= 1 {
            return Vec::new();
        }
        self.selected.iter().filter(|&&i| Some(i) != self.current).filter_map(|&i| self.items.get(i).cloned()).collect()
    }

    /// A left click on a thumbnail: Ctrl toggles, Shift selects the range from the anchor.
    /// The clicked photo becomes the one being edited.
    pub(super) fn click_thumb(&mut self, i: usize, ctrl: bool, shift: bool) {
        if shift && self.anchor.is_some() {
            let a = self.anchor.unwrap_or(i);
            self.selected = (a.min(i)..=a.max(i)).collect();
        } else if ctrl {
            if !self.selected.insert(i) {
                self.selected.remove(&i);
            }
            self.anchor = Some(i);
        } else {
            self.selected = [i].into();
            self.anchor = Some(i);
        }
        self.select(i);
    }

    pub(super) fn select_all(&mut self) {
        self.selected = (0..self.items.len()).collect();
    }

    fn invert_selection(&mut self) {
        self.selected = (0..self.items.len()).filter(|i| !self.selected.contains(i)).collect();
    }

    fn deselect_all(&mut self) {
        // The photo being edited stays selected.
        self.selected = self.current.into_iter().collect();
    }

    // ---- the batch edit session (C# PushUndo / FlushBatchSync) ----------------------

    /// An edit gesture starts: snapshot for undo; with several photos selected this opens
    /// the batch session (the others get the changed fields when the photo is committed).
    pub(super) fn edit_begin(&mut self) {
        self.redo.clear();
        self.show_original = false;
        let mut step = UndoStep { current: self.adj.clone(), others: None };
        if self.sync.is_none() {
            let others = self.selected_others();
            if !others.is_empty() {
                step.others = Some(others.iter().map(|it| (it.clone(), store::load_all(&it.path, it.copy).0.unwrap_or_default())).collect());
                self.sync = Some((others, self.adj.clone()));
            }
        }
        self.undo.push(step);
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
    }

    /// Write the pending batch edit: the fields changed on the current photo (against the
    /// session's baseline) go onto every other selected photo; the session ends.
    pub(super) fn flush_batch_sync(&mut self) {
        let Some((targets, baseline)) = self.sync.take() else { return };
        for it in targets {
            let mut a = store::load_all(&it.path, it.copy).0.unwrap_or_default();
            edits::apply_delta(&mut a, &self.adj, &baseline);
            self.write_other(&it, &a);
        }
    }

    /// Save another photo's adjustments and refresh its badge and thumbnail.
    fn write_other(&mut self, it: &Item, a: &ImageAdjustments) {
        if !self.headless() {
            if let Err(e) = store::save(&it.path, a, it.copy, None) {
                self.status = f("無法儲存 {0}：{1}", &[&it.name(), &e]);
                return;
            }
        }
        if let Some(i) = self.items.iter().position(|x| x.key == it.key) {
            self.items[i].edited = !store::is_default(a);
            self.thumb_version += 1;
            let item = self.items[i].clone();
            self.worker.thumbnail(&item, Some(a.clone()), self.thumb_version);
        }
    }

    pub(super) fn do_undo(&mut self) {
        let Some(step) = self.undo.pop() else { return };
        self.redo.push(UndoStep { current: self.adj.clone(), others: None });
        self.adj = step.current;
        // The step that opened a batch edit puts every other photo back too, and drops the
        // pending sync so it cannot re-apply afterwards.
        if let Some(others) = step.others {
            for (it, prior) in others {
                self.write_other(&it, &prior);
            }
            self.sync = None;
        }
        self.edited();
    }

    /// Redo is for the current photo only: a batch sync is not replayed.
    pub(super) fn do_redo(&mut self) {
        let Some(step) = self.redo.pop() else { return };
        self.undo.push(UndoStep { current: self.adj.clone(), others: None });
        self.adj = step.current;
        self.edited();
    }

    /// A reset on the current photo and, right away, on every other selected photo.
    pub(super) fn reset_selected(&mut self, reset: fn(&mut ImageAdjustments)) {
        self.edit_begin();
        reset(&mut self.adj);
        for it in self.selected_others() {
            let mut a = store::load_all(&it.path, it.copy).0.unwrap_or_default();
            reset(&mut a);
            self.write_other(&it, &a);
        }
        // Supersedes any pending slider sync.
        self.sync = None;
        self.edited();
    }

    // ---- presets ----------------------------------------------------------------------

    /// Apply a preset to every selected photo (the white balance is never touched).
    pub(super) fn apply_preset_to_selection(&mut self, name: &str) {
        if !self.has_photo() {
            return;
        }
        self.edit_begin();
        self.presets.apply(name, &mut self.adj);
        for it in self.selected_others() {
            let mut a = store::load_all(&it.path, it.copy).0.unwrap_or_default();
            self.presets.apply(name, &mut a);
            self.write_other(&it, &a);
        }
        self.sync = None;
        self.edited();
        self.status = f("已套用風格檔：{0}", &[&tr(name)]);
    }

    /// 風格檔種類: the list, 套用該風格檔 and 編輯風格檔….
    pub(super) fn preset_panel(&mut self, ui: &mut egui::Ui) {
        let on = self.has_photo();
        theme::section(ui, t("風格檔種類"), |ui| {
            let names: Vec<String> = presets::BUILT_IN_NAMES.iter().map(|s| s.to_string()).chain(self.presets.custom_names()).collect();
            if !names.contains(&self.preset_choice) {
                self.preset_choice = presets::DEFAULT_NAME.to_string();
            }
            let label = |n: &str| if presets::is_builtin(n) { tr(n) } else { format!("{n}{}", t("（自訂）")) };
            egui::ComboBox::from_id_salt("preset_choice").width(ui.available_width()).selected_text(label(&self.preset_choice)).show_ui(ui, |ui| {
                for n in &names {
                    ui.selectable_value(&mut self.preset_choice, n.clone(), label(n));
                }
            });
            ui.horizontal(|ui| {
                let w = ui.available_width() - 110.0;
                if crate::widgets::fixed_button(ui, on, Vec2::new(w, 28.0), RichText::new(t("套用該風格檔")).color(Color32::WHITE), |b| b.fill(theme::ACCENT)).clicked() {
                    let n = self.preset_choice.clone();
                    self.apply_preset_to_selection(&n);
                }
                if crate::widgets::fixed_button(ui, true, Vec2::new(104.0, 28.0), t("編輯風格檔…"), |b| b).clicked() {
                    self.preset_editor = Some(crate::presets_ui::PresetEditor::new(&self.presets));
                }
            });
        });
    }

    // ---- copy / paste -----------------------------------------------------------------

    /// 複製照片設定 (one photo): everything, geometry and local edits included.
    fn copy_settings(&mut self, i: usize) {
        let it = &self.items[i];
        let a = if Some(i) == self.current { Some(self.adj.clone()) } else { store::load_all(&it.path, it.copy).0 };
        self.copied = a;
        self.copy_source = Some(it.key.clone());
        self.status = t("已複製相片設定").into();
    }

    /// 貼上照片設定 onto every selected photo.
    fn paste_settings(&mut self) {
        let Some(src) = self.copied.clone() else {
            self.status = t("尚未複製任何設定").into();
            return;
        };
        for it in self.selected_items() {
            if Some(it.key.as_str()) == self.current.map(|i| self.items[i].key.as_str()) {
                self.edit_begin();
                self.adj = src.clone();
                self.edited();
            } else {
                self.write_other(&it, &src);
            }
        }
        self.sync = None;
        self.status = t("已貼上相片設定").into();
    }

    // ---- virtual copies, hiding, deleting ---------------------------------------------

    fn save_preview_list(&mut self) {
        let copies: Vec<(String, i32)> = self.items.iter().filter(|i| i.copy > 0).map(|i| (i.path.clone(), i.copy)).collect();
        library::rewrite_virtual_copies(&mut self.preview_list, &copies);
        if !self.headless() {
            if let Err(e) = self.preview_list.save(&self.folder) {
                self.status = f("無法儲存 {0}：{1}", &[&"preview_list.xml", &e]);
            }
        }
    }

    /// 建立副本: a virtual copy right after the photo, starting from its adjustments.
    fn create_virtual_copy(&mut self, i: usize) {
        self.save_current_if_dirty();
        let src = self.items[i].clone();
        let next = library::next_copy_index(self.items.iter().map(|x| (x.path.as_str(), x.copy)).chain(self.preview_list.virtual_copies.iter().map(|v| (v.path.as_str(), v.index))), &src.path);
        let (a, exif, _) = store::load_all(&src.path, src.copy);
        let a = a.unwrap_or_default();
        if !self.headless() {
            if let Err(e) = store::save(&src.path, &a, next, exif.as_ref()) {
                self.status = f("無法建立副本：{0}", &[&e]);
                return;
            }
        }
        let copy = Item { key: store::make_key(&src.path, next), path: src.path.clone(), copy: next, number: 0, hidden: false, edited: !store::is_default(&a) };
        self.items.insert(i + 1, copy);
        self.save_preview_list();
        self.refresh_items_keep_selection();
        self.status = t("已建立虛擬副本").into();
    }

    /// 隱藏且不輸出 for the selection (also the Delete key).
    pub(super) fn hide_selected(&mut self) {
        let sel = self.selected_items();
        if sel.is_empty() {
            return;
        }
        for it in &sel {
            library::hide(&mut self.preview_list, &it.key);
        }
        self.save_preview_list();
        self.refresh_items_keep_selection();
        self.status = f("已隱藏 {0} 張（不輸出）", &[&sel.len()]);
    }

    fn unhide_selected(&mut self) {
        let sel = self.selected_items();
        for it in &sel {
            library::unhide(&mut self.preview_list, &it.key);
        }
        self.save_preview_list();
        self.refresh_items_keep_selection();
        self.status = t("已取消隱藏").into();
    }

    /// 不顯示隱藏 / 顯示全部.
    pub(super) fn set_show_hidden(&mut self, show: bool) {
        if self.settings.show_hidden == show {
            return;
        }
        self.settings.show_hidden = show;
        if !self.headless() {
            self.settings.save();
        }
        self.refresh_items_keep_selection();
    }

    /// 刪除檔案: a virtual copy is removed at once; a real photo asks first.
    pub(super) fn delete_item(&mut self, i: usize) {
        let it = self.items[i].clone();
        if it.copy > 0 {
            if Some(i) == self.current {
                self.saved_adj = self.adj.clone(); // nothing to save for a copy being removed
            }
            if !self.headless() {
                library::remove_virtual_copy(&mut self.preview_list, &it.path, it.copy);
            }
            self.items.remove(i);
            self.save_preview_list();
            self.refresh_items_keep_selection();
            self.status = t("已刪除虛擬副本").into();
        } else {
            self.confirm = Some(Confirm::DeleteFile(it));
        }
    }

    fn delete_file_confirmed(&mut self, it: Item) {
        if self.headless() {
            return;
        }
        if self.current.map(|c| self.items[c].path == it.path).unwrap_or(false) {
            // Its XML goes away with it: nothing to save.
            self.saved_adj = self.adj.clone();
            self.sync = None;
        }
        match library::delete_photo(&mut self.preview_list, &it.path) {
            Ok(()) => {
                self.items.retain(|x| x.path != it.path);
                self.save_preview_list();
                self.refresh_items_keep_selection();
                self.status = f("已刪除 {0}（已移到資源回收筒）", &[&it.name()]);
            }
            Err(e) => self.status = format!("{}{e}", t("刪除失敗：")),
        }
    }

    /// 升級處理版本: photos on an older version go to the current one; asks first.
    fn upgrade_selected(&mut self) {
        let targets = self.selected_items();
        if !targets.is_empty() {
            self.confirm = Some(Confirm::Upgrade(targets));
        }
    }

    /// 匯出 XMP: each selected photo's adjustments to its sidecar (`{stem}.xmp`).
    pub(super) fn export_xmp_selected(&mut self) {
        let current_key = self.current.map(|i| self.items[i].key.clone());
        let mut n = 0;
        for it in self.selected_items() {
            let (stored, exif, _) = store::load_all(&it.path, it.copy);
            let adj = if Some(&it.key) == current_key.as_ref() { Some(self.adj.clone()) } else { stored };
            let Some(adj) = adj else { continue };
            if self.headless() {
                n += 1;
                continue;
            }
            match xmp::export_sidecar(&it.path, it.copy, &adj, exif.as_ref()) {
                Ok(_) => n += 1,
                Err(e) => {
                    self.status = f("無法寫入 {0}：{1}", &[&xmp::sidecar_path(&it.path, it.copy), &e]);
                    return;
                }
            }
        }
        self.status = f("已匯出 {0} 個 XMP 檔", &[&n]);
    }

    /// 匯入 XMP: each selected photo takes its sidecar's settings (as one undo step for the
    /// photo being edited).
    pub(super) fn import_xmp_selected(&mut self) {
        let current_key = self.current.map(|i| self.items[i].key.clone());
        let (mut n, mut missing) = (0, 0);
        for it in self.selected_items() {
            if Some(&it.key) == current_key.as_ref() {
                let Some(a) = xmp::import_sidecar(&it.path, it.copy, &self.adj) else {
                    missing += 1;
                    continue;
                };
                self.edit_begin();
                self.adj = a;
                self.edited();
                if self.needs_source_reload() {
                    self.reload_source();
                }
                n += 1;
            } else {
                let base = store::load_all(&it.path, it.copy).0.unwrap_or_default();
                match xmp::import_sidecar(&it.path, it.copy, &base) {
                    Some(a) => {
                        self.write_other(&it, &a);
                        n += 1;
                    }
                    None => missing += 1,
                }
            }
        }
        self.status = if missing > 0 { f("已匯入 {0} 個 XMP 檔（{1} 張沒有可讀的 XMP）", &[&n, &missing]) } else { f("已匯入 {0} 個 XMP 檔", &[&n]) };
    }

    fn upgrade_confirmed(&mut self, targets: Vec<Item>) {
        let opt = self.settings.loader_options();
        let mut n = 0;
        let current_key = self.current.map(|i| self.items[i].key.clone());
        for it in targets {
            if Some(&it.key) == current_key.as_ref() {
                if self.adj.pipeline_version >= ImageAdjustments::CURRENT_PIPELINE_VERSION {
                    continue;
                }
                self.edit_begin();
                let exif = self.exif.clone();
                edits::upgrade(&mut self.adj, exif.as_ref());
                self.edited();
                n += 1;
                // 處理版本 3 edits a RAW from LibRaw's linear decode.
                if self.needs_source_reload() {
                    self.reload_source();
                }
            } else {
                let (a, exif, _) = store::load_all(&it.path, it.copy);
                let Some(mut a) = a else { continue };
                let mut exif = exif.unwrap_or_default();
                crate::worker::enrich_camera_color(&it.path, &mut exif, opt);
                if edits::upgrade(&mut a, Some(&exif)) {
                    if !self.headless() {
                        let _ = store::save(&it.path, &a, it.copy, Some(&exif));
                    }
                    self.write_other(&it, &a);
                    n += 1;
                }
            }
        }
        self.status = if n == 0 { t("選取的照片已是最新處理版本").into() } else { f("已升級 {0} 張照片的處理版本", &[&n]) };
    }

    /// Rebuild the strip (after hiding, copies, deletes, the show-hidden switch) keeping
    /// the selection and the photo being edited.
    pub(super) fn refresh_items_keep_selection(&mut self) {
        let sel_keys: Vec<String> = self.selected.iter().filter_map(|&i| self.items.get(i).map(|x| x.key.clone())).collect();
        let cur_key = self.current.and_then(|i| self.items.get(i).map(|x| x.key.clone()));
        let cur_pos = self.current.unwrap_or(0);
        self.rebuild_items();
        self.selected = sel_keys.iter().filter_map(|k| self.items.iter().position(|x| &x.key == k)).collect();
        for it in self.items.clone() {
            if !self.thumbs.contains_key(&it.key) {
                self.worker.thumbnail(&it, None, self.thumb_version);
            }
        }
        if self.items.is_empty() {
            self.clear_editor();
            return;
        }
        match cur_key.and_then(|k| self.items.iter().position(|x| x.key == k)) {
            // Same photo, new position: no reload.
            Some(i) => self.current = Some(i),
            None => {
                // The edited photo left the strip (hidden, deleted): edit its neighbour.
                self.save_current_if_dirty();
                self.current = None;
                let next = cur_pos.min(self.items.len() - 1);
                self.selected = [next].into();
                self.anchor = Some(next);
                self.select(next);
            }
        }
        if self.anchor.is_some_and(|a| a >= self.items.len()) {
            self.anchor = None;
        }
    }

    // ---- the strip --------------------------------------------------------------------

    pub(super) fn strip(&mut self, ui: &mut egui::Ui) {
        if self.items.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new(t("開啟一個相片資料夾開始編輯（Ctrl+O）")).color(theme::TEXT_FAINT));
            });
            return;
        }
        let mut clicked = None;
        let mut menu = None;
        let scroll_to = std::mem::take(&mut self.scroll_to_current);
        let (ctrl, shift) = ui.input(|i| (i.modifiers.command, i.modifiers.shift));
        // The wheel scrolls the strip without Shift too (it has only the one direction).
        ui.style_mut().always_scroll_the_only_direction = true;
        let mut area = egui::ScrollArea::horizontal().id_salt("strip_scroll").auto_shrink([false, false]);
        if let Some(x) = self.strip_offset.take().or(self.shot_strip_offset) {
            area = area.horizontal_scroll_offset(x);
        }
        let out = area.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                for (i, it) in self.items.iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(176.0, 136.0), egui::Sense::click());
                    if ui.is_rect_visible(rect) {
                        let selected = self.selected.contains(&i);
                        let current = self.current == Some(i);
                        let copy_source = self.copy_source.as_deref() == Some(it.key.as_str());
                        let thumb = self.thumbs.get(&it.key).map(|(t, _)| t);
                        paint_thumb(ui.painter(), rect, it, thumb, selected, current, copy_source, self.settings.show_thumbnail_number);
                        if thumb.is_none() && self.undecodable.contains(&it.path) {
                            let area = egui::Rect::from_min_size(rect.min + Vec2::new(2.0, 2.0), Vec2::new(172.0, 115.0));
                            ui.painter().text(area.center(), egui::Align2::CENTER_CENTER, t("無法解碼"), egui::FontId::proportional(theme::scaled(13.0)), theme::EDITED);
                        }
                    }
                    if resp.clicked() {
                        clicked = Some(i);
                    }
                    if resp.secondary_clicked() {
                        menu = Some((i, resp.interact_pointer_pos().unwrap_or(rect.center())));
                    }
                    if scroll_to && self.current == Some(i) {
                        ui.scroll_to_rect(rect, Some(egui::Align::Center));
                    }
                }
            });
        });
        let view = out.inner_rect;
        let offset = out.state.offset.x;
        let max_offset = (out.content_size.x - view.width()).max(0.0);
        // Right-button drag pans the strip; after a drag the release opens no menu.
        let (pressed, down, pos) = ui.input(|i| (i.pointer.secondary_pressed(), i.pointer.secondary_down(), i.pointer.interact_pos()));
        let pan = self.strip_pan.update(pressed && pos.is_some_and(|p| view.contains(p)), down, pos.map(|p| p.x), offset);
        if let Some(x) = pan.offset {
            self.strip_offset = Some(x.clamp(0.0, max_offset));
            ui.ctx().request_repaint();
        }
        if pan.panning {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        if pan.dragged {
            menu = None;
        }
        // ◀ ▶ over the ends while photos are hidden past them; a click pages one view.
        let arrow_h = 136.0 / 3.0;
        let arrow_w = arrow_h * 0.55;
        let mid = view.min.y + 4.0 + 136.0 / 2.0;
        for (left, more) in [(true, offset > 0.5), (false, offset < max_offset - 0.5)] {
            if !more {
                continue;
            }
            let x0 = if left { view.min.x + 6.0 } else { view.max.x - 6.0 - arrow_w };
            let r = egui::Rect::from_min_size(egui::pos2(x0, mid - arrow_h / 2.0), Vec2::new(arrow_w, arrow_h));
            let (tip, back) = if left { (r.min.x, r.max.x) } else { (r.max.x, r.min.x) };
            let pts = vec![egui::pos2(tip, mid), egui::pos2(back, r.min.y), egui::pos2(back, r.max.y)];
            ui.painter().add(egui::Shape::convex_polygon(pts, Color32::from_white_alpha(178), egui::Stroke::NONE));
            let hit = ui.interact(r.expand(6.0), ui.id().with(("strip_arrow", left)), egui::Sense::click());
            if hit.clicked() {
                let page = view.width() - 176.0;
                self.strip_offset = Some((if left { offset - page } else { offset + page }).clamp(0.0, max_offset));
                clicked = None;
            }
        }
        if let Some(i) = clicked {
            self.click_thumb(i, ctrl, shift);
        }
        if let Some((i, pos)) = menu {
            // Right-click on an unselected photo selects just it first.
            if !self.selected.contains(&i) {
                self.selected = [i].into();
                self.anchor = Some(i);
                self.select(i);
            }
            self.strip_menu = Some((i, pos, true));
        }
    }

    /// The thumbnail right-click menu (`ShowThumbnailMenu`).
    pub(super) fn strip_menu_ui(&mut self, ctx: &egui::Context) {
        let Some((i, pos, just_opened)) = self.strip_menu else { return };
        if i >= self.items.len() {
            self.strip_menu = None;
            return;
        }
        let sel = self.selected_items();
        let any_shown = sel.iter().any(|x| !x.hidden);
        let any_hidden = sel.iter().any(|x| x.hidden);
        let is_copy = self.items[i].copy > 0;
        let mut action = None;
        let mut item_pick = None;
        let area = egui::Area::new(egui::Id::new("strip_menu")).order(egui::Order::Foreground).fixed_pos(pos).constrain(true).show(ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                ui.set_min_width(210.0);
                let mut item = |ui: &mut egui::Ui, text: &str, enabled: bool, a: MenuAction| {
                    if ui.add_enabled(enabled, egui::Button::new(text).frame(false).min_size(Vec2::new(200.0, 22.0))).clicked() {
                        action = Some(a);
                    }
                };
                item(ui, t("全選"), true, MenuAction::SelectAll);
                item(ui, t("反向選擇"), true, MenuAction::Invert);
                item(ui, t("取消全選"), true, MenuAction::DeselectAll);
                ui.separator();
                egui::CollapsingHeader::new(t("套用風格檔")).id_salt("menu_presets").show(ui, |ui| {
                    for n in presets::BUILT_IN_NAMES.iter().map(|s| s.to_string()).chain(self.presets.custom_names()) {
                        item(ui, &tr(&n), true, MenuAction::ApplyPreset(n.clone()));
                    }
                });
                // Copying only makes sense for one photo.
                item(ui, t("複製照片設定"), sel.len() <= 1, MenuAction::Copy(i));
                item(ui, t("貼上照片設定"), self.copied.is_some(), MenuAction::Paste);
                item(ui, t("升級處理版本"), true, MenuAction::Upgrade);
                if self.settings.xmp_support {
                    item(ui, t("匯出 XMP"), true, MenuAction::ExportXmp);
                    item(ui, t("匯入 XMP"), true, MenuAction::ImportXmp);
                }
                ui.separator();
                item(ui, t("建立副本"), true, MenuAction::VirtualCopy(i));
                item(ui, t("隱藏且不輸出"), any_shown, MenuAction::Hide);
                item(ui, t("取消隱藏"), any_hidden, MenuAction::Unhide);
                item(ui, if is_copy { t("刪除副本") } else { t("刪除檔案") }, true, MenuAction::Delete(i));
                ui.separator();
                let show = self.settings.show_hidden;
                if ui.radio(!show, t("不顯示隱藏")).clicked() {
                    item_pick = Some(MenuAction::ShowHidden(false));
                }
                if ui.radio(show, t("顯示全部")).clicked() {
                    item_pick = Some(MenuAction::ShowHidden(true));
                }
                ui.separator();
                item(ui, t("匯出照片…"), true, MenuAction::Export);
            });
        });
        // A click anywhere else closes it (not the click that opened it).
        let outside = ctx.input(|inp| inp.pointer.any_pressed()) && !area.response.contains_pointer();
        if just_opened {
            self.strip_menu = Some((i, pos, false));
        } else if outside || ctx.input(|inp| inp.key_pressed(egui::Key::Escape)) {
            self.strip_menu = None;
        }
        let Some(a) = action.or(item_pick) else { return };
        self.strip_menu = None;
        match a {
            MenuAction::SelectAll => self.select_all(),
            MenuAction::Invert => self.invert_selection(),
            MenuAction::DeselectAll => self.deselect_all(),
            MenuAction::ApplyPreset(n) => self.apply_preset_to_selection(&n),
            MenuAction::Copy(i) => self.copy_settings(i),
            MenuAction::Paste => self.paste_settings(),
            MenuAction::Upgrade => self.upgrade_selected(),
            MenuAction::ExportXmp => self.export_xmp_selected(),
            MenuAction::ImportXmp => self.import_xmp_selected(),
            MenuAction::VirtualCopy(i) => self.create_virtual_copy(i),
            MenuAction::Hide => self.hide_selected(),
            MenuAction::Unhide => self.unhide_selected(),
            MenuAction::Delete(i) => self.delete_item(i),
            MenuAction::ShowHidden(s) => self.set_show_hidden(s),
            MenuAction::Export => self.open_export(true),
        }
    }

    /// The 確定 / 取消 window for deletes and upgrades.
    pub(super) fn confirm_ui(&mut self, ctx: &egui::Context) {
        let Some(c) = &self.confirm else { return };
        let (title, text) = match c {
            Confirm::DeleteFile(it) => (t("刪除照片檔案").to_string(), f("確定刪除檔案？（會移到資源回收桶）\n{0}", &[&it.name()])),
            Confirm::Upgrade(v) => (t("升級處理版本").to_string(), f("升級到處理版本 3（寬色域線性管線，可用高光復原、HSL 與曲線）。曝光與白平衡維持不變，畫面可能略有變化。要升級選取的 {0} 張照片嗎？", &[&v.len()])),
            Confirm::ClearCache => (t("刪除快取縮圖").to_string(), t("關閉資料夾並刪除此資料夾的快取與縮圖檔案？\n（編輯設定會保留，下次開啟會重新產生快取）").to_string()),
        };
        let mut answer = None;
        egui::Window::new(title).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.label(text);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new(t("確定")).color(Color32::WHITE)).fill(theme::ACCENT).min_size(Vec2::new(90.0, 28.0))).clicked() {
                    answer = Some(true);
                }
                if ui.add(egui::Button::new(t("取消")).min_size(Vec2::new(90.0, 28.0))).clicked() {
                    answer = Some(false);
                }
            });
        });
        match answer {
            Some(true) => match self.confirm.take() {
                Some(Confirm::DeleteFile(it)) => self.delete_file_confirmed(it),
                Some(Confirm::Upgrade(v)) => self.upgrade_confirmed(v),
                Some(Confirm::ClearCache) => self.clear_cache_confirmed(),
                None => {}
            },
            Some(false) => self.confirm = None,
            None => {}
        }
    }
}

/// One strip cell: thumbnail, #number, badges (hidden eye, copy, edited), selection.
#[allow(clippy::too_many_arguments)]
/// Right-button drag on the thumbnail strip: past `PAN_SLOP` points it pans, and the
/// button's release then opens no menu (the menu comes from a click, a release without
/// a drag).
#[derive(Default)]
pub(super) struct StripPan {
    /// Press x, the strip offset then, whether it has moved past the slop.
    drag: Option<(f32, f32, bool)>,
}

pub(super) struct PanStep {
    /// The strip's new offset.
    pub offset: Option<f32>,
    /// Panning now (grab cursor).
    pub panning: bool,
    /// This press became a drag: no menu for it.
    pub dragged: bool,
}

impl StripPan {
    const PAN_SLOP: f32 = 4.0;

    /// One frame: `pressed` = the right button went down over the strip this frame.
    pub fn update(&mut self, pressed: bool, down: bool, x: Option<f32>, offset: f32) -> PanStep {
        if pressed {
            self.drag = x.map(|x| (x, offset, false));
        }
        let mut step = PanStep { offset: None, panning: false, dragged: false };
        let Some((x0, off0, moved)) = self.drag.as_mut() else { return step };
        if down {
            if let Some(x) = x {
                let dx = x - *x0;
                *moved |= dx.abs() > Self::PAN_SLOP;
                if *moved {
                    step.offset = Some(*off0 - dx);
                }
            }
            step.panning = *moved;
            step.dragged = *moved;
        } else {
            step.dragged = *moved;
            self.drag = None;
        }
        step
    }
}

fn paint_thumb(p: &egui::Painter, rect: egui::Rect, it: &Item, thumb: Option<&egui::TextureHandle>, selected: bool, current: bool, copy_source: bool, show_number: bool) {
    let fill = if current {
        Color32::from_rgb(0x2F, 0x3E, 0x52)
    } else if selected {
        Color32::from_rgb(0x2A, 0x30, 0x3A)
    } else {
        theme::WINDOW
    };
    p.rect_filled(rect, 3.0, fill);
    let img_area = egui::Rect::from_min_size(rect.min + Vec2::new(2.0, 2.0), Vec2::new(172.0, 115.0));
    let mut photo = img_area;
    if let Some(tex) = thumb {
        let s = tex.size_vec2();
        let k = (img_area.width() / s.x).min(img_area.height() / s.y);
        photo = egui::Rect::from_center_size(img_area.center(), s * k);
        p.image(tex.id(), photo, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), if it.hidden { Color32::from_gray(110) } else { Color32::WHITE });
    } else {
        p.rect_filled(img_area, 2.0, theme::PANEL);
    }
    // #number counts hidden photos too, so it skips when one is hidden.
    if show_number {
        let tag = p.layout_no_wrap(format!("#{}", it.number), egui::FontId::proportional(theme::scaled(12.0)), Color32::WHITE);
        let tag_rect = egui::Rect::from_min_size(photo.min + Vec2::new(3.0, 3.0), tag.size() + Vec2::new(8.0, 2.0));
        p.rect_filled(tag_rect, 3.0, Color32::from_rgba_unmultiplied(90, 90, 98, 185));
        p.galley(tag_rect.min + Vec2::new(4.0, 1.0), tag, Color32::WHITE);
    }

    // Badges, right-aligned from the top-right corner.
    let mut rx = photo.max.x - 4.0;
    let ry = photo.min.y + 4.0;
    if it.hidden {
        let r = egui::Rect::from_min_size(egui::pos2(rx - 18.0, ry), Vec2::new(18.0, 13.0));
        p.rect_filled(r, 3.0, Color32::from_rgba_unmultiplied(40, 40, 46, 205));
        let pen = egui::Stroke::new(1.4, Color32::from_rgb(240, 240, 244));
        // An eye with a slash through it.
        let eye: Vec<egui::Pos2> = (0..=24).map(|k| r.center() + Vec2::angled(k as f32 / 24.0 * std::f32::consts::TAU) * Vec2::new(5.0, 3.0)).collect();
        p.add(egui::Shape::line(eye, pen));
        p.line_segment([egui::pos2(r.min.x + 3.0, r.max.y - 2.0), egui::pos2(r.max.x - 3.0, r.min.y + 2.0)], pen);
        rx -= 22.0;
    }
    if it.copy > 0 {
        let r = egui::Rect::from_min_size(egui::pos2(rx - 30.0, ry), Vec2::new(30.0, 13.0));
        p.rect_filled(r, 3.0, Color32::from_rgb(0xE6, 0xC8, 0x4C));
        p.text(r.center(), egui::Align2::CENTER_CENTER, "copy", egui::FontId::proportional(theme::scaled(10.5)), Color32::BLACK);
        rx -= 34.0;
    }
    if it.edited {
        p.circle_filled(egui::pos2(rx - 4.5, ry + 4.5), 4.5, theme::EDITED);
    }
    if copy_source {
        let r = photo.shrink(1.0);
        let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
        p.extend(egui::Shape::dashed_line(&pts, egui::Stroke::new(2.0, Color32::from_rgb(0xE6, 0xC8, 0x4C)), 6.0, 3.0));
    }
    p.text(
        egui::pos2(rect.center().x, rect.max.y - 9.0),
        egui::Align2::CENTER_CENTER,
        super::truncate(&it.name(), 26),
        egui::FontId::proportional(theme::scaled(11.5)),
        if selected { theme::TEXT } else { theme::TEXT_DIM },
    );
    if selected {
        p.rect_stroke(rect, 3.0, egui::Stroke::new(2.0, theme::ACCENT), egui::StrokeKind::Inside);
    }
}

#[cfg(test)]
mod tests {
    use super::StripPan;

    #[test]
    fn strip_pan_slop_and_menu() {
        // A right click that barely moves: no pan, the menu stays.
        let mut p = StripPan::default();
        assert!(p.update(true, true, Some(100.0), 50.0).offset.is_none());
        assert!(p.update(false, true, Some(103.0), 50.0).offset.is_none());
        let r = p.update(false, false, Some(103.0), 50.0);
        assert!(!r.dragged && r.offset.is_none());
        // A drag: past 4 points it pans by the pointer's travel (left drag = scroll right),
        // and the release swallows the menu.
        let mut p = StripPan::default();
        p.update(true, true, Some(100.0), 50.0);
        let r = p.update(false, true, Some(80.0), 50.0);
        assert_eq!((r.offset, r.panning), (Some(70.0), true));
        // Coming back within the slop still pans (once moved, always a drag).
        assert_eq!(p.update(false, true, Some(98.0), 50.0).offset, Some(52.0));
        let r = p.update(false, false, Some(98.0), 50.0);
        assert!(r.dragged && !r.panning);
        // Nothing pressed: nothing happens.
        assert!(!p.update(false, false, Some(0.0), 0.0).dragged);
    }
}
