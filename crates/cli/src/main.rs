//! `awpr` — headless diagnostics, the counterpart of the Swift `awpr-cli` and the C#
//! `--selftest` / `--hashtest` switches.

mod bench;
mod cachebench;
mod compare;
mod dllcheck;
mod exporttest;
mod gputest;
mod hashtest;
mod heictest;
mod stages;
mod tiletest;
mod viewtest;

use awpr_core::color::{self, WhiteBalanceReference};
use awpr_core::libraw;

const USAGE: &str = "\
usage:
  awpr info <raw>                 LibRaw version, sizes and camera colour data
  awpr meta <file>                the info-panel EXIF fields and visible size (as cached in rawpipe.xml)
  awpr hashtest <raw> [report]    colour-pipeline fingerprint (diff against the Swift / C# report)
  awpr gputest <raw> [report]     CPU vs GPU on the 14 cases + 3 heal cases (8-bit criteria)
  awpr gputest3 <raw> [report]    the same for 處理版本 3 (14 cases + highlights / HSL / curves + encoded source)
  awpr v3cmp <raw> <outDir>       a photo as 處理版本 2 and upgraded to 3 (PNGs + mean difference)
  awpr heictest <file>...         HEIC through the system decoder (size, orientation, EXIF)
  awpr tiletest <raw> [report]    full-size export render: GPU in strips vs CPU (版本 2 and 3)
  awpr viewtest <raw> [report]    the editor's viewer shader vs a CPU reference (offscreen, any backend)
  awpr exporttest <image> <outDir> <report>  export JPEG / PNG / TIFF (8 / 16-bit, ICC, watermark) and check them
  awpr bench <raw>                decode / proxy / pipeline timings
  awpr cachebench <dir> <raw>...  cache build time: 處理版本 2 only vs + 處理版本 3 (two decodes / one), identical output
  awpr stages <raw>               SHA of each intermediate (bisect a cross-platform difference)
  awpr dumpsrc <raw> <out.f32>    write the hashtest source proxy
  awpr cmpsrc <raw> <other.f32>   render the 14 cases from both proxies and compare (gputest criteria)
  awpr dllcheck <libraw.dll> <raw>  Windows: compare with the C# build's libraw.dll decode";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("info") if args.len() >= 2 => info(&args[1]),
        Some("meta") if args.len() >= 2 => meta(&args[1]),
        Some("hashtest") if args.len() >= 2 => hashtest::run(&args[1], args.get(2).map(String::as_str)),
        Some("gputest") if args.len() >= 2 => gputest::run(&args[1], args.get(2).map(String::as_str)),
        Some("gputest3") if args.len() >= 2 => gputest::run_v3(&args[1], args.get(2).map(String::as_str)),
        Some("v3cmp") if args.len() >= 3 => gputest::compare_versions(&args[1], &args[2]),
        Some("heictest") if args.len() >= 2 => heictest::run(&args[1..]),
        Some("tiletest") if args.len() >= 2 => tiletest::run(&args[1], args.get(2).map(String::as_str)),
        Some("viewtest") if args.len() >= 2 => viewtest::run(&args[1], args.get(2).map(String::as_str)),
        Some("exporttest") if args.len() >= 4 => exporttest::run(&args[1], &args[2], &args[3]),
        Some("bench") if args.len() >= 2 => bench::run(&args[1]),
        Some("cachebench") if args.len() >= 3 => cachebench::run(&args[1], &args[2..]),
        Some("stages") if args.len() >= 2 => stages::run(&args[1]),
        Some("dumpsrc") if args.len() >= 3 => compare::dump(&args[1], &args[2]),
        Some("cmpsrc") if args.len() >= 3 => compare::run(&args[1], &args[2]),
        Some("dllcheck") if args.len() >= 3 => dllcheck::run(&args[1], &args[2]),
        _ => {
            eprintln!("{USAGE}");
            2
        }
    };
    std::process::exit(code);
}

fn info(path: &str) -> i32 {
    println!("LibRaw   : {} (available: {})", libraw::version(), libraw::available());
    match libraw::read_sizes(path) {
        Some(s) => println!(
            "sizes    : raw {}x{} visible {}x{} margin L{} T{} flip {}",
            s.raw_width, s.raw_height, s.width, s.height, s.left_margin, s.top_margin, s.flip
        ),
        None => {
            println!("sizes    : LibRaw cannot open this file");
            return 1;
        }
    }
    match libraw::read_camera_color(path) {
        Some(c) => {
            println!("pre_mul  : {:?}", c.pre_mul);
            println!("cam_mul  : {:?}", c.cam_mul);
            println!("rgb_cam  : {:?}", c.rgb_cam);
            if let Some((k, t)) = color::as_shot(&c) {
                println!("as-shot  : {k} K / tint {t}");
            }
            let m = color::white_balance_matrix(&c, 5200.0, 0.0, WhiteBalanceReference::Decode);
            println!("WB 5200K : {m:?}");
        }
        None => println!("camera colour: none (black-body fallback)"),
    }
    0
}

fn meta(path: &str) -> i32 {
    let e = awpr_photo::exif::read(path);
    let rows = [
        ("CameraMake", e.camera_make.clone()),
        ("CameraModel", e.camera_model.clone()),
        ("Lens", e.lens.clone()),
        ("ISO", e.iso.clone()),
        ("Aperture", e.aperture.clone()),
        ("Shutter", e.shutter.clone()),
        ("FocalLength", e.focal_length.clone()),
        ("ExposureBias", e.exposure_bias.clone()),
        ("WhiteBalance", e.white_balance.clone()),
        ("MeteringMode", e.metering_mode.clone()),
        ("DateTaken", e.date_taken.clone()),
        ("Width", e.width.to_string()),
        ("Height", e.height.to_string()),
        ("FileSize", e.file_size.to_string()),
    ];
    for (k, v) in rows {
        println!("{k:<13}: {v}");
    }
    println!("visible      : {:?}", awpr_photo::exif::visible_size(path));
    0
}
