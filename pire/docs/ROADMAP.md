# Roadmap

## Implemented in v0.2

- Dependency-aware plugin kernel and capability registry.
- Built-in and out-of-process plugins.
- Offline, OpenAI-compatible, and JSON command providers.
- Local/cloud model metadata and deterministic task routing.
- Balanced, cost, latency, quality, and local-first strategies.
- Bounded provider fallback with side-effect stickiness.
- Aggregate routing learning and explicit feedback.
- Read, list, search, write, hash-anchored edit, multi-file patch, and process
  tools.
- Workspace confinement, project trust, and operation approval.
- Append-only JSONL sessions with resume, rename, and fork.
- File resource URIs.
- Dynamic prompt and skill slash commands.
- External sidecar model/tool/command protocol.
- Lightweight Crossterm editor, history, paste, slash completion, and @file
  completion.
- One-shot, terminal-event, JSON-event, and management CLI surfaces.
- Layered trust-aware configuration.

## Protocol-ready, not yet built in

The current contracts intentionally leave clean plugin boundaries for:

- MVM execution backend;
- LSP code-intelligence plugin;
- DAP debugging plugin;
- ACP/editor frontend;
- versioned long-running RPC server;
- local model download/load manager;
- GitHub issue and pull-request resource resolvers;
- browser automation sidecar;
- Python and JavaScript persistent-kernel sidecars;
- advisor and isolated subagent plugins;
- Wasm extension host;
- strict cross-file mutation rollback journal.

These are not claimed as implemented merely because an extension point exists.
Each should ship as a vertical slice with protocol fixtures, cancellation,
limits, trust behavior, and cross-platform tests.

## Next milestone

1. Convert provider output to a true incremental stream with cancellation.
2. Add protocol handshake/capability negotiation to provider commands and
   sidecars.
3. Add strict mutation journaling and `/undo`.
4. Add tree-linked session entries and safe model-assisted compaction.
5. Add `MvmExecutionPlugin` and make it selectable per project/profile.
6. Add LSP diagnostics, formatting, rename, references, and code actions.
7. Add typed read-only advisor and review-worker plugins.
8. Add ACP and stable RPC protocol crates.

## Non-goals for the base executable

- Embedded model weights.
- Embedded browser runtime.
- Embedded Python or JavaScript runtime.
- Native third-party dynamic libraries.
- Autonomous rewriting of Pire's own binary, trust policy, or credentials.
