# Plugins

Pire uses one composition model for built-in and external capabilities.

## Built-in plugins

Built-ins are ordinary Rust values implementing `pire_core::Plugin`. The CLI
selects and mounts them during bootstrap according to the resolved
configuration and trust state.

Current built-ins include:

```text
pire.execution.host
pire.router.cost-aware
pire.learning.aggregate-json
pire.sessions.jsonl
pire.commands.builtin
pire.resources.file
pire.tools.builtin
pire.provider.<model-id>
pire.command.prompt.<prompt-id>
```

Prompt and skill Markdown files are adapted into slash-command plugins rather
than interpreted by a separate command path.

## External sidecars

External plugins use the `pire-plugin-v1` protocol. A manifest may declare
multiple providers, tools, and slash commands.

Example manifest:

```json
{
  "protocol": "pire-plugin-v1",
  "id": "example.echo",
  "version": "1.0.0",
  "description": "Example out-of-process plugin",
  "dependencies": [],
  "command": ["python3", "examples/plugins/echo_sidecar.py"],
  "timeout_secs": 30,
  "max_message_bytes": 1048576,
  "providers": [],
  "tools": [],
  "commands": [
    {
      "name": "echo",
      "description": "Echo slash-command arguments"
    }
  ]
}
```

Every invocation writes one JSON object to stdin:

```json
{
  "protocol": "pire-plugin-v1",
  "plugin_id": "example.echo",
  "capability": {
    "kind": "slash_command",
    "id": "echo"
  },
  "request": {
    "arguments": "hello"
  }
}
```

The sidecar writes exactly one response object to stdout:

```json
{
  "protocol": "pire-plugin-v1",
  "result": {
    "message": "hello"
  }
}
```

Diagnostics belong on stderr. Stdout is reserved for the protocol response.

## Discovery

Pire searches these external-plugin locations:

1. User-global plugin directory adjacent to the user configuration.
2. Additional global directories from `[plugins].directories`.
3. Trusted project `.pire/plugins/*.json`.

Untrusted project sidecars are never loaded.

Validate a manifest without running it:

```bash
pire plugin validate .pire/plugins/example.json
```

## Provider sidecars

A provider capability receives a serialized `CompletionRequest` in the
`request` field and returns a `ProviderResponse` as its result. The provider
descriptor in the manifest declares model identity, cost metadata, context
window, locality, and capabilities.

## Tool sidecars

A tool manifest declares:

- name;
- description;
- JSON input schema;
- operation class: read, write, or process.

Pire evaluates trust and approval before invoking an external tool. The
sidecar receives its arguments and canonical workspace path. Requests and
responses are bounded by the effective global plugin byte limit and timeout.

## Security properties

- No native library is dynamically linked into the Pire process.
- Project sidecars require project trust.
- Sidecars receive a minimal preserved runtime environment when environment
  clearing is enabled.
- Protocol messages have hard byte limits.
- Processes have hard timeouts.
- Exit status and malformed JSON are surfaced as typed failures.
- Sidecar tools remain subject to operation approval.

Project trust is still not a process sandbox. Deploy sidecars through an MVM
execution backend for workloads that need stronger isolation.

## Adding another capability kind

A new first-class capability generally requires:

1. A provider-neutral trait and data contract in `pire-core`.
2. A registry map and registration method.
3. A built-in or sidecar adapter in `pire-builtins`.
4. Discovery/bootstrap wiring in `pire-cli`.
5. Typed events and tests.

Do not add a one-off global singleton or direct call from the agent loop when
the behavior can be expressed as a registered capability.
