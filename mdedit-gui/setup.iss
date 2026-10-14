; MDEditor 安装脚本 (Inno Setup 6)
[Setup]
AppId={{A5C3E2D1-9B4F-4E6A-8C2D-3F1A7B5E9D0C}
AppName=MDEditor
AppVersion=1.0.0
AppPublisher=MDEditor
AppPublisherURL=https://www.yuanyuzhou.me
DefaultDirName={autopf}\MDEditor
DefaultGroupName=MDEditor
OutputDir=.
OutputBaseFilename=MDEditor_setup_1.0.0
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64
ArchitecturesInstallIn64BitMode=x64
SetupIconFile=mdedit.ico
UninstallDisplayIcon={app}\mdedit.exe

[Tasks]
Name: "desktopicon"; Description: "创建桌面快捷方式"; GroupDescription: "附加任务："

[Files]
Source: "mdedit.exe"; DestDir: "{app}"; Flags: ignoreversion; AfterInstall: SetAssocCommand(ExpandConstant('{app}\mdedit.exe'))

[Icons]
Name: "{group}\MDEditor"; Filename: "{app}\mdedit.exe"
Name: "{group}\卸载 MDEditor"; Filename: "{uninstallexe}"
Name: "{autodesktop}\MDEditor"; Filename: "{app}\mdedit.exe"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Classes\.md"; ValueType: string; ValueName: ""; ValueData: "mdedit.file"; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\.markdown"; ValueType: string; ValueName: ""; ValueData: "mdedit.file"; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\mdedit.file"; ValueType: string; ValueName: ""; ValueData: "MDEditor"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\mdedit.file\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\mdedit.exe,0"
Root: HKCU; Subkey: "Software\Classes\mdedit.file\shell\open"; Flags: uninsdeletekey

[Code]
procedure SetAssocCommand(ExePath: String);
begin
  RegWriteStringValue(HKCU, 'Software\Classes\mdedit.file\shell\open\command', '', '"' + ExePath + '" "%1"');
end;