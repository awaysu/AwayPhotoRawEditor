//! Raw bindings to the C shim in `shim.c` (the same file the Swift build compiles).
//! Everything above this layer goes through `awpr-core::libraw`.

#![allow(non_camel_case_types)]

use std::os::raw::{c_char, c_double, c_int, c_void};

// Pull in the bundled zlib so the linker sees it.
extern crate libz_sys;

pub type awpr_raw = *mut c_void;

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct awpr_sizes {
    pub raw_width: c_int,
    pub raw_height: c_int,
    pub width: c_int,
    pub height: c_int,
    pub left_margin: c_int,
    pub top_margin: c_int,
    pub iwidth: c_int,
    pub iheight: c_int,
    pub flip: c_int,
}

#[repr(C)]
#[derive(Debug)]
pub struct awpr_image {
    pub opaque: *mut c_void,
    pub owner: awpr_raw,
    pub data: *const c_void,
    pub type_: c_int,
    pub width: c_int,
    pub height: c_int,
    pub colors: c_int,
    pub bits: c_int,
    pub data_size: c_int,
}

impl Default for awpr_image {
    fn default() -> Self {
        Self {
            opaque: std::ptr::null_mut(),
            owner: std::ptr::null_mut(),
            data: std::ptr::null(),
            type_: 0,
            width: 0,
            height: 0,
            colors: 0,
            bits: 0,
            data_size: 0,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct awpr_meta {
    pub make: [c_char; 64],
    pub model: [c_char; 64],
    pub lens: [c_char; 128],
    pub iso_speed: f32,
    pub shutter: f32,
    pub aperture: f32,
    pub focal_len: f32,
    pub timestamp: i64,
    pub width: c_int,
    pub height: c_int,
    pub flip: c_int,
}

impl Default for awpr_meta {
    fn default() -> Self {
        Self {
            make: [0; 64],
            model: [0; 64],
            lens: [0; 128],
            iso_speed: 0.0,
            shutter: 0.0,
            aperture: 0.0,
            focal_len: 0.0,
            timestamp: 0,
            width: 0,
            height: 0,
            flip: 0,
        }
    }
}

extern "C" {
    pub fn awpr_libraw_available() -> c_int;
    pub fn awpr_libraw_version() -> *const c_char;
    pub fn awpr_read_sizes(path: *const c_char, out: *mut awpr_sizes) -> c_int;
    pub fn awpr_read_camera_color(
        path: *const c_char,
        pre_mul: *mut c_double,
        cam_mul: *mut c_double,
        rgb_cam: *mut c_double,
    ) -> c_int;
    pub fn awpr_read_meta(path: *const c_char, out: *mut awpr_meta) -> c_int;
    pub fn awpr_decode_full(path: *const c_char, bps: c_int, out: *mut awpr_image) -> c_int;
    pub fn awpr_decode_linear(path: *const c_char, out: *mut awpr_image) -> c_int;
    pub fn awpr_decode_thumb(path: *const c_char, out: *mut awpr_image, flip: *mut c_int) -> c_int;
    pub fn awpr_free_image(img: *mut awpr_image);
    pub fn awpr_camera_count() -> c_int;
    pub fn awpr_camera_name(index: c_int) -> *const c_char;
}
