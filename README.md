# AwayPhotoRawEditor

AwayPhotoRawEditor 以 **Rust 重寫為跨平台版本**（Windows / macOS / Linux），取代 C#（Windows）與 Swift（macOS）兩套程式。

AwayPhotoRawEditor is a non-destructive RAW photo editor written in **Rust** for Windows, macOS and Linux.

**目前版本：2.0.1**——三平台安裝檔、處理版本 3（寬色域線性管線、高光復原、HSL、曲線）、局部遮罩、HEIC、XMP。
發佈說明見 [`docs/RELEASE-NOTES-2.0.1.md`](docs/RELEASE-NOTES-2.0.1.md)，開發紀錄見 [`docs/ROADMAP.md`](docs/ROADMAP.md)、打包與發佈流程見 [`docs/RELEASE.md`](docs/RELEASE.md)。

| crate | 內容 |
|---|---|
| `crates/libraw-sys` | LibRaw 0.22.2（`vendor/`，原始碼直接編）＋與 Swift 版共用的 C shim |
| `crates/core` | 模型、色彩科學、色調曲線、CPU 管線、縮圖、LibRaw 橋接（Swift `AwayRawCore` 的移植） |
| `crates/gpu` | wgpu 管線：WGSL kernel（Swift Metal kernel 的逐行移植）、常駐 GPU 的 `GpuFrame`、viewer shader、直方圖 |
| `crates/photo` | `RAW_TEMP` 快取與 `rawpipe.xml`（與 C#／Swift 逐位元組互通）、EXIF、一般圖檔解碼 |
| `crates/app` | `AwayPhotoRawEditor`：egui + wgpu 編輯器，大圖直接從 GPU buffer 畫 |
| `crates/cli` | `awpr`：`hashtest`、`gputest`／`gputest3`、`tiletest`、`viewtest`、`exporttest`、`heictest`、`cachebench`、`v3cmp`、`bench`、`stages`、`meta`、`dumpsrc`／`cmpsrc`、`dllcheck`、`info` |

## 建置 / Build

```bash
cargo build --release            # Windows / Linux（Rust 1.95 以上）
scripts/build-macos.sh <libomp>  # macOS（OpenMP 版 LibRaw，見 docs）
target/release/AwayPhotoRawEditor
target/release/awpr hashtest <raw> report.txt
cargo test --workspace
```

⚠️ 管線的浮點運算順序刻意與 Swift / C# 一字不差（f32/f64 的分界、加法順序、.NET 銀行家捨入），
這樣 `hashtest` 才能逐位元對照。改 `crates/core` 的算式時要三邊一起改，並重跑 `scripts/hashtest-diff.sh`。

⚠️ `rawpipe.xml` 由三個版本共用：改 `crates/photo/src/store.rs` 的欄位或順序前，先確認 C#（`AdjustmentXmlStore.cs`）
與 Swift（`AdjustmentXmlStore.swift`）同步；`cargo test -p awpr-photo` 會拿兩邊寫的檔案做逐位元組往返測試。

## 舊版本 / Legacy versions

| 平台 | 技術 | 分支 |
|------|------|------|
| Windows | C# / WinForms / .NET 8 | [`legacy/windows`](https://github.com/awaysu/AwayPhotoRawEditor/tree/legacy/windows) |
| macOS | Swift / AppKit / Metal | [`legacy/macos`](https://github.com/awaysu/AwayPhotoRawEditor/tree/legacy/macos) |

兩個分支皆保留完整的開發歷史，僅作封存，不再更新。

## 下載 / Download

**https://www.awaysu.cc/software/awayphotoraweditor**

## 授權 / License

BSD-3-Clause，Copyright (c) 2026 Chih-Wei Su (Awaysu)。LibRaw 依 CDDL-1.0 靜態連結（見 `crates/libraw-sys/vendor/LibRaw-0.22.2/LICENSE.CDDL`）。
