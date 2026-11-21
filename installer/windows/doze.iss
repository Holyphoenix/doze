; Doze Windows Installer & In-Place Updater (Inno Setup 6)
; Running a newer installer with the same AppId automatically updates the existing
; installation in place while preserving user settings & secrets in %APPDATA%\doze\Doze\.

#ifndef MyAppVersion
  #define MyAppVersion "0.1.0"
#endif

#define MyAppName "Doze"
#define MyAppPublisher "Doze Contributors"
#define MyAppExeName "doze.exe"
#define MyAppId "{{8E3C6B42-9C77-4A8D-9E2E-6E4F7D2A1B90}"

[Setup]
AppId={#MyAppId}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
; Per-user install by default (no admin required), can still be overridden via /ALLUSERS
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
UsePreviousAppDir=yes
UsePreviousTasks=yes
CloseApplications=force
CloseApplicationsFilter=doze.exe
RestartApplications=no
OutputDir=..\..\dist
OutputBaseFilename=Doze-Setup-x86_64
SetupIconFile=..\..\assets\doze-icon.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
ChangesEnvironment=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "addtopath"; Description: "Add Doze to your user PATH (run 'doze' from terminal)"; GroupDescription: "System Integration"

[Files]
Source: "..\..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\assets\doze-icon.ico"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\doze-icon.ico"
Name: "{group}\Uninstall {#MyAppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\doze-icon.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[Code]
const
  EnvKey = 'Environment';

procedure AddToUserPath(Dir: string);
var
  CurrentPath: string;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, EnvKey, 'Path', CurrentPath) then
    CurrentPath := '';
  if Pos(';' + Uppercase(Dir) + ';', ';' + Uppercase(CurrentPath) + ';') = 0 then
  begin
    if (CurrentPath <> '') and (CurrentPath[Length(CurrentPath)] <> ';') then
      CurrentPath := CurrentPath + ';';
    CurrentPath := CurrentPath + Dir;
    RegWriteExpandStringValue(HKEY_CURRENT_USER, EnvKey, 'Path', CurrentPath);
  end;
end;

procedure RemoveFromUserPath(Dir: string);
var
  CurrentPath, UpperPath, UpperDir: string;
  P: Integer;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, EnvKey, 'Path', CurrentPath) then
    Exit;
  UpperPath := ';' + Uppercase(CurrentPath) + ';';
  UpperDir := ';' + Uppercase(Dir) + ';';
  P := Pos(UpperDir, UpperPath);
  if P > 0 then
  begin
    Delete(CurrentPath, P, Length(Dir) + 1);
    if (Length(CurrentPath) > 0) and (CurrentPath[Length(CurrentPath)] = ';') then
      Delete(CurrentPath, Length(CurrentPath), 1);
    RegWriteExpandStringValue(HKEY_CURRENT_USER, EnvKey, 'Path', CurrentPath);
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if (CurStep = ssPostInstall) and WizardIsTaskSelected('addtopath') then
    AddToUserPath(ExpandConstant('{app}'));
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    RemoveFromUserPath(ExpandConstant('{app}'));
end;
