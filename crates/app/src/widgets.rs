//! Self-drawn controls: the adjustment slider (`AdjustmentSlider`) and the histogram.

use crate::theme;
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};

#[derive(Clone, Copy, PartialEq)]
pub enum Gradient {
    None,
    /// Blue → amber (low Kelvin = "the light was warm" = cooler picture).
    Temperature,
    /// Green → magenta.
    Tint,
    /// Grey → full colour.
    Saturation,
}

#[derive(Clone, Copy)]
pub struct SliderSpec {
    pub label: &'static str,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    pub decimals: usize,
    /// Neutral in the middle: draw a tick at the default and fill from it.
    pub bipolar: bool,
    pub gradient: Gradient,
    /// One wheel notch.
    pub wheel_step: f64,
}

impl SliderSpec {
    pub const fn pm100(label: &'static str) -> Self {
        Self { label, min: -100.0, max: 100.0, default: 0.0, decimals: 0, bipolar: true, gradient: Gradient::None, wheel_step: 1.0 }
    }

    pub const fn gradient(mut self, g: Gradient) -> Self {
        self.gradient = g;
        self
    }
}

#[derive(Default)]
pub struct SliderResponse {
    /// An edit gesture started (push the undo step now, before the value moves).
    pub began: bool,
    pub changed: bool,
}

