#!/usr/bin/env python3
"""Merge per-component kcov Cobertura reports and gate the merged line rate.

`kcov --merge` is a black box that fails silently. On Linux it returned a report
containing only the three `src/lib` components — 24 files, 861 lines — while the
same command on macOS merged all six for 558 files and 31,259 lines. Both ran
kcov 43 with identical arguments. The gate read whatever came back and never
asked whether it covered the codebase, so Continuous Integration graded 2.8% of
the product and reported 93.70%.

This replaces the merge with a union we own: parse each component's Cobertura
report and OR the hit counts per (file, line), so a line covered by any one
component counts once.

What a component contributes is not, however, ours to control. kcov 43 reads the
product line tables of only `runner` and `lib` of the eight component binaries on
Linux. That is a kcov defect, not a misconfiguration: a run with no include or
exclude filter at all returns nothing but `/opt/zig/lib/compiler_rt/*` for the
rest, while their debug info carries product units rooted squarely inside the
include path, and the same sources measure every component on macOS. Refusing to
publish under that defect left the lane with no measurement at all, so the gate
instead grades the union of the components that did collect and states on every
run how many of how many that was. The `required` set is the regression signal —
a component that collects today and stops fails the build, which is the case the
refusal was written to catch. It holds only the components that have collected
every run; `deadline` and `s3` come and go between runs from identical sources,
which is why a flickering component must not be able to fail the build.

A rate over a subset is not a rate over the codebase. Every surface this writes
says so, because the subset flatters: what Linux reads grades around 92% where
the whole codebase measures around 90%.
"""

from __future__ import annotations

import argparse
import shutil
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

# kcov already drops `*_test.zig` via --exclude-pattern. Test roots reach the
# report under a different spelling, so they are dropped here for the same
# reason: a gate satisfiable by writing more test files measures the wrong thing.
TEST_ROOT_NAMES = frozenset({"tests.zig", "test.zig"})


def is_product_source(filename: str) -> bool:
    """True when a reported path is shipped code rather than a test body."""
    basename = filename.rsplit("/", 1)[-1]
    return not (basename.endswith("_test.zig") or basename in TEST_ROOT_NAMES)


def find_report(component_dir: Path) -> Path:
    """Return the component's Cobertura report.

    kcov writes exactly one, under a `<binary>.<hash>/` subdirectory. Sorting
    keeps the choice deterministic if a stale sibling ever survives the rm -rf.
    """
    reports = sorted(p for p in component_dir.rglob("cobertura.xml") if p.stat().st_size > 0)
    if not reports:
        raise FileNotFoundError(f"no non-empty cobertura.xml under {component_dir}")
    return reports[0]


def source_root(root_element: ET.Element) -> Path | None:
    """The directory a report's filenames are relative to.

    Components root at different depths — the daemon's report is relative to
    `src/`, the lib lane's to `src/lib/` — so the same file arrives as
    `lib/common/backoff.zig` from one and `common/backoff.zig` from the other.
    Left unnormalised they union as two files, one of them looking untested.
    """
    element = root_element.find("sources/source")
    if element is None or not element.text:
        return None
    return Path(element.text.strip())


def read_component(report: Path, repo_root: Path) -> dict[tuple[str, int], bool]:
    """Parse one Cobertura report into {(repo-relative file, line): covered}."""
    with report.open("rb") as handle:
        root = ET.parse(handle).getroot()
    base = source_root(root)
    lines: dict[tuple[str, int], bool] = {}
    for class_element in root.iter("class"):
        raw = class_element.get("filename")
        if raw is None:
            continue
        resolved = (base / raw) if base is not None else Path(raw)
        try:
            filename = resolved.relative_to(repo_root).as_posix()
        except ValueError:
            filename = resolved.as_posix()
        if not is_product_source(filename):
            continue
        container = class_element.find("lines")
        if container is None:
            continue
        for line in container.findall("line"):
            number = line.get("number")
            if number is None:
                continue
            key = (filename, int(number))
            covered = int(line.get("hits", "0")) > 0
            lines[key] = lines.get(key, False) or covered
    return lines


def raw_class_names(report: Path, limit: int = 8) -> list[str]:
    """The class filenames a report carries BEFORE any filtering — evidence for
    diagnosing a component whose product view is empty. On Linux, Continuous
    Integration produced reports for a varying subset of components whose
    product view was zero while the file itself was not; this names what kcov
    actually wrote so the failure says which shape it took."""
    with report.open("rb") as handle:
        root = ET.parse(handle).getroot()
    names = [c.get("filename", "?") for c in root.iter("class")]
    shown = names[:limit]
    if len(names) > limit:
        shown.append(f"... +{len(names) - limit} more")
    return shown


def union_components(
    coverage_dir: Path, names: list[str], repo_root: Path, required: list[str]
) -> tuple[dict[tuple[str, int], bool], list[str], list[str]]:
    """Union every named component into (merged lines, collected names, empty names).

    A component contributing nothing is fatal only when it is named in
    `required`. Elsewhere it is reported and survived, because on Linux kcov
    cannot read most of these binaries at all and there is no measurement to be
    had by refusing.
    """
    merged: dict[tuple[str, int], bool] = {}
    collected: list[str] = []
    empty: list[str] = []
    for name in names:
        report = find_report(coverage_dir / name)
        component = read_component(report, repo_root)
        files = len({filename for filename, _ in component})
        print(f"→ [zig] component={name} files={files} lines={len(component)}")
        if not component:
            print(f"    raw classes in {report}: {raw_class_names(report)}")
            empty.append(name)
            continue
        collected.append(name)
        for key, covered in component.items():
            merged[key] = merged.get(key, False) or covered

    regressed = [name for name in required if name in empty]
    if regressed:
        raise ValueError(
            "required components contributed no measured lines: "
            + ", ".join(regressed)
            + " — these read on the last green run, so this is a regression rather "
            "than the known Linux capture gap"
        )
    if not collected:
        raise ValueError("no component contributed a measured line — there is nothing to grade")
    return merged, collected, empty


