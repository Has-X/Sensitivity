; Sensitivity's native Windows 11 installer.
; Pass /DAppVersion, /DSourceDir and /DOutputDir from CI or a local build.

#ifndef AppVersion
  #define AppVersion "1.1.2"
#endif
#ifndef SourceDir
  #define SourceDir "..\\native-publish"
#endif
#ifndef OutputDir
  #define OutputDir "..\\dist"
#endif
#ifndef AppArchitecture
  #define AppArchitecture "x64"
#endif
#if AppArchitecture == "x64"
  #define InstallArchitecture "x64compatible"
#else
  #define InstallArchitecture "arm64"
#endif

[Setup]
AppId={{A2E220C2-402D-4B6F-94D2-04D09F30A25E}
AppName=Sensitivity
AppVersion={#AppVersion}
AppVerName=Sensitivity {#AppVersion}
AppPublisher=Chromatic
AppPublisherURL=https://chromatic.hu
AppSupportURL=mailto:feedback@chromatic.hu
AppUpdatesURL=https://github.com/Has-X/Sensitivity/releases
DefaultDirName={autopf}\Sensitivity
DefaultGroupName=Sensitivity
UninstallDisplayIcon={app}\Sensitivity.exe
SetupIconFile=assets\app.ico
OutputDir={#OutputDir}
OutputBaseFilename=Sensitivity-Setup-{#AppArchitecture}
VersionInfoVersion={#AppVersion}
VersionInfoCompany=Chromatic
VersionInfoDescription=Sensitivity Installer
VersionInfoProductName=Sensitivity
VersionInfoProductVersion={#AppVersion}
ArchitecturesAllowed={#InstallArchitecture}
ArchitecturesInstallIn64BitMode={#InstallArchitecture}
; Inno Setup 7 can emit a native x64 setup bootstrapper. On Windows on ARM,
; Windows 11 runs that bootstrapper through x64 emulation while it installs the
; native ARM64 Sensitivity files selected by AppArchitecture.
SetupArchitecture=x64
PrivilegesRequired=admin
DisableWelcomePage=yes
DisableProgramGroupPage=yes
DisableReadyPage=yes
DisableDirPage=auto
ShowLanguageDialog=no
WizardStyle=modern dynamic windows11 hidebevels
WizardSizePercent=120,120
WizardKeepAspectRatio=yes
WizardBackColor=$FFFFFF
WizardBackColorDynamicDark=$000000
WizardBackImageFile=assets\installer-background-light.png
WizardBackImageFileDynamicDark=assets\installer-background-dark.png
WizardImageFile=
WizardSmallImageFile=
Compression=lzma2/ultra64
SolidCompression=yes
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "hungarian"; MessagesFile: "compiler:Languages\Hungarian.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "german"; MessagesFile: "compiler:Languages\German.isl"
Name: "french"; MessagesFile: "compiler:Languages\French.isl"

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[InstallDelete]
; A self-contained WinUI publish changes its runtime file set between releases.
; Remove only files owned by the application so superseded framework binaries do
; not survive an upgrade. User-provided ROM archives are intentionally untouched.
Type: files; Name: "{app}\*.dll"
Type: files; Name: "{app}\*.exe"
Type: files; Name: "{app}\*.json"
Type: files; Name: "{app}\*.pri"
Type: files; Name: "{app}\*.winmd"

[Icons]
Name: "{autoprograms}\Sensitivity"; Filename: "{app}\Sensitivity.exe"; WorkingDir: "{app}"

[Run]
Filename: "{app}\Sensitivity.exe"; Description: "{cm:LaunchProgram,Sensitivity}"; Flags: nowait postinstall skipifsilent

[Code]
const
  EnvironmentKey = 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment';
  WM_SETTINGCHANGE = $001A;
  SMTO_ABORTIFHUNG = $0002;

function SendMessageTimeout(hWnd: HWND; Msg: UINT; wParam: Longint; lParam: String;
  fuFlags, uTimeout: UINT; var lpdwResult: DWORD): LRESULT;
  external 'SendMessageTimeoutW@user32.dll stdcall';

procedure NotifyEnvironmentChanged;
var
  ResultCode: DWORD;
begin
  SendMessageTimeout(HWND_BROADCAST, WM_SETTINGCHANGE, 0, 'Environment',
    SMTO_ABORTIFHUNG, 5000, ResultCode);
end;

function PathContains(const PathValue, Directory: String): Boolean;
begin
  Result := Pos(';' + Lowercase(Directory) + ';',
    ';' + Lowercase(PathValue) + ';') > 0;
end;

procedure AddCommandLineToPath;
var
  PathValue: String;
  AppDirectory: String;
begin
  AppDirectory := ExpandConstant('{app}');
  if not RegQueryStringValue(HKLM64, EnvironmentKey, 'Path', PathValue) then begin
    Log('Unable to read the machine PATH. The Sensitivity CLI was not added.');
    exit;
  end;
  if PathContains(PathValue, AppDirectory) then begin
    Log('Sensitivity is already present on the machine PATH.');
    exit;
  end;
  if RegWriteStringValue(HKLM64, EnvironmentKey, 'Path', PathValue + ';' + AppDirectory) then begin
    NotifyEnvironmentChanged;
    Log('Added Sensitivity to the machine PATH.');
  end else
    Log('Unable to add Sensitivity to the machine PATH.');
end;

procedure RemoveCommandLineFromPath;
var
  PathValue: String;
  AppDirectory: String;
  UpdatedPath: String;
begin
  AppDirectory := ExpandConstant('{app}');
  if not RegQueryStringValue(HKLM64, EnvironmentKey, 'Path', PathValue) then exit;
  UpdatedPath := PathValue;
  StringChangeEx(UpdatedPath, ';' + AppDirectory, '', True);
  StringChangeEx(UpdatedPath, AppDirectory + ';', '', True);
  if CompareText(UpdatedPath, AppDirectory) = 0 then UpdatedPath := '';
  if UpdatedPath = PathValue then exit;
  if RegWriteStringValue(HKLM64, EnvironmentKey, 'Path', UpdatedPath) then begin
    NotifyEnvironmentChanged;
    Log('Removed Sensitivity from the machine PATH.');
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then AddCommandLineToPath;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then RemoveCommandLineFromPath;
end;