fn round_to(v: f64, decimals: usize) -> f64 {
    let f = 10f64.powi(decimals as i32);
    (v * f).round() / f
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

fn gradient_color(g: Gradient, t: f32) -> Color32 {
    match g {
        Gradient::None => theme::TRACK,
        Gradient::Temperature => lerp_color(Color32::from_rgb(0x4A, 0x7F, 0xD6), Color32::from_rgb(0xE0, 0xA8, 0x3A), t),
        Gradient::Tint => lerp_color(Color32::from_rgb(0x4C, 0xB0, 0x5A), Color32::from_rgb(0xC8, 0x4C, 0xC0), t),
        Gradient::Saturation => {
            // Hue around the wheel, saturation rising left to right.
            let hue = t * 360.0;
            let c = egui::ecolor::Hsva::new(hue / 360.0, t * 0.75, 0.72, 1.0);
            Color32::from(c)
        }
    }
}

/// A column shared by the slider rows of one panel (one `Ui`) — the labels, the value
/// boxes: as wide as its widest entry in the current language, measured anew every pass.
#[derive(Clone, Copy, Default)]
struct SharedColumn {
    pass: u64,
    /// The widest entry seen so far in this pass.
    cur: f32,
    /// The widest entry of the previous pass (what the rows use).
    prev: f32,
}

/// The label width a row always gets: 5 CJK characters (the longest zh label is 4).
fn label_cap() -> f32 {
    5.0 * theme::scaled(14.0)
}

/// The shortest track (points) the label column may squeeze it to beyond `label_cap`.
const MIN_TRACK: f32 = 110.0;

fn slider_scope_key() -> egui::Id {
    egui::Id::new("awpr_slider_scope")
}

/// Slider rows added inside `f` share one label / value column even when each sits in its
/// own child `Ui` (`push_id`), as the HSL and mask rows do.
pub fn slider_scope<R>(ui: &mut egui::Ui, f: impl FnOnce(&mut egui::Ui) -> R) -> R {
    // Its own key: the rows added straight into `ui` (another page) keep theirs.
    let id = ui.id().with("awpr_slider_scope");
    ui.data_mut(|d| d.insert_temp(slider_scope_key(), Some(id)));
    let r = f(ui);
    ui.data_mut(|d| d.insert_temp::<Option<egui::Id>>(slider_scope_key(), None));
    r
}

/// The width of the shared column `key` for a row whose own entry needs `need`.
fn shared_column_width(ui: &egui::Ui, key: &'static str, need: f32) -> f32 {
    let base = ui.data(|d| d.get_temp::<Option<egui::Id>>(slider_scope_key())).flatten().unwrap_or(ui.id());
    let id = base.with(("awpr_slider_column", key));
    let pass = ui.ctx().cumulative_pass_nr();
    let mut col = ui.data(|d| d.get_temp::<SharedColumn>(id)).unwrap_or_default();
    if col.pass != pass {
        if col.cur != col.prev && col.pass != 0 {
            // The longest label changed (language, panel content): lay out once more.
            ui.ctx().request_repaint();
        }
        col.prev = col.cur;
        col.cur = 0.0;
        col.pass = pass;
    }
    col.cur = col.cur.max(need);
    if need > col.prev {
        // Rows above were laid out narrower: redo this pass rather than show them ragged.
        ui.ctx().request_discard("slider column");
    }
    let w = col.prev.max(need);
    ui.data_mut(|d| d.insert_temp(id, col));
    w
}

/// One slider row on a single line: `label | track | value`. The label column is as wide
/// as the panel's longest label as long as the track keeps `MIN_TRACK` (it always gets
/// `label_cap`; a longer label is cut with "…" and shown on hover), the value boxes are as
/// wide as the panel's longest value ("-100", "12000", "0.00"), the track takes the rest.
pub fn adjust_slider(ui: &mut egui::Ui, spec: &SliderSpec, value: &mut f64, enabled: bool) -> SliderResponse {
    let mut out = SliderResponse::default();
    let width = ui.available_width();
    let font = egui::FontId::proportional(theme::scaled(14.0));
    let text_col = if enabled { theme::TEXT } else { theme::TEXT_FAINT };
    // Label / value to the track's end; the 6.5 point knob overhangs the end into it.
    let gap = 6.0;
    let value_font = egui::TextStyle::Body.resolve(ui.style());
    let value_need = [spec.min, spec.max].iter().map(|v| text_width(ui, &format!("{v:.*}", spec.decimals), &value_font)).fold(0.0f32, f32::max) + 2.0;
    let value_w = shared_column_width(ui, "value", value_need);
    let need = text_width(ui, spec.label, &font);
    let room = width - value_w - 2.0 * gap - MIN_TRACK;
    let label_w = shared_column_width(ui, "label", need).min(label_cap().max(room)).max(24.0);
    // 95 % of the 2.0.1 row (2.0.2: a little denser).
    let row_h = ((ui.fonts_mut(|f| f.row_height(&font)) + 6.0).max(24.0) * 0.95).round();
    let (outer, outer_resp) = ui.allocate_exact_size(Vec2::new(width, row_h), Sense::hover());
    if !ui.is_rect_visible(outer) {
        return out;
    }

    // Label (double-click → default, like double-clicking the slider). Never wraps.
    let label_rect = Rect::from_min_size(outer.min, Vec2::new(label_w, row_h));
    let label_resp = ui.interact(label_rect, ui.id().with((spec.label, "label")), Sense::click());
    let mut job = egui::text::LayoutJob::simple_singleline(spec.label.to_owned(), font, text_col);
    job.wrap = egui::text::TextWrapping::truncate_at_width(label_w);
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let elided = galley.elided;
    ui.painter().galley(Pos2::new(label_rect.min.x, label_rect.center().y - galley.size().y / 2.0), galley, text_col);
    let label_resp = if elided { label_resp.on_hover_text(spec.label) } else { label_resp };

    // Editable value on the right.
    let value_h = ui.spacing().interact_size.y;
    let value_rect = Rect::from_min_size(Pos2::new(outer.max.x - value_w, outer.center().y - value_h / 2.0), Vec2::new(value_w, value_h));
    let mut v = *value;
    // In a child sized to the box (a DragValue is otherwise at least `interact_size.x`).
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(value_rect).layout(egui::Layout::centered_and_justified(egui::Direction::TopDown)));
    child.spacing_mut().interact_size.x = value_w;
    child.spacing_mut().button_padding.x = 2.0;
    let dv = child.add(
        egui::DragValue::new(&mut v)
            .range(spec.min..=spec.max)
            .speed((spec.max - spec.min) / 400.0)
            .fixed_decimals(spec.decimals),
    );
    if enabled && (dv.drag_started() || dv.gained_focus()) {
        out.began = true;
    }
    if enabled && dv.changed() {
        *value = round_to(v, spec.decimals);
        out.changed = true;
    }

    // Track.
    let track = Rect::from_min_max(Pos2::new(label_rect.max.x + gap, outer.center().y - 4.0), Pos2::new(value_rect.min.x - gap, outer.center().y + 4.0));
    let hit = track.expand2(Vec2::new(6.0, 6.0));
    let resp = ui.interact(hit, ui.id().with((spec.label, "track")), Sense::click_and_drag());
    let t_of = |v: f64| (((v - spec.min) / (spec.max - spec.min)).clamp(0.0, 1.0)) as f32;
    let painter = ui.painter();
    let line = Rect::from_center_size(track.center(), Vec2::new(track.width(), 6.0));
    if spec.gradient == Gradient::None {
        painter.rect_filled(line, 3.0, theme::TRACK);
        // Fill from the neutral point (bipolar) or the left end.
        let from = if spec.bipolar { t_of(spec.default) } else { 0.0 };
        let to = t_of(*value);
        let (a, b) = if from < to { (from, to) } else { (to, from) };
        let fill = Rect::from_min_max(Pos2::new(line.min.x + a * line.width(), line.min.y), Pos2::new(line.min.x + b * line.width(), line.max.y));
        painter.rect_filled(fill, 3.0, if enabled { theme::ACCENT } else { theme::TEXT_FAINT });
    } else {
        // A colour band, no fill (two layers on 6 px only muddy it).
        let n = 48;
        for i in 0..n {
            let t0 = i as f32 / n as f32;
            let t1 = (i + 1) as f32 / n as f32;
            let r = Rect::from_min_max(Pos2::new(line.min.x + t0 * line.width(), line.min.y), Pos2::new(line.min.x + t1 * line.width() + 0.5, line.max.y));
            let mut c = gradient_color(spec.gradient, (t0 + t1) / 2.0);
            if !enabled {
                c = lerp_color(c, theme::PANEL, 0.6);
            }
            painter.rect_filled(r, 0.0, c);
        }
    }
    if spec.bipolar {
        let x = line.min.x + t_of(spec.default) * line.width();
        painter.line_segment([Pos2::new(x, line.min.y - 3.0), Pos2::new(x, line.max.y + 3.0)], Stroke::new(1.0, theme::TEXT_FAINT));
    }
    let kx = line.min.x + t_of(*value) * line.width();
    let knob_col = if !enabled {
        theme::TEXT_FAINT
    } else if resp.hovered() || resp.dragged() {
        Color32::WHITE
    } else {
        Color32::from_rgb(0xD8, 0xD8, 0xDC)
    };
    painter.circle(Pos2::new(kx, line.center().y), 6.5, knob_col, Stroke::new(1.0, Color32::from_black_alpha(120)));

    if !enabled {
        return out;
    }
    let set_from_x = |x: f32, value: &mut f64| {
        let t = ((x - line.min.x) / line.width()).clamp(0.0, 1.0) as f64;
        let nv = round_to(spec.min + t * (spec.max - spec.min), spec.decimals);
        if nv != *value {
            *value = nv;
            true
        } else {
            false
        }
    };
    if resp.double_clicked() || label_resp.double_clicked() {
        out.began = true;
        if *value != spec.default {
            *value = spec.default;
            out.changed = true;
        }
    } else if resp.drag_started() || resp.clicked() {
        out.began = true;
        if let Some(p) = resp.interact_pointer_pos() {
            out.changed |= set_from_x(p.x, value);
        }
    } else if resp.dragged() {
        if let Some(p) = resp.interact_pointer_pos() {
            out.changed |= set_from_x(p.x, value);
        }
    }
    // Wheel over the slider fine-tunes it.
    if outer_resp.hovered() {
        let notches = ui.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::MouseWheel { delta, .. } => Some(delta.y.signum()),
                    _ => None,
                })
                .sum::<f32>()
        });
        if notches != 0.0 {
            let nv = round_to((*value + notches as f64 * spec.wheel_step).clamp(spec.min, spec.max), spec.decimals);
            if nv != *value {
                out.began = true;
                *value = nv;
                out.changed = true;
            }
        }
    }
    out
}

