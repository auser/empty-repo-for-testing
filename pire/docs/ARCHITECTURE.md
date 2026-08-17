# Architecture

Pire is organized around a deliberately small kernel and a set of mounted
capabilities.

```text
pire-cli
  ├── operational CLI and terminal editor
  ├── trust-aware configuration bootstrap
  ├── approval renderer
  └── application composition
        │
        ▼
pire-core
  ├── dependency-ordered plugin kernel
  ├── capability registry
  ├── provider-neutral agent loop
  ├── typed event stream
  ├── routing contracts
  ├── cancellation
  └── stable data contracts
        ▲
        │
pire-builtins
  ├── providers
  ├── coding tools
  ├── slash commands
  ├── router
  ├── aggregate learning
  ├── sessions
  ├── resources
  ├── execution
  └── sidecar adapters
```

The dependency direction is one-way:

```text
pire-cli ───────────────► pire-core
    │
    └────► pire-builtins ─► pire-core
```

`pire-core` contains no Clap, terminal, file-configuration, HTTP, or concrete
model-vendor dependency.

## Kernel

A plugin supplies metadata and mounts capabilities:

```rust
pub trait Plugin: Send {
    fn metadata(&self) -> PluginMetadata;
    fn mount(&mut self, registry: &mut Registry) -> Result<(), String>;
}
```

Metadata includes an ID, version, description, and dependencies. `Kernel`
performs deterministic topological mounting and rejects duplicate IDs,
unresolved dependencies, and duplicate capability registrations.

The kernel itself is not a plugin. It is the composition root that gives
plugins a bounded place to register capabilities.

## Capability registry

The registry currently supports:

```text
Provider
Tool
SlashCommand
Router
LearningStore
SessionStore
ResourceResolver
ExecutionBackend
```

Each capability records its source plugin. `/plugins` and the `plugins`
management command expose the mounted graph for diagnostics.

## Agent loop

The provider-neutral loop performs:

```text
classify task
  ↓
collect provider/model descriptors
  ↓
apply hard eligibility and route scoring
  ↓
call selected provider
  ├── final answer ─────────────► finish
  └── tool calls
        ↓
      resolve registered tools
        ↓
      enforce workspace, trust, and approval policy
        ↓
      execute with limits
        ↓
      append typed results
        └───────────────────────► next provider step
```

Fallback is allowed only for retryable provider failures and only before a
mutating tool side effect. This prevents another model from unknowingly
repeating or conflicting with a successful write or process operation.

## Events

The agent emits typed events for routing, provider starts, text, tools, usage,
warnings, and turn completion. The terminal renderer, JSON Lines renderer,
and session recorder are independent event sinks.

This lets future ACP/RPC/editor frontends consume the same execution stream
without moving terminal concerns into the agent core.

## Configuration bootstrap

Project trust is resolved before project configuration, prompts, skills, or
sidecar manifests are loaded. The project schema excludes providers,
credential bindings, executable plugin directories, and global storage.

Project tool permissions and limits merge monotonically: a project can reduce
capabilities or budgets, but cannot expand the user-global policy.

## Extension boundaries

Third-party native dynamic libraries are intentionally not loaded. Rust does
not provide a stable general-purpose plugin ABI, and an in-process native
plugin would immediately inherit all Pire memory and process authority.

External extensions use a versioned, bounded stdin/stdout protocol. A future
Wasm plugin host or MVM-backed sidecar can implement the same core capability
contracts without changing the agent loop.
