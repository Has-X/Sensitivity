# Installation

Use the latest archive from [GitHub Releases](https://github.com/Has-X/Sensitivity/releases).

Sensitivity is developed and published by [Chromatic](https://chromatic.hu). For product feedback, email [feedback@chromatic.hu](mailto:feedback@chromatic.hu).

## Windows

Choose `Sensitivity-Setup-x64.exe` for Intel and AMD PCs, or `Sensitivity-Setup-arm64.exe` for Windows on ARM. Both install the native Fluent 2 WinUI 3 application in Program Files and add a Start menu shortcut. Portable ZIPs are available for each architecture. Every package includes `sensitivity-cli.exe`, the WinUSB setup guide, and PowerShell completions. The installer follows the Windows light or dark preference; the app follows the system theme and accent color, uses Mica on supported Windows versions, and automatically selects English, Hungarian, or Spanish from the system language when available.

## Linux and macOS

Extract the platform archive and launch `sensitivity-gui` for the desktop interface or `sensitivity` for the CLI. Linux archives include the optional udev rule installer.

## Build from source

```console
cargo build --workspace --release --locked
```

The minimum supported Rust toolchain is 1.97.1.
