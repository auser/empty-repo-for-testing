# Security model

- All workspace paths are canonicalized and checked against the canonical root.
- Parent traversal and absolute paths are rejected.
- Symlink escapes are rejected for existing paths and write ancestors.
- File reads, writes, resource loading, process output, provider output, and stdin are bounded.
- Write and process tools require project trust and approval.
- Non-interactive prompt approvals fail closed.
- Sessions are append-only JSONL records.
- Provider credentials are read from named environment variables and are not stored in configuration.
- Third-party local model adapters can run out of process instead of sharing Pire's address space.
