# CHANGELOG

## 2.0.0-dev（2026-10-09）

Rust 重寫：PoC 第 1–3 步搬入 main。

- **核心 + LibRaw**：LibRaw 0.22.2 原始碼直接編入；CPU 管線與 Swift 版 `hashtest` 4 檔 × 14 組 56/56 逐位元相同，Windows 解碼與 C# 版 `libraw.dll` 逐像素相同、速度約 2×。
- **wgpu GPU 管線**：同一份 WGSL 在 D3D12 / Metal / Vulkan 上跑，與 CPU 版差 ≥2 的像素為 0，精度與 Swift Metal 相同；Windows 優先用 D3D12。
- **egui + wgpu 編輯視窗**：開資料夾、縮圖列、大圖直接從 GPU buffer 畫、滑桿即時更新；`--shot` 離屏截圖可做 headless 驗證。
- **與舊版互通**：`RAW_TEMP` 快取、`rawpipe.xml`、`preview_list.xml` 與 C# / Swift 版逐位元組相容。
- **工具互動編輯**：裁切框（比例、角度即時拉直、四角優先抓取）、線性漸層手把（白／黃／藍、旋轉 icon）、修護圓（仿製／修補、拖曳、Delete）、白平衡滴管；`--shot` 加 `AWPR_SHOT_TOOL=crop|gradient|heal`。
- **匯出**：C# 匯出設定全部選項（位置／次資料夾／重新命名／同名處理／格式／長邊／DPI／品質／EXIF／浮水印）、匯出進度與取消；**TIFF／PNG 在 16-bit 精度時輸出 16-bit**、JPEG／PNG／TIFF 嵌入 sRGB ICC；浮水印跨平台繪製並可即時預覽；`awpr exporttest`。三平台 `cargo test` 29 個通過。
- **照片管理**：風格檔（內建＋自訂、編輯視窗、備份／還原、套用不改白平衡）、縮圖多選與批次同步（寫檔時 flush、復原含批次、重做不重播）、複製／貼上設定、虛擬副本、隱藏系統（斜線眼睛、跳號、顯示隱藏切換）、刪除到資源回收筒、升級處理版本。三平台 37 個測試通過。
- 已知問題：Windows 新建置的 exe 以 `--shot` 執行時約 2.4 秒無聲結束（PoC 原始 exe 正常），疑為本機安全機制，待確認。
