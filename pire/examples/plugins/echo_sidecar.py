#!/usr/bin/env python3
"""Minimal pire-plugin-v1 sidecar using only Python's standard library."""

from __future__ import annotations

import json
import sys
from typing import Any

PROTOCOL = "pire-plugin-v1"


def respond(result: dict[str, Any]) -> None:
    json.dump({"protocol": PROTOCOL, "result": result}, sys.stdout)
    sys.stdout.write("\n")


def main() -> int:
    try:
        request = json.load(sys.stdin)
        if request.get("protocol") != PROTOCOL:
            raise ValueError("unsupported protocol")
        capability = request.get("capability") or {}
        kind = capability.get("kind")
        identifier = capability.get("id")
        payload = request.get("request") or {}

        if kind == "slash_command" and identifier == "echo":
            respond({"message": str(payload.get("arguments", ""))})
            return 0

        if kind == "tool" and identifier == "word_count":
            arguments = payload.get("arguments") or {}
            text = arguments.get("text")
            if not isinstance(text, str):
                raise ValueError("word_count requires a string `text` argument")
            count = len(text.split())
            respond(
                {
                    "content": f"{count} word(s)",
                    "data": {"words": count},
                    "side_effect": False,
                }
            )
            return 0

        raise ValueError(f"unsupported capability: {kind}/{identifier}")
    except Exception as error:  # protocol boundary: return diagnostics on stderr
        print(f"echo sidecar error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