// ---- point curve editor (處理版本 3 曲線) -------------------------------------------------

/// The 曲線 editor: a square with the curve through its points. Click empty space to add a
/// point (and drag it), drag a point to move it, double-click or right-click a point to
/// remove it (the two end points stay). Points are kept in x order, 0..1 × 0..1; an
/// untouched curve is an empty list (identity).
pub fn curve_editor(ui: &mut egui::Ui, id: egui::Id, points: &mut Vec<(f64, f64)>, color: Color32, enabled: bool, drag: &mut Option<usize>) -> SliderResponse {
    let mut out = SliderResponse::default();
    let side = ui.available_width().min(260.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(side, side), if enabled { Sense::click_and_drag() } else { Sense::hover() });
    let _ = id;
    let painter = ui.painter_at(rect.expand(6.0));
    painter.rect_filled(rect, 3.0, theme::VIEWER);
    let grid = Stroke::new(1.0, Color32::from_white_alpha(18));
    for i in 1..4 {
        let f = i as f32 / 4.0;
        painter.line_segment([Pos2::new(rect.min.x + f * rect.width(), rect.min.y), Pos2::new(rect.min.x + f * rect.width(), rect.max.y)], grid);
        painter.line_segment([Pos2::new(rect.min.x, rect.min.y + f * rect.height()), Pos2::new(rect.max.x, rect.min.y + f * rect.height())], grid);
    }
    painter.line_segment([rect.left_bottom(), rect.right_top()], Stroke::new(1.0, Color32::from_white_alpha(30)));
    let to_screen = |(x, y): (f64, f64)| Pos2::new(rect.min.x + x as f32 * rect.width(), rect.max.y - y as f32 * rect.height());
    let to_norm = |p: Pos2| (((p.x - rect.min.x) / rect.width()).clamp(0.0, 1.0) as f64, ((rect.max.y - p.y) / rect.height()).clamp(0.0, 1.0) as f64);

    let shown: Vec<(f64, f64)> = if points.len() >= 2 { points.clone() } else { vec![(0.0, 0.0), (1.0, 1.0)] };
    let curve = awpr_core::v3::MonotoneCurve::new(&shown);
    let line: Vec<Pos2> = (0..=128).map(|i| {
        let x = i as f64 / 128.0;
        to_screen((x, curve.eval(x)))
    }).collect();
    let col = if enabled { color } else { theme::TEXT_FAINT };
    painter.add(egui::Shape::line(line, Stroke::new(2.0, col)));
    for (i, &p) in shown.iter().enumerate() {
        let active = *drag == Some(i);
        painter.circle(to_screen(p), if active { 5.5 } else { 4.5 }, if active { Color32::WHITE } else { col }, Stroke::new(1.0, Color32::from_black_alpha(160)));
    }
    if !enabled {
        return out;
    }

    let hit = |pos: Pos2, pts: &[(f64, f64)]| pts.iter().enumerate().map(|(i, &p)| (i, to_screen(p).distance(pos))).filter(|(_, d)| *d <= 9.0).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| i);
    let remove = |pts: &mut Vec<(f64, f64)>, i: usize| -> bool {
        if pts.len() >= 2 && i > 0 && i + 1 < pts.len() {
            pts.remove(i);
            true
        } else {
            false
        }
    };
    if resp.double_clicked() || resp.secondary_clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let mut pts = shown.clone();
            if let Some(i) = hit(pos, &pts) {
                out.began = true;
                if remove(&mut pts, i) {
                    *points = pts;
                    out.changed = true;
                }
            }
        }
        *drag = None;
        return out;
    }
    if resp.drag_started() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            out.began = true;
            let mut pts = shown.clone();
            let i = match hit(pos, &pts) {
                Some(i) => i,
                None => {
                    let (x, y) = to_norm(pos);
                    let at = pts.iter().position(|p| p.0 > x).unwrap_or(pts.len());
                    // Too close to a neighbour: no new point.
                    if (at > 0 && x - pts[at - 1].0 < 0.02) || (at < pts.len() && pts[at].0 - x < 0.02) {
                        return out;
                    }
                    pts.insert(at, (x, y));
                    out.changed = true;
                    at
                }
            };
            *points = pts;
            *drag = if resp.clicked() { None } else { Some(i) };
        }
    } else if resp.dragged() {
        if let (Some(i), Some(pos)) = (*drag, resp.interact_pointer_pos()) {
            let mut pts = shown.clone();
            if i < pts.len() {
                let (mut x, y) = to_norm(pos);
                let lo = if i == 0 { 0.0 } else { pts[i - 1].0 + 0.01 };
                let hi = if i + 1 == pts.len() { 1.0 } else { pts[i + 1].0 - 0.01 };
                x = x.clamp(lo, hi.max(lo));
                if pts[i] != (x, y) {
                    pts[i] = (x, y);
                    *points = pts;
                    out.changed = true;
                }
            }
        }
    }
    if resp.drag_stopped() {
        *drag = None;
    }
    out
}

