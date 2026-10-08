# CHANGELOG

## 2.0.0-dev（2026-10-09）

Rust 重寫：PoC 第 1–3 步搬入 main。

- **核心 + LibRaw**：LibRaw 0.22.2 原始碼直接編入；CPU 管線與 Swift 版 `hashtest` 4 檔 × 14 組 56/56 逐位元相同，Windows 解碼與 C# 版 `libraw.dll` 逐像素相同、速度約 2×。
- **wgpu GPU 管線**：同一份 WGSL 在 D3D12 / Metal / Vulkan 上跑，與 CPU 版差 ≥2 的像素為 0，精度與 Swift Metal 相同；Windows 優先用 D3D12。
- **egui + wgpu 編輯視窗**：開資料夾、縮圖列、大圖直接從 GPU buffer 畫、滑桿即時更新；`--shot` 離屏截圖可做 headless 驗證。
- **與舊版互通**：`RAW_TEMP` 快取、`rawpipe.xml`、`preview_list.xml` 與 C# / Swift 版逐位元組相容。
- 已知問題：Windows 新建置的 exe 以 `--shot` 執行時約 2.4 秒無聲結束（PoC 原始 exe 正常），疑為本機安全機制，待確認。
