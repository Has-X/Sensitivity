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

[Setup]
AppId={{A2E220C2-402D-4B6F-94D2-04D09F30A25E}
AppName=Sensitivity
AppVersion={#AppVersion}
AppVerName=Sensitivity {#AppVersion}
AppPublisher=HasX
AppPublisherURL=https://github.com/Has-X/Sensitivity
AppSupportURL=https://github.com/Has-X/Sensitivity/issues
AppUpdatesURL=https://github.com/Has-X/Sensitivity/releases
DefaultDirName={autopf}\Sensitivity
DefaultGroupName=Sensitivity
UninstallDisplayIcon={app}\Sensitivity.exe
SetupIconFile=assets\app.ico
OutputDir={#OutputDir}
OutputBaseFilename=Sensitivity-Setup-x64
VersionInfoVersion={#AppVersion}
VersionInfoCompany=HasX
VersionInfoDescription=Sensitivity Installer
VersionInfoProductName=Sensitivity
VersionInfoProductVersion={#AppVersion}
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
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

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\Sensitivity"; Filename: "{app}\Sensitivity.exe"; WorkingDir: "{app}"

[Run]
Filename: "{app}\Sensitivity.exe"; Description: "{cm:LaunchProgram,Sensitivity}"; Flags: nowait postinstall skipifsilent
