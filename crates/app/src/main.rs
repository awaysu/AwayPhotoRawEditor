//! AwayPhotoRawEditor — the cross-platform editor (egui + wgpu).
//!
//! The window and the photo pipeline share one wgpu device, so a rendered frame is drawn
//! straight from the GPU buffer it was computed into.
//!
//!   AwayPhotoRawEditor                         open the last folder
//!   AwayPhotoRawEditor --shot <folder> <png> [WxH] [index]
//!                                              diagnostics: screenshot and quit
//!   env AWPR_SHOT_ADJ="exposure=0.5,..."       (shot) adjustments applied in memory
//!   env AWPR_SHOT_TOOL=crop|gradient|heal      (shot) open that tool (sample spots if none)
//!   env AWPR_SHOT_DLG=export|presets|settings|fonts|about|firstrun|cameras
//!                                              (shot) open that window
//!   env AWPR_LANG=zh-TW|en|ja|ko|zh-CN|de|fr|es  interface language for this run
//!   env AWPR_SHOT_SCROLL=bottom               (shot) scroll the right column to its end
//!   env AWPR_UI_SCALE=1.5                      force the interface size
//!   env AWPR_SHOT_SELECT=1,2                   (shot) select these strip positions (1-based)
//!   env AWPR_SHOT_MENU=1                       (shot) open the thumbnail menu
//!   env AWPR_SHOT_SHOW_HIDDEN=1                (shot) show hidden photos
//!   env AWPR_SHOT_XMP=1                        (shot) turn 支援 XMP on (menus offer XMP)
//!   env AWPR_SHOT_WM=<text>                    (shot) turn the export watermark on (live preview)
//!   env AWPR_NO_GPU=1                          render on the CPU

#![cfg_attr(all(windows, not(debug_assertions), not(feature = "console")), windows_subsystem = "windows")]

mod app;
mod export_ui;
mod i18n;
mod presets_ui;
mod settings;
mod theme;
mod tools;
mod update;
mod viewer;
mod widgets;
mod worker;

use eframe::egui;
use eframe::egui_wgpu::{self, WgpuSetup, WgpuSetupCreateNew};
use std::sync::Arc;

/// Prefer Direct3D 12 on Windows (3x faster than Vulkan on the Iris Xe's LUT-heavy
/// stages, see docs/POC-STEP2.md), a real GPU over a software one, and a discrete GPU
/// over an integrated one. WGPU_BACKEND still narrows the list first.
fn pick_adapter(adapters: &[wgpu::Adapter], _surface: Option<&wgpu::Surface<'_>>) -> Result<wgpu::Adapter, String> {
    let rank = |a: &wgpu::Adapter| {
        let i = a.get_info();
        let backend = match i.backend {
            wgpu::Backend::Dx12 if cfg!(windows) => 0,
            wgpu::Backend::Metal | wgpu::Backend::Vulkan => 1,
            _ => 2,
        };
        let kind = match i.device_type {
            wgpu::DeviceType::DiscreteGpu => 0,
            wgpu::DeviceType::IntegratedGpu => 1,
            wgpu::DeviceType::VirtualGpu | wgpu::DeviceType::Other => 2,
            wgpu::DeviceType::Cpu => 3,
        };
        (backend, kind)
    };
    adapters.iter().min_by_key(|a| rank(a)).cloned().ok_or_else(|| crate::i18n::t("找不到 GPU").to_string())
}

fn wgpu_options() -> egui_wgpu::WgpuConfiguration {
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    setup.native_adapter_selector = Some(Arc::new(pick_adapter));
    setup.device_descriptor = Arc::new(|adapter: &wgpu::Adapter| {
        // The photo pipeline shares this device: it needs the adapter's real buffer
        // limits (a 16 MP frame is 256 MB, double the default binding size).
        let mut limits = adapter.limits();
        limits.max_texture_dimension_2d = limits.max_texture_dimension_2d.max(8192);
        wgpu::DeviceDescriptor { label: Some("AwayPhotoRawEditor"), required_limits: limits, ..Default::default() }
    });
    egui_wgpu::WgpuConfiguration { wgpu_setup: WgpuSetup::CreateNew(setup), ..Default::default() }
}

/// AWPR_TRACE=<file>: append a timestamped line per event (diagnostics).
pub fn trace(msg: &str) {
    use std::sync::OnceLock;
    static PATH: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();
    if let Some(p) = PATH.get_or_init(|| std::env::var_os("AWPR_TRACE").map(Into::into)) {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
            let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
            let _ = writeln!(f, "{t} {msg}");
        }
    }
}

/// A release build has no console: keep the panic message somewhere findable.
fn install_crash_log() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let path = std::env::temp_dir().join("awpr_crash.txt");
        let msg = format!("{info}\n{}\n", std::backtrace::Backtrace::force_capture());
        let _ = std::fs::write(&path, msg);
        default(info);
    }));
}

fn main() -> eframe::Result {
    install_crash_log();
    trace("main start");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut shot = None;
    let mut size = egui::vec2(1500.0, 1000.0);
    if args.first().map(String::as_str) == Some("--shot") && args.len() >= 3 {
        if let Some((w, h)) = args.get(3).and_then(|s| s.split_once('x')) {
            if let (Ok(w), Ok(h)) = (w.parse(), h.parse()) {
                size = egui::vec2(w, h);
            }
        }
        shot = Some(app::ShotPlan {
            folder: args[1].clone(),
            out: args[2].clone(),
            select: args.get(4).and_then(|s| s.parse().ok()).unwrap_or(0),
            adjust: std::env::var("AWPR_SHOT_ADJ").ok(),
            started: std::time::Instant::now(),
            requested: false,
        });
    }
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("AwayPhotoRawEditor")
        .with_icon(eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/linux/awayphotoraweditor.png")).unwrap_or_default())
        .with_inner_size(size)
        .with_min_inner_size(egui::vec2(1100.0, 700.0));
    if shot.is_none() {
        viewport = viewport.with_maximized(true);
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: wgpu_options(),
        ..Default::default()
    };
    let r = eframe::run_native("AwayPhotoRawEditor", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, shot)))));
    trace(&format!("run_native returned {:?}", r.as_ref().err()));
    r
}
