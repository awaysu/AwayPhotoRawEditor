# CLAUDE.md — AwayPhotoRawEditor（Rust 跨平台版）

AwayPhotoRawEditor 以 **Rust 重寫為 Windows / macOS / Linux 三平台版**，取代 C#（`legacy/windows`）與 Swift（`legacy/macos`）兩套程式。
介面用 **egui + wgpu 原生視窗**（不用 Tauri，大圖直接從 GPU buffer 畫）；RAW 解碼用 LibRaw（原始碼直接編進去）。

- 授權 BSD-3-Clause，Copyright (c) 2026 Chih-Wei Su (Awaysu)。LibRaw 靜態連結走 CDDL-1.0。
- 顯示名稱一律「AwayPhotoRawEditor」。對使用者一律用繁體中文。
- 下載頁：https://www.awaysu.cc/software/awayphotoraweditor ／ GitHub Releases：https://github.com/awaysu/AwayPhotoRawEditor/releases

## 目前狀態
- **1.1.1 已發佈（2026-10-11，tag `v1.1.1`，awaysu.cc + GitHub Release，三平台皆簽章／公證）**：介面第三～六輪（TASK-014～017）。1.1.0（2026-10-10）是 Rust 版第一個正式版，接續 C# 1.0.18。
- 重寫的第 0–10 步全部完成（2026-10-09），步驟內容與 commit 見 `docs/ROADMAP.md`，各步報告在 `docs/`。
- 之前短暫發過 2.0.0／2.0.1，已依使用者決定撤回：GitHub tag／Release 刪除、awaysu.cc 的版本說明與封存檔清除。git 歷史裡仍有 2.0.x 的 commit 訊息，不要再改。
- 之後為維護與新需求；下一版從 **1.1.2**（修正）或 **1.2.0**（新功能）起。
- PoC 原本在 `C:\Users\AwayWork\Desktop\WORKSPACE2\testAwayPhotoRawEditor\AwayPhotoRawEditor_Rust`（第 1–3 步，三平台驗證過），已不再使用。

## 工作方式（Multi-Agent）
- Agent-11（PM）規劃、拆任務、審查、發佈；Agent-12（Software Engineer）寫程式。任務編號 `TASK-NNN`（最後一個是 TASK-017），信箱 `.ai/bus/`（最後一封 0065）。每個任務：Engineer commit（不 push）→ PM 審查截圖與測試 → 修正 → PM push。
- PM 不改程式碼，但可以跑打包腳本、上傳、建 Release、改文件。
- 處理版本對照：XML `PipelineVersion` 0／1／2 ＝ 使用者看到的「處理版本 1／2／3」；版本 3 走 `crates/core/src/v3.rs`，GPU 對照用 `awpr gputest3`。
- 大項目完成時更新本檔的「目前狀態」與 `docs/ROADMAP.md`，並在 `CHANGELOG.md` 記錄（新版在上）。
- 介面需求一律附 `--shot` 截圖前後對照，放 `docs/images/ui-<版本>-*.jpg`；使用者的螢幕是 1920×1080、150%（邏輯 1280×720），以這個尺寸驗證版面。

## 不可違反的原則
1. **先完全重現現有輸出，再升級色彩**：`crates/core` 的浮點運算順序刻意與 C# / Swift 一字不差，改算式要三邊一起改並重跑 `scripts/hashtest-diff.sh`、`awpr gputest`。色彩升級一律用新的「處理版本」，舊照片維持舊算式。
2. **`rawpipe.xml` / `preview_list.xml` / 設定檔與 C#、Swift 版互通**：改 `crates/photo/src/store.rs` 或 `crates/app/src/settings.rs` 的欄位或順序前先對照 `AdjustmentXmlStore.cs` / `.swift`；新欄位加在既有欄位之後；`cargo test -p awpr-photo` 的逐位元組往返測試必須保持通過。
3. **每一步都要有可重複的驗證**（單元測試、hashtest / gputest / viewtest、`--shot` 離屏截圖），不能只靠「看起來正常」。
4. **不要做需要前景視窗或鍵盤焦點的測試**（桌面是共用的）；用 `--shot`、`AWPR_NO_GPU=1` 等 headless 方式。
5. 影像幾何不隨介面縮放：100% 檢視 = 1 影像像素 : 1 實體螢幕像素；只有手把、判定半徑、線寬跟著 DPI 縮放。
6. 密碼不寫進 repo、不寫進訊息、不 echo：awaysu.cc API 密碼只從 `D:\Awaysu\web_info.txt`「網頁登入密碼為…」那行讀進 `AWAYSU_API_PASSWORD`；Mac 鑰匙圈密碼只經 `AWPR_KEYCHAIN_PASSWORD`。
7. `git add` 只加自己改的檔，不用 `-A`／`.`；不用名稱砍行程（只砍自己啟動的 PID）；`D:\Awaysu\raw_test` 的原檔不可改動。

