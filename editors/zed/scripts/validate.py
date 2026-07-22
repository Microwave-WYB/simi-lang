#!/usr/bin/env python3
"""Static checks and optional Tree-sitter integration checks for the extension."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
import tomllib
from pathlib import Path
from urllib.parse import urlparse

COMPONENT = Path(__file__).resolve().parents[1]
LANGUAGE_FILES = (
    "config.toml",
    "highlights.scm",
    "brackets.scm",
    "indents.scm",
    "outline.scm",
    "textobjects.scm",
)
ALLOWED_CAPTURES = {
    "highlights.scm": {
        "boolean",
        "comment",
        "constant.builtin",
        "function",
        "keyword",
        "number",
        "operator",
        "property",
        "punctuation.bracket",
        "punctuation.delimiter",
        "string",
        "string.escape",
        "variable",
        "variable.parameter",
    },
    "brackets.scm": {"open", "close"},
    "indents.scm": {"indent", "end"},
    "outline.scm": {"context", "item", "name"},
    "textobjects.scm": {"comment.around", "function.around", "function.inside"},
}
CAPTURE = re.compile(r"@([A-Za-z0-9_.-]+)")
REVISION = re.compile(r"[0-9a-f]{40}(?:[0-9a-f]{24})?")


def check(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def load_toml(path: Path) -> dict:
    with path.open("rb") as source:
        return tomllib.load(source)


def check_source_extension() -> None:
    manifest = load_toml(COMPONENT / "extension.toml")
    check(manifest["id"] == "simi", "extension id must be simi")
    check(manifest["name"] == "Simi", "extension name must be Simi")
    check(manifest["schema_version"] == 1, "unsupported extension schema")
    check("grammars" not in manifest, "source manifest must remain machine-independent")

    language = COMPONENT / "languages" / "simi"
    for relative in LANGUAGE_FILES:
        check((language / relative).is_file(), f"missing language file: {relative}")

    config = load_toml(language / "config.toml")
    check(config["name"] == "Simi", "language name must be Simi")
    check(config["grammar"] == "simi", "language grammar must be simi")
    check("simi" in config["path_suffixes"], "missing .simi association")
    check("-- " in config["line_comments"], "missing Simi line comment")

    for query_name, allowed in ALLOWED_CAPTURES.items():
        text = (language / query_name).read_text(encoding="utf-8")
        captures = set(CAPTURE.findall(text))
        unexpected = sorted(captures - allowed)
        check(not unexpected, f"unsupported captures in {query_name}: {unexpected}")
        check(captures, f"query contains no captures: {query_name}")


def check_generated_extension(extension: Path) -> Path:
    manifest = load_toml(extension / "extension.toml")
    grammar = manifest.get("grammars", {}).get("simi")
    check(grammar is not None, "generated manifest has no grammars.simi entry")
    check(REVISION.fullmatch(grammar.get("rev", "")) is not None, "invalid grammar revision")

    parsed = urlparse(grammar.get("repository", ""))
    check(parsed.scheme in {"file", "https"}, "grammar URL must use file or https")
    check(parsed.username is None and parsed.password is None, "grammar URL contains credentials")
    check((extension / "languages" / "simi" / "config.toml").is_file(), "language not copied")

    if parsed.scheme == "file":
        grammar_path = Path(parsed.path)
        check(grammar_path.is_dir(), "local grammar repository is missing")
        head = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=grammar_path,
            check=True,
            text=True,
            stdout=subprocess.PIPE,
        ).stdout.strip()
        check(head == grammar["rev"], "manifest does not pin local grammar HEAD")
        return grammar_path
    return Path()


def run_tree_sitter_checks(extension: Path, grammar: Path) -> None:
    fixture = COMPONENT / "tests" / "fixtures" / "language.simi"
    subprocess.run(
        ["tree-sitter", "parse", "--quiet", str(fixture)], cwd=grammar, check=True
    )
    language = extension / "languages" / "simi"
    for query_name in LANGUAGE_FILES[1:]:
        subprocess.run(
            ["tree-sitter", "query", "--quiet", str(language / query_name), str(fixture)],
            cwd=grammar,
            check=True,
        )
    print("tree-sitter parse and query checks passed")


def main() -> None:
    arguments = argparse.ArgumentParser()
    arguments.add_argument("--extension", type=Path)
    arguments.add_argument("--tree-sitter", action="store_true")
    args = arguments.parse_args()

    try:
        check_source_extension()
        print("static extension checks passed")
        if args.extension is not None:
            grammar = check_generated_extension(args.extension.resolve())
            print("generated extension checks passed")
            if args.tree_sitter:
                check(bool(grammar), "Tree-sitter checks require a local file:// grammar")
                run_tree_sitter_checks(args.extension.resolve(), grammar)
        elif args.tree_sitter:
            check(False, "--tree-sitter requires --extension")
    except (AssertionError, KeyError, OSError, subprocess.CalledProcessError) as error:
        print(f"validation failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error


if __name__ == "__main__":
    main()
