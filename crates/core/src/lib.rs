//! AwayPhotoRawEditor engine: models, colour science and the CPU pipeline.
//!
//! The CPU code is a port of the Swift `AwayRawCore`, which is itself verified
//! pixel-for-pixel against the C# reference (`hashtest`). Changing any maths here means
//! changing it in all implementations, or the photos stop matching.

pub mod buffer;
pub mod color;
pub mod libraw;
pub mod model;
pub mod pipeline;
pub mod resize;
pub mod tone;
pub mod v3;

pub use buffer::FloatImage;
pub use model::{CameraColorInfo, HealSpot, ImageAdjustments, LinearGradient, Rotation};
pub use pipeline::{apply_to_float, ProcessContext, SourceKind};
