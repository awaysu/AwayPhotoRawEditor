# ROADMAP — Rust 跨平台版開發計畫（2026-10-09 擬定）

原則：每一步 = Agent-12 實作 → Agent-11 審查 → 修正 → 下一步。每步都要有可重複的驗證。
功能面以 C# v1.0.18（`legacy/windows`，見其 `CHANGELOG.md`）為基準，不可退步。

| 步驟 | 任務 | 內容 | 驗證 | 狀態 |
|---|---|---|---|---|
| 0 | TASK-001 | 把 PoC（crates/docs/scripts/tests）搬進本 repo，建置、測試、hashtest 無退步，`.gitignore`、`LICENSE`、`CHANGELOG.md` 就位 | `cargo test --workspace`、hashtest 與 `tests/reference` 相同 | 完成 2026-10-09 |
| 1 | TASK-002 | 工具互動編輯：裁切框（含角度拉直、預設比例、四角加大抓取）、線性漸層手把（白/黃/藍點、旋轉手把）、修護圓（仿製/修補切換、拖曳、刪除）、白平衡滴管 | `--shot` + `AWPR_SHOT_*` 截圖；XML 寫入與 C# 相同格式 | 完成 2026-10-09 |
| 2 | TASK-003 | 匯出：ExportForm 全部選項（格式 JPEG/PNG/TIFF、品質、符合寬高、DPI、重新命名、浮水印、去重）、**16-bit TIFF/PNG 並嵌 sRGB ICC**、匯出進度、匯出隱藏過濾 | `awpr exporttest`（移植 C# `--exporttest`） | 完成 2026-10-09 |
| 3 | TASK-004 | 風格檔（PresetStore、編輯視窗、備份/還原全部、套用時不改白平衡）、多選批次同步、複製/貼上設定、虛擬副本、隱藏/取消隱藏/顯示全部、刪除、升級處理版本 | 單元測試 + `--shot` | 完成 2026-10-09 |
| 4 | TASK-005 | 設定視窗（介面大小、字體大小 12 級、RAW 精度、GPU 開關、捲軸、恢復預設）、8 種語言（移植 `Localization.cs`）、第一次執行語言選擇、關於／檢查更新／支援相機列表 | i18n 完整性測試（每個 key 八語） | 完成 2026-10-09 |
| 5 | TASK-006 | 打包與 CI：Windows（Inno Setup + 簽章，照 legacy CLAUDE.md）、macOS（universal DMG + Developer ID + 公證）、Linux（.deb/.rpm）；GitHub Actions 三平台跑 `cargo test` + hashtest；awaysu.cc 上傳腳本 | CI 綠燈 | 完成 2026-10-09 |
| 6 | TASK-007 | 色彩管線升級＝處理版本 3：LibRaw 輸出線性寬色域（Rec.2020）f32、高光復原（lightcraft highlight.rs）、HSL（OkLCh）、曲線、XMP 匯出/匯入（lightcraft xmp.rs） | gputest3 18 組三平台通過；舊版照片 hashtest 逐位元組不變 | 完成 2026-10-09（5247bdf、044f7e4） |
| 7 | TASK-008 | 放射狀／筆刷遮罩（僅處理版本 3，CPU 點陣化權重＋GPU 套用）；HEIC 評估 `docs/HEIC-EVAL.md` | gputest3 20 組三平台通過；exporttest 差 0 | 完成 2026-10-09（f7fcb80、aec9c3c） |
| 8 | TASK-009 | HEIC：macOS ImageIO、Windows WIC（缺延伸模組要提示）、Linux 執行時載入系統 libheif；不內嵌 libheif／libde265 | Linux／mac 實解（含 irot＋EXIF 6 直式、10-bit、P3）正確；Windows 只驗到缺 HEVC 延伸模組的提示路徑 | 完成 2026-10-09（14a7649） |
| 9 | TASK-010 | 收尾：>16 MP 匯出分段走 GPU、遮罩權重留在 GPU、遮罩面板在 1080p／1600×1000 可完整操作、DE/FR/ES 縮短、CI 首次實跑 | 三平台 cargo test 73、hashtest IDENTICAL、gputest／gputest3 全過、tiletest 8-bit 差 ≤1 | 完成 2026-10-09（fe5ddca、1dec452） |
| 10 | TASK-011 | 版本改 2.0.0（2026-10-10 使用者改為接續 C# 的 1.1.0，2.0.x 撤回）、CHANGELOG 整理成發佈說明、三平台重新打包（mac 簽章由 PM 跑）、使用者驗收、依 docs/RELEASE.md 發佈（使用者決定） | 三平台安裝檔 sha256；awaysu.cc check_update 三平台 update_available | 完成 2026-10-09（ba97288；2.0.0／2.0.1 發佈後撤回）；**1.1.0 於 2026-10-10 發佈**（7d6205e、tag v1.1.0、awaysu.cc、GitHub Release v1.1.0） |

