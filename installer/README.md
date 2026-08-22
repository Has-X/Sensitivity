# Windows installer

`Sensitivity.iss` builds native x64 or ARM64 installers with Inno Setup 7.1 or newer. It uses Inno's Windows 11 style, follows the system light or dark appearance, and includes the Inno language resources available for the supported application locales without adding a language-selection page. Hindi has no bundled Inno message pack and therefore uses the English installer messages while the installed app remains Hindi.

Build a self-contained Windows publish first, then compile it:

```powershell
dotnet publish apps/windows/Sensitivity.WinUI/Sensitivity.WinUI.csproj -c Release -r win-x64 --self-contained true -p:DebugType=None -p:DebugSymbols=false -o native-publish
Copy-Item target\x86_64-pc-windows-msvc\release\sensitivity.exe native-publish/sensitivity-cli.exe
& "$env:LOCALAPPDATA\Programs\Inno Setup 7\ISCC.exe" "/DAppVersion=1.1.3" "/DAppArchitecture=x64" "/DSourceDir=$PWD\native-publish" "/DOutputDir=$PWD\installer\out" installer/Sensitivity.iss
```

The result supports unattended package managers and deployment tools:

```powershell
.\Sensitivity-Setup-x64.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART
```

The installer adds its installation directory to the machine `PATH`, so a new
terminal can run `sensitivity-cli` without navigating to Program Files. It
removes only its own entry during uninstall.

For ARM64, publish with `-r win-arm64`, build the Rust backend for
`aarch64-pc-windows-msvc`, and pass `/DAppArchitecture=arm64`. The project
copies its generated `Sensitivity.pri` into every publish directory because it
is required by the unpackaged Windows App SDK runtime. Inno Setup's supported
64-bit bootstrapper is x64; on Windows on ARM it runs through Windows 11's
native x64 emulation and installs only the ARM64 app payload.

The installer is deliberately unsigned until a release-signing certificate is configured. Do not represent an unsigned build as signed.
