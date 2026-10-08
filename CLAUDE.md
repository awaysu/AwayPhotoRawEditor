# CLAUDE.md — AwayPhotoRawEditor（Rust 跨平台版）

AwayPhotoRawEditor 以 **Rust 重寫為 Windows / macOS / Linux 三平台版**，取代 C#（`legacy/windows`）與 Swift（`legacy/macos`）兩套程式。
介面用 **egui + wgpu 原生視窗**（不用 Tauri，大圖直接從 GPU buffer 畫）；RAW 解碼用 LibRaw（原始碼直接編進去）。

- 授權 BSD-3-Clause，Copyright (c) 2026 Chih-Wei Su (Awaysu)。LibRaw 靜態連結走 CDDL-1.0。
- 顯示名稱一律「AwayPhotoRawEditor」。對使用者一律用繁體中文。
- 下載頁：https://www.awaysu.cc/software/awayphotoraweditor

## 目前狀態
**第 0 步完成（2026-10-09，commit 9cd31d8）；第 1 步 TASK-002 工具互動編輯進行中。** 進度與各步驟的詳細內容見 `docs/ROADMAP.md`，完成的步驟報告在 `docs/`。
PoC 原本在 `C:\Users\AwayWork\Desktop\WORKSPACE2\testAwayPhotoRawEditor\AwayPhotoRawEditor_Rust`（第 1–3 步，三平台驗證過）。

## 工作方式（Multi-Agent）
- Agent-11（PM）規劃、拆步驟、審查；Agent-12（Software Engineer）寫程式。每一步寫完 → PM 審查 → 修正 → 下一步。
- 大項目完成時更新本檔的「目前狀態」與 `docs/ROADMAP.md`，並在 `CHANGELOG.md` 記錄。

## 不可違反的原則
1. **先完全重現現有輸出，再升級色彩**：`crates/core` 的浮點運算順序刻意與 C# / Swift 一字不差，改算式要三邊一起改並重跑 `scripts/hashtest-diff.sh`、`awpr gputest`。色彩升級一律用新的「處理版本」，舊照片維持舊算式。
2. **`rawpipe.xml` / `preview_list.xml` 與 C#、Swift 版互通**：改 `crates/photo/src/store.rs` 的欄位或順序前先對照 `AdjustmentXmlStore.cs` / `.swift`；`cargo test -p awpr-photo` 做逐位元組往返測試，必須保持通過。
3. **每一步都要有可重複的驗證**（單元測試、hashtest / gputest / viewtest、`--shot` 離屏截圖），不能只靠「看起來正常」。
4. **不要做需要前景視窗或鍵盤焦點的測試**（桌面是共用的）；用 `--shot`、`AWPR_NO_GPU=1` 等 headless 方式。
5. 影像幾何不隨介面縮放：100% 檢視 = 1 影像像素 : 1 實體螢幕像素；只有手把、判定半徑、線寬跟著 DPI 縮放。

## 建置 / 指令
```bash
cargo build --release                    # Windows / Linux（Rust ≥ 1.95）
scripts/build-macos.sh <libomp>          # macOS
target/release/AwayPhotoRawEditor        # 開上次的資料夾
target/release/AwayPhotoRawEditor --shot <資料夾> <out.png> [WxH] [第幾張]   # 離屏截圖後結束
target/release/awpr hashtest|gputest|viewtest|bench|stages|meta|info ...
cargo test --workspace
```
- `AWPR_NO_GPU=1` 強制 CPU；`AWPR_TRACE=<檔案>` 執行軌跡；`AWPR_SHOT_ADJ=...`、`AWPR_SHOT_ZOOM=...` 截圖時套調整。
- Windows 正式版沒有主控台，panic 寫到 `%TEMP%\awpr_crash.txt`；`--features console` 保留主控台。
- ⚠️ 本機防毒軟體會擋新建置的 exe（無聲結束、沒有紀錄；2026-10-09 使用者確認）。`--shot` 失敗時先懷疑這個，不要花時間查程式；請使用者把 `target` 加入防毒排除清單。
- 重建前先關掉殘留的 `AwayPhotoRawEditor.exe`（只砍自己啟動的 PID，不要用名稱砍）。

