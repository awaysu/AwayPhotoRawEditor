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
| GPU | Direct3D 12 / ComputeSharp | **Metal**（見下方「GPU 加速」） | 同樣的 shader 對照 CPU 參考實作逐行寫 |
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

> ✅ **已實證（2026-08-28）**：`/tmp/raw_test/RAW_TEMP/` 內是 **`AwayPhotoRawEditor_mac`（C# 版 macOS port）寫的**
> `.rawpipe.xml`（`<?xml version="1.0" encoding="utf-8"?>` —— 我們的 writer 不寫 encoding，可據此分辨）。
> Swift 端每個欄位都正確讀出，**且原樣寫回時 16 位數的 double 逐位元相同**。
> `~/Library/Application Support/AwayPhotoRawEditor/export.xml` 同樣是 C# 版寫的，也讀得正確。
> ⚠️ 這證明的是**與 C# 實作**相容；與 Windows 版本身的相容性推論自「兩邊都是 .NET XmlSerializer」，尚未直接驗證。

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
| `AwayPhotoRawEditor --uitest <folder> [report]` | **UI 流程測試**：把資料夾複製到暫存區，驅動真正的 `MainViewController` 跑 載入→編輯→復原／重做→切圖存檔→多選批次同步與批次復原→風格檔→重設→裁切比例→旋轉→虛擬副本→複製貼上→隱藏／顯示全部／還原→刪除副本→關閉存檔，逐項斷言（含磁碟上的 XML）。截圖測不到的邏輯都在這裡。JPEG 與 RAW 資料夾都要跑。|

**`--shot` 是把 view 自己畫進 bitmap（`cacheDisplay`），不是螢幕截圖** —— 所以不需要「畫面錄製」權限，
SSH 或 CI 裡都能跑。**尺寸是設 view 的 frame 而不是視窗**：macOS 會把視窗夾進螢幕範圍，
走視窗的話結果會隨執行的螢幕而變。截完直接 `exit()`——背景快取工作否則會讓行程一直活著。

## 與 Windows 版功能對照（2026-08-29 全面比對後補齊）

拿 Windows 版的 `MainForm` 選單／快捷鍵／右鍵選單／各對話框逐項對過。以下是補齊的：

