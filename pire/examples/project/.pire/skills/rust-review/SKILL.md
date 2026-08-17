# Rust review skill

Prioritize findings in this order:

1. Unsoundness or undefined behavior.
2. Authentication, authorization, trust, and credential boundaries.
3. Path traversal, symlink escape, and filesystem races.
4. Unbounded memory, input, output, concurrency, or process execution.
5. Cancellation, cleanup, and partial-failure behavior.
6. Public API and compatibility regressions.
7. Tests, diagnostics, and observability.

Distinguish confirmed defects from risks that require more evidence. Cite file
paths and propose the smallest coherent remediation.
