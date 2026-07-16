# Migrating from MiAssistantFork

Sensitivity is the maintained successor to MiAssistantFork (MAF) and now contains its useful GUI, cancellation, packaging, and fuzz-testing ideas without retaining a duplicate protocol implementation. Verify the connected recovery with `sensitivity doctor`, then replace an existing MAF workflow.

## Command mapping

| MiAssistantFork | Sensitivity | Notes |
| --- | --- | --- |
| `miassistant-cli detect` | `sensitivity detect` | Performs the direct-USB protocol handshake. |
| `miassistant-cli info` | `sensitivity info` | Human-readable by default; add `--json` for scripts. |
| `miassistant-cli list-roms` | `sensitivity list-allowed-roms` | Uses the connected device identity. |
| `miassistant-cli flash ROM.zip` | `sensitivity flash ROM.zip` | Hashes, validates, and sideloads the package. |
| `miassistant-cli sideload ROM.zip --validate TOKEN` | `sensitivity flash ROM.zip --token TOKEN` | Add `--wipe` only when the recovery requires it. |
| `miassistant-cli format-data` | `sensitivity format-data` | Requires typing `ERASE`, or `--yes` for intentional automation. |
| `miassistant-cli reboot` | `sensitivity reboot` | Same recovery command. |

The duplicate `miassistant` executable has been retired. Update scripts to use the `sensitivity` name.

## Behavioral differences

- Sensitivity leaves the desktop ADB server alone by default. Use `--adb-policy stop` only when diagnostics show that ADB owns the recovery interface.
- Sensitivity requires HTTPS for Xiaomi validation unless the hidden advanced override is explicitly supplied.
- Downloads are written to a hidden partial file and only replace the destination after MD5 verification.
- Ctrl-C or the GUI Cancel button requests a graceful close after the current USB operation.
- MAF's claimed resume field is intentionally unavailable: protocol review found that the same field is the verified data-wipe flag, so sending a block offset there could request an unintended wipe.
- Arbitrary raw ADB commands are not part of the normal interface. Sensitivity exposes the supported Xiaomi recovery operations directly.
- Windows uses the native `Sensitivity.App.exe` WinUI 3 interface. Linux and macOS use `sensitivity-gui`. Both consume the same tested Rust recovery implementation and include multi-device selection, validation, wipe confirmation, progress, and cancellation.

## Automation changes

Use `sensitivity info --json` rather than parsing labeled text. Generate completion definitions with `sensitivity completions bash`, `zsh`, `fish`, `elvish`, or `powershell`.

Scripts that can erase data must pass `--yes` explicitly. Scripts should not pass `--adb-policy stop` globally; doing so stops a running ADB server even for devices unrelated to the recovery operation.
