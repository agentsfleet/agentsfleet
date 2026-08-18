#!/usr/bin/env python3
"""Fail-closed verification graph, result provenance, and timing checks."""
import argparse
import hashlib
import json
import os
import platform
import subprocess
import tempfile
from pathlib import Path

import verification_evidence as evidence

POLICY = {
    "agentsfleetd-tests": ("unit", "agentsfleetd", []),
    "agentsfleet-runner-tests": ("unit", "runner", []),
    "agentsfleet-lib-tests": ("unit", "lib", []),
    "agentsfleet-logging-tests": ("unit", "logging", []),
    "agentsfleet-call-deadline-tests": ("unit", "deadline", []),
    "agentsfleet-s3-tests": ("unit", "s3", []),
    "agentsfleetd-test-auth": ("unit", None, []),
    "agentsfleet-runner-integration-tests": ("integration", "runner_integration", ["kernel"]),
    "agentsfleetd-integration-tests": ("integration", "integration", ["postgres", "redis", "qstash", "http_ports"]),
}
DEFAULT_GRAPH = Path(".tmp/verification-graph.json")
FNV_OFFSET = 14695981039346656037
FNV_PRIME = 1099511628211
MASK64 = (1 << 64) - 1
ISOLATED_TEST_MARKER = "daemon boot -> SIGTERM -> drain"


class VerificationError(ValueError):
    """A specific verification boundary failure."""


def parse_listing(text, source="listing"):
    """Return ROOT records and matching TEST names from compiler wire text."""
    roots, pending, errors = [], [], []
    for number, raw in enumerate(text.splitlines(), 1):
        if not raw:
            continue
        fields = raw.split("\t")
        if fields[0] == "ROOT" and len(fields) == 3 and all(fields[1:]):
            roots.append((fields[1], fields[2]))
        elif fields[0] == "TEST" and len(fields) == 4 and all(fields[1:]):
            pending.append((fields[1], fields[2], fields[3]))
        else:
            errors.append(f"{source}:{number}:{raw}")
    if errors:
        raise VerificationError("malformed records: " + ", ".join(errors))
    root_set = set(roots)
    tests = [(lane, root, name) for lane, root, name in pending if (lane, root) in root_set]
    return roots, tests


def build_graph(listings):
    """Build and validate a stable graph from parsed listing text values."""
    roots, tests = [], []
    for source, text in listings:
        parsed_roots, parsed_tests = parse_listing(text, source)
        roots.extend(parsed_roots)
        tests.extend(parsed_tests)
    counts = {}
    for lane, root in roots:
        counts.setdefault(lane, []).append(root)
    duplicates = sorted(f"{lane}:{root}" for lane, values in counts.items()
                        if len(values) > 1 for root in values)
    missing = sorted(set(POLICY) - set(counts))
    missing_owner = sorted(set(counts) - set(POLICY))
    problems = []
    if duplicates:
        problems.append("duplicate roots: " + ", ".join(duplicates))
    if missing:
        problems.append("missing roots: " + ", ".join(missing))
    if missing_owner:
        problems.append("missing owner: " + ", ".join(missing_owner))
    if problems:
        raise VerificationError("; ".join(problems))
    ordered = []
    root_by_lane = {lane: root for lane, root in roots}
    for lane in POLICY:
        root = root_by_lane[lane]
        owner, component, isolation = POLICY[lane]
        ordered.append({"lane": lane, "root": root, "owner": owner,
                        "component": component, "isolation": isolation,
                        "tests": sorted(name for test_lane, test_root, name in tests
                                        if (test_lane, test_root) == (lane, root))})
    digest = hashlib.sha256(canonical(ordered)).hexdigest()
    return {"roots": ordered, "graph_digest": digest}


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def atomic_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(dir=path.parent, prefix=path.name + ".")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            json.dump(value, stream, indent=2, sort_keys=True)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def fnv1a64(name):
    value = FNV_OFFSET
    for byte in name.encode("utf-8"):
        value = ((value ^ byte) * FNV_PRIME) & MASK64
    return value


def shard_candidates(graph):
    roots = [root for root in graph["roots"] if root["lane"] == "agentsfleetd-integration-tests"]
    return [name for root in roots for name in root.get("tests", [])
            if "_integration_test" in name or "integration:" in name or name.endswith(".test_0")]


def assign_shards(candidates, count):
    if count <= 0:
        raise VerificationError("shard count must be positive")
    shards = [[] for _ in range(count)]
    for name in candidates:
        if count > 1 and ISOLATED_TEST_MARKER in name:
            shards[-1].append(name)
        else:
            regular_shards = count - 1 if count > 1 else 1
            shards[fnv1a64(name) % regular_shards].append(name)
    return shards


