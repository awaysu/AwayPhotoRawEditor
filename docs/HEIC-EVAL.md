# HEIC 支援評估（TASK-008，只評估、未實作）

2026-10-09，Agent-12。目前狀態：`paths::REGULAR_EXTENSIONS` 已列 `heic`／`heif`（照 Swift 版），但 `codec::heic_supported()` 回 false，HEIC 檔會出現在縮圖列卻無法解碼。C# 1.x 不支援 HEIC；Swift 版靠 macOS ImageIO 解碼。

## 結論（建議）

**做，但分平台：macOS 與 Windows 用系統解碼器，Linux 用系統的 libheif（動態連結）。不把 libheif／libde265 編進我們的執行檔。**

| 平台 | 建議做法 | 使用者端需求 | 授權影響 | 估工時 |
|---|---|---|---|---|
| macOS | ImageIO（`CGImageSource`），和 Swift 版一樣 | 無（系統內建） | 無 | 1 天 |
| Windows | WIC（`IWICBitmapDecoder`，`windows` crate） | 需要「HEIF 影像延伸模組」＋「HEVC 視訊延伸模組」；後者要付費（歷來 US$0.99）或由 OEM 預裝；沒有時顯示提示 | 無 | 2 天 |
| Linux | 執行時 `dlopen` 系統的 `libheif.so.1`（Ubuntu 24.04：1.17.6，HEVC 解碼由 `libheif-plugin-libde265` 提供）；deb／rpm 只寫 `Recommends`／`Suggests` | 裝 `libheif1` + `libheif-plugin-libde265`（Ubuntu 預設裝 GNOME 時通常已有，awpr-linux 上已有） | LGPL 函式庫以動態方式使用，我們維持 BSD-3 | 2 天 |
| 共通 | HEIC 走「一般圖檔」路徑（`DecodeSource::Rendered`），版本 3 時用 gamma 編碼來源；EXIF／方向見下 | — | — | 1 天（含測試、i18n、文件） |

合計約 **6 個工作天**，風險中等，主要在 Windows（使用者機器缺延伸模組的比例，以及 WIC 在 Store 套件下的行為差異），見「風險」。

不建議「三平台都內嵌 libheif＋libde265」：授權與建置成本最高（下節），換來的只是 Windows 不必裝延伸模組。若之後確定 Windows 使用者多半沒有 HEVC 延伸模組，再考慮只在 Windows 隨附 **動態** 的 libheif／libde265 DLL（方案 B）。

## 授權

- **libheif**：LGPL-3.0（其範例程式為 MIT）。**libde265**：LGPL-3.0（範例程式 MIT）。兩者都是 strukturag 的專案。
- 本專案是 BSD-3-Clause，LibRaw 走 CDDL-1.0 靜態連結。LGPL 函式庫：
  - **動態連結**：可以，條件是隨附授權文字、告知使用者可替換該函式庫（DLL／.so 本來就可替換）、提供（或指向）該版本原始碼。對 BSD 專案本身沒有傳染。
  - **靜態連結**：LGPL-3.0 §4(d) 要求讓使用者能「重新連結」修改過的函式庫 → 必須另外提供我們的目的檔（.o／.lib）或整套可重建的原始碼。我們原始碼本來就公開，所以理論上做得到，但每個版本都要附上對應的建置方式並維持可重建，對 macOS 簽章／公證後的 .app 尤其麻煩（重新連結後簽章失效）。**不建議靜態。**
- **專利**：HEVC（H.265）有專利池（MPEG LA／Access Advance／Velos）。軟體解碼器的專利授權責任在散布者身上，這是自己隨附 libde265 的最大灰色地帶；用作業系統的解碼器（ImageIO、WIC＋Microsoft 的 HEVC 延伸模組）或使用者自己裝的系統套件（Linux 發行版的 libde265）時，專利問題不在我們身上。這是建議「用系統解碼器」的主要理由。
- **x265（編碼器）是 GPL-2.0**：vcpkg 的 `libheif` port 預設 feature `hevc` 會拉進 x265。若走方案 B，必須 `libheif[core]` 關掉預設 feature，只留 libde265 解碼。

## 建置可行性（若要自己帶 libheif）

- **crate**：`libheif-rs`（MIT）→ `libheif-sys`（MIT）。支援 libheif 1.17～1.23（`v1_17`…`v1_23`／`latest` feature）。
  - Linux：`system-deps`／pkg-config 找系統的 libheif（需 `libheif-dev` ≥ 1.17）。這會在**建置時**連結 `libheif.so`，執行時缺函式庫就整個程式起不來 → 我們應該改用 `libloading` 執行時載入（或只在 HEIC 解碼時 dlopen），讓沒裝 libheif 的機器照常執行。
  - Windows：用 `vcpkg` crate 找 vcpkg 裝的 libheif（`cargo vcpkg build`）。
  - `embedded-libheif` feature：從原始碼編 libheif 並**靜態**連結，但 libde265 等編解碼器不含在內，要另外提供 → 授權上就是上面的靜態問題，不採用。
- **libheif 建置**：只剩 CMake（1.16 起移除 autotools），編解碼器可內建或做成外掛（`WITH_LIBDE265_PLUGIN`，`LIBHEIF_PLUGIN_PATH` 載入）。Windows 用 vcpkg：`vcpkg install libheif[core]:x64-windows`（動態、不含 x265）＋ `libde265`，產出 `heif.dll`、`libde265.dll`，隨安裝檔放在 exe 旁。估 2～3 天（含 CI、Inno Setup、THIRD-PARTY-NOTICES、LGPL 原始碼提供方式）。
- macOS 若也自帶：要做 universal（arm64＋x86_64）兩個 dylib，放 `Contents/Frameworks`、簽章與公證；ImageIO 已經能讀，沒有理由這樣做。

