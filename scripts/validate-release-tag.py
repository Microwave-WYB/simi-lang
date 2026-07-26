#!/usr/bin/env python3
"""Validate an immutable Simi prerelease tag."""

from __future__ import annotations

import re
import sys


TAG_PATTERN = re.compile(
    r"^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
    r"-(alpha|beta|rc)\.(0|[1-9][0-9]*)$"
)


def validate_tag(tag: str) -> None:
    """Raise ValueError unless *tag* is an immutable prerelease tag."""
    if TAG_PATTERN.fullmatch(tag) is None:
        raise ValueError(
            f"invalid release tag {tag!r}; expected vMAJOR.MINOR.PATCH-"
            "(alpha|beta|rc).N"
        )


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print(f"usage: {argv[0]} TAG", file=sys.stderr)
        return 2
    try:
        validate_tag(argv[1])
    except ValueError as error:
        print(error, file=sys.stderr)
        return 1
    print(argv[1])
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
