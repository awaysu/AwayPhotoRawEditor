# AwayPhotoRawEditor / Away PhotoRaw Editor

顯示名稱一律「Away PhotoRaw Editor」（頂列 Logo、關於視窗、OS 視窗標題 MainForm.Text）。

深色主題的 Windows 桌面 RAW 相片編輯器（Lightroom / Capture One 類）。非破壞式編輯，所有調整以 XML 存於各資料夾的 `RAW_TEMP` 快取。

- **技術**：C# / .NET 8（`net8.0-windows`）/ WinForms / x64 / `unsafe`
- **UI**：深色、全自繪控制項，字體 Microsoft JhengHei UI
- **程式 icon**：`src\icon\icon.ico`（csproj `<ApplicationIcon>`；MainForm 以 `Icon.ExtractAssociatedIcon` 帶入視窗/工作列）。由 `src\icon\icon.png` 產生：`src\icon\make_icon.ps1`（用 powershell.exe 跑）會 flood-fill 去白底（保留鏡頭白圈）、裁切置中、輸出 256~16 多尺寸 ICO；換圖後重跑即可
- **外部工具**（已內建於 `tools/`，執行期自動偵測，抓不到自動退回）：
  - LibRaw 0.22.1 → `tools/libraw/LibRaw-0.22.1/bin/libraw.dll`（RAW 解碼，P/Invoke）
  - ExifTool 13.59 → `tools/exiftool/exiftool.exe`（需 `exiftool_files/` 同目錄）

## 建置 / 執行

```powershell
# 建置
dotnet build src\AwayPhotoRawEditor.csproj -c Debug

# 發佈 (release, 單機)
dotnet publish src\AwayPhotoRawEditor.csproj -c Release -r win-x64

# 直接執行 (GUI，最大化啟動，自動開啟上次資料夾)
src\bin\Debug\net8.0-windows\AwayPhotoRawEditor.exe
```

> ⚠️ 重建前先關掉殘留進程，否則 exe 被鎖：
> `Get-Process AwayPhotoRawEditor -ErrorAction SilentlyContinue | Stop-Process -Force`
> WinExe 不會阻塞 PowerShell，用 `Start-Process ... -Wait`（或 `-PassThru` + `WaitForExit`）。

### 診斷模式（`Program.cs`，headless，用於驗證/截圖）
| 指令 | 用途 |
|---|---|
| `--selftest <img> <report>` | 引擎端到端：解碼→EXIF→快取→管線→histogram→XML 往返，寫報告 |
| `--exporttest <img> <outDir> <report>` | 匯出全流程測試（含去重）|
| `--shot <folder> <png> [waitMs] [WxH]` | 開資料夾、算圖後截主畫面（預設 1500×1040 非最大化以離屏截圖，可用第 4 參數如 `1500x760` 指定 client 尺寸（會解除 MinimumSize）測試矮視窗捲軸；設 `Program.Headless` 停用 MainForm 快速鍵——**離屏視窗會搶真實鍵盤焦點**，曾因使用者按到 Del 把測試照片隱藏）|
| `--dlgshot <folder\|export\|settings\|progress\|gradient\|gradoverlay\|presets\|about> <png>` | 截對話框（progress=進度視窗、gradient=漸層 ribbon、gradoverlay=漸層覆疊手把、presets=編輯風格檔、about=關於）|
| `--gallery <png> [sampleImg]` | 截所有自繪控制項 |
| 環境變數 `AWPR_TRACE=1` | 執行軌跡寫到 `%TEMP%\awpr_trace.txt`（`Diagnostics/Trace.cs`）|

## 架構（`src/`）

UI（Forms/Panels/Controls）→ 服務（Rendering/Export/Storage）→ 領域（Imaging/Exif/Models）。
領域層不依賴 WinForms，可獨立測試。畫面永遠是「原圖 proxy + `ImageAdjustments`」即時運算的結果。

