# Recovery safety

Sensitivity talks directly to stock Xiaomi Recovery in **Connect with Mi Assistant** mode. It does not unlock bootloaders or bypass account, anti-rollback, or FRP protections.

Before flashing:

1. Confirm the codename and region match the device.
2. Use an official Recovery ROM and verify its checksum.
3. Read the validation and wipe warning before approving the operation.
4. Keep the USB connection stable and do not interrupt a running transfer.

The `doctor`, `devices`, and `info` commands are read-only. ADB is preserved by default. Only use the explicit ADB stop policy when another process owns the recovery interface.

## Validation diagnostics

If Xiaomi accepts a request but does not return a usable validation token, save a redacted response for the issue report:

```console
sensitivity flash ROM.zip --dump-json validation-shape.json
```

On Windows, use `sensitivity-cli.exe`. The diagnostic keeps JSON field names and value types while replacing every scalar value. It does not contain the validation token, device serial number, server message, or ROM URL. Review any diagnostic file before attaching it to a public issue.
