# Security policy

Pire is pre-beta software. Security-sensitive interfaces—including sidecar
plugins, host execution, trust, and provider credential handling—may evolve
before the first stable protocol release.

## Reporting a vulnerability

Do not publish an unpatched credential exposure, workspace escape, arbitrary
code-execution flaw, or protocol-authentication weakness in a public issue.

Until a dedicated private reporting address is configured for the eventual
Pire repository, contact the repository owner privately and include:

- affected version or commit;
- operating system;
- minimal reproduction;
- impact;
- whether credentials or private source may have been exposed;
- suggested mitigation when known.

## Supported versions

Only the newest tagged pre-release and the current default branch are expected
to receive security fixes before 1.0.

## Current security boundaries

Project trust controls whether project configuration, instructions, skills,
prompts, and sidecars are loaded. It is not a sandbox.

The host execution backend is bounded by time and output, but it executes with
the user's operating-system authority. Use an isolated machine, container, or
future MVM execution backend for untrusted and unattended workloads.

External plugins run out of process but remain native executables. Install
only plugins whose source and distribution you trust.
