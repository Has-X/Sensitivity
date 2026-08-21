# Contributing to Sensitivity

Start with the [Wiki source](docs/wiki/Home.md), the [README](README.md), and the relevant architecture notes under `docs/`.

Before opening a pull request, run the checks that match your change:

```console
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo check --manifest-path fuzz/Cargo.toml --locked
```

Keep pull requests focused. Do not include device serial numbers, validation tokens, private ROM URLs, or raw recovery captures. Changes that affect flashing, wiping, USB ownership, validation, or release packaging need a clear safety note and regression coverage where practical.

For user-facing wording and translation work, follow the [English source and translator guide](docs/LOCALIZATION.md). It defines semantic keys, placeholders, and safety-critical terms across the Windows app, installer, portable GUI, and CLI.
