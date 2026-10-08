; AwayPhotoRawEditor 2 installer (Inno Setup 6).
; Built by scripts/package-windows.ps1, which passes the version and folders:
;   ISCC /DMyAppVersion=2.0.0 /DMyFileVersion=2.0.0.0 /DSourceDir=<staged files> /DOutputDir=<dist> AwayPhotoRawEditor.iss
; Per-user install (no administrator rights, no UAC): {localappdata}\Programs\AwayPhotoRawEditor 2.
;
; Side by side with the C# 1.x build: a new AppId and its own folder, so installing 2.x
; neither replaces nor uninstalls 1.x. Both read the same settings and RAW_TEMP files, so a
; user can keep both. The shortcut is called "AwayPhotoRawEditor" like 1.x's (the display
; name is always AwayPhotoRawEditor), so it takes over 1.x's Start menu shortcut; 1.x still
; starts from its own folder and uninstalls from Settings > Apps. The uninstaller only removes what it installed:
; settings (%AppData%\AwayPhotoRawEditor) and the RAW_TEMP folders beside the photos stay.

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
; New for 2.x (the 1.x id is {8E1A2C64-5A17-4D0B-9C67-AWPRE0100001}); keep this one for every 2.x release.
AppId={{9063DED4-6DA5-4A20-933D-AC78F788359C}
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
DefaultDirName={autopf}\AwayPhotoRawEditor 2
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
