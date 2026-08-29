# macOS 兩個版本的比較：C#（Avalonia port）vs Swift（AppKit）

2026-08-30，Mac mini M2、macOS 26.6.2。兩邊都是 **release 建置**、同一批檔案（複本）、依序執行（不並行）。

- C#：`~/workspace1/AwayPhotoRawEditor_mac/build/AwayPhotoRawEditor.app`（2026-08-28 publish，self-contained，Homebrew libraw 0.22.2 + libomp/libjpeg/lcms2 + exiftool）
- Swift：`.build/release/awpr-cli`（自建 libraw 0.22.2，`-DLIBRAW_NOTHREADS -DNO_JPEG -DNO_LCMS`，ImageIO 讀 EXIF）

量測條件刻意對齊：Swift 的 `awpr-cli cmpbench <img> [--gpu] [--render-only]` 逐項照 C# 的
`--decodetest`／`--rendertest`／`--enginetest`（同樣縮到 2400 px、同六組調整含兩組浮水印、單次冷跑、CPU）。
原始輸出：`cs_results.txt`／`sw_results.txt`（session scratchpad `cmp/`）。

## 1. RAW 全解析度解碼（8-bit，ms，取第二次執行）

| 檔案 | 像素 | C# | Swift | Swift(float) | C# 快 |
|---|---|---|---|---|---|
| Sony ILCE-7RM6 ARW | 60 MP | **3335** | 4929 | 4584 | 1.5× |
| Sony ILCE-7RM5 ARW | 60 MP | **2891** | 4182 | 3843 | 1.4× |
| Sony ILCE-7M3 ARW | 24 MP | **636** | 1376 | 1131 | 2.2× |
| Sony RX100M2 ARW | 20 MP | **654** | 1249 | 1005 | 1.9× |
| Canon EOS R CR3 | 30 MP | **1112** | 2075 | 1823 | 1.9× |
| Panasonic S5 RW2 | 24 MP | **1074** | 1579 | 1332 | 1.5× |
| Adobe DNG | 4 MP | **340** | 502 | 304 | 1.5× |
| Nikon Z 8 NEF（HE 壓縮） | 45 MP | 失敗（LibRaw 不支援） | 2062（ImageIO 全解析度 8256×5504） | 1686 | — |

**原因已確認**：C# 打包的是 Homebrew libraw，連著 `libomp.dylib`；LibRaw 在 `ahd_demosaic`／`dht_demosaic`／`misc_demosaic`／
`raw2image`／`postprocessing_aux`／`crx`（CR3）／`pana8`／`fuji_compressed` 都有 `#pragma omp`。
`/usr/bin/time` 佐證：C# 解碼 user 10.3 s／real 3.6 s（約 3 核），Swift user 6.3 s／real 5.4 s（約 1.2 核）。
Swift 端是刻意用 `-DLIBRAW_NOTHREADS`（少一個要簽的 dylib）。**這是可以補回來的**，見「後續」。

## 2. 後製管線（2400 px 來源、CPU 單次冷跑、含 8-bit 轉換與浮水印；ms）

以 ILCE-7M3 為代表（其他檔案數字幾乎相同，因為來源都是 2400 px）：

| 調整 | C# | Swift CPU | Swift GPU（warm） | Swift CPU 快 | GPU 快 |
|---|---|---|---|---|---|
| 01 原圖 | 74 | **29** | 14 | 2.6× | 5.3× |
| 02 曝光+色溫+對比+鮮豔 | 79 | **25** | 12 | 3.2× | 6.6× |
| 03 曝光−+色溫+亮暗部+暗角 | 75 | **26** | 13 | 2.9× | 5.8× |
| 04 裁切+旋轉+銳利化 | 134 | **48** | 13 | 2.8× | 10× |
| 05 浮水印（白，PingFang TC 150） | 114 | **30** | 12 | 3.8× | 9.5× |
| 06 浮水印（黑，Helvetica Neue 120） | 64 | **25** | 14 | 2.6× | 4.6× |

全部八個檔案：C# 73–103 ms（01）、134–248 ms（04）；Swift 22–33 ms、47–51 ms。
Swift 的 GPU 第一次 cold 約 40 ms（pipeline state／buffer pool 暖機），之後 12–15 ms。C# 沒有 GPU 路徑（Phase 8 未開始）。
兩邊輸出的 JPEG 大小逐案相差 <1%（同一條管線的佐證；逐像素對照見 `hashtest` 98/98）。