def validate_shards(candidates, shards):
    expected, found = set(candidates), [name for shard in shards for name in shard]
    duplicate = sorted({name for name in found if found.count(name) > 1})
    missing, extra = sorted(expected - set(found)), sorted(set(found) - expected)
    empty = [str(index) for index, shard in enumerate(shards) if not shard]
    problems = []
    for label, values in (("duplicate tests", duplicate), ("missing tests", missing),
                          ("unexpected tests", extra), ("empty shards", empty)):
        if values:
            problems.append(label + ": " + ", ".join(values))
    if problems:
        raise VerificationError("; ".join(problems))
    return True


def current_identity(graph_path=DEFAULT_GRAPH, environment_label=None):
    diff = subprocess.run(["git", "diff", "HEAD", "--binary"], check=True,
                          capture_output=True).stdout
    untracked = subprocess.run(
        ["git", "ls-files", "--others", "--exclude-standard", "-z"], check=True,
        capture_output=True,
    ).stdout.split(b"\0")
    source = hashlib.sha256(diff)
    for raw_path in sorted(path for path in untracked if path):
        path = Path(os.fsdecode(raw_path))
        source.update(raw_path + b"\0")
        source.update(path.read_bytes())
    head = subprocess.run(["git", "rev-parse", "HEAD"], check=True, text=True,
                          capture_output=True).stdout.strip()
    zig = subprocess.run(["zig", "version"], check=True, text=True,
                         capture_output=True).stdout.strip()
    with Path(graph_path).open(encoding="utf-8") as stream:
        graph = json.load(stream)
    environment = {"system": platform.system(), "machine": platform.machine()}
    if environment_label is not None:
        environment["label"] = environment_label
    return {"source_revision": head + "+" + source.hexdigest(),
            "toolchain_identity": zig, "graph_digest": graph["graph_digest"],
            "environment_identity": environment}


