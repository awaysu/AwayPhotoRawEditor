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
# 先建一次 libomp 與 LibRaw（只需做一次，或升級時）。順序不能反：LibRaw 要連 libomp。
brew install cmake            # 只有 build_libomp.sh 需要
Scripts/build_libomp.sh       # LLVM OpenMP runtime → ThirdParty/libomp（universal、minos 14）
Scripts/build_libraw.sh       # 自動偵測 ThirdParty/libomp，有就開 OpenMP；沒有就退回單執行緒

# 開發用
swift build
swift run awpr-cli selftest <圖片>

# 產生 .app（含簽章）
Scripts/build_app.sh
open build/AwayPhotoRawEditor.app
```

### 發佈 / 簽章 / 公證

發佈流程（每版照做，2026-09-02 起）：

0. **先在 `CHANGELOG.md` 最上方加這一版的大要**（每版一段、最新在上，對使用者描述、不寫內部細節）——網站的版本歷史就是這段文字。
1. **改版本號**：只有一處 `AppVersionInfo.version`（`App/UpdateCheck.swift`），`build_app.sh` 讀它寫進 Info.plist，關於視窗與檢查更新都用它。
2. commit、push 到 `awaysu/AwayPhotoRawEditor_Swift`。
3. 簽章＋公證＋DMG：

```bash
# 一次做完：重建 → hardened runtime 簽章 → 公證 → stapler → DMG → DMG 也公證
Scripts/sign_and_notarize.sh "Developer ID Application: Chih-Wei Su (BNH8YS88T9)" awpr-notary
# → build/AwayPhotoRawEditor-<版本>.dmg，最後印的 SHA256 才是要公布的
```

4. 上傳網站（同 app／平台／副檔名會自動取代，重跑同一指令即可；`changelog` 帶這一版的大要，伺服器自動加 `v<版本> (日期)` 標題並放到歷史最上面，同版本不會重複）：

```bash
# 密碼在 ~/workspace1/web_info.txt（後台登入密碼）——絕不能進 repo；用 header 帶，不要寫進指令歷史
DMG=build/AwayPhotoRawEditor-<版本>.dmg
curl -H "X-Api-Password: $AWAYSU_API_PASSWORD" -F app=awayphotoraweditor_mac -F platform=macos \
     -F version=<版本> -F "changelog=<release-notes.txt" -F sha256=$(shasum -a 256 $DMG | cut -d' ' -f1) \
     -F "file=@$DMG" "https://www.awaysu.cc/software/api.php?action=upload"
