//! `dllcheck <libraw.dll> <raw>` (Windows only) — decode with the C# build's own
//! `tools/libraw/.../libraw.dll`, with the C# settings, and compare it with this build's
//! LibRaw. Answers "does the Rust Windows build start from the same pixels as the
//! shipping Windows app?" without needing a C# hashtest.

#[cfg(windows)]
pub fn run(dll: &str, path: &str) -> i32 {
    let dll = dll.to_string();
    let path = path.to_string();
    // LibRaw needs a big stack (see libraw::run_large_stack).
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || unsafe { check(&dll, &path) })
        .unwrap()
        .join()
        .unwrap()
}

#[cfg(windows)]
unsafe fn check(dll: &str, path: &str) -> i32 {
    use awpr_core::libraw;
    use libloading::{Library, Symbol};
    use std::ffi::{c_char, c_int, c_void, CStr, CString};

    /// libraw_processed_image_t
    #[repr(C)]
    struct Processed {
        type_: c_int,
        height: u16,
        width: u16,
        colors: u16,
        bits: u16,
        data_size: u32,
        data: [u8; 1],
    }

    type Lr = *mut c_void;
    let lib = Library::new(dll).expect("load libraw.dll");
    let init: Symbol<unsafe extern "C" fn(u32) -> Lr> = lib.get(b"libraw_init").unwrap();
    let version: Symbol<unsafe extern "C" fn() -> *const c_char> = lib.get(b"libraw_version").unwrap();
    let set_color: Symbol<unsafe extern "C" fn(Lr, c_int)> = lib.get(b"libraw_set_output_color").unwrap();
    let set_bps: Symbol<unsafe extern "C" fn(Lr, c_int)> = lib.get(b"libraw_set_output_bps").unwrap();
    let set_nab: Symbol<unsafe extern "C" fn(Lr, c_int)> = lib.get(b"libraw_set_no_auto_bright").unwrap();
    let open: Symbol<unsafe extern "C" fn(Lr, *const c_char) -> c_int> = lib.get(b"libraw_open_file").unwrap();
    let unpack: Symbol<unsafe extern "C" fn(Lr) -> c_int> = lib.get(b"libraw_unpack").unwrap();
    let process: Symbol<unsafe extern "C" fn(Lr) -> c_int> = lib.get(b"libraw_dcraw_process").unwrap();
    let mem: Symbol<unsafe extern "C" fn(Lr, *mut c_int) -> *mut Processed> =
        lib.get(b"libraw_dcraw_make_mem_image").unwrap();
    let clear: Symbol<unsafe extern "C" fn(*mut Processed)> = lib.get(b"libraw_dcraw_clear_mem").unwrap();
    let close: Symbol<unsafe extern "C" fn(Lr)> = lib.get(b"libraw_close").unwrap();

    println!("C# libraw.dll : {}", CStr::from_ptr(version()).to_string_lossy());
    println!("Rust LibRaw   : {}", libraw::version());

    let cp = CString::new(path).unwrap();
    let t = std::time::Instant::now();
    let lr = init(0);
    set_color(lr, 1);
    set_bps(lr, 16);
    set_nab(lr, 0);
    if open(lr, cp.as_ptr()) != 0 || unpack(lr) != 0 || process(lr) != 0 {
        println!("!! libraw.dll could not decode the file");
        close(lr);
        return 1;
    }
    let mut err = 0;
    let img = mem(lr, &mut err);
    if img.is_null() {
        println!("!! make_mem_image failed ({err})");
        close(lr);
        return 1;
    }
    let p = &*img;
    let (w, h, colors) = (p.width as usize, p.height as usize, p.colors as usize);
    let dll_pixels = std::slice::from_raw_parts(p.data.as_ptr() as *const u16, w * h * colors).to_vec();
    clear(img);
    close(lr);
    let dll_ms = t.elapsed().as_secs_f64() * 1000.0;

    let t = std::time::Instant::now();
    let Some(rust) = libraw::decode_full_untrimmed(path, 16) else {
        println!("!! Rust decode failed");
        return 1;
    };
    println!("decode time   : C# dll {dll_ms:.0} ms, Rust {:.0} ms", t.elapsed().as_secs_f64() * 1000.0);
    if (rust.width, rust.height) != (w, h) {
        println!("❌ size differs: dll {w} x {h}, Rust {} x {}", rust.width, rust.height);
        return 1;
    }
    let inv = 1.0f32 / 65535.0f32;
    let mut differ = 0usize;
    let mut max = 0f32;
    for (i, px) in rust.data.chunks_exact(4).enumerate() {
        for c in 0..3 {
            let d = (px[c] - dll_pixels[i * colors + c] as f32 * inv).abs();
            if d > 0.0 {
                differ += 1;
                max = max.max(d);
            }
        }
    }
    println!("pixels        : {w} x {h}");
    println!("differing ch. : {differ}  (max {max:.2e})");
    println!("{}", if differ == 0 { "✅ identical to the C# build's decode" } else { "❌ differs" });
    i32::from(differ != 0)
}

#[cfg(not(windows))]
pub fn run(_dll: &str, _path: &str) -> i32 {
    eprintln!("dllcheck only runs on Windows");
    2
}
