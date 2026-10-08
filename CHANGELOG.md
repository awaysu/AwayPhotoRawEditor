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
- **設定與語言**：八語介面（C# 266 條原樣移植＋87 條新字串，測試掃描未翻譯字串）、各語言字型、第一次執行語言選擇、設定視窗（介面大小、字體大小 12 級、RAW 精度、GPU）、☰ 選單（最近開啟、還原隱藏、相機列表 1258 種）、關於視窗與檢查更新。三平台 43 個測試通過。功能面至此與 C# v1.0.18 對等。
- **打包與 CI**：Windows Inno Setup 安裝檔（per-user、八語、簽章）＋攜帶版 zip；macOS universal .app／DMG（Developer ID 簽章、公證、staple，Gatekeeper 驗收通過）；Linux .deb／.rpm；GitHub Actions 三平台 CI；`docs/RELEASE.md`。版本號單一來源 Cargo.toml。
- **色彩管線升級（處理版本 3）**：RAW 改從 LibRaw 的線性相機 RGB 開始（不套白平衡／矩陣／gamma），全程 f32 線性、Rec.2020 工作色域，最後才轉 sRGB 並做色域壓縮（不再出現高飽和截色、天空青色截斷）。新增**高光復原**（全解析度重建截掉的色版＋柔和高光肩部）、**HSL**（8 色帶 × 色相／飽和度／明度，OkLCh）、**曲線**（RGB＋紅綠藍，單調三次樣條，點可拖曳／新增／刪除）；GPU（WGSL）同步實作，三平台 `awpr gputest3` 18 組全部通過。新照片預設處理版本 3；舊照片維持原算式（hashtest 4 檔逐位元組不變），「升級處理版本」2→3 曝光與白平衡不變。`rawpipe.xml` 只在最後追加新欄位。新增 **XMP 匯出／匯入**（Camera Raw `crs:` 標準欄位＋自家命名空間保存全部調整）。三平台 62 個測試通過。
- **兩階段背景快取**：開資料夾先只建版本 2 快取（縮圖、8-bit 代理，速度與以前相同，資料夾立即可用），整個資料夾做完後再於背景逐張建處理版本 3 的線性代理；打開尚無代理的照片會插隊、先用 8-bit 來源顯示，代理好了自動換源並重畫縮圖。同一張兩份都缺時只做一次 LibRaw unpack。
- 已知問題：Windows 新建置的 exe 以 `--shot` 執行時約 2.4 秒無聲結束（PoC 原始 exe 正常），疑為本機安全機制，待確認。