```

5. 驗證：`awpr-cli updatecheck` 回「已是最新」、`api.php?action=changelog&app=awayphotoraweditor_mac&version=<版本>` 有內容。
   只補歷史不上傳檔用 `action=set_changelog`（可帶 `date`），只改版本用 `action=set_version`。

事前只需做一次（app-specific password 在 appleid.apple.com 產生，**不是** Apple ID 密碼）：

```bash
xcrun notarytool store-credentials awpr-notary \
    --apple-id <apple-id> --team-id <TEAMID> --password <app-specific-password>
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
| RAW 解碼 | LibRaw 0.22.2 DLL | **LibRaw 0.22.2**（自行建置，**含 OpenMP**） | 同版本 → 同像素。這是顏色能對得起來的前提。OpenMP 見下方 |
| 一般格式 | WIC | **ImageIO / CGImageSource** | 系統原生；順便多支援 HEIC |
| EXIF | ExifTool（外部行程） | **ImageIO 原生** | ExifTool 是 500+ 個未簽章的 Perl 檔，每個 Mach-O 都要簽才過得了公證，不值得 |
| GPU | Direct3D 12 / ComputeSharp | **Metal**（見下方「GPU 加速」） | 同樣的 shader 對照 CPU 參考實作逐行寫 |
| 高 DPI | `Ui.S()` 手動縮放全部版面 | **不需要** | macOS 以點為單位排版、Retina 由系統處理。字級仍可調（設定 → 字體大小） |
| 版面過高 | `DarkScrollHost` + `AutoFitScale` | **NSScrollView（overlay scroller）** | 同一個問題：右欄內容約 900pt，1040 高的視窗放不下 → 左右欄可捲動 |
| 語言/外觀切換 | `Application.Restart()` | **重新啟動 app** | 同樣的理由：字型與調色盤是開機時決定的快取靜態值 |
| 關閉資料夾並刪除快取縮圖 | 只刪 `_thumb.jpg`／`.rawpipe.png`／`.f16`，**保留 XML** | **整個 `RAW_TEMP` 丟進垃圾桶**（含調整 XML、`preview_list.xml`） | 使用者要的是「資料夾消失」（2026-08-31）。對話框明講會丟調整設定；用 `trashItem` 才救得回來，沒有垃圾桶的磁碟區退回 `removeItem`。`--uitest [11]` 斷言 |
| Delete 鍵 | Del = 隱藏且不輸出、Shift+Del = 刪除檔案 | **不綁**（2026-09-01 使用者要求：誤按一下照片就不見）；漸層工具下 ⌫ = 刪除選取的漸層 | 隱藏／刪除只從縮圖右鍵選單進 |
| RAW 縮圖的底圖 | 永遠是相機內嵌預覽（`_thumb.jpg`） | **proxy 一產生就改從 proxy 裁 240×160**（`{file}.rawpipe.png.thumb.jpg`，macOS 專用快取檔）；內嵌預覽只當 proxy 還沒好之前的佔位 | 使用者反映「縮圖和實際編輯的結果不一樣」（2026-09-01）：相機 JPEG 是相機自己的色調曲線／風格／白平衡，套上編輯後和 LibRaw 解的 proxy 越差越遠。同底圖＋同白平衡參考（`proxyDecodeSource`）→ 兩邊由構造保證一致。`selftest [3]`、`--uitest [1]` 斷言 |
| 隱藏／刪除的復原 | 無 | **恢復上一步（⌘Z）可還原隱藏與刪除**：`UndoStep` 是 enum（`edit`／`hide`／`remove`），切換照片只清 `edit`，`hide`／`remove` 留著（隱藏目前這張本來就會切到鄰近一張）。刪除實體檔時記下 `trashItem` 回傳的位置、非佔位的 sidecar、`preview_list` 條目，復原時搬回、重寫、重建縮圖。`--uitest [7]／[8]／[8b]` 斷言 | 使用者要求（2026-09-01） |

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
| `awpr-cli exporttest <img> <outDir> [report]` | 匯出全流程（含同名去重、EXIF 保留）。`AWPR_TEST_WATERMARK=文字` 開浮水印（`_SIZE`／`_POS=TopLeft…` 可調），匯出後直接開圖看 |
| `awpr-cli render <img> <out.png> [--exposure N …] [--grad-exposure N]` | 單張套用調整後輸出 PNG，並印出來源／輸出的通道均值與「任一通道 ≥0.999」的像素比例。`--grad-exposure` 是一個蓋滿整張（m=1）的漸層，用來把漸層曝光和全域曝光放在同一把尺上比 |
| `awpr-cli bench <img>` | 各階段 proxy / 全解析度耗時 |
| `awpr-cli cmpbench <img> [--gpu] [--render-only]` | **與 C# mac 版對照用**：逐項照它的 `--decodetest`／`--rendertest`／`--enginetest` 條件（2400 px、同六組調整、單次冷跑）。結果與結論見 `Docs/Comparison-CSharp-vs-Swift-2026-08-30.md` |
| `AwayPhotoRawEditor --shot <folder> <png> [waitMs] [WxH]` | 主畫面離屏截圖。`AWPR_SHOT_TOOL=crop\|gradient\|heal` 會選好工具（漸層會先新增一個）再拍，用來看手把幾何 |
| `AwayPhotoRawEditor --dlgshot <export\|settings\|presets\|about\|fonts\|firstrun\|progress> <png>` | 對話框離屏截圖 |
| `AwayPhotoRawEditor --uitest <folder> [report]` | **UI 流程測試**：把資料夾複製到暫存區，驅動真正的 `MainViewController` 跑 載入→編輯→復原／重做→切圖存檔→多選批次同步與批次復原→風格檔→重設→裁切比例→旋轉→虛擬副本→複製貼上→隱藏／顯示全部／還原→刪除副本→關閉存檔，逐項斷言（含磁碟上的 XML）。截圖測不到的邏輯都在這裡。JPEG 與 RAW 資料夾都要跑。|

