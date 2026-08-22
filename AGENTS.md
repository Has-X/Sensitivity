# Sensitivity engineering guide

## Scope and architecture

Sensitivity has one Rust recovery core. The CLI in `src/` is the authority for
USB, ROM validation, download, sideload, and cancellation behaviour. Front
ends supervise that core; they do not recreate recovery protocol logic.

- `apps/windows/Sensitivity.WinUI` is the native WinUI 3 Windows application.
- `crates/gui` is the portable eframe GUI for Linux and macOS.
- `installer/` contains the Inno Setup 7 Windows installer.
- `locales/` contains the versioned CLI, portable GUI, and Windows catalogs.

Do not add a second protocol implementation, invoke shells for backend
commands, or pass sensitive values through logs, settings, or process
arguments. Use `ProcessStartInfo.ArgumentList` in the Windows app.

## Safety boundaries

Flashing and wiping are destructive. Preserve these invariants:

- Validate a Recovery ROM before sideloading it.
- Keep the default ADB policy non-disruptive. Stop ADB only after an explicit
  user choice or CLI policy.
- Require clear confirmation for a server-requested wipe.
- Never commit device serial numbers, validation tokens, recovery captures,
  private ROM links, or credentials.
- Treat live device behaviour as unverified until it has been tested on that
  exact recovery state. Offline tests do not prove a flash.

## Windows application

Use WinUI 3 controls and Windows App SDK resources before adding custom XAML.
Preserve system light and dark mode, system accent behaviour, keyboard access,
and high-contrast fallback. Use Fluent icons rather than emoji or arbitrary
Unicode symbols for actions. Follow [DESIGN.md](DESIGN.md) for UI work.

User-facing Windows strings use stable semantic keys in `locales/*/windows.json`.
Do not add hard-coded translated prose to XAML or C#; update the English source
and every catalog through the localization workflow.

## Validation

Run the narrowest relevant checks while changing code. Before a release-facing
change, run:

```console
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
pwsh -NoProfile -File tools/check-locales.ps1
cargo check --manifest-path fuzz/Cargo.toml --locked
pwsh -NoProfile -File tools/preflight.ps1
```

The Windows CI matrix is the authoritative ARM64 and XAML publish check. Do
not claim a real recovery or flash test without a connected device and a
recorded, non-sensitive result.

## Documentation and review

Keep `README.md`, relevant pages in `docs/`, and the Wiki source in `docs/wiki/`
consistent with product changes. Keep PRs focused, explain safety effects, and
resolve review feedback with a test or other concrete validation where possible.
See [LOCALIZATION.md](docs/LOCALIZATION.md) for translator context and catalog
rules.
