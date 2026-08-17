# Developing Pire

## Toolchain

The workspace pins Rust 1.97.1:

```bash
rustup show
```

## Local gate

```bash
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --all-targets
cargo build --locked --release -p pire-cli
```

Or:

```bash
just ci
```

## Crate boundaries

- Add provider-neutral contracts to `pire-core`.
- Add concrete built-ins and protocol adapters to `pire-builtins`.
- Keep Clap, terminal rendering, trust bootstrap, and file configuration in
  `pire-cli`.
- Do not add vendor SDKs to `pire-core`.
- Do not add a direct feature branch in the agent loop when a registered
  capability can represent the behavior.

## Adding a built-in plugin

1. Define or reuse a core capability trait.
2. Implement `Plugin` and register the capability in `mount`.
3. Declare dependency plugin IDs in `PluginMetadata`.
4. Add capability-level tests.
5. Wire selection into CLI bootstrap from resolved configuration.
6. Confirm `/plugins` reports the source plugin.

## Adding a sidecar capability

1. Define the manifest under `pire-plugin-v1`.
2. Keep stdout protocol-only; use stderr for diagnostics.
3. Enforce bounded input/output and timeout.
4. Declare the correct operation class for tools.
5. Validate the manifest with `pire plugin validate`.
6. Add an end-to-end fixture.

## Dependency policy

A dependency should have a compiled use site when introduced. Keep optional
heavy capabilities in sidecars or feature-gated boundary crates.

The release binary is checked against a 25 MiB upper bound in CI. The goal is
to remain substantially below that ceiling; exceeding it requires an
architecture decision explaining the user-visible benefit and alternatives.

## Compatibility

Before changing any serialized or subprocess contract, add a version field,
fixture tests, and migration or rejection behavior. Never silently reinterpret
an older protocol message.
