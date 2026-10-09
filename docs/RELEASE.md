# 發佈流程（Windows／macOS／Linux）

一個平台一支腳本，產出放在 `dist/`（git 不追蹤）。**這份文件與 repo 裡沒有任何密碼或私鑰**：
Windows 簽章憑證在 `CurrentUser\My`，Mac 的 Developer ID 在登入鑰匙圈、公證帳密在鑰匙圈 profile，
awaysu.cc 的上傳密碼只從環境變數 `AWAYSU_API_PASSWORD` 讀。

CI（`.github/workflows/ci.yml`）在三平台跑 build＋test；打 `v*` tag 時 `release.yml` 會做出
**未簽章**的三平台安裝檔當 workflow artifacts（方便檢查），正式檔一律在本機簽章後再上傳。

---

## 0. 發佈前

```
[ ] Cargo.toml 的 [workspace.package] version 改好（唯一的版本來源；.iss、Info.plist、deb/rpm 都由腳本帶入）
[ ] CHANGELOG.md 最上面有「## <版本>」這一段（cargo test 的 version_has_one_source 會檢查）
[ ] cargo test --workspace 三平台全過（Windows 本機、ssh awpr-linux、ssh awpr-mac）
[ ] hashtest 4 檔和 tests/results/<平台>/ 逐位元組相同（樣本在 D:\Awaysu\raw_samples）
[ ] THIRD-PARTY-NOTICES.md 與 Cargo.lock 的直接依賴一致
```

版本號含預覽字尾時（例如 `1.2.0-dev`）：Windows 檔案版本寫 `1.2.0.0`、Mac 的
`CFBundleShortVersionString` 寫 `1.2.0`，deb／rpm 寫 `1.2.0~dev`（`~` 讓它排在正式版 1.2.0 之前；
rpm 不接受 `-`）。

---

## 1. Windows

### 前置

| 要什麼 | 確認 |
|---|---|
| 簽章憑證 `CN=Awaysu, O=Awaysu, C=TW` | `Get-Item Cert:\CurrentUser\My\997D278FE3FD6FFA1F8E43683047530DE7210C66`（2036-08-13 到期） |
| Inno Setup 6 | `%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe` |
| Windows SDK 的 `rc.exe` | 嵌入 exe 的圖示與版本資源（`crates/app/build.rs`，用 winresource）；沒有它 build 會過，但會出現 `Windows resources not embedded` 的警告 |

⚠️ **一律用指紋指定憑證**，不要用名稱比對（曾經挑到作廢的 `CN=AwayTerminal (awaysu)`）。

### 打包

```powershell
powershell -ExecutionPolicy Bypass -File scripts\package-windows.ps1
```

