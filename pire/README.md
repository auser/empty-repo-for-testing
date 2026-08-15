# Pire

A production-oriented Rust foundation for Pire.

## Configuration precedence

```text
Rust defaults < config/app.toml < PIRE_* environment variables < CLI overrides
```

Nested environment keys use `__`, for example:

```bash
PIRE_HTTP_CLIENT_CONFIG__MAX_BUFFERED_BYTES=524288
PIRE_LOGGING__LEVEL=debug
```

Only frequently changed values have CLI overrides. Most new configuration
fields are added once to the appropriate resolved section in `src/config.rs`
and then work automatically from TOML and environment variables.

## Verification

```bash
just ci
```

## Examples

```bash
cargo run -- --version
cargo run -- doctor
printf 'context\n' | cargo run -- summarize this
```
