//! Photo metadata for the info panel and the XML cache, formatted exactly as the native
//! builds format it (`ExifReader.swift`, which mirrors the strings ExifTool gives the
//! Windows build).

use crate::paths;
use crate::store::ExifData;
use crate::tiff::{self, TiffMeta};
use awpr_core::libraw;

/// Read everything the panel shows. Never fails: missing fields stay empty.
pub fn read(path: &str) -> ExifData {
    let mut e = ExifData { file_path: path.to_string(), ..Default::default() };
    e.file_size = std::fs::metadata(path).map(|m| m.len() as i64).unwrap_or(0);

    let t = tiff::read(path).unwrap_or_default();
    // LibRaw fills what the TIFF reader cannot see (makernote-only fields, odd containers).
    let raw = if paths::is_raw(path) { libraw::read_meta(path) } else { None };

    e.camera_make = t.make.clone().or_else(|| raw.as_ref().map(|r| r.make.clone())).unwrap_or_default();
    e.camera_model = t.model.clone().or_else(|| raw.as_ref().map(|r| r.model.clone())).unwrap_or_default();
    e.lens = first_non_empty(&[
        t.lens_model.clone().unwrap_or_default(),
        raw.as_ref().map(|r| r.lens.clone()).unwrap_or_default(),
        t.lens_make.clone().unwrap_or_default(),
    ]);

    let iso = t.iso.map(|v| v as f64).or_else(|| raw.as_ref().map(|r| r.iso_speed as f64)).unwrap_or(0.0);
    e.iso = if iso > 0.0 { format!("{}", iso.round() as i64) } else { String::new() };

    let fnum = t.f_number.or_else(|| raw.as_ref().map(|r| r.aperture as f64)).unwrap_or(0.0);
    e.aperture = if fnum > 0.0 { format!("f/{}", trim_zeros(fnum)) } else { String::new() };

    e.shutter = format_shutter(t.exposure_time.or_else(|| raw.as_ref().map(|r| r.shutter as f64)).unwrap_or(0.0));

    let fl = t.focal_length.or_else(|| raw.as_ref().map(|r| r.focal_len as f64)).unwrap_or(0.0);
    e.focal_length = if fl > 0.0 { format!("{} mm", trim_zeros(fl)) } else { String::new() };

    let ec = t.exposure_bias.unwrap_or(0.0);
    e.exposure_bias = if ec == 0.0 { "0 EV".into() } else { format!("{:+.1} EV", ec) };

    e.white_balance = match t.white_balance {
        Some(0) => "Auto".into(),
        Some(_) => "Manual".into(),
        None => String::new(),
    };
    e.metering_mode = t.metering_mode.map(metering_mode_name).unwrap_or_default();
    e.date_taken = first_non_empty(&[
        t.date_original.clone().unwrap_or_default(),
        t.date_digitized.clone().unwrap_or_default(),
        t.date_time.clone().unwrap_or_default(),
    ]);

    let (w, h) = dimensions(path, &t, raw.as_ref());
    e.width = w as i64;
    e.height = h as i64;
    e
}

/// The visible frame size: what the native builds show and what LibRaw's output is
/// trimmed to (mask borders on models LibRaw has no crop table for).
pub fn visible_size(path: &str) -> Option<(usize, usize)> {
    tiff::read(path).and_then(|t| t.visible_size())
}

fn dimensions(path: &str, t: &TiffMeta, raw: Option<&libraw::RawMeta>) -> (usize, usize) {
    if paths::is_raw(path) {
        if let Some(v) = t.visible_size() {
            return v;
        }
        if let Some(r) = raw.filter(|r| r.width > 0 && r.height > 0) {
            return (r.width as usize, r.height as usize);
        }
    }
    if let Some(v) = t.visible_size() {
        return v;
    }
    image::image_dimensions(path).map(|(w, h)| (w as usize, h as usize)).unwrap_or((0, 0))
}

fn first_non_empty(v: &[String]) -> String {
    v.iter().find(|s| !s.trim().is_empty()).cloned().unwrap_or_default()
}

/// .NET's "0.#": at most one decimal place, no trailing zero.
pub fn trim_zeros(v: f64) -> String {
    let s = format!("{:.1}", v);
    s.strip_suffix(".0").map(str::to_string).unwrap_or(s)
}

pub fn format_shutter(seconds: f64) -> String {
    if seconds <= 0.0 {
        String::new()
    } else if seconds >= 1.0 {
        format!("{} s", trim_zeros(seconds))
    } else {
        format!("1/{} s", (1.0 / seconds).round() as i64)
    }
}

fn metering_mode_name(v: u32) -> String {
    match v {
        0 => "Unknown",
        1 => "Average",
        2 => "Center-weighted average",
        3 => "Spot",
        4 => "Multi-spot",
        5 => "Multi-segment",
        6 => "Partial",
        255 => "Other",
        _ => "",
    }
    .to_string()
}
