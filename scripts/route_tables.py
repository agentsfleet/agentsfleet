"""The architecture page's route tables, held against the published routes.

`docs/architecture/api_resources.md` is where a client author reads the
workspace-chat cutover: seventeen new person routes, seven changed, two
removed with no alias, and four authoring machine routes. `openapi_coverage`
pins the published document to the router; this holds the page to the
document, so a route renamed in code and not on the page, or a removed route
that kept a handler, is caught.

The expected counts are written out rather than read from the headings, so a
table that lost a row and a heading edited to match still fail.
"""

from __future__ import annotations

PAGE = "docs/architecture/api_resources.md"

NEW_PERSON = "### New person routes: "
CHANGED_PERSON = "### Changed person routes: "
REMOVED_PERSON = "### Removed person routes: "
MACHINE = "Four new machine routes under "

# The cutover's shape, as the spec's goal states it.
EXPECTED = {NEW_PERSON: 17, CHANGED_PERSON: 7, REMOVED_PERSON: 2}
MACHINE_ROUTES = 4

# The page's path prefixes: `D`, `F` and `G` all sit under `W`.
PREFIXES = {
    "W": "/v1/workspaces/{workspace_id}",
    "D": "W/drafts/{draft_id}",
    "F": "W/fleets/{fleet_id}",
    "G": "W/fleet-libraries/{library_id}",
}
HTTP_METHODS = ("get", "put", "post", "delete", "patch", "head", "options")
TABLE_EDGE = "|"
SEPARATOR = "---"
BACKTICK = "`"

RULE_COUNT = "doc-route-table-count"
RULE_UNSERVED = "doc-route-unserved"
RULE_REMOVED = "doc-route-removed-served"

Operation = tuple[str, str]


def expand(resource: str) -> str:
    """`W/models` → `/v1/workspaces/{workspace_id}/models`, until no prefix is left."""
    head, _, rest = resource.partition("/")
    if head not in PREFIXES:
        return resource
    expanded = expand(PREFIXES[head])
    return f"{expanded}/{rest}" if rest else expanded


def rows(page: str, marker: str) -> list[list[str]]:
    """The table rows under the line starting with `marker`, header excluded."""
    lines = page.splitlines()
    start = next((i for i, line in enumerate(lines) if line.startswith(marker)), None)
    if start is None:
        return []
    body = lines[start + 1:]
    first = next((i for i, line in enumerate(body) if line.startswith(TABLE_EDGE)), len(body))
    table = []
    for line in body[first:]:
        if not line.startswith(TABLE_EDGE):
            break
        cells = [cell.strip() for cell in line.split(TABLE_EDGE)]
        if len(cells) > 1 and not cells[1].startswith(SEPARATOR):
            table.append(cells)
    return table[1:]


def person(page: str, marker: str) -> list[Operation]:
    """A person table's operations: `| N1 | GET W/models | … |`."""
    operations = []
    for cells in rows(page, marker):
        method, _, resource = (cells[2] if len(cells) > 2 else "").partition(" ")
        operations.append((expand(resource.strip()), method))
    return operations


def machine(page: str) -> list[Operation]:
    """The machine table's operations: ``| POST `/renew` | … |`` under the marker's base path."""
    line = next((line for line in page.splitlines() if line.startswith(MACHINE)), "")
    parts = line.split(BACKTICK)
    base = parts[1] if len(parts) > 1 else ""
    operations = []
    for cells in rows(page, MACHINE):
        method, _, suffix = (cells[1] if len(cells) > 1 else "").partition(" ")
        operations.append((f"{base}{suffix.strip().strip(BACKTICK)}", method))
    return operations


def declared(page: str, marker: str) -> int | None:
    """The count a heading declares: `### New person routes: 17` → 17."""
    for line in page.splitlines():
        if line.startswith(marker):
            count = line[len(marker):].strip()
            return int(count) if count.isdigit() else None
    return None


def served(document: dict) -> set[Operation]:
    """Every (path, METHOD) the published document serves."""
    paths = document.get("paths", {}) if isinstance(document, dict) else {}
    return {
        (path, method.upper())
        for path, item in paths.items() if isinstance(item, dict)
        for method in item if method in HTTP_METHODS
    }


def missing(operations: list[Operation], routes: set[Operation]) -> list[str]:
    return [f"{method} {path}" for path, method in operations if (path, method) not in routes]


def findings(page: str, document: dict) -> list[tuple[str, str]]:
    """(rule, message) for every way the page and the document disagree."""
    routes = served(document)
    found = []
    for marker, expected in EXPECTED.items():
        heading = marker.strip("# :")
        table = person(page, marker)
        if len(table) != expected or declared(page, marker) != expected:
            found.append((RULE_COUNT, f"{heading}: {len(table)} rows, heading {declared(page, marker)}, expected {expected}"))
    authoring = machine(page)
    if len(authoring) != MACHINE_ROUTES:
        found.append((RULE_COUNT, f"machine routes: {len(authoring)} rows, expected {MACHINE_ROUTES}"))
    owed = person(page, NEW_PERSON) + person(page, CHANGED_PERSON) + authoring
    found += [(RULE_UNSERVED, f"the page names `{route}`, which is not served") for route in missing(owed, routes)]
    for path, method in person(page, REMOVED_PERSON):
        aliases = sorted(f"{m} {p}" for p, m in routes if p == path)
        if aliases:
            found.append((RULE_REMOVED, f"{method} {path} was removed with no alias, yet {', '.join(aliases)} is served"))
    return found
