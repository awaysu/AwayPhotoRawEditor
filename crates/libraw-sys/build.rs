//! Compiles LibRaw 0.22.2 from `vendor/` and the C shim into one static library.
//!
//! The defines match `AwayPhotoRawEditor_Swift/Scripts/build_libraw.sh`, so the three
//! platforms decode with the same LibRaw, the same options and the same compiled-out
//! decoders. That is what lets `hashtest` compare against the Swift and C# reports.
//!
//! Licence note: LibRaw is dual-licensed LGPL-2.1 / CDDL-1.0. Linking it statically is
//! done under CDDL-1.0 (file-level copyleft; the unmodified sources ship in `vendor/`).

use std::env;
use std::path::{Path, PathBuf};

fn collect_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read vendor/src") {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "cpp") {
            let name = path.file_name().unwrap().to_string_lossy();
            // *_ph.cpp are placeholders for a build without postprocessing; they redefine
            // dcraw_process and friends.
            if !name.ends_with("_ph.cpp") {
                out.push(path);
            }
        }
    }
}

fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let libraw = root.join("vendor/LibRaw-0.22.2");
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let msvc = target_env == "msvc";
    let openmp = env::var("CARGO_FEATURE_OPENMP").is_ok()
        && openmp_supported(&target_os)
        && env::var("AWPR_LIBRAW_OPENMP").as_deref() != Ok("0");

    let mut sources = Vec::new();
    collect_sources(&libraw.join("src"), &mut sources);
    sources.sort();

    let zlib_include = env::var("DEP_Z_INCLUDE").ok();

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .files(&sources)
        .include(&libraw)
        .include(libraw.join("libraw"))
        .define("NDEBUG", None)
        .define("NO_JASPER", None)
        .define("NO_JPEG", None)
        .define("NO_LCMS", None)
        .define("USE_ZLIB", None)
        .define("LIBRAW_NODLL", None)
        .opt_level(3)
        .warnings(false);
    if let Some(inc) = &zlib_include {
        build.include(inc);
    }
    if msvc {
        build.flag("/EHsc").flag("/std:c++14").define("_CRT_SECURE_NO_WARNINGS", None);
    } else {
        build.flag("-std=c++11").flag("-w");
    }
    let mac_omp = if target_os == "macos" { mac_libomp_dir() } else { None };
    if openmp {
        if msvc {
            build.flag("/openmp");
        } else {
            build.flag("-fopenmp");
        }
    } else if let Some(dir) = &mac_omp {
        // Apple clang understands the pragmas but ships no runtime: point it at libomp.
        build.flag("-Xclang").flag("-fopenmp").include(dir.join("include"));
    } else {
        build.define("LIBRAW_NOTHREADS", None);
    }
    build.compile("raw");

    let mut shim = cc::Build::new();
    shim.file(root.join("src/shim.c"))
        .include(&libraw)
        .define("LIBRAW_NODLL", None)
        .opt_level(3);
    shim.compile("awpr_shim");

    if let Some(dir) = &mac_omp {
        println!("cargo:rustc-link-search=native={}", dir.join("lib").display());
        // The executable needs an rpath to it: scripts/build-macos.sh adds one for
        // development; a release .app bundles it in Contents/Frameworks.
        println!("cargo:rustc-link-lib=dylib=omp");
    }
    if openmp && !msvc {
        // gcc's runtime; MSVC links vcomp automatically from the /openmp objects.
        println!("cargo:rustc-link-lib=gomp");
    }
    if !msvc {
        let cxx = if target_os == "macos" { "c++" } else { "stdc++" };
        println!("cargo:rustc-link-lib={cxx}");
    }
    if target_os == "windows" {
        // LibRaw's datastream uses Winsock's ntohl/htonl.
        println!("cargo:rustc-link-lib=ws2_32");
    }
    println!("cargo:rerun-if-changed=src/shim.c");
    println!("cargo:rerun-if-changed=src/shim.h");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=AWPR_LIBRAW_OPENMP");
    println!("cargo:rerun-if-env-changed=AWPR_LIBOMP_DIR");
}

/// Apple clang has no bundled OpenMP runtime, so macOS goes through `mac_libomp_dir`.
fn openmp_supported(target_os: &str) -> bool {
    target_os != "macos"
}

/// $AWPR_LIBOMP_DIR, when it holds `include/omp.h` and `lib/libomp.dylib`. Without it,
/// macOS LibRaw is single-threaded: same pixels, ~2.5x slower full decode.
/// `scripts/build-macos.sh` points it at the universal, minos-14 libomp the Swift app
/// already builds and bundles.
///
/// Only the dylib is accepted: linking Homebrew's static libomp.a crashed inside
/// LibRaw's first `omp critical` (null lock in __kmp_acquire_ticket_lock).
fn mac_libomp_dir() -> Option<PathBuf> {
    if env::var("AWPR_LIBRAW_OPENMP").as_deref() == Ok("0") {
        return None;
    }
    let dir = PathBuf::from(env::var("AWPR_LIBOMP_DIR").ok()?);
    (dir.join("include/omp.h").is_file() && dir.join("lib/libomp.dylib").is_file()).then_some(dir)
}
