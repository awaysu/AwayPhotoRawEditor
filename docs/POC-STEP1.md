# 第 1 步技術驗證：Rust 核心 + LibRaw 三平台（2026-10-08）

目標：證明 AwayPhotoRawEditor 的色彩管線可以用 **一份 Rust 程式碼** 在 Windows、macOS、Linux 跑，
而且結果和現有的 C#（Windows）與 Swift（macOS）版本一致。介面（egui）與 GPU（wgpu）不在這一步。

## 結論

| 驗證項目 | 結果 |
|---|---|
| macOS：Rust 與 Swift 版 `hashtest` | ✅ **4 檔 × 14 組 = 56/56 SHA 逐位元相同**（通道均值、取樣點也全同） |
| Windows：Rust 的 LibRaw 解碼與 C# 版附的 `libraw.dll` | ✅ **20 個可解的檔案逐像素相同**（Sony ×12、Canon ×2、Panasonic ×2、Leica/Adobe DNG ×2…，含 61 MP 的 7RM 系列） |
| Windows：管線算式與 C# 一致 | ✅ 推論：管線在 macOS 已與 Swift 逐位元相同（Swift 又與 C# 98/98 相同），且所有 LUT、白平衡矩陣在三平台的雜湊都相同（`awpr stages`） |
| Linux：可建置、可解碼、可跑完整管線 | ✅ Ubuntu 24.04 x86_64，與 Windows 版 8-bit 輸出差 ≥2 的像素為 0 |
| 中文路徑（Windows） | ✅ `測試照片 資料夾/風景_01.ARW` 結果與英文路徑相同（shim 在 Windows 改走 `libraw_open_wfile`） |
| 效能 | ✅ macOS CPU 管線 8 個階段都 ≥ Swift；解碼比 Swift 快 1.6×；Windows 解碼比 C# 版的 dll 快 ~2× |

**沒有做到的**：三個平台**彼此**逐位元相同。見下面「跨平台差異」。

## 跨平台差異（這次驗證新發現的事實）

管線本身三平台完全相同（`stages` 的 LUT / 矩陣雜湊一致）。差異全部來自 **LibRaw 在不同平台編譯後的解碼結果**：

| 比較 | 8-bit 輸出 | 說明 |
|---|---|---|
| Windows vs Linux（都 x86_64） | 差≥2：**0**；差 1：0.0000% | Sony 14/14 SHA 相同；CR3/RW2/DNG 有 1e-9 級的差（MSVC vs GCC 編 LibRaw） |
| Windows vs macOS（x86 vs arm64） | 每 1,300 萬通道約 **0–39 個差≥2**；差 1 約 0.002–0.006% | 少數像素差到 0.05，看起來是 AHD 去馬賽克在極少數像素選了不同方向 |

- 這個 Windows vs macOS 的差異**現有兩個產品之間本來就有**：Rust-Windows 解碼 = C# 的 libraw.dll、Rust-macOS = Swift，
  所以 C#（Windows）與 Swift（macOS）兩個上架版之間就是這個差。Swift 版 CLAUDE.md 寫的「與 Windows 版本身的逐像素對照…推論相同但沒直接跑過」，
  實測是**不完全相同**（肉眼不可見）。
- 排除過的原因：OpenMP（開關不影響 x86 結果）、FMA 融合（macOS 關掉 `-ffp-contract` 結果會變，但仍不等於 x86）。
- **尚未證實**的推測：Apple libm 與 glibc/UCRT 的 `pow` 等函式最後一位不同（LibRaw 的 AHD 建 Lab 查表用 `pow`），
  在少數像素翻轉了方向判斷。要證實需要在 Mac 用 Rosetta 跑 x86_64 版（這台 Mac 沒裝 Rosetta，沒有替使用者安裝）。
- 若將來要三平台逐位元相同：可以讓 LibRaw 改用同一套數學函式（例如替換 `pow`/`log`/`cbrt` 為可攜實作）。代價是 Mac 版會和現有 Swift 版差這幾個像素。**這是產品決定，先不做。**

## 效能（24 MP Sony A7M3，2560 proxy，暖機後三次取最好）

macOS（Apple M2，8 核）— 與 Swift 版 `awpr-cli bench` 同條件、同 8 組調整：

| 階段 | Swift proxy | **Rust proxy** | Swift 全圖 | **Rust 全圖** |
|---|---:|---:|---:|---:|
| 全解析度解碼 | — | — | 754 ms | **472 ms** |
| 白平衡+曝光 | 21 | **16** | 113 | **88** |
| 色調曲線 | 20 | **16** | 112 | **87** |
| 降噪+銳利化 | 52 | **42** | 311 | **237** |
| 漸層 | 46 | **39** | 259 | **214** |
| 暗角 | 24 | **20** | 134 | **108** |
| 裁切+角度 | 31 | **29** | 171 | **160** |
| 廣角變形 | 33 | **31** | 184 | **170** |
| 綜合 | 65 | **62** | 359 | **352** |

（Swift 的 Metal GPU 路徑 proxy 綜合 22 ms——GPU 是第 2 步 wgpu 的事。）

