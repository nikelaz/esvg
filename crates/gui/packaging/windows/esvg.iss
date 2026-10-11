#define AppVersion GetEnv("APP_VERSION")
#define AppBinary GetEnv("APP_BINARY")

#if AppVersion == "" || AppBinary == ""
  #error APP_VERSION and APP_BINARY must be set when compiling this installer
#endif

[Setup]
AppId={{A15B63E0-7C4D-4A6C-B4C1-DC548FBB1D8F}
AppName=ESVG
AppVersion={#AppVersion}
AppPublisher=ESVG
DefaultDirName={localappdata}\Programs\ESVG
DefaultGroupName=ESVG
UninstallDisplayIcon={app}\esvg.exe
ArchitecturesAllowed=x64
ArchitecturesInstallIn64BitMode=x64
PrivilegesRequired=lowest
OutputDir=..\..\dist
OutputBaseFilename=esvg-{#AppVersion}-windows-x86_64-setup
SetupIconFile=..\..\assets\icons\windows\esvg.ico
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional shortcuts:"; Flags: unchecked

[Files]
Source: "{#AppBinary}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\ESVG"; Filename: "{app}\esvg.exe"
Name: "{autodesktop}\ESVG"; Filename: "{app}\esvg.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\esvg.exe"; Description: "Launch ESVG"; Flags: postinstall nowait skipifsilent
