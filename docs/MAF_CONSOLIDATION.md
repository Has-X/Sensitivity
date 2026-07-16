# MiAssistantFork consolidation ledger

MiAssistantFork has been folded into Sensitivity so users and maintainers have one repository and one protocol implementation.

The consolidation audit used MiAssistantFork commit `c26601f5e977ba6abe5fe8b94953b15280d7a8fd` as its source snapshot.

| MAF capability | Sensitivity disposition |
| --- | --- |
| Desktop GUI | Replaced on Windows by native WinUI 3 with Fluent Design 2; Linux and macOS retain `sensitivity-gui`. Both use the shared recovery implementation and include multi-device selection, validation, wipe confirmation, progress, cancellation, reboot, and format-data. |
| Modular core | Replaced by the root `sensitivity` library, consumed by both CLI and GUI. No second ADB/USB/sideload stack remains. |
| Cancellation/progress callback | Merged into `sideload_zip_with_progress`; the CLI and GUI provide their own presentation. |
| Permissive recovery USB matching | Merged safely as exact protocol-1 preference with class/subclass fallback only when no exact interface exists. |
| Fuzzing | Merged as the `fuzz/` cargo-fuzz target for the ADB header decoder. |
| Precompiled release automation | Replaced with one matrix that builds the CLI and GUI for Linux, Windows, Intel macOS, and Apple Silicon macOS, plus checksums, a Windows MSI, a macOS app bundle, and platform setup guides. |
| Raw arbitrary ADB command | Omitted. The public interface exposes supported recovery operations rather than an unsafe, hard-to-support escape hatch. |
| Claimed interrupted-transfer resume | Rejected as unsafe. MAF sends its start block in the protocol field verified by Sensitivity as the wipe flag; values other than zero could request data erasure. |
| Separate CLI protocol code | Removed as redundancy. Compatibility command names and migration mappings are documented instead. |
| MAF branding/assets and old WiX metadata | Removed. Release archives are the supported precompiled distribution and carry Sensitivity branding and AGPL metadata. |

The old repository can remain archived online for historical attribution, but it is not needed to build, test, package, or develop Sensitivity.
