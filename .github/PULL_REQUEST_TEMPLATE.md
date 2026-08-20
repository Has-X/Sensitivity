## What changed

<!-- Keep this summary focused on user-visible or security-relevant behavior. -->

## Validation

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo test --workspace --locked`
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings`
- [ ] `cargo check --manifest-path fuzz/Cargo.toml --locked` when fuzz or dependency files changed
- [ ] Release packaging checked when packaging or workflow files changed

## Safety checklist

- [ ] No serial numbers, validation tokens, private ROM URLs, or raw device captures are included.
- [ ] Any wipe, USB claim, ADB, or recovery behavior change is explicitly documented.
- [ ] Documentation and issue references are updated where needed.
