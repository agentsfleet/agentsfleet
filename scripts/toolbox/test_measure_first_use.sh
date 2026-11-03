#!/usr/bin/env bash
# Native Linux regression proofs for first-use mount cleanup.
# Optional argument selects the pre-fix sampler for the red-green proof.
# A second argument selects one failure case for its isolated red proof.
set -euo pipefail
readonly SAMPLER="${1:-scripts/toolbox/measure_first_use.sh}"
if [ "$(uname -s)" != Linux ] || [ "$(id -u)" -ne 0 ]; then
  echo "test_measure_first_use.sh: needs Linux root" >&2
  exit 1
fi
python3 - "$SAMPLER" "${2:-}" <<'PY'
import json
import os
import random
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path

sampler = Path(sys.argv[1]).resolve()
# Every case checks the image's loop devices, so a host without losetup fails
# here, before any fixture is made, rather than mid-case.
losetup = shutil.which("losetup")
assert losetup, "losetup is not on PATH: every case lists the image's loop devices with it"
fixture = Path(tempfile.mkdtemp(prefix="first-use-cleanup-", dir="/var/tmp"))
source = fixture / "source"
source.mkdir()
(source / "notice").write_text("cleanup fixture\n")
for name in ("tmp", "workspace", "proc", "dev", "run"):
    (source / name).mkdir()
binaries = ["/usr/bin/dash", "/usr/bin/sleep"]
dependencies = subprocess.check_output(["ldd", *binaries], text=True)
libraries = [name for name in re.findall(r"\s(/[^\s]+)", dependencies) if Path(name).is_file()]
for name in dict.fromkeys([*binaries, *libraries]):
    target = source / name.lstrip("/")
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(name, target)
(source / "usr/bin/sh").symlink_to("dash")
image = fixture / "fixture.erofs"
subprocess.run(["mkfs.erofs", "--quiet", str(image), str(source)], check=True)
cases = ["signal_after_mount", "mount_failed", "command_failed", "unmount_failed",
         "null_baseline", "publication_symlink", "signal_during_measurement", "missing_digest",
         "signal_during_publication", "signal_hangup_during_publication",
         "direct_io_when_asked", "direct_io_refused"]
# Cases whose measurement runs to a published report.
published = ("publication_symlink", "direct_io_when_asked")
if sys.argv[2]:
    assert sys.argv[2] in cases
    cases = [sys.argv[2]]
random.SystemRandom().shuffle(cases)


def mounted_root(marker):
    if not marker.is_file():
        return None
    root = Path(marker.read_text().strip())
    assert root.parent.name.startswith("toolbox-first-use.") and root.name == "root"
    return root


def loops_on(image, patience=5):
    """The loop devices backed by `image`, once any autoclear has finished."""
    deadline = time.monotonic() + patience
    while True:
        found = subprocess.run([losetup, "--noheadings", "--output", "NAME", "--associated", str(image)],
                               capture_output=True, text=True, check=True).stdout.split()
        if not found or time.monotonic() >= deadline:
            return found
        time.sleep(0.05)


def refused_direct_io(directory):
    """A device that did not take direct I/O ends the run unmeasured and detached."""
    wrapper = directory / "losetup"
    wrapper.write_text('#!/bin/bash\nargs=()\nfor arg in "$@"; do\n'
                       ' [ "$arg" = --direct-io=on ] || args+=("$arg")\ndone\n'
                       f'exec {losetup} "${{args[@]}}"\n')
    wrapper.chmod(0o700)
    environment = dict(os.environ, PATH=str(directory) + ":" + os.environ["PATH"],
                       TOOLBOX_LOOP_DIRECT_IO="on")
    output = directory / "report.json"
    result = subprocess.run(["bash", str(sampler), str(image), str(output), "20"],
                            env=environment, capture_output=True, text=True, timeout=90)
    assert result.returncode == 1, f"direct_io_refused: exit {result.returncode}: {result.stderr}"
    assert "did not take direct I/O" in result.stderr, result.stderr
    assert not output.exists(), "direct_io_refused: a report was published"
    assert not loops_on(image), "direct_io_refused: the refused device was left attached"
    print(json.dumps(dict(case="direct_io_refused", exit=result.returncode, result="PASS")))