Windows（i5-13500H 筆電，16 執行緒）：解碼 C# 版 libraw.dll **2.5 s → Rust 1.1 s**（61 MP：8.0 s → 4.3 s），綜合 proxy 146 ms / 全圖 838 ms。
Linux（12 執行緒）：解碼 791 ms，綜合 proxy 120 ms / 全圖 651 ms。

最佳化都**不改變任何一個數值**（每次改完都重跑 hashtest，與改前逐字元相同）：
像素階段融合成一次掃描（原本每階段各掃一次記憶體）、複製來源併入第一個階段、LUT 查表去掉邊界檢查、模糊改成整列累加（每個像素的加法順序不變）。

## 踩到的坑

- **macOS 的 LibRaw 要 OpenMP 才快**：Apple clang 沒有 OpenMP runtime。連 **Homebrew 的靜態 `libomp.a` 會在 LibRaw 第一個 `omp critical` 當掉**
  （`__kmp_acquire_ticket_lock` 空指標；macOS 的 crash report 在 `~/Library/Logs/DiagnosticReports/awpr-*.ips`，SSH 下 lldb 不能附加）。
  改用 Swift 版 `Scripts/build_libomp.sh` 編的 universal / minos 14 **動態** `libomp.dylib` 就正常——也就是現有 Swift app 打包的同一個。`scripts/build-macos.sh`。
- **Windows 路徑**：LibRaw 的 `libraw_open_file` 在 Windows 讀 ANSI code page，中文資料夾會開不了 → shim 轉 UTF-16 走 `libraw_open_wfile`。
- **Sony 7RM5/7RM6 的遮罩黑邊**：`decode_full(…, None)` 會自動裁（10240×7168 → 10017×6673），和 C# / Swift 的 hashtest 一致；
  比對 dll 要用 `decode_full_untrimmed`。
- **Nikon Z8 HE 壓縮的 NEF**：LibRaw 0.22.2 解不了（C# 版同樣解不了；Swift 版退回 ImageIO）。Rust 版之後需要自己的退回方案。
- PowerShell 5.1 呼叫 plink 會吃掉參數裡的雙引號；遠端指令裡別用 `"`，或寫成腳本再傳上去。

## LibRaw 的建置方式

`crates/libraw-sys` 從 `vendor/LibRaw-0.22.2`（未修改的官方原始碼）直接用 `cc` 編成靜態庫，三平台同一組 define：
`NO_JASPER NO_JPEG NO_LCMS USE_ZLIB`（與 Swift 版 `build_libraw.sh` 相同）。zlib 用 `libz-sys` 的內建版本，三平台同一份。

- 授權：LibRaw 為 LGPL-2.1 / CDDL-1.0 雙授權。**靜態連結是依 CDDL-1.0**（檔案層級 copyleft，原始碼未修改、隨附於 `vendor/`）。
  若偏好照舊用 LGPL 動態連結，可改成編成 dll/dylib/so——**這是要使用者決定的事**。
- OpenMP：Windows（MSVC `/openmp`）、Linux（`-fopenmp` + libgomp）預設開；macOS 需 `AWPR_LIBOMP_DIR`（見上）。

## 怎麼重跑

```bash
# Windows / Linux
cargo build --release
target/release/awpr hashtest <raw> report.txt
target/release/awpr bench <raw>

# macOS（帶 OpenMP）
scripts/build-macos.sh ~/WorkspaceAwaysu/AwayPhotoRawEditor_Swift/ThirdParty/libomp

# 與 Swift 報告逐行對照（略過「平台」「執行階段」兩行）
scripts/hashtest-diff.sh target/release/awpr <samples 資料夾> tests/reference/swift-macos <輸出資料夾>

# Windows：與 C# 版的 libraw.dll 逐像素對照
target/release/awpr dllcheck <AwayPhotoRawEditor>/tools/libraw/LibRaw-0.22.2/bin/libraw.dll <raw>

# 跨平台：在 A 平台 dumpsrc，拿到 B 平台 cmpsrc（gputest 標準：8-bit 差≥2 不允許、差1 ≤ 0.07%）
awpr dumpsrc <raw> a.f32 ;  awpr cmpsrc <raw> a.f32
```

測試檔：raw.pixls.us 的公開樣本（Sony ILCE-7M3、Canon EOS R CR3、Panasonic DC-S5、Leica M10 DNG），
加上使用者的 `D:\Awaysu\raw_test`（只在 Windows 用來比對 dll，沒有複製到別台）。
參考報告在 `tests/reference/swift-macos/`（Swift `awpr-cli hashtest` 在 Mac mini M2 產生），三平台結果在 `tests/results/`。

## 下一步（第 2 步）

1. wgpu 版的管線（WGSL），用 Swift `gputest` 的 8-bit 標準對照 CPU。
2. 決定：跨平台逐位元相同要不要做（見上）、LibRaw 靜態（CDDL）或動態（LGPL）。
3. 非 RAW 格式（JPEG/HEIC/TIFF）的解碼：要選跨平台的 crate，現有兩版各用 WIC / ImageIO。
4. `rawpipe.xml` 讀寫（與 .NET XmlSerializer 相容，Swift 的 `DotNetXml.swift` 是規格）。
