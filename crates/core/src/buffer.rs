//! RGBA float image, row-major, nominally 0..1 (may exceed during processing).

use rayon::prelude::*;

#[derive(Clone)]
pub struct FloatImage {
    pub width: usize,
    pub height: usize,
    /// Length = width * height * 4, as R,G,B,A.
    pub data: Vec<f32>,
}

impl FloatImage {
    pub fn new(width: usize, height: usize) -> Self {
        assert!(width > 0 && height > 0, "Invalid image size");
        Self { width, height, data: vec![0.0; width * height * 4] }
    }

    #[inline(always)]
    pub fn index(&self, x: usize, y: usize) -> usize {
        (y * self.width + x) * 4
    }

    /// Runs `body(y, row)` for every row across all cores. Rows are independent, so the
    /// result does not depend on how rayon splits the work.
    pub fn par_rows_mut<F>(&mut self, body: F)
    where
        F: Fn(usize, &mut [f32]) + Sync + Send,
    {
        let stride = self.width * 4;
        self.data
            .par_chunks_mut(stride)
            .enumerate()
            .for_each(|(y, row)| body(y, row));
    }
}

/// .NET `Math.Round(double)`: banker's rounding. Every site ported from a C#
/// `Math.Round` uses this; `(int)(v + 0.5)` sites stay as truncation.
#[inline(always)]
pub fn round_half_even(v: f64) -> f64 {
    v.round_ties_even()
}

#[inline(always)]
pub fn round_half_even_f32(v: f32) -> f32 {
    v.round_ties_even()
}
