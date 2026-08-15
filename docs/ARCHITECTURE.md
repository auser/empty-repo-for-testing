# Architecture

Pire separates product policy from execution primitives.

```text
pire-cli ───────► pire-core
    │
    └───────────► pire-providers ─────► pire-core
```

`pire-core` does not read environment variables, parse command-line flags, initialize logging, or know about a specific model vendor. It owns the provider-neutral agent loop, workspace boundary, approval contract, tool registry, events, and append-only sessions.

`pire-providers` adapts external inference systems to the core `Provider` trait. Local inference is treated as a first-class provider, not a special branch in the agent loop.

`pire-cli` composes configuration, trust, approvals, resources, providers, tools, sessions, and terminal rendering into an application.

## Agent loop

```text
input
  → provider completion
    → final text: finish
    → tool calls: approve, execute, append results, repeat
```

Every run has hard step and tool-call limits. Tool failures are returned to the model as structured tool results; provider, observer, and limit failures stop the run.
