#!/usr/bin/env python3
"""A test calls the code it proves; it never reads the repository's files.

A test that reads source, configuration, a workflow or a document asserts on
text: it turns red when a file is reflowed and stays green when the behaviour
breaks in a way the text does not show. This lint finds every such read in a
test file, in TypeScript and JavaScript, Rust, shell and Python.

    python3 scripts/check_tests_call_code.py [--root <dir>]

Exit 0 clean, 1 with one line per finding, 2 on a usage error.

What counts:

- A test file is one whose name marks it a test (`*.test.ts`, `*.spec.tsx`,
  `*_test.sh`, `*_test.py`, `test_*.py`), any file under a `test/`, `tests/`
  or `__tests__/` directory, and Rust's `tests.rs`, `*_tests.rs` and the body
  of a `#[cfg(test)]` module.
- A read is a file-reading call (`readFileSync`, `Bun.file`, `include_str!`,
  `fs::read_to_string`, `open`, `read_text`, `cat`, `grep`, `sed`, …) or a
  directory walk, whose path is anchored in the repository: it is built from
  the test's own location (`__dirname`, `import.meta.url`,
  `CARGO_MANIFEST_DIR`, `__file__`, `BASH_SOURCE`, `git rev-parse`), from a
  relative path, or from a variable that was.
- A path through a fixture directory (`fixtures/`, `testdata/`,
  `__fixtures__/`, `__snapshots__/`, `golden/`) is the test's own input and is
  allowed. So is a path the test made itself (a temporary directory), because
  nothing anchors it in the repository, and a path naming nothing git tracks
  (a cache the harness writes).
- Handing a file to something that executes it is calling the code: a shell
  read piped into `bash` or redirected into a copy, `Bun.write` of a
  `Bun.file`, and `source` are not findings.
- A read spelled inside a Python string literal is data, never a call: a
  test that seeds code samples (this lint's own test does) holds them in
  strings. Code between an f-string's braces still runs, so it still counts.
"""

from __future__ import annotations

import io
from pathlib import Path
import re
import sys
import tokenize

sys.dont_write_bytecode = True

import check_source_lib as lib

MESSAGE = "call the code or move the check to a lint"
TARGET_MAX = 80

from check_tests_call_code_languages import (
    FIXTURE, FIXTURE_DIRS, JS_SUFFIXES, LANGUAGES, SKIPPED, TEST_DIRS, TS_TEST_NAME,
)

# A shell value that runs something: its output is not a path, so it anchors nothing.
EXECUTED_SUBSTITUTION = re.compile(r'^"?\$\((?!\s*(?:cd|dirname|git rev-parse|pwd|realpath)\b)')
# A read that hands the file to something that runs it, or copies it for that.
SHELL_EXECUTES = re.compile(r"\|\s*(?:\w+=\S*\s+)*(?:bash|sh)\b|>\s*\"?\$\w")
JS_COPY = re.compile(r"Bun\.write\([^;]*$")
STRING_LITERAL = re.compile(r""""([^"\n]*)"|'([^'\n]*)'|`([^`]*)`""")
POSITIONAL = re.compile(r'\$\{?[1-9]\b')
SHELL_FUNCTION = re.compile(r"^(?P<name>[A-Za-z_]\w*)\s*\(\)\s*\{(?P<body>.*?)^\}", re.MULTILINE | re.DOTALL)
RESOLVE_DEPTH = 4
# What a shell read's match may begin with before the command itself.
LEADING_DELIMITERS = "\n\t ;|&(`"
# A literal that spells a path: a separator, or a name with an extension. An
# encoding ("utf8") or a message is neither.
PATH_LIKE = re.compile(r"^[^\s]*(?:/|\.[A-Za-z0-9]+$)[^\s]*$")
# Tokens whose text is a Python literal; an f-string's braced code tokenizes apart.
PYTHON_LITERAL_TOKENS = frozenset({"STRING", "FSTRING_MIDDLE", "TSTRING_MIDDLE"})


def language_of(path: Path) -> str | None:
    if path.name.endswith(JS_SUFFIXES):
        return "js"
    return {".rs": "rust", ".py": "python", ".sh": "shell"}.get(path.suffix)


def is_test_file(relative_path: Path) -> bool:
    name, parts = relative_path.name, relative_path.parts
    if any(part in FIXTURE_DIRS for part in parts[:-1]):
        return False
    if TS_TEST_NAME.search(name) or name.endswith(("_test.sh", "_test.py")):
        return True
    if name.startswith("test_") and name.endswith(".py"):
        return True
    if name.endswith(".rs") and (name == "tests.rs" or name.endswith("_tests.rs")):
        return True
    return language_of(relative_path) is not None and bool(TEST_DIRS.intersection(parts[:-1]))


