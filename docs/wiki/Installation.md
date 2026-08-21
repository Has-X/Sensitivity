# Installation

Use the latest archive from [GitHub Releases](https://github.com/Has-X/Sensitivity/releases).

## Windows

Run `Sensitivity-Setup-x64.exe` to install Sensitivity in Program Files and launch it from Start, or extract the portable ZIP. The archive includes the native WinUI 3 app, `sensitivity-cli.exe`, the WinUSB setup guide, and PowerShell completions. The installer follows the Windows light or dark preference and automatically selects English, Hungarian, or Spanish from the system language when available.

## Linux and macOS

Extract the platform archive and launch `sensitivity-gui` for the desktop interface or `sensitivity` for the CLI. Linux archives include the optional udev rule installer.

## Build from source

```console
cargo build --workspace --release --locked
```

The minimum supported Rust toolchain is 1.95.