**`--shot` 是把 view 自己畫進 bitmap（`cacheDisplay`），不是螢幕截圖** —— 所以不需要「畫面錄製」權限，
SSH 或 CI 裡都能跑。**尺寸是設 view 的 frame 而不是視窗**：macOS 會把視窗夾進螢幕範圍，
走視窗的話結果會隨執行的螢幕而變。截完直接 `exit()`——背景快取工作否則會讓行程一直活著。

## 與 Windows 版功能對照（2026-08-29 全面比對後補齊）

拿 Windows 版的 `MainForm` 選單／快捷鍵／右鍵選單／各對話框逐項對過。以下是補齊的：

- **重做**（Windows Ctrl+Y → macOS ⌘⇧Z）：`redoStack`，新編輯清空 redo；只重播單張、不重播批次同步（同 Windows）。undo 上限 80。
- **紀錄**（☰ → 紀錄 ▸ 最近 20 個資料夾，`清除紀錄`）：存 settings.xml 的 `<RecentFolders><string>…`，與 Windows 同格式。資料夾不存在時提示。
- **快捷鍵**：F5 = 重新整理、Esc、←→、`\`。**⌫／⇧⌫ 刻意不綁隱藏／刪除**（見差異表）。
- **縮圖右鍵選單**照 Windows 順序：全選／反向選擇／取消全選、套用風格檔 ▸、複製／貼上照片設定、升級處理版本、建立副本、隱藏且不輸出／取消隱藏、刪除檔案、不顯示隱藏／顯示全部（勾選）、匯出照片。
- **工具分頁再按一次取消**（2026-08-30 真人操作發現漏搬）：Windows 的 `TopTab.AllowDeselect`——啟動時**沒有選工具**（裁切頁灰掉當佔位、鎖住），
  按選中的分頁 → `ToolMode.None`（viewer 顯示最終裁切結果、左鍵可平移／循環縮放），Esc 與 c/g/h 一律走 `toolsPanel.selectTool`，分頁外觀才會跟著變。
  `--uitest [5b]` 有斷言。
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
- **⚠️ macOS 14 起 `NSView.clipsToBounds` 預設是 false**（2026-08-30 真人操作：100%／200% 縮放後整個畫面只剩右欄）：
  viewer 在 `draw` 裡把放大的圖畫到自己的 bounds 外、蓋掉左欄與頂列。`centerColumn`／`viewer`／`strip` 現在明確 `clipsToBounds = true`；
  **任何會畫到 bounds 外的自繪 view（捲動、縮放）都要設**。⚠️ 這個溢出**只有螢幕上的 compositor 看得到**：`cacheDisplay`（drawRect 遞迴）
  與 `layer.render(in:)`（headless 視窗沒有 display cycle）都看不出來，實測關掉裁切兩者照樣「相同」。所以 `--uitest [2b]` 是契約式斷言
  （三個 view 的 `clipsToBounds` 必須為 true），**真正的驗證要真人按 100%／200% 看**。
- **⚠️ 所有對話框 controller 都要自己持有自己**（2026-08-30，真人操作：匯出視窗三個按鈕按了沒反應）：
  `let dialog = ExportWindowController(...)`／`SettingsWindowController()`／`AboutWindowController()`… 全是區域變數，`show()` 一回來就釋放，
  按鈕的 `[weak self]` 閉包全部變 nil——與進度視窗同一類 bug，只是進度視窗先被發現。現在 `DialogController` 基底與 `ExportWindowController`
  都有 `retainedWhileShown`（`show`→`close`）。**新加對話框一律繼承 `DialogController`**。`--uitest [10]` 對每種對話框斷言「show 後仍存活、close 後釋放」。
- **⚠️ `hitTest(_:)` 收到的點是「父 view 座標」**（2026-08-30 真人操作：工具的裁切／漸層／修護按了沒反應）：
  `RibbonLockView` 的 `hitTest` 原本沒隱藏就一律回 `self`，把整個工具面板（含分頁列）的點擊全吃掉；要 `frame.contains(point)` 才回自己。
  **uitest 直接呼叫 `tabs.click(index:)` 測不到這種事**——現在 `[5b]` 另外走真正的 `toolsPanel.hitTest`（分頁列點得到、參數區回鎖層）。
  覆寫 `hitTest` 的 view 一律要加這種斷言。
- **裁切框有懸停游標（macOS 版增補，Windows 版沒有）**：滑過角落顯示自畫的斜向雙箭頭（AppKit 沒有公開的對角 resize 游標）、
  邊是 ↔↕、框內是手掌。區域判定與 `beginCropDrag` 共用同一個 `cropHitZone`，游標出現的地方就一定抓得到。`--uitest [2d]` 斷言四角。
  ⚠️ **游標圖是在 `flipped: false`（y 向上）的 `NSImage` 裡畫的**：(-a,-a)→(a,a) 是 ↗↙ 不是 ↖↘，第一版就這樣畫反了（2026-08-31 真人發現）。
  `[2d]` 現在另外驗像素（↖↘ 的左上角要有筆畫、右上角不能有）；`AWPR_UITEST_CURSOR_DUMP=<dir>` 會把兩個游標存成 PNG 給人看。
- **viewer 手把尺寸是真人調出來的（2026-08-30）**：裁切角落判定 18 pt（邊 10）、角把手是畫在框**內側**的 L 形（畫在外側會在框貼齊圖邊時被裁掉）；
  漸層點半徑 10、白點 12、藍點距白點 256 pt、藍點內畫旋轉箭頭、命中半徑 14。C# 版是 5／6／64／10。改這些後用 `AWPR_SHOT_TOOL` 截圖看。
- **⌘A 全選走主選單（`AppDelegate.selectAll`）**，`--uitest [3]` 用 `NSApp.mainMenu.performKeyEquivalent` 驗證；另外接了 **Ctrl+A**（Windows 習慣）。
- **⚠️ `ProgressWindowController` 要自己持有自己（`show`→`close` 之間）**：呼叫端只在 completion 閉包裡抓著它，
  閉包一回傳 controller 就死了，而 `finish()` 排的 0.8 秒延遲關閉與「取消」按鈕都是 `[weak self]` →
  sheet 永遠留在畫面上、也按不掉。這是**第一次真人開資料夾就撞到的 bug**（2026-08-29），`--uitest` 全綠也沒抓到，
  現在 `[1] 載入` 有一條「快取進度視窗自動關閉」斷言（`ProgressWindowController.shownCount`）。
  **debug 建置下 16 張 RAW 的 uitest 會逾時**（快取重建超過 30 s），用 3 張的資料夾跑。
  **uitest 步驟之間不要用固定延遲**：`[8]` 刪除副本會觸發重新載入，之前固定等 0.3 s 再編輯，機器一忙 RAW 就載不完、
  編輯被 `isLoading` 守門丟掉 → `[9]` 偶發失敗。一律 `waitUntil(!isLoading && proxy != nil)`。
- **⚠️ 快取檔一律原子寫入、讀取時驗檔尾**（2026-08-30，真人操作發現「7RM5／7RM6 編輯區下半部是彩色亂塊」）：
  `ImageIOCodec.write` 原本 `CGImageDestinationCreateWithURL` 直接寫目標路徑，開資料夾的背景工人與使用者點到同一張時的 `loadPhoto`
  會**同時寫同一個 proxy PNG**（60 MP 解碼最久、最容易撞到），或被 kill 留下半個檔；ImageIO 解截斷／交錯的 PNG **不會報錯**
  （`CGImageSourceGetStatus` 照樣回 complete），上半部正確、下半部是垃圾。現在：
  (1) 先寫 `.<name>.<uuid>.part` 再 `rename(2)`（匯出也走這條）；(2) `CacheManager.load` 檢查 PNG 以 `IEND` chunk、JPEG 以 `FF D9` 結尾，
  不對就刪掉回 nil，`loadProxy`／`loadProxyFloat` 會**再產生一次**（不是退回全解析度解碼）；(3) `ensureProxyCache`／`ensureThumbnailCache`
  以路徑為 key 單飛（`withPathLock`），點到正在產生的照片會等它。
  重現：把 proxy PNG `head -c 45%` 截斷再 `--shot`，修前下半黑、修後自動重做。
- **`MetalTarget` 的 scratch buffer 在 command buffer 完成前不回共用 pool**：各階段編在同一個 command buffer、到 `result()` 才 commit，
  原本 `release()` 立刻歸還 pool，另一個 target（縮圖算圖是並行的）借走後 CPU `update(from:)` 會寫進**尚未執行的 kernel 還要讀的記憶體**。
  現在 `release()` 進 target 自己的 `retired` 清單（同 target 後續階段可重用，Metal 會追蹤同一 command buffer 內的 hazard），`flush()` 之後才歸還。gputest 17/17 不變。
- **`closeFolder()` 要把 `settings.lastFolder` 清空並存檔**（2026-08-31 真人操作：關閉資料夾並刪除快取後，重開程式又自動載入同一個資料夾）：
  與 Windows 的 `CloseFolder` 相同——使用者主動關閉就該維持關閉狀態。只有「開啟資料夾」寫 LastFolder，結束程式不經過 `closeFolder`，所以自動重開不受影響。headless 只改記憶體不寫檔；`--uitest [9]` 斷言。
- **刪除／隱藏前一定先 `saveCurrentIfDirty()`**——被刪的不一定是目前那張，目前那張的未存編輯不能跟著丟。
- **漸層的曝光是在亮部已經裁掉之後才乘上去的（與 Windows 版相同；2026-09-01 使用者問「用漸層拉暗，亮部為什麼還是過曝」）**：
  管線順序是 第 1+2 步白平衡＋曝光（線性域，`encode` 時夾到 1.0）→ 第 3 步色調 LUT → **第 7 步漸層**。實測 ARW `render --exposure 1` 有 36.8% 像素任一通道已在 1.0，
  再加 `--grad-exposure -1` 後裁切像素 0%、均值回到與曝光 0 相近——數字上「有拉暗」，但那 36.8% 只是被統一乘成 encode(0.5)≈0.71 的一片平灰，
  細節在漸層之前就沒了，看起來就是「灰掉的過曝」。要像 Lightroom 那樣把亮部救回來，漸層曝光得搬進第 1+2 步、在 encode 之前於線性域相乘
  （CPU、Metal kernel、C# 版三處一起改；會改變所有含漸層曝光照片的輸出、hashtest 與 Windows 版分岔），**尚未決定要不要做**。

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
- **`-DNO_JPEG -DNO_LCMS`**：少兩個 dylib 要簽。代價是
  lossy DNG 等少數格式 LibRaw 解不了 —— 但 `RawLoader` 會依序退回「內嵌預覽 → ImageIO」，
  macOS 原生就認得那些格式，所以實際覆蓋率沒有損失。
- **⚠️ LibRaw 一定要開 OpenMP（2026-08-30 量出來的）**：原本為了少簽一個 dylib 用 `-DLIBRAW_NOTHREADS`，
  結果全解析度解碼比 C# mac 版慢 1.4–2.2×（LibRaw 的 AHD/DHT 去馬賽克、CR3 `crx`、`raw2image`、postprocessing 都是 `#pragma omp`，
  單核 vs 三核）。**不能拿 Homebrew 的 libomp**：arm64-only、minos 26。`Scripts/build_libomp.sh` 從 LLVM 20.1.8 原始碼建
  universal／minos 14 的 `libomp.dylib`（37 秒），`build_libraw.sh` 偵測到就 `-Xclang -fopenmp` 連上，並複製一份到
  `ThirdParty/libraw/lib` 旁邊（同一個 rpath 解析兩個）；`build_app.sh` 看到 libraw 引用 `@rpath/libomp.dylib` 就一起打包簽章
  （`collect_deps` 本來就跳過 `@rpath/*`，所以要特別處理）。
  實測：7RM6 4929 → 2874 ms、CR3 2075 → 897、7M3 1376 → 635、RW2 1579 → 839（追平或超過 C#）；
  **hashtest 三檔 42 組 SHA 逐字元不變**——OpenMP 只切列、不改算式。
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
  漏了這一步的症狀是「拉滑桿沒反應」。**反方向也一樣**（2026-08-30 真人操作：新增漸層後拉色溫，漸層消失）：
  viewer／工具面板改了 `adj`，色彩面板那份複本還是舊的，下一次拉色溫就把舊複本寫回去。現在 `adj` 的 `didSet`
  會把新值推給四個面板與 viewer，**任何改 `adj` 的路徑都自動同步**。`--uitest [2c]` 斷言。