def cfg_test_regions(source: str) -> list[tuple[int, int]]:
    """Offsets of each `#[cfg(test)]` item's body in a Rust source file."""
    regions = []
    for match in re.finditer(r"#\[cfg\(test\)\]", source):
        brace, semicolon = source.find("{", match.end()), source.find(";", match.end())
        if brace == -1 or (semicolon != -1 and semicolon < brace):
            continue  # `mod tests;` — the body is its own file
        depth, index = 0, brace
        while index < len(source):
            depth += {"{": 1, "}": -1}.get(source[index], 0)
            if depth == 0:
                break
            index += 1
        regions.append((match.start(), index))
    return regions


def strip_comments(source: str, language: str) -> str:
    if language in ("js", "rust"):
        return lib.blank_js_comments(source)
    marker = LANGUAGES[language]["comment"]
    return "\n".join(
        "" if line.lstrip().startswith(marker) else line for line in source.splitlines()
    ) + "\n"


def assignments(code: str, language: str) -> dict[str, str]:
    found: dict[str, str] = {}
    for match in LANGUAGES[language]["assign"].finditer(code):
        groups = match.groupdict()
        name, value = groups.get("name") or groups.get("loop"), groups.get("value") or groups.get("items")
        if language == "shell" and EXECUTED_SUBSTITUTION.match(value.strip()):
            continue
        found[name] = (found.get(name, "") + " " + value).strip()
    return found


def names_pattern(names: set[str], language: str) -> str:
    joined = "|".join(map(re.escape, sorted(names)))
    return rf"\$\{{?(?:{joined})\b" if language == "shell" else rf"(?<![\w.$])(?:{joined})\b"


def grow(values: dict[str, str], seed: str, language: str, blocked: re.Pattern[str] | None) -> set[str]:
    """Names whose value matches `seed` or names an earlier match, to a fixpoint."""
    names: set[str] = set()
    while True:
        pattern = re.compile(seed if not names else f"{seed}|{names_pattern(names, language)}")
        found = {n for n, v in values.items() if pattern.search(v) and not (blocked and blocked.search(v))}
        if found <= names:
            return names
        names |= found


def resolved_text(argument: str, values: dict[str, str], language: str) -> str:
    """The argument with the values of the names it uses appended, a few hops deep."""
    text = argument
    for _ in range(RESOLVE_DEPTH):
        used = [v for n, v in values.items() if re.search(names_pattern({n}, language), text) and v not in text]
        if not used:
            break
        text += " " + " ".join(used)
    return text


def names_untracked_path(text: str, tracked_parts: frozenset[str]) -> bool:
    """True when every path the text spells names something git does not track."""
    parts = [
        part
        for literal in STRING_LITERAL.finditer(text)
        if PATH_LIKE.search("".join(g or "" for g in literal.groups()))
        for part in "".join(g or "" for g in literal.groups()).split("/")
        if part and part not in (".", "..") and "$" not in part and "{" not in part
    ]
    return bool(parts) and not any(part in tracked_parts for part in parts)


def python_literal_spans(code: str) -> list[tuple[int, int]]:
    """Offsets of each Python string literal's text, from the standard tokenizer."""
    line_offsets = [0]
    for line in code.splitlines(keepends=True):
        line_offsets.append(line_offsets[-1] + len(line))
    spans = []
    try:
        for token in tokenize.generate_tokens(io.StringIO(code).readline):
            if tokenize.tok_name[token.type] in PYTHON_LITERAL_TOKENS:
                (start_row, start_col), (end_row, end_col) = token.start, token.end
                spans.append((line_offsets[start_row - 1] + start_col, line_offsets[end_row - 1] + end_col))
    except (tokenize.TokenError, SyntaxError):
        pass  # source that will not tokenize keeps the literals found so far, and every read is a candidate
    return spans


def call_argument(code: str, start: int, language: str) -> str:
    """The text a read call is handed: its parenthesised arguments, or the rest of the command."""
    line_end = code.find("\n", start)
    line_end = len(code) if line_end == -1 else line_end
    if language == "shell":
        while code[:line_end].rstrip().endswith("\\") and line_end < len(code):
            next_end = code.find("\n", line_end + 1)
            line_end = len(code) if next_end == -1 else next_end
        return code[start:line_end]
    paren = code.find("(", start, line_end + 1)
    if paren == -1:
        return code[start:line_end]
    depth, index = 0, paren
    while index < len(code):
        depth += {"(": 1, ")": -1}.get(code[index], 0)
        if depth == 0:
            return code[paren + 1 : index]
        index += 1
    return code[paren + 1 :]


class Scan:
    """One test file's code, with the names anchored in the repository."""

    def __init__(self, source: str, language: str, tracked_parts: frozenset[str]) -> None:
        self.language = language
        self.code = strip_comments(source, language)
        self.values = assignments(self.code, language)
        anchors = LANGUAGES[language]["anchors"]
        self.fixtures = grow(self.values, FIXTURE.pattern, language, None)
        fixture_names = re.compile(names_pattern(self.fixtures, language)) if self.fixtures else None
        self.anchored = grow(self.values, anchors, language, fixture_names)
        self.anchor = re.compile(anchors if not self.anchored else f"{anchors}|{names_pattern(self.anchored, language)}")
        self.fixture_names = fixture_names
        self.tracked_parts = tracked_parts
        self.literals = python_literal_spans(self.code) if language == "python" else []

    def is_repository_read(self, argument: str) -> bool:
        if not self.anchor.search(argument) or FIXTURE.search(argument):
            return False
        if self.fixture_names and self.fixture_names.search(argument):
            return False
        return not names_untracked_path(resolved_text(argument, self.values, self.language), self.tracked_parts)

    def executes(self, start: int, argument: str) -> bool:
        if self.language == "shell":
            return bool(SHELL_EXECUTES.search(argument))
        statement_start = max(self.code.rfind(";", 0, start), self.code.rfind("\n\n", 0, start))
        return self.language == "js" and bool(JS_COPY.search(self.code[statement_start + 1 : start]))


