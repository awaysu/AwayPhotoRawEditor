; AwayPhotoRawEditor installer (Inno Setup 6).
; Built by scripts/package-windows.ps1, which passes the version and folders:
;   ISCC /DMyAppVersion=1.1.0 /DMyFileVersion=1.1.0.0 /DSourceDir=<staged files> /DOutputDir=<dist> AwayPhotoRawEditor.iss
; Per-user install (no administrator rights, no UAC): {localappdata}\Programs\AwayPhotoRawEditor.
;
; An in-place upgrade of the C# 1.0.x build: the same AppId and folder, and before the files
; go in, the old uninstallers run silently (C# 1.0.x, and the withdrawn Rust 2.0.x that was
; installed side by side in "AwayPhotoRawEditor 2"), so Settings > Apps lists one
; AwayPhotoRawEditor. Neither old uninstaller, nor this one, deletes user data: settings
; (%AppData%\AwayPhotoRawEditor), presets and the RAW_TEMP folders beside the photos stay.

#ifndef MyAppVersion
  #error Pass /DMyAppVersion=<version> (scripts/package-windows.ps1 does)
#endif
#ifndef MyFileVersion
  #define MyFileVersion "0.0.0.0"
#endif
#ifndef SourceDir
  #error Pass /DSourceDir=<folder with the staged files>
#endif
#ifndef OutputDir
  #define OutputDir "Output"
#endif

#define MyAppName "AwayPhotoRawEditor"
#define MyAppPublisher "Awaysu"
#define MyAppURL "https://www.awaysu.cc/software/awayphotoraweditor"
#define MyAppExeName "AwayPhotoRawEditor.exe"

[Setup]
; The C# 1.0.x id: Windows sees 1.1.0 as an upgrade of the same program. (The withdrawn
; 2.0.x used {9063DED4-6DA5-4A20-933D-AC78F788359C}; [Code] removes that install.)
AppId={{8E1A2C64-5A17-4D0B-9C67-AWPRE0100001}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppCopyright=Copyright (c) 2026 Chih-Wei Su (Awaysu)
VersionInfoVersion={#MyFileVersion}
VersionInfoProductName={#MyAppName}
VersionInfoCompany={#MyAppPublisher}
VersionInfoDescription={#MyAppName} Setup
DefaultDirName={autopf}\AwayPhotoRawEditor
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir={#OutputDir}
OutputBaseFilename=AwayPhotoRawEditor-Setup-v{#MyAppVersion}
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
UninstallDisplayName={#MyAppName} {#MyAppVersion}
LicenseFile={#SourceDir}\LICENSE.txt
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
; Chinese: the official Inno Setup translations (issrc Files/Languages), kept here with a
; UTF-8 BOM so ISCC reads them as UTF-8 whatever the build machine's code page.
Name: "chinesetraditional"; MessagesFile: "ChineseTraditional.isl"
Name: "chinesesimplified"; MessagesFile: "ChineseSimplified.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "japanese"; MessagesFile: "compiler:Languages\Japanese.isl"
Name: "korean"; MessagesFile: "compiler:Languages\Korean.isl"
Name: "german"; MessagesFile: "compiler:Languages\German.isl"
Name: "french"; MessagesFile: "compiler:Languages\French.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"

[Tasks]
; Desktop shortcut: offered, not ticked.
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[Code]
const
  UninstallKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\';
  CsharpId = '{8E1A2C64-5A17-4D0B-9C67-AWPRE0100001}_is1';
  Rust2Id = '{9063DED4-6DA5-4A20-933D-AC78F788359C}_is1';

// Run one old version's uninstaller silently and wait for it. A failure is logged, never
// fatal: the new files still go in.
procedure RemoveOldVersion(RootKey: Integer; RootName, Id: String);
var
  Cmd, Exe: String;
  Code, Waited: Integer;
begin
  if not RegQueryStringValue(RootKey, UninstallKey + Id, 'UninstallString', Cmd) then
    exit;
  Exe := RemoveQuotes(Cmd);
  Log(Format('Old version %s\...\%s: %s', [RootName, Id, Exe]));
  if not FileExists(Exe) then begin
    Log('  uninstaller not found; skipped');
    exit;
  end;
  if not Exec(Exe, '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART', '', SW_HIDE, ewWaitUntilTerminated, Code) then begin
    Log(Format('  could not run it: %s', [SysErrorMessage(Code)]));
    exit;
  end;
  // The uninstaller hands over to a copy of itself in %TEMP%; it is done once its exe is gone.
  Waited := 0;
  while FileExists(Exe) and (Waited < 120) do begin
    Sleep(500);
    Waited := Waited + 1;
  end;
  if FileExists(Exe) then
    Log(Format('  exit code %d; still installed after 60 s', [Code]))
  else
    Log(Format('  exit code %d; removed', [Code]));
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  RemoveOldVersion(HKCU, 'HKCU', Rust2Id);
  RemoveOldVersion(HKCU, 'HKCU', CsharpId);
  RemoveOldVersion(HKLM, 'HKLM', CsharpId);
  Result := '';
end;
