"""Timing and workflow evidence checks for the verification graph."""
import argparse
import json
import re
import statistics
import subprocess
import tempfile
import time
from pathlib import Path


class EvidenceError(ValueError):
    """Verification evidence is incomplete or incomparable."""


def summarize_timings(records, expected_labels):
    errors = []
    labels = [record.get("label") for record in records]
    for label in expected_labels:
        if labels.count(label) != 1:
            errors.append(f"expected one timing for {label}")
    unexpected = sorted(str(label) for label in labels if label not in expected_labels)
    if unexpected:
        errors.append("unexpected timings: " + ", ".join(unexpected))
    intervals = []
    for record in records:
        label = record.get("label")
        started = record.get("started_at_ms")
        finished = record.get("finished_at_ms")
        duration = record.get("duration_ms")
        if record.get("outcome") != "success" or record.get("exit_code") != 0:
            errors.append(f"{label}: worker did not succeed")
        if any(isinstance(value, bool) or not isinstance(value, (int, float))
               for value in (started, finished, duration)):
            errors.append(f"{label}: invalid timestamps")
        elif started >= finished or duration != finished - started:
            errors.append(f"{label}: invalid timing interval")
        else:
            intervals.append((started, finished, duration))
    if errors:
        raise EvidenceError("; ".join(errors))
    events = []
    for started, finished, _ in intervals:
        events.extend(((started, 1), (finished, -1)))
    active = peak = 0
    for _, change in sorted(events, key=lambda event: (event[0], event[1])):
        active += change
        peak = max(peak, active)
    if len(intervals) > 1 and peak < 2:
        raise EvidenceError("worker timings do not overlap")
    first = min(started for started, _, _ in intervals)
    last = max(finished for _, finished, _ in intervals)
    summed = sum(duration for _, _, duration in intervals)
    return {
        "worker_count": len(intervals),
        "started_at_ms": first,
        "finished_at_ms": last,
        "fanout_wall_ms": last - first,
        "summed_worker_ms": summed,
        "overlap_ms": max(0, summed - (last - first)),
        "peak_concurrency": peak,
    }


def write_timing_summary(paths, output, shard_count):
    records = [json.loads(Path(path).read_text(encoding="utf-8")) for path in paths]
    expected = ["runner-kernel"] + [f"integration-shard-{index}"
                                    for index in range(shard_count)]
    summary = summarize_timings(records, expected)
    destination = Path(output)
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n",
                           encoding="utf-8")
    return summary


def write_start(path):
    destination = Path(path)
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(f"{time.time_ns() // 1_000_000}\n", encoding="utf-8")


def compare_samples(samples, scope):
    selected = [sample for sample in samples if sample.get("scope") == scope]
    results = {}
    for state in ("cold", "warm"):
        group = [sample for sample in selected if sample.get("cache_state") == state]
        identities = {
            (sample.get("source_revision"), sample.get("image_identity"))
            for sample in group
        }
        if len(identities) != 1:
            raise EvidenceError(f"{state}: unlike source/image samples")
        kinds = {
            kind: [
                sample.get("duration_ms")
                for sample in group
                if sample.get("kind") == kind
            ]
            for kind in ("baseline", "candidate")
        }
        if any(
            len(values) < 3
            or any(not isinstance(value, (int, float)) or value <= 0 for value in values)
            for values in kinds.values()
        ):
            raise EvidenceError(f"{state}: insufficient positive samples")
        baseline, candidate = map(
            statistics.median, (kinds["baseline"], kinds["candidate"])
        )
        improvement = (baseline - candidate) * 100 / baseline
        results[state] = improvement
        if improvement < 35:
            raise EvidenceError(f"{state}: improvement_pct={improvement:.2f} below 35")
    return results


def find_orphans(workflows):
    offenders = []
    direct = re.compile(r"(?<![-\w])zig\s+build\s+test-integration(?!-bin)(?:\s|$)")
    for name, text in workflows.items():
        for number, line in enumerate(text.splitlines(), 1):
            if direct.search(line):
                offenders.append(f"{name}:{number}")
    return offenders


def validate_runtime_shards(binary, count, expected, run=subprocess.run):
    def selected(index, shard_count):
        with tempfile.TemporaryFile(mode="w+", encoding="utf-8") as stderr:
            run(
                [str(binary), f"--shard-index={index}", f"--shard-count={shard_count}",
                 "--list-selected"],
                check=True, stderr=stderr, text=True,
            )
            stderr.seek(0)
            return stderr.read().splitlines()

    unsharded = selected(0, 1)
    shards = [selected(index, count) for index in range(count)]
    flattened = [name for shard in shards for name in shard]
    duplicate = sorted({name for name in flattened if flattened.count(name) > 1})
    if set(unsharded) != set(expected):
        raise EvidenceError("runtime unsharded discovery differs from compiler graph")
    if duplicate or sorted(flattened) != sorted(unsharded):
        raise EvidenceError("runtime shard union has omissions or duplicates")
    return shards


def execution_environment_errors(manifests):
    errors = []
    by_execution = {}
    for manifest in manifests:
        by_execution.setdefault(manifest.get("execution"), []).append(
            manifest.get("environment_identity", {})
        )
    expected_labels = {
        "unit": "unit",
        "runner-kernel": "runner-kernel",
        "integration-shard": "integration",
    }
    for execution, label in expected_labels.items():
        for environment in by_execution.get(execution, []):
            if environment.get("label") != label:
                errors.append(f"{execution}: mismatched environment label")
    runner = by_execution.get("runner-kernel", [])
    if runner and runner[0].get("system") != "Linux":
        errors.append("runner-kernel: execution environment is not Linux")
    shard_environments = {
        (value.get("system"), value.get("machine"))
        for value in by_execution.get("integration-shard", [])
    }
    if len(shard_environments) > 1:
        errors.append("integration shards used unlike execution environments")
    return errors


def main(argv=None):
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    summary = commands.add_parser("summarize")
    summary.add_argument("--timing", action="append", required=True)
    summary.add_argument("--output", required=True)
    summary.add_argument("--expected-shard-count", type=int, required=True)
    start = commands.add_parser("start")
    start.add_argument("--output", required=True)
    args = parser.parse_args(argv)
    if args.command == "start":
        write_start(args.output)
        return
    if args.expected_shard_count <= 0:
        parser.error("expected shard count must be positive")
    result = write_timing_summary(
        args.timing, args.output, args.expected_shard_count,
    )
    print(
        f"workers={result['worker_count']} peak_concurrency={result['peak_concurrency']} "
        f"fanout_wall_ms={result['fanout_wall_ms']} "
        f"summed_worker_ms={result['summed_worker_ms']} overlap_ms={result['overlap_ms']}"
    )


if __name__ == "__main__":
    try:
        main()
    except (EvidenceError, OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"verification evidence error: {error}")