def describe_scope(collected: list[str], empty: list[str]) -> str:
    """One line stating what fraction of the component set the rate covers.

    Printed on success as well as failure: a number over a subset read as a
    number over the codebase is the exact misreading this gate exists to stop.
    """
    total = len(collected) + len(empty)
    scope = f"measured over {len(collected)} of {total} components"
    if not empty:
        return f"  {scope} — every component collected"
    return f"  ⚠ {scope}; kcov captured nothing for: {', '.join(sorted(empty))}"


def summarise(merged: dict[tuple[str, int], bool]) -> tuple[int, int, int, float]:
    """Return (files, covered, valid, percentage) for the merged union."""
    files = len({filename for filename, _ in merged})
    valid = len(merged)
    covered = sum(1 for hit in merged.values() if hit)
    percentage = (covered / valid * 100) if valid else 0.0
    return files, covered, valid, percentage


def write_merged_report(target: Path, merged: dict[tuple[str, int], bool]) -> None:
    """Write the union as Cobertura XML so the published artefact matches the gate.

    The workflow uploads this directory with `if-no-files-found: error`. It used
    to hold kcov's merge output, which is the report that lied; writing our own
    keeps the artefact and makes it agree with the number the gate enforces.
    """
    by_file: dict[str, list[tuple[int, bool]]] = {}
    for (filename, number), covered in merged.items():
        by_file.setdefault(filename, []).append((number, covered))

    files, covered_count, valid, percentage = summarise(merged)
    coverage = ET.Element(
        "coverage",
        {
            "line-rate": f"{percentage / 100:.6f}",
            "lines-covered": str(covered_count),
            "lines-valid": str(valid),
            "branch-rate": "0.0",
            "version": "1.9",
        },
    )
    packages = ET.SubElement(coverage, "packages")
    package = ET.SubElement(packages, "package", {"name": "zig", "line-rate": f"{percentage / 100:.6f}"})
    classes = ET.SubElement(package, "classes")
    for filename in sorted(by_file):
        class_element = ET.SubElement(
            classes, "class", {"name": filename, "filename": filename}
        )
        lines = ET.SubElement(class_element, "lines")
        for number, covered in sorted(by_file[filename]):
            ET.SubElement(lines, "line", {"number": str(number), "hits": "1" if covered else "0"})

    # Cleared, not overwritten: the directory previously held kcov's merge
    # output, and leaving that beside ours would publish two reports disagreeing
    # about the same run.
    shutil.rmtree(target, ignore_errors=True)
    target.mkdir(parents=True, exist_ok=True)
    ET.ElementTree(coverage).write(target / "cobertura.xml", encoding="utf-8", xml_declaration=True)
    (target / "summary.txt").write_text(
        f"{covered_count}/{valid} lines covered across {files} files ({percentage:.2f}%)\n",
        encoding="utf-8",
    )


def parse_args(argv: list[str] | None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--coverage-dir", type=Path, required=True)
    parser.add_argument("--component", action="append", required=True, dest="components")
    parser.add_argument(
        "--require-component",
        action="append",
        default=[],
        dest="required",
        help="component that must carry measured lines; empty means a regression, not a capture gap",
    )
    parser.add_argument("--min-pct", type=float, required=True)
    parser.add_argument("--summary-file", type=Path, required=True)
    parser.add_argument("--merged-report", type=Path, default=None)
    parser.add_argument("--repo-root", type=Path, required=True)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        merged, collected, empty = union_components(
            args.coverage_dir, args.components, args.repo_root.resolve(), args.required
        )
    except (FileNotFoundError, ValueError, ET.ParseError) as error:
        print(f"✗ Zig coverage merge failed: {error}", file=sys.stderr)
        return 1

    files, covered, valid, percentage = summarise(merged)
    scope = describe_scope(collected, empty)
    if args.merged_report is not None:
        write_merged_report(args.merged_report, merged)
    args.summary_file.parent.mkdir(parents=True, exist_ok=True)
    args.summary_file.write_text(
        f"zig_line_coverage_pct={percentage:.2f}\n"
        f"zig_line_coverage_min_pct={args.min_pct:g}\n"
        f"zig_measured_files={files}\n"
        f"zig_measured_lines={valid}\n"
        f"zig_components_measured={len(collected)}\n"
        f"zig_components_total={len(collected) + len(empty)}\n"
        f"zig_components_empty={','.join(sorted(empty))}\n",
        encoding="utf-8",
    )

    if percentage + 1e-9 < args.min_pct:
        print(
            f"✗ Zig line coverage {percentage:.2f}% is below threshold "
            f"{args.min_pct:.2f}% ({covered}/{valid} lines across {files} files)\n{scope}",
            file=sys.stderr,
        )
        return 1

    print(
        f"✓ [zig] merged line coverage passed ({percentage:.2f}% >= {args.min_pct:g}%; "
        f"{covered}/{valid} lines across {files} files)"
    )
    print(scope)
    return 0


if __name__ == "__main__":
    sys.exit(main())
