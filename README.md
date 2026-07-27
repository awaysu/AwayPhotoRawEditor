# Away PhotoRaw Editor

深色主題的 Windows 桌面 RAW 相片編輯器（Lightroom / Capture One 類）。
非破壞式編輯，所有調整以 XML 存於各資料夾的 `RAW_TEMP` 快取，原始檔案永不變動。

![icon](src/icon/icon.png)

## 下載

安裝檔請到 [awaysu/Download](https://github.com/awaysu/Download) 下載
`AwayPhotoRawEditor-Setup-vX.Y.Z.exe`（自含式，無需另裝 .NET）。

> 安裝檔以自簽憑證簽署，Windows SmartScreen 可能顯示警告，
> 點「其他資訊 → 仍要執行」即可安裝。安裝為每使用者、不需系統管理員權限。

## 功能

- RAW 解碼（LibRaw）＋一般影像格式（WIC），EXIF 讀取（ExifTool）
- 基本調整／色彩（Kelvin 白平衡、滴管）／細節（銳利度、暗角、降噪）
- 裁切、旋轉、廣角變形、多重線性漸層、局部修護
- 風格檔（內建＋自訂，可編輯覆寫）
- 縮圖多選批次編輯、批次復原、虛擬副本
- 匯出：重新命名規則、尺寸上限、DPI、浮水印、EXIF 保留
- 全自繪深色 UI

## 系統需求

- Windows 10 / 11 x64

## 從原始碼建置

```powershell
# .NET 8 SDK
dotnet build src\AwayPhotoRawEditor.csproj -c Debug

# 發佈（自含式）
dotnet publish src\AwayPhotoRawEditor.csproj -c Release -r win-x64 --self-contained true

# 安裝檔（需 Inno Setup 6）
iscc installer\AwayPhotoRawEditor.iss
```

外部工具已內建於 `tools/`（LibRaw 0.22.1、ExifTool 13.59），執行期自動偵測。

## 作者

Awaysu (awaysu@gmail.com)