# How long one case's sampler may run. A direct-I/O case reads every one of its
# 20 runs past the host cache, which takes several times a buffered case's
# time on a virtual disk; the bound catches a hang, not a slow disk.
BOUND = 90
DIRECT_IO_BOUND = 300


def run_sampler(command, environment, marker, case):
    publication = case in ("signal_during_publication", "signal_hangup_during_publication")
    if case != "signal_during_measurement" and not publication:
        bound = DIRECT_IO_BOUND if case == "direct_io_when_asked" else BOUND
        return subprocess.run(command, env=environment, capture_output=True, text=True, timeout=bound)
    process = subprocess.Popen(command, env=environment, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, text=True, start_new_session=True)
    try:
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            root = mounted_root(marker)
            started = (marker.parent / "publication-started" if publication
                       else root.parent / "workspace/cancellation-started" if root else None)
            if started and started.is_file():
                break
            assert process.poll() is None, "sampler exited before cancellation"
            time.sleep(0.01)
        else:
            raise AssertionError(f"{case}: cancellation point did not start")
        children = Path(f"/proc/{process.pid}/task/{process.pid}/children").read_text().split()
        assert len(children) == 1, f"unexpected sampler children: {children}"
        if case == "signal_hangup_during_publication":
            os.killpg(process.pid, signal.SIGHUP)
        else:
            process.send_signal(signal.SIGTERM)
        stdout, stderr = process.communicate(timeout=3)
        assert not Path(f"/proc/{children[0]}").exists(), "measurement child was not reaped"
        return subprocess.CompletedProcess(command, process.returncode, stdout, stderr)
    finally:
        if process.poll() is None:
            owned = Path(f"/proc/{process.pid}/task/{process.pid}/children").read_text().split()
            for child in owned:
                try:
                    os.kill(int(child), signal.SIGTERM)
                except ProcessLookupError:
                    pass
            process.kill()
            process.communicate(timeout=5)


