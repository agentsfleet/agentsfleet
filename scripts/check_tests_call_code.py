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
  `fs::read_to_string`, `open`, `read_text`, `cat`, `grep`, `sed`, a shell
  `< file`, a spawned `grep` or `sed`, …), a directory walk, or a call to a
  test function whose body reads its parameter, whose path is anchored in the
  repository: it is built from the test's own location (`__dirname`,
  `import.meta.url`, `CARGO_MANIFEST_DIR`, `__file__`, `BASH_SOURCE`,
  `git rev-parse`), from a relative path (in Rust, a crate-relative literal
  handed whole to the read), or from a variable that was.
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

from pathlib import Path
import re
import sys

sys.dont_write_bytecode = True

import check_source_lib as lib

MESSAGE = "call the code or move the check to a lint"
TARGET_MAX = 80

from check_tests_call_code_languages import (
    FIXTURE, FIXTURE_DIRS, JS_SUFFIXES, LANGUAGES, SKIPPED, TEST_DIRS, TS_TEST_NAME,
)
from check_tests_call_code_text import (
    call_argument, cfg_test_regions, in_js_literal, js_code_only, js_functions, matching_close, python_literal_spans,
)

# A shell value that runs something: its output is not a path, so it anchors nothing.
EXECUTED_SUBSTITUTION = re.compile(r'^"?\$\((?!\s*(?:cd|dirname|git rev-parse|pwd|realpath)\b)')
# A read that hands the file to something that runs it, or copies it for that.
SHELL_EXECUTES = re.compile(r"\|\s*(?:\w+=\S*\s+)*(?:bash|sh)\b|>\s*\"?\$\w")
# A command whose input redirection feeds a shell: `bash < "$SCRIPT_DIR/x.sh"`.
SHELL_RUNS_INPUT = re.compile(r"(?:^|[;&|(])\s*(?:\w+=\S*\s+)*(?:bash|sh)\b[^;&|]*$")
# A copy call: a read inside its parentheses copies the file for running.
JS_COPY = re.compile(r"\bBun\.write\s*\(")
STRING_LITERAL = re.compile(r""""([^"\n]*)"|'([^'\n]*)'|`([^`]*)`""")
POSITIONAL = re.compile(r'\$\{?[1-9]\b')
SHELL_FUNCTION = re.compile(r"^(?P<name>[A-Za-z_]\w*)\s*\(\)\s*\{(?P<body>.*?)^\}", re.MULTILINE | re.DOTALL)
RESOLVE_DEPTH = 4
# What a shell read's match may begin with before the command itself.
LEADING_DELIMITERS = "\n\t ;|&(`"
# A literal that spells a path: a separator, or a name with an extension. An
# encoding ("utf8") or a message is neither.
PATH_LIKE = re.compile(r"^[^\s]*(?:/|\.[A-Za-z0-9]+$)[^\s]*$")


def language_of(path: Path) -> str | None:
    if path.name.endswith(JS_SUFFIXES):
        return "js"
    return {".rs": "rust", ".py": "python", ".sh": "shell"}.get(path.suffix)


def is_test_file(relative_path: Path) -> bool:
    name, parts = relative_path.name, relative_path.parts
    if TS_TEST_NAME.search(name) or name.endswith(("_test.sh", "_test.py")):
        return True
    if name.startswith("test_") and name.endswith(".py"):
        return True
    if name.endswith(".rs") and (name == "tests.rs" or name.endswith("_tests.rs")):
        return True
    # A helper beneath a fixture directory is the tests' input; one named as a test is a test anywhere.
    if any(part in FIXTURE_DIRS for part in parts[:-1]):
        return False
    return language_of(relative_path) is not None and bool(TEST_DIRS.intersection(parts[:-1]))


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
        direct = LANGUAGES[language].get("direct")
        self.direct = re.compile(direct) if direct else None

    def names_relative_path(self, argument: str) -> bool:
        """A relative path literal handed whole to the read, or through one name."""
        if self.direct is None:
            return False
        value = self.values.get(argument.strip().lstrip("&"), "")
        return bool(self.direct.search(argument) or self.direct.search(value))

    def is_repository_read(self, argument: str) -> bool:
        if not (self.anchor.search(argument) or self.names_relative_path(argument)) or FIXTURE.search(argument):
            return False
        if self.fixture_names and self.fixture_names.search(argument):
            return False
        return not names_untracked_path(resolved_text(argument, self.values, self.language), self.tracked_parts)

    def executes(self, start: int, argument: str) -> bool:
        return executes(self.code, self.language, start, argument)


