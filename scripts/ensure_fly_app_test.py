"""Self-tests for ensure_fly_app.sh.

The script's whole reason for existing is that it REFUSES to report success
when an app is not actually running, so the negative cases are the point. A
fake flyctl is injected through $FLYCTL; nothing reaches Fly.
"""

import os
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "ensure_fly_app.sh"

# Two machines started, which satisfies a desired count of 1 or 2.
STARTED_TWO = '[{"id":"a","state":"started"},{"id":"b","state":"started"}]'
STOPPED_TWO = '[{"id":"a","state":"stopped"},{"id":"b","state":"stopped"}]'
NO_MACHINES = "[]"


class EnsureFlyAppTest(unittest.TestCase):
    def run_script(self, *args, machine_list, record=None):
        """Run the script with a fake flyctl that prints `machine_list`."""
        with tempfile.TemporaryDirectory() as tmp:
            fake = Path(tmp) / "flyctl"
            log = Path(tmp) / "calls.log"
            fake.write_text(
                textwrap.dedent(
                    f"""\
                    #!/usr/bin/env bash
                    echo "$@" >> {log}
                    case "$1" in
                      "machine")  printf '%s' '{machine_list}' ;;
                      "image")    printf '%s' '[{{"Digest":"sha256:deadbeef"}}]' ;;
                      *)          : ;;
                    esac
                    """
                )
            )
            fake.chmod(0o755)
            env = dict(os.environ, FLYCTL=str(fake), PATH=os.environ["PATH"],
                       POLL_ATTEMPTS="2", POLL_SLEEP_SECONDS="0")
            proc = subprocess.run(
                ["bash", str(SCRIPT), *args],
                capture_output=True,
                text=True,
                env=env,
            )
            calls = log.read_text() if log.exists() else ""
            if record is not None:
                record.append(calls)
            return proc, calls

    def test_running_app_at_desired_count_succeeds(self):
        proc, _ = self.run_script("otelcol-dev", "deploy/fly/otelcol-dev", "1",
                                  machine_list=STARTED_TWO)
        self.assertEqual(proc.returncode, 0, proc.stderr)

    def test_records_the_deployed_digest(self):
        proc, _ = self.run_script("otelcol-dev", "deploy/fly/otelcol-dev", "1",
                                  machine_list=STARTED_TWO)
        self.assertIn("sha256:deadbeef", proc.stdout)

    def test_deploys_from_context_when_no_machines_exist(self):
        # The positional build context is what makes the image's COPY resolve;
        # a deploy without it is the failure this asserts against.
        _, calls = self.run_script("otelcol-dev", "deploy/fly/otelcol-dev", "1",
                                   machine_list=NO_MACHINES)
        self.assertIn("deploy deploy/fly/otelcol-dev --app otelcol-dev", calls)

    def test_scales_to_the_desired_count(self):
        _, calls = self.run_script("otelcol-prod", "deploy/fly/otelcol-prod", "2",
                                   machine_list=STARTED_TWO)
        self.assertIn("scale count 2 --app otelcol-prod", calls)

    def test_fails_when_machines_never_start(self):
        # The defect the script exists to prevent: reporting success while the
        # collector is not serving, so the caller deploys a daemon at it.
        proc, _ = self.run_script("otelcol-dev", "deploy/fly/otelcol-dev", "1",
                                  machine_list=STOPPED_TWO)
        self.assertEqual(proc.returncode, 1)
        self.assertIn("never reached", proc.stderr)

    def test_fails_when_running_count_is_below_desired(self):
        one_started = '[{"id":"a","state":"started"},{"id":"b","state":"stopped"}]'
        proc, _ = self.run_script("otelcol-prod", "deploy/fly/otelcol-prod", "2",
                                  machine_list=one_started)
        self.assertEqual(proc.returncode, 1)

    def test_rejects_a_non_numeric_count(self):
        proc, _ = self.run_script("otelcol-dev", "deploy/fly/otelcol-dev", "two",
                                  machine_list=STARTED_TWO)
        self.assertEqual(proc.returncode, 2)

    def test_rejects_a_zero_count(self):
        # Zero would make every later assertion vacuous: nothing running still
        # satisfies "at least zero running".
        proc, _ = self.run_script("otelcol-dev", "deploy/fly/otelcol-dev", "0",
                                  machine_list=NO_MACHINES)
        self.assertEqual(proc.returncode, 2)

    def test_rejects_wrong_argument_count(self):
        proc, _ = self.run_script("otelcol-dev", machine_list=STARTED_TWO)
        self.assertEqual(proc.returncode, 2)


if __name__ == "__main__":
    unittest.main()