- **`App/`** — `AppPaths`（支援副檔名、RAW_TEMP 忽略、三種快取檔命名、工具偵測；**2026-07 專案由 awPhotoRawEditor 改名為 AwayPhotoRawEditor**：`AppDataDir` 首次建立 `%AppData%\AwayPhotoRawEditor` 時會從舊名資料夾一次性複製設定/風格檔）、`AppSettings`（UseLibRaw / UseHighPrecisionRawPipeline / **ShowColumnScrollBars**(顯示捲軸，預設 false)，存 `%AppData%\AwayPhotoRawEditor\settings.xml`）、**`AppVersion`（`Version = "v1.0.0"`；⚠️ 每次交付更新把第三位 +1 並同步 csproj `<Version>`，關於視窗顯示此值）**
- **`Models/`** — `ImageAdjustments`（★全部調整參數 + Clone/ResetAll/ResetTonal/**ResetBasicColorDetail**/**ApplyDelta**(只複製 baseline→edited 有變動的欄位，供多選批次同步；**跳過 HealSpots 與 Gradients**)/ValueEquals/IsDefault；**漸層改為 `List<LinearGradient> Gradients`**(可加多個線性漸層)+ 執行期 `ActiveGradientIndex`(欄位、`[XmlIgnore]`、反射自動略過)/`ActiveGradient`(取用時自動夾住索引)）、**`LinearGradient`**(單一線性漸層：CenterX/CenterY 位置、Angle 角度、Range 範圍 + 曝光/對比/亮部/暗部/飽和度 + Clone/ValueEquals/HasEffect)、`HealSpot`、`ExifData`、`PhotoItem`（虛擬副本 key `path|copy:N`）、`PresetProfile`（5 組內建風格值 + `DefaultName`「預設時設定」為 fullReset）、`Enums`（`ToolMode.None` = 未選工具）
- **`Imaging/`** — `FloatImageBuffer`（float RGBA）、`LibRawInterop`（P/Invoke，解碼跑在 64MB 大堆疊執行緒）、`WicDecoder`（一般格式 + EXIF 方向）、`CacheManager`（縮圖 JPEG / proxy PNG / `.f32` 浮點快取）、`RawLoader`（LibRaw→ExifTool 預覽→WIC 退回鏈）、`ImageProcessor`（★11 步管線、平行；**10c 暗角 `Vignette`**：裁切後套用、三個回傳路徑(SkipGeometry/SkipCropRect/完整)都經 `Finish()`，從 ~1/3 半徑起 smoothstep，**正值壓暗四角**(滑桿往右=暗角越重)）、`ToneCurve`、`ImageStats`
- **`Exif/ExifReader`** — ExifTool（`#` 後綴取數值欄位）+ WIC 退回 + 嵌入預覽擷取
- **`Storage/`** — `AdjustmentXmlStore`（Save/Load/EnsureDefault/LoadExif/IsDefaultPlaceholder）、`PreviewListStore`（隱藏項 + 虛擬副本）、`PresetStore`（風格檔覆寫：Save/Get/**Remove**/**HasOverride**/ApplyCustom/**CustomNames**(presets.xml 中非內建的自訂名稱)/**ResetAllToDefaults**(清空整份 presets.xml)；套用時**優先用覆寫值、沒有才用 `PresetProfile` 內建值**）
- **`Rendering/RenderScheduler`** — debounce + 遞增版本 + CancellationToken，只有最新算圖回寫 UI；UI SyncContext 在 `Fire()`（UI 執行緒）延遲擷取
- **`Export/`** — `ExportSettings`（位置/次資料夾/**重新命名 `RenameMode`**(Original 按原檔`_edited` / DateTime `IMG{yyMMddHHmmss}{NN}`，NN 為同秒數末兩碼計數 / Sequence `IMG00001`)/**存檔同名 `ConflictMode`**(AppendNumber `_1,_2…` 預設 / Overwrite 直接覆蓋)/格式/**`MaxLongEdge` 寬長最大值**(長邊上限，維持比例、不放大，預設 3200)/**Resolution DPI**(預設 300)/JPEG 品質(預設 100)/PreserveExif/**標誌(浮水印) Watermark\* 欄位 + `BuildWatermark()`**：全域覆蓋，Enable 預設關；Arial/150/透明20%/右下/邊緣30）、`Exporter`（讀 full→套用→縮放/center-crop→依 RenameMode 命名→依 ConflictMode 解決同名→`SetResolution` 寫 DPI→開檔總管；DateTime 取 EXIF `DateTaken`，抓不到退回檔案修改時間）
- **標誌/浮水印是全域設定，非每張編輯**：`Models/WatermarkSpec`（Enabled/Text/FontName/FontSize/Transparency/Position/Margin）由 `ExportSettings.BuildWatermark()` 產生，透過 `ProcessContext.Watermark` 傳入 `ImageProcessor.ApplyWatermark(bmp, ctx)`。匯出時 `Exporter` 帶入；主預覽由 `MainForm._exportSettings.BuildWatermark()` 帶入(勾選 Enable 才畫)。**`ImageAdjustments` 已無 watermark 欄位；`ToolMode` 已無 `Watermark`**。無陰影(只畫白字)。
- **`Controls/`** — 自繪深色：`Theme`（色盤/字型）、`AdjustmentSlider`、`ImageViewer`（縮放平移 + crop/gradient/heal overlay + WB 滴管；**左鍵單擊循環 適合→100%→200%，左鍵拖曳平移**，見下方注意事項；**漸層**：畫出所有線性漸層，選取中那個顯示 白(位置，可 2D 拖曳)/黃(範圍，沿軸)/藍(角度，白點右側固定偏移)三手把，點其他漸層白點可切換選取(`GradientSelectionChanged` 事件→ToolsPanel 重新綁定)，白點按右鍵跳出「刪除此線性漸層」選單；空白處點擊不新增）、`ThumbnailStrip`（多選 Ctrl/Shift + `SelectAll`/`InvertSelection`/`DeselectAll`；**取得焦點時 Ctrl+A 全選**(`ControlStyles.Selectable` + `OnKeyDown`)；捲軸 `BarH=16`）、`HistogramControl`、`SectionPanel`（`ManualLayout`+`ContentArea` 固定尺寸絕對定位）、`FlatButton`（`Primary` setter 會 `Invalidate`）/`IconButton`/`TopTab`（分頁切換，`AllowDeselect` 可取消選取→`SelectedIndex=-1`）、`CardPanel`、`DarkGroupBox`（深色 GroupBox：圓角 PanelBg + 邊框，標題在框線上；匯出視窗用它分區）、**`DarkScrollHost`**（深色垂直捲動容器，包住左右欄 FlowLayoutPanel；內容超高時右緣出現 10px 自繪捲軸並把內容縮窄 10px——**捲軸與內容不重疊**，重疊兄弟控制項的 z-order 在 WinForms 不可靠、DrawToBitmap 也會反序；滾輪用 IMessageFilter 依游標位置轉送：游標在 AdjustmentSlider 上仍是微調數值、焦點輸入控制項保持原生行為、其餘捲動欄位）、`UiFactory`（暗色 Combo/Text/Check/Numeric）、`VStackPanel`、`PaintHelpers`
- **`Panels/`** — `AdjustPanelBase`（CreateSlider/AddSliderAt + Bind + EditBegin/Changed）、`BasicAdjustPanel`/`ColorPanel`(色溫內部一律 Kelvin(5200 中性)；**RAW 顯示 Kelvin 2000-12000**，**非 RAW(jpg/bmp…)改用置中 0 的 ±100 暖冷刻度**(`SetTemperatureMode(isRaw)`，`MainForm.RebindAll` 依 `AppPaths.IsRaw` 呼叫)；`HideWhiteBalanceRow()` 供編輯風格檔視窗隱藏白平衡列)/`DetailPanel`、`PresetPanel`（310×106，標題「**風格檔種類**」：combo(內建+自訂，`ReloadNames()` 重建) + 「**套用該風格檔**」按鈕(唯一事件 `ApplyPreset`，combo 改選**不再**自動套用)；儲存/恢復/新增移到 `PresetEditorForm`）、`ToolsPanel`（Ribbon：**`TopTab` 分頁 (裁切/漸層/修護, `AllowDeselect`) + 3 疊放 ribbon**，**`SelectedIndex=-1` 預設不選、再點同一分頁取消選取、未選時 `_ribbonHost.Enabled=false` 鎖住參數；各 ribbon 底部有 裁切/修護重設。標誌已移到匯出視窗**；**漸層 ribbon**：滑桿編輯「目前選取的線性漸層」(`ActiveGradient`，沒有時停用)，底部「**新增線性漸層**」按鈕(按了才新增，可多個，新的預設在最上方)+「漸層重設(清除全部)」。**曾試過改成 DarkGroupBox 手風琴，使用者看過後要求改回**(備份 `backup\ToolsPanel.TopTab.cs.bak` 是現行 TopTab 版)）、`InfoPanel.cs`（含 `HistogramPanel` 290×158 + `PhotoInfoPanel` + `ExifView`）
- **`Forms/`** — `MainForm`（整合核心；**多選批次編輯 + 批次 undo**，見下；App 選單：「關閉資料夾」/「**關閉資料夾並刪除快取縮圖**」(`DeleteCacheFiles`：只刪 `_thumb.jpg`/`.rawpipe.png`/`.f32`，**保留調整 XML 與 preview_list**；刪前 `GC.Collect` 釋放未 Dispose 的 proxy 檔鎖)/「**還原已隱藏的照片（N 張）**」(清空 preview_list Hidden→重開資料夾；無隱藏時停用)/「**重新整理資料夾 (F5)**」/「設定…」/「**編輯風格檔…**」(開 `PresetEditorForm`，關閉後 `_preset.ReloadNames()` 刷新下拉)/「**關於**」(在「結束」上方，開 `AboutForm`)；**「紀錄」子選單用黑字**(淺色下拉選單灰字看不清)；**刪除檔案走資源回收桶**(`Microsoft.VisualBasic.FileIO`，快取檔仍直接刪)；**Redo**：`_redo` 堆疊，`DoUndo` 前先推現狀、`PushUndo`/切圖/關資料夾清空，**批次同步不重播**(redo 只還原目前照片)；**快速鍵**(`OnKeyDown`，`IsTextInputActive()` 打字時全部跳過)：←→切圖、Ctrl+Z/Ctrl+Y、**Del=移除於預覽列(選取全部)、Shift+Del=刪除檔案、Esc=取消滴管→取消工具、F5=重新整理、`\`=對照原圖**；**拖放資料夾/圖檔**到視窗即開啟(`GetDroppedFolder`)；**對照原圖**：`_showOriginal` 由 JobFactory 換成「中性調整+保留幾何(裁切/旋轉/廣角變形)」重算，編輯手勢(PushUndo)與切圖自動關閉；**無照片狀態**：`ClearEditor`(存回未儲存編輯→清空 viewer/直方圖/EXIF→面板綁到 new `ImageAdjustments()` 預設值→`SetEditorEnabled(false)` 停用 基本/色彩/細節/工具/風格檔 面板 + 三顆重設/恢復按鈕 + 對照原圖)——啟動時、開到空資料夾、`CloseFolder`、以及**預覽列最後一張被移除/刪除**(`RefreshStripKeepSelection` 見 `_items.Count==0` 走 `ClearEditor`)都會進入；`ApplyLoaded` 載入成功後 `SetEditorEnabled(true)` 恢復）、`FolderPickerForm`、`ExportForm`（**兩欄 1048×572，區塊用 `DarkGroupBox`**：左欄 儲存位置→次資料夾→重新命名→存檔遇到相同檔名；右欄 格式與尺寸(含**解析度 DPI** combo)→**浮水印**(啟用+文字/字體/大小/顏色/透明度/位置/邊緣)(啟用勾選+文字/字體/大小/透明/位置/邊緣)。標誌控制項改動即 `CommitWatermark()` 寫回 settings 並 `WatermarkChanged` 事件→主預覽即時重繪）、`SettingsForm`、`AboutForm`（關於 380×290：「作者:」+ **名字與 Email 以嵌入資源 `src\Assets\email.png` 圖片顯示、非文字**「Awaysu (awaysu@gmail.com)」(csproj `EmbeddedResource`，重繪圖片時用 GDI+ 以 Segoe UI 10pt/#E0E0E4/背景 #1E1E1E 產生) + 「Source Code:」GitHub 連結(LinkLabel，點擊 `Process.Start` 開瀏覽器，https://github.com/awaysu/AwayPhotoRawEditor) + 版本(取 `AppVersion.Version`)）、`PresetEditorForm`（**編輯風格檔** 598×732：左側清單=內建(除「預設時設定」)+自訂(顯示「（自訂）」後綴，owner-draw 深色)；右側**重用** 基本調整/色彩/細節 面板（記得 `Dock=None` 再設 Location——SectionPanel 預設 Dock.Top、`SetTemperatureMode(true)`、`HideWhiteBalanceRow()`）；修改於切換選取/關閉時自動 commit（值沒變不寫；內建改回預設值自動移除覆寫）；「新增」以目前顯示值建立自訂風格檔；「恢復預設」`MessageBox` 確認後 `PresetStore.ResetAllToDefaults()`）、`ProgressForm`（可取消，快取產生 + 匯出共用；建構子可選 `subtitle`(常駐副標，兩行置中)/`doneMessage`(完成後滿格短暫顯示~0.85s)；`Text=""` 不顯示 OS 標題，只留樣式化 header 一個標題）
- **`Diagnostics/`** — `SelfTest`、`GalleryForm`、`Trace`（僅診斷用，可保留）

## UI 版面（MainForm）
Top Bar (Dock Top, 60) → 縮圖列 (Dock Top, **158**) → body = 3 欄 TableLayout（左 330 / 中 fill / 右 320）。
- **縮圖列**：`ThumbnailStrip` 高 158，縮圖以**寬度**縮放（約 172×115，橫幅），高度貼合縮圖不留空白；底部橫向捲軸 `BarH=16`（加粗好抓）。
- **左欄**（FlowLayoutPanel TopDown，無捲軸，間距 10）：基本調整(310×245) → 色彩(310×210) → 細節(310×145，銳利度/**暗角**/降噪) → 「**基本／色彩／細節 重設**」按鈕(310×30) → 套用風格檔(310×106)。
- **中央**：Viewer + 底部工具列(36：適合/100%/200%/**對照原圖**(toggle，`Primary` 顯示開關；也可按 `\`；以中性調整+保留幾何重算，編輯手勢或切圖自動關) + LibRaw 標籤 Dock Right 160)。
- **右欄**（FlowLayoutPanel TopDown，無捲軸）：直方圖(290×158) → 照片資訊(290×285) → 工具(290×355, Ribbon)；欄底 **Dock Bottom Panel(96)**：**全部重設**(呼叫 `ResetAllAdjustments`) + **恢復上一步**(`DoUndo`) 兩顆固定按鈕。
- 色彩：window `#1E1E1E` / 面板 `#252525` / 工具列 `#2D2D2D` / viewer `#141414`。
- 各 Section 固定尺寸、內容絕對定位（見 `SectionPanel.ManualLayout`）。
- 左右欄以 **`DarkScrollHost`** 包住（右欄僅捲動上方區塊，底部「全部重設／恢復上一步」固定不捲）：視窗夠高時無捲軸；太矮時右緣出現 10px 深色捲軸可捲動，內容縮窄 10px（左欄 320 ≥ 10+310、右欄 310 ≥ 15+290，區塊不會被裁到）。**由設定「顯示捲軸」開關（`ShowColumnScrollBars`，預設關）**，關閉時 `ScrollEnabled=false` 完全回到舊行為（滿寬、超出裁切、不攔滾輪）。
- 左欄區塊間距：上 padding 4；基本調整→色彩、色彩→細節之間 4px，細節之後維持 10px。右欄上 padding 同為 4 對齊。
- 重設按鈕分布：左欄「基本／色彩／細節 重設」(只重設這三區, `ResetBasicColorDetail`)、右欄底「全部重設」(`ResetAll`) / 「恢復上一步」(`DoUndo`)、工具各 ribbon 底「裁切/漸層/修護重設」(標誌已移到匯出視窗)。**縮圖多選時,重設按鈕與滑桿調整都會套用到全部選取的照片**(見「多選批次編輯」)。

## 資料流
開資料夾 → 忽略 RAW_TEMP、掛載 `preview_list.xml`(隱藏/虛擬副本) → `ProgressForm`「產生快取（縮圖＋預覽）」(副標兩行、置中「第一次產生快取與縮圖檔案需要一些時間\n請稍等...」，完成顯示「完成，可以開始編輯」)：每張做**縮圖 + `EnsureProxyCache`**（proxy 全解碼很慢，故**開資料夾時一次全部做好，之後點選即時**；proxy 用獨立 `SemaphoreSlim(2)` 限併發防 OOM，並依 `SourcePath` 去重讓虛擬副本共用）→ 選圖(背景載入 XML+EXIF+proxy，此時 proxy 已快取命中)→`ScheduleRender(immediate)` → 拉滑桿(EditBegin 推 undo → debounce 背景算圖 → 更新 viewer/histogram/均值/EXIF/縮圖 edited 標記) → 切圖存 dirty → 匯出。

### 多選批次編輯（`MainForm`）
縮圖多選(>1)時：
- **重設**(全部重設 / 基本色彩細節重設, `ResetSelected`)：對目前照片即時、對其他選取照片**立即** load→reset→save，馬上清掉 edited 標記。
- **滑桿 / WB / 旋轉等**：在編輯手勢開始(`PushUndo`)時擷取「其他選取照片清單 `_syncTargets` + 調整前 baseline `_syncBaseline`」，只把**你動過的欄位**(`ApplyDelta`)同步到其他照片；當下先亮 edited 標記回饋，實際寫入延到目前照片**提交時**(`SaveCurrentIfDirty`→`FlushBatchSync`，即切圖/匯出/關資料夾)一次完成，避免拖曳時對上百張猛寫檔。
- **批次 undo**：`_undo` 為 `UndoStep{Current, Others}`；批次手勢的第一步會快照所有受影響照片的舊調整，`DoUndo` 會把它們一起還原並取消尚未 flush 的同步。**與單張 undo 一致，切圖會 `_undo.Clear()`**，故批次還原僅在切圖前有效。

## 慣例 / 注意事項（踩過的坑）
- **`ImageAdjustments.ValueEquals` 只比對可寫屬性**（`if (!p.CanWrite) continue;`）。`IsDefault` 是 computed 屬性 `=> ValueEquals(Default)`，若用反射走訪到它會**無限遞迴 → stack overflow**（只在全預設值、無 short-circuit 時觸發）。新增 computed 屬性務必確保被排除。
- **`LoadPhoto` 有去重 + `_loadVersion` 版本控制**，且**不 Dispose proxy**（交給 GC）——避免背景算圖還在讀 proxy 時被釋放（use-after-free，會 0xC00000FD/0xC0000005）。
- **UI handle 未建立前不可 `BeginInvoke`**：自動開啟上次資料夾放在 `OnShown`，不要放建構式。
- **`SectionPanel`/`VStackPanel` 有 `_inRelayout` 重入防護**（設 Height → OnResize → Relayout 迴圈）。
- **WinForms Dock 疊放順序**：最後加入的先 dock。要 Top Bar 佔滿頂端→最後加它；Fill 最先加。
- **`VStackPanel` 子控制項不要再設 `Dock`**（手動座標排版會衝突）。
- **LibRaw RAW 全解析度解碼在執行緒集區(1MB 堆疊)可能溢位** → `LibRawInterop.RunLargeStack` 用 64MB 堆疊執行緒跑原生解碼。
- **ExifTool**：文字欄位(WhiteBalance/MeteringMode)保留友善字串，數值欄位用 `-Tag#` 強制數字。
- 高精度 proxy 以 `.rawpipe.png.f32` 浮點無損快取實現（非字面 16-bit PNG）。
- 讀檔/解碼失敗全程 try/catch 退回，不崩潰；長時間操作皆 async + `ProgressForm` 可取消。
- **`FlatButton.Primary` 是「有 backing field + `Invalidate`」的屬性**，不是自動屬性——早期版本是自動屬性，切換工具高亮時舊鈕不重繪，導致**兩顆同時看起來被選**。自繪控制項的視覺狀態屬性都要在 setter 觸發重繪。
- **`ImageViewer` 左鍵：點擊 vs 拖曳**用 `PanThreshold`(4px) 區分。左鍵按下起 `Drag.Pan` 但移動未超過門檻不平移；放開時若沒移動 → `CycleZoomAt`（適合→100%→200%→適合）。**平移時不可把 `_zoomMode` 改成 `Custom`**（要保留 100%/200%），否則平移後再點擊會跳回「適合」而非接續循環；只有滾輪才進 `Custom`。
- **開資料夾預先產生 proxy 很吃時間/記憶體**：RAW 全解碼 × 全部照片。用小號 `SemaphoreSlim(2)` 限併發、依來源去重；第一次開資料夾會較久（可取消），換來之後點選即時。
- **多選批次同步的目標要在「編輯手勢開始」時擷取，不能在提交時抓**：單擊縮圖會先把選取變成單張、才觸發 `SelectionChanged`→`LoadPhoto`→`SaveCurrentIfDirty`，所以提交當下 `SelectedItems` 已經不是原本的多選。故 `_syncTargets` 在 `PushUndo` 擷取、`FlushBatchSync` 才使用。
- **`ApplyDelta` 只複製「有變動的純量欄位」，且跳過 `HealSpots`**(位置相關，不跨照片同步)。批次滑桿同步用它，才不會把別張的裁切/白平衡/局部修護洗掉。
- **風格檔覆寫模型**：所有編輯/新增/恢復都在 `PresetEditorForm`（App 選單「編輯風格檔…」）。存全部欄位但 `ApplyCustom` 套用時只取 tonal/色彩/細節；內建風格檔的修改存成 PresetStore 覆寫（改回與內建預設完全相同時自動 `Remove` 覆寫）；自訂風格檔即 presets.xml 中非內建名稱的項目。**「預設時設定」(`DefaultName`, fullReset) 不在編輯清單、名稱不可用於自訂**。
- **`ProgressForm` 的 `doneMessage` 延遲用 `Task.Delay(_, _cts.Token)`**；完成訊息只在未取消時顯示，取消/例外照舊直接關閉。

## 快取檔（各資料夾 `RAW_TEMP/`）
`{file}_thumb.jpg`（縮圖）、`{file}.rawpipe.png`(+`.f32`)（proxy）、`{file}.rawpipe.xml` / `{file}.copyN.rawpipe.xml`（調整）、`preview_list.xml`（隱藏 + 虛擬副本）。
