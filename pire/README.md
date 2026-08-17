# Pire

Pire is a small, plugin-first Rust coding-agent harness inspired by the
terminal ergonomics of Pi, the resource-oriented capabilities of OMP, and the
"everything is a plugin" composition model of DeepSeek Harness.

The kernel knows only how to mount dependency-ordered plugins and expose their
capabilities. Providers, tools, slash commands, routers, learning stores,
sessions, resources, and execution backends all use the same registry.

## Quick start

```bash
cargo build --release -p pire-cli
./target/release/pire
```

Inside the interactive shell:

```text
/help
/models
/model auto
/route local-first
/plugins
/status
```

One-shot use:

```bash
./target/release/pire --print "Explain this repository"
git diff | ./target/release/pire --print "Review this patch"
./target/release/pire --print "Review this file" @src/main.rs
```

The deterministic offline provider is enabled by default, so the executable
can be exercised without credentials or a running model server.

## Configuration

Pire resolves configuration in this order:

```text
compiled defaults
  < user configuration
  < trusted workspace .pire/config.toml
  < explicit --config FILE
  < PIRE_* environment variables
  < CLI flags
```

Nested environment keys use `__`:

```bash
PIRE_ROUTER__STRATEGY=cost
PIRE_AGENT__MAX_STEPS=32
PIRE_TOOLS__ALLOW_PROCESS=false
```

Copy the annotated template into a project:

```bash
mkdir -p .pire
cp config/app.toml .pire/config.toml
```

Project configuration is ignored until the canonical workspace is trusted.
Project files may tighten tool policy but cannot define providers,
credentials, executable plugins, or global storage locations.

## Local models without Ollama

Pire supports local models through either:

1. An OpenAI-compatible loopback endpoint, including llama.cpp, mistral.rs,
   vLLM, or LM Studio.
2. A bounded JSON command provider that starts an adapter on demand.
3. An external `pire-plugin-v1` sidecar that provides one or more models.

Model weights are never embedded in the base executable.

## Plugin kinds

A mounted plugin may provide any combination of:

- model providers;
- coding tools;
- slash commands;
- model routers;
- aggregate learning stores;
- append-only session stores;
- resource URI resolvers;
- execution backends.

Built-ins implement the same `Plugin` trait as externally described sidecars.
Third-party native libraries are not loaded into the Pire process because
Rust does not provide a stable general-purpose plugin ABI. External plugins
use a versioned, bounded JSON protocol over stdin/stdout.

## Safety model

- Workspace paths are canonicalized and symlink escape is rejected.
- Project configuration and executable project resources require trust.
- Reads, writes, and processes are distinct operations.
- Writes support content-hash preconditions and atomic replacement.
- Provider fallback stops after a mutating tool side effect.
- Remote HTTP endpoints require HTTPS unless they are loopback addresses.
- HTTP redirects are disabled so authorization cannot cross origins.
- Aggregate learning stores no prompts, responses, file contents, tool
  arguments, tool output, or credentials.
- Process execution is bounded by timeout and output limits.

Project trust is not a sandbox. The execution backend boundary is designed so
an MVM-backed implementation can replace host execution for stronger
isolation.

## Verify

```bash
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --all-targets
cargo build --locked --release -p pire-cli
```

See `docs/ARCHITECTURE.md`, `docs/PLUGINS.md`, `docs/ROUTING.md`, and
`docs/SECURITY.md`.
