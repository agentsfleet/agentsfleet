#!/usr/bin/env python3
"""The tables and columns the design doc signs off are the ones schema/ creates.

The `test_arch_schema_layout_matches_doc` row of `check_architecture_doc.sh`:

    python3 scripts/check_architecture_doc_schema.py <postgres_schemas.md> <schema dir>

Exit 0 with a one-line count when both sides list the same tables with the
same columns, 1 with one line per difference (naming the table and column), 2
on a usage error.

The doc side is every pipe-table row whose first cell is a backticked table
name and whose third is the backticked column list, under a `## <schema>`
heading or naming its schema itself (`vault.secrets`). A parenthesised note
after a column (`id (=1)`) is not part of its name.

The schema side is every `CREATE TABLE <schema>.<table> (...)`, its columns
being the elements that are not a table constraint. An `ALTER TABLE` that adds,
drops or renames a column, and a `DROP TABLE`, are reported rather than
replayed: a slot that reshapes a table after its CREATE is a layout this check
cannot read, and a check that guesses passes the drift it exists to catch.
"""

from __future__ import annotations

from pathlib import Path
import re
import sys

sys.dont_write_bytecode = True

EXIT_CLEAN, EXIT_FINDINGS, EXIT_USAGE = 0, 1, 2

Layout = dict[str, set[str]]

HEADING = "## "
CREATE = re.compile(r"\bCREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?([a-z_]+\.[a-z_]+)\s*\(", re.IGNORECASE)
ALTER = re.compile(r"\bALTER\s+TABLE\s+(?:IF\s+EXISTS\s+)?(?:ONLY\s+)?([a-z_]+\.[a-z_]+)\s+([^;]*)", re.IGNORECASE)
RESHAPES = re.compile(r"\b(?:ADD\s+COLUMN|DROP\s+COLUMN|RENAME)\b|\bADD\s+(?!CONSTRAINT\b)", re.IGNORECASE)
DROP = re.compile(r"\bDROP\s+TABLE\b", re.IGNORECASE)
# The first word of a table element that is a constraint, never a column.
TABLE_CONSTRAINTS = frozenset({"constraint", "primary", "unique", "check", "foreign", "exclude", "like"})


def backticked(cell: str) -> str | None:
    """The text between the first pair of backticks, when the cell opens with one."""
    if not cell.startswith("`"):
        return None
    return cell[1:].split("`", 1)[0]


def designed(doc: str) -> tuple[Layout, list[str]]:
    layout: Layout = {}
    problems = []
    schema = ""
    for line in doc.splitlines():
        if line.startswith(HEADING):
            words = line[len(HEADING):].split()
            schema = words[0] if words else ""
            continue
        cells = [cell.strip() for cell in line.split("|")]
        if len(cells) < 4:
            continue
        table, columns = backticked(cells[1]), backticked(cells[3])
        if table is None or columns is None:
            continue
        name = table if "." in table else f"{schema}.{table}"
        if name in layout:
            problems.append(f"{name} is listed twice in the doc")
        layout[name] = {column.split(" (", 1)[0].strip() for column in columns.split(",")}
    return layout, problems


def without_comments(sql: str) -> str:
    """The text with every `--` comment blanked; a quoted `--` is kept."""
    out, quoted, index = [], False, 0
    while index < len(sql):
        char = sql[index]
        if char == "'":
            quoted = not quoted
        elif not quoted and sql.startswith("--", index):
            index = sql.find("\n", index)
            if index < 0:
                break
            continue
        out.append(char)
        index += 1
    return "".join(out)


def top_level_elements(sql: str, open_paren: int) -> list[str]:
    """The comma-separated elements inside the parenthesis opening at `open_paren`."""
    elements, depth, quoted, start = [], 0, False, open_paren + 1
    for index in range(open_paren, len(sql)):
        char = sql[index]
        if char == "'":
            quoted = not quoted
        elif quoted:
            continue
        elif char == "(":
            depth += 1
        elif char == ")":
            depth -= 1
            if depth == 0:
                elements.append(sql[start:index])
                return elements
        elif char == "," and depth == 1:
            elements.append(sql[start:index])
            start = index + 1
    raise ValueError("the column list never closes")


def built(schema_dir: Path) -> tuple[Layout, dict[str, str], list[str]]:
    layout: Layout = {}
    homes: dict[str, str] = {}
    problems = []
    for path in sorted(schema_dir.glob("*.sql")):
        sql = without_comments(path.read_text(encoding="utf-8"))
        where = path.as_posix()
        for create in CREATE.finditer(sql):
            name = create.group(1).lower()
            try:
                elements = top_level_elements(sql, create.end() - 1)
            except ValueError as fault:
                problems.append(f"{where}: CREATE TABLE {name}: {fault}")
                continue
            if name in layout:
                problems.append(f"{name} is created twice: {homes[name]} and {where}")
            columns = {element.split()[0].strip('"') for element in elements if element.split()}
            layout[name] = {column for column in columns if column.lower() not in TABLE_CONSTRAINTS}
            homes[name] = where
        for alter in ALTER.finditer(sql):
            if RESHAPES.search(alter.group(2)):
                problems.append(f"{where}: ALTER TABLE {alter.group(1)} reshapes columns after the CREATE")
        if DROP.search(sql):
            problems.append(f"{where}: DROP TABLE removes a table after its CREATE")
    return layout, homes, problems


def differences(doc_path: str, design: Layout, schema: Layout, homes: dict[str, str]) -> list[str]:
    found = []
    for name in sorted(schema.keys() - design.keys()):
        found.append(f"{name} is created in {homes[name]} but {doc_path} does not list it")
    for name in sorted(design.keys() - schema.keys()):
        found.append(f"{name} is listed in {doc_path} but no schema slot creates it")
    for name in sorted(design.keys() & schema.keys()):
        for column in sorted(schema[name] - design[name]):
            found.append(f"{name}.{column} is in {homes[name]} but not in {doc_path}")
        for column in sorted(design[name] - schema[name]):
            found.append(f"{name}.{column} is in {doc_path} but not in {homes[name]}")
    return found


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print(f"usage: {Path(sys.argv[0]).name} <postgres_schemas.md> <schema dir>", file=sys.stderr)
        return EXIT_USAGE
    doc_path, schema_dir = argv[0], Path(argv[1])
    design, problems = designed(Path(doc_path).read_text(encoding="utf-8"))
    schema, homes, schema_problems = built(schema_dir)
    problems += schema_problems
    if not design:
        problems.append(f"{doc_path} lists no table — the doc moved or its table shape changed")
    if not schema:
        problems.append(f"{schema_dir} creates no table — the directory moved or is empty")
    problems += differences(doc_path, design, schema, homes)
    for problem in problems:
        print(problem)
    if problems:
        return EXIT_FINDINGS
    columns = sum(len(names) for names in design.values())
    print(f"{len(design)} tables and {columns} columns match between {doc_path} and {schema_dir}")
    return EXIT_CLEAN


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
