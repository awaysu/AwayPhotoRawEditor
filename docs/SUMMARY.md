# AwayPhotoRawEditor Rust 版：進度總結與建議（2026-10-08，第 3 步完成後更新）

目標：用一份 Rust 程式碼取代 C#（Windows）與 Swift（macOS）兩套程式，並新增 Linux。
做法參考 AwayTerminal 2.x 用 Rust 取代 C# 版的經驗，但介面用 **egui + wgpu 原生視窗**，不用 Tauri。

## 路線與進度

| 步驟 | 內容 | 狀態 |
|---|---|---|
| 1 | Rust 核心 + LibRaw，三平台技術驗證 | ✅ 完成（[`POC-STEP1.md`](POC-STEP1.md)） |
| 2 | wgpu GPU 管線（D3D12 / Metal / Vulkan） | ✅ 完成（[`POC-STEP2.md`](POC-STEP2.md)） |
| 3 | egui + wgpu 介面：開資料夾、縮圖列、GPU 直接畫大圖、滑桿即時更新、`rawpipe.xml` 相容 | ✅ 完成（[`POC-STEP3.md`](POC-STEP3.md)） |
| 3b | 其餘介面功能：裁切／漸層／修護的互動編輯、白平衡滴管、匯出、風格檔、批次、8 種語言、設定 | 未開始 |
| 4 | 打包（沿用 AwayTerminal 的發佈流程：簽章、安裝檔、DMG、Linux 套件） | 未開始 |
| 5 | 色彩管線升級（見下方「從 lightcraft 比較得到的建議」） | 未開始 |

原則：**先完全重現現有輸出（hashtest 逐位元相同），再升級色彩**。升級時照 v1.0.15 的「處理版本」機制，舊照片維持舊算式，直到使用者主動升級。

## 第 1 步結果：核心 + LibRaw

- **macOS**：Rust 與 Swift 版 `hashtest` 4 檔 × 14 組 = **56/56 逐位元相同**。
- **Windows**：Rust 的 LibRaw 解碼與 C# 版附的 `libraw.dll` **20 個檔案逐像素相同**（含 61 MP 的 7RM 系列），中文路徑可用。
- **Linux**：可建置、可跑完整管線，與 Windows 8-bit 輸出差 ≥2 的像素為 0。
- **效能**：Mac CPU 管線 8 個階段都不輸 Swift，解碼快 1.6×；Windows 解碼比 C# 版快約 2×（2.5 s → 1.1 s）。
- **跨平台差異**：Windows 與 Mac 之間每 1,300 萬通道約有 0–39 個差 ≥2，來自 LibRaw 在 x86 / arm64 上的解碼結果（推測是數學函式庫的 `pow` 最後一位不同）。**現有的 C# 版和 Swift 版之間本來就有這個差**，肉眼看不出來。

## 第 2 步結果：wgpu GPU 管線

同一份 GPU 程式碼（wgpu + WGSL）在三個平台的 GPU 上跑，輸出都和 CPU 版一致，精度和 Swift 版的 Metal 相同。

### 驗證結果

用的是 Swift `gputest` 的標準：14 組 hashtest 調整加 3 組修護，8-bit 值不能差 2 以上，差 1 的比例不能超過 0.07%。

| 平台 / GPU | 結果 | 最大差 | 差 ≥2 |
|---|---|---|---|
| Windows · Intel Iris Xe（D3D12） | 4 張公開樣本 + 4 個使用者檔案（含 7RM6 6100 萬像素），全部 17/17 | 1.41e-4 | 0 |
| Mac · M2（Metal） | 4 張公開樣本，全部 17/17 | 1.41e-4 | 0 |
| Linux · RTX 3060（Vulkan） | 4 張公開樣本，全部 17/17 | 1.56e-4 | 0 |

