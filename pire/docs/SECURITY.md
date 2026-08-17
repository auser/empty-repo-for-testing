# Security model

Pire operates on source code and may invoke tools requested by an untrusted
model. Its security boundaries are explicit and layered.

## Project trust

The canonical workspace path is checked against a user-global trust store
before Pire loads:

- `.pire/config.toml`;
- project prompts;
- project skills;
- project sidecar manifests;
- project instruction files.

A project may be trusted persistently:

```bash
pire --workspace . trust grant
```

or for one invocation:

```bash
pire --trust-project --print "review this repository"
```

Trust means the user permits Pire to load project-provided resources. It does
not make model output safe and is not a process sandbox.

## Configuration scopes

Global or explicit configuration may declare:

```text
providers and endpoints
credential environment-variable names
external plugin directories
session and learning storage paths
```

Project configuration cannot declare those fields. It may only tune a safe
subset of router, agent, resource, UI, and tool settings.

Project tool permissions and limits merge monotonically. A project cannot
turn on a globally disabled write/process capability or expand a global byte,
time, or entry budget.

## Operations and approvals

Tools declare one operation class:

```text
Read
Write
Process
```

Reads are permitted when the read plugin is enabled. Write and process
operations require a trusted project and then follow `approval_mode`:

```text
prompt
always
never
```

In non-interactive mode, `prompt` denies unless `--yes` is supplied.

## Filesystem

- The workspace is canonicalized.
- Existing paths are canonicalized before access.
- Symlink escape outside the workspace is rejected.
- New paths require a canonical parent within the workspace.
- Reads and writes have byte limits.
- Edits require a content hash and reject stale files.
- Writes use a temporary file, flush it, and replace the destination.
- Multi-file patches validate all paths and preconditions before applying.

The current multi-file writer validates the full patch before starting but
cannot guarantee rollback after an operating-system failure midway through
several independent file replacements. A future mutation journal will provide
strict cross-file rollback.

## Process execution

The host execution backend:

- never invokes a shell for agent tool calls;
- accepts a program and explicit argument vector;
- supports environment clearing;
- preserves only basic runtime variables after clearing;
- captures stdout/stderr in bounded temporary files;
- enforces timeout and aggregate output limits;
- observes cancellation;
- reports exit status.

The host backend does not yet guarantee descendant-process-tree termination
on every operating system and does not provide network, CPU, memory, or
filesystem isolation. The `ExecutionBackend` trait exists so MVM can become a
stronger replacement without changing tools, sidecars, or the agent loop.

## Providers and credentials

- Credentials are looked up by environment-variable name.
- Literal API keys do not belong in configuration.
- Non-loopback remote HTTP endpoints require HTTPS.
- HTTP redirects are disabled.
- Provider response bodies are bounded.
- Authentication, rate limiting, availability, request, and malformed-response
  errors are classified separately.
- Routing fallback stops after a mutating tool side effect.

## External plugins

Pire does not load third-party Rust dynamic libraries. External plugins run as
separate processes using `pire-plugin-v1` and are limited by message size,
time, operation approval, and project trust.

A sidecar is still native code running as the user. Use a future MVM execution
backend for untrusted or unattended plugins.

## Learning privacy

Aggregate routing learning stores operational metrics only. It deliberately
excludes prompt, response, source, tool, and secret content.

## Reporting vulnerabilities

Do not open a public issue containing an unpatched credential exposure or
sandbox escape. Add a private security-reporting channel before public beta
and publish its address in `SECURITY.md` at the repository root.