## 測試資料
- `D:\Awaysu\raw_samples\`：hashtest 的 4 個公開樣本（raw.pixls.us：`canon_eosr.CR3`、`leica_m10.DNG`、`pana_s5.RW2`、`sony_a7m3.ARW`），報告要與 `tests/results/windows-x86_64/` 逐位元組相同。
- `D:\Awaysu\raw_test\`：使用者提供的 RAW（含 61 MP ARW），只能拷貝出來用，不要改動原檔。

## 建置機（2026-10-09 起，免密碼 SSH 已設好，alias 在 `~/.ssh/config`）
| 平台 | alias | 工作目錄 | 工具鏈 |
|---|---|---|---|
| Linux x86_64（12 核） | `ssh awpr-linux` | `/home/awaysu/WorkspaceAwaysu/AwayPhotoRawEditor` | rustc 1.99（/usr/bin） |
| macOS 26.6 arm64（M2） | `ssh awpr-mac` | `/Users/awaysu/WorkspaceAwaysu/AwayPhotoRawEditor` | rustc 1.99（`export PATH=/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH`，非互動 shell 要自己加） |
- 兩台都在各自 WorkspaceAwaysu 下**新建** `AwayPhotoRawEditor` 目錄處理（用 `git clone` 或 rsync 本機 repo），不要動旁邊的 `AwayPhotoRawEditor_Rust_PoC`（PoC 建置檔與測試資料）。
- Mac 遠端 SSH 看不到桌面，視窗畫面要請使用者在機器前確認；Linux 可用 Xvfb。
- 密碼不寫進 repo；需要時問使用者。

## 版本號
正式版從 **2.0.0** 開始（使用者 2026-10-09 決定）；完成前 `Cargo.toml` 維持 `2.0.0-dev`。

## crate 配置
| crate | 內容 |
|---|---|
| `crates/libraw-sys` | LibRaw 0.22.2 原始碼 + C shim（`awpr_read_meta` 等） |
| `crates/core` | 模型、色彩科學、色調曲線、CPU 管線、縮圖、LibRaw 橋接 |
| `crates/gpu` | wgpu 管線（WGSL）、常駐 GPU 的 `GpuFrame`、viewer shader、直方圖 |
| `crates/photo` | `RAW_TEMP` 快取、`rawpipe.xml`、`preview_list.xml`、EXIF／TIFF 讀取、一般圖檔解碼 |
| `crates/app` | `AwayPhotoRawEditor` 執行檔（egui + wgpu） |
| `crates/cli` | `awpr` 診斷工具 |

## 已決定的事（2026-10-09，PM 依 PoC 報告的預設採用，使用者可推翻）
- 三平台**不**強求逐位元相同（LibRaw 在 x86 / arm64 的 `pow` 末位差，C# 與 Swift 版本來就有）。
- LibRaw 維持靜態連結（CDDL）。
- GPU 全圖上限維持 16 MP，之後有 60 MP 實測再調。
- HEIC 延後（需 libheif），放在打包之後評估。
- 開資料夾維持背景產生快取、可直接操作（不做 C# 的進度視窗）。

## 測試資料
- RAW 測試檔：`D:\Awaysu\raw_test`（ARW/CR3/NEF/RW2/DNG，含 61 MP）。測試時拷貝到 scratch 用，不改原檔。

## 參考來源
- 舊版 C#（行為規格）：`git show origin/legacy/windows:<path>`，重點檔 `src/Forms/MainForm.cs`、`src/Controls/ImageViewer.cs`、`src/Export/Exporter.cs`、`src/App/Localization.cs`、`src/Storage/*.cs`；其 `CLAUDE.md` 有大量踩雷紀錄（DPI、簽章、SmartScreen）。
- 舊版 Swift（`origin/legacy/macos`）：`AwayRawCore`、`RawLoader.swift`。
- 跨平台發佈流程：`C:\Users\AwayWork\Desktop\WORKSPACE2\AwayTerminal2`（`docs/RELEASE.md`、`scripts/release.mjs`；mac Developer ID 簽章＋公證、Linux 只發 .deb/.rpm、awaysu.cc `api.php?action=upload`，密碼只從環境變數讀）。
- lightcraft（MIT / Apache-2.0，可借程式碼，要寫進 THIRD-PARTY-NOTICES）：高光復原 `crates/raw/src/highlight.rs`、OkLCh HSL、XMP `crates/meta/src/xmp.rs`。比較報告：https://claude.ai/artifact/JR4KPWkMBEDHu62d2haKh2
