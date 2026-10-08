//! Edit description types. Field names, defaults and ranges follow
//! `AwayRawCore/Models/*.swift` (which follow the C# classes), because they are what the
//! shared `RAW_TEMP/*.rawpipe.xml` files carry.

/// Discrete image rotation (degrees clockwise).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Rotation {
    #[default]
    R0,
    R90,
    R180,
    R270,
}

/// A single linear (graduated) filter. Geometry is in normalized image coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct LinearGradient {
    pub center_x: f64,
    pub center_y: f64,
    /// Degrees.
    pub angle: f64,
    pub range: f64,
    pub exposure: f64,
    pub contrast: f64,
    pub highlights: f64,
    pub shadows: f64,
    pub saturation: f64,
}

impl Default for LinearGradient {
    fn default() -> Self {
        Self {
            center_x: 0.5,
            center_y: 0.15,
            angle: 0.0,
            range: 0.25,
            exposure: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            saturation: 0.0,
        }
    }
}

impl LinearGradient {
    /// True when this gradient actually changes any pixels.
    pub fn has_effect(&self) -> bool {
        self.exposure != 0.0
            || self.contrast != 0.0
            || self.highlights != 0.0
            || self.shadows != 0.0
            || self.saturation != 0.0
    }
}

/// 遮罩 kinds (處理版本 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MaskKind {
    /// 放射狀: an ellipse, effect inside (outside when inverted).
    #[default]
    Radial,
    /// 筆刷: painted strokes.
    Brush,
}

/// One painted stroke. Points in normalized image coordinates (pre-crop / pre-rotation,
/// like gradients); the brush settings are the ones it was painted with.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BrushStroke {
    /// Brush radius as a fraction of the image's long edge.
    pub radius: f64,
    /// 0..100: soft edge as a share of the radius.
    pub feather: f64,
    /// 0..100: opacity a stroke lays down.
    pub flow: f64,
    /// 擦除: removes from the mask instead of adding.
    pub erase: bool,
    pub points: Vec<(f64, f64)>,
}

/// A local adjustment through a mask (處理版本 3). The adjustment set is the linear
/// gradient's.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalMask {
    pub kind: MaskKind,
    pub exposure: f64,
    pub contrast: f64,
    pub highlights: f64,
    pub shadows: f64,
    pub saturation: f64,
    /// 反轉: the effect where the mask is not.
    pub invert: bool,
    /// Radial: 0..100, the soft share of the ellipse's radius.
    pub feather: f64,
    /// Radial geometry. Centre in normalized image coordinates; radii as fractions of the
    /// long edge (so a circle stays round whatever the aspect); angle in degrees.
    pub center_x: f64,
    pub center_y: f64,
    pub radius_x: f64,
    pub radius_y: f64,
    pub angle: f64,
    /// Brush settings for the next stroke (size as a fraction of the long edge).
    pub brush_size: f64,
    pub brush_feather: f64,
    pub brush_flow: f64,
    pub strokes: Vec<BrushStroke>,
}

impl Default for LocalMask {
    fn default() -> Self {
        Self {
            kind: MaskKind::Radial,
            exposure: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            saturation: 0.0,
            invert: false,
            feather: 50.0,
            center_x: 0.5,
            center_y: 0.5,
            radius_x: 0.25,
            radius_y: 0.18,
            angle: 0.0,
            brush_size: 0.04,
            brush_feather: 50.0,
            brush_flow: 100.0,
            strokes: Vec::new(),
        }
    }
}

impl LocalMask {
    /// True when this mask changes any pixels.
    pub fn has_effect(&self) -> bool {
        let adjusts = self.exposure != 0.0 || self.contrast != 0.0 || self.highlights != 0.0 || self.shadows != 0.0 || self.saturation != 0.0;
        adjusts && (self.kind == MaskKind::Radial || self.invert || self.strokes.iter().any(|s| !s.points.is_empty()))
    }
}

/// A single heal / clone point, in normalized image coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct HealSpot {
    pub target_x: f64,
    pub target_y: f64,
    pub source_x: f64,
    pub source_y: f64,
    /// Pixels at the resolution it was authored on (informational).
    pub radius: f64,
    /// Fraction of the image's larger dimension.
    pub radius_norm: f64,
    pub use_inpaint: bool,
}

impl Default for HealSpot {
    fn default() -> Self {
        Self {
            target_x: 0.0,
            target_y: 0.0,
            source_x: 0.0,
            source_y: 0.0,
            radius: 10.0,
            radius_norm: 0.02,
            use_inpaint: false,
        }
    }
}