def build_result(identity, execution, outcome, duration_ms, cache_state, reports,
                 shard_index=None, shard_count=None, isolation_key=None):
    if duration_ms < 0:
        raise VerificationError("duration_ms must be nonnegative")
    if not reports:
        raise VerificationError("at least one report is required")
    checked = []
    for report in reports:
        path = Path(report)
        if not path.is_file() or path.stat().st_size == 0:
            raise VerificationError(f"empty or missing report: {report}")
        checked.append({"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
    result = dict(identity, execution=execution, outcome=outcome,
                  duration_ms=duration_ms, cache_state=cache_state, reports=checked)
    if execution == "integration-shard":
        if shard_index is None or shard_count is None or not 0 <= shard_index < shard_count:
            raise VerificationError("integration-shard requires valid shard_index and shard_count")
        if not isolation_key:
            raise VerificationError("integration-shard requires an isolation_key")
        result.update(shard_index=shard_index, shard_count=shard_count,
                      isolation_key=isolation_key)
    return result


def validate_result(manifest, identity, execution):
    errors = [f"mismatched {field}" for field, expected in identity.items()
              if manifest.get(field) != expected]
    if manifest.get("execution") != execution:
        errors.append("mismatched execution")
    if manifest.get("outcome") != "success":
        errors.append("failed outcome")
    for report in manifest.get("reports", []):
        path = Path(report.get("path", ""))
        if not path.is_file() or path.stat().st_size == 0:
            errors.append(f"empty report {path}")
        elif hashlib.sha256(path.read_bytes()).hexdigest() != report.get("sha256"):
            errors.append(f"tampered report {path}")
    if not manifest.get("reports"):
        errors.append("missing reports")
    if errors:
        raise VerificationError("; ".join(errors))


def validate_results(manifests, identity, expected_shards, report_nonempty=None):
    errors, executions, indices, isolation_keys = [], [], [], []
    for number, manifest in enumerate(manifests):
        label = str(manifest.get("manifest_path", number))
        for field, expected in identity.items():
            if field == "environment_identity":
                continue
            if manifest.get(field) != expected:
                errors.append(f"{label}: mismatched {field}")
        if manifest.get("outcome") != "success":
            errors.append(f"{label}: failed outcome")
        executions.append(manifest.get("execution"))
        if manifest.get("execution") == "integration-shard":
            indices.append(manifest.get("shard_index"))
            isolation_keys.append(manifest.get("isolation_key"))
            if manifest.get("shard_count") != expected_shards:
                errors.append(f"{label}: mismatched shard_count")
        reports = manifest.get("reports", [])
        if not reports:
            errors.append(f"{label}: missing reports")
        for report in reports:
            path = report.get("path", "")
            good = report_nonempty(path) if report_nonempty else Path(path).is_file() and Path(path).stat().st_size > 0
            if not good:
                errors.append(f"{label}: empty report {path}")
            elif report_nonempty is None and hashlib.sha256(Path(path).read_bytes()).hexdigest() != report.get("sha256"):
                errors.append(f"{label}: tampered report {path}")
    for execution in ("unit", "runner-kernel"):
        if executions.count(execution) != 1:
            errors.append(f"expected one {execution}")
    unknown_executions = sorted(
        str(execution) for execution in executions
        if execution not in ("unit", "runner-kernel", "integration-shard")
    )
    if unknown_executions:
        errors.append("unknown executions: " + ", ".join(unknown_executions))
    duplicate = sorted({index for index in indices if indices.count(index) > 1}, key=str)
    missing = sorted(set(range(expected_shards)) - set(indices))
    unexpected = sorted(set(indices) - set(range(expected_shards)), key=str)
    if duplicate:
        errors.append("duplicate shards: " + ", ".join(map(str, duplicate)))
    if missing:
        errors.append("missing shards: " + ", ".join(map(str, missing)))
    if unexpected:
        errors.append("unexpected shards: " + ", ".join(map(str, unexpected)))
    if any(not key for key in isolation_keys):
        errors.append("missing shard isolation keys")
    if len(set(isolation_keys)) != len(isolation_keys):
        errors.append("duplicate shard isolation keys")
    errors.extend(evidence.execution_environment_errors(manifests))
    if errors:
        raise VerificationError("; ".join(errors))
    return True


def load_graph(path):
    with Path(path).open(encoding="utf-8") as stream:
        return json.load(stream)


def compiler_graph():
    commands = (("build", ["zig", "build", "list-tests"]),
                ("runner", ["zig", "build", "--build-file", "build_runner.zig", "list-tests"]))
    return build_graph([(name, subprocess.run(command, check=True, text=True,
                                               capture_output=True).stdout)
                        for name, command in commands])


def main(argv=None):
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    validate = commands.add_parser("validate"); validate.add_argument("--output", default=DEFAULT_GRAPH)
    shards = commands.add_parser("shards"); shards.add_argument("--count", type=int, required=True); shards.add_argument("--output", default=DEFAULT_GRAPH); shards.add_argument("--binary")
    write = commands.add_parser("write-result")
    write.add_argument("--output", required=True); write.add_argument("--execution", required=True, choices=("unit", "integration-shard", "runner-kernel"))
    write.add_argument("--outcome", required=True); write.add_argument("--duration-ms", required=True, type=float); write.add_argument("--cache-state", required=True)
    write.add_argument("--shard-index", type=int); write.add_argument("--shard-count", type=int); write.add_argument("--report", action="append", default=[])
    write.add_argument("--isolation-key")
    write.add_argument("--graph", default=DEFAULT_GRAPH); write.add_argument("--environment-label")
    single = commands.add_parser("validate-result"); single.add_argument("--manifest", required=True); single.add_argument("--execution", required=True)
    single.add_argument("--graph", default=DEFAULT_GRAPH); single.add_argument("--environment-label")
    results = commands.add_parser("validate-results"); results.add_argument("--manifest", action="append", required=True); results.add_argument("--expected-shard-count", type=int, required=True)
    results.add_argument("--graph", default=DEFAULT_GRAPH); results.add_argument("--environment-label")
    compare = commands.add_parser("compare"); compare.add_argument("--scope", choices=("local", "ci"), required=True); compare.add_argument("--evidence", required=True)
    commands.add_parser("orphans")
    args = parser.parse_args(argv)
    if args.command == "validate":
        atomic_json(args.output, compiler_graph()); print("verification graph valid duplicate_roots=0 missing_roots=0")
    elif args.command == "shards":
        graph = compiler_graph(); atomic_json(args.output, graph); candidates = shard_candidates(graph); assigned = assign_shards(candidates, args.count); validate_shards(candidates, assigned)
        if args.binary: evidence.validate_runtime_shards(args.binary, args.count, candidates)
        for index, names in enumerate(assigned): print(f"shard={index} count={len(names)} digest={hashlib.sha256(canonical(names)).hexdigest()}")
    elif args.command == "write-result":
        atomic_json(args.output, build_result(current_identity(args.graph, args.environment_label), args.execution, args.outcome, args.duration_ms, args.cache_state, args.report, args.shard_index, args.shard_count, args.isolation_key))
    elif args.command == "validate-result":
        validate_result(load_graph(args.manifest), current_identity(args.graph, args.environment_label), args.execution); print("verification result reusable")
    elif args.command == "validate-results":
        manifests = []
        for path in args.manifest:
            value = load_graph(path); value["manifest_path"] = path; manifests.append(value)
        validate_results(manifests, current_identity(args.graph, args.environment_label), args.expected_shard_count); print("verification results valid")
    elif args.command == "compare":
        payload = load_graph(args.evidence); values = payload.get("samples", payload) if isinstance(payload, dict) else payload
        result = evidence.compare_samples(values, args.scope); print(" ".join(f"{state}_improvement_pct={value:.2f}" for state, value in result.items()))
    else:
        paths = Path(".github/workflows").glob("*.y*ml"); offenders = evidence.find_orphans({str(path): path.read_text(encoding="utf-8") for path in paths})
        if offenders: raise VerificationError("orphaned paths: " + ", ".join(offenders))
        print("orphaned_paths=0")


if __name__ == "__main__":
    try:
        main()
    except (VerificationError, evidence.EvidenceError, OSError, subprocess.CalledProcessError, json.JSONDecodeError) as error:
        raise SystemExit(f"verification error: {error}")
