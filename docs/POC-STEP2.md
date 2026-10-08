# 第 2 步：wgpu GPU 管線（2026-10-08）

目標：把 Swift 版的 Metal 管線移植成 **wgpu + WGSL**，一份 shader 在三個平台跑
（Windows Direct3D 12、macOS Metal、Linux Vulkan），並用 Swift `gputest` 的 8-bit 標準驗證與 CPU 一致。

## 結論

| 平台 / GPU / 後端 | gputest（17 組 × 檔案數） | 最大 float 差 | 8-bit 差≥2 |
|---|---|---|---|
| Windows · Intel Iris Xe · **D3D12** | ✅ 4 公開樣本 + 4 個使用者檔案（含 61 MP 7RM6），全部 17/17 | 1.41e-4 | 0 |
| macOS · Apple M2 · **Metal** | ✅ 4 公開樣本，全部 17/17 | 1.41e-4 | 0 |
| Linux · NVIDIA RTX 3060 · **Vulkan** | ✅ 4 公開樣本，全部 17/17 | 1.56e-4 | 0 |

- 標準照 Swift / Windows 版：8-bit 差≥2 不允許、差 1 ≤ 0.07%。實測差 1 最多 0.055%，差異都在幾何階段
  （CPU 用 f64、WGSL 沒有 f64，同 Metal / D3D 的取捨）。
- **與 Swift 版自己的 Metal 精度相同**：同一台 M2、同一個檔案，Swift `gputest` 最大差 1.52e-4、差 1 最多 0.051%；
  Rust wgpu 1.41e-4、0.046%（`tests/results/gpu/macos-m2-metal/swift-sony.txt` 是 Swift 的報告）。
- 修護案例證明修護真的有作用：改動像素數 18601 / 12849 / 33523 px，**與 Swift 版記錄的 18601 逐位相同**。
- 第 1 步沒有退步：重構後 macOS 的 CPU hashtest 仍是 Swift 56/56 逐位元相同，Windows 輸出與重構前逐字元相同。

## 架構

- `crates/core::pipeline`：步驟順序只寫一次（`run_pipeline`），經 `StageTarget` trait 驅動——就是 Swift 版留的那個縫。
  `CpuTarget`（參考實作）與 `awpr-gpu::GpuTarget` 共用同一份順序與參數建構。
- `crates/gpu`：
  - `shaders.rs`：6 個 kernel（融合像素階段、模糊 H/V/合成、重取樣、90° 旋轉），**逐行對照 Swift 的 Metal kernel**；
    轉換曲線與色調曲線是上傳同一張 LUT，不在 shader 裡算 `pow()`。
  - `GpuPipeline::upload` → `render` → `GpuFrame`：**來源常駐 GPU、結果留在 GPU**。編輯器開照片時上傳一次 proxy，之後每次拉滑桿
    只跑 kernel，畫面直接從 GPU buffer 畫（第 3 步的 egui/wgpu 介面會這樣接）；`download()` 只在匯出時用。
  - 修護維持 CPU（同 Swift）：讀回 → `pipeline::heal` → 上傳。
  - 緩衝區池（依大小回收，上限 512 MB）、讀回用的 staging 也回收。同一個 queue 依序執行，所以回收不必等 GPU。
  - 失敗（沒有 GPU、shader 編不過、超過尺寸上限、驗證錯誤）一律回 Err，呼叫端改走 CPU。
- Windows 預設 **D3D12**：Intel Iris Xe 上像素階段 D3D12 15 ms、Vulkan 42 ms。`WGPU_BACKEND` 仍可覆蓋。

## 效能（24 MP Sony A7M3；GPU 為「常駐」：來源已在 GPU、不讀回，連續 10 次取最好）

| 綜合調整 | proxy CPU | **proxy GPU** | 全圖 CPU | **全圖 GPU** |
|---|---:|---:|---:|---:|
| Windows · Iris Xe（D3D12） | 158 ms | **69 ms** | 828 ms | **636 ms** |
| macOS · M2（Metal） | 62 ms | **25 ms** | 352 ms | **162 ms** |
| Linux · RTX 3060（Vulkan） | 114 ms | **7.1 ms** | 622 ms | **61 ms** |

M2 上與 Swift 版 Metal 同條件比較（`awpr-cli bench` proxy GPU 欄）：

| 階段 | Swift Metal | **Rust wgpu** |
|---|---:|---:|
| 白平衡+曝光 | 11 | **4.1** |
| 色調曲線 | 34 | **4.1** |
| 降噪+銳利化 | 26 | **20** |
| 漸層 | 10 | **4.6** |
| 暗角 | 17 | **5.6** |
| 裁切+角度 | 9 | **7.3** |
| 廣角變形 | 8 | **5.5** |
| 綜合 | **22** | 25 |

（Swift 的 GPU 欄含把來源 memcpy 進 shared buffer，Rust 的常駐欄不含——所以不是完全同條件，結論是「同一級」。）

### 量到的事

- **獨立顯卡會降頻**：RTX 3060 閒置後第一次 render，同一個像素 kernel 5 ms；連續跑時 0.55 ms。
  `awpr gputest` / `bench` 因此取「連續 10 次最好」，代表拖滑桿時的情況。`AWPR_GPU_PROFILE=1` 會印每個 kernel 的 GPU 時間（timestamp query），
  `scripts/gpu-profile.sh` 整理成每組一行。
- **修護比 Swift 慢**：wgpu 沒有 Metal 的 shared memory，修護要整張 70 MB 讀回再上傳。M2 上修護案例 Swift 7 ms、Rust 25 ms；
  RTX 3060 要過 PCIe，83 ms（CPU 57 ms）。這是目前唯一比 Swift 慢的地方。改法：只讀回／上傳修護圓的外接矩形（幾十 KB），或把修護寫成 kernel。
- **16 MP 尺寸上限**（沿用 Swift 在 M2 量的值）在這些 GPU 上偏保守：24 MP 全圖 GPU 在 M2 快 2.2×、RTX 3060 快 10×。
  Swift 當初是在 66 MP 時看到換頁（8 GB M2）。上限要不要依 GPU 記憶體調整，留到有 60 MP 級的 GPU 實測再決定；目前預設不變。

## 指令

```bash
awpr gputest <raw> [report]          # 17 組 CPU vs GPU，8-bit 標準
awpr bench <raw>                     # CPU / GPU、proxy / 全圖四欄
scripts/gputest-all.sh <awpr> <samples> <out>
AWPR_GPU_PROFILE=1 awpr gputest <raw>   # 每個 kernel 的 GPU 時間
WGPU_BACKEND=vulkan awpr gputest <raw>  # 指定後端
```

報告：`tests/results/gpu/<平台>/`。

## 下一步（第 3 步：介面）

egui + wgpu 視窗：開資料夾 → 縮圖列 → 大圖從 `GpuFrame` 直接畫 → 右側滑桿即時更新。
需要先決定：非 RAW 格式（JPEG/HEIC/TIFF）的解碼 crate、`rawpipe.xml` 讀寫（.NET XmlSerializer 相容）。
