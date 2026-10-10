#!/usr/bin/env python3
"""JSX rules for the dashboard, run by `check_app_source.py`.

A JSX opening tag is read to its real end: a `>` inside `onClick={() => a > b}`
or a string does not close it. Each rule then reads one attribute of one
design-system primitive.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import re
import sys

sys.dont_write_bytecode = True

import check_source_lib as lib

APP = "ui/packages/app"

# --- Action buttons follow one variant rule ----------------------------------
# `default` (no variant: the one primary action), `ghost` for every other
# non-destructive action, `destructive`, and `link` inline in prose.
DASHBOARD = f"{APP}/app/(dashboard)"
BUTTON = "Button"
PERMITTED_VARIANTS = frozenset({"ghost", "destructive", "link"})
# JSX takes either quote for a string attribute, so both spell a literal.
LITERAL_VARIANT = re.compile(r"""\bvariant=(?P<quote>["'])(?P<variant>[a-z-]+)(?P=quote)""")
DYNAMIC_VARIANT = re.compile(r"\bvariant=\{")
# Sites whose variant comes from a per-action config, audited on that config.
CONFIG_DRIVEN_SITES = frozenset({
    f"{DASHBOARD}/admin/runners/[runnerId]/components/RunnerHeader.tsx",
    f"{DASHBOARD}/w/[workspaceId]/fleets/[id]/components/KillSwitch.tsx",
})

# --- Interface primitives keep their own typeface ----------------------------
FONT_ROOTS = (f"{APP}/app", f"{APP}/components")
INTERFACE_COMPONENTS = frozenset({
    "Button", "NavItem", "PageTitle", "CardTitle", "SectionLabel",
    "SectionHeader", "DialogTitle", "Badge", "Pagination", "SelectTrigger",
})
DESIGN_IMPORT = re.compile(
    r'import\s+(?:type\s+)?\{(?P<names>[^}]*)\}\s*from\s*"@agentsfleet/design-system(?:/design-system)?"'
)
NON_INTERFACE_FONT = re.compile(r"\bfont-(?:mono|display|serif)\b|\bfont-\[")
CONSTANT = re.compile(r"\bconst (?P<name>\w+)\s*=\s*")
STRING = re.compile(r""""(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|`(?:[^`\\]|\\.)*`""")
IDENTIFIER = re.compile(r"\b[A-Za-z_]\w*\b")
CLASS_NAME = re.compile(r"\bclassName=")


class UnterminatedTag(Exception):
    """A JSX opening tag that never closes."""


@dataclass(frozen=True)
class Tag:
    name: str
    start: int
    text: str


def expression_end(source: str, start: int, closer: str) -> int:
    """Index of `closer` at depth zero from `start`, honouring braces and strings."""
    depth, quote, index = 0, "", start
    while index < len(source):
        char = source[index]
        if quote:
            if char == "\\":
                index += 1
            elif char == quote:
                quote = ""
        elif char in "\"'`":
            quote = char
        elif char in "{([":
            depth += 1
        elif char in "})]":
            if depth == 0 and char == closer:
                return index
            depth -= 1
        elif char == closer and depth == 0:
            return index
        index += 1
    return -1


def opening_tags(source: str, names: frozenset[str]) -> list[Tag]:
    """Every opening tag whose element name is in `names`."""
    tags = []
    for match in re.finditer(r"<(?P<name>[A-Z]\w*)(?![\w.])", source):
        if match["name"] not in names:
            continue
        end = expression_end(source, match.end(), ">")
        if end == -1:
            raise UnterminatedTag(f"unterminated <{match['name']} at line {lib.line_of(source, match.start())}")
        tags.append(Tag(match["name"], match.start(), source[match.start() : end + 1]))
    return tags


def tsx_files(root: Path, relative_dir: str) -> list[Path]:
    return lib.source_files(root / relative_dir, (".tsx", ".jsx"))


def check_button_variants(root: Path) -> list[lib.Finding]:
    found = []
    for path in tsx_files(root, DASHBOARD):
        site = lib.relative(root, path)
        source = lib.blank_js_comments(path.read_text(encoding="utf-8"))
        try:
            tags = opening_tags(source, frozenset({BUTTON}))
        except UnterminatedTag as error:
            found.append(lib.Finding(site, 1, "app-button-variant", str(error)))
            continue
        for tag in tags:
            literal = LITERAL_VARIANT.search(tag.text)
            line = lib.line_of(source, tag.start)
            if literal and literal["variant"] not in PERMITTED_VARIANTS:
                found.append(lib.Finding(site, line, "app-button-variant", f'variant="{literal["variant"]}"; an action is default, ghost, destructive or link'))
            elif not literal and DYNAMIC_VARIANT.search(tag.text) and site not in CONFIG_DRIVEN_SITES:
                found.append(lib.Finding(site, line, "app-button-variant", "variant={…} outside a config-driven site"))
    return found


def imported_primitives(source: str) -> dict[str, str]:
    """Local name → design-system export, for every named import."""
    names = {}
    for statement in DESIGN_IMPORT.finditer(source):
        for item in statement["names"].split(","):
            parts = item.replace("type ", "").split(" as ")
            if parts[0].strip():
                names[parts[-1].strip()] = parts[0].strip()
    return names


def constants(source: str) -> dict[str, str]:
    """Each `const NAME = …` and the text of its initializer."""
    found = {}
    for match in CONSTANT.finditer(source):
        end = expression_end(source, match.end(), ";")
        newline = source.find("\n", match.end())
        stop = end if end != -1 and (newline == -1 or end < newline or source[match.end()] in "[({`") else newline
        found.setdefault(match["name"], source[match.end() : stop if stop != -1 else len(source)])
    return found


def overrides_font(expression: str, known: dict[str, str], seen: frozenset[str] = frozenset()) -> bool:
    if any(NON_INTERFACE_FONT.search(literal.group(0)) for literal in STRING.finditer(expression)):
        return True
    bare = STRING.sub("", expression)
    return any(
        overrides_font(known[name], known, seen | {name})
        for name in IDENTIFIER.findall(bare) if name in known and name not in seen
    )


def class_name_value(tag: str) -> str | None:
    match = CLASS_NAME.search(tag)
    if not match:
        return None
    start = match.end()
    if tag[start] == "{":
        end = expression_end(tag, start + 1, "}")
        return tag[start + 1 : end]
    literal = STRING.match(tag, start)
    return literal.group(0) if literal else None


def font_overrides(source: str) -> list[tuple[int, str]]:
    """(offset, primitive) for each interface primitive given another typeface."""
    code = lib.blank_js_comments(source)
    primitives = {local: name for local, name in imported_primitives(code).items() if name in INTERFACE_COMPONENTS}
    known = constants(code)
    overrides = []
    for tag in opening_tags(code, frozenset(primitives)):
        value = class_name_value(tag.text)
        if value is not None and overrides_font(value, known):
            overrides.append((tag.start, primitives[tag.name]))
    return overrides


def check_interface_fonts(root: Path) -> list[lib.Finding]:
    found = []
    for base in FONT_ROOTS:
        for path in tsx_files(root, base):
            source = path.read_text(encoding="utf-8")
            try:
                overrides = font_overrides(source)
            except UnterminatedTag as error:
                found.append(lib.Finding(lib.relative(root, path), 1, "app-interface-font", str(error)))
                continue
            for offset, primitive in overrides:
                found.append(lib.Finding(lib.relative(root, path), lib.line_of(source, offset), "app-interface-font", f"{primitive} keeps the interface typeface; no font-mono, font-display or font-serif"))
    return found
