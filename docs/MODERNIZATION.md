# Sensitivity modernization

## Product decision

Sensitivity is the sole maintained product and repository. MiAssistantFork has been consolidated into it, not retained as a parallel implementation or compatibility layer.

The working direct-USB, validation, and sideload behavior remains authoritative. Both `sensitivity` and `sensitivity-gui` consume that one library implementation.

## Experience contract

A new user can download one archive, connect a phone in **Connect with Mi Assistant** recovery mode, and use either a guided GUI or the scriptable CLI without installing Rust, Android platform tools, or Xiaomi desktop software.

Both interfaces must:

- leave unrelated ADB processes alone unless the user explicitly requests otherwise;
- display destructive actions and require explicit confirmation;
- validate packages before sideloading;
- support cancellation without inventing an unverified resume protocol;
- avoid exposing validation tokens or device serials in normal logs.

## Unified architecture

- The root `sensitivity` library owns USB discovery, ADB framing, Xiaomi commands, validation, download, and sideload.
- The root CLI owns commands, prompts, diagnostics, JSON output, supervised progress/cancellation events, completions, and automation behavior.
- `apps/windows/Sensitivity.WinUI` is the native Fluent Design 2 / WinUI 3 presentation layer and supervises the CLI backend without receiving validation tokens.
- `crates/gui` is the portable Linux/macOS presentation layer and calls the same library operations.
- Release archives contain both front ends, documentation, and platform setup files.

See [MAF_CONSOLIDATION.md](MAF_CONSOLIDATION.md) for the disposition of MAF-specific features.

## Release gate

CI and offline tests prove compilation, parsing, framing, validation helpers, packaging, and front-end integration. Do not describe a release as hardware-validated until representative real devices have exercised detect/info, normal flash, server-required wipe, cancellation, failure reporting, and reboot. Record sanitized logs without serials or validation tokens.
