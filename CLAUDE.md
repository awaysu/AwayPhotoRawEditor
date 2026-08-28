# AwayPhotoRawEditor — macOS

macOS 版的 AwayPhotoRawEditor：由 Windows 的 C# / .NET 8 / WinForms 版移植成 **Swift / AppKit**。
非破壞式 RAW 相片編輯器，所有調整以 XML 存於各資料夾的 `RAW_TEMP` 快取，**與 Windows 版格式完全相同**。

原始專案：`/Users/awaysu/gihub/AwayPhotoRawEditor`（C# 版，仍是**演算法的參考實作**）。
本專案的 `CLAUDE.md` 只寫 **macOS 版特有的事**；管線步驟、滑桿語意、色彩科學的「為什麼」請看 C# 版的 CLAUDE.md。

- **授權**：BSD 3-Clause，Copyright (c) 2026 Chih-Wei Su (Awaysu)
- **技術**：Swift 6（`swiftLanguageMode(.v5)`）/ AppKit 自繪 / SwiftPM / arm64 + x86_64
- **最低系統**：macOS 14
- **UI**：深色、全自繪控制項（與 C# 版的 `Controls/` 一一對應）

## 建置 / 執行

```bash
# 先建一次 LibRaw（只需做一次，或升級 LibRaw 時）
Scripts/build_libraw.sh

# 開發用
swift build
swift run awpr-cli selftest <圖片>

# 產生 .app（含簽章）
Scripts/build_app.sh
open build/AwayPhotoRawEditor.app
```

### 發佈 / 簽章 / 公證

```bash
# 一次做完：重建 → hardened runtime 簽章 → 公證 → stapler → DMG → DMG 也公證
Scripts/sign_and_notarize.sh "Developer ID Application: Chih-Wei Su (TEAMID)" awpr-notary
```

事前只需做一次（app-specific password 在 appleid.apple.com 產生，**不是** Apple ID 密碼）：

```bash
xcrun notarytool store-credentials awpr-notary \
    --apple-id awaysu@gmail.com --team-id <TEAMID> --password <app-specific-password>
```

- **⚠️ 順序不能顛倒**：內層 dylib 先簽 → .app 再簽 → 放進 DMG → DMG 再簽。
  簽章會改變檔案內容，所以**要公布的 SHA256 一定是全部簽完之後才算**（與 Windows 版同一條規則）。
- **`--options runtime`（hardened runtime）是公證的必要條件**，`Resources/AwayPhotoRawEditor.entitlements` 沒有放寬任何限制。
- **沒有沙盒**：程式要在使用者指定的任意資料夾旁邊寫 `RAW_TEMP` 快取。
  App Sandbox 下每個資料夾都要 security-scoped bookmark；Developer ID 散布不需要沙盒，所以不開。
  之後若要上 Mac App Store，才需要加 `com.apple.security.app-sandbox` + `user-selected.read-write`，並把重開資料夾用的 bookmark 存起來。
- 公證失敗時看原因：`xcrun notarytool log <submission-id> --keychain-profile awpr-notary`

## 與 Windows 版的差異（**移植決策，都是刻意的**）

| 項目 | Windows | macOS | 為什麼 |
|---|---|---|---|
| RAW 解碼 | LibRaw 0.22.2 DLL | **LibRaw 0.22.2**（自行建置） | 同版本 → 同像素。這是顏色能對得起來的前提 |
| 一般格式 | WIC | **ImageIO / CGImageSource** | 系統原生；順便多支援 HEIC |
| EXIF | ExifTool（外部行程） | **ImageIO 原生** | ExifTool 是 500+ 個未簽章的 Perl 檔，每個 Mach-O 都要簽才過得了公證，不值得 |
| GPU | Direct3D 12 / ComputeSharp | **尚未實作**（CPU only） | 使用者決定：先純 CPU，Metal 列為後續階段 |
| 高 DPI | `Ui.S()` 手動縮放全部版面 | **不需要** | macOS 以點為單位排版、Retina 由系統處理。字級仍可調（設定 → 字體大小） |
| 版面過高 | `DarkScrollHost` + `AutoFitScale` | **NSScrollView（overlay scroller）** | 同一個問題：右欄內容約 900pt，1040 高的視窗放不下 → 左右欄可捲動 |
| 語言/外觀切換 | `Application.Restart()` | **重新啟動 app** | 同樣的理由：字型與調色盤是開機時決定的快取靜態值 |

### 檔案格式**完全相容**（重要）

`RAW_TEMP/*.rawpipe.xml`、`preview_list.xml`、`presets.xml`、`export.xml`、`settings.xml`
一律沿用 .NET `XmlSerializer` 的格式，兩邊可以互相讀寫（同一張照片在 Windows 調完，複製到 Mac 接著調）。

`Storage/DotNetXml.swift` 就是為了這件事存在的。它刻意重現 .NET 的慣例：

