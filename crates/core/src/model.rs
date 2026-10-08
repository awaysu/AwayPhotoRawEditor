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
    /// 0 = legacy maths, 1 = linear light + camera-matrix white balance.
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
}

impl ImageAdjustments {
    pub const CURRENT_PIPELINE_VERSION: i32 = 1;

    pub fn is_legacy_pipeline(&self) -> bool {
        self.pipeline_version < 1
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