def read_findings(scan: Scan, relative_path: str, regions: list[tuple[int, int]] | None) -> list[lib.Finding]:
    found = []
    for read in LANGUAGES[scan.language]["reads"].finditer(scan.code):
        start = read.start() + len(read.group(0)) - len(read.group(0).lstrip(LEADING_DELIMITERS))
        if regions is not None and not any(a <= start <= b for a, b in regions):
            continue
        if any(a <= start < b for a, b in scan.literals):
            continue
        argument = call_argument(scan.code, start, scan.language)
        if scan.language == "python" or read.group(0).lstrip().startswith("."):
            line_start = scan.code.rfind("\n", 0, read.start()) + 1
            argument = scan.code[line_start : read.end()] + argument + ")"
        if read.group(0).startswith("from"):
            argument = read.group(0)
        if scan.is_repository_read(argument) and not scan.executes(start, argument):
            found.append(finding(relative_path, scan.code, start, argument))
    return found


def shell_helper_findings(scan: Scan, relative_path: str) -> list[lib.Finding]:
    """Calls that hand an anchored path to a function whose body reads its argument."""
    readers = set()
    for function in SHELL_FUNCTION.finditer(scan.code):
        body = function["body"]
        params = {n for n, v in assignments(body, "shell").items() if POSITIONAL.search(v)}
        param = re.compile(POSITIONAL.pattern + ("|" + names_pattern(params, "shell") if params else ""))
        for read in LANGUAGES["shell"]["reads"].finditer(body):
            if param.search(call_argument(body, read.start(), "shell")):
                readers.add(function["name"])
    found = []
    for name in readers:
        for call in re.finditer(rf"^\s*{re.escape(name)}\b(?!\s*\(\))", scan.code, re.MULTILINE):
            argument = call_argument(scan.code, call.start(), "shell")
            if scan.is_repository_read(argument):
                found.append(finding(relative_path, scan.code, call.start(), argument))
    return found


def finding(relative_path: str, code: str, offset: int, argument: str) -> lib.Finding:
    target = " ".join(argument.replace("\\\n", " ").split())[:TARGET_MAX]
    return lib.Finding(relative_path, lib.line_of(code, offset), "test reads", f"{target}; {MESSAGE}")


def findings_in(relative_path: str, source: str, language: str, regions, tracked_parts: frozenset[str]) -> list[lib.Finding]:
    scan = Scan(source, language, tracked_parts)
    found = read_findings(scan, relative_path, regions)
    if language == "shell":
        found += shell_helper_findings(scan, relative_path)
    first_per_line: dict[int, lib.Finding] = {}
    for item in found:
        first_per_line.setdefault(item.line, item)
    return list(first_per_line.values())


def candidate_files(root: Path) -> list[Path]:
    tracked = lib.tracked_files(root)
    paths = [Path(p) for p in tracked] if tracked else [p.relative_to(root) for p in root.rglob("*") if p.is_file()]
    return [p for p in paths if not SKIPPED.intersection(p.parts)]


def lint(root: Path) -> list[lib.Finding]:
    files = candidate_files(root)
    tracked_parts = frozenset(part for path in files for part in path.parts)
    found = []
    for relative_path in files:
        language = language_of(relative_path)
        path = root / relative_path
        if language is None or not path.is_file():
            continue
        source = path.read_text(encoding="utf-8", errors="replace")
        if is_test_file(relative_path):
            found += findings_in(relative_path.as_posix(), source, language, None, tracked_parts)
        elif language == "rust" and "#[cfg(test)]" in source:
            found += findings_in(relative_path.as_posix(), source, language, cfg_test_regions(source), tracked_parts)
    return found


def report(findings: list[lib.Finding]) -> int:
    for item in sorted(set(findings)):
        print(f"{item.path}:{item.line}: test reads {item.message}")
    if findings:
        print(f"✗ [tests call code] {len(set(findings))} test read(s) of repository files", file=sys.stderr)
        return lib.EXIT_FINDINGS
    print("✓ [tests call code] no test reads a repository file")
    return lib.EXIT_CLEAN


def main(argv: list[str]) -> int:
    root = lib.parse_root(argv)
    if root is None:
        print(f"usage: {Path(sys.argv[0]).name} [{lib.ROOT_FLAG} <dir>]", file=sys.stderr)
        return lib.EXIT_USAGE
    return report(lint(root))


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