- **`pipelineVersion` / `activeGradientIndex` 不參與 `valueEquals`**：
  與 C# 版一樣，舊版照片沒動滑桿不能被當成「已編輯」，`resetAll()` 也不能偷偷升級版本
  （`resetAll` 會把 version 存起來再放回去）。
- **多選批次同步的目標要在「編輯手勢開始」時擷取**（`pushUndo`），不能在提交時抓——
  單擊縮圖會先把選取變成單張才觸發載入，提交當下 `selectedItems` 已經不是原本的多選。
- **縮圖的底圖**（`RawLoader.loadThumbnailBase`）：RAW 優先用 proxy 裁出來的 `.rawpipe.png.thumb.jpg`（`ensureProxyCache` 順手寫，
  **proxy 已存在但這張缺少時也會從現成 proxy 補一張**——舊資料夾開啟時自動追上，不必刪快取；
  `applyLoaded` 後會重畫該格，所以 proxy 一好縮圖就換過去），白平衡參考與編輯區同一個 `proxyDecodeSource`。
  ⚠️ **測行為前先確認跑的是哪個 .app**：2026-09-01 使用者回報「還是有差距」，其實是在跑早上建的 `build/AwayPhotoRawEditor.app`；
  `swift build` 只更新 `.build/debug/`，**要 `Scripts/build_app.sh` 重建、再重開 app** 才會吃到修改（`ps aux | grep AwayPhotoRawEditor` 看路徑，`關於` 看 build 時間）。
  **還沒有 proxy 時才退回相機內嵌預覽 `_thumb.jpg`**——那張**相機白平衡已經烤在裡面**，所以走 `whiteBalanceReference = .asShot`；
  沒有相機色彩資料時用 `5200 + (adj.temperature - exif.colorTemperature)` 的偏移法。照原值算會把白平衡套第二次。
  非 RAW 的 `_thumb.jpg` 與 proxy 同樣來自 ImageIO，本來就一致，不另外產生。
