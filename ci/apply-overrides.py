from __future__ import annotations

import pathlib
import sys


def replace_once(path: pathlib.Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(
            f"expected exactly one match in {path} but found {count}: {old!r}"
        )
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: apply-overrides.py PROJECT_ROOT")

    root = pathlib.Path(sys.argv[1])
    mutation = root / "crates/pire-core/src/mutation.rs"
    workspace = root / "crates/pire-core/src/workspace.rs"

    replace_once(
        mutation,
        "use std::{collections::BTreeMap, path::PathBuf, sync::Mutex};",
        "use std::{\n"
        "    collections::BTreeMap,\n"
        "    path::{Path, PathBuf},\n"
        "    sync::Mutex,\n"
        "};",
    )
    replace_once(
        mutation,
        "fn read_optional(&self, path: &PathBuf)",
        "fn read_optional(&self, path: &Path)",
    )
    replace_once(
        mutation,
        "    path: &PathBuf,\n    old: Option<&[u8]>,",
        "    path: &Path,\n    old: Option<&[u8]>,",
    )
    replace_once(
        mutation,
        "        _ => Err(MutationError::Stale(path.clone())),",
        "        _ => Err(MutationError::Stale(path.to_path_buf())),",
    )
    replace_once(
        workspace,
        "fs::rename(&temporary, &destination).or_else(|error| {\n"
        "            let _ = fs::remove_file(&temporary);\n"
        "            Err(error)\n"
        "        })?;",
        "fs::rename(&temporary, &destination).inspect_err(|_| {\n"
        "            let _ = fs::remove_file(&temporary);\n"
        "        })?;",
    )


if __name__ == "__main__":
    main()
