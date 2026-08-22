# Native Windows architecture

Sensitivity's Windows application is an unpackaged, self-contained WinUI 3
desktop app using the current stable Windows App SDK. Its Fluent 2 shell uses
adaptive NavigationView layouts, Segoe Fluent icons, Windows system theme and
accent resources, accessible native controls, and Mica where supported. The
recovery protocol remains in the shared Rust core and is shipped as
`sensitivity-cli.exe` beside the user-facing `Sensitivity.exe`.

The publish target explicitly copies Sensitivity's generated PRI resource file.
Windows App SDK 2.x currently omits this file from unpackaged publish output by
default, which causes an immediate WinUI startup failure without this safeguard.

## Process boundary

The app starts the backend with `ProcessStartInfo.ArgumentList`; ROM paths are
never interpolated into a command line. Standard output and standard error are
read concurrently so a full pipe cannot stall a recovery operation.

Long-running commands use a private temporary directory containing two
one-shot control files:

- creating `cancel` requests a graceful USB close;
- creating `approve-wipe` approves a server-required data wipe.

The backend deletes stale control files before connecting. The app first asks
for graceful cancellation and only terminates the process tree after an
eight-second timeout. The temporary directory is removed when the operation
ends.

## Machine event contract

`sensitivity-cli.exe --machine` writes one JSON object per line. Current events are:

```json
{"event":"status","message":"Validating ROM with Xiaomi"}
{"event":"progress","current":1048576,"total":4194304}
{"event":"confirmation_required","kind":"data_wipe","message":"..."}
{"event":"completed","message":"Flash completed"}
{"event":"error","message":"..."}
```

Unknown event names and extra fields must be ignored by supervisors. Human
diagnostic text may still appear, so clients should parse only complete JSON
objects. Exit status remains authoritative: zero is success and non-zero is
failure.

Validation tokens and raw protocol authentication values are deliberately not
included in events, diagnostics, settings, or process arguments. App settings
contain only ADB preferences and the last selected ROM path in the current
user's local application-data directory.

## ADB coexistence

The safe default is to leave the user's ADB server running. If Windows reports
that the recovery USB interface is busy, the app offers to stop ADB once and
retry; it does not silently disrupt another debugging session. Users with a
persistently conflicting setup can opt into stopping ADB before every direct
USB connection.

## Build and release

The Windows CI job is the authoritative XAML compilation check because the
Windows App SDK XAML compiler is a Windows executable. Releases publish the
WinUI app and Rust backend together for both x64 and ARM64, then produce a
portable ZIP and architecture-matched Inno Setup installer for each. The Start
menu shortcut launches only `Sensitivity.exe`; the CLI remains available for
automation and diagnostics.