每步完成：更新本表「狀態」、`CLAUDE.md` 目前狀態、`CHANGELOG.md`，commit 到 main。

## 處理版本 3 的決定（2026-10-09，PM）
- 最後編碼曲線沿用 BT.709（與版本 1／2 相同），升級 2→3 亮度一致；不改用 sRGB 曲線。
- 高光復原預設 0（新照片與升級後都是 0，樣子最接近版本 2）；要不要給新照片預設值，使用者可改。
- `rawpipe.xml` 不另存 `.v3.xml`：PM 用 .NET 9 ＋ legacy/windows 的模型類別實測，C# 1.x 讀版本 3 的 XML 不丟例外、`PipelineVersion=2` 保留；但 1.x 重存會丟掉 HSL／曲線／高光復原欄位（接受）。
- 版本 3 的線性代理快取是 16-bit PNG（`RAW_TEMP/{file}.rawpipe.v3.png`，約 20–25 MB／張），建快取分兩階段（044f7e4）：先建版本 2 快取（速度與舊版相同，可立即操作），整個資料夾做完後背景逐張補線性代理；選取沒有代理的照片會插隊，等待期間用 8-bit 來源以版本 3 算式顯示、代理完成後自動換源。不採用 2×2 合併的代理（預覽與匯出必須同一來源）。

## 遮罩與 HEIC 的決定（2026-10-09，PM）
- 遮罩的調整項與漸層相同（曝光、對比、亮部、暗部、飽和度），不加色溫／色調／清晰度；批次同步不複製遮罩（與漸層同規則）。
- HEIC 用各平台系統解碼器（Swift 版本來就是 ImageIO），不隨附 libheif／libde265，避開 LGPL 靜態連結與 HEVC 專利問題；Windows 使用者需要 HEIF／HEVC 延伸模組，缺時明確提示。

## 已知差異（相對 C# 版，刻意保留或待日後處理）
- 滴管取樣位置：照片有裁切／旋轉時，取樣點與點擊位置不一致（C# 本來就如此）。要修需做幾何逆變換。
- Delete 鍵：工具分頁有選取項目時刪該項目；沒有選取時才是「隱藏照片」（C# 一律隱藏）。
- 修護：選取中的圈畫粗（C# 看不出選取）。
- 匯出縮放用面積平均（C# 用 bicubic），浮水印字形用 ab_glyph（C# 用 GDI+）：位置算法相同，但像素不逐位元相同。
- TIFF 匯出只寫基本標籤（Make／Model／DateTime／Software），沒有 EXIF 子 IFD；JPEG／PNG 有完整 EXIF。
- 浮水印位置維持 C# 的四角（export.xml 相容）。
- Windows 的 WIC HEIC 實際解碼與方向未驗證（建置機沒有付費的 HEVC 延伸模組）；請有裝的使用者跑 `awpr heictest <iPhone 直式照片>` 確認四角方向。
- 超過 16 MP 的匯出已改為 GPU 分條帶（fe5ddca），結果與 CPU 8-bit 差 ≤1；速度與 CPU 互有勝負（版本 3 快 15–40%，版本 2 與 Mac 61 MP 慢 10–30%，卡在搬運與 CPU 尾段）。PM 決定維持 GPU 優先，條帶管線化留作日後效能項目。
- 「顯示捲軸」設定只管左欄；右欄需要時一定顯示（開工具時照片資訊收成一行）。設定保留以維持 settings.xml 互通。
- 編輯風格檔視窗多一顆「刪除這個自訂風格檔」（C# 只能恢復預設一次刪光）。
- 刪除照片時一併移除其虛擬副本與 XML（C# 會留下指向不存在檔案的副本，視為 C# 疏漏）。
- 語言／介面大小／字級即時套用不重啟（C# 重啟）。第一次執行判定：settings.rust.xml 與 settings.xml 都不存在才跳語言選擇。
- 介面風格（經典深色／暖白相紙）未做，欄位保留往返。相機列表顯示 LibRaw 內建清單（C# 開瀏覽器）。
- 德／法／西文在右欄工具分頁有幾處截字（hover 看全文）；待日後逐條縮短翻譯。
