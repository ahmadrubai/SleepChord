; Compile with scripts/build-installer.ps1 using the staged Windows x64 release.
#ifndef AppVersion
  #error AppVersion must be supplied by scripts/build-installer.ps1.
#endif
#define AppName "SleepChord"
#define AppExe "SleepChord.exe"
#define AppIdValue "A4124301-3999-424E-A324-88FEDDD35FCD"
#define ReleaseDir "..\dist\windows-x64"
#define PartyProtocol "Software\Classes\bitchord"

[Setup]
AppId={#AppIdValue}
AppName={#AppName}
AppVersion={#AppVersion}
VersionInfoVersion={#AppVersion}
VersionInfoProductName={#AppName}
VersionInfoDescription=SleepChord Setup
DefaultDirName={localappdata}\Programs\{#AppName}
DefaultGroupName={#AppName}
DisableDirPage=auto
DisableProgramGroupPage=yes
UsePreviousAppDir=yes
PrivilegesRequired=lowest
SetupArchitecture=x64
ArchitecturesAllowed=x64os
ArchitecturesInstallIn64BitMode=x64os
MinVersion=10.0
OutputDir=..\dist\installer
OutputBaseFilename=SleepChord-Setup-{#AppVersion}-x64
SetupIconFile=..\assets\icon\icon.ico
WizardSmallImageFile=..\assets\icon\icon.png
WizardStyle=modern
Compression=lzma2
SolidCompression=yes
UninstallDisplayName={#AppName}
UninstallDisplayIcon={app}\{#AppExe},0
; The app recognizes a root-level unins000.exe as opt-in to an upstream updater.
UninstallFilesDir={app}\Uninstall
CloseApplications=yes
RestartApplications=no

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "partylinks"; Description: "Open SleepChord party invitation links"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked; Check: PartyLinksAvailable

[Files]
Source: "{#ReleaseDir}\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#ReleaseDir}\vcruntime140.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#ReleaseDir}\vcruntime140_1.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#ReleaseDir}\README.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#ReleaseDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#ReleaseDir}\THIRD-PARTY-NOTICES.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#ReleaseDir}\SHA256SUMS.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"; IconFilename: "{app}\{#AppExe}"
Name: "{userdesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"; IconFilename: "{app}\{#AppExe}"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "{#PartyProtocol}"; ValueType: string; ValueData: "URL:SleepChord party invitation"; Tasks: partylinks; Check: PartyLinksAvailable
Root: HKCU; Subkey: "{#PartyProtocol}"; ValueType: string; ValueName: "URL Protocol"; Tasks: partylinks; Check: PartyLinksAvailable
Root: HKCU; Subkey: "{#PartyProtocol}\DefaultIcon"; ValueType: string; ValueData: """{app}\{#AppExe}"",0"; Tasks: partylinks; Check: PartyLinksAvailable
Root: HKCU; Subkey: "{#PartyProtocol}\shell\open\command"; ValueType: string; ValueData: """{app}\{#AppExe}"" ""%1"""; Tasks: partylinks; Check: PartyLinksAvailable

[Run]
Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent
Filename: "https://developer.microsoft.com/microsoft-edge/webview2/"; Description: "Open Microsoft's WebView2 Runtime download page"; Flags: shellexec nowait postinstall skipifsilent unchecked; Check: WebView2Missing

[Code]
const
  UninstallKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{#AppIdValue}_is1';
  PartyProtocol = '{#PartyProtocol}';
  WebView2Key = 'Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';

var
  CanRegisterPartyLinks: Boolean;
  UpgradeDir: String;

// The app starts without creating any shortcuts. Inno owns the active links.
#include "legacy-shortcut.iss"

function PreviousInstallDir: String;
begin
  if not RegQueryStringValue(HKCU64, UninstallKey, 'Inno Setup: App Path', Result) then
    Result := '';
end;

function PartyCommand(Directory: String): String;
begin
  Result := '"' + AddBackslash(Directory) + '{#AppExe}" "%1"';
end;

function PartyLinksAvailable: Boolean;
begin
  Result := CanRegisterPartyLinks;
end;

function InitializeSetup: Boolean;
var
  CurrentCommand, InstalledDir: String;
begin
  CanRegisterPartyLinks := not RegKeyExists(HKCR, 'bitchord');
  InstalledDir := PreviousInstallDir;
  UpgradeDir := InstalledDir;
  if (InstalledDir <> '') and
    RegQueryStringValue(HKCU, PartyProtocol + '\shell\open\command', '', CurrentCommand) then
    CanRegisterPartyLinks := CompareText(CurrentCommand, PartyCommand(InstalledDir)) = 0;
  Result := True;
end;

function RuntimeVersionExists(RootKey: HKEY): Boolean;
var
  Version: String;
begin
  Result := RegQueryStringValue(RootKey, WebView2Key, 'pv', Version) and
    (Trim(Version) <> '') and (Trim(Version) <> '0.0.0.0');
end;

function WebView2Missing: Boolean;
begin
  Result := not (RuntimeVersionExists(HKLM32) or RuntimeVersionExists(HKCU));
end;

procedure InitializeWizard;
begin
  if WebView2Missing then
  begin
    Log('WebView2 Evergreen Runtime was not found. Continuing with offline SleepChord installation.');
    CreateOutputMsgPage(wpSelectTasks, 'Microsoft Edge WebView2 Runtime',
      'WebView2 is needed for sign-in and web verification.',
      'You can finish installing SleepChord and install WebView2 later. ' +
      'Microsoft offers an Evergreen Bootstrapper for online use and an Evergreen Standalone Installer for offline use.' + #13#10#13#10 +
      'https://developer.microsoft.com/microsoft-edge/webview2/');
  end;
end;

function DirectoryIsEmpty(Directory: String): Boolean;
var
  Entry: TFindRec;
begin
  Result := True;
  if FindFirst(AddBackslash(Directory) + '*', Entry) then
  try
    repeat
      if (Entry.Name <> '.') and (Entry.Name <> '..') then
      begin
        Result := False;
        Exit;
      end;
    until not FindNext(Entry);
  finally
    FindClose(Entry);
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Directory, InstalledDir, ProfileDir: String;
begin
  Result := '';
  Directory := AddBackslash(ExpandConstant('{app}'));
  InstalledDir := PreviousInstallDir;
  ProfileDir := AddBackslash(ExpandConstant('{userappdata}\ytm-rs'));
  if Pos(Lowercase(ProfileDir), Lowercase(Directory)) = 1 then
    Result := 'Choose a folder outside the SleepChord user profile.'
  else if InstalledDir <> '' then
  begin
    if CompareText(Directory, AddBackslash(InstalledDir)) <> 0 then
      Result := 'SleepChord is already installed in ' + InstalledDir + '. Use that folder for this upgrade.'
    else if FileExists(Directory + 'portable.txt') or FileExists(Directory + 'unins000.exe') then
      Result := 'An incompatible portable or legacy installation marker is present in the selected folder.';
  end
  else if DirExists(Directory) and not DirectoryIsEmpty(Directory) then
    Result := 'Choose an empty folder for this first installation of SleepChord.';
end;

procedure RemovePartyLinks;
var
  CurrentCommand, Icon, Description, Flag: String;
begin
  { Only clean up our values if the shared protocol still targets this copy. }
  if not RegQueryStringValue(HKCU, PartyProtocol + '\shell\open\command', '', CurrentCommand) then
    Exit;
  if CompareText(CurrentCommand, PartyCommand(ExpandConstant('{app}'))) <> 0 then
    Exit;
  RegDeleteValue(HKCU, PartyProtocol + '\shell\open\command', '');
  RegDeleteKeyIfEmpty(HKCU, PartyProtocol + '\shell\open\command');
  RegDeleteKeyIfEmpty(HKCU, PartyProtocol + '\shell\open');
  RegDeleteKeyIfEmpty(HKCU, PartyProtocol + '\shell');
  if RegQueryStringValue(HKCU, PartyProtocol + '\DefaultIcon', '', Icon) and
    (CompareText(Icon, ExpandConstant('"{app}\{#AppExe}",0')) = 0) then
    RegDeleteValue(HKCU, PartyProtocol + '\DefaultIcon', '');
  RegDeleteKeyIfEmpty(HKCU, PartyProtocol + '\DefaultIcon');
  if RegQueryStringValue(HKCU, PartyProtocol, '', Description) and
    (Description = 'URL:SleepChord party invitation') then
  begin
    RegDeleteValue(HKCU, PartyProtocol, '');
    if RegQueryStringValue(HKCU, PartyProtocol, 'URL Protocol', Flag) and (Flag = '') then
      RegDeleteValue(HKCU, PartyProtocol, 'URL Protocol');
  end;
  RegDeleteKeyIfEmpty(HKCU, PartyProtocol);
end;

procedure ArchiveObsoleteStartMenuShortcut(Directory: String);
begin
  ArchiveLegacyStartMenuShortcut(ExpandConstant('{userprograms}\SleepChord.lnk'), Directory,
    ExpandConstant('{userappdata}\ytm-rs\data\icons'),
    ExpandConstant('{localappdata}\SleepChord\Legacy Shortcuts'));
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
  begin
    if not WizardIsTaskSelected('partylinks') then
      RemovePartyLinks;
    if (UpgradeDir <> '') and (CompareText(AddBackslash(UpgradeDir), AddBackslash(ExpandConstant('{app}'))) = 0) then
      ArchiveObsoleteStartMenuShortcut(UpgradeDir);
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
  begin
    RemovePartyLinks;
    ArchiveObsoleteStartMenuShortcut(ExpandConstant('{app}'));
  end;
end;
