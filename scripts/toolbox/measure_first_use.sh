#!/usr/bin/env bash
# Measure cold first use with the toolbox as bubblewrap's read-only root.
# Usage: sudo bash measure_first_use.sh IMAGE OUTPUT.json [RUNS [BASELINE.json]]
# Each command gets a fresh writable workspace and a cleared host page cache.
set -euo pipefail

readonly MIN_RUNS=20
readonly DEFAULT_RUNS=100
if [ "$#" -lt 2 ] || [ "$#" -gt 4 ]; then
  echo "usage: $0 IMAGE OUTPUT.json [RUNS [BASELINE.json]]" >&2
  exit 2
fi
readonly IMAGE="$1" OUTPUT="$2" RUNS="${3:-$DEFAULT_RUNS}" BASELINE="${4:-}"
if [[ ! "$RUNS" =~ ^[1-9][0-9]*$ ]] || [ "$RUNS" -lt "$MIN_RUNS" ]; then
  echo "measure_first_use.sh: RUNS must be an integer of at least $MIN_RUNS" >&2
  exit 2
fi
if [ "$(uname -s)" != Linux ] || [ "$(id -u)" -ne 0 ]; then
  echo "measure_first_use.sh: needs Linux root to mount EROFS and drop caches" >&2
  exit 1
fi
for command in mount mountpoint umount bwrap python3; do
  command -v "$command" >/dev/null
done
[ -f "$IMAGE" ]

work="$(mktemp -d /var/tmp/toolbox-first-use.XXXXXX)"
cleanup() {
  # A failed unmount must leave its tree intact rather than remove a mounted root.
  # Observe the kernel mount even if a signal arrived before mount returned.
  if mountpoint -q "$work/root" && ! umount "$work/root"; then
    echo "measure_first_use.sh: unmount failed; retained $work" >&2
    return 1
  fi
  rm -rf "$work"
}
trap cleanup EXIT
cancel() {
  trap '' INT TERM HUP
  local child
  # The job table also covers a signal between child launch and wait.
  for child in $(jobs -pr); do
    kill -TERM "$child" 2>/dev/null || true
    wait "$child" 2>/dev/null || true
  done
  exit "$1"
}
trap 'cancel 130' INT
trap 'cancel 143' TERM
trap 'cancel 129' HUP
mkdir "$work/root"
mount -t erofs -o loop,ro,nosuid,nodev -- "$IMAGE" "$work/root"

python3 - "$IMAGE" "$OUTPUT" "$RUNS" "$BASELINE" "$work" <<'PY' &
import hashlib
import json
import math
import os
import platform
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path

MIN_RUNS = 20
REGRESSION_LIMIT = 1.10
COMMAND_TIMEOUT_SECONDS = 120
NANOSECONDS_PER_MILLISECOND = 1_000_000
SHA256_PATTERN = r"[0-9a-f]{64}"
DROP_CACHES = Path("/proc/sys/vm/drop_caches")
NODE_TEST = """const test = require('node:test');
const assert = require('node:assert/strict');
test('adds', () => assert.equal(1 + 1, 2));
"""
GIT_COMMIT = (
    "git init -q && printf 'one\\n' >README && git add README && "
    "git -c user.name=agentsfleet -c user.email=noreply@agentsfleet.net "
    "commit -q -m one"
)
STEPS = {
    "node_test": ["node", "--test", "/workspace/sum.test.js"],
    "python_import": ["python3", "-c", "import hashlib, json, pathlib, ssl"],
    "git_commit": ["sh", "-ec", GIT_COMMIT],
    "node_start": ["node", "-e", "0"],
    "uv": ["uv", "--version"],
    "rg": ["rg", "--version"],
    "jq": ["jq", "--version"],
    "curl": ["curl", "--version"],
}
image, output, runs, baseline_path, work = sys.argv[1:]
image, output, work = Path(image), Path(output), Path(work)
runs = int(runs)
baseline = json.loads(Path(baseline_path).read_text()) if baseline_path else None


def validate_baseline():
    if not baseline_path:
        return
    if not isinstance(baseline, dict):
        raise SystemExit("baseline must be a JSON object")
    digest = baseline.get("image_sha256")
    if not isinstance(digest, str) or re.fullmatch(SHA256_PATTERN, digest) is None:
        raise SystemExit("baseline needs a SHA-256 image digest")
    if (baseline.get("arch") != platform.machine()
            or baseline.get("kernel") != platform.release()
            or baseline.get("cache") != "cold"
            or baseline.get("runs", 0) < MIN_RUNS
            or set(baseline.get("steps", {})) != set(STEPS)):
        raise SystemExit("baseline needs the same architecture, kernel, cold cache and steps")
    for name, command in STEPS.items():
        step = baseline["steps"][name]
        p99 = step.get("p99_ms")
        if (step.get("command") != command or isinstance(p99, bool)
                or not isinstance(p99, (int, float)) or not math.isfinite(p99) or p99 <= 0):
            raise SystemExit(f"baseline has an incompatible command or p99 for {name}")