- **刪除照片走垃圾桶（`trashItem`）**，不是 `removeItem`。
- **`CancelToken` 是協作式的**：背景算圖被新的取代時只是設旗標，管線在階段邊界檢查。

## 快取檔（各資料夾 `RAW_TEMP/`）

`{file}_thumb.jpg`（縮圖：相機內嵌預覽）、`{file}.rawpipe.png`(+`.f16`、`.src`)（proxy）、
**`{file}.rawpipe.png.thumb.jpg`（RAW 專用、從 proxy 裁的縮圖底圖，macOS 才有；Windows 版不認得、也不會刪，無害）**、
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

與 hashtest 同樣 14 組，**再加 3 組修護案例（複製／填補／綜合＋修護×2，`healCases()`）**，CPU 與 GPU 各跑一次。
修護案例刻意不放進那 14 組——`hashtest` 的清單必須與 C# 版逐字相同；而 GPU 路徑的修護是
「flush → CPU 就地改 shared buffer → 後續階段接著用」，那個交接才是要測的東西。
因為兩邊跑的是同一段 CPU 修護程式，修護沒生效也會「一致」，所以修護案例**另外對照無修護的 CPU 結果**，
改動像素數必須 > 0（實測與圓面積吻合：r=77 px → 18601 px vs π·77² ≈ 18626）。**驗收看 8-bit，不看 float**：

