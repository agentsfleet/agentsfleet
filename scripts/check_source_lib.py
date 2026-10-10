"""Shared plumbing for the repository-owned source lints (`scripts/check_*.py`).

Each lint is a set of rules over files in this repository. A rule reports a
`Finding`: the file, the line, the rule's name and what to do. The lints take a
root directory so their own tests can point them at a fixture tree.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import re
import subprocess
import sys

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


def blank_js_comments(source: str) -> str:
    """`source` with every comment blanked to spaces; strings and lines kept.

    A rule that matches code must not match a comment that names the thing it
    bans, and a report must still point at the right line, so a comment's
    characters become spaces (newlines survive) rather than disappearing.
    """
    out = []
    index = 0
    quote = ""
    length = len(source)
    while index < length:
        char = source[index]
        if quote:
            out.append(char)
            if char == "\\" and index + 1 < length:
                out.append(source[index + 1])
                index += 2
                continue
            if char == quote:
                quote = ""
            index += 1
            continue
        pair = source[index : index + 2]
        if pair in ("//", "/*"):
            end = source.find("\n", index) if pair == "//" else source.find("*/", index + 2)
            end = length if end == -1 else (end if pair == "//" else end + 2)
            out.append("".join(c if c == "\n" else " " for c in source[index:end]))
            index = end
            continue
        if char in "\"'`":
            quote = char
        out.append(char)
        index += 1
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
