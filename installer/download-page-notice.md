# 下載頁說明文字（安全性警告）

貼到 https://www.awaysu.cc/software/awayphotoraweditor 的下載按鈕**下方**。

目的：SmartScreen 的「Windows 已保護您的電腦」不是病毒偵測，是「這個發行者還沒有信譽」；
而 zip 解壓縮後才跳警告是 MOTW（下載標記會從 zip 傳染給解出來的每個檔案），
下載後先「解除封鎖」就能省掉一輪。兩件事先講清楚，可以少掉大部分的回報。

⚠️ SHA256 要在**簽章之後**才計算（簽章會改變檔案內容）：

```powershell
Get-FileHash installer\Output\AwayPhotoRawEditor-Setup-v1.0.12.exe -Algorithm SHA256
Get-FileHash installer\Output\AwayPhotoRawEditor-v1.0.12.zip -Algorithm SHA256
```

---

## 中文版

### 關於安裝時的安全性警告

本程式由個人開發並以自簽憑證簽章，尚未累積 Microsoft SmartScreen 的發行者信譽，
因此首次執行時 Windows 可能會顯示警告。這**不是**偵測到病毒，而是「這個發行者不常見」的提醒。

**安裝檔**
出現「Windows 已保護您的電腦」時，點選左側的「**其他資訊**」→「**仍要執行**」即可繼續安裝。

**攜帶版 (zip)**
從網路下載的壓縮檔會被 Windows 標記，解壓縮後每個檔案都會沿用這個標記。
請在解壓縮**之前**先解除封鎖：

1. 在下載到的 zip 上按**右鍵** →「**內容**」
2. 勾選視窗最下方的「**解除封鎖**」→「**確定**」
3. 再解壓縮

**如果防毒軟體把檔案刪除或隔離**
這是誤判，請將檔案還原並加入白名單；也歡迎回報給我，我會向防毒廠商提交誤判申訴。
您也可以先用 [VirusTotal](https://www.virustotal.com/) 檢查，或核對下方的 SHA256 確認檔案完整。

安裝檔 SHA256：`7D22DFD54B135D315F5BD80335C894582D4833FF3C3250E1CC8DEBECB71D4053`
攜帶版 SHA256：`2FBAEA7A2302E094D1C191342CA96408C6808B5B15501EB3F7B94C48D463E981`

---

## English version

### About the security warning

This application is developed by an individual and signed with a self-issued certificate, so it has
not yet built up publisher reputation with Microsoft SmartScreen. Windows may therefore show a
warning the first time you run it. This is **not** a virus detection — it only means the publisher
is not yet widely recognised.

**Installer**
If you see "Windows protected your PC", click "**More info**" → "**Run anyway**" to continue.

**Portable version (zip)**
Windows marks archives downloaded from the internet, and every file extracted from them inherits
that mark. Unblock the archive **before** extracting:

1. **Right-click** the downloaded .zip → "**Properties**"
2. Tick "**Unblock**" at the bottom of the dialog → "**OK**"
3. Then extract

**If your antivirus removes or quarantines the file**
This is a false positive. Please restore the file and add it to your exclusions — and do let me
know, so I can submit a false-positive report to the vendor. You can also scan it on
[VirusTotal](https://www.virustotal.com/) or verify the SHA256 checksums below.

Installer SHA256: `7D22DFD54B135D315F5BD80335C894582D4833FF3C3250E1CC8DEBECB71D4053`
Portable SHA256: `2FBAEA7A2302E094D1C191342CA96408C6808B5B15501EB3F7B94C48D463E981`

---

## HTML（可直接貼進網頁）

```html
<section class="security-notice">
  <h3>關於安裝時的安全性警告 / About the security warning</h3>

  <p>本程式由個人開發並以自簽憑證簽章，尚未累積 Microsoft SmartScreen 的發行者信譽，因此首次執行時
     Windows 可能會顯示警告。這<strong>不是</strong>偵測到病毒，而是「這個發行者不常見」的提醒。</p>

  <p><strong>安裝檔</strong>：出現「Windows 已保護您的電腦」時，點選左側的
     「<strong>其他資訊</strong>」→「<strong>仍要執行</strong>」即可繼續安裝。</p>

  <p><strong>攜帶版 (zip)</strong>：從網路下載的壓縮檔會被 Windows 標記，解壓縮後每個檔案都會沿用這個標記。
     請在解壓縮<strong>之前</strong>先解除封鎖：</p>
  <ol>
    <li>在下載到的 zip 上按<strong>右鍵</strong> →「<strong>內容</strong>」</li>
    <li>勾選視窗最下方的「<strong>解除封鎖</strong>」→「<strong>確定</strong>」</li>
    <li>再解壓縮</li>
  </ol>

  <p><strong>如果防毒軟體把檔案刪除或隔離</strong>：這是誤判，請將檔案還原並加入白名單，
     也歡迎回報給我，我會向防毒廠商提交誤判申訴。您也可以用
     <a href="https://www.virustotal.com/" target="_blank" rel="noopener">VirusTotal</a>
     檢查，或核對下方的 SHA256 確認檔案完整。</p>

  <hr>

  <p>This application is developed by an individual and signed with a self-issued certificate, so it
     has not yet built up publisher reputation with Microsoft SmartScreen. Windows may show a warning
     the first time you run it. This is <strong>not</strong> a virus detection.</p>

  <p><strong>Installer</strong>: if you see “Windows protected your PC”, click
     “<strong>More info</strong>” → “<strong>Run anyway</strong>”.</p>

  <p><strong>Portable (zip)</strong>: right-click the downloaded .zip →
     “<strong>Properties</strong>” → tick “<strong>Unblock</strong>” → “<strong>OK</strong>”,
     then extract. Windows marks downloaded archives and every extracted file inherits that mark.</p>

  <p><strong>If your antivirus removes the file</strong>: it is a false positive. Please restore it,
     add an exclusion, and let me know so I can file a false-positive report with the vendor.</p>

  <hr>

  <p class="checksums">
    <code>AwayPhotoRawEditor-Setup-v1.0.12.exe</code> SHA256:
    <code>7D22DFD54B135D315F5BD80335C894582D4833FF3C3250E1CC8DEBECB71D4053</code><br>
    <code>AwayPhotoRawEditor-v1.0.12.zip</code> SHA256:
    <code>2FBAEA7A2302E094D1C191342CA96408C6808B5B15501EB3F7B94C48D463E981</code>
  </p>
</section>
```