## 建置 / 指令
```bash
cargo build --release                    # Windows / Linux（Rust ≥ 1.95）
scripts/build-macos.sh <libomp>          # macOS
target/release/AwayPhotoRawEditor        # 開上次的資料夾
target/release/AwayPhotoRawEditor --shot <資料夾> <out.png> [WxH] [第幾張]   # 離屏截圖後結束
target/release/awpr hashtest|gputest|gputest3|viewtest|v3cmp|bench|stages|meta|info|heictest|exporttest ...
cargo test --workspace                   # 1.1.1：76 項
```
- 環境變數：`AWPR_NO_GPU=1` 強制 CPU；`AWPR_TRACE=<檔案>` 執行軌跡；`AWPR_UI_SCALE=1.5`、`AWPR_LANG=de` 模擬縮放／語言；截圖用 `AWPR_SHOT_ADJ="exposure=1,hlr=100,version=3"`、`AWPR_SHOT_ZOOM`、`AWPR_SHOT_TOOL`、`AWPR_SHOT_COLOR_TAB`、`AWPR_SHOT_HSL_TAB`、`AWPR_SHOT_SCROLL=bottom`、`AWPR_SHOT_XMP=1`、`AWPR_SHOT_STRIP_SCROLL=<點數>`、`AWPR_SHOT_DLG=settings-custom|close`（鍵名見 `crates/app/src/app.rs` 的 `apply_adjust_spec`）。
- Windows 正式版沒有主控台，panic 寫到 `%TEMP%\awpr_crash.txt`；`--features console` 保留主控台。
- ⚠️ 本機防毒軟體會擋新建置的 exe（無聲結束、沒有紀錄；留下 `.tmp` 快取檔是典型症狀）。`--shot` 失敗時先懷疑這個，不要花時間查程式；請使用者暫時關閉或把 `target` 加入排除清單。
- 重建前先關掉殘留的 `AwayPhotoRawEditor.exe`（只砍自己啟動的 PID）。
- Linux 截圖：建置機上 `xvfb-run -a -s "-screen 0 1920x1080x24" target/release/AwayPhotoRawEditor --shot …`。Mac 在 SSH 下 **無法** `--shot`（沒有 GUI session，建不出 Metal 裝置，2026-10-10 實測卡死）；Mac 畫面要請使用者在機器前看。

## 介面版面（1.1.1）
頂部工具列（大按鈕，「匯出…」在最右、藍色）→ 右欄從工具列下方到視窗底（「直方圖與照片資訊」一區（無處理版本列）、工具四分頁（遮罩清單是下拉）；底部固定：全部重設／恢復上一步／重做）；縮圖列（144）只橫跨左欄與影像區；左欄（基本調整、色彩三分頁、細節、風格檔）；影像區底部檢視工具列。側欄寬 `SIDE_W = 312`；每個滑桿「名稱｜滑桿｜數值」一列不換行；開工具時照片資訊維持完整表格（1.1.1）、直方圖變矮（32）；沒開工具時工具區只有四個分頁按鈕。1920×1080／150% 下，沒開工具、修護時右欄不出現捲軸；裁切、漸層與遮罩會出現捲軸，使用者 2026-10-10 決定接受（方案 D，見 .ai/bus/0053）。

