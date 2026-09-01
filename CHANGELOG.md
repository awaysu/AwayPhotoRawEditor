# 版本紀錄 / Changelog

AwayPhotoRawEditor macOS 版各版修改大要（日期為發佈日期）。網站上的版本歷史與本檔同步（`api.php?action=changelog&app=awayphotoraweditor_mac`）。

## v1.0.19（2026-09-02）
- 預覽列縮圖改為與編輯區同一份像素：RAW 之前用相機內嵌預覽當底圖，套上調整後和實際編輯結果對不起來；舊資料夾開啟時會自動補齊，不必刪快取
- 「恢復上一步」（⌘Z）可以還原「隱藏且不輸出」與「刪除檔案」：刪掉的檔案會從垃圾桶搬回原處，調整設定與虛擬副本一併還原
- Delete 鍵不再隱藏／刪除照片（避免誤按一下照片就不見），改由縮圖右鍵選單操作；漸層工具下按 ⌫ 刪除選取的漸層
- 診斷指令 awpr-cli render 新增 --grad-exposure 與亮部裁切統計

## v1.0.18（2026-09-01）
- macOS 版首次發佈：以 Swift／AppKit 重寫，Apple Silicon 與 Intel 通用，需 macOS 14 以上；Developer ID 簽章並經 Apple 公證
- 與 Windows 版共用 RAW_TEMP 調整檔格式，同一個資料夾可以在兩邊接著編輯
- LibRaw 0.22.2（含 OpenMP 多執行緒）解碼 RAW、Metal GPU 加速預覽、ImageIO 讀取 EXIF 與一般格式（含 HEIC）
- 「關於」視窗的檢查更新指向 macOS 專屬下載頁
