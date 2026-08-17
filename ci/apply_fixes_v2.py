from __future__ import annotations

import sys
from pathlib import Path

TARGET = Path(__file__).resolve().parents[1] / "work"


def replace(path: str, old: str, new: str, *, count: int = 1) -> None:
    target = TARGET / path
    text = target.read_text(encoding="utf-8")
    found = text.count(old)
    if found < count:
        raise RuntimeError(
            f"expected at least {count} occurrence(s) in {path}, found {found}: {old!r}"
        )
    target.write_text(text.replace(old, new, count), encoding="utf-8")


def main() -> None:
    replace(
        "crates/pire-builtins/src/storage.rs",
        '''    fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
    fs::rename(&temporary, path).map_err(|error| {''',
        '''    fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
    #[cfg(windows)]
    if path.exists() {
        fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    fs::rename(&temporary, path).map_err(|error| {''',
    )
    replace(
        "crates/pire-cli/src/interactive.rs",
        "line.trim_end_matches(['\\r', '\\n']).to_owned()",
        "line.trim_end_matches(|character| character == '\\r' || character == '\\n').to_owned()",
    )

    # Project limits may only tighten global limits, just like capability
    # booleans. This prevents a repository from expanding global resource
    # budgets after trust is granted.
    replace(
        "crates/pire-cli/src/config.rs",
        '''        tools.allow_process = tools
            .allow_process
            .map(|value| value && global.tools.allow_process);
    }
    LayeredConfig::try_from(&project).map(Some)''',
        '''        tools.allow_process = tools
            .allow_process
            .map(|value| value && global.tools.allow_process);
        tools.max_read_bytes = tools
            .max_read_bytes
            .map(|value| value.min(global.tools.max_read_bytes));
        tools.max_write_bytes = tools
            .max_write_bytes
            .map(|value| value.min(global.tools.max_write_bytes));
        tools.max_process_output_bytes = tools
            .max_process_output_bytes
            .map(|value| value.min(global.tools.max_process_output_bytes));
        tools.process_timeout_secs = tools
            .process_timeout_secs
            .map(|value| value.min(global.tools.process_timeout_secs));
        tools.max_list_entries = tools
            .max_list_entries
            .map(|value| value.min(global.tools.max_list_entries));
    }
    LayeredConfig::try_from(&project).map(Some)''',
    )

    print("Applied second-stage portability and policy fixes")


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(f"apply_fixes_v2.py failed: {error}", file=sys.stderr)
        raise
