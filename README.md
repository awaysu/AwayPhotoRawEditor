# AwayPhotoRawEditor for macOS

類似 Lightroom 的 RAW 相片編輯器，macOS 原生版（Swift / AppKit）。
由 [Windows 版](https://github.com/awaysu/AwayPhotoRawEditor)（C# / WinForms）移植。

非破壞式編輯：所有調整以 XML 存於各資料夾的 `RAW_TEMP` 快取，**原始檔案永不變動**。

## 功能

- RAW 解碼（LibRaw 0.22.2，與 Windows 版同版本 → 同像素）＋一般格式（ImageIO，含 HEIC）
- **GPU 加速（Metal）**：預覽算圖快 1.5–4.8×，偵測不到或超過尺寸上限時自動改用 CPU，結果相同
- 線性光色彩管線：白平衡與曝光在線性域運算，有相機色彩資料時以相機矩陣做白平衡
- 處理版本（Lightroom 式）：舊照片維持舊算式，可主動升級
- 基本調整／色彩（Kelvin 白平衡、滴管）／細節（銳利度、暗角、降噪）
- 裁切、旋轉、廣角變形、多重線性漸層、局部修護
- 風格檔（內建＋自訂，可編輯覆寫、備份／還原）
- 縮圖多選批次編輯、批次復原／重做（⌘Z／⌘⇧Z）、虛擬副本、隱藏且不輸出
- 拖曳資料夾到視窗即開啟、最近開啟紀錄
- 匯出：重新命名規則、尺寸上限、DPI、浮水印、EXIF 保留
- 八語介面：繁體中文、English、日本語、한국어、简体中文、Deutsch、Français、Español
- 「關於」視窗可檢查更新

## 系統需求

macOS 14 以上，Apple Silicon 或 Intel（universal）。

## 與 Windows 版互通

`RAW_TEMP` 內的調整檔、`presets.xml`、`export.xml`、`settings.xml` **兩邊格式完全相同**，
同一張照片可以在 Windows 調到一半、複製到 Mac 接著調。

## 從原始碼建置

```bash
# 1. 建置 LibRaw（只需一次；universal、對齊最低系統版本、無外部相依）
Scripts/build_libraw.sh

# 2. 建置 .app
Scripts/build_app.sh
open build/AwayPhotoRawEditor.app
```

開發時 `swift build` / `swift run awpr-cli selftest <圖片>` 即可。

### 診斷指令

```bash
awpr-cli info <圖片>              # LibRaw 可用性、裁切表、EXIF、相機色彩資料
awpr-cli selftest <圖片>          # 引擎端到端
awpr-cli hashtest <圖片> out.txt  # 色彩管線指紋（與 C# 版 --hashtest 逐字元對照用）
awpr-cli gputest <圖片>           # CPU / GPU 對照
awpr-cli bench <圖片>             # 各階段 CPU / GPU 耗時
awpr-cli updatecheck              # 檢查更新 API
```

### 簽章與公證

```bash
xcrun notarytool store-credentials awpr-notary \
    --apple-id <你的 Apple ID> --team-id <TEAMID> --password <app-specific-password>

Scripts/sign_and_notarize.sh "Developer ID Application: Your Name (TEAMID)" awpr-notary
```

產出 `build/AwayPhotoRawEditor-<版本>.dmg`（已簽章、已公證、已 staple）。

## 第三方元件

- [LibRaw](https://www.libraw.org/) 0.22.2 — LGPL 2.1（動態連結，`Scripts/build_libraw.sh` 會下載原始碼）
- Apple ImageIO / Core Graphics / Core Text

## 授權

[BSD 3-Clause](LICENSE) — Copyright (c) 2026, Chih-Wei Su (Awaysu)

## 作者

Chih-Wei Su (Awaysu) — awaysu@gmail.com

歡迎自由修改成你自己的版本，只希望你能在「關於」視窗中提及來源是這裡
（AwayPhotoRawEditor / Awaysu）。