- 8-bit 通道差 **≥2 一律不允許**（那代表算式分岔）
- 8-bit 差 1 的比例 ≤ 0.07%

實測（M2，ARW 與 DNG）：**17/17 通過**，最大 float 差 1.85e-04、差≥2 為 0、差1 ≤ 0.021%；修護案例 float 差 1.19e-07。
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

## 更新檢查（2026-08-29；2026-09-01 上架後改指獨立條目）

`App/UpdateCheck.swift`，「關於」視窗的「檢查更新」。與 Windows 版同一支 API。

- **⚠️ Windows 與 macOS 在網站上是兩個獨立的 app 條目、GitHub 也是兩個 repo，各自維護**：
  | | 網站 slug | GitHub |
  |---|---|---|
  | Windows（C#） | `awayphotoraweditor` | `awaysu/AwayPhotoRawEditor` |
  | macOS（本專案） | `awayphotoraweditor_mac` | `awaysu/AwayPhotoRawEditor_Swift` |
  版本號、changelog、下載檔都分開管理（macOS 1.0.19 起與 Windows 的 1.0.18 分開走，不是綁定）。
  `UpdateCheck.appSlug`／`pageUrl` 與「關於」的 Source Code 連結都指 macOS 這邊——
  **上架 2026-09-01 曾誤傳 DMG 到 Windows 條目**，就是因為當時 slug 還指著 `awayphotoraweditor`。