/// Text in a box of exactly `width` × `height`, left-aligned and vertically centred, cut
/// with "…" when it does not fit (the full text on hover). Never wraps or widens the column.
pub fn fixed_text(ui: &mut egui::Ui, width: f32, height: f32, text: &str, font: egui::FontId, color: Color32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(width);
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let elided = galley.elided;
    ui.painter().galley(Pos2::new(rect.min.x, rect.center().y - galley.size().y / 2.0), galley, color);
    if elided { resp.on_hover_text(text) } else { resp }
}

/// Widths for items that want `want` but must share `room`: short ones keep their width,
/// the long ones are cut to an equal share of what is left.
pub fn share_widths<const N: usize>(want: [f32; N], room: f32) -> [f32; N] {
    let mut sorted = want;
    sorted.sort_by(f32::total_cmp);
    let mut left = room;
    let mut cap = f32::INFINITY;
    for (i, w) in sorted.iter().enumerate() {
        let share = left / (N - i) as f32;
        if *w > share {
            cap = share;
            break;
        }
        left -= w;
    }
    want.map(|w| w.min(cap).max(0.0))
}

/// The width `text` needs in `font`.
pub fn text_width(ui: &egui::Ui, text: &str, font: &egui::FontId) -> f32 {
    ui.fonts_mut(|f| f.layout_no_wrap(text.to_owned(), font.clone(), Color32::WHITE).size().x).ceil()
}

