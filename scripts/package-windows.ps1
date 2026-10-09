# Windows release: build -> sign exe -> installer (Inno Setup) -> sign installer -> portable zip
# -> SHA256 (last: signing changes the bytes). Output in dist\ (git-ignored).
#
#   powershell -ExecutionPolicy Bypass -File scripts\package-windows.ps1 [-NoSign] [-Thumbprint <sha1>]
#
# Order matters: the exe goes inside the installer and the zip, so it is signed first; the
# installer is signed after ISCC builds it; checksums are computed after all signing.
# The certificate is chosen by thumbprint only (CN=Awaysu, CurrentUser\My) - never by a
# name match, which once picked a retired certificate.
param(
    [switch]$NoSign,
    [string]$Thumbprint = "997D278FE3FD6FFA1F8E43683047530DE7210C66",
    [string]$TimestampServer = "http://timestamp.digicert.com"
)
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

# ---- version: the workspace version in Cargo.toml is the only source ----
$cargo = Get-Content -Raw -Encoding UTF8 (Join-Path $Root "Cargo.toml")
if ($cargo -notmatch '(?ms)\[workspace\.package\].*?^version\s*=\s*"([^"]+)"') { throw "No workspace version in Cargo.toml" }
$Version = $Matches[1]
# Windows file versions are four numbers: 1.2.0-dev -> 1.2.0.0
$nums = ($Version -replace '[-+].*$', '').Split('.') + @('0', '0', '0', '0')
$FileVersion = ($nums[0..3] -join '.')
Write-Host "==> AwayPhotoRawEditor $Version (file version $FileVersion)"

$Dist = Join-Path $Root "dist"
$Name = "AwayPhotoRawEditor-v$Version"
$Stage = Join-Path $Dist $Name
New-Item -ItemType Directory -Force $Dist | Out-Null
if (Test-Path $Stage) { Remove-Item -Recurse -Force $Stage }
New-Item -ItemType Directory -Force $Stage | Out-Null

# ---- 1. build ----
Write-Host "==> 1/6  cargo build --release"
& cargo build --release -p awpr-app
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
$Exe = Join-Path $Root "target\release\AwayPhotoRawEditor.exe"

# ---- signing helper ----
$cert = $null
if (-not $NoSign) {
    $cert = Get-Item "Cert:\CurrentUser\My\$Thumbprint" -ErrorAction SilentlyContinue
    if (-not $cert) { throw "Certificate $Thumbprint not found in CurrentUser\My (use -NoSign for an unsigned build)" }
}
function Sign-File([string]$Path) {
    if ($NoSign) { Write-Host "    (unsigned) $Path"; return }
    $r = Set-AuthenticodeSignature -FilePath $Path -Certificate $cert -HashAlgorithm SHA256 -TimestampServer $TimestampServer
    # UnknownError / UntrustedRoot is the *verification* of a self-signed root, not a failure.
    $s = Get-AuthenticodeSignature -FilePath $Path
    if ($s.SignatureType -ne "Authenticode" -or -not $s.SignerCertificate -or -not $s.TimeStamperCertificate) {
        throw "Signing $Path failed: $($r.Status) $($r.StatusMessage)"
    }
    Write-Host "    signed $([IO.Path]::GetFileName($Path)) ($($s.Status), timestamp $($s.TimeStamperCertificate.Subject))"
}

# ---- 2. sign the exe, stage the files (same layout as installed) ----
Write-Host "==> 2/6  sign exe + stage"
Copy-Item $Exe $Stage
Sign-File (Join-Path $Stage "AwayPhotoRawEditor.exe")
Copy-Item (Join-Path $Root "LICENSE") (Join-Path $Stage "LICENSE.txt")
Copy-Item (Join-Path $Root "THIRD-PARTY-NOTICES.md") $Stage
Copy-Item (Join-Path $Root "crates\libraw-sys\vendor\LibRaw-0.22.2\LICENSE.CDDL") (Join-Path $Stage "LibRaw-LICENSE.CDDL.txt")
Copy-Item (Join-Path $Root "CHANGELOG.md") $Stage

# ---- 3. installer ----
Write-Host "==> 3/6  Inno Setup"
$iscc = Join-Path $env:LOCALAPPDATA "Programs\Inno Setup 6\ISCC.exe"
if (-not (Test-Path $iscc)) { $iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe" }
if (-not (Test-Path $iscc)) { throw "ISCC.exe (Inno Setup 6) not found" }
& $iscc /Q "/DMyAppVersion=$Version" "/DMyFileVersion=$FileVersion" "/DSourceDir=$Stage" "/DOutputDir=$Dist" (Join-Path $Root "installer\AwayPhotoRawEditor.iss")
if ($LASTEXITCODE -ne 0) { throw "ISCC failed" }
$Setup = Join-Path $Dist "AwayPhotoRawEditor-Setup-v$Version.exe"

# ---- 4. sign the installer ----
Write-Host "==> 4/6  sign installer"
Sign-File $Setup

# ---- 5. portable zip (the staged folder, base directory included) ----
Write-Host "==> 5/6  portable zip"
$Zip = Join-Path $Dist "$Name.zip"
if (Test-Path $Zip) { Remove-Item -Force $Zip }
Add-Type -AssemblyName System.IO.Compression.FileSystem
[IO.Compression.ZipFile]::CreateFromDirectory($Stage, $Zip, [IO.Compression.CompressionLevel]::Optimal, $true)

# ---- 6. checksums, after every signature ----
Write-Host "==> 6/6  SHA256SUMS.txt"
$sums = foreach ($f in @($Setup, $Zip)) {
    $h = (Get-FileHash -Algorithm SHA256 $f).Hash.ToLower()
    "$h  $([IO.Path]::GetFileName($f))"
}
$sumFile = Join-Path $Dist "SHA256SUMS.txt"
[IO.File]::WriteAllLines($sumFile, $sums)
$sums | ForEach-Object { Write-Host "    $_" }
Write-Host "==> Done: $Setup"
Write-Host "          $Zip"
