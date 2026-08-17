# Plugin philosophy

Pire adopts the useful part of an "everything is a plugin" architecture:
capabilities are mounted, dependency-ordered, observable, replaceable, and
owned by the plugin that supplied them.

The kernel is intentionally not a general service locator. It has three jobs:

1. Validate and topologically mount plugins.
2. Reject duplicate capability identities.
3. Expose typed capability registries to the application.

## What is a plugin?

A plugin is a component with:

```text
stable ID
version
description
dependency IDs
bounded mount operation
one or more typed capabilities
```

A feature flag is not automatically a plugin. A global singleton is not a
plugin. A branch in the agent loop is not a plugin.

## Why use one registry?

Using one registry gives Pire a uniform answer to:

```text
Where did this tool come from?
Which plugin owns this slash command?
Which models are available?
Which execution backend is active?
What must mount before this capability?
What can RPC or an editor discover?
```

The same metadata powers `/plugins`, diagnostics, future RPC capability
negotiation, and audit/session events.

## Built-ins are plugins

Built-in Rust code does not receive a privileged alternative integration path.
The host execution backend, router, offline provider, coding tools, commands,
learning store, session store, and file resources all mount through `Kernel`.

This makes replacement practical. An `MvmExecutionPlugin` can register an
execution backend without changing `run_process`, provider sidecars, or the
agent loop.

## External plugins are processes

Third-party extensions are intentionally out of process. Loading a native Rust
dynamic library would couple Pire to unstable ABI details and grant the plugin
full in-process authority.

The `pire-plugin-v1` protocol gives an external executable a bounded way to
provide models, tools, and commands. A future protocol revision can add
resources, routers, session stores, or frontends through capability
negotiation rather than ad hoc environment conventions.

## What remains host-owned?

A few mechanisms must remain below plugins:

```text
plugin dependency resolution
capability identity enforcement
configuration/trust bootstrap
process entry and exit codes
protocol version negotiation
```

Those are the kernel's integrity boundary. Treating the integrity boundary
itself as an untrusted plugin would make dependency and security policy
circular.

## Design rule

When adding a feature, ask:

> Is this application policy, or is it a capability another implementation
> could replace?

Replaceable behavior belongs behind a core trait and registry entry. Bootstrap
integrity belongs in the host.
