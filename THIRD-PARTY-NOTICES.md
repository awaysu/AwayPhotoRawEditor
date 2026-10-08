# Third-party notices

AwayPhotoRawEditor is licensed under BSD-3-Clause (see `LICENSE`). It includes or links the
following third-party components. Versions are those in `Cargo.lock` at the time of writing;
the full dependency tree (transitive crates included) can be listed with
`cargo metadata --format-version 1`.

## Compiled in from source

| Component | Version | License | Notes |
|---|---|---|---|
| [LibRaw](https://www.libraw.org/) | 0.22.2 | CDDL-1.0 (chosen) or LGPL-2.1 | Vendored in `crates/libraw-sys/vendor/LibRaw-0.22.2/`, statically linked under CDDL-1.0. Its source, including any changes, is available in this repository. See `LICENSE.CDDL` in that folder. |
| [zlib](https://zlib.net/) (bundled by `libz-sys`) | (libz-sys 1.1) | Zlib | Statically linked, used by LibRaw for DNG. |

## Code adapted from other projects

| Project | License | What |
|---|---|---|
| [lightcraft](https://github.com/storytold/lightcraft) | MIT OR Apache-2.0 (used under MIT) | 處理版本 3 (`crates/core/src/v3.rs`): highlight reconstruction (`crates/raw/src/highlight.rs`: clip-neutral, coarse-to-fine chromaticity fill), the OkLab / OkLCh conversions and colour tools with the 8-band partition-of-unity weights (`crates/color/src/perceptual.rs`, `crates/pipeline/src/colorops.rs`), the monotone cubic curve (`crates/color/src/spline.rs`), the gamut compression to the output space; the XMP reader's structure (`crates/photo/src/xmp.rs`, from `crates/meta/src/xmp.rs`). |

lightcraft's licence (MIT):

```
MIT License

Copyright (c) 2026 ArtCraft Team and the LightCraft contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Used at run time, not included

| Component | License | How |
|---|---|---|
| [libheif](https://github.com/strukturag/libheif) with [libde265](https://github.com/strukturag/libde265) (Linux) | LGPL-3.0 | HEIC decoding on Linux. The program loads the system's `libheif.so.1` at run time when a HEIC is opened (the `.deb` / `.rpm` only *recommend* the packages); nothing of either library is compiled into or shipped with AwayPhotoRawEditor, so the LGPL's terms stay with the user's own installation and its replaceability is untouched. |
| Windows Imaging Component with Microsoft's HEIF / HEVC extensions (Windows), ImageIO (macOS) | Operating system components | HEIC decoding through the operating system. |

## Rust crates (direct dependencies)

| Crate | Version | License |
|---|---|---|
| [egui / eframe / egui-wgpu / epaint](https://github.com/emilk/egui) | 0.36 | MIT OR Apache-2.0 |
| [epaint_default_fonts](https://github.com/emilk/egui) | 0.36 | (MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0 |
| [wgpu](https://github.com/gfx-rs/wgpu) | 30 | MIT OR Apache-2.0 |
| [image](https://github.com/image-rs/image) | 0.25 | MIT OR Apache-2.0 |
| [tiff](https://github.com/image-rs/image-tiff) | 0.11 | MIT |
| [ab_glyph](https://github.com/alexheretic/ab-glyph) | 0.2 | Apache-2.0 |
| [quick-xml](https://github.com/tafia/quick-xml) | 0.42 | MIT |
| [rfd](https://github.com/PolyMeilex/rfd) | 0.17 | MIT |
| [rayon](https://github.com/rayon-rs/rayon) | 1.12 | MIT OR Apache-2.0 |
| [chrono](https://github.com/chronotope/chrono) | 0.4 | MIT OR Apache-2.0 |
| [trash](https://github.com/Byron/trash-rs) | 5 | MIT |
| [ureq](https://github.com/algesten/ureq) (update check) | 3 | MIT OR Apache-2.0 |
| [serde_json](https://github.com/serde-rs/json) | 1 | MIT OR Apache-2.0 |
| [sys-locale](https://github.com/1Password/sys-locale) | 0.3 | MIT OR Apache-2.0 |
| [bytemuck](https://github.com/Lokathor/bytemuck) | 1.25 | Zlib OR Apache-2.0 OR MIT |
| [pollster](https://github.com/zesterer/pollster) | 0.4 | Apache-2.0 OR MIT |
| [libz-sys](https://github.com/rust-lang/libz-sys) | 1.1 | MIT OR Apache-2.0 |
| [cc](https://github.com/rust-lang/cc-rs) (build only) | 1.2 | MIT OR Apache-2.0 |
| [sha2](https://github.com/RustCrypto/hashes) (`awpr` CLI) | 0.10 | MIT OR Apache-2.0 |
| [libloading](https://github.com/nagisa/rust_libloading) (`awpr` CLI on Windows; libheif on Linux) | 0.8 | ISC |
| [windows](https://github.com/microsoft/windows-rs) (WIC, Windows) | 0.62 | MIT OR Apache-2.0 |

## Fonts

| Font | License | Use |
|---|---|---|
| Ubuntu Light (from `epaint_default_fonts`) | Ubuntu Font Licence 1.0 | egui's UI font for Latin text, and the last fallback for watermark text. |
| Hack (from `epaint_default_fonts`) | MIT / Bitstream Vera | egui's monospace font. |
| Noto Emoji, emoji-icon-font (from `epaint_default_fonts`) | OFL-1.1 / MIT | egui's emoji / icon fallback. |

The CJK UI font and watermark fonts are **not** shipped: they are loaded from the operating
system's font folders at run time.

## Colour profile

The sRGB ICC profile embedded in exported JPEG / PNG / TIFF files is generated by the program
(`crates/photo/src/export.rs`, `srgb_icc`) from the published sRGB primaries and transfer curve;
no third-party profile file is included.