- **版本比較交給伺服器的 `update_available`**，不自己實作（規則是 PHP `version_compare`）。
- **更新說明只用 `action=changelog&version=`，絕不退回 `release_notes`**（見 Windows CLAUDE.md）。
- 任何失敗一律回 nil、靜默略過。
- 測試：`awpr-cli updatecheck`（走與 UI 完全相同的程式碼路徑）。
- **發佈用的上傳 API**（`action=upload`，spec 在 private repo `awaysu/software-web` 的
  `readme_for_program.txt`）：帶 `app=awayphotoraweditor_mac`、`platform=macos`、`version`、
  `sha256`（伺服器會比對）與檔案；同 app 同平台同副檔名會自動取代，重跑同一指令即可。

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

## 尚未完成 / 後續（2026-08-29 盤點）

### 需要使用者才能做的
- ~~**Developer ID 簽章＋公證**~~ ✅ 完成（2026-09-01）：憑證 `Developer ID Application: Chih-Wei Su (BNH8YS88T9)`，
  notarytool profile `awpr-notary` 已存鑰匙圈，`sign_and_notarize.sh` 跑通、spctl 回 `Notarized Developer ID`。
  私鑰備份與新機器還原步驟（含 `errSecInternalComponent` 的解法）在 private repo `awaysu/codesign-backup` 的 README。
  憑證 **2031-09-01 到期**。
- **Fujifilm RAF**：沒有樣本（Windows 版是用 X-T30 測的）。X-Trans 去馬賽克與 Bayer 路徑不同，拿到後跑
  `awpr-cli info`（看 `libraw sizes` 有沒有裁切表、相機色彩資料讀不讀得到）→ `selftest` → `hashtest` 與 C# 版對照。