try:
    for case in cases:
        directory = fixture / case
        directory.mkdir()
        if case == "direct_io_refused":
            refused_direct_io(directory)
            continue
        marker = directory / "mounted-target"
        mount = directory / "mount"
        mount.write_text(
            '#!/bin/bash\nset -euo pipefail\n'
            f'printf "%s\\n" "${{@: -1}}" >"{marker}"\n'
            + ('exit 77\n' if case == "mount_failed" else '/usr/bin/mount "$@"\n')
            + ('kill -TERM "$PPID"\n' if case == "signal_after_mount" else '')
        )
        mount.chmod(0o700)
        if case == "unmount_failed":
            umount = directory / "umount"
            umount.write_text('#!/bin/bash\nexit 73\n')
            umount.chmod(0o700)
            # Let measurement succeed: an EXIT trap returning 1 must fail the run.
            python = directory / "python3"
            python.write_text('#!/bin/bash\nexit 0\n')
            python.chmod(0o700)
        baseline = []
        called = directory / "command-called"
        publication = case in ("signal_during_publication", "signal_hangup_during_publication")
        if case in ("null_baseline", "missing_digest", *published) or publication:
            bwrap = directory / "bwrap"
            bwrap.write_text(f'#!/bin/bash\ntouch "{called}"\nexit 0\n')
            bwrap.chmod(0o700)
        if case == "signal_during_measurement":
            bwrap = directory / "bwrap"
            bwrap.write_text('#!/bin/bash\nset -euo pipefail\nargs=()\n'
                             'for arg in "$@"; do\n args+=("$arg")\n [ "$arg" != -- ] || break\ndone\n'
                             'exec /usr/bin/bwrap "${args[@]}" /usr/bin/sh -ec '
                             "'printf running >/workspace/cancellation-started; exec /usr/bin/sleep 120'\n")
            bwrap.chmod(0o700)
        if case == "null_baseline":
            invalid = directory / "baseline.json"
            invalid.write_text("null\n")
            baseline = [str(invalid)]
        environment = dict(os.environ, PATH=str(directory) + ":" + os.environ["PATH"])
        if case == "direct_io_when_asked":
            environment["TOOLBOX_LOOP_DIRECT_IO"] = "on"
        if publication:
            # Hold the real JSON writer after it owns its exclusive staging file.
            hook = directory / "sitecustomize.py"
            hook.write_text("import json, time\nfrom pathlib import Path\n"
                            "def hold_report(*args, **kwargs):\n"
                            f"    Path({str(directory / 'publication-started')!r}).touch()\n"
                            "    time.sleep(120)\n"
                            "json.dump = hold_report\n")
            environment["PYTHONPATH"] = str(directory)
        if case == "missing_digest":
            # Produce a compatible baseline, then remove only its required digest.
            seed = directory / "complete-baseline.json"
            subprocess.run(["bash", str(sampler), str(image), str(seed), "20"],
                           env=environment, capture_output=True, text=True, check=True, timeout=90)
            incomplete = json.loads(seed.read_text())
            del incomplete["image_sha256"]
            seed.write_text(json.dumps(incomplete))
            baseline = [str(seed)]
            called.unlink()
        output = directory / "report.json"
        victim = directory / "unrelated-file"
        if case == "publication_symlink":
            victim.write_text("preserve this file\n")
            output.with_name(output.name + ".tmp").symlink_to(victim)
        root = None
        try:
            result = run_sampler(["bash", str(sampler), str(image), str(output), "20", *baseline],
                                 environment, marker, case)
            assert marker.is_file(), f"{case}: injection did not execute"
            root = mounted_root(marker)
            mounted = subprocess.run(["/usr/bin/mountpoint", "-q", str(root)]).returncode == 0
            if case in published:
                assert result.returncode == 0, f"{case} failed: {result.stderr}"
                report = json.loads(output.read_text())
                assert report["runs"] == 20 and len(report["steps"]) == 8
                # Exit 0 is the direct-I/O proof: the sampler refuses a device
                # whose dio reads other than 1, which direct_io_refused shows.
                assert all(len(step["samples_ms"]) == 20 for step in report["steps"].values())
            else:
                assert not output.exists(), f"{case}: incomplete report published"
            if case == "publication_symlink":
                assert victim.read_text() == "preserve this file\n", "report overwrote a symlink target"
            if publication:
                assert not list(directory.glob(f".{output.name}.*.tmp")), "cancelled publication leaked staging file"
            if case == "null_baseline":
                assert result.returncode != 0 and not called.exists(), "invalid baseline reached measurement"
                assert "baseline" in result.stderr
            if case == "missing_digest":
                assert result.returncode != 0 and not called.exists(), "incomplete baseline reached measurement"
                assert "baseline needs a SHA-256 image digest" in result.stderr
            if case == "unmount_failed":
                assert result.returncode != 0 and mounted and root.parent.exists()
                assert "unmount failed; retained" in result.stderr
            else:
                expected = (129 if case == "signal_hangup_during_publication"
                            else 143 if case.startswith("signal_") else 77 if case == "mount_failed"
                            else 0 if case in published else 1)
                assert result.returncode == expected, f"{case}: exit {result.returncode}, expected {expected}"
                assert not mounted and not root.parent.exists(), f"{case}: leaked mount or tree"
                assert not loops_on(image), f"{case}: a loop device still holds the image"
            print(json.dumps(dict(case=case, exit=result.returncode,
                                  mounted=mounted, tree_retained=root.parent.exists(),
                                  report_published=output.exists(), result="PASS")))
        finally:
            root = root or mounted_root(marker)
            if root is not None:
                if subprocess.run(["/usr/bin/mountpoint", "-q", str(root)]).returncode == 0:
                    subprocess.run(["/usr/bin/umount", str(root)], check=True)
                shutil.rmtree(root.parent, ignore_errors=True)
finally:
    shutil.rmtree(fixture)
PY