/// A button of exactly `size` whose text is cut with "…" when it does not fit: words go into
/// the fixed box instead of widening the column (the C# rule for long translations). The
/// full text shows on hover when it was cut. The text is centred.
pub fn fixed_button(ui: &mut egui::Ui, enabled: bool, size: Vec2, text: impl Into<egui::WidgetText>, f: impl FnOnce(egui::Button) -> egui::Button) -> egui::Response {
    let text: egui::WidgetText = text.into();
    let full = text.text().to_string();
    let cut = text_width(ui, &full, &egui::TextStyle::Button.resolve(ui.style())) + 2.0 * ui.spacing().button_padding.x > size.x;
    // The box is allocated first and the button placed in it: a cut label can make the
    // button report a few points more than `size`, which must not widen the column.
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::centered_and_justified(egui::Direction::TopDown)));
    if !enabled {
        child.disable();
    }
    child.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
    let resp = child.add(f(egui::Button::new(text).min_size(size)));
    if cut { resp.on_hover_text(full) } else { resp }
}

/// 256-bin RGB histogram (R, G, B in `bins[0..256]`, `[256..512]`, `[512..768]`).
pub struct Histogram {
    pub bins: Vec<u32>,
}

impl Histogram {
    /// Per-channel mean in 0..255 units (Σ i·count / n — identical to summing pixels).
    pub fn means(&self) -> [f64; 3] {
        let mut m = [0.0; 3];
        for (c, mc) in m.iter_mut().enumerate() {
            let ch = &self.bins[c * 256..(c + 1) * 256];
            let n: u64 = ch.iter().map(|&v| v as u64).sum();
            if n > 0 {
                *mc = ch.iter().enumerate().map(|(i, &v)| i as f64 * v as f64).sum::<f64>() / n as f64;
            }
        }
        m
    }

    /// From a rendered CPU buffer, binned like `ImageStats.computeHistogram`.
    pub fn from_float(data: &[f32]) -> Self {
        let mut bins = vec![0u32; 768];
        let bin = |v: f32| ((v * 255.0 + 0.5) as i32).clamp(0, 255) as usize;
        for px in data.chunks_exact(4) {
            bins[bin(px[0])] += 1;
            bins[256 + bin(px[1])] += 1;
            bins[512 + bin(px[2])] += 1;
        }
        Self { bins }
    }
}

/// `compact` (a tool is open): the means go inside the plot's top-left corner instead of
/// a line of their own.
pub fn histogram(ui: &mut egui::Ui, h: Option<&Histogram>, height: f32, compact: bool) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, theme::VIEWER);
    let Some(h) = h else { return };
    // Ignore the pure 0 / 255 spikes when scaling, so a clipped sky does not flatten the rest.
    let max = (1..255).flat_map(|i| [h.bins[i], h.bins[256 + i], h.bins[512 + i]]).max().unwrap_or(1).max(1) as f32;
    let colors = [Color32::from_rgba_unmultiplied(235, 70, 70, 110), Color32::from_rgba_unmultiplied(70, 200, 90, 110), Color32::from_rgba_unmultiplied(80, 130, 255, 110)];
    let w = rect.width() / 256.0;
    for (c, col) in colors.iter().enumerate() {
        let mut mesh = egui::Mesh::default();
        for i in 0..256 {
            let v = (h.bins[c * 256 + i] as f32 / max).min(1.0);
            if v <= 0.0 {
                continue;
            }
            let x0 = rect.min.x + i as f32 * w;
            let r = Rect::from_min_max(Pos2::new(x0, rect.max.y - v * (rect.height() - 4.0)), Pos2::new(x0 + w + 0.3, rect.max.y));
            mesh.add_colored_rect(r, *col);
        }
        painter.add(mesh);
    }
    let m = h.means();
    let means = format!("R {:.1}  G {:.1}  B {:.1}", m[0], m[1], m[2]);
    if compact {
        let font = egui::TextStyle::Monospace.resolve(ui.style());
        let galley = painter.layout_no_wrap(means, font, theme::TEXT);
        let at = rect.min + Vec2::new(4.0, 2.0);
        painter.rect_filled(Rect::from_min_size(at, galley.size()).expand(2.0), 2.0, Color32::from_black_alpha(150));
        painter.galley(at, galley, theme::TEXT);
    } else {
        ui.label(egui::RichText::new(means).monospace().color(theme::TEXT_DIM));
    }
}