def measure(command):
    workspace = work / "workspace"
    if workspace.exists():
        shutil.rmtree(workspace)
    workspace.mkdir(mode=0o777)
    workspace.chmod(0o777)
    (workspace / "tmp").mkdir(mode=0o777)
    (workspace / "tmp").chmod(0o777)
    (workspace / "sum.test.js").write_text(NODE_TEST)
    sandbox = [
        "bwrap", "--unshare-all", "--die-with-parent", "--new-session",
        "--cap-drop", "ALL", "--uid", "65534", "--gid", "65534",
        "--ro-bind", str(work / "root"), "/",
        "--bind", str(workspace), "/workspace",
        "--bind", str(workspace / "tmp"), "/tmp",
        "--proc", "/proc", "--dev", "/dev", "--tmpfs", "/run",
        "--clearenv", "--setenv", "PATH", "/usr/local/bin:/usr/bin:/bin",
        "--setenv", "HOME", "/workspace", "--setenv", "LANG", "C.UTF-8",
        "--chdir", "/workspace", "--", *command,
    ]
    os.sync()
    DROP_CACHES.write_text("3")
    start = time.perf_counter_ns()
    result = subprocess.run(sandbox, capture_output=True, timeout=COMMAND_TIMEOUT_SECONDS)
    elapsed = (time.perf_counter_ns() - start) / NANOSECONDS_PER_MILLISECOND
    if result.returncode:
        raise SystemExit(f"{command!r} failed ({result.returncode}): {result.stderr.decode(errors='replace')}")
    return elapsed


def percentile(samples, percent):
    return sorted(samples)[math.ceil(len(samples) * percent / 100) - 1]


def publish_report(report):
    def cancelled(signum, _frame):
        raise SystemExit(128 + signum)

    signal.signal(signal.SIGTERM, cancelled)
    # Defer cancellation until the exclusively created file has a cleanup owner.
    previous_mask = signal.pthread_sigmask(signal.SIG_BLOCK, {signal.SIGTERM})
    try:
        descriptor, filename = tempfile.mkstemp(prefix=f".{output.name}.", suffix=".tmp", dir=output.parent)
        temporary = Path(filename)
        try:
            signal.pthread_sigmask(signal.SIG_SETMASK, previous_mask)
            with os.fdopen(descriptor, "w") as handle:
                json.dump(report, handle, indent=2, sort_keys=True)
                handle.write("\n")
            os.replace(temporary, output)
        finally:
            temporary.unlink(missing_ok=True)
    finally:
        signal.pthread_sigmask(signal.SIG_SETMASK, previous_mask)


validate_baseline()
with image.open("rb") as handle:
    digest = hashlib.file_digest(handle, "sha256").hexdigest()
samples = {name: [] for name in STEPS}
# Interleave tools so each one's samples span the whole run.
for number in range(runs):
    for name, command in STEPS.items():
        samples[name].append(measure(command))
    print(f"first use: {number + 1}/{runs}", file=sys.stderr)

report = {
    "arch": platform.machine(), "kernel": platform.release(), "cache": "cold",
    "image_sha256": digest, "image_bytes": image.stat().st_size, "runs": runs,
    "regression_limit": REGRESSION_LIMIT, "steps": {},
}
regressions = []
for name, command in STEPS.items():
    p99 = percentile(samples[name], 99)
    step = {"command": command, "p50_ms": percentile(samples[name], 50),
            "p99_ms": p99, "samples_ms": samples[name]}
    if baseline is not None:
        ratio = p99 / baseline["steps"][name]["p99_ms"]
        step.update(p99_ratio=ratio, result="REGRESSION" if ratio > REGRESSION_LIMIT else "PASS")
        if ratio > REGRESSION_LIMIT:
            regressions.append(name)
    report["steps"][name] = step
report["baseline_sha256"] = baseline["image_sha256"] if baseline else None
publish_report(report)
print(json.dumps({"report": str(output), "regressions": regressions}))
sys.exit(1 if regressions else 0)
PY
wait "$!"
