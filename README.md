SENSITIVITY (HASX)
===================

MiAssistant reimplementation in Rust: a clean, safe, production-ready CLI that talks ADB over USB (Mi Assistant mode), validates Xiaomi Recovery ROMs against the miotaV3 endpoint, and sideloads in 64 KiB chunks.

Key features
- Pure Rust, no adb.exe required
- USB via libusb (`rusb`): detects Mi Assistant interface 0xff/0x42/1
- ADB mini-protocol implementation (CONNECT/OPEN/WRTE/OKAY/CLSE)
- Xiaomi custom commands: getdevice, getsn, getversion, getcodebase, getbranch, getlanguage, getregion, getromzone, format-data, reboot
- ROM validation: AES-128-CBC + Base64 framing, HTTP(S) POST; robust diagnostics
- Sideload: `sideload-host:{size}:{chunk}:{token}:0`, 64 KiB chunks, smooth progress bar

Build
- Requires Rust 1.70+ (Edition 2021)
- On Windows, install libusb-1.0 (runtime DLL). If `libusb-1.0.dll` is missing, the tool will explain how to fix (use Zadig to bind WinUSB to device if needed).

Windows USB notes
- You may need to install a WinUSB driver for the ADB interface. Use Zadig to select the interface with class 0xff, subclass 0x42, protocol 1 and install WinUSB.
- Ensure `libusb-1.0.dll` is available (e.g., via vcpkg or a bundled copy). The program errors clearly if it cannot load the DLL.

CLI
- `miassistant read-info`
- `miassistant list-allowed-roms`
- `miassistant flash <path-to-zip> [--yes]`
- `miassistant format-data`
- `miassistant reboot`

Global flags
- `--device-index <n>`: choose among multiple devices (default 0)
- `--chunk-size <bytes>`: chunk size for sideload (default 65536)
- `--server-url <url>`: default `https://update.miui.com/updates/miotaV3.php`
- `--http`: allow HTTP (prints a big warning)
- `--debug-usb`: log raw USB packet directions/sizes
- `--verbose`: more logging

Tests
- AES-128-CBC + Base64 roundtrip vector
- JSON extraction between first `{` and last `}`
- MD5 of fixture file
- Optional integration test (feature `integration`): mocks server for validate flow

Example
```
miassistant read-info --verbose
miassistant flash C:\\ROM\\miui_recovery.zip --yes --debug-usb
```

Security and safety
- No `unsafe` used
- Handles short reads/writes as errors
- Explicit diagnostics for validation failures and malformed responses