- **元素順序＝C# 類別的宣告順序**（XmlSerializer 讀取時對順序敏感，改欄位兩邊要一起改）
- `null` 參考 → **整個元素省略**；空字串 → `<Name />`
- 布林是 `true` / `false`
- 陣列用型別名當子元素：`<PreMul><double>1.9</double>…</PreMul>`
- **數字用 .NET 的 round-trip 格式**：`5200`，不是 `5200.0`（`DotNetXml.string(_:)`）

> ✅ 已實證：`~/Library/Application Support/AwayPhotoRawEditor/export.xml` 是 **Windows 版寫的**
> （`<?xml version="1.0" encoding="utf-8"?>`，我們的 writer 不寫 encoding），Swift 端每個欄位都正確讀出。

設定檔位置：`%AppData%\AwayPhotoRawEditor\` → **`~/Library/Application Support/AwayPhotoRawEditor/`**。

## 架構（`Sources/`）

與 C# 版同構：UI → 服務 → 領域。領域層（`AwayRawCore`）**不依賴 AppKit**，所以 CLI 可以直接跑整條管線。

- **`CLibRawShim/`** — LibRaw C API 的薄殼。
  也順便把 C API 沒有 getter 的欄位（`sizes.flip`、可見區尺寸）**用真正的 struct 讀出來**——
  Windows 版只能靠 byte offset 硬算（`SizesFlipOffset = 40`），升級 LibRaw 就要重驗；這裡不必。
- **`AwayRawCore/`**
  - `Models/` — `ImageAdjustments`（**struct**，值語意）、`LinearGradient`、`HealSpot`、`CameraColorInfo`、`ExifData`、`PhotoItem`、`PresetProfile`
  - `Imaging/` — `FloatImageBuffer`、`ColorScience`、`ToneCurve`、`ImageProcessor`(+`+CPU`)、`LibRawBridge`、`ImageIOCodec`、`CacheManager`、`RawLoader`、`ImageStats`、`Watermark`、`PipelineUpgrade`
  - `Storage/` — `DotNetXml`、`AdjustmentXmlStore`、`PresetStore`、`PreviewListStore`
  - `Exif/ExifReader`、`Export/`、`App/`（`AppPaths`、`AppSettings`、`Localization`）
- **`AwayPhotoRawEditor/`** — `Controls/`（自繪）、`Panels/`、`UI/`（MainViewController + 對話框）
- **`awpr-cli/`** — headless 診斷（見下）

### 管線（`ImageProcessor`）

步驟順序只寫一次在 `runPipeline(_:_:_:)`，跑在 `StageTarget` 上。目前只有 `CPUTarget`；
**Metal 階段要加的是第二個 `StageTarget`，順序不必再寫一次**（這是照著 C# 版的 GPU 架構留的縫）。

CPU 實作是參考實作，**改算式時要與 C# 版一起改**。

## 診斷模式（`awpr-cli` 與 `--shot`）

Windows 版有 `--selftest` / `--shot` / `--dlgshot`；這裡拆成兩個執行檔。

| 指令 | 用途 |
|---|---|
| `awpr-cli info <img>` | LibRaw 可用性與 `libraw sizes`、EXIF、相機色彩資料 |
| `awpr-cli selftest <img> [report]` | 引擎端到端：解碼→EXIF→快取→管線（含逐階段）→histogram→XML 往返→風格檔 |
| `awpr-cli exporttest <img> <outDir> [report]` | 匯出全流程（含同名去重、EXIF 保留） |
| `awpr-cli render <img> <out.png> [--exposure N …]` | 單張套用調整後輸出 PNG |
| `awpr-cli bench <img>` | 各階段 proxy / 全解析度耗時 |
| `AwayPhotoRawEditor --shot <folder> <png> [waitMs] [WxH]` | 主畫面離屏截圖 |
| `AwayPhotoRawEditor --dlgshot <export\|settings\|presets\|about\|fonts\|firstrun\|progress> <png>` | 對話框離屏截圖 |

**`--shot` 是把 view 自己畫進 bitmap（`cacheDisplay`），不是螢幕截圖** —— 所以不需要「畫面錄製」權限，
SSH 或 CI 裡都能跑。**尺寸是設 view 的 frame 而不是視窗**：macOS 會把視窗夾進螢幕範圍，
走視窗的話結果會隨執行的螢幕而變。截完直接 `exit()`——背景快取工作否則會讓行程一直活著。

## 慣例 / 注意事項（踩過的坑）

- **⚠️ `install_name_tool` 會讓 dylib 的簽章失效，Apple Silicon 上未正確簽章的執行檔會被 SIGKILL**
  （crash report 寫 `Code Signature Invalid`）。所以 `build_app.sh` 改完 install name **一定要重簽**
  （沒給 `SIGN_IDENTITY` 時用 ad-hoc `-`，本機跑得起來就夠）。這是移植初期第一個真正的當機。
- **⚠️ 不要直接打包 Homebrew 的 libraw**：Homebrew 是照「建置那台機器的 macOS」編的
  （實測 `minos 26.0`），包進去會把 app 的最低系統一起拉高。`Scripts/build_libraw.sh` 自己編一份
  universal、`minos 14.0`、**沒有任何非系統相依**的版本。
- **LibRaw 直接用 clang 編，不走 configure**：configure 需要 pkg-config（本機沒有），
  而且直接編才能一行同時產出 arm64 + x86_64 並指定 deployment target。
  **要排除 `src/**/*_ph.cpp`** —— 那是「不含 postprocessing 的建置」用的 placeholder，
  會重複定義 `dcraw_process` 等符號。
- **`-DLIBRAW_NOTHREADS -DNO_JPEG -DNO_LCMS`**：少三個 dylib 要簽。代價是
  lossy DNG 等少數格式 LibRaw 解不了 —— 但 `RawLoader` 會依序退回「內嵌預覽 → ImageIO」，
  macOS 原生就認得那些格式，所以實際覆蓋率沒有損失。
- **`isFlipped` 要一路蓋到容器**：版面座標全是「由上往下」，但 `NSView` 預設原點在左下。
  只有根 view 翻轉不夠 —— `topBar` / `leftColumn` / `centerColumn` / `viewerBar` / `rightColumn` /
  `rightBottom` 都必須是 `FlippedView`，否則**子元件會整組上下顛倒**（移植途中實際踩到：
  左欄變成 風格檔 在最上面）。
- **`NSTableView` 放進沒有 frame 的 `NSScrollView` 會完全不畫**（data source 照樣被問列數）。
  風格檔清單因此改成自繪的 `DarkListView` —— 反正整個 app 都是自繪控制項，這樣也一致。
- **`ColorScience` 的兩張 LUT 是 `UnsafePointer<Float>`，不是 `[Float]`**：
  每個像素每個通道都要查表，用 `withUnsafeBufferPointer` 會在一次算圖裡進出上千萬次。
- **`FloatImageBuffer` 是 class 不是 struct**：管線多執行緒就地改寫，
  6000 萬像素的陣列如果有 copy-on-write 會直接毀掉效能。
- **`ImageAdjustments` 是 struct（值語意）**，與 C# 的 class 不同。
  因此面板改的是**自己那份複本**，owner 在 `onChanged` 時要**讀回來**（`self.adj = p.adjustments ?? self.adj`）。
  漏了這一步的症狀是「拉滑桿沒反應」。
- **`pipelineVersion` / `activeGradientIndex` 不參與 `valueEquals`**：
  與 C# 版一樣，舊版照片沒動滑桿不能被當成「已編輯」，`resetAll()` 也不能偷偷升級版本
  （`resetAll` 會把 version 存起來再放回去）。
- **多選批次同步的目標要在「編輯手勢開始」時擷取**（`pushUndo`），不能在提交時抓——
  單擊縮圖會先把選取變成單張才觸發載入，提交當下 `selectedItems` 已經不是原本的多選。
- **縮圖的白平衡要 rebase**：RAW 縮圖底圖是相機內嵌預覽、**相機白平衡已經烤在裡面**，
  所以 `renderThumbnail` 走 `whiteBalanceReference = .asShot`；沒有相機色彩資料時才用
  `5200 + (adj.temperature - exif.colorTemperature)` 的偏移法。照原值算會把白平衡套第二次。
- **刪除照片走垃圾桶（`trashItem`）**，不是 `removeItem`。
- **`CancelToken` 是協作式的**：背景算圖被新的取代時只是設旗標，管線在階段邊界檢查。

## 快取檔（各資料夾 `RAW_TEMP/`）

`{file}_thumb.jpg`（縮圖）、`{file}.rawpipe.png`(+`.f16`)（proxy）、
`{file}.rawpipe.xml` / `{file}.copyN.rawpipe.xml`（調整）、`preview_list.xml`（隱藏 + 虛擬副本）。

`.f16` 是 16-bit 整數（`AP16` magic），與 Windows 版同一個格式。舊的 `.f32` 裝的是 8-bit 量化值，
**換副檔名讓它自然失效**，兩種都會被 `deleteCacheFiles` 清掉。

## 尚未完成 / 後續

- **Metal 加速**（使用者指定的第二階段）：加一個 `StageTarget` 實作即可，步驟順序不必重寫。
  C# 版的 `GpuShaders.cs` 是逐行對照 CPU 函式寫的，可以直接當 Metal shader 的藍本。
- **實機 RAW 尚未驗證**：這台機器上沒有 RAW 檔，`selftest` / UI 都是用 JPEG 跑過的。
  拿到 ARW / NEF / CR3 / RAF 後請跑 `awpr-cli info` 確認 `libraw sizes` 有裁切表、
  再跑 `selftest` 看相機色彩資料有沒有讀到。
- **與 C# 版的逐像素對照測試**還沒做（需要兩邊都能跑的機器）。
- 更新檢查（Windows 版的 `UpdateCheck`）尚未移植。
