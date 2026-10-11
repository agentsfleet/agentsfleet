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
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sampler = Path(sys.argv[1]).resolve()
fixture = Path(tempfile.mkdtemp(prefix="first-use-cleanup-", dir="/var/tmp"))
source = fixture / "source"
source.mkdir()
(source / "notice").write_text("cleanup fixture\n")
image = fixture / "fixture.erofs"
subprocess.run(["mkfs.erofs", "--quiet", str(image), str(source)], check=True)
cases = ["signal_after_mount", "mount_failed", "command_failed", "unmount_failed",
         "null_baseline", "publication_symlink"]
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


try:
    for case in cases:
        directory = fixture / case
        directory.mkdir()
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
        if case in ("null_baseline", "publication_symlink"):
            bwrap = directory / "bwrap"
            bwrap.write_text(f'#!/bin/bash\ntouch "{called}"\nexit 0\n')
            bwrap.chmod(0o700)
        if case == "null_baseline":
            invalid = directory / "baseline.json"
            invalid.write_text("null\n")
            baseline = [str(invalid)]
        environment = dict(os.environ, PATH=str(directory) + ":" + os.environ["PATH"])
        output = directory / "report.json"
        victim = directory / "unrelated-file"
        if case == "publication_symlink":
            victim.write_text("preserve this file\n")
            output.with_name(output.name + ".tmp").symlink_to(victim)
        root = None
        try:
            result = subprocess.run(["bash", str(sampler), str(image), str(output), "20", *baseline],
                                    env=environment, capture_output=True, text=True, timeout=90)
            assert marker.is_file(), f"{case}: injection did not execute"
            root = mounted_root(marker)
            mounted = subprocess.run(["/usr/bin/mountpoint", "-q", str(root)]).returncode == 0
            if case == "publication_symlink":
                assert result.returncode == 0, f"publication failed: {result.stderr}"
                assert victim.read_text() == "preserve this file\n", "report overwrote a symlink target"
                report = json.loads(output.read_text())
                assert report["runs"] == 20 and len(report["steps"]) == 8
                assert all(len(step["samples_ms"]) == 20 for step in report["steps"].values())
            else:
                assert not output.exists(), f"{case}: incomplete report published"
            if case == "null_baseline":
                assert result.returncode != 0 and not called.exists(), "invalid baseline reached measurement"
                assert "baseline" in result.stderr
            if case == "unmount_failed":
                assert result.returncode != 0 and mounted and root.parent.exists()
                assert "unmount failed; retained" in result.stderr
            else:
                expected = (143 if case == "signal_after_mount" else 77 if case == "mount_failed"
                            else 0 if case == "publication_symlink" else 1)
                assert result.returncode == expected, f"{case}: exit {result.returncode}, expected {expected}"
                assert not mounted and not root.parent.exists(), f"{case}: leaked mount or tree"
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
