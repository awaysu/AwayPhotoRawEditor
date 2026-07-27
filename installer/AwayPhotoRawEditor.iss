; Away PhotoRaw Editor 安裝檔（Inno Setup 6）
; 建置：先 dotnet publish -c Release -r win-x64 --self-contained true，
; 再用 ISCC 編譯本檔，輸出到 installer\Output\。
; 安裝為「每使用者」（無需系統管理員權限），目錄 {localappdata}\Programs\AwayPhotoRawEditor。

#define MyAppName "Away PhotoRaw Editor"
#define MyAppVersion "1.0.0"
#define MyAppPublisher "Awaysu"
#define MyAppURL "https://github.com/awaysu/AwayPhotoRawEditor"
#define MyAppExeName "AwayPhotoRawEditor.exe"
#define PublishDir "..\src\bin\Release\net8.0-windows\win-x64\publish"

[Setup]
AppId={{8E1A2C64-5A17-4D0B-9C67-AWPRE0100001}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
DefaultDirName={autopf}\AwayPhotoRawEditor
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=Output
OutputBaseFilename=AwayPhotoRawEditor-Setup-v{#MyAppVersion}
SetupIconFile=..\src\icon\icon.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#PublishDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\tools\*"; DestDir: "{app}\tools"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent
