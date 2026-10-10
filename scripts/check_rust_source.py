#!/usr/bin/env python3
"""Rules over the Rust tree that no compiler pass and no unit test can see.

Each rule compares two things the repository declares in separate places and
that must agree: a schema file with the migration list, a component name with
every other component name, a default with the benchmark behind it, a bundle's
prose with the gates the daemon still has.

    python3 scripts/check_rust_source.py [--root <dir>]

Exit 0 clean, 1 with one line per finding, 2 on a usage error.
"""

from __future__ import annotations

from collections import defaultdict
import json
from pathlib import Path
import re
import sys

sys.dont_write_bytecode = True

import check_source_lib as lib

# --- One component name, one type -------------------------------------------
# utoipa keys a document's components by name alone; a second type registered
# under a taken name replaces the first without a word. The daemon's document
# registers both crates' schemas, so a collision across them is as silent as
# one inside either.
SCHEMA_CRATES = ("rustd/crates/afd_wire/src", "rustd/crates/afd_api_wire/src")
DERIVE = "derive(utoipa::ToSchema)"
ALIAS = "schema(as = "
MANUAL_IMPL = "impl utoipa::ToSchema for "
ITEM = re.compile(r"^(?:pub(?:\([^)]*\))? )?(?:struct|enum) (?P<name>\w+)")
MANUAL_NAME = re.compile(r"^impl utoipa::ToSchema for (?P<name>\w+)")

# --- The migration list is the schema directory ----------------------------
MIGRATION_LIST = "rustd/crates/afd_db/src/migration.rs"
MIGRATION_ENTRY = re.compile(r'migration!\("(?P<file>[^"]+)"\)')
SCHEMA_GLOB = "schema/*.sql"

# --- A bundle never promises a retired gate --------------------------------
BUNDLES = "tests/fixtures/fleetbundle"
BUNDLE_DOCUMENTS = ("SKILL.md", "TRIGGER.md")
# The per-event approval card the daemon retired: the standing integration
# grant authorises a repository write, so a bundle promising the card tells its
# model about a gate that never opens.
RETIRED_GATE = "approval card"

# --- The stream ceiling is a measured rung ----------------------------------
KNOBS = "rustd/crates/agentsfleetd/src/preflight/knobs.rs"
STREAM_DEFAULT = re.compile(r"\bconst SSE_MAX_STREAMS_DEFAULT: usize = (?P<value>[0-9_]+);")
TAIL_BASELINE = "bench/baselines/tail.rig.json"
P95_BOUND_MS = 250.0
HEAP_BOUND_BYTES = 256 * 1024 * 1024
UNDELIVERED_BOUND = 0


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


def check_migration_list(root: Path) -> list[lib.Finding]:
    listing = (root / MIGRATION_LIST).read_text(encoding="utf-8") if (root / MIGRATION_LIST).is_file() else ""
    listed = [match["file"] for match in MIGRATION_ENTRY.finditer(listing)]
    tracked = {Path(path).name for path in lib.tracked_files(root, SCHEMA_GLOB)}
    if not listed or not tracked:
        return [lib.Finding(MIGRATION_LIST, 1, "rust-migration-list", "no migration entry or no tracked schema file was read")]
    findings = [
        lib.Finding(f"schema/{name}", 1, "rust-migration-list", f"no `migration!(\"{name}\")` entry in {MIGRATION_LIST}; it would never run")
        for name in sorted(tracked - set(listed))
    ]
    findings += [
        lib.Finding(MIGRATION_LIST, 1, "rust-migration-list", f"`{name}` is not a tracked schema file")
        for name in sorted(set(listed) - tracked)
    ]
    findings += [
        lib.Finding(MIGRATION_LIST, 1, "rust-migration-list", f"`{name}` is listed twice")
        for name in sorted({name for name in listed if listed.count(name) > 1})
    ]
    return findings


def check_bundle_prose(root: Path) -> list[lib.Finding]:
    documents = sorted(p for name in BUNDLE_DOCUMENTS for p in (root / BUNDLES).glob(f"*/{name}"))
    if not documents:
        return [lib.Finding(BUNDLES, 1, "rust-bundle-retired-gate", "no bundle document found to read; the fixtures moved")]
    findings = []
    for document in documents:
        for number, line in enumerate(document.read_text(encoding="utf-8").splitlines(), start=1):
            if RETIRED_GATE in line.lower():
                findings.append(lib.Finding(lib.relative(root, document), number, "rust-bundle-retired-gate", f"the bundle promises the retired {RETIRED_GATE}"))
    return findings


def check_stream_ceiling(root: Path) -> list[lib.Finding]:
    def finding(message: str) -> list[lib.Finding]:
        return [lib.Finding(KNOBS, 1, "rust-stream-ceiling", message)]

    knobs = (root / KNOBS).read_text(encoding="utf-8") if (root / KNOBS).is_file() else ""
    declared = STREAM_DEFAULT.search(knobs)
    if not declared or not (root / TAIL_BASELINE).is_file():
        return finding(f"SSE_MAX_STREAMS_DEFAULT and {TAIL_BASELINE} must both exist")
    default = int(declared["value"].replace("_", ""))
    baseline = json.loads((root / TAIL_BASELINE).read_text(encoding="utf-8"))
    series = baseline.get("series", {})
    measured = baseline.get("measurements", {})
    rungs = series.get("ladder_streams", [])
    p95 = series.get("stream_receive_p95_ms", [])
    if len(p95) != len(rungs) or default not in rungs:
        return finding(f"{default} streams is not a rung {TAIL_BASELINE} measured with its p95")
    at = rungs.index(default)
    live, heap = series.get("streams_live", []), series.get("heap_bytes_per_stream", [])
    if at >= min(len(live), len(heap)) or live[at] != default:
        return finding(f"the {default}-stream rung did not reach every stream")
    if measured.get("frames_undelivered", 1) > UNDELIVERED_BOUND or measured.get("streams_unreached", 1) > UNDELIVERED_BOUND:
        return finding("the baseline lost frames or streams")
    if p95[at] >= P95_BOUND_MS or heap[at] * default >= HEAP_BOUND_BYTES:
        return finding(f"the {default}-stream rung missed its p95 or heap bound")
    return []


RULES = (check_schema_names, check_migration_list, check_bundle_prose, check_stream_ceiling)


def lint(root: Path) -> list[lib.Finding]:
    return [finding for rule in RULES for finding in rule(root)]


if __name__ == "__main__":
    raise SystemExit(lib.run(sys.argv[1:], "rust source", lint))
