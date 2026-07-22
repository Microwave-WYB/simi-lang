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

    increase = re.compile(config["increase_indent_pattern"])
    decrease = re.compile(config["decrease_indent_pattern"])
    for line in ("case value of", "[head, ..tail] when ready do", "try"):
        check(increase.search(line) is not None, f"line should increase indentation: {line}")
    check(increase.search("_ do value end") is None, "one-line clause must not indent next line")
    for line in ("end", "catch", "elseif ready then", "else"):
        check(decrease.search(line) is not None, f"line should decrease indentation: {line}")
    for legacy in ("match value with", "case value ->"):
        check(increase.search(legacy) is None, f"legacy syntax affects indentation: {legacy}")
        check(decrease.search(legacy) is None, f"legacy syntax affects indentation: {legacy}")

    highlights = (language / "highlights.scm").read_text(encoding="utf-8")
    for keyword in ('"case"', '"of"', '"when"'):
        check(keyword in highlights, f"missing highlight keyword: {keyword}")
    for removed in ('"match"', '"with"', '"->"'):
        check(removed not in highlights, f"legacy highlight token remains: {removed}")

    indents = (language / "indents.scm").read_text(encoding="utf-8")
    check("(case_expression" in indents, "case_expression is not indented")
    check("(pattern_clause" in indents, "pattern_clause is not indented")
    for removed_node in ("match_expression", "case_clause"):
        check(removed_node not in indents, f"legacy indent node remains: {removed_node}")

    fixture = (COMPONENT / "tests" / "fixtures" / "language.simi").read_text(encoding="utf-8")
    check("case value of" in fixture, "fixture does not exercise case-of syntax")
    check("_ do nil end" in fixture, "fixture does not exercise one-line clauses")
    for removed in ("match ", " with\n", " ->"):
        check(removed not in fixture, f"fixture contains legacy syntax: {removed.strip()}")

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
    highlight_result = subprocess.run(
        ["tree-sitter", "query", str(language / "highlights.scm"), str(fixture)],
        cwd=grammar,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    )
    highlight_captures = set(
        re.findall(r"capture: \d+ - ([A-Za-z0-9_.-]+),", highlight_result.stdout)
    )
    required_highlights = {
        "comment",
        "function",
        "keyword",
        "operator",
        "property",
        "string",
        "variable",
    }
    missing_highlights = sorted(required_highlights - highlight_captures)
    check(not missing_highlights, f"fixture is missing semantic highlights: {missing_highlights}")

    for query_name in LANGUAGE_FILES[2:]:
        subprocess.run(
            ["tree-sitter", "query", "--quiet", str(language / query_name), str(fixture)],
            cwd=grammar,
            check=True,
        )
    print("tree-sitter parse, semantic highlight, and query checks passed")


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
