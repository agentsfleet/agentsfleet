#!/usr/bin/env python3
"""Check customer-facing API and command help text."""

from dataclasses import dataclass
from pathlib import Path
import re
import sys

sys.dont_write_bytecode = True

ROOT = Path(__file__).resolve().parents[1]
TEXT_KEY = re.compile(r"^(?P<indent>\s*)(?:summary|description):(?:\s*(?P<value>.*))?$")
TS_DESCRIPTION = re.compile(r"\bdescription\s*:\s*([\"'`])(?P<text>.+?)\1")
TS_METHOD_DESCRIPTION = re.compile(r"\.description\(\s*([\"'`])(?P<text>.+?)\1\s*\)")
WORD = re.compile(r"\b[\w][\w'-]*\b")
SENTENCE = re.compile(r"(?<=[.!?])\s+")

PLAIN_WORDS = {
    "utilise": re.compile(r"\butilis(?:e|es|ed|ing)\b", re.IGNORECASE),
    "facilitate": re.compile(r"\bfacilitat(?:e|es|ed|ing)\b", re.IGNORECASE),
    "leverage": re.compile(r"\bleverag(?:e|es|ed|ing)\b", re.IGNORECASE),
    "orchestrate": re.compile(r"\borchestrat(?:e|es|ed|ing|ion)\b", re.IGNORECASE),
    "instantiate": re.compile(r"\binstantiat(?:e|es|ed|ing|ion)\b", re.IGNORECASE),
    "terminate": re.compile(r"\bterminat(?:e|es|ed|ing|ion|al)\b", re.IGNORECASE),
    "provision": re.compile(r"\bprovision(?:s|ed|ing)?\b", re.IGNORECASE),
    "execute": re.compile(r"\bexecut(?:e|es|ed|ing|ion)\b", re.IGNORECASE),
    "persist": re.compile(r"\bpersist(?:s|ed|ing|ent|ence)?\b", re.IGNORECASE),
    "hydrate": re.compile(r"\bhydrat(?:e|es|ed|ing|ion)\b", re.IGNORECASE),
    "artifact": re.compile(r"\bartifacts?\b", re.IGNORECASE),
}

MARKETING_WORDS = {
    word: re.compile(rf"\b{re.escape(word)}\b", re.IGNORECASE)
    for word in (
        "powerful",
        "revolutionary",
        "enterprise-grade",
        "seamless",
        "cutting-edge",
        "robust",
        "next-generation",
        "world-class",
        "intelligent",
    )
}

INTERNAL_DETAILS = (
    re.compile(r"\b(?:core|vault)\.[A-Za-z_]"),
    re.compile(r"\bfleet_execution_telemetry\b"),
    re.compile(r"\b(?:PostgreSQL|Redis|JSONB|XADD|Lua-EVAL)\b", re.IGNORECASE),
    re.compile(
        r"\b(?:middleware|row-level|implementation detail|allocation failure|"
        r"worker thread|runner plane|data plane|control plane)\b",
        re.IGNORECASE,
    ),
    re.compile(r"\b(?:src|docs)/[A-Za-z0-9_./-]+"),
    re.compile(r"\bRULE [A-Z0-9-]+\b"),
    re.compile(r"\bM[0-9]+(?:[_-][0-9]+)?\b"),
)

REMOVED_COMMANDS = {
    "agentsfleet install --from": "agentsfleet install --library <LIBRARY_ID>",
    "agentsfleet workspace add": "agentsfleet workspace create",
    "agentsfleet fleet-key add": "agentsfleet fleet-key create",
    "agentsfleet secret add": "agentsfleet secret create",
    "agentsfleet tenant provider add": "agentsfleet tenant provider create",
}

SCAN_SUFFIXES = {".zig", ".ts", ".tsx", ".js", ".jsx", ".yaml", ".yml", ".md"}


@dataclass(frozen=True)
class PublicText:
    line: int
    value: str


def issue(rule: str, path: Path, line: int, message: str) -> str:
    return f"{rule} {path}:{line}: {message}"


def clean_yaml_value(value: str) -> str:
    text = value.strip()
    if text in {">", ">-", "|", "|-", "|+"}:
        return ""
    if len(text) >= 2 and text[0] == text[-1] and text[0] in {"'", '"'}:
        return text[1:-1]
    return text