- **重做**（Windows Ctrl+Y → macOS ⌘⇧Z）：`redoStack`，新編輯清空 redo；只重播單張、不重播批次同步（同 Windows）。undo 上限 80。
- **紀錄**（☰ → 紀錄 ▸ 最近 20 個資料夾，`清除紀錄`）：存 settings.xml 的 `<RecentFolders><string>…`，與 Windows 同格式。資料夾不存在時提示。
- **快捷鍵**：⌫ = 隱藏且不輸出、⇧⌫ = 刪除檔案、F5 = 重新整理、Esc、←→、`\`。
- **縮圖右鍵選單**照 Windows 順序：全選／反向選擇／取消全選、套用風格檔 ▸、複製／貼上照片設定、升級處理版本、建立副本、隱藏且不輸出／取消隱藏、刪除檔案、不顯示隱藏／顯示全部（勾選）、匯出照片。
- **雙擊縮圖**強制重新載入。**拖曳資料夾（或照片）到視窗**即開啟。
- **隱藏的照片一律不匯出**（匯出全部／選取／目前皆同），沒有可匯出的就提示。
- **非 RAW 照片的色溫滑桿**是 ±100（= 5200 ± 3000 K，`ColorPanel.nonRawScale`），RAW 才是 Kelvin。`rebindAll` 依照片切換。
- **比例下拉改變時重排裁切框**（`applyCropAspect`，含旋轉後的畫面比例）。
- **白平衡滴管**改用 Windows 的 `EstimateWhiteBalance` 演算法（紅藍平衡搜尋＋綠色偏移算 tint），並依 proxy 的解碼來源選對參考基準。
- **設定**：恢復預設、字體大小… 移進設定視窗（按「套用」才寫）。**匯出**：儲存設定（不匯出）、浮水印即時預覽。**關於**：版本／編譯時間／作者／下載／原始碼／第三方／授權，檢查更新改為對話框並可直接開下載頁。
- **支援RAW檔相機列表**連結、還原已隱藏的照片顯示張數、狀態列用 Windows 的「LibRaw 讀取中／已啟用」字樣。
- 診斷用環境變數 `AWPR_UI_LANGUAGE` / `AWPR_UI_STYLE`（只改記憶體、不寫 settings.xml）。

**刻意沒搬的**：介面大小百分比（macOS 以點排版、Retina 由系統處理）、顯示捲軸開關（左右欄本來就用 overlay scroller）、介面風格預覽卡（用下拉）。

## 慣例 / 注意事項（踩過的坑）

- **⚠️⚠️ 翻譯表曾經是字典字面值，重複的 key 會在第一次使用時 trap** —— 而 `L.t` 對繁中直接回傳、不碰表，
  所以**繁中全部測試通過、其他七種語言一啟動就當**（2026-08-29 發現時已經有 14 個重複 key，從 Metal 那次 commit 起就會當）。
  現在改成 `entries: [(String, Tr)]` 再 `Dictionary(_, uniquingKeysWith:)`，重複只會後者蓋前者。
  `selftest` 的 `[8] 語言` 會把八種語言都查一次。**改 UI 字串後一律用 `AWPR_UI_LANGUAGE=English` 跑一次 `--dlgshot`**。
- **UI 字串一律用翻譯表裡的 key**（= Windows 版的繁中原文），不要自己發明新的繁中句子——沒對上 key 的字串在七種語言下會原樣顯示中文。
  有 `python3` 稽核腳本的做法：抓出所有 `L.t(` / `title:` / `addLabel(` 的 CJK 字串比對 `Localization.swift`（見 git log 2026-08-29）。
- **固定寬度的控制項要量文字再排**（`Theme.measure` / `Theme.truncate`）：德文、法文比中文長一倍以上。`FlatButton` 現在會把過長的標題截成「…」；
  頂列按鈕、檢視列按鈕、照片資訊的值欄、白平衡列、比例列都改為量寬。**新加的固定版面要用 `AWPR_UI_LANGUAGE=German` 截圖看一次。**
- **`NSTextField` 的 label 不會自己換行**：`maximumNumberOfLines` 不夠，還要 `usesSingleLineMode=false`、`cell?.wraps=true`、`preferredMaxLayoutWidth`；
  而且中文沒有空格，word-wrap 根本無處可斷——真的要兩行就用兩個 label。
- **⚠️ app 當機後 macOS 會在下次啟動跳「要重新開啟視窗嗎？」的 modal**（`NSPersistentUIRestorer`），headless 的 `--shot` 會永遠卡住、看起來像 hang。
  已設 `window.isRestorable = false`；診斷時再加 `-ApplePersistenceIgnoreState YES` 保險。用 `sample <pid>` 看主執行緒卡在哪裡，不要猜。
- **headless 不自動開啟上次資料夾**（`viewDidAppear` 有 `Shot.headless` 守門）：截圖要顯示指定的資料夾，而且碰到桌面／文件底下的資料夾會跳 TCC 授權對話框，無人可按。
- **`RawLoader` 的「上次解碼來源」不能是 instance 屬性**：開資料夾時兩個解碼同時跑會互相覆蓋，proxy 的 `.src` 標記就會寫錯。改成隨像素一起回傳（`decodeFullWithSource`）。
- **匯出的浮水印要照縮放比例縮**：Windows 是在全解析度畫完再縮圖；這裡在縮圖後畫，所以 `watermarkScale = 縮後長邊 / 全圖長邊`，不然 66 MP 匯成 2400 px 時浮水印大四倍。
- **`rebuildItems` 會建立新的 `PhotoItem` 物件**：任何重建後 `current` 要重新指向同 key 的新物件，否則 edited 標記更新到孤兒上。`refreshStripKeepSelection` 負責這件事，並在目前照片消失（隱藏／刪除）時選最近的一張。
- **`loadPhoto` 到 `applyLoaded` 之間有一段非同步空窗**（`isLoading`）：`current` 已經換了、但面板和 `adj` 還是上一張的。
  這段時間的編輯會套到錯的照片、然後被載入結果蓋掉。`pushUndo` / `onAdjustmentChanged` 在 `isLoading` 時直接丟棄；
  `--uitest` 有一條斷言專門測這個。切換照片時 `undoStack` **和 `redoStack`** 都要清——不清 redo 會把上一張的編輯貼到這一張。
- **`openFolder` 開頭就把 `current` / `loadedKey` 設回 nil**：否則快取產生期間的編輯會落在舊資料夾的照片上，
  從「紀錄」重開同一個資料夾時第一張的 `loadedKey` 相同會讓載入變成 no-op（`current` 指向孤兒物件）。
- **診斷啟動加 `-ApplePersistenceIgnoreState YES`**：app 曾當機過，macOS 下次啟動會跳「重新開啟視窗？」的 modal；
  `window.isRestorable = false` 已避免將來再存狀態，但舊的狀態檔還在時仍會問一次。
- **刪除／隱藏前一定先 `saveCurrentIfDirty()`**——被刪的不一定是目前那張，目前那張的未存編輯不能跟著丟。

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

## 與 C# 版逐像素對照（2026-08-29）✅ 完成

`awpr-cli hashtest <img> [report]` 是 C# 版 `--hashtest` 的對應物：
**同樣 14 組調整**（C# 版又是照 Windows 版 `GpuParity.Cases()` 抄的）、同樣的來源準備、
同樣對 BGRA bytes 取 SHA-256、同樣的 F9 / G9 數字格式，兩份報告可以直接 `diff`。

```
7 個檔案 × 14 組 = 98 組指紋，全部逐字元相同
（Sony ARW、Canon CR3、Panasonic RW2、Adobe DNG、三張 JPEG）
```

相同的不只是 SHA，還包括 9 位小數的通道均值與 9 位有效數字的取樣點。
C# 版自己的驗收標準只要求「取樣點差 < 1e-5」（允許 libm 的 ULP 差異），實測是**零差異**。

### ⚠️ 這個對照抓到的真 bug：.NET `Math.Round` 是**銀行家捨入**

`Math.Round(double)` 預設 half-to-even，Swift 的 `rounded()` 是 half-away-from-zero。
1705 px 的畫面裁 0.9 剛好是 1534.5 → 兩邊得到 1534 與 1535，**輸出圖片差一個像素**。

所有從 C# `Math.Round` 移植過來的地方都改用 `Double.roundedHalfEven`
（裁切尺寸、模糊半徑、修護座標、縮放尺寸、浮水印邊距）。
**C# 寫成 `(int)(v + 0.5)` 的地方（例如 8-bit 量化）維持不變** —— 那是 floor，不是 Math.Round。

> 改 `ImageProcessor` / `ColorScience` 後請重跑：
> `awpr-cli hashtest <img> a.txt` 與 C# 版 `--hashtest <img> b.txt` 再 `diff`。

## GPU 加速（Metal，2026-08-29）

`Imaging/Metal/`：`MetalShaders`（kernel 原始碼，內嵌成字串）、`MetalPipeline`（裝置／pipeline state／
buffer pool）、`MetalTarget`（`StageTarget` 實作）。**步驟順序沒有重寫** —— 就是當初留的那個縫。

- **kernel 逐行對照 CPU 函式**，並且**上傳同一組 LUT** 而不是在 shader 裡算 `pow()`，
  就是為了讓兩邊不會漂開。`MTLCompileOptions` 明確關掉 fast-math（會允許編譯器重排運算）。
- **修護維持 CPU**：只碰幾個小圓。buffer 是 `.storageModeShared`，所以 CPU 直接就地改，不必上傳下載。
- **所有階段編進同一個 command buffer**，只在 CPU 要讀像素時（heal、result）才 commit＋wait。
  原本每個階段各自 commit，proxy 尺寸下那個來回跟實際運算一樣久。
- **回傳結果不複製**：`FloatImageBuffer` 可以「借用」別人擁有的記憶體（`borrowing:owner:`），
  直接把 shared buffer 交出去並持有 `MTLBuffer`。
- **scratch buffer 用 pool 重用**：一張 66 MP 的圖每份 buffer 是 1 GB，每個階段重新配置的成本
  比 kernel 本身還高。

### 驗證：`awpr-cli gputest <img> [report]`

與 hashtest 同樣 14 組，CPU 與 GPU 各跑一次。**驗收看 8-bit，不看 float**：

- 8-bit 通道差 **≥2 一律不允許**（那代表算式分岔）
- 8-bit 差 1 的比例 ≤ 0.07%

實測（M2）：**14/14 通過**，最大 float 差 1.85e-04、差≥2 為 0、差1 ≤ 0.021%。
float 差來自幾何階段 —— CPU 用 `double`、Metal 沒有 double，座標量級 ~2000 就帶約 1e-4 的捨入，
雙線性取樣把它變成同量級的數值差。**Windows 版同一個階段實測 2.9e-4，是一樣的取捨。**
`gputest` 會在「實際上沒跑 GPU」時明講並略過，不會假通過。

### ⚠️ 尺寸上限 16 MP，這是量出來的

`MetalPipeline.practicalPixelCap`。66 MP 時每份 buffer ~1 GB，階段變成純記憶體頻寬瓶頸，
GPU 不再有優勢，而且工作集逼近實體記憶體時會崩掉
（**單一個暗角 pass 實測 8.4 秒，CPU 只要 0.43 秒** —— 那是在換頁，不是在算）。

上限之下 GPU 穩定快 1.5–4.8×（proxy 2560×1707，綜合案例 64 ms → 22 ms，2.8×）。
上限遠高於任何 proxy（2560 長邊約 4.4 MP），所以**互動編輯一定走 GPU**，
而全解析度匯出安靜地走 CPU —— 那裡 CPU 本來就有競爭力且可預測。

## 更新檢查（2026-08-29）

`App/UpdateCheck.swift`，「關於」視窗的「檢查更新」。與 Windows 版同一支 API，
只有 `platform=macos` 不同（實測伺服器認得，會原樣回傳）。

- **版本比較交給伺服器的 `update_available`**，不自己實作（規則是 PHP `version_compare`）。
- **更新說明只用 `action=changelog&version=`，絕不退回 `release_notes`**（見 Windows CLAUDE.md）。
- 任何失敗一律回 nil、靜默略過。
- 測試：`awpr-cli updatecheck`（走與 UI 完全相同的程式碼路徑）。
- ⚠️ **`latest_version` 目前是 1.0.17（Windows 的版本）**，而 macOS 版是 1.0.0，
  所以現在一定會說「有新版」。網站的 `downloads` 對 `platform=macos` 是空陣列 ——
  要等 macOS 版上架後，這個提示才有意義。

## 實機 RAW 驗證（2026-08-28，`/tmp/raw_test` 16 檔）

Sony ARW ×11、Nikon NEF ×2、Canon CR3、Panasonic RW2、Adobe DNG，**16/16 selftest 全過、全部全解析度**。

### ✅ 與 C# 版逐位元相同的色彩科學

拿 C# macOS port 寫的 `.rawpipe.xml` 當基準，用 Swift 重新從 `cam_mul` 算 as-shot 色溫／色調：

```
8 / 8 逐位元完全相同   （Sony ILCE-7RM6 ×4、Canon EOS R、Panasonic DC-S5、Sony ×2）
例：3667.023643867582 K / -12.540928311851719   兩邊 17 位有效數字全同
```

這條路徑涵蓋 `rgb_cam` 反矩陣 → XYZ↔sRGB → 黑體軌跡（Kang 2002）→ uv↔xy →
粗掃＋40 次三分搜尋 → Duv/tint 換算 → .NET round-trip 數字格式。**改 `ColorScience` 後請重跑這個比對。**

### ✅ 遮罩黑邊修正確實有效

| 機型 | libraw sizes | ImageIO 可見區 | 結果 |
|---|---|---|---|
| Sony ILCE-7RM6 | raw 10240×7168 / visible **10240×7168**（無裁切表）| 9984×6656 | 裁到 9984×6656，**無黑邊** |
| Nikon Z 8 | raw 8280×5520 / visible **8280×5520**（無裁切表）| 8256×5504 | 裁到 8256×5504 |
| Canon EOS R | visible 6742×4498（有裁切表 L146 T48）| 6720×4480 | 採 ImageIO 值 |

**ImageIO 成功取代了 ExifTool `FullImageSize` 的角色** —— 這正是當初需要 ExifTool 的唯一硬需求。

### ⚠️ Nikon Z 8 的 HE 壓縮：LibRaw 解不了（Windows 版也一樣）

`libraw_unpack_function_name` 回報 `nikon_he_load_raw()`，但 `libraw_unpack` 回
`Unsupported file format or not RAW file`。**用 Homebrew 版（有 libjpeg）實測同樣失敗**，
所以與我們 `-DNO_JPEG` 的取捨無關，是 LibRaw 0.22.2 本身不含 Nikon High Efficiency 的解碼器。

macOS 這邊有 Windows 沒有的救援：**ImageIO 原生解得開，而且是全解析度 8256×5504**。
因此 `RawLoader.decodeFull` 的退回順序改成：

```
LibRaw → ImageIO 全解析度 → 內嵌預覽（縮小版，最後手段）
```

**⚠️ 順序很重要**：原本是「LibRaw → 內嵌預覽 → ImageIO」，而 `extractPreview` 走
`CGImageSourceCreateThumbnailAtIndex` 且上限 4096px，所以 Z 8 會拿到 4096×2731 的縮圖版而不是全圖。

### ⚠️ 隨之而來的白平衡參考基準問題

ImageIO 解出來的 RAW **已經把相機白平衡烤進去了**（等同 `cam_mul`），LibRaw 解的則是
平衡到 `pre_mul`（日光）。餵錯 `WhiteBalanceReference` 會整張偏色
（此例 preMul/camMul ≈ 1.157 / 1 / 0.823，明顯偏暖）。

所以 proxy 旁邊會寫一個 **`.rawpipe.png.src` 標記檔**（內容 `libraw` 或 `imageio`），
`RawLoader.proxyDecodeSource(path:)` 讀它決定 `whiteBalanceReference`。
**刻意不寫進共用 XML** —— 那是跨平台的編輯資料，這只是本機快取的中繼資料。
沒有標記檔的舊快取一律當 `libraw`（在標記存在之前就是這個行為）。

## 尚未完成 / 後續

- **Fujifilm RAF 尚未測**：手上沒有樣本（Windows 版是用 X-T30 測的）。
  X-Trans 的去馬賽克路徑與 Bayer 不同，值得單獨驗。拿到檔案後跑
  `awpr-cli info`（看 `libraw sizes` 與相機色彩資料）→ `selftest` → `hashtest` 與 C# 版對照。
- **上架 Mac App Store** 若要做：得加 `com.apple.security.app-sandbox`＋`user-selected.read-write`，
  並把重開資料夾用的 security-scoped bookmark 存起來（見 entitlements 的註解）。
- 更新檢查（Windows 版的 `UpdateCheck`）尚未移植。
