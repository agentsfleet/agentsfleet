#!/usr/bin/env python3
"""Record what a coverage producer measured, and refuse it when it no longer fits.

The Zig coverage union is graded from reports two different lanes wrote. Once
those lanes stopped being one recipe, "the reports are on disk" stopped being
evidence that they describe the build in front of you: a `coverage/zig/` tree
survives a branch switch, a toolchain bump and a rebuild, and the union grader
reads whatever it finds.

So each producer writes a manifest naming the components it collected and the
four things that decide whether those numbers still mean anything:

  * `source_digest` — the working-tree sources that reach the measured binaries.
    Different sources, different lines; a rate over the old ones is fiction.
  * `toolchain` — the Zig identity that built them. Codegen decides which lines
    exist at all.
  * `graph_digest` — the component inventory, required components, required
    roots and floors in force. A union graded against a different inventory is
    graded against a different question.
  * `environment` — the platform, because it decides which components are even
    required (`make/test.mk` carries two lists for exactly that reason).

`validate` recomputes all four and refuses on the first that disagrees, naming
the field and both values. It also refuses a manifest that failed, that measured
a component at zero lines, that points at a report which changed on disk since
it was written, or that came from a narrowed run — a `TEST_FILTER` run measures
a subset and cannot support a floor.

The union check is the other half: every component in the inventory must appear
exactly once across the manifests. An omission would shrink the denominator
silently, and a duplicate would mean two lanes ran the same binary, which is the
duplication this whole split exists to remove.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import subprocess
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

from check_zig_coverage import find_report

# Every provenance field, in the order failures are reported. One definition
# site: `record` writes exactly these and `validate` recomputes exactly these,
# so a field added here cannot be validated by only one of them.
PROVENANCE_FIELDS = ("source_digest", "toolchain", "graph_digest", "environment")
OUTCOME_PASSED = "passed"
READ_CHUNK_BYTES = 1 << 20


def file_digest(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(READ_CHUNK_BYTES), b""):
            digest.update(chunk)
    return digest.hexdigest()


def source_files(repo_root: Path, paths: list[str]) -> list[Path]:
    """Every file under `paths`, walked off disk rather than out of the index.

    Deliberately not `git ls-files`. The grade job runs in a container against
    an artifact and a checkout that may have arrived as a tarball with no `.git`
    at all, and a digest that cannot be computed there is a gate that cannot
    run. Walking also counts a brand new uncommitted module, which an
    index-based digest would call identical to the tree without it.

    These paths hold sources only — no build output lands in them — so there is
    nothing here to exclude.
    """
    found: list[Path] = []
    for entry in paths:
        candidate = repo_root / entry
        if candidate.is_file():
            found.append(candidate)
        elif candidate.is_dir():
            found.extend(child for child in candidate.rglob("*") if child.is_file())
    return sorted(found)


def source_digest(repo_root: Path, paths: list[str]) -> str:
    digest = hashlib.sha256()
    for candidate in source_files(repo_root, paths):
        digest.update(str(candidate.relative_to(repo_root)).encode("utf-8"))
        digest.update(file_digest(candidate).encode("utf-8"))
    return digest.hexdigest()


def toolchain_identity() -> str:
    """The compiler AND the instrument. Codegen decides which lines exist;
    kcov decides which of them get read — a kcov bump behind an unchanged
    `zig version` once dropped 534 of 558 files from the union silently."""
    zig = subprocess.run(["zig", "version"], capture_output=True, text=True, check=True)
    kcov = subprocess.run(["kcov", "--version"], capture_output=True, text=True, check=True)
    return f"zig={zig.stdout.strip()} {kcov.stdout.strip()}"


def graph_digest(parts: list[str]) -> str:
    """Digest the inventory and floor arguments, whitespace-normalised.

    Make hands these through as single strings whose internal spacing depends on
    how the variable was written, so `agentsfleetd=90 runner=87` and the same
    pair with two spaces must not read as two different graphs.
    """
    digest = hashlib.sha256()
    for part in parts:
        digest.update(" ".join(part.split()).encode("utf-8"))
        digest.update(b"\0")
    return digest.hexdigest()


def environment_identity() -> str:
    return f"{platform.system()}-{platform.machine()}"


def measured_lines(report: Path) -> int:
    """Lines the report carries, not lines the grader will keep.

    Deliberately a raw count. The denominator rules — test bodies, test support,
    inline `test {}` blocks — belong to `check_zig_coverage_floors` and stay
    there; duplicating them here would give two answers to one question. All
    this needs to know is whether the component collected anything at all.
    """
    return sum(1 for _ in ET.parse(report).getroot().iter("line"))


def current_provenance(repo_root: Path, source_paths: list[str], graph: list[str]) -> dict[str, str]:
    return {
        "source_digest": source_digest(repo_root, source_paths),
        "toolchain": toolchain_identity(),
        "graph_digest": graph_digest(graph),
        "environment": environment_identity(),
    }


def split_manifest_argument(value: str) -> tuple[str, Path]:
    """`producer:path` — the producer name is what a failure message can act on.

    "manifest .tmp/verification/unit.json is missing" tells you a file is not
    there; "test-coverage-zig evidence is missing" tells you which command to
    run.
    """
    producer, separator, path = value.partition(":")
    if not separator or not producer or not path:
        raise argparse.ArgumentTypeError(f"expected producer:path, got {value!r}")
    return producer, Path(path)


def load_manifest(producer: str, path: Path) -> tuple[dict | None, list[str]]:
    if not path.exists():
        return None, [f"{producer} evidence is missing at {path} — that lane did not run"]
    try:
        with path.open(encoding="utf-8") as handle:
            return json.load(handle), []
    except (json.JSONDecodeError, UnicodeDecodeError) as error:
        return None, [f"{producer} evidence at {path} is not readable JSON: {error}"]


def check_provenance(producer: str, manifest: dict, current: dict[str, str]) -> list[str]:
    failures = []
    for field in PROVENANCE_FIELDS:
        recorded = manifest.get(field)
        if recorded != current[field]:
            failures.append(
                f"{producer} {field} mismatch: recorded={recorded} current={current[field]}"
            )
    return failures


def check_usability(producer: str, manifest: dict) -> list[str]:
    failures = []
    outcome = manifest.get("outcome")
    if outcome != OUTCOME_PASSED:
        failures.append(f"{producer} evidence records outcome={outcome!r}, not {OUTCOME_PASSED!r}")
    if manifest.get("filtered"):
        failures.append(
            f"{producer} evidence came from a narrowed run (filtered) — "
            "a subset of the suite cannot support a coverage floor"
        )
    if not manifest.get("components"):
        failures.append(f"{producer} evidence records no components")
    return failures


def check_components(producer: str, manifest: dict, repo_root: Path, seen: dict) -> list[str]:
    failures = []
    for component in manifest.get("components", []):
        name = component.get("name")
        seen.setdefault(name, []).append(producer)
        report = repo_root / component.get("report", "")
        if component.get("measured_lines", 0) <= 0:
            failures.append(f"{producer} component {name} measured 0 lines")
        if not report.is_file():
            failures.append(f"{producer} component {name} report is missing at {report}")
            continue
        if file_digest(report) != component.get("digest"):
            failures.append(
                f"{producer} component {name} report changed on disk after it was recorded"
            )
    return failures


def check_union(seen: dict[str, list[str]], expected: list[str]) -> list[str]:
    failures = []
    for name in expected:
        producers = seen.get(name, [])
        if not producers:
            failures.append(f"component {name} is in the inventory but no lane produced it")
        elif len(producers) > 1:
            failures.append(
                f"component {name} was produced more than once, by {' and '.join(producers)}"
            )
    for name in sorted(set(seen) - set(expected)):
        failures.append(f"component {name} is not in the inventory")
    return failures


def relative_report(report: Path, repo_root: Path) -> Path:
    """Record the report path relative to the repository when it sits inside it.

    Continuous Integration hands the grade job a fresh checkout and an unpacked
    artifact, so an absolute path recorded on the producer's runner would name a
    directory that job does not have. A redirected coverage directory — what the
    lane tests use — has no relative form, and stays absolute; `repo_root / p`
    resolves either, because joining an absolute path discards the left side.
    """
    return report.relative_to(repo_root) if report.is_relative_to(repo_root) else report


def record(args: argparse.Namespace) -> int:
    components = []
    for name in args.components:
        report = find_report(args.coverage_dir / name)
        components.append(
            {
                "name": name,
                "report": str(relative_report(report, args.repo_root)),
                "measured_lines": measured_lines(report),
                "digest": file_digest(report),
            }
        )
    manifest = {
        "producer": args.producer,
        "filtered": bool(args.filtered),
        "components": components,
        "outcome": args.outcome,
        **current_provenance(args.repo_root, args.source_path, args.graph),
    }
    args.manifest.parent.mkdir(parents=True, exist_ok=True)
    with args.manifest.open("w", encoding="utf-8") as handle:
        json.dump(manifest, handle, indent=2, sort_keys=True)
        handle.write("\n")
    filtered_note = " (filtered — not usable for a floor)" if args.filtered else ""
    print(
        f"✓ [evidence] {args.producer} recorded "
        f"{len(components)} component(s){filtered_note} → {args.manifest}"
    )
    return 0


def validate(args: argparse.Namespace) -> int:
    current = current_provenance(args.repo_root, args.source_path, args.graph)
    failures: list[str] = []
    seen: dict[str, list[str]] = {}
    for producer, path in args.manifests:
        manifest, load_failures = load_manifest(producer, path)
        failures.extend(load_failures)
        if manifest is None:
            continue
        failures.extend(check_provenance(producer, manifest, current))
        failures.extend(check_usability(producer, manifest))
        failures.extend(check_components(producer, manifest, args.repo_root, seen))
    failures.extend(check_union(seen, args.expected_components))
    for failure in failures:
        print(f"✗ [evidence] {failure}", file=sys.stderr)
    if failures:
        return 1
    print(
        f"✓ [evidence] {len(args.manifests)} manifest(s) match this build; "
        f"{len(args.expected_components)} component(s) each produced exactly once"
    )
    return 0


def add_shared_arguments(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--repo-root", type=Path, required=True)
    parser.add_argument("--source-path", action="append", required=True)
    parser.add_argument("--graph", action="append", required=True)


def parse_args(argv: list[str] | None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)

    recorder = commands.add_parser("record", help="write one producer's evidence manifest")
    add_shared_arguments(recorder)
    recorder.add_argument("--producer", required=True)
    recorder.add_argument("--manifest", type=Path, required=True)
    recorder.add_argument("--coverage-dir", type=Path, required=True)
    recorder.add_argument("--component", action="append", required=True, dest="components")
    recorder.add_argument("--outcome", default=OUTCOME_PASSED)
    recorder.add_argument("--filtered", action="store_true")
    recorder.set_defaults(handler=record)

    validator = commands.add_parser("validate", help="refuse evidence that no longer fits")
    add_shared_arguments(validator)
    validator.add_argument(
        "--manifest", action="append", required=True, dest="manifests",
        type=split_manifest_argument,
    )
    validator.add_argument(
        "--expect-component", action="append", required=True, dest="expected_components",
    )
    validator.set_defaults(handler=validate)

    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        return args.handler(args)
    except (FileNotFoundError, ET.ParseError, subprocess.CalledProcessError) as error:
        print(f"✗ [evidence] {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