def executes(code: str, language: str, start: int, argument: str) -> bool:
    """True when the read at `start` hands the file to something that runs it, or copies it for that."""
    if language == "shell":
        line = code[code.rfind("\n", 0, start) + 1 : start]
        feeds_shell = argument.startswith("<") and SHELL_RUNS_INPUT.search(line)
        return bool(SHELL_EXECUTES.search(argument) or feeds_shell)
    if language != "js":
        return False
    code = js_code_only(code)
    # A copy whose parenthesis never closes is a misread, never an exemption.
    return any(start < matching_close(code, copy.end() - 1, "()") < len(code) for copy in JS_COPY.finditer(code, 0, start))


def read_findings(scan: Scan, relative_path: str, regions: list[tuple[int, int]] | None) -> list[lib.Finding]:
    found = []
    for read in LANGUAGES[scan.language]["reads"].finditer(scan.code):
        start = read.start() + len(read.group(0)) - len(read.group(0).lstrip(LEADING_DELIMITERS))
        if regions is not None and not any(a <= start <= b for a, b in regions):
            continue
        if any(a <= start < b for a, b in scan.literals) or in_js_literal(scan.code, start, scan.language):
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


def reads_a_parameter(body: str, language: str, param: re.Pattern[str]) -> bool:
    for read in LANGUAGES[language]["reads"].finditer(body):
        start = read.start() + len(read.group(0)) - len(read.group(0).lstrip(LEADING_DELIMITERS))
        if in_js_literal(body, start, language):
            continue
        argument = call_argument(body, start, language)
        if param.search(argument) and not executes(body, language, start, argument):
            return True
    return False


def helper_readers(code: str, language: str) -> set[str]:
    """Functions whose body reads a path built from one of their parameters."""
    readers = set()
    if language == "shell":
        for function in SHELL_FUNCTION.finditer(code):
            body = function["body"]
            params = {n for n, v in assignments(body, "shell").items() if POSITIONAL.search(v)}
            param = re.compile(POSITIONAL.pattern + ("|" + names_pattern(params, "shell") if params else ""))
            if reads_a_parameter(body, "shell", param):
                readers.add(function["name"])
    elif language == "js":
        for name, params, body, _ in js_functions(code):
            if params:
                derived = params | grow(assignments(body, "js"), names_pattern(params, "js"), "js", None)
                if reads_a_parameter(body, "js", re.compile(names_pattern(derived, "js"))):
                    readers.add(name)
    return readers


def helper_findings(scan: Scan, relative_path: str) -> list[lib.Finding]:
    """Calls that hand an anchored path to a function whose body reads its argument."""
    found = []
    for name in helper_readers(scan.code, scan.language):
        if scan.language == "shell":
            calls = re.finditer(rf"^\s*{re.escape(name)}\b(?!\s*\(\))", scan.code, re.MULTILINE)
        else:
            calls = (
                c for c in re.finditer(rf"(?<![\w.$]){re.escape(name)}\s*\(", scan.code)
                if not re.search(r"\bfunction\s*$", scan.code[max(0, c.start() - 40) : c.start()])
            )
        for call in calls:
            argument = call_argument(scan.code, call.start(), scan.language)
            if scan.is_repository_read(argument):
                found.append(finding(relative_path, scan.code, call.start(), argument))
    return found


def finding(relative_path: str, code: str, offset: int, argument: str) -> lib.Finding:
    target = " ".join(argument.replace("\\\n", " ").split())[:TARGET_MAX]
    return lib.Finding(relative_path, lib.line_of(code, offset), "test reads", f"{target}; {MESSAGE}")


def findings_in(relative_path: str, source: str, language: str, regions, tracked_parts: frozenset[str]) -> list[lib.Finding]:
    scan = Scan(source, language, tracked_parts)
    found = read_findings(scan, relative_path, regions)
    found += helper_findings(scan, relative_path)
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
