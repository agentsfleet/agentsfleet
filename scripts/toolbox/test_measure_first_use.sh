#!/usr/bin/env bash
# Native Linux regression proofs for first-use mount cleanup.
# Optional argument selects the pre-fix sampler for the red-green proof.
set -euo pipefail
readonly SAMPLER="${1:-scripts/toolbox/measure_first_use.sh}"
if [ "$(uname -s)" != Linux ] || [ "$(id -u)" -ne 0 ]; then
  echo "test_measure_first_use.sh: needs Linux root" >&2
  exit 1
fi
python3 - "$SAMPLER" <<'PY'
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
cases = ["signal_after_mount", "mount_failed", "command_failed", "unmount_failed"]
random.SystemRandom().shuffle(cases)
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
        environment = dict(os.environ, PATH=str(directory) + ":" + os.environ["PATH"])
        output = directory / "report.json"
        root = None
        try:
            result = subprocess.run(["bash", str(sampler), str(image), str(output), "20"],
                                    env=environment, capture_output=True, text=True, timeout=45)
            assert marker.is_file(), f"{case}: injection did not execute"
            root = Path(marker.read_text().strip())
            assert root.parent.name.startswith("toolbox-first-use.") and root.name == "root"
            mounted = subprocess.run(["/usr/bin/mountpoint", "-q", str(root)]).returncode == 0
            assert not output.exists(), f"{case}: incomplete report published"
            if case == "unmount_failed":
                assert result.returncode != 0 and mounted and root.parent.exists()
                assert "unmount failed; retained" in result.stderr
            else:
                expected = 143 if case == "signal_after_mount" else 77 if case == "mount_failed" else 1
                assert result.returncode == expected, f"{case}: exit {result.returncode}, expected {expected}"
                assert not mounted and not root.parent.exists(), f"{case}: leaked mount or tree"
            print(json.dumps(dict(case=case, exit=result.returncode,
                                  mounted=mounted, tree_retained=root.parent.exists(),
                                  report_published=output.exists(), result="PASS")))
        finally:
            if root is not None:
                if subprocess.run(["/usr/bin/mountpoint", "-q", str(root)]).returncode == 0:
                    subprocess.run(["/usr/bin/umount", str(root)], check=True)
                shutil.rmtree(root.parent, ignore_errors=True)
finally:
    shutil.rmtree(fixture)
PY
