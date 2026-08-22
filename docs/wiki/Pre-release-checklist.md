# Release preflight

Do not create a release tag until the source revision has passed the automated
release preflight and the remaining hardware gates are recorded.

## Automated gate

On Windows, run:

```powershell
pwsh -NoProfile -File tools/preflight.ps1
```

This checks formatting, every locale catalog, Clippy, tests, fuzz-target
compilation, release builds, a self-contained WinUI publish, runtime file
composition, CLI startup for every supported language, and the Inno Setup
installer when ISCC is installed. The output directory contains hashes for the
generated preflight files.

For a source-only check on a machine without Inno Setup, use:

```powershell
pwsh -NoProfile -File tools/preflight.ps1 -SkipInstaller
```

That is not equivalent to validating the final installer. The release workflow
installs Inno Setup and compiles both x64 and ARM64 installers.

## Human and hardware gate

The automated gate cannot prove USB driver ownership or a real recovery
transfer. Before declaring a release hardware-validated, record sanitized
evidence for:

1. Device discovery and information read in stock Recovery's **Connect with Mi Assistant** mode.
2. A successful official Recovery ROM download and checksum validation.
3. A normal flash, a server-required wipe confirmation, cancellation, and reboot.
4. Windows x64 and ARM64 installation, Start menu launch, uninstall, and a new terminal resolving `sensitivity-cli` from `PATH`.
5. At least one narrow-window and RTL visual pass. Arabic requires RTL testing; Chinese, Japanese, Korean, Thai, and Hindi need high-DPI text checks.

Never include serial numbers, validation tokens, raw USB captures, or private
ROM URLs in the evidence.
