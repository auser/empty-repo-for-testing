# Pire

Pire is a clean Rust coding-agent harness inspired by the small-core, extensible approach of Pi and the modular Rust architecture of Corvus.

## Product goals

- One provider-neutral agent loop.
- Safe, bounded coding tools.
- Append-only resumable sessions.
- Project instructions and skills.
- Cloud models and local models behind the same interface.
- No mandatory model daemon.
- Predictable configuration precedence.
- A reusable Rust core rather than a CLI-shaped monolith.

## Workspace

```text
crates/
├── pire-core       # agent loop, tools, workspace, approvals, sessions
├── pire-providers  # offline, OpenAI-compatible, and command adapters
└── pire-cli        # configuration, terminal UX, trust, built-in tools
```

## Configuration precedence

```text
Rust defaults
  < user config
  < .pire/config.toml
  < --config FILE
  < PIRE_* environment variables
  < CLI flags
```

Nested environment keys use `__`:

```bash
PIRE_PROVIDER__KIND=open-ai-compatible
PIRE_PROVIDER__MODEL=qwen3-coder
PIRE_PROVIDER__BASE_URL=http://127.0.0.1:8080/v1
PIRE_AGENT__MAX_STEPS=24
```

## Local models

An OpenAI-compatible local server:

```bash
pire \
  --provider open-ai-compatible \
  --model qwen3-coder \
  --base-url http://127.0.0.1:8080/v1 \
  --print \
  review src/main.rs
```

An out-of-process local adapter:

```bash
pire \
  --provider command \
  --command 'my-local-model-adapter --json' \
  --model local-model \
  --print \
  explain this repository
```

The command adapter receives one `CompletionRequest` JSON object on stdin and must return one `ProviderResponse` JSON object on stdout. This supports llama.cpp wrappers, Candle/Burn-based runners, and other local inference engines without linking them into Pire's trusted process.

## Offline development

```bash
cargo run -p pire-cli -- --print hello
cargo run -p pire-cli -- --print 'tool:list_files {"path":"."}'
```

## Verification

```bash
cargo generate-lockfile
just ci
```