## iPhone HEIC 的 EXIF 與方向

- iPhone 的 HEIC 用 `irot`／`imir` 屬性表示旋轉，並另附 EXIF Orientation。libheif 解碼時**預設套用** `irot`／`imir`（`heif_decoding_options.ignore_transformations = false`），所以解出的影像已經是正的；此時**不可再套 EXIF Orientation**，否則會轉兩次。ImageIO（`kCGImageSourceCreateThumbnailWithTransform`／`CGImageSourceCreateImageAtIndex` 再依 `kCGImagePropertyOrientation` 轉）與 WIC（`IWICBitmapFrameDecode` 給未轉的影像，方向在 metadata `/app1/ifd/{ushort=274}` 或 HEIF 的 `System.Photo.Orientation`）的規則不同，各平台都要寫測試。
- EXIF 讀取：libheif 用 `heif_image_handle_get_list_of_metadata_block_IDs(handle, "Exif", …)` 取出 Exif 區塊（前 4 位元組是 TIFF 標頭偏移），餵給我們現有的 `photo::tiff` 讀取器即可得到相機、鏡頭、ISO、快門、光圈、日期。ImageIO 用 `CGImageSourceCopyPropertiesAtIndex`；WIC 用 metadata query reader。
- 色彩：iPhone HEIC 是 Display P3（附 nclx 或 ICC）。我們的一般圖檔路徑假設 sRGB；直接當 sRGB 會讓飽和色偏淡。版本 3 的管線已是寬色域（Rec.2020 工作空間），可以在解碼時把 P3 → 線性 → Rec.2020 正確帶入，而不是先壓到 sRGB——這是 HEIC 進版本 3 時應該順便做的事（＋1 天）。
- 深度圖、HDR 增益圖（iOS 17+ 的 gain map）、Live Photo：先不支援，只讀主影像。

## 風險

1. **Windows 使用者缺 HEVC 延伸模組**：WIC 會回「找不到元件」。需要明確的錯誤提示（含 Microsoft Store 連結），否則看起來像程式壞了。Microsoft 已把免費版 HEVC 延伸模組從 Store 下架，付費版每台機器要使用者自己買。若使用者回報這是主要痛點，再評估方案 B（隨附動態 DLL，並承擔 HEVC 專利的不確定性）。
2. **Linux 發行版差異**：Ubuntu 24.04 是 libheif 1.17.6；較舊的發行版（22.04 是 1.12）API 不同。用 `dlopen` 只呼叫 1.12 以後都有的少數 C API（`heif_context_read_from_file`、`heif_decode_image`、metadata 取得）即可涵蓋。
3. **安全性**：libheif 這兩年 CVE 不少（Ubuntu 2026 年的 1.17.6-1ubuntu4.4／4.6 安全更新修了多個溢位）。用系統函式庫的好處是跟著系統更新；自帶就要自己追版本。
4. **10-bit HEIC**：新款 iPhone 可能存 10-bit。libheif 要用 `heif_chroma_interleaved_RRGGBB_LE` 取 16-bit 輸出，否則會截成 8-bit；WIC／ImageIO 要指定 64bpp 格式。我們的一般圖檔路徑已支援 16-bit（`to_float` 的非 8-bit 分支），只要解碼端給得出來。

## 建議的實作順序（若決定做）

1. 共通：`codec::load_heic(path) -> Option<FloatImage>` 介面＋EXIF 走 `tiff`；`heic_supported()` 依平台回報；不支援時的提示字串（八語）。
2. macOS ImageIO（最快、最穩，先上）。
3. Linux `dlopen` libheif（awpr-linux 已裝 1.17.6 與 libde265 外掛，可直接測）。
4. Windows WIC，含「缺延伸模組」偵測與提示。
5. Display P3 → 版本 3 寬色域的正確帶入。
6. 測試樣本：iPhone 直式／橫式（irot）、10-bit、附 P3 ICC 各一張；三平台 `--shot` 與 EXIF 欄位比對。

## 參考
- libheif（授權 LGPL、外掛機制、CMake）：https://github.com/strukturag/libheif
- libde265（授權 LGPL）：https://github.com/strukturag/libde265
- libheif-rs／libheif-sys（MIT；pkg-config、vcpkg、`embedded-libheif`、支援 1.17～1.23）：https://docs.rs/crate/libheif-rs/latest 、https://github.com/Cykooz/libheif-sys
- vcpkg libheif port（1.23.5；預設 feature `hevc` = x265，GPL-2.0）：https://github.com/microsoft/vcpkg/blob/master/ports/libheif/vcpkg.json
- Ubuntu 24.04 libheif 1.17.6 與 libde265 外掛、近期安全更新：https://ubuntuupdates.org/package/core/noble/main/updates/libheif-plugin-libde265
- Windows 讀 HEIC 需要 HEIF＋HEVC 延伸模組：https://apps.microsoft.com/detail/9ntld6msd8bm 、https://winhelponline.com/blog/how-to-open-heic-files 、https://www.windowscentral.com/how-open-heic-and-hevc-files-windows-10s-photos-app
