#!/usr/bin/env python3
"""Rules over the Rust tree that neither ast-grep nor a Rust test can see.

Most rules moved out. The single-file type rules are ast-grep rules
(`.ast-grep/rules/rust/rust-integer-millis.yml`, `rust-untyped-row.yml`, run
by `make lint-source`), and two cross-file rules are Rust tests: the migration
list against `schema/` (`afd_db` `tests/migrations.rs`) and the stream ceiling
against its tail baseline (`agentsfleetd` `preflight/stream_ceiling_tests.rs`).

Two stay here. One component name, one type: `afd_api`'s
`every_component_name_is_one_type` test reads one document per handler, so two
colliding types the same handler reaches collapse before it sees them; this
scan reads every declaration in the two wire crates. And finding no
`rustd/crates/**/*.rs` file is a finding, because a moved crate root would turn
every Rust source rule green without reading a line.

    python3 scripts/check_rust_source.py [--root <dir>]

Exit 0 clean, 1 with one line per finding, 2 on a usage error.
"""

from __future__ import annotations

from collections import defaultdict
from pathlib import Path
import re
import sys

sys.dont_write_bytecode = True

import check_source_lib as lib

CRATES = "rustd/crates"

# --- One component name, one type -------------------------------------------
# utoipa keys a document's components by name alone; a second type registered
# under a taken name replaces the first without a word. The daemon's document
# registers both crates' schemas, so a collision across them is as silent as
# one inside either.
SCHEMA_CRATES = ("rustd/crates/afd_wire/src", "rustd/crates/afd_api_wire/src")
DERIVE = "derive(utoipa::ToSchema)"
ALIAS = "schema(as = "
ITEM = re.compile(r"^(?:pub(?:\([^)]*\))? )?(?:struct|enum) (?P<name>\w+)")
MANUAL_NAME = re.compile(r"^impl utoipa::ToSchema for (?P<name>\w+)")


def published_name(lines: list[str], derive_at: int) -> str | None:
    """The name a derive publishes: its alias, else the type's own name."""
    alias = None
    open_attribute = False
    for raw in lines[derive_at + 1 :]:
        line = raw.strip()
        if ALIAS in line:
            name = line.split(ALIAS, 1)[1].rstrip(")]")
            alias = name.rsplit("::", 1)[-1]
        if open_attribute:
            open_attribute = not line.endswith("]")
            continue
        if line.startswith("#"):
            open_attribute = not line.endswith("]")
            continue
        if line.startswith("//") or not line:
            continue
        item = ITEM.match(line)
        return (alias or item["name"]) if item else None
    return None


def check_schema_names(root: Path) -> list[lib.Finding]:
    owners: dict[str, list[str]] = defaultdict(list)
    for crate in SCHEMA_CRATES:
        for path in lib.source_files(root / crate, (".rs",), include_tests=True):
            lines = path.read_text(encoding="utf-8").splitlines()
            for at, line in enumerate(lines):
                manual = MANUAL_NAME.match(line.strip())
                name = manual["name"] if manual else (published_name(lines, at) if DERIVE in line else None)
                if manual or DERIVE in line:
                    owners[name or "<a derive with no item>"].append(f"{lib.relative(root, path)}:{at + 1}")
    if not owners:
        return [lib.Finding(SCHEMA_CRATES[0], 1, "rust-schema-name", "no ToSchema derive was read; the scan is broken, not the tree")]
    findings = []
    for name, sites in sorted(owners.items()):
        if len(sites) > 1:
            first_path, first_line = sites[1].rsplit(":", 1)
            findings.append(lib.Finding(first_path, int(first_line), "rust-schema-name", f"`{name}` is also published at {', '.join(s for s in sites if s != sites[1])}; utoipa keeps only one"))
    return findings


def check_sources_exist(root: Path) -> list[lib.Finding]:
    if lib.source_files(root / CRATES, (".rs",), include_tests=True):
        return []
    return [lib.Finding(CRATES, 1, "rust-source-read", f"no `{CRATES}/**/*.rs` file was found; every Rust source rule would pass without reading one")]


RULES = (check_schema_names, check_sources_exist)


def lint(root: Path) -> list[lib.Finding]:
    return [finding for rule in RULES for finding in rule(root)]


if __name__ == "__main__":
    raise SystemExit(lib.run(sys.argv[1:], "rust source", lint))