/// The complete non-destructive edit description for one photo.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageAdjustments {
    /// 0 = legacy maths, 1 = linear light + camera-matrix white balance, 2 = the wide-gamut
    /// linear pipeline (`v3.rs`; shown to the user as 處理版本 1 / 2 / 3).
    pub pipeline_version: i32,

    pub exposure: f64,
    pub contrast: f64,
    pub highlights: f64,
    pub shadows: f64,
    pub whites: f64,
    pub blacks: f64,

    pub temperature: f64,
    pub tint: f64,
    pub vibrance: f64,
    pub saturation: f64,

    pub sharpening: f64,
    pub noise_reduction: f64,
    pub vignette: f64,

    pub distortion: f64,

    pub crop_aspect_ratio: String,
    pub crop_angle: f64,
    pub crop_x: f64,
    pub crop_y: f64,
    pub crop_width: f64,
    pub crop_height: f64,
    pub rotation: Rotation,

    pub gradients: Vec<LinearGradient>,

    pub heal_size: f64,
    pub heal_spots: Vec<HealSpot>,

    // ---- pipeline version 2 (處理版本 3) only ----
    /// 高光復原 0..100.
    pub highlight_recovery: f64,
    /// HSL per colour band (紅 橙 黃 綠 青 藍 紫 洋紅), each −100..100.
    pub hsl_hue: [f64; 8],
    pub hsl_saturation: [f64; 8],
    pub hsl_luminance: [f64; 8],
    /// Point curves in 0..1 × 0..1 (encoded values); empty = identity.
    pub curve_rgb: Vec<(f64, f64)>,
    pub curve_red: Vec<(f64, f64)>,
    pub curve_green: Vec<(f64, f64)>,
    pub curve_blue: Vec<(f64, f64)>,
    /// 遮罩: radial and brush local adjustments.
    pub masks: Vec<LocalMask>,
}

impl ImageAdjustments {
    pub const CURRENT_PIPELINE_VERSION: i32 = 2;
    /// The first version with the wide-gamut pipeline (處理版本 3).
    pub const V3_PIPELINE_VERSION: i32 = 2;

    pub fn is_legacy_pipeline(&self) -> bool {
        self.pipeline_version < 1
    }

    /// 處理版本 3: highlight recovery, HSL and curves exist only here.
    pub fn is_v3(&self) -> bool {
        self.pipeline_version >= Self::V3_PIPELINE_VERSION
    }

    /// True when any version-3-only value differs from its default.
    pub fn has_v3_values(&self) -> bool {
        self.highlight_recovery != 0.0
            || self.hsl_hue.iter().chain(&self.hsl_saturation).chain(&self.hsl_luminance).any(|&v| v != 0.0)
            || [&self.curve_rgb, &self.curve_red, &self.curve_green, &self.curve_blue].iter().any(|c| !crate::v3::curve_is_identity(c))
            || !self.masks.is_empty()
    }

    /// True when at least one mask changes pixels (and the version has masks).
    pub fn has_active_mask(&self) -> bool {
        self.is_v3() && self.masks.iter().any(LocalMask::has_effect)
    }

    /// True when at least one gradient actually changes pixels.
    pub fn has_active_gradient(&self) -> bool {
        self.gradients.iter().any(LinearGradient::has_effect)
    }
}

impl Default for ImageAdjustments {
    fn default() -> Self {
        Self {
            pipeline_version: Self::CURRENT_PIPELINE_VERSION,
            exposure: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
            temperature: 5200.0,
            tint: 0.0,
            vibrance: 0.0,
            saturation: 0.0,
            sharpening: 0.0,
            noise_reduction: 0.0,
            vignette: 0.0,
            distortion: 0.0,
            crop_aspect_ratio: "Original".to_string(),
            crop_angle: 0.0,
            crop_x: 0.0,
            crop_y: 0.0,
            crop_width: 1.0,
            crop_height: 1.0,
            rotation: Rotation::R0,
            gradients: Vec::new(),
            heal_size: 10.0,
            heal_spots: Vec::new(),
            highlight_recovery: 0.0,
            hsl_hue: [0.0; 8],
            hsl_saturation: [0.0; 8],
            hsl_luminance: [0.0; 8],
            curve_rgb: Vec::new(),
            curve_red: Vec::new(),
            curve_green: Vec::new(),
            curve_blue: Vec::new(),
            masks: Vec::new(),
        }
    }
}

/// The camera's colour data as LibRaw reports it. Multipliers are normalised to G = 1.
#[derive(Debug, Clone, PartialEq)]
pub struct CameraColorInfo {
    /// LibRaw `pre_mul`: the daylight multipliers its default decode balances to.
    pub pre_mul: [f64; 3],
    /// LibRaw `cam_mul`: the as-shot multipliers the camera recorded.
    pub cam_mul: [f64; 3],
    /// LibRaw `rgb_cam` (3×3, row-major).
    pub rgb_cam: [f64; 9],
}

impl CameraColorInfo {
    pub fn is_valid(&self) -> bool {
        let positive = |a: &[f64]| a.iter().all(|&v| v > 0.0 && !v.is_infinite());
        positive(&self.pre_mul)
            && positive(&self.cam_mul)
            && self.rgb_cam.iter().all(|v| v.is_finite())
    }
}
