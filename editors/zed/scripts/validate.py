#!/usr/bin/env python3
"""Static checks and optional Tree-sitter integration checks for the extension."""

from __future__ import annotations

import argparse
import json
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
        "type",
        "type.definition",
        "type.parameter",
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
SERVER_ID = "simi-lsp"


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
    check(manifest.get("snippets") == ["./snippets/simi.json"], "invalid snippets manifest")
    language_server = manifest.get("language_servers", {}).get(SERVER_ID)
    check(language_server is not None, "source manifest must declare simi-lsp")
    check(language_server.get("name") == "Simi Language Server", "invalid server name")
    check(language_server.get("languages") == ["Simi"], "simi-lsp must serve Simi")
    check("capabilities" not in manifest, "PATH-only language server needs no process capability")

    cargo = load_toml(COMPONENT / "Cargo.toml")
    check(cargo["lib"]["crate-type"] == ["cdylib"], "Zed extension must be a cdylib")
    check(
        cargo["dependencies"].get("zed_extension_api") == "=0.7.0",
        "Zed extension API must pin published version 0.7.0",
    )
    rust_source = (COMPONENT / "src" / "lib.rs").read_text(encoding="utf-8")
    check('worktree.which("simi")' in rust_source, "server must resolve from worktree PATH")
    check("worktree.shell_env()" in rust_source, "server must inherit worktree shell environment")
    check('args: vec!["lsp".to_owned()]' in rust_source, "server must use the lsp subcommand")
    check("target/debug" not in rust_source, "extension must not assume a Cargo target path")
    check(
        "simi was not found on the worktree PATH" in rust_source,
        "missing-server diagnostic must explain PATH lookup",
    )

    language = COMPONENT / "languages" / "simi"
    for relative in LANGUAGE_FILES:
        check((language / relative).is_file(), f"missing language file: {relative}")

    snippets_path = COMPONENT / "snippets" / "simi.json"
    check(snippets_path.is_file(), "missing Simi snippets")
    snippets = json.loads(snippets_path.read_text(encoding="utf-8"))
    prefixes = {snippet["prefix"] for snippet in snippets.values()}
    check(
        prefixes == {"case", "do", "fn"},
        "unexpected Simi snippet inventory",
    )
    expected_case_snippet = [
        "case ${1} of",
        "    ${2}",
        "end",
    ]
    expected_fn_snippet = [
        "fn ${1}(${2}) ${3}",
    ]
    expected_do_snippet = [
        "do",
        "    ${1}",
        "end",
    ]
    check(snippets["Case expression"]["body"] == expected_case_snippet, "invalid case snippet body")
    check(snippets["Named function"]["body"] == expected_fn_snippet, "invalid fn snippet body")
    check(snippets["Standalone block"]["body"] == expected_do_snippet, "invalid do snippet body")
    for snippet in snippets.values():
        flat = "\n".join(snippet["body"])
        check("$0" not in flat, f"{snippet['prefix']} must not use $0")
        check("${0" not in flat, f"{snippet['prefix']} must use numeric-only tab stops")
    vscode_snippets = COMPONENT.parent / "vscode" / "snippets" / "simi.json"
    check(
        snippets_path.read_bytes() == vscode_snippets.read_bytes(),
        "VS Code and Zed snippets must stay identical",
    )

    config = load_toml(language / "config.toml")
    check(config["name"] == "Simi", "language name must be Simi")
    check(config["grammar"] == "simi", "language grammar must be simi")
    check("simi" in config["path_suffixes"], "missing .simi association")
    check("-- " in config["line_comments"], "missing Simi line comment")

    increase = re.compile(config["increase_indent_pattern"])
    decrease = re.compile(config["decrease_indent_pattern"])
    for line in (
        "fn add(left, right)",
        "[head, ..tail] when ready =>",
        "[head, ..tail] when ready => do",
        "catch",
        "    case value of",
        '    case "x of y" of',
        '    case " of " of',
        "    case value of -- of in a comment",
        "    do",
    ):
        check(increase.search(line) is not None, f"line should increase indentation: {line}")
    for line in (
        "fn add(a, b) do a + b end",
        "_ => do value end",
        "case value of _ => do n end",
        'case "x of y" of _ => do 1 end',
        "    value -- fake =>",
    ):
        check(increase.search(line) is None, f"one-line or comment-arrow form must not indent: {line}")
    for line in ("end", "catch", "elseif ready then", "else"):
        check(decrease.search(line) is not None, f"line should decrease indentation: {line}")
    check(decrease.search("_ =>") is None, "arm arrow must not decrease indentation")
    case_indent = 4
    provisional_indent = case_indent + (4 if increase.search("    case value of") else 0)
    check(provisional_indent == 8, "incomplete case must provisionally indent one level")
    for legacy in ("match value with", "case value ->"):
        check(increase.search(legacy) is None, f"legacy syntax affects indentation: {legacy}")
        check(decrease.search(legacy) is None, f"legacy syntax affects indentation: {legacy}")

    highlights = (language / "highlights.scm").read_text(encoding="utf-8")
    for keyword in ('"case"', '"of"', '"when"'):
        check(keyword in highlights, f"missing highlight keyword: {keyword}")
    for removed in ('"match"', '"with"'):
        check(removed not in highlights, f"legacy highlight token remains: {removed}")
    check('"->"' in highlights, "type return arrow is not highlighted")

    shared_indents = (COMPONENT.parent / "tree-sitter" / "queries" / "indents.scm").read_text(encoding="utf-8")
    branch_capture = shared_indents.rsplit("[", 1)[-1]
    check('"of"' not in branch_capture, "shared query must not capture of as an indent branch")
    check("(case_expression)" in shared_indents, "shared query must indent the enclosing case")
    check("(case_clause)" in shared_indents, "shared query must indent each case-arm body")
    check("(catch_arm)" in shared_indents, "shared query must indent each catch-arm body")

    indents = (language / "indents.scm").read_text(encoding="utf-8")
    check('(case_expression\n  "end" @end) @indent' in indents, "case_expression must own clause-level indentation")
    check("(case_clause) @indent" in indents, "each case clause must own its body indentation")
    check('(protected_expression\n  "end" @end) @indent' in indents, "protected_expression must own its final end indentation")
    check("(catch_arm) @indent" in indents, "each catch arm must own its body indentation")
    check('"of" @end' not in indents, "of must not be an arm alignment/end capture")
    for removed_node in ("match_expression", "pattern_clause"):
        check(removed_node not in indents, f"legacy indent node remains: {removed_node}")

    fixture = (COMPONENT / "tests" / "fixtures" / "language.simi").read_text(encoding="utf-8")
    check("case value" in fixture and fixture.count(" =>") >= 7, "fixture does not exercise repeated case arms")
    check("        _ =>" in fixture, "fixture does not exercise a direct final case arm")
    check("        0 =>" in fixture, "fixture does not exercise a direct do case-arm expression")
    check(fixture.count(" =>") >= 7, "fixture does not exercise repeated case and catch arms")
    protected_block = [
        ("do", 0),
        ("let error = { error = \"example\" }", 4),
        ("raise error", 4),
        ("catch", 0),
        ("{ error = message } when message != nil =>", 4),
        ("classify([final, indexed_pairs])", 8),
        ("\"retry\" =>", 4),
        ("do", 8),
        ("let recovered = classify([final])", 12),
        ("recovered", 12),
        ("end", 8),
        ("_ =>", 4),
        ("nil", 8),
        ("end", 0),
    ]
    fixture_lines = fixture.splitlines()
    protected_start = fixture_lines.index("do")
    actual_protected_block = [
        (line.lstrip(), len(line) - len(line.lstrip(" ")))
        for line in fixture_lines[protected_start : protected_start + len(protected_block)]
    ]
    check(actual_protected_block == protected_block, "fixture must indent protected catch arms and direct bodies")
    check("?>" in fixture and "?" in fixture, "fixture does not exercise nil control flow")
    for removed in ("match ", " with\n"):
        check(removed not in fixture, f"fixture contains legacy syntax: {removed.strip()}")
    check(" -> " in fixture, "fixture does not exercise return annotations")

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
    check((extension / "snippets" / "simi.json").is_file(), "snippets not copied")
    check((extension / "Cargo.toml").is_file(), "extension Cargo.toml not copied")
    check((extension / "src" / "lib.rs").is_file(), "extension Rust source not copied")
    server = manifest.get("language_servers", {}).get(SERVER_ID)
    check(server is not None, "generated manifest has no simi-lsp declaration")
    check(server.get("languages") == ["Simi"], "generated simi-lsp language mismatch")

    if parsed.scheme == "https":
        extension_repository = urlparse(manifest.get("repository", ""))
        check(
            extension_repository.scheme == "https" and bool(extension_repository.netloc),
            "publishable extension repository must be top-level https",
        )

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

    shared_indent_result = subprocess.run(
        [
            "tree-sitter",
            "query",
            str(COMPONENT.parent / "tree-sitter" / "queries" / "indents.scm"),
            str(fixture),
        ],
        cwd=grammar,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    )
    check(
        re.search(r"indent\\.branch[^\\n]*text: `of`", shared_indent_result.stdout) is None,
        "shared query must not emit branch captures for of tokens",
    )
    check(
        shared_indent_result.stdout.count("capture: indent.begin") >= 6,
        "shared query must capture enclosing case/protected expressions and their arms",
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
