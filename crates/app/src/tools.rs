//! The interactive tools on the viewer — crop box, linear gradients, heal spots and the
//! white-balance picker. Port of the C# `ImageViewer` overlays (and the bits of
//! `MainForm` they call), kept free of egui so the geometry can be unit-tested.
//!
//! Screen positions are in points: handle sizes, hit radii and line widths are fixed
//! point values, so they follow the display scaling, while the image geometry (`View`)
//! maps image pixels exactly (CLAUDE.md principle 5).

use awpr_core::color::{self, WhiteBalanceReference};
use awpr_core::pipeline::white_balance_multipliers;
use awpr_core::{CameraColorInfo, HealSpot, ImageAdjustments, LinearGradient, LocalMask, Rotation};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolMode {
    None,
    Crop,
    Gradient,
    Heal,
    /// 遮罩 (處理版本 3).
    Mask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealMode {
    /// 仿製: copy from a source circle.
    Clone,
    /// 修補: fill from the surroundings.
    Inpaint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drag {
    None,
    CropMove,
    CropL,
    CropR,
    CropT,
    CropB,
    CropTL,
    CropTR,
    CropBL,
    CropBR,
    GradCenter,
    GradRotate,
    GradRange,
    HealTarget,
    HealSource,
    MaskCenter,
    MaskRadiusX,
    MaskRadiusY,
    MaskRotate,
    MaskPaint,
}

/// Crop corner grab zone; corners are tested first (2026-08-31: 10 → 20 and priority over edges).
pub const CROP_CORNER_HIT: f64 = 20.0;
/// Crop edge grab zone.
pub const CROP_EDGE_HIT: f64 = 10.0;
/// Gradient handle hit radius (doubled with the handles, 12 → 24).
pub const HANDLE_HIT_RADIUS: f64 = 24.0;
/// Blue rotate handle: fixed screen distance from the white dot (64 → 256).
pub const ROT_HANDLE_DIST: f64 = 256.0;
/// Smallest heal hit radius, for spots drawn tiny at low zoom.
pub const HEAL_MIN_HIT: f64 = 8.0;
/// Smallest crop side (normalized).
const MIN_CROP: f64 = 0.05;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct P {
    pub x: f64,
    pub y: f64,
}

impl P {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

pub fn dist(a: P, b: P) -> f64 {
    ((a.x - b.x) * (a.x - b.x) + (a.y - b.y) * (a.y - b.y)).sqrt()
}

/// Where the displayed image sits on screen: `ImgToCtrl` / `CtrlToNorm` of the C# viewer.
#[derive(Debug, Clone, Copy)]
pub struct View {
    /// Screen position of the image's top-left corner.
    pub ox: f64,
    pub oy: f64,
    /// Screen units per image pixel.
    pub scale: f64,
    /// Displayed image size (pixels).
    pub w: f64,
    pub h: f64,
}

impl View {
    pub fn img_to_ctrl(&self, fx: f64, fy: f64) -> P {
        P::new(self.ox + fx * self.scale, self.oy + fy * self.scale)
    }

    pub fn norm_to_ctrl(&self, nx: f64, ny: f64) -> P {
        self.img_to_ctrl(nx * self.w, ny * self.h)
    }

    pub fn ctrl_to_norm(&self, p: P) -> (f64, f64) {
        let ix = (p.x - self.ox) / self.scale / self.w;
        let iy = (p.y - self.oy) / self.scale / self.h;
        (ix.clamp(0.0, 1.0), iy.clamp(0.0, 1.0))
    }
}

// ---- crop --------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Box4 {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl Box4 {
    fn contains(&self, p: P) -> bool {
        // System.Drawing.RectangleF.Contains: right / bottom edges excluded.
        p.x >= self.left && p.x < self.right && p.y >= self.top && p.y < self.bottom
    }
}

pub fn crop_ctrl_rect(adj: &ImageAdjustments, v: &View) -> Box4 {
    let tl = v.norm_to_ctrl(adj.crop_x, adj.crop_y);
    let br = v.norm_to_ctrl(adj.crop_x + adj.crop_width, adj.crop_y + adj.crop_height);
    Box4 { left: tl.x, top: tl.y, right: br.x, bottom: br.y }
}

/// Corners first with the larger grab zone, then edges, then inside (move).
pub fn crop_hit_test(r: &Box4, p: P) -> Drag {
    let (hc, h) = (CROP_CORNER_HIT, CROP_EDGE_HIT);
    let (cl, cr) = ((p.x - r.left).abs() < hc, (p.x - r.right).abs() < hc);
    let (ct, cb) = ((p.y - r.top).abs() < hc, (p.y - r.bottom).abs() < hc);
    if cl && ct {
        return Drag::CropTL;
    }
    if cr && ct {
        return Drag::CropTR;
    }
    if cl && cb {
        return Drag::CropBL;
    }
    if cr && cb {
        return Drag::CropBR;
    }
    let (l, rr) = ((p.x - r.left).abs() < h, (p.x - r.right).abs() < h);
    let (t, b) = ((p.y - r.top).abs() < h, (p.y - r.bottom).abs() < h);
    let in_x = p.x > r.left - h && p.x < r.right + h;
    let in_y = p.y > r.top - h && p.y < r.bottom + h;
    if l && in_y {
        return Drag::CropL;
    }
    if rr && in_y {
        return Drag::CropR;
    }
    if t && in_x {
        return Drag::CropT;
    }
    if b && in_x {
        return Drag::CropB;
    }
    if r.contains(p) {
        Drag::CropMove
    } else {
        Drag::None
    }
}

/// "3:2" → 1.5; "Custom" or anything unparsable → 0 (`MainForm.ParseAspect`).
pub fn parse_aspect(aspect: &str) -> f64 {
    if aspect == "Custom" {
        return 0.0;
    }
    match aspect.split_once(':') {
        Some((a, b)) if !b.contains(':') => match (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
            (Ok(a), Ok(b)) if b > 0.0 => a / b,
            _ => 0.0,
        },
        _ => 0.0,
    }
}

/// The locked crop aspect (pixel width / height) while dragging. "Original" locks to the
/// displayed photo's own proportions; 0 = free.
pub fn crop_aspect(adj: &ImageAdjustments, w: f64, h: f64) -> f64 {
    if h <= 0.0 {
        return 0.0;
    }
    let a = adj.crop_aspect_ratio.as_str();
    if a.is_empty() || a == "Original" {
        return w / h;
    }
    match a.split_once(':') {
        Some((x, y)) if !y.contains(':') => match (x.parse::<f64>(), y.parse::<f64>()) {
            (Ok(x), Ok(y)) if x > 0.0 && y > 0.0 => x / y,
            _ => 0.0,
        },
        _ => 0.0,
    }
}

fn set_crop_edge(adj: &mut ImageAdjustments, left: Option<f64>, right: Option<f64>, top: Option<f64>, bottom: Option<f64>) {
    let (mut x0, mut y0) = (adj.crop_x, adj.crop_y);
    let (mut x1, mut y1) = (adj.crop_x + adj.crop_width, adj.crop_y + adj.crop_height);
    if let Some(l) = left {
        x0 = l.clamp(0.0, x1 - MIN_CROP);
    }
    if let Some(r) = right {
        x1 = r.clamp(x0 + MIN_CROP, 1.0);
    }
    if let Some(t) = top {
        y0 = t.clamp(0.0, y1 - MIN_CROP);
    }
    if let Some(b) = bottom {
        y1 = b.clamp(y0 + MIN_CROP, 1.0);
    }
    adj.crop_x = x0;
    adj.crop_y = y0;
    adj.crop_width = x1 - x0;
    adj.crop_height = y1 - y0;
}

/// Drag the whole box: `start` is the box when the drag began, (dx, dy) the normalized
/// pointer movement since then.
pub fn move_crop(adj: &mut ImageAdjustments, start: [f64; 4], dx: f64, dy: f64) {
    let [sx, sy, sw, sh] = start;
    adj.crop_x = (sx + dx).clamp(0.0, 1.0 - sw);
    adj.crop_y = (sy + dy).clamp(0.0, 1.0 - sh);
}

/// Resize from an edge / corner handle to the normalized pointer (nx, ny) on a displayed
/// image of w × h pixels. A locked aspect keeps its ratio: corners pivot on the opposite
/// corner, edges grow the other dimension around the centre.
pub fn drag_crop_handle(adj: &mut ImageAdjustments, mode: Drag, nx: f64, ny: f64, w: f64, h: f64) {
    let ratio = crop_aspect(adj, w, h);
    if ratio <= 0.0 {
        match mode {
            Drag::CropL => set_crop_edge(adj, Some(nx), None, None, None),
            Drag::CropR => set_crop_edge(adj, None, Some(nx), None, None),
            Drag::CropT => set_crop_edge(adj, None, None, Some(ny), None),
            Drag::CropB => set_crop_edge(adj, None, None, None, Some(ny)),
            Drag::CropTL => set_crop_edge(adj, Some(nx), None, Some(ny), None),
            Drag::CropTR => set_crop_edge(adj, None, Some(nx), Some(ny), None),
            Drag::CropBL => set_crop_edge(adj, Some(nx), None, None, Some(ny)),
            Drag::CropBR => set_crop_edge(adj, None, Some(nx), None, Some(ny)),
            _ => {}
        }
        return;
    }

    let (x0, y0) = (adj.crop_x, adj.crop_y);
    let (x1, y1) = (x0 + adj.crop_width, y0 + adj.crop_height);
    let (cx, cy) = (x0 + adj.crop_width / 2.0, y0 + adj.crop_height / 2.0);
    // Normalized height = normalized width * k keeps the pixel ratio.
    let k = w / (h * ratio);
    let mut set = |x: f64, y: f64, wn: f64, hn: f64| {
        adj.crop_x = x;
        adj.crop_y = y;
        adj.crop_width = wn;
        adj.crop_height = hn;
    };

    match mode {
        Drag::CropBR => {
            let mut wn = MIN_CROP.max(nx - x0);
            let mut hn = wn * k;
            if wn > 1.0 - x0 {
                wn = 1.0 - x0;
                hn = wn * k;
            }
            if hn > 1.0 - y0 {
                hn = 1.0 - y0;
                wn = hn / k;
            }
            set(x0, y0, wn, hn);
        }
        Drag::CropTL => {
            let mut wn = MIN_CROP.max(x1 - nx);
            let mut hn = wn * k;
            if wn > x1 {
                wn = x1;
                hn = wn * k;
            }
            if hn > y1 {
                hn = y1;
                wn = hn / k;
            }
            set(x1 - wn, y1 - hn, wn, hn);
        }
        Drag::CropTR => {
            let mut wn = MIN_CROP.max(nx - x0);
            let mut hn = wn * k;
            if wn > 1.0 - x0 {
                wn = 1.0 - x0;
                hn = wn * k;
            }
            if hn > y1 {
                hn = y1;
                wn = hn / k;
            }
            set(x0, y1 - hn, wn, hn);
        }
        Drag::CropBL => {
            let mut wn = MIN_CROP.max(x1 - nx);
            let mut hn = wn * k;
            if wn > x1 {
                wn = x1;
                hn = wn * k;
            }
            if hn > 1.0 - y0 {
                hn = 1.0 - y0;
                wn = hn / k;
            }
            set(x1 - wn, y0, wn, hn);
        }
        Drag::CropL | Drag::CropR => {
            let mut wn = if mode == Drag::CropL { MIN_CROP.max(x1.min(x1 - nx)) } else { MIN_CROP.max((1.0 - x0).min(nx - x0)) };
            let mut hn = wn * k;
            if hn > 1.0 {
                hn = 1.0;
                wn = hn / k;
            }
            let ny0 = (cy - hn / 2.0).clamp(0.0, 1.0 - hn);
            let x = if mode == Drag::CropL { x1 - wn } else { x0 };
            set(x, ny0, wn, hn);
        }
        Drag::CropT | Drag::CropB => {
            let mut hn = if mode == Drag::CropT { MIN_CROP.max(y1.min(y1 - ny)) } else { MIN_CROP.max((1.0 - y0).min(ny - y0)) };
            let mut wn = hn / k;
            if wn > 1.0 {
                wn = 1.0;
                hn = wn * k;
            }
            let nx0 = (cx - wn / 2.0).clamp(0.0, 1.0 - wn);
            let y = if mode == Drag::CropT { y1 - hn } else { y0 };
            set(nx0, y, wn, hn);
        }
        _ => {}
    }
}

/// Picking an aspect in the 比例 list: the largest centred box of that ratio (in the
/// pixels of a `proxy_w` × `proxy_h` source). "Original" clears the crop. Returns false
/// when the ratio cannot be parsed (the box is left alone). `MainForm.ApplyCropAspect`.
pub fn apply_crop_aspect(adj: &mut ImageAdjustments, aspect: &str, proxy_w: f64, proxy_h: f64) -> bool {
    if aspect == "Original" {
        (adj.crop_x, adj.crop_y, adj.crop_width, adj.crop_height) = (0.0, 0.0, 1.0, 1.0);
        return true;
    }
    let ratio = parse_aspect(aspect);
    if ratio <= 0.0 || proxy_h <= 0.0 {
        return false;
    }
    let img_ratio = proxy_w / proxy_h;
    let (w, h) = if ratio > img_ratio { (1.0, img_ratio / ratio) } else { (ratio / img_ratio, 1.0) };
    adj.crop_width = w;
    adj.crop_height = h;
    adj.crop_x = (1.0 - w) / 2.0;
    adj.crop_y = (1.0 - h) / 2.0;
    true
}

/// 裁切重設: crop, angle, wide-angle correction, rotation and aspect back to defaults.
pub fn reset_crop_geometry(adj: &mut ImageAdjustments) {
    (adj.crop_x, adj.crop_y, adj.crop_width, adj.crop_height) = (0.0, 0.0, 1.0, 1.0);
    adj.crop_angle = 0.0;
    adj.distortion = 0.0;
    adj.rotation = Rotation::R0;
    adj.crop_aspect_ratio = "Original".to_string();
}

/// The 比例 list entry (0 原始 … 5 自訂) for a stored aspect string.
pub fn aspect_index(aspect: &str) -> usize {
    match aspect {
        "3:2" => 1,
        "4:3" => 2,
        "16:9" => 3,
        "1:1" => 4,
        "Original" => 0,
        a if a.contains(':') => 5,
        _ => 0,
    }
}

// ---- gradients ---------------------------------------------------------------------

/// `ImageAdjustments.ActiveGradient`: the selected gradient, a stale index clamped to the
/// last one; None when there are none.
pub fn active_gradient(adj: &ImageAdjustments, index: &mut i32) -> Option<usize> {
    if adj.gradients.is_empty() {
        return None;
    }
    if *index < 0 || *index as usize >= adj.gradients.len() {
        *index = adj.gradients.len() as i32 - 1;
    }
    Some(*index as usize)
}

/// Axis of variation (screen coordinates, y down).
pub fn grad_axis(g: &LinearGradient) -> (f64, f64) {
    let a = g.angle * std::f64::consts::PI / 180.0;
    (a.sin(), a.cos())
}

/// Blue rotate handle: a fixed screen distance from the white dot along the gradient line.
pub fn rotate_handle_pos(center: P, g: &LinearGradient) -> P {
    let a = g.angle * std::f64::consts::PI / 180.0;
    P::new(center.x + a.cos() * ROT_HANDLE_DIST, center.y - a.sin() * ROT_HANDLE_DIST)
}

/// Yellow range handle: `range` image heights along the axis.
pub fn range_handle_pos(center: P, g: &LinearGradient, v: &View) -> P {
    let (ux, uy) = grad_axis(g);
    let d = g.range * v.h * v.scale;
    P::new(center.x + ux * d, center.y + uy * d)
}

/// Index of the gradient whose white dot is under `p`.
pub fn gradient_at_point(adj: &ImageAdjustments, v: &View, p: P) -> Option<usize> {
    adj.gradients.iter().position(|g| dist(p, v.norm_to_ctrl(g.center_x, g.center_y)) < HANDLE_HIT_RADIUS)
}

/// The active gradient's rotate (tested first) or range handle under `p`.
pub fn gradient_handle_hit(g: &LinearGradient, v: &View, p: P) -> Drag {
    let center = v.norm_to_ctrl(g.center_x, g.center_y);
    if dist(p, rotate_handle_pos(center, g)) < HANDLE_HIT_RADIUS {
        Drag::GradRotate
    } else if dist(p, range_handle_pos(center, g, v)) < HANDLE_HIT_RADIUS {
        Drag::GradRange
    } else {
        Drag::None
    }
}

/// Move one of the active gradient's handles to the pointer `p`.
pub fn drag_gradient(g: &mut LinearGradient, drag: Drag, p: P, v: &View) {
    let center = v.norm_to_ctrl(g.center_x, g.center_y);
    match drag {
        Drag::GradCenter => {
            let (nx, ny) = v.ctrl_to_norm(p);
            g.center_x = nx;
            g.center_y = ny;
        }
        Drag::GradRange => g.range = (dist(center, p) / (v.h * v.scale)).clamp(0.02, 1.0),
        Drag::GradRotate => g.angle = (-(p.y - center.y)).atan2(p.x - center.x) * 180.0 / std::f64::consts::PI,
        _ => {}
    }
}

// ---- masks ---------------------------------------------------------------------------

/// The rotate handle sits this far (points) beyond the radial's X-radius handle.
pub const MASK_ROT_OFFSET: f64 = 40.0;

/// The selected mask, a stale index clamped to the last one; None when there are none.
pub fn active_mask(adj: &ImageAdjustments, index: &mut i32) -> Option<usize> {
    if adj.masks.is_empty() {
        return None;
    }
    if *index < 0 || *index as usize >= adj.masks.len() {
        *index = adj.masks.len() as i32 - 1;
    }
    Some(*index as usize)
}

/// Screen points per unit of the image's long edge (mask radii are in those units).
pub fn long_px(v: &View) -> f64 {
    v.w.max(v.h) * v.scale
}

/// A radial's axes on screen: X-radius direction, Y-radius direction (unit vectors).
fn radial_axes(m: &LocalMask) -> ((f64, f64), (f64, f64)) {
    let (s, c) = m.angle.to_radians().sin_cos();
    ((c, s), (-s, c))
}

/// Centre, X-radius, Y-radius and rotate handle positions of a radial mask.
pub fn radial_handles(m: &LocalMask, v: &View) -> [P; 4] {
    let c = v.norm_to_ctrl(m.center_x, m.center_y);
    let ((ax, ay), (bx, by)) = radial_axes(m);
    let (rx, ry) = (m.radius_x * long_px(v), m.radius_y * long_px(v));
    [
        c,
        P::new(c.x + ax * rx, c.y + ay * rx),
        P::new(c.x + bx * ry, c.y + by * ry),
        P::new(c.x + ax * (rx + MASK_ROT_OFFSET), c.y + ay * (rx + MASK_ROT_OFFSET)),
    ]
}

/// The radial handle under `p` (rotate, then the radii, then the centre).
pub fn radial_hit(m: &LocalMask, v: &View, p: P) -> Drag {
    let [c, hx, hy, rot] = radial_handles(m, v);
    if dist(p, rot) < HANDLE_HIT_RADIUS {
        Drag::MaskRotate
    } else if dist(p, hx) < HANDLE_HIT_RADIUS {
        Drag::MaskRadiusX
    } else if dist(p, hy) < HANDLE_HIT_RADIUS {
        Drag::MaskRadiusY
    } else if dist(p, c) < HANDLE_HIT_RADIUS {
        Drag::MaskCenter
    } else {
        Drag::None
    }
}

/// The radial mask whose centre is under `p`.
pub fn radial_at_point(adj: &ImageAdjustments, v: &View, p: P) -> Option<usize> {
    adj.masks.iter().position(|m| m.kind == awpr_core::MaskKind::Radial && dist(p, v.norm_to_ctrl(m.center_x, m.center_y)) < HANDLE_HIT_RADIUS)
}

/// Move a radial handle to the pointer.
pub fn drag_radial(m: &mut LocalMask, drag: Drag, p: P, v: &View) {
    let c = v.norm_to_ctrl(m.center_x, m.center_y);
    let ((ax, ay), (bx, by)) = radial_axes(m);
    let (dx, dy) = (p.x - c.x, p.y - c.y);
    match drag {
        Drag::MaskCenter => {
            let (nx, ny) = v.ctrl_to_norm(p);
            (m.center_x, m.center_y) = (nx, ny);
        }
        Drag::MaskRadiusX => m.radius_x = ((dx * ax + dy * ay).abs() / long_px(v)).clamp(0.005, 2.0),
        Drag::MaskRadiusY => m.radius_y = ((dx * bx + dy * by).abs() / long_px(v)).clamp(0.005, 2.0),
        Drag::MaskRotate => m.angle = dy.atan2(dx).to_degrees(),
        _ => {}
    }
}

/// Points of a radial's outline at `scale` × its radii, on screen.
pub fn radial_outline(m: &LocalMask, v: &View, scale: f64, n: usize) -> Vec<P> {
    let c = v.norm_to_ctrl(m.center_x, m.center_y);
    let ((ax, ay), (bx, by)) = radial_axes(m);
    let (rx, ry) = (m.radius_x * long_px(v) * scale, m.radius_y * long_px(v) * scale);
    (0..=n)
        .map(|i| {
            let t = i as f64 / n as f64 * std::f64::consts::TAU;
            let (u, w) = (t.cos() * rx, t.sin() * ry);
            P::new(c.x + ax * u + bx * w, c.y + ay * u + by * w)
        })
        .collect()
}

// ---- heal --------------------------------------------------------------------------

/// A spot's radius on screen.
pub fn heal_radius_px(s: &HealSpot, v: &View) -> f64 {
    s.radius_norm * v.w.max(v.h) * v.scale
}

/// The spot circle under `p` (target first, then a clone spot's source).
pub fn heal_hit(adj: &ImageAdjustments, v: &View, p: P) -> Option<(usize, Drag)> {
    for (i, s) in adj.heal_spots.iter().enumerate() {
        let r = HEAL_MIN_HIT.max(heal_radius_px(s, v));
        if dist(p, v.norm_to_ctrl(s.target_x, s.target_y)) < r {
            return Some((i, Drag::HealTarget));
        }
        if !s.use_inpaint && dist(p, v.norm_to_ctrl(s.source_x, s.source_y)) < r {
            return Some((i, Drag::HealSource));
        }
    }
    None
}

/// The target circle under `p` (right-click delete only looks at targets).
pub fn heal_target_at(adj: &ImageAdjustments, v: &View, p: P) -> Option<usize> {
    adj.heal_spots
        .iter()
        .position(|s| dist(p, v.norm_to_ctrl(s.target_x, s.target_y)) < HEAL_MIN_HIT.max(heal_radius_px(s, v)))
}

/// 大小 (0–50) → radius as a fraction of the larger image side.
pub fn heal_radius_norm(size: f64) -> f64 {
    0.005f64.max(size / 500.0)
}

/// A new spot at (nx, ny); a clone spot samples 6% of the width to the left.
pub fn new_heal_spot(nx: f64, ny: f64, size: f64, mode: HealMode) -> HealSpot {
    HealSpot {
        target_x: nx,
        target_y: ny,
        source_x: (nx - 0.06).clamp(0.0, 1.0),
        source_y: ny,
        radius: size,
        radius_norm: heal_radius_norm(size),
        use_inpaint: mode == HealMode::Inpaint,
    }
}

/// Resize a spot to a new 大小; false when it already has that size.
pub fn resize_heal_spot(s: &mut HealSpot, size: f64) -> bool {
    let rn = heal_radius_norm(size);
    if (s.radius_norm - rn).abs() < 1e-9 {
        return false;
    }
    s.radius_norm = rn;
    s.radius = size;
    true
}

// ---- white balance picker ----------------------------------------------------------

/// The 色溫 range a non-RAW photo's ±100 slider can show (5200 ± 3000 K).
pub fn clamp_temp_for_current(kelvin: f64, is_raw: bool) -> f64 {
    if is_raw {
        kelvin
    } else {
        kelvin.clamp(5200.0 - 100.0 * 30.0, 5200.0 + 100.0 * 30.0)
    }
}

/// Temperature / tint that neutralise a sampled proxy colour (display-encoded, 0..1).
/// New pipeline + camera data → exact camera-space solve; otherwise a black-body search on
/// linearised (v1) or encoded (legacy) values. `MainForm.EstimateWhiteBalance`.
pub fn estimate_white_balance(r: f32, g: f32, b: f32, legacy: bool, camera: Option<&CameraColorInfo>) -> (f64, f64) {
    if r <= 1e-4 && g <= 1e-4 && b <= 1e-4 {
        return (5200.0, 0.0);
    }
    let lut = color::decode_lut();
    if !legacy {
        if let Some(cam) = camera.filter(|c| c.is_valid()) {
            let (lr, lg, lb) = (color::linearize(lut, r) as f64, color::linearize(lut, g) as f64, color::linearize(lut, b) as f64);
            if let Some(kt) = color::neutralizing_cam_mul(cam, lr, lg, lb, WhiteBalanceReference::Decode).and_then(|m| color::cam_mul_to_kelvin_tint(cam, &m)) {
                return (kt.0.clamp(color::MIN_KELVIN, color::MAX_KELVIN), kt.1);
            }
        }
    }
    let (r, g, b) = if legacy { (r, g, b) } else { (color::linearize(lut, r), color::linearize(lut, g), color::linearize(lut, b)) };
    let (mut best_t, mut best_err) = (5200.0, f64::MAX);
    let mut t = 2000.0;
    while t <= 12000.0 {
        let (mr, _, mb) = white_balance_multipliers(t, 0.0);
        let err = (r as f64 * mr - b as f64 * mb).abs();
        if err < best_err {
            best_err = err;
            best_t = t;
        }
        t += 100.0;
    }
    let (mr2, mg2, mb2) = white_balance_multipliers(best_t, 0.0);
    let gg = g as f64 * mg2;
    let avg = (r as f64 * mr2 + b as f64 * mb2) / 2.0;
    // More green → a negative tint compensates.
    let tint = ((gg - avg) * 300.0).clamp(-100.0, 100.0);
    (best_t, -tint)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> View {
        // A 3000 × 2000 photo shown at 0.25 points per pixel from (100, 50).
        View { ox: 100.0, oy: 50.0, scale: 0.25, w: 3000.0, h: 2000.0 }
    }

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn crop_hit_corners_win_over_edges() {
        let r = Box4 { left: 100.0, top: 100.0, right: 400.0, bottom: 300.0 };
        // 15 points from both the left and the top edge: inside the corner zone.
        assert_eq!(crop_hit_test(&r, P::new(115.0, 115.0)), Drag::CropTL);
        assert_eq!(crop_hit_test(&r, P::new(388.0, 290.0)), Drag::CropBR);
        assert_eq!(crop_hit_test(&r, P::new(385.0, 112.0)), Drag::CropTR);
        assert_eq!(crop_hit_test(&r, P::new(95.0, 285.0)), Drag::CropBL);
        // Edges only within 10 points and away from the corners.
        assert_eq!(crop_hit_test(&r, P::new(105.0, 200.0)), Drag::CropL);
        assert_eq!(crop_hit_test(&r, P::new(395.0, 200.0)), Drag::CropR);
        assert_eq!(crop_hit_test(&r, P::new(250.0, 92.0)), Drag::CropT);
        assert_eq!(crop_hit_test(&r, P::new(250.0, 308.0)), Drag::CropB);
        assert_eq!(crop_hit_test(&r, P::new(250.0, 200.0)), Drag::CropMove);
        assert_eq!(crop_hit_test(&r, P::new(250.0, 320.0)), Drag::None);
    }

    #[test]
    fn crop_free_edges_keep_minimum() {
        let mut a = ImageAdjustments { crop_aspect_ratio: "Custom".into(), ..Default::default() };
        // "Custom" is not a W:H pair → free resize.
        drag_crop_handle(&mut a, Drag::CropTL, 0.2, 0.1, 3000.0, 2000.0);
        assert!(approx(a.crop_x, 0.2) && approx(a.crop_y, 0.1) && approx(a.crop_width, 0.8) && approx(a.crop_height, 0.9));
        // Dragging the right edge past the left one stops 5% from it.
        drag_crop_handle(&mut a, Drag::CropR, 0.0, 0.5, 3000.0, 2000.0);
        assert!(approx(a.crop_x, 0.2) && approx(a.crop_width, MIN_CROP));
    }

    #[test]
    fn crop_ratio_locked_corner_and_edge() {
        // 1:1 on a 3:2 photo: normalized height = width * 1.5.
        let mut a = ImageAdjustments { crop_aspect_ratio: "1:1".into(), ..Default::default() };
        drag_crop_handle(&mut a, Drag::CropBR, 0.4, 0.9, 3000.0, 2000.0);
        assert!(approx(a.crop_width, 0.4) && approx(a.crop_height, 0.6));
        assert!(approx(a.crop_width * 3000.0, a.crop_height * 2000.0)); // square in pixels
        // Too wide to fit vertically: the height clamps and the width follows.
        drag_crop_handle(&mut a, Drag::CropBR, 0.9, 0.9, 3000.0, 2000.0);
        assert!(approx(a.crop_height, 1.0) && approx(a.crop_width, 1.0 / 1.5));
        // Edge: the other side grows around the centre.
        let mut b = ImageAdjustments { crop_aspect_ratio: "1:1".into(), crop_x: 0.25, crop_y: 0.25, crop_width: 0.2, crop_height: 0.3, ..Default::default() };
        drag_crop_handle(&mut b, Drag::CropR, 0.65, 0.5, 3000.0, 2000.0);
        assert!(approx(b.crop_x, 0.25) && approx(b.crop_width, 0.4) && approx(b.crop_height, 0.6));
        assert!(approx(b.crop_y + b.crop_height / 2.0, 0.4)); // centre kept
        // "Original" locks to the photo's own proportions.
        let mut c = ImageAdjustments::default();
        drag_crop_handle(&mut c, Drag::CropTL, 0.5, 0.0, 3000.0, 2000.0);
        assert!(approx(c.crop_x, 0.5) && approx(c.crop_y, 0.5) && approx(c.crop_width, 0.5) && approx(c.crop_height, 0.5));
    }

    #[test]
    fn crop_move_and_aspect_pick() {
        let mut a = ImageAdjustments { crop_x: 0.1, crop_y: 0.1, crop_width: 0.5, crop_height: 0.5, ..Default::default() };
        move_crop(&mut a, [0.1, 0.1, 0.5, 0.5], 0.7, -0.3);
        assert!(approx(a.crop_x, 0.5) && approx(a.crop_y, 0.0));
        // 16:9 on a 3:2 source: full width, height 1.5 / 1.777…
        assert!(apply_crop_aspect(&mut a, "16:9", 3000.0, 2000.0));
        assert!(approx(a.crop_width, 1.0) && approx(a.crop_height, 1.5 / (16.0 / 9.0)));
        assert!(approx(a.crop_y, (1.0 - a.crop_height) / 2.0));
        // 1:1: full height, centred.
        assert!(apply_crop_aspect(&mut a, "1:1", 3000.0, 2000.0));
        assert!(approx(a.crop_width, 2.0 / 3.0) && approx(a.crop_height, 1.0) && approx(a.crop_x, 1.0 / 6.0));
        assert!(!apply_crop_aspect(&mut a, "Custom", 3000.0, 2000.0));
        assert!(apply_crop_aspect(&mut a, "Original", 3000.0, 2000.0));
        assert!(approx(a.crop_width, 1.0) && approx(a.crop_x, 0.0));
        assert_eq!(aspect_index("4:3"), 2);
        assert_eq!(aspect_index("5:4"), 5);
        assert_eq!(aspect_index("Original"), 0);
    }

    #[test]
    fn crop_angle_slider_is_negated() {
        // The 角度 slider shows −CropAngle; the stored value keeps its meaning.
        let mut a = ImageAdjustments::default();
        let ui_value = 12.5;
        a.crop_angle = -ui_value;
        assert_eq!(-a.crop_angle, 12.5);
    }

    #[test]
    fn gradient_handles() {
        let v = view();
        let g = LinearGradient::default(); // centre (0.5, 0.15), angle 0, range 0.25
        let c = v.norm_to_ctrl(g.center_x, g.center_y);
        assert!(approx(c.x, 100.0 + 1500.0 * 0.25) && approx(c.y, 50.0 + 300.0 * 0.25));
        // Angle 0: the axis points down, the line runs horizontally, rotate handle to the right.
        let rot = rotate_handle_pos(c, &g);
        assert!(approx(rot.x, c.x + 256.0) && approx(rot.y, c.y));
        let range = range_handle_pos(c, &g, &v);
        assert!(approx(range.x, c.x) && approx(range.y, c.y + 0.25 * 2000.0 * 0.25));
        assert_eq!(gradient_handle_hit(&g, &v, P::new(rot.x + 10.0, rot.y - 10.0)), Drag::GradRotate);
        assert_eq!(gradient_handle_hit(&g, &v, P::new(range.x, range.y + 20.0)), Drag::GradRange);
        assert_eq!(gradient_handle_hit(&g, &v, c), Drag::None);

        let mut adj = ImageAdjustments::default();
        adj.gradients.push(g.clone());
        adj.gradients.push(LinearGradient { center_y: 0.8, ..Default::default() });
        assert_eq!(gradient_at_point(&adj, &v, P::new(c.x + 5.0, c.y)), Some(0));
        assert_eq!(gradient_at_point(&adj, &v, v.norm_to_ctrl(0.5, 0.8)), Some(1));
        let mut idx = 7;
        assert_eq!(active_gradient(&adj, &mut idx), Some(1)); // stale index → last
        assert_eq!(idx, 1);

        // Rotating: pointer straight above the centre = 90°; range follows the distance.
        let mut g2 = g.clone();
        drag_gradient(&mut g2, Drag::GradRotate, P::new(c.x, c.y - 50.0), &v);
        assert!(approx(g2.angle, 90.0));
        drag_gradient(&mut g2, Drag::GradRange, P::new(c.x, c.y + 50.0), &v);
        assert!(approx(g2.range, 50.0 / 500.0));
        drag_gradient(&mut g2, Drag::GradRange, P::new(c.x, c.y + 1.0), &v);
        assert!(approx(g2.range, 0.02));
        drag_gradient(&mut g2, Drag::GradCenter, P::new(0.0, 2000.0), &v);
        assert!(approx(g2.center_x, 0.0) && approx(g2.center_y, 1.0));
    }

    #[test]
    fn radial_handles_follow_the_shape() {
        let v = view();
        let mut m = LocalMask { center_x: 0.5, center_y: 0.5, radius_x: 0.2, radius_y: 0.1, angle: 0.0, ..Default::default() };
        let [c, hx, hy, rot] = radial_handles(&m, &v);
        let lp = long_px(&v);
        assert!(approx(hx.x - c.x, 0.2 * lp) && approx(hx.y, c.y));
        assert!(approx(hy.y - c.y, 0.1 * lp) && approx(hy.x, c.x));
        assert!(approx(rot.x - hx.x, MASK_ROT_OFFSET));
        assert_eq!(radial_hit(&m, &v, hx), Drag::MaskRadiusX);
        assert_eq!(radial_hit(&m, &v, rot), Drag::MaskRotate);
        assert_eq!(radial_hit(&m, &v, c), Drag::MaskCenter);
        // Dragging the X handle out doubles the radius; rotating puts the axis on the pointer.
        drag_radial(&mut m, Drag::MaskRadiusX, P::new(c.x + 0.4 * lp, c.y), &v);
        assert!(approx(m.radius_x, 0.4));
        drag_radial(&mut m, Drag::MaskRotate, P::new(c.x, c.y + 100.0), &v);
        assert!(approx(m.angle, 90.0));
        let [_, hx2, _, _] = radial_handles(&m, &v);
        assert!(approx(hx2.x, c.x) && hx2.y > c.y);
        drag_radial(&mut m, Drag::MaskCenter, v.norm_to_ctrl(0.25, 0.75), &v);
        assert!(approx(m.center_x, 0.25) && approx(m.center_y, 0.75));
    }

    #[test]
    fn heal_spots() {
        let v = view();
        let s = new_heal_spot(0.5, 0.5, 10.0, HealMode::Clone);
        assert!(approx(s.radius_norm, 0.02) && approx(s.source_x, 0.44) && !s.use_inpaint);
        assert!(approx(new_heal_spot(0.03, 0.5, 0.0, HealMode::Inpaint).radius_norm, 0.005));
        assert_eq!(new_heal_spot(0.03, 0.5, 0.0, HealMode::Inpaint).source_x, 0.0);
        // 0.02 × 3000 px × 0.25 = 15 points.
        assert!(approx(heal_radius_px(&s, &v), 15.0));
        let mut adj = ImageAdjustments::default();
        adj.heal_spots.push(s.clone());
        let t = v.norm_to_ctrl(0.5, 0.5);
        let src = v.norm_to_ctrl(0.44, 0.5);
        assert_eq!(heal_hit(&adj, &v, P::new(t.x + 14.0, t.y)), Some((0, Drag::HealTarget)));
        assert_eq!(heal_hit(&adj, &v, P::new(src.x - 14.0, src.y)), Some((0, Drag::HealSource)));
        assert_eq!(heal_hit(&adj, &v, P::new(t.x, t.y + 40.0)), None);
        // 修補 spots have no source circle.
        adj.heal_spots[0].use_inpaint = true;
        assert_eq!(heal_hit(&adj, &v, P::new(src.x - 14.0, src.y)), None);
        assert_eq!(heal_target_at(&adj, &v, t), Some(0));
        let mut s2 = s.clone();
        assert!(resize_heal_spot(&mut s2, 25.0));
        assert!(approx(s2.radius_norm, 0.05) && s2.radius == 25.0);
        assert!(!resize_heal_spot(&mut s2, 25.0));
    }

    #[test]
    fn wb_picker_black_body() {
        // Neutral grey stays at the 5200 K / 0 neutral point (both pipelines).
        assert_eq!(estimate_white_balance(0.5, 0.5, 0.5, true, None), (5200.0, 0.0));
        assert_eq!(estimate_white_balance(0.5, 0.5, 0.5, false, None), (5200.0, 0.0));
        // Black: no information.
        assert_eq!(estimate_white_balance(0.0, 0.0, 0.0, false, None), (5200.0, 0.0));
        // A grey seen under 3000 K light (legacy: multipliers act on the encoded values).
        let (mr, mg, mb) = white_balance_multipliers(3000.0, 0.0);
        let (r, g, b) = ((0.4 / mr) as f32, (0.4 / mg) as f32, (0.4 / mb) as f32);
        let (t, tint) = estimate_white_balance(r, g, b, true, None);
        assert_eq!(t, 3000.0);
        assert!(tint.abs() < 1e-3, "{tint}");
        // Green cast: (0.6 − 0.5) × 300 = 30 → tint −30 at 5200 K.
        let (t, tint) = estimate_white_balance(0.5, 0.6, 0.5, true, None);
        assert_eq!(t, 5200.0);
        assert!((tint + 30.0).abs() < 1e-4, "{tint}");
        // Clamped to ±100.
        assert_eq!(estimate_white_balance(0.1, 0.9, 0.1, true, None).1, -100.0);
        // Non-RAW photos keep the value inside their ±100 slider scale.
        assert_eq!(clamp_temp_for_current(12000.0, false), 8200.0);
        assert_eq!(clamp_temp_for_current(12000.0, true), 12000.0);
    }

    #[test]
    fn wb_picker_camera_matrix() {
        let cam = CameraColorInfo {
            pre_mul: [2.1792, 1.0, 1.2902],
            cam_mul: [1.9463, 1.0, 1.5488],
            rgb_cam: [1.7, -0.6, -0.1, -0.2, 1.5, -0.3, 0.05, -0.45, 1.4],
        };
        // A neutral patch in the decoded (pre_mul-balanced) proxy → the pre_mul illuminant.
        let (k, t) = estimate_white_balance(0.5, 0.5, 0.5, false, Some(&cam));
        let (ek, et) = color::cam_mul_to_kelvin_tint(&cam, &cam.pre_mul).unwrap();
        assert!((k - ek.clamp(2000.0, 12000.0)).abs() < 1e-6 && (t - et).abs() < 1e-6);

        // A grey lit by 3800 K / +12 tint, as the decode renders it, is solved back to that light.
        let target = color::kelvin_tint_to_cam_mul(&cam, 3800.0, 12.0).unwrap();
        let inv = color::invert3(&cam.rgb_cam).unwrap();
        let white = [inv[0] + inv[1] + inv[2], inv[3] + inv[4] + inv[5], inv[6] + inv[7] + inv[8]];
        // neutralizing_cam_mul: mul = pre_mul · white / patch → patch = pre_mul · white / target.
        let patch_cam: Vec<f64> = (0..3).map(|i| cam.pre_mul[i] * white[i] / target[i] * 0.3).collect();
        let lin: Vec<f64> = (0..3).map(|r| (0..3).map(|c| cam.rgb_cam[r * 3 + c] * patch_cam[c]).sum()).collect();
        let enc: Vec<f32> = lin.iter().map(|&l| color::encode_exact(l) as f32).collect();
        let (k, t) = estimate_white_balance(enc[0], enc[1], enc[2], false, Some(&cam));
        // The encode → LUT linearise round trip costs a little precision.
        assert!((k - 3800.0).abs() < 25.0, "{k}");
        assert!((t - 12.0).abs() < 1.5, "{t}");
        // Legacy photos ignore the camera data and use the black-body search.
        assert_eq!(estimate_white_balance(0.5, 0.5, 0.5, true, Some(&cam)), (5200.0, 0.0));
    }
}
