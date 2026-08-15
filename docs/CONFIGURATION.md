# Configuration

The complete precedence order is:

1. Rust defaults.
2. Platform user configuration.
3. Workspace `.pire/config.toml`.
4. Explicit `--config FILE`.
5. `PIRE_*` environment variables.
6. Sparse CLI overrides.

An explicitly named configuration file is required. User and workspace files are optional. Unknown fields are rejected.

Most settings should be file/environment-only. Add a CLI override only when a value is useful for one invocation. This keeps `CliOverrides` intentionally smaller than `AppConfig`.

Examples:

```bash
PIRE_PROVIDER__KIND=open-ai-compatible
PIRE_PROVIDER__MODEL=local-coder
PIRE_TOOLS__ALLOW_PROCESS=false
PIRE_SECURITY__APPROVAL_MODE=never
```
