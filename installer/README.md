# Windows installer

`Sensitivity.iss` builds the native x64 installer with Inno Setup 7.1 or newer. It uses Inno's Windows 11 style, follows the system light or dark appearance, and includes English, Hungarian, and Spanish setup resources without adding a language-selection page.

Build a self-contained Windows publish first, then compile it:

```powershell
dotnet publish apps/windows/Sensitivity.WinUI/Sensitivity.WinUI.csproj -c Release -r win-x64 --self-contained true -p:DebugType=None -p:DebugSymbols=false -o native-publish
Copy-Item target/release/sensitivity.exe native-publish/sensitivity-cli.exe
& "$env:LOCALAPPDATA\Programs\Inno Setup 7\ISCC.exe" "/DAppVersion=1.1.2" "/DSourceDir=$PWD\native-publish" "/DOutputDir=$PWD\installer\out" installer/Sensitivity.iss
```

The result supports unattended package managers and deployment tools:

```powershell
.\Sensitivity-Setup-x64.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART
```

The installer is deliberately unsigned until a release-signing certificate is configured. Do not represent an unsigned build as signed.