順序（腳本照這個做，**不能顛倒**）：`cargo build --release` → **簽 exe** → 放進 `dist\AwayPhotoRawEditor-v<版本>\`
（exe、LICENSE.txt、THIRD-PARTY-NOTICES.md、LibRaw-LICENSE.CDDL.txt、CHANGELOG.md）→ ISCC 做安裝檔 →
**簽安裝檔** → 攜帶版 zip（同一個資料夾，含最上層目錄，結構和安裝後相同）→ **最後**才算 SHA256
寫到 `dist\SHA256SUMS.txt`（簽章會改檔案內容）。`-NoSign` 做未簽版。

產出：

```
dist\AwayPhotoRawEditor-Setup-v<版本>.exe
dist\AwayPhotoRawEditor-v<版本>.zip
dist\SHA256SUMS.txt
```

### 安裝檔的行為

| 項目 | 1.1.0 起 |
|---|---|
| 安裝範圍 | 每使用者（`PrivilegesRequired=lowest`），`%LOCALAPPDATA%\Programs\AwayPhotoRawEditor`（與 C# 1.0.x 同一個資料夾），免 UAC |
| AppId | 沿用 C# 1.0.x 的 `{8E1A2C64-5A17-4D0B-9C67-AWPRE0100001}`：Windows 視為同一套軟體升級，「設定 > 應用程式」只有一個 AwayPhotoRawEditor |
| 舊版 | **就地升級、安裝前自動移除舊版**：`PrepareToInstall` 依 Uninstall 登錄找 C# 1.0.x（HKCU 與 HKLM 的 `{8E1A…}_is1`）與已撤回的 2.0.x（HKCU 的 `{9063DED4-6DA5-4A20-933D-AC78F788359C}_is1`，裝在 `AwayPhotoRawEditor 2`），以 `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART` 執行它們的解除安裝程式並等它結束；失敗只寫進安裝紀錄，不中止安裝。兩個舊版的 .iss 都沒有 `[UninstallDelete]`，不會刪使用者資料 |
| 開始功能表 | `AwayPhotoRawEditor`（顯示名稱一律 AwayPhotoRawEditor） |
| 桌面捷徑 | 提供，**預設不勾** |
| 授權頁 | 顯示 LICENSE（BSD-3） |
| 安裝檔語言 | 繁中（排第一）、簡中、英、日、韓、德、法、西。中文語言檔取自 Inno Setup 官方 issrc `Files/Languages/`，存成 UTF-8 含 BOM 放在 `installer/`（沒 BOM 時 ISCC 會用系統碼頁讀成亂碼） |
| 解除安裝 | 只刪安裝的檔案；`%APPDATA%\AwayPhotoRawEditor` 的設定與照片旁的 `RAW_TEMP` 都保留 |

### 驗收（不必安裝）

```powershell
Get-AuthenticodeSignature dist\AwayPhotoRawEditor-Setup-v<版本>.exe | Format-List SignatureType, Status, SignerCertificate, TimeStamperCertificate
```

`SignatureType = Authenticode`、`SignerCertificate` 指紋 `997D…0C66`、`TimeStamperCertificate` 有值即可。
`Status = UnknownError`（根憑證不受信任）是**自簽憑證的正常結果**，不是簽章失敗；真正的問題是
`HashMismatch` 或 `NotSigned`。

安裝檔內容：7-Zip 新版打不開 Inno 的壓縮內容，改看 ISCC 的編譯紀錄（不加 `/Q` 會列出每個檔案），
或直接看 `dist\AwayPhotoRawEditor-v<版本>\`（安裝檔與 zip 都是從這個資料夾做的）。

⚠️ Git Bash 裡直接呼叫 ISCC 時要加 `MSYS_NO_PATHCONV=1`，否則 `/DMyAppVersion=…` 會被當成路徑轉換。

### 給使用者的說明：Windows 讀 HEIC

Windows 版用系統的 WIC 讀 HEIC（iPhone 照片），程式本身不帶 HEVC 解碼器。使用者電腦要有兩個
Microsoft Store 元件：

1. **HEIF 影像延伸模組**（HEIF Image Extensions，免費，Windows 10 1809 以後多半已預裝）
2. **HEVC 視訊延伸模組**（HEVC Video Extensions，付費，約 US$0.99；部分品牌電腦出廠已附）

缺任何一個時，縮圖顯示「無法解碼」，照片資訊寫出上面這句提示；裝好後重新整理資料夾（F5）即可。
下載頁與 FAQ 請放這段說明。macOS 不需要任何安裝；Linux 見下方的 `Recommends`。

---

## 2. macOS（在 awpr-mac）

### 前置

| 要什麼 | 確認 |
|---|---|
| Developer ID 憑證在**登入**鑰匙圈 | `security find-identity -v -p codesigning` 看到 `Developer ID Application: Chih-Wei Su (BNH8YS88T9)` |
| 公證 profile | `xcrun notarytool history --keychain-profile AwayTerminalNotary`（公證帳密綁 Apple ID，不綁 app，可共用 AwayTerminal 那個 profile） |
| 兩個編譯目標 | `rustup target add aarch64-apple-darwin x86_64-apple-darwin` |
| universal libomp | `../AwayPhotoRawEditor_Swift/ThirdParty/libomp`（`lipo -archs` 要有兩個架構）；或 `AWPR_LIBOMP_DIR` 指過去 |

⚠️ 那台 Mac 的 `awpr-signing`、`ios-signing` 鑰匙圈是鎖著的，碰到就停在解鎖對話框。腳本的 codesign
一律加 `--keychain ~/Library/Keychains/login.keychain-db`，每個簽章／公證步驟超過 180 秒就砍掉
（`AWPR_STEP_TIMEOUT`）。

⚠️ **登入鑰匙圈在 SSH 工作階段是鎖著的**：`security show-keychain-info ~/Library/Keychains/login.keychain-db`
回 `User interaction is not allowed`，codesign 會報 `errSecInternalComponent`（2026-10-09 實測）。
從別的 SSH 工作階段 `security unlock-keychain` 也不會帶過來。所以從 SSH 打包時設
`AWPR_KEYCHAIN_PASSWORD`，腳本會在同一個工作階段先 `security unlock-keychain -p` 解鎖登入鑰匙圈；
或在 Mac 本機的終端機（已登入的 GUI 工作階段）跑。**密碼只能從環境變數帶入，絕不可寫進 repo、腳本或任何檔案**
（例如 `read -s AWPR_KEYCHAIN_PASSWORD; export AWPR_KEYCHAIN_PASSWORD`）。

### 打包

```sh
AWPR_SIGN_IDENTITY="Developer ID Application: Chih-Wei Su (BNH8YS88T9)" \
AWPR_NOTARY_PROFILE=AwayTerminalNotary \
scripts/package-macos.sh
```

步驟：兩個架構各 build 一次（rpath `@executable_path/../Frameworks`、`MACOSX_DEPLOYMENT_TARGET=14.0`）→
`lipo` 成 universal → `.app`（Info.plist：`com.awaysu.awayphotoraweditor`、AppIcon.icns、
`Frameworks/libomp.dylib`、`Resources/` 裡的 LICENSE.txt／THIRD-PARTY-NOTICES.md／LibRaw-LICENSE.CDDL.txt／CHANGELOG.md）→
簽 libomp → 簽主程式與 bundle（hardened runtime、entitlements、時間戳）→ `ditto` 壓縮送公證 → staple →
DMG（app＋Applications 捷徑）→ 簽 DMG → 公證 → staple → 加 quarantine 模擬下載做 `spctl` 驗收 →
SHA256 寫到 `dist/SHA256SUMS-macos.txt`。

- 沒給 `AWPR_SIGN_IDENTITY`：`.app` 只做 ad-hoc 簽章（Apple silicon 上要有簽章才能執行），DMG 不簽，
  別台電腦會被 Gatekeeper 擋。沒給 `AWPR_NOTARY_PROFILE`：有簽章、不公證。
- `AWPR_LIBRAW_OPENMP=0`：LibRaw 不用 OpenMP（CI 用；像素相同，解碼約慢 2.5 倍）。

### 驗收

```sh
APP=dist/AwayPhotoRawEditor.app
lipo -archs $APP/Contents/MacOS/AwayPhotoRawEditor        # x86_64 arm64
codesign --verify --deep --strict --verbose=2 $APP         # valid on disk
spctl --assess --type execute --verbose=4 $APP             # 公證後：accepted / Notarized Developer ID
xcrun stapler validate dist/AwayPhotoRawEditor-<版本>-macOS.dmg
```

被退件時：`xcrun notarytool log <submission-id> --keychain-profile AwayTerminalNotary`。

---

## 3. Linux（在 awpr-linux）

只發 `.deb` 與 `.rpm`（2026-10-05 決定，不發 AppImage）。

```sh
scripts/package-linux.sh
```

第一次會 `cargo install cargo-deb cargo-generate-rpm`。內容（`crates/app/Cargo.toml` 的
`[package.metadata.deb]`／`[package.metadata.generate-rpm]`）：

```
/usr/bin/AwayPhotoRawEditor
/usr/share/applications/awayphotoraweditor.desktop
/usr/share/icons/hicolor/256x256/apps/awayphotoraweditor.png
/usr/share/doc/awayphotoraweditor/{LICENSE, LibRaw-LICENSE.CDDL, THIRD-PARTY-NOTICES.md, CHANGELOG.md}
```

相依：deb 由 `dpkg-shlibdeps` 自動算（libc6、libgomp1、libstdc++6），另外加 winit／wgpu 執行時才載入的
`libxkbcommon0`、`libvulkan1`；rpm 寫 `libxkbcommon`、`vulkan-loader`。
HEIC：deb `Recommends: libheif1, libheif-plugin-libde265`，rpm `Recommends: libheif, libheif-freeworld`
（Fedora 的 libheif 不含 HEVC 解碼，RPM Fusion 的 libheif-freeworld 才有）。程式在打開 HEIC 時才
`dlopen("libheif.so.1")`，沒裝也能正常啟動，只是 HEIC 顯示「無法解碼」。驗收：`dpkg -c`、`dpkg-deb --info`；
rpm 用 `rpm -qlp`（awpr-linux 沒有 rpm 指令時腳本改用 7z＋zstd＋cpio 列內容）。

---

## 4. 上傳到 awaysu.cc

**全部簽完、公證完才算 SHA256**。密碼只從環境變數讀（從使用者自己保管的檔案載入，不要寫進 repo 或指令歷史）：

```sh
V=<版本>
f=dist/AwayPhotoRawEditor-Setup-v$V.exe        # 或 .zip / -macOS.dmg / .deb / .rpm
SHA=$(sha256sum "$f" | cut -d' ' -f1)            # mac：shasum -a 256
curl -H "X-Api-Password: $AWAYSU_API_PASSWORD" \
  -F app=awayphotoraweditor -F version=$V -F platform=windows -F sha256=$SHA \
  -F "changelog=<CHANGELOG.md 這一版的內容>" \
  -F "file=@$f" \
  "https://www.awaysu.cc/software/api.php?action=upload"