def public_yaml_text(source: str) -> list[PublicText]:
    lines = source.splitlines()
    values: list[PublicText] = []
    index = 0
    while index < len(lines):
        match = TEXT_KEY.match(lines[index])
        if not match:
            index += 1
            continue
        base_indent = len(match.group("indent"))
        chunks = [clean_yaml_value(match.group("value") or "")]
        cursor = index + 1
        while cursor < len(lines):
            candidate = lines[cursor]
            if candidate.strip() and len(candidate) - len(candidate.lstrip()) <= base_indent:
                break
            if candidate.strip():
                chunks.append(candidate.strip())
            cursor += 1
        value = " ".join(chunk for chunk in chunks if chunk)
        values.append(PublicText(index + 1, clean_yaml_value(value)))
        index = cursor
    return values


def lint_wording(path: Path, line: int, text: str) -> list[str]:
    problems: list[str] = []
    for replacement, pattern in PLAIN_WORDS.items():
        if pattern.search(text):
            problems.append(issue("DOC-05", path, line, f"use a plain word instead of '{replacement}'"))
    for word, pattern in MARKETING_WORDS.items():
        if pattern.search(text):
            problems.append(issue("DOC-07", path, line, f"remove marketing word '{word}'"))
    for sentence in SENTENCE.split(text):
        count = len(WORD.findall(sentence))
        if count > 25:
            problems.append(issue("DOC-02", path, line, f"sentence has {count} words; maximum is 25"))
    return problems


def lint_openapi_source(path: Path, source: str) -> list[str]:
    problems: list[str] = []
    for field in public_yaml_text(source):
        problems.extend(lint_wording(path, field.line, field.value))
        if re.search(r"\$[0-9]", field.value):
            problems.append(issue("DOC-23", path, field.line, "remove mutable price copy"))
        if re.search(r"\b(?:version\s+|v)[0-9]+\.[0-9]+(?:\.[0-9]+)?\b", field.value, re.IGNORECASE):
            problems.append(issue("DOC-31", path, field.line, "remove release-number prose"))
        if any(pattern.search(field.value) for pattern in INTERNAL_DETAILS):
            problems.append(issue("DOC-22", path, field.line, "state customer behavior, not implementation details"))
    return problems


def lint_removed_commands(path: Path, source: str) -> list[str]:
    problems: list[str] = []
    for removed, replacement in REMOVED_COMMANDS.items():
        for match in re.finditer(re.escape(removed), source):
            line = source.count("\n", 0, match.start()) + 1
            problems.append(issue("DOC-09", path, line, f"replace with '{replacement}'"))
    return problems


def lint_cli_source(path: Path, source: str) -> list[str]:
    problems: list[str] = []
    for pattern in (TS_DESCRIPTION, TS_METHOD_DESCRIPTION):
        for match in pattern.finditer(source):
            line = source.count("\n", 0, match.start()) + 1
            problems.extend(lint_wording(path, line, match.group("text")))
    return problems


def is_public_source(path: Path) -> bool:
    if path.suffix not in SCAN_SUFFIXES:
        return False
    name = path.name.lower()
    return not any(marker in name for marker in ("_test", ".test", ".spec"))


def files_under(relative: str) -> list[Path]:
    base = ROOT / relative
    if not base.exists():
        return []
    return [path for path in base.rglob("*") if path.is_file() and is_public_source(path)]


def lint_repository() -> list[str]:
    problems: list[str] = []
    openapi_files = sorted((ROOT / "public/openapi").rglob("*.yaml"))
    for path in openapi_files:
        relative = path.relative_to(ROOT)
        source = path.read_text(encoding="utf-8")
        problems.extend(lint_openapi_source(relative, source))
        problems.extend(lint_removed_commands(relative, source))
    for path in sorted((ROOT / "cli/src/program").rglob("*.ts")):
        relative = path.relative_to(ROOT)
        source = path.read_text(encoding="utf-8")
        problems.extend(lint_cli_source(relative, source))
    public_roots = ("src", "cli/src", "public", "ui/packages/app", "tests/fixtures")
    for path in sorted({path for root in public_roots for path in files_under(root)}):
        relative = path.relative_to(ROOT)
        source = path.read_text(encoding="utf-8", errors="replace")
        problems.extend(lint_removed_commands(relative, source))
    return sorted(set(problems))


def main() -> int:
    problems = lint_repository()
    if problems:
        print("\n".join(problems))
        print(f"documentation check failed: {len(problems)} issue(s)")
        return 1
    print("documentation check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
