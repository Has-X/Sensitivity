# Recovery safety

Sensitivity talks directly to stock Xiaomi Recovery in **Connect with Mi Assistant** mode. It does not unlock bootloaders or bypass account, anti-rollback, or FRP protections.

Before flashing:

1. Confirm the codename and region match the device.
2. Use an official Recovery ROM and verify its checksum.
3. Read the validation and wipe warning before approving the operation.
4. Keep the USB connection stable and do not interrupt a running transfer.

The `doctor`, `devices`, and `info` commands are read-only. ADB is preserved by default. Only use the explicit ADB stop policy when another process owns the recovery interface.

ROM downloads use unique temporary files in the destination directory. Only a
checksum-verified download replaces the destination, using an atomic rename.
Failed transfers, checksum checks, and finalization attempts remove their own
temporary file and preserve any existing ROM.

The Windows app automatically selects a ZIP by default to help beginners.
Turn off **Settings → Automatically select a ZIP** to disable folder selection
and restoration of a previously selected path. Automatic discovery prefers a
codename match or a single ZIP and has no project-name exclusions. An explicit
current-session file choice is preserved. **Find ZIP** remains available when
automatic selection is off and asks you to review the result. A suggested or
selected ZIP is never compatibility approval: Xiaomi validation and the normal
flash/wipe confirmations remain mandatory before sideload.

For failures, use **Diagnostics → Export bug report** and include reproduction
steps. See [support logging](../SUPPORT_LOGGING.md) for contents and privacy.

Third-party catalogs can find full Recovery packages and additional copies of
Xiaomi's selected artifact. A listing is not compatibility approval. See
[firmware sources](../FIRMWARE_SOURCES.md) for adapters and download policy.

## Validation diagnostics

Check a local package without opening sideload, and save a redacted response for troubleshooting:

```console
sensitivity flash ROM.zip --validate-only --dump-json validation-shape.json
```

To inspect Xiaomi's package-discovery response without downloading a ROM:

```console
sensitivity list-allowed-roms --dump-json discovery-shape.json
```

On Windows, use `sensitivity-cli.exe`. The diagnostic keeps JSON field names and value types while replacing every scalar value. It does not contain the validation token, device serial number, server message, or ROM URL. Review any diagnostic file before attaching it to a public issue.