```

- `platform`：`windows`（安裝檔與 zip 各傳一次）、`macos`（dmg）、`linux`（deb、rpm 各一次）。
- 同平台同副檔名的既有項目會被取代（舊檔自動保留為「舊版本」）。
- `changelog` 在同一版第一次上傳時帶就好；中文內容用檔案帶入（`-F "changelog=<changelog.txt"`），避免 shell 編碼問題。
- 上傳後確認 `api.php?action=check_update&app=awayphotoraweditor&platform=<平台>&version=<舊版>` 回 `update_available: true`，
  再從 `download.url` 抓回來比對 SHA256。程式內「關於 → 檢查更新」問的就是這一條（`crates/app/src/update.rs`）。

---

## 5. 還沒做的

| 項目 | 現況 |
|---|---|
| Mac 的 Developer ID 簽章與公證 | 腳本已寫好；2026-10-09 在 SSH 下因登入鑰匙圈鎖著停在 codesign，要在 Mac 本機跑一次確認 |
| CI 實際執行 | 只做了 actionlint 靜態檢查（0 問題），要 push 後才會真的跑 |
| hashtest 樣本放哪 | CI 的 `AWPR_SAMPLES_URL` secret 尚未設定，該步驟目前會跳過並標示 |
| 公信 CA 程式碼簽章憑證 | 仍是自簽，SmartScreen 會警告（見 legacy/windows 的 CLAUDE.md） |
| 一支腳本上傳全部平台 | 沒有，照上面的 curl 逐一上傳 |