- ~~**網站上架 macOS 版**~~ ✅ 完成（2026-09-01）：v1.0.18 DMG（公證＋staple）已上傳到獨立條目
  `awayphotoraweditor_mac`（見「更新檢查」的對照表），下載回傳 SHA256 逐位元驗證過、
  `awpr-cli updatecheck` 回「已是最新」。SHA256：`44b053198aa09f9a1c6d606b9e73a2c842f5418508c84c132700b2b3bdb63a1d`。
  尚待使用者：在網站管理頁把誤傳到 Windows 條目（`awayphotoraweditor`）的 macOS DMG 下載項目刪除，
  並補 `_mac` 條目的顯示名稱／副標／changelog（目前還是 slug 佔位字）。
- **真人操作**（2026-08-30/31 已摸過一輪，抓到 8 個 headless 測不到的 bug，全修：進度視窗關不掉、匯出等對話框按鈕全死、
  100%↑ 畫面溢出、工具分頁點不到（hitTest）、工具不能取消、Ctrl+A、裁切角落難點、新增漸層被別的滑桿蓋掉——每一個都在
  「踩過的坑」有記錄與 uitest 斷言）。**還沒真人驗過的**：修護圈圈與右鍵刪除、trackpad 縮放手感、拖曳資料夾到視窗、
  第一次執行的語言選擇、漸層藍點 256 pt 距離的手感。

### 效能：與 C# mac 版比較（2026-08-30，`Docs/Comparison-CSharp-vs-Swift-2026-08-30.md`）
後製 CPU 快 3×、GPU 快 5–10×、proxy 熱取快 3–5×、記憶體少 30–45%；解碼原本慢 1.4–2.2×（LibRaw 沒開 OpenMP），
**同日補上 OpenMP 後追平或超過 C#**（見「踩過的坑」）。公證時 `Contents/Frameworks` 現在有 `libraw.25.dylib` 與 `libomp.dylib` 兩個。

### 刻意沒搬（macOS 不需要）
☰ 選單的「匯出目前照片／匯出照片／匯出全部照片」與「還原已隱藏的照片」（2026-08-31 使用者要求移除：右下匯出按鈕與縮圖右鍵選單已涵蓋；`restoreHiddenPhotos()` 仍在、只拿掉入口）、
介面大小百分比（系統處理 Retina）、顯示捲軸開關（左右欄本來就是 overlay scroller）、
介面風格預覽卡（用下拉）、Mac App Store 沙盒（見 entitlements 註解）。

### 已驗證（2026-08-29 補）
- **浮水印匯出的實際畫面**：`AWPR_TEST_WATERMARK=文字 awpr-cli exporttest <img> <outDir>`，
  Sony 9984 px → 2400 px 實看：150 pt 縮成 36 px、邊距 30→7 px，右下／左上都對，Ä 變音符、中文回退字型、gjpq 下伸部都完整。
- **修護（Heal）在 GPU 路徑**：`gputest` 加了 3 組修護案例，ARW／DNG 都 17/17（見「GPU 加速」）。

### 尚未驗證
- **Intel 機器**：universal 二進位含 x86_64 切片，但只在 M2 上跑過。
- **macOS 14**：deployment target 14.0，實測機器是 26。
- **與 Windows 版本身的逐像素對照**：`hashtest` 對照的是 C# 的 macOS port（98/98 逐字元相同）；Windows 版與它出自同一份 C#，推論相同但沒直接跑過。
- **補的約 50 條翻譯**（重做、紀錄、GPU、匯出對話框的新標籤等）沒有母語者看過；Windows 原有的 265 條原樣沿用。

### 程式本身
功能對照 Windows 版已逐項對完（見「與 Windows 版功能對照」），沒有已知缺的功能。
引擎（selftest／gputest／exporttest／C# hashtest 對照）、UI 流程（`--uitest` JPEG＋RAW）、八語啟動 smoke 全綠。
