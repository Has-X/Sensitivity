# Windows setup

Run `Sensitivity-Setup-x64.exe` and open **Sensitivity** from the Start menu, or extract the portable ZIP and open `Sensitivity.exe`. The installer places the application in `Program Files\Sensitivity`, creates a Start menu entry, and registers a normal Windows uninstall entry. The Windows interface is self-contained; no separate .NET or Windows App SDK installation is required.

The installer follows the Windows light or dark setting. The app selects a supported system language or in-app override, including the 34 runtime catalogs listed in the project localization guide. For unattended deployment, use `Sensitivity-Setup-x64.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART`.

The app includes diagnostics and can explain likely driver or ADB ownership problems. The installer adds `sensitivity-cli.exe` to the machine `PATH`; open a new terminal after setup and run `sensitivity-cli` for scripts and detailed troubleshooting.

## One-time WinUSB setup

Stock Xiaomi recovery exposes a vendor-specific Mi Assistant USB interface. Windows must associate that interface with its built-in WinUSB driver before Sensitivity can claim it directly.

1. Boot the phone into stock recovery and choose **Connect with Mi Assistant**.
2. Connect the phone directly, without a USB hub.
3. Open Zadig as administrator and enable **Options > List All Devices**.
4. Select the Mi Assistant/Android interface whose class is `ff`, subclass `42`, and protocol `01`. Do not select an unrelated phone interface.
5. Choose **WinUSB** and install/replace the driver.
6. Reconnect the phone, open Sensitivity, and select **Diagnostics > Run diagnostics**.

Sensitivity leaves the normal ADB server running by default. If it appears to own the recovery interface, the app asks before stopping ADB and retrying. From the CLI, the equivalent command is:

```powershell
.\sensitivity-cli.exe --adb-policy stop doctor
```

The old driver can be restored through Windows Device Manager by uninstalling the selected USB interface and reconnecting the phone.
