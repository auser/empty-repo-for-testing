# Local models

Pire does not require Ollama and does not embed model weights in its base
binary.

## OpenAI-compatible endpoint

Use any local server that implements `/v1/chat/completions`:

```toml
[[models]]
id = "local-coder"
kind = "open-ai-compatible"
provider_name = "llama.cpp"
model = "qwen-coder"
base_url = "http://127.0.0.1:8080/v1"
enabled = true
local = true
priority = 30
tags = ["general", "coding", "review", "debugging", "tools"]
input_cost_per_million = 0.0
output_cost_per_million = 0.0
context_window = 131072
timeout_secs = 300
max_response_bytes = 8388608

[models.capabilities]
tools = true
structured_output = true
```

Loopback HTTP is allowed. Non-loopback remote endpoints require HTTPS.

Suitable servers include llama.cpp, mistral.rs, vLLM, LM Studio, and other
compatible runtimes. Pire does not start or manage those servers in v0.2.

## Command provider

The command provider starts an adapter for each request:

```toml
[[models]]
id = "command-local"
kind = "command"
provider_name = "local-runtime"
model = "local-coder"
command = [
  "/absolute/path/to/pire-local-adapter",
  "--model",
  "/models/coder.gguf"
]
enabled = true
local = true
priority = 20
tags = ["general", "coding", "tools"]
input_cost_per_million = 0.0
output_cost_per_million = 0.0
context_window = 32768
timeout_secs = 300
max_response_bytes = 8388608

[models.capabilities]
tools = true
```

The adapter receives:

```json
{
  "protocol": "pire-provider-v1",
  "request": {
    "model": "local-coder",
    "messages": [],
    "tools": []
  }
}
```

It writes a `ProviderResponse` to stdout:

```json
{
  "text": "response text",
  "tool_calls": [],
  "usage": {
    "input_tokens": 100,
    "output_tokens": 25
  }
}
```

Diagnostics go to stderr. The command is bounded by timeout and output bytes.

## Provider sidecar

A `pire-plugin-v1` sidecar may expose multiple local models in one manifest.
This is the preferred extension boundary for a model manager, first-run weight
downloader, or a persistent Rust-native inference service.

## Future local model manager

A separate optional `pire-modeld` should own:

```text
model discovery
download with resume
checksum verification
license display
disk quota
RAM/VRAM fit checks
load/unload
warmup
health checks
progress and cancellation
```

Keeping model lifecycle in an optional sidecar preserves a small base binary
and avoids forcing one inference runtime on every Pire installation.
