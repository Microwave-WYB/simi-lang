#!/usr/bin/env python3
"""Regenerate the shared language-tour contents and navigation blocks."""

from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TOUR = ROOT / "docs" / "language-tour"
ORDER = TOUR / "order.txt"
LANDING = ROOT / "docs" / "language-tour.md"
CONTENTS_START = "<!-- tour:contents:start -->"
CONTENTS_END = "<!-- tour:contents:end -->"
NAV_START = "<!-- tour:navigation:start -->"
NAV_END = "<!-- tour:navigation:end -->"


def load_pages() -> list[tuple[str, str]]:
    pages: list[tuple[str, str]] = []
    for line_number, raw in enumerate(ORDER.read_text().splitlines(), start=1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        try:
            filename, title = line.split("|", 1)
        except ValueError as error:
            raise SystemExit(f"{ORDER}:{line_number}: expected filename|Title") from error
        if re.match(r"^\d", filename):
            raise SystemExit(f"{ORDER}:{line_number}: tour filenames must not be numbered")
        pages.append((filename, title))
    if not pages:
        raise SystemExit(f"{ORDER}: no tour pages")
    if len({filename for filename, _ in pages}) != len(pages):
        raise SystemExit(f"{ORDER}: duplicate filename")
    return pages


def github_anchor(title: str) -> str:
    value = title.lower().replace("`", "")
    value = re.sub(r"<[^>]+>", "", value)
    value = re.sub(r"[^\w\- ]", "", value)
    return re.sub(r" +", "-", value.strip())


def replace_generated_or_legacy_contents(markdown: str, replacement: str) -> str:
    generated = re.compile(
        rf"{re.escape(CONTENTS_START)}.*?{re.escape(CONTENTS_END)}[ \t]*\n*",
        re.DOTALL,
    )
    if generated.search(markdown):
        return generated.sub(replacement + "\n\n", markdown, count=1)
    legacy = re.compile(r"## Tour contents\n.*?(?=\n## )", re.DOTALL)
    if not legacy.search(markdown):
        raise SystemExit("tour page is missing its Tour contents section")
    return legacy.sub(replacement, markdown, count=1)


def strip_generated_or_legacy_navigation(markdown: str) -> str:
    generated = re.compile(
        rf"\n{re.escape(NAV_START)}.*?{re.escape(NAV_END)}\s*$",
        re.DOTALL,
    )
    if generated.search(markdown):
        return generated.sub("", markdown).rstrip()
    legacy = re.compile(r"\n---\n\n(?:\[Previous:.*\]\([^\n]+\)\n\n)?(?:\[Next:.*\]\([^\n]+\)\n?)?\s*$")
    return legacy.sub("", markdown).rstrip()


def page_contents(
    pages: list[tuple[str, str]], current: int, sections: list[tuple[int, str]]
) -> str:
    lines = [CONTENTS_START, "## Tour contents", ""]
    for index, (filename, title) in enumerate(pages):
        if index == current:
            lines.append(f"- {title}")
            for level, section in sections:
                indent = "  " if level == 2 else "    "
                lines.append(f"{indent}- [{section}](#{github_anchor(section)})")
        else:
            lines.append(f"- [{title}]({filename})")
    lines.append(CONTENTS_END)
    return "\n".join(lines)


def navigation(pages: list[tuple[str, str]], current: int) -> str:
    lines = [NAV_START, "---", ""]
    if current > 0:
        filename, title = pages[current - 1]
        lines.extend([f"[Previous: {title}]({filename})", ""])
    if current + 1 < len(pages):
        filename, title = pages[current + 1]
        lines.extend([f"[Next: {title}]({filename})", ""])
    if lines[-1] == "":
        lines.pop()
    lines.append(NAV_END)
    return "\n".join(lines)


def build_page(pages: list[tuple[str, str]], current: int) -> None:
    filename, title = pages[current]
    path = TOUR / filename
    if not path.is_file():
        raise SystemExit(f"missing tour page: {path}")
    markdown = path.read_text()
    if not markdown.startswith(f"# {title}\n"):
        raise SystemExit(f"{path}: expected title '# {title}'")

    markdown = strip_generated_or_legacy_navigation(markdown)
    without_contents = re.sub(
        rf"{re.escape(CONTENTS_START)}.*?{re.escape(CONTENTS_END)}\n*",
        "",
        markdown,
        count=1,
        flags=re.DOTALL,
    )
    if without_contents == markdown:
        without_contents = re.sub(
            r"## Tour contents\n.*?(?=\n## )\n*", "", markdown, count=1, flags=re.DOTALL
        )
    sections = []
    for match in re.finditer(r"^(##|###) (.+)$", without_contents, re.MULTILINE):
        sections.append((len(match.group(1)), match.group(2)))

    contents = page_contents(pages, current, sections)
    markdown = replace_generated_or_legacy_contents(markdown, contents)
    markdown = markdown.rstrip() + "\n\n" + navigation(pages, current) + "\n"
    path.write_text(markdown)


def build_landing(pages: list[tuple[str, str]]) -> None:
    markdown = LANDING.read_text()
    lines = [CONTENTS_START, "## Tour contents", ""]
    lines.extend(
        f"- [{title}](language-tour/{filename})" for filename, title in pages
    )
    lines.append(CONTENTS_END)
    replacement = "\n".join(lines)
    generated = re.compile(
        rf"{re.escape(CONTENTS_START)}.*?{re.escape(CONTENTS_END)}[ \t]*\n*",
        re.DOTALL,
    )
    if generated.search(markdown):
        markdown = generated.sub(replacement + "\n\n", markdown, count=1)
    else:
        legacy = re.compile(r"## Tour contents\n\n(?:- .+\n)+")
        if not legacy.search(markdown):
            raise SystemExit(f"{LANDING}: missing Tour contents list")
        markdown = legacy.sub(replacement + "\n", markdown, count=1)
    LANDING.write_text(markdown)


def main() -> None:
    pages = load_pages()
    tracked = {path.name for path in TOUR.glob("*.md")}
    ordered = {filename for filename, _ in pages}
    if tracked != ordered:
        raise SystemExit(
            f"{ORDER}: page set mismatch; missing={sorted(tracked - ordered)}, "
            f"unknown={sorted(ordered - tracked)}"
        )
    for current in range(len(pages)):
        build_page(pages, current)
    build_landing(pages)


if __name__ == "__main__":
    main()
