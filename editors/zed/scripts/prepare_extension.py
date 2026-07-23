#!/usr/bin/env python3
"""Prepare a portable local or future-publishable Simi Zed extension."""

from __future__ import annotations

import argparse
import re
import shutil
import subprocess
import sys
from pathlib import Path
from urllib.parse import urlparse

COMPONENT = Path(__file__).resolve().parents[1]
BASE_MANIFEST = COMPONENT / "extension.toml"
LANGUAGES = COMPONENT / "languages"
RUST_SOURCE = COMPONENT / "src"
HEX_REVISION = re.compile(r"[0-9a-fA-F]{40}(?:[0-9a-fA-F]{24})?")


def fail(message: str) -> None:
    raise SystemExit(message)


def run(*command: str, cwd: Path) -> str:
    completed = subprocess.run(
        command,
        cwd=cwd,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return completed.stdout.strip()


def copy_grammar(source: Path, destination: Path) -> None:
    source = source.resolve()
    if not (source / "grammar.js").is_file():
        fail(f"grammar source has no grammar.js: {source}")
    if not (source / "src" / "parser.c").is_file():
        fail(
            "grammar source has no generated src/parser.c; run the grammar's "
            f"generation recipe first: {source}"
        )

    if destination.exists():
        shutil.rmtree(destination)
    shutil.copytree(
        source,
        destination,
        ignore=shutil.ignore_patterns(
            ".git", ".local", "node_modules", "target", "build"
        ),
    )


def initialize_git_repository(grammar: Path) -> str:
    run("git", "init", "--quiet", "--initial-branch=main", cwd=grammar)
    run("git", "add", "--all", cwd=grammar)
    run(
        "git",
        "-c",
        "user.name=Simi local extension",
        "-c",
        "user.email=simi-local@example.invalid",
        "commit",
        "--quiet",
        "--message=Local Simi grammar snapshot",
        cwd=grammar,
    )
    return run("git", "rev-parse", "HEAD", cwd=grammar)


def validate_https_url(value: str, label: str) -> None:
    parsed = urlparse(value)
    if parsed.scheme != "https" or not parsed.netloc:
        fail(f"{label} must be an absolute https:// URL")
    if parsed.username is not None or parsed.password is not None:
        fail(f"{label} must not contain embedded credentials")


def validate_revision(value: str) -> None:
    if not HEX_REVISION.fullmatch(value):
        fail("grammar commit must be an exact 40- or 64-character hexadecimal Git ID")


def write_extension(
    destination: Path,
    grammar_repository: str,
    grammar_commit: str,
    extension_repository: str | None = None,
) -> None:
    if destination.exists():
        shutil.rmtree(destination)
    destination.mkdir(parents=True)
    shutil.copytree(LANGUAGES, destination / "languages")
    shutil.copytree(RUST_SOURCE, destination / "src")
    shutil.copy2(COMPONENT / "Cargo.toml", destination / "Cargo.toml")
    cargo_lock = COMPONENT / "Cargo.lock"
    if cargo_lock.is_file():
        shutil.copy2(cargo_lock, destination / "Cargo.lock")

    manifest = BASE_MANIFEST.read_text(encoding="utf-8").rstrip() + "\n"
    if extension_repository is not None:
        first_table = manifest.find("\n[")
        if first_table < 0:
            first_table = len(manifest)
        manifest = (
            manifest[:first_table]
            + f'\nrepository = "{extension_repository}"\n'
            + manifest[first_table:]
        )
    manifest += (
        "\n[grammars.simi]\n"
        f'repository = "{grammar_repository}"\n'
        f'rev = "{grammar_commit}"\n'
    )
    (destination / "extension.toml").write_text(manifest, encoding="utf-8")


def prepare_local(args: argparse.Namespace) -> None:
    source = args.grammar_source.resolve()
    output = args.output.resolve()
    grammar = output / "tree-sitter-simi"
    extension = output / "extension"
    output.mkdir(parents=True, exist_ok=True)

    copy_grammar(source, grammar)
    commit = initialize_git_repository(grammar)
    write_extension(extension, grammar.as_uri(), commit)
    print(f"grammar: {grammar}")
    print(f"commit: {commit}")
    print(f"extension: {extension}")


def prepare_publish(args: argparse.Namespace) -> None:
    validate_https_url(args.grammar_repository, "grammar repository")
    validate_https_url(args.extension_repository, "extension repository")
    validate_revision(args.grammar_commit)
    destination = args.output.resolve()
    write_extension(
        destination,
        args.grammar_repository,
        args.grammar_commit.lower(),
        args.extension_repository,
    )
    print(f"prepared extension: {destination}")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser()
    subparsers = result.add_subparsers(dest="command", required=True)

    local = subparsers.add_parser("local", help="prepare a local dev extension")
    local.add_argument("--grammar-source", type=Path, required=True)
    local.add_argument("--output", type=Path, required=True)
    local.set_defaults(function=prepare_local)

    publish = subparsers.add_parser(
        "publish", help="prepare an extension pinned to a public grammar commit"
    )
    publish.add_argument("--grammar-repository", required=True)
    publish.add_argument("--grammar-commit", required=True)
    publish.add_argument("--extension-repository", required=True)
    publish.add_argument("--output", type=Path, required=True)
    publish.set_defaults(function=prepare_publish)
    return result


def main() -> None:
    args = parser().parse_args()
    try:
        args.function(args)
    except subprocess.CalledProcessError as error:
        detail = error.stderr.strip() if error.stderr else str(error)
        fail(f"command failed: {detail}")


if __name__ == "__main__":
    main()
