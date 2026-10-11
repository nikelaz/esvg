#define AppVersion GetEnv("APP_VERSION")
#define AppBinary GetEnv("APP_BINARY")
#define CliBinary GetEnv("CLI_BINARY")

#if AppVersion == "" || AppBinary == "" || CliBinary == ""
  #error APP_VERSION, APP_BINARY and CLI_BINARY must be set when compiling this installer
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
; Relative to this script, so this is the repository root dist folder that the release uploads.
OutputDir=..\..\..\..\dist
OutputBaseFilename=esvg-{#AppVersion}-windows-x86_64-setup
SetupIconFile=..\..\assets\icons\windows\esvg.ico
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
; Lets new terminals see the PATH change without signing out.
ChangesEnvironment=yes

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional shortcuts:"; Flags: unchecked
Name: "addtopath"; Description: "Add the esvg command to PATH (open a new terminal after installing)"; GroupDescription: "Command line:"; Flags: checkedonce

[Files]
Source: "{#AppBinary}"; DestDir: "{app}"; Flags: ignoreversion
; The command line tool lives in its own folder, so `esvg` on PATH does not clash with the GUI's esvg.exe.
Source: "{#CliBinary}"; DestDir: "{app}\cli"; DestName: "esvg.exe"; Flags: ignoreversion

[Registry]
; Records that this installer added the PATH entry, so uninstalling only removes an entry it made.
; Listed first: entries run in order, and NeedsAddPath is false once the PATH entry below is written.
Root: HKCU; Subkey: "Software\ESVG"; ValueType: dword; ValueName: "AddedCliToPath"; ValueData: 1; Tasks: addtopath; Check: NeedsAddPath(ExpandConstant('{app}\cli')); Flags: uninsdeletekey
; Appends the CLI folder to the user's PATH. Only the user's own PATH is touched.
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; ValueData: "{olddata};{app}\cli"; Tasks: addtopath; Check: NeedsAddPath(ExpandConstant('{app}\cli')); Flags: preservestringtype

[Icons]
Name: "{autoprograms}\ESVG"; Filename: "{app}\esvg.exe"
Name: "{autodesktop}\ESVG"; Filename: "{app}\esvg.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\esvg.exe"; Description: "Launch ESVG"; Flags: postinstall nowait skipifsilent

[Code]
// Uninstalling does not use uninsdeletevalue, because that would delete the whole PATH.
// Instead, only the CLI folder is taken out of the value, and only if this installer added it.

function NeedsAddPath(Dir: string): Boolean;
var
  OrigPath: string;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', OrigPath) then
  begin
    Result := True;
    exit;
  end;
  Result := Pos(';' + Uppercase(Dir) + ';', ';' + Uppercase(OrigPath) + ';') = 0;
end;

// Returns Path without the entries equal to Dir. Every other entry, including
// empty ones, is kept exactly as it was.
function WithoutPathEntry(Path, Dir: string): string;
var
  Rest, Entry: string;
  Separator: Integer;
  First, Done: Boolean;
begin
  Result := '';
  Rest := Path;
  First := True;
  Done := False;
  while not Done do
  begin
    Separator := Pos(';', Rest);
    if Separator = 0 then
    begin
      Entry := Rest;
      Done := True;
    end
    else
    begin
      Entry := Copy(Rest, 1, Separator - 1);
      Delete(Rest, 1, Separator);
    end;

    if CompareText(Entry, Dir) <> 0 then
    begin
      if First then
        Result := Entry
      else
        Result := Result + ';' + Entry;
      First := False;
    end;
  end;
end;

procedure RemoveFromPath(Dir: string);
var
  OrigPath, NewPath: string;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', OrigPath) then
    exit;
  NewPath := WithoutPathEntry(OrigPath, Dir);
  if NewPath <> OrigPath then
    RegWriteExpandStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', NewPath);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Added: Cardinal;
begin
  // usUninstall runs before the uninstaller deletes the Software\ESVG marker.
  if CurUninstallStep = usUninstall then
    if RegQueryDWordValue(HKEY_CURRENT_USER, 'Software\ESVG', 'AddedCliToPath', Added) and (Added = 1) then
      RemoveFromPath(ExpandConstant('{app}\cli'));
end;