## 測試資料
- `D:\Awaysu\raw_samples\`：hashtest 的 4 個公開樣本（raw.pixls.us：`canon_eosr.CR3`、`leica_m10.DNG`、`pana_s5.RW2`、`sony_a7m3.ARW`），報告要與 `tests/results/windows-x86_64/` 逐位元組相同。
- `D:\Awaysu\raw_test\`：使用者提供的 RAW（ARW/CR3/NEF/RW2/DNG，含 61 MP ARW），只能拷貝出來用，不要改動原檔。
- 建置機上的樣本：Linux `~/WorkspaceAwaysu/raw61/0958900946.arw`，Mac `~/WorkspaceAwaysu/0958900946.arw`。

## 建置機（免密碼 SSH，alias 在 `~/.ssh/config`）
| 平台 | alias | Agent-12 工作目錄 | 工具鏈 |
|---|---|---|---|
| Linux x86_64（12 核，RTX 3060） | `ssh awpr-linux` | `/home/awaysu/WorkspaceAwaysu/AwayPhotoRawEditor` | rustc 1.99（/usr/bin） |
| macOS 26.6 arm64（M2） | `ssh awpr-mac` | `/Users/awaysu/WorkspaceAwaysu/AwayPhotoRawEditor` | rustc 1.99（`export PATH=/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH`，非互動 shell 要自己加） |
- 兩台的 `AwayPhotoRawEditor` 是 Agent-12 的工作目錄（origin 是 GitHub）；**發佈建置用旁邊的 worktree `AwayPhotoRawEditor-rel`**（`git fetch --tags origin && git checkout v<版本>` 後跑打包腳本），不要動 Agent-12 的工作目錄，也不要動 `AwayPhotoRawEditor_Rust_PoC`。
- **Mac 簽章／公證要在同一個 SSH 指令裡解鎖登入鑰匙圈**（解鎖狀態跟 security session 走；密碼從 `D:Awaysumac_keychain.txt` 第一行讀進環境變數，2026-10-11 起）：`AWPR_KEYCHAIN_PASSWORD=… AWPR_SIGN_IDENTITY="Developer ID Application: Chih-Wei Su (BNH8YS88T9)" AWPR_NOTARY_PROFILE=AwayTerminalNotary scripts/package-macos.sh`。公證約 3–5 分鐘，用背景工作等。
- 不要動 Mac 上鎖住的 `awpr-signing`／`ios-signing` 鑰匙圈。

## 發佈流程（1.1.0 實跑過，細節在 `docs/RELEASE.md`）
1. 版本在 `Cargo.toml`（`version_has_one_source` 測試）；`CHANGELOG.md` 新段在最上；`docs/RELEASE-NOTES-<版本>.md`；README「目前版本」。
2. push main、`git tag -a v<版本>`、push tag。
3. Windows：`powershell -ExecutionPolicy Bypass -File scripts/package-windows.ps1`（簽章指紋 997D278FE3FD6FFA1F8E43683047530DE7210C66，含時間戳）→ `dist/`。Linux：`-rel` worktree 跑 `scripts/package-linux.sh`。Mac：上面那行。三個可同時跑。
4. awaysu.cc：每檔 `api.php?action=upload`（exe、zip、dmg、deb、rpm），changelog **只帶該版本段落**用檔案帶入（`-F "changelog=<檔案"`；檔尾「---」後的 legacy 說明會被解析成另一個版本條目，2026-10-10 發生過，用 `delete_changelog` 清掉）。上傳會自動把被取代的檔封存到 old_versions。驗證 `check_update` 三平台。
5. `gh release create v<版本> --notes-file docs/RELEASE-NOTES-<版本>.md <5 個檔> dist/SHA256SUMS-<版本>.txt`。CI 的 release.yml 只上傳 unsigned artifact，不碰 Release 資產。
6. 更新本檔「目前狀態」、`docs/ROADMAP.md`，commit push，INFO 給 Agent-12。
- 把舊檔放進網站「舊版本」：API 沒有直接動作，用「備份現行檔 → upload 舊檔（帶 download_id，不帶 version）→ upload 原檔換回 → delete_old_version 多出的現行版歸檔 → keep_old_version」（2026-10-11 1.0.19.dmg 實跑過）。
- 撤回版本：`gh release delete --cleanup-tag`、`git tag -d`；awaysu.cc 用 `delete_changelog`（version）＋ `delete_old_version`（id 從 `old_versions` 取）；現行下載檔只能用新版上傳取代。

## 版本號與安裝程式
- 接續 C# 的 1.0.18，Rust 版從 **1.1.0** 開始（使用者 2026-10-10 決定）。
- Windows 安裝檔（Inno Setup，`installer/AwayPhotoRawEditor.iss`）沿用 C# 的 AppId `{8E1A2C64-5A17-4D0B-9C67-AWPRE0100001}`，per-user 裝在 `%LOCALAPPDATA%\Programs\AwayPhotoRawEditor`，`[Code] PrepareToInstall` 先靜默執行舊版（C# 1.0.x、2.0.x `{9063DED4…}`）的解除安裝程式；設定、風格檔、`RAW_TEMP` 不受影響。桌面捷徑預設不勾。安裝語言繁中／簡中／英日韓德法西。
- mac bundle id `com.awaysu.awayphotoraweditor`；deb/rpm 套件名 `awayphotoraweditor`。

## crate 配置
| crate | 內容 |
|---|---|
| `crates/libraw-sys` | LibRaw 0.22.2 原始碼 + C shim（`awpr_read_meta`、`awpr_decode_linear` 等） |
| `crates/core` | 模型、色彩科學、色調曲線、CPU 管線（`run_local`／`run_tail`）、處理版本 3 `v3.rs`、遮罩 `masks.rs`、縮圖、LibRaw 橋接 |
| `crates/gpu` | wgpu 管線（WGSL `PIXEL_V3`、`MASKS`、`apply_tiled`）、常駐 GPU 的 `GpuFrame`、viewer shader、直方圖 |
| `crates/photo` | `RAW_TEMP` 快取（`.rawpipe.v3.png` 16-bit 線性 proxy、兩階段背景產生）、`rawpipe.xml`、`preview_list.xml`、XMP `xmp.rs`、HEIC `heif.rs`／`heic.rs`、EXIF／TIFF、一般圖檔解碼 |
| `crates/app` | `AwayPhotoRawEditor` 執行檔（egui + wgpu）；`widgets::adjust_slider`、`settings.rs`、`i18n_table.rs`（8 語，掃描測試） |
| `crates/cli` | `awpr` 診斷工具 |

## 已決定的事（使用者可推翻）
- 三平台**不**強求逐位元相同（LibRaw 在 x86 / arm64 的 `pow` 末位差，C# 與 Swift 版本來就有）。
- LibRaw 維持靜態連結（CDDL）。
- GPU 全圖上限 16 MP，超過走 `apply_tiled` 分塊。
- HEIC 用系統解碼器（macOS ImageIO、Windows WIC 需 HEIF＋HEVC 延伸模組、Linux dlopen libheif）；Windows WIC 路徑尚未在有 HEVC 延伸模組的機器上驗證。
- 開資料夾維持背景產生快取、可直接操作（不做 C# 的進度視窗）；選到的照片插隊到佇列最前。
- 縮圖同步即時（200 ms debounce）。
- XMP 匯出／匯入預設隱藏，設定「支援 XMP」打開才出現。
- deb/rpm 不加 epoch（2.0.0 的 Linux 套件只有我們自己裝過）。
- CI 的 hashtest 步驟因 `AWPR_SAMPLES_URL` secret 未設而跳過。

## 參考來源
- 舊版 C#（行為規格）：`git show origin/legacy/windows:<path>`，重點檔 `src/Forms/MainForm.cs`、`src/Controls/ImageViewer.cs`、`src/Export/Exporter.cs`、`src/App/Localization.cs`、`src/Storage/*.cs`、`installer/AwayPhotoRawEditor.iss`；其 `CLAUDE.md` 有大量踩雷紀錄（DPI、簽章、SmartScreen）。
- 舊版 Swift（`origin/legacy/macos`）：`AwayRawCore`、`RawLoader.swift`。
- 跨平台發佈流程的原型：`C:\Users\AwayWork\Desktop\WORKSPACE2\AwayTerminal2`（`docs/RELEASE.md`、`scripts/release.mjs`）。awaysu.cc API 說明在 private repo `awaysu/software-web` 的 `readme_for_program.txt`。
- lightcraft（MIT / Apache-2.0，已借用並列在 THIRD-PARTY-NOTICES）：高光復原、OkLCh HSL、XMP。比較報告：https://claude.ai/artifact/JR4KPWkMBEDHu62d2haKh2