- **精度和 Swift 一樣**：同一台 M2、同一個檔案，Swift 自己的 gputest 最大差 1.52e-4、差 1 最多 0.051%；Rust 是 1.41e-4、0.046%。
- **修護確實有作用**：改動的像素數 18601 px，和 Swift 版的紀錄完全相同。
- **第 1 步沒有退步**：Mac 上 CPU 版仍和 Swift 56/56 完全相同。

### 速度

條件：2400 萬像素 Sony、綜合調整。照片打開時把預覽圖上傳到 GPU 一次，之後每次拉滑桿直接在 GPU 上算、直接畫在畫面上，不讀回 CPU。

| | 預覽圖 CPU → GPU | 全圖 CPU → GPU |
|---|---|---|
| Windows Iris Xe | 158 → 69 ms | 828 → 636 ms |
| Mac M2 | 62 → 25 ms（Swift Metal 22 ms，同一級） | 352 → 162 ms |
| Linux RTX 3060 | 114 → 7 ms | 622 → 61 ms |

在 M2 上和 Swift Metal 逐階段比，8 個階段裡 7 個 Rust 比較快，綜合調整差不多。

### 過程中發現的事

- **Windows 改用 D3D12**：wgpu 原本預設選 Vulkan，但在 Intel 內顯上 D3D12 快將近 3 倍，所以 Windows 現在優先用 D3D12（`WGPU_BACKEND` 可覆蓋）。
- **獨立顯卡會降頻**：RTX 3060 閒置後同一個運算要 5 ms，連續跑時只要 0.55 ms。測速改成連續跑 10 次取最快，比較接近拖滑桿時的狀況。設 `AWPR_GPU_PROFILE=1` 可看每個 kernel 的 GPU 時間。
- **修護比 Swift 慢，是目前唯一的落後**：Metal 有 CPU、GPU 共用的記憶體，wgpu 沒有，所以修護要把整張 70 MB 讀回 CPU 再上傳。M2 上 Swift 7 ms、Rust 25 ms。之後可以改成只傳修護圓附近的小範圍。
- **16 MP 上限可能太保守**：Swift 當初在 M2 量出 GPU 只處理 1600 萬像素以內的圖，但 2400 萬像素全圖在 M2 上 GPU 仍快 2.2 倍、RTX 3060 快 10 倍。要不要依 GPU 調整，等測過 6000 萬像素級的檔案再決定，目前維持原值。

## 第 3 步結果：編輯器視窗

- **三平台都能建置、執行**：Windows（Iris Xe／D3D12）、Linux（RTX 3060／Vulkan）實際開窗截圖；macOS（M2／Metal）建置與 shader 驗證通過，視窗畫面要在 Mac mini 前面確認（SSH 下看不到桌面）。
- **大圖直接從 GPU 畫**：GPU 管線和 egui 共用同一個 wgpu 裝置，算好的 buffer 由 viewer shader 直接畫到視窗，CPU 不碰像素。新增 `awpr viewtest` 把同一個 shader 畫到離屏貼圖和 CPU 參考比對：三平台 × 4 樣本、100%／200%／50%／37% 全部通過（沒有 8-bit 差 2）。
- **`rawpipe.xml` 與 C#／Swift 互通**：兩邊寫的 5 個檔案讀進來再寫回去**逐位元組相同**；你的 CR3（裁切＋漸層）、NEF（.f16＋調整）用 C# 版的快取直接開啟、照常套用，原檔沒被改動。
- **相機資訊不再依賴 ExifTool／ImageIO**：自寫的 TIFF 標籤讀取器＋LibRaw，三平台同一份程式；欄位與 Swift 版逐欄一致（DNG／RW2 的尺寸有小差異，見報告）。
- **介面**：照 C# 版版面與操作（滑桿雙擊回預設、滾輪微調、單擊循環縮放、拖曳平移、對照原圖、復原／重做、←→ 切換）。1080p＋150% 的螢幕照 C# 的規則自動縮小，整個版面都放得下。
- 非 RAW 格式（JPEG／PNG／TIFF／BMP）用 `image` crate；**HEIC 還沒做**（需要 libheif）。

## 從 lightcraft 比較得到的建議

