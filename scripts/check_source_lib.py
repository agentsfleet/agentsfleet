"""Shared plumbing for the repository-owned source lints (`scripts/check_*.py`).

Each lint is a set of rules over files in this repository. A rule reports a
`Finding`: the file, the line, the rule's name and what to do. The lints take a
root directory so their own tests can point them at a fixture tree.
"""

from __future__ import annotations

import contextlib
from dataclasses import dataclass
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
from typing import Iterator

sys.dont_write_bytecode = True

ROOT = Path(__file__).resolve().parents[1]

EXIT_CLEAN = 0
EXIT_FINDINGS = 1
EXIT_USAGE = 2

ROOT_FLAG = "--root"

# Directories no source rule reads: dependencies and generated output.
SKIPPED_DIRS = frozenset({"node_modules", ".next", "dist", "coverage", ".turbo", "target"})

# File names that mark a test, in every language this repository writes tests in.
TEST_NAME = re.compile(r"(\.test\.|\.spec\.|_test\.)")

JS_SUFFIXES = (".ts", ".tsx", ".js", ".jsx", ".mjs")


@dataclass(frozen=True, order=True)
class Finding:
    path: str
    line: int
    rule: str
    message: str

    def __str__(self) -> str:
        return f"{self.path}:{self.line}: {self.rule}: {self.message}"


def line_of(text: str, offset: int) -> int:
    """The 1-based line an offset into `text` sits on."""
    return text.count("\n", 0, offset) + 1


def relative(root: Path, path: Path) -> str:
    return path.relative_to(root).as_posix()


def source_files(base: Path, suffixes: tuple[str, ...], include_tests: bool = False) -> list[Path]:
    """Every file under `base` with one of `suffixes`, dependencies skipped."""
    if not base.is_dir():
        return []
    found = []
    for path in base.rglob("*"):
        if not path.is_file() or not path.name.endswith(suffixes):
            continue
        if SKIPPED_DIRS.intersection(path.relative_to(base).parts):
            continue
        if not include_tests and TEST_NAME.search(path.name):
            continue
        found.append(path)
    return sorted(found)


# A slash after one of these starts a regular expression literal: an operator
# or an opener leaves nothing on its left for a division to divide. `<` stays
# out so a JSX closing tag reads as markup, not a pattern.
REGEX_AFTER_CHARS = frozenset("(,=:[!&|?{};+-*%~^>")
REGEX_AFTER_WORDS = frozenset({
    "return", "typeof", "case", "do", "else", "in", "of", "new", "delete",
    "void", "throw", "yield", "await", "instanceof",
})
IDENTIFIER_TAIL = re.compile(r"[A-Za-z_$][\w$]*$")


def _regex_may_start(code: str) -> bool:
    """Whether a slash after `code` (comments already blanked) opens a pattern."""
    before = code.rstrip()
    if not before:
        return True
    if before[-1] in REGEX_AFTER_CHARS:
        return True
    word = IDENTIFIER_TAIL.search(before)
    return bool(word) and word.group(0) in REGEX_AFTER_WORDS


def _regex_end(source: str, start: int) -> int | None:
    """The offset past the pattern literal opening at `start`, or None.

    A literal never spans a line, so one with no closing slash before the
    newline was a division after all and the caller treats the slash as one.
    """
    index = start + 1
    in_class = False
    while index < len(source):
        char = source[index]
        if char == "\n":
            return None
        if char == "\\":
            index += 2
            continue
        if char == "/" and not in_class:
            index += 1
            while index < len(source) and source[index].isalpha():
                index += 1
            return index
        in_class = (in_class or char == "[") and char != "]"
        index += 1
    return None


def _quoted_end(source: str, start: int) -> int:
    """The offset past the string opening at `start`.

    A single- or double-quoted string ends at its line: JavaScript allows no
    raw newline in one, so a quote that reaches one was an apostrophe in JSX
    text, and the next line is code again. A template literal spans lines.
    """
    quote = source[start]
    index = start + 1
    while index < len(source):
        char = source[index]
        if char == "\\":
            index += 2
            continue
        if char == quote:
            return index + 1
        if char == "\n" and quote != "`":
            return index
        index += 1
    return len(source)


def blank_js_comments(source: str) -> str:
    """`source` with every comment blanked to spaces; strings and lines kept.

    A rule that matches code must not match a comment that names the thing it
    bans, and a report must still point at the right line, so a comment's
    characters become spaces (newlines survive) rather than disappearing.
    String, template and pattern literals are copied whole, so a `//` inside
    one is never taken for a comment.
    """
    out: list[str] = []
    index = 0
    length = len(source)
    while index < length:
        char = source[index]
        pair = source[index : index + 2]
        if pair in ("//", "/*"):
            end = source.find("\n", index) if pair == "//" else source.find("*/", index + 2)
            end = length if end == -1 else (end if pair == "//" else end + 2)
            out.append("".join(c if c == "\n" else " " for c in source[index:end]))
            index = end
            continue
        end = None
        if char in "\"'`":
            end = _quoted_end(source, index)
        elif char == "/" and _regex_may_start("".join(out[-64:])):
            end = _regex_end(source, index)
        end = end or index + 1
        out.append(source[index:end])
        index = end
    return "".join(out)


def tracked_files(root: Path, *patterns: str) -> list[str]:
    """Paths git tracks under `root` matching `patterns`, relative to it."""
    result = subprocess.run(
        ["git", "ls-files", "--", *patterns],
        cwd=root,
        capture_output=True,
        text=True,
        check=False,
    )
    return sorted(line for line in result.stdout.splitlines() if line)


def parse_root(argv: list[str]) -> Path | None:
    """`--root <dir>` or nothing; None on any other argument."""
    if not argv:
        return ROOT
    if len(argv) == 2 and argv[0] == ROOT_FLAG and Path(argv[1]).is_dir():
        return Path(argv[1]).resolve()
    return None


def report(findings: list[Finding], label: str) -> int:
    """Print every finding, then a verdict; the exit code a lint returns."""
    for finding in sorted(set(findings)):
        print(finding)
    if findings:
        print(f"✗ [{label}] {len(set(findings))} finding(s)")
        return EXIT_FINDINGS
    print(f"✓ [{label}] clean")
    return EXIT_CLEAN


def run(argv: list[str], label: str, lint) -> int:
    """The shared entry point: parse `--root`, lint, report."""
    root = parse_root(argv)
    if root is None:
        print(f"usage: {Path(sys.argv[0]).name} [{ROOT_FLAG} <dir>]", file=sys.stderr)
        return EXIT_USAGE
    return report(lint(root), label)


@contextlib.contextmanager
def fixture_repository(files: dict[str, str]) -> Iterator[Path]:
    """A temporary git repository holding `files`, for a lint's own tests.

    The GIT_* variables a hook exports are dropped while it lives: under
    `git commit` the pre-commit hook exports GIT_INDEX_FILE, and left in place
    the fixture's `git add` would write into the index of the running commit.
    """
    saved = {key: os.environ.pop(key) for key in list(os.environ) if key.startswith("GIT_")}
    try:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for relative_path, text in files.items():
                (root / relative_path).parent.mkdir(parents=True, exist_ok=True)
                (root / relative_path).write_text(text, encoding="utf-8")
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            subprocess.run(["git", "add", "-A"], cwd=root, check=True)
            yield root
    finally:
        os.environ.update(saved)
