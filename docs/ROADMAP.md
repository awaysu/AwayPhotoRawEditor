# ROADMAP — Rust 跨平台版開發計畫（2026-10-09 擬定）

原則：每一步 = Agent-12 實作 → Agent-11 審查 → 修正 → 下一步。每步都要有可重複的驗證。
功能面以 C# v1.0.18（`legacy/windows`，見其 `CHANGELOG.md`）為基準，不可退步。

| 步驟 | 任務 | 內容 | 驗證 | 狀態 |
|---|---|---|---|---|
| 0 | TASK-001 | 把 PoC（crates/docs/scripts/tests）搬進本 repo，建置、測試、hashtest 無退步，`.gitignore`、`LICENSE`、`CHANGELOG.md` 就位 | `cargo test --workspace`、hashtest 與 `tests/reference` 相同 | 完成 2026-10-09 |
| 1 | TASK-002 | 工具互動編輯：裁切框（含角度拉直、預設比例、四角加大抓取）、線性漸層手把（白/黃/藍點、旋轉手把）、修護圓（仿製/修補切換、拖曳、刪除）、白平衡滴管 | `--shot` + `AWPR_SHOT_*` 截圖；XML 寫入與 C# 相同格式 | 完成 2026-10-09 |
| 2 | TASK-003 | 匯出：ExportForm 全部選項（格式 JPEG/PNG/TIFF、品質、符合寬高、DPI、重新命名、浮水印、去重）、**16-bit TIFF/PNG 並嵌 sRGB ICC**、匯出進度、匯出隱藏過濾 | `awpr exporttest`（移植 C# `--exporttest`） | 完成 2026-10-09 |
| 3 | TASK-004 | 風格檔（PresetStore、編輯視窗、備份/還原全部、套用時不改白平衡）、多選批次同步、複製/貼上設定、虛擬副本、隱藏/取消隱藏/顯示全部、刪除、升級處理版本 | 單元測試 + `--shot` | 進行中 |
| 4 | TASK-005 | 設定視窗（介面大小、字體大小 12 級、RAW 精度、GPU 開關、捲軸、恢復預設）、8 種語言（移植 `Localization.cs`）、第一次執行語言選擇、關於／檢查更新／支援相機列表 | i18n 完整性測試（每個 key 八語） | 未開始 |
| 5 | TASK-006 | 打包與 CI：Windows（Inno Setup + 簽章，照 legacy CLAUDE.md）、macOS（universal DMG + Developer ID + 公證）、Linux（.deb/.rpm）；GitHub Actions 三平台跑 `cargo test` + hashtest；awaysu.cc 上傳腳本 | CI 綠燈 | 未開始 |
| 6 | TASK-007 | 色彩管線升級＝處理版本 3：LibRaw 輸出線性寬色域（Rec.2020）f32、高光復原（lightcraft highlight.rs）、HSL（OkLCh）、曲線、XMP 匯出/匯入（lightcraft xmp.rs） | gputest 新版 14 組；舊版照片 hashtest 不變 | 未開始 |
| 7 | TASK-008 | 放射狀／筆刷遮罩；HEIC（libheif）評估 | | 未開始 |

每步完成：更新本表「狀態」、`CLAUDE.md` 目前狀態、`CHANGELOG.md`，commit 到 main。

## 已知差異（相對 C# 版，刻意保留或待日後處理）
- 滴管取樣位置：照片有裁切／旋轉時，取樣點與點擊位置不一致（C# 本來就如此）。要修需做幾何逆變換。
- Delete 鍵：工具分頁有選取項目時刪該項目；沒有選取時才是「隱藏照片」（C# 一律隱藏）。
- 修護：選取中的圈畫粗（C# 看不出選取）。
- 匯出縮放用面積平均（C# 用 bicubic），浮水印字形用 ab_glyph（C# 用 GDI+）：位置算法相同，但像素不逐位元相同。
- TIFF 匯出只寫基本標籤（Make／Model／DateTime／Software），沒有 EXIF 子 IFD；JPEG／PNG 有完整 EXIF。
- 浮水印位置維持 C# 的四角（export.xml 相容）。
- 全解析度匯出超過 16 MP 走 CPU（C# 有分段 GPU BandedTarget）：列為第 6 步之後的效能項目。
