# Configuration

## Precedence

Pire resolves values in this order:

```text
compiled defaults
  < user configuration
  < trusted project configuration
  < explicit --config file
  < PIRE_* environment variables
  < CLI flags
```

The final source has the highest precedence.

The user configuration location is platform-specific and can be inspected
with:

```bash
pire config paths
```

The project configuration is:

```text
WORKSPACE/.pire/config.toml
```

It is ignored until the canonical workspace is trusted.

## Environment variables

Use `_` after the prefix and `__` between nested fields:

```bash
PIRE_ROUTER__STRATEGY=cost
PIRE_ROUTER__PREFER_LOCAL=true
PIRE_AGENT__MAX_STEPS=32
PIRE_TOOLS__ALLOW_PROCESS=false
PIRE_LOGGING__LEVEL=debug
```

Arrays such as model profiles are best maintained in the user or explicit
TOML file rather than encoded as environment variables.

## CLI overrides

Only values useful for a single invocation are exposed as direct flags:

```bash
pire \
  --model local-coder \
  --provider llama.cpp \
  --route local-first \
  --max-steps 32 \
  --max-tool-calls 128 \
  --approval-mode prompt \
  --print \
  "review this repository"
```

CLI override structures are sparse `Option<T>` values. An omitted flag does
not overwrite a lower-priority source.

## Model profiles

Models are top-level array entries:

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
parallel_tools = false
vision = false
structured_output = true
reasoning = false
streaming = false
```

`id` is the stable Pire-facing name. `model` is the identifier sent to the
provider endpoint.

## Project-safe schema

A project file may contain only:

```text
[router]
[agent]
[tools]
[resources]
[ui]
```

It cannot declare models, endpoints, credential bindings, plugin paths,
session paths, learning paths, or global security mode.

Project tool booleans are combined with logical AND against global policy.
Project tool limits are clamped to the corresponding global limit.

## Validation

Pire rejects:

- no enabled models;
- empty or duplicate model IDs;
- HTTP models without a base URL;
- command models without a command vector;
- zero model, router, agent, tool, resource, UI, or plugin limits;
- unknown fields in all supported schemas.

An explicitly named configuration file is required to exist. Implicit user
and project files are optional.

## Templates

```text
config/user.toml  full user-scoped template
config/app.toml   safe project-scoped template
```