出處：[AwayPhotoRawEditor 與 lightcraft 比較](https://claude.ai/artifact/JR4KPWkMBEDHu62d2haKh2)。
lightcraft 也是 Rust + egui + wgpu，架構和這次重寫的方向相同，可以直接參考它的做法（MIT / Apache-2.0 授權，可以借用程式碼）。

### 和 lightcraft 的主要差距（依影響大小），以及放在哪一步做

| # | 差距 | 建議做法 | 放在 |
|---|---|---|---|
| 1 | 工作色域只有 sRGB，高飽和色在 LibRaw 輸出時就被截掉；也沒有高光復原。這是畫質的天花板 | LibRaw 改輸出相機原生或線性寬色域（Rec.2020），32-bit 浮點處理；加高光復原（參考 lightcraft `crates/raw/src/highlight.rs`）。用新的處理版本 2，舊照片不受影響 | 第 5 步 |
| 2 | 沒有 HSL 和曲線 | HSL 用 OkLCh 色彩空間做（參考 lightcraft 的做法），曲線加在色調曲線之後 | 第 5 步 |
| 3 | 遮罩只有漸層 | 先加放射狀和筆刷遮罩；亮度、色彩範圍遮罩之後再說 | 第 5 步之後 |
| 4 | 編輯紀錄不是 XMP，也沒有圖庫 | 保留 `rawpipe.xml`（和現有兩版互通），另外加 XMP 匯出／匯入（參考 lightcraft `crates/meta/src/xmp.rs`）。圖庫維持以資料夾為單位 | 第 5 步之後 |
| 5 | Windows 版匯出是 8-bit 且沒有嵌 ICC | 匯出加 16-bit TIFF / PNG 並嵌入 ICC 描述檔 | 第 3 步做匯出時一起做 |

### 要保留的優點

- **相機色彩用 LibRaw 的相機矩陣**：lightcraft 自己寫解碼器，部分格式的相機色彩沒有校正過、CR3 和壓縮 RAF 目前只能開內嵌預覽。這次重寫繼續用 LibRaw，這是正確的選擇。
- **逐像素驗證**：hashtest / gputest 的比對方式要沿用到每一次改動，這是兩版能保持一致的原因。
- **8 種介面語言、浮水印、Windows 和 Mac 編輯檔互通**：都是 lightcraft 沒有的，Rust 版要保留。

### 要避免的

- lightcraft 大約一週寫了 11.8 萬行，作者自己也說標記為完成的功能還會出 bug。Rust 版維持「先對齊現有輸出、每步驗證」的節奏，不追功能數量。
- lightcraft 的 CI 只在發佈時跑。Rust 版建議第 4 步就加上 CI：每次改動都在三平台自動跑 `hashtest` 與單元測試。

## 待你決定的事

1. **三平台是否要逐位元相同**：要的話得替換 LibRaw 用的數學函式，代價是 Mac 版會和現有 Swift 版差幾個像素。
2. **LibRaw 授權**：目前是靜態連結（走 CDDL）；也可以改成動態連結（走 LGPL，和 C# 版相同）。
3. **GPU 尺寸上限**：16 MP 是否要依 GPU 記憶體調高。
4. **HEIC**：程式要不要附帶 libheif（LGPL，動態連結）；三台建置機目前都沒有。
5. **開資料夾的流程**：要不要像 C# 版先跳進度視窗、等快取全部做完；Rust 版目前在背景產生，可以直接操作。

## 檔案位置

- 專案：`AwayPhotoRawEditor_Rust`。介面在 `crates/app`、快取與 XML 在 `crates/photo`、GPU 程式在 `crates/gpu`，各步驟報告在 `docs/`，三平台的測試報告在 `tests/results/`。
- 只 commit 到本機 git，沒有推到 GitHub。
- Linux 和 Mac 的 `~/WorkspaceAwaysu/AwayPhotoRawEditor_Rust_PoC` 留有建置檔與測試資料（各約 1.9 GB），之後的步驟還會用到。