## 3. proxy 快取（2560 長邊，冷啟＝解碼+縮放+寫 PNG+.f16；熱取＝讀 .f16；ms）

| 檔案 | C# 冷啟 | Swift 冷啟 | C# 熱取 | Swift 熱取 |
|---|---|---|---|---|
| 7RM6 | **3922** | 5940 | 42 | **7** |
| 7RM5 | **3580** | 4565 | 38 | **8** |
| 7M3 | **1091** | 1613 | 28 | **13** |
| RX100M2 | **1123** | 1530 | 28 | **9** |
| CR3 | **1600** | 2364 | 28 | **12** |
| RW2 | **1548** | 1841 | 28 | **12** |
| DNG | **684** | 735 | 28 | **7** |
| Z8 NEF | 1136（內嵌預覽當 proxy） | 3616（真的全解析度） | 28 | 11 |

冷啟被解碼主宰（同第 1 節）；熱取 Swift 快 3–5×（開資料夾後點照片的體感就是這個）。

## 4. 記憶體（同樣的 rendertest 工作量，max RSS）

| 檔案 | C# | Swift |
|---|---|---|
| 7RM6 | 2.60 GB | 2.61 GB |
| 7RM5 | 2.99 GB | **2.32 GB** |
| 7M3 | 1.74 GB | **1.11 GB** |
| RX100M2 | 1.61 GB | **0.92 GB** |
| CR3 | 1.96 GB | **1.28 GB** |
| RW2 | 1.74 GB | **1.11 GB** |
| DNG | 0.70 GB | **0.49 GB** |

## 5. 啟動與體積

| | C# | Swift |
|---|---|---|
| headless 啟動（trivial 指令） | 0.06 s／46 MB | 0.01 s／6 MB |
| .app 大小 | 126 MB（self-contained .NET + Avalonia + Skia + exiftool + 5 dylib） | 9.2 MB（1 dylib） |
| 公證要簽的 Mach-O | 數十個（exiftool 的 Perl 檔另計） | 3 個 |

## 6. 相容性差異

- **Nikon Z 8 HE NEF**：兩邊 LibRaw 都解不了；C# 退回內嵌預覽（proxy 是縮圖版），Swift 退回 ImageIO **全解析度**。
- **DNG 的 EXIF 機型**：兩邊都讀不到（C# enginetest 的唯一失敗項；Swift `info` 同樣空白）——這張 DNG 的問題，不是移植差異。
- **遮罩黑邊**：Swift 用 ImageIO 可見尺寸自動裁（7RM6 9984×6656），C# 在 app 內靠 ExifTool `FullImageSize`（headless decodetest 不帶時得到 10017×6673）。

## 結論

| 面向 | 勝出 | 差距 |
|---|---|---|
| 讀 RAW（解碼） | **C#** | 1.4–2.2×，**純粹是 OpenMP**，Swift 可補 |
| 後製（CPU） | **Swift** | ~3× |
| 後製（GPU） | **Swift**（C# 無） | 5–10× |
| 切換照片（proxy 熱取） | **Swift** | 3–5× |
| 記憶體 | **Swift** | 少 30–45%（60 MP 持平） |
| 啟動／體積／簽章複雜度 | **Swift** | 一個數量級 |
| 與 Windows 版共用程式碼 | **C#** | 領域層可 `diff -r` 逐檔同步 |

**整體：Swift 版比較好**——使用者實際感受到的（拉滑桿、切照片、匯出）快 3–10 倍，記憶體與體積都小得多。
唯一輸的是「開資料夾第一次產生快取」那一段，而且原因單一、可補。

## 後續

1. **LibRaw 開 OpenMP**：`Scripts/build_libraw.sh` 拿掉 `-DLIBRAW_NOTHREADS`、加 `-Xclang -fopenmp`，
   連結並打包 `libomp.dylib`（Homebrew 的 `libomp` 是 universal，minos 需重驗）、`build_app.sh` 多簽一個 dylib。
   預期解碼追平 C#（同一版 LibRaw），開資料夾冷啟快 1.5–2×。
2. C# 版若要繼續：Phase 8 Metal 是它最大的缺口，Swift 的 `MetalShaders` 可以逐行搬（kernel 本來就是照 CPU 參考實作寫的）。
