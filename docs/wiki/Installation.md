# Installation

Use the latest archive from [GitHub Releases](https://github.com/Has-X/Sensitivity/releases).

## Windows

Install the MSI and launch **Sensitivity** from Start, or extract the portable ZIP. The archive includes the native WinUI 3 app, `sensitivity-cli.exe`, the WinUSB setup guide, and PowerShell completions.

## Linux and macOS

Extract the platform archive and launch `sensitivity-gui` for the desktop interface or `sensitivity` for the CLI. Linux archives include the optional udev rule installer.

## Build from source

```console
cargo build --workspace --release --locked
```

The minimum supported Rust toolchain is 1.88.
