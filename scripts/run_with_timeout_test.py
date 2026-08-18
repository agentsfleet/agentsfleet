import subprocess
import sys
import tempfile
import unittest
import json
from pathlib import Path

import run_with_timeout


class TimeoutTests(unittest.TestCase):
    def test_success_and_failure_status_are_preserved(self):
        self.assertEqual(0, run_with_timeout.run([sys.executable, "-c", "pass"], 2, "ok"))
        self.assertEqual(7, run_with_timeout.run([sys.executable, "-c", "raise SystemExit(7)"], 2, "bad"))

    def test_timeout_names_owner_and_returns_124(self):
        with tempfile.TemporaryDirectory() as directory:
            timing = Path(directory) / "timing.json"
            result = subprocess.run(
                [sys.executable, "scripts/run_with_timeout.py", "--seconds", "1",
                 "--label", "integration-shard-2", "--timing-output", str(timing),
                 "--", sys.executable, "-c", "import time; time.sleep(10)"],
                capture_output=True, text=True, check=False,
            )
            record = json.loads(timing.read_text(encoding="utf-8"))
        self.assertEqual(124, result.returncode)
        self.assertIn("integration-shard-2 timed out", result.stderr)
        self.assertEqual("timeout", record["outcome"])
        self.assertEqual(124, record["exit_code"])
        self.assertGreaterEqual(record["finished_at_ms"], record["started_at_ms"])

    def test_failed_owner_still_writes_timing_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            timing = Path(directory) / "timing.json"
            self.assertEqual(
                7,
                run_with_timeout.run(
                    [sys.executable, "-c", "raise SystemExit(7)"],
                    2, "failed-owner", timing,
                ),
            )
            record = json.loads(timing.read_text(encoding="utf-8"))
        self.assertEqual("failed", record["outcome"])
        self.assertEqual(7, record["exit_code"])

    def test_external_start_includes_setup_before_the_process(self):
        started = run_with_timeout.time.time_ns() // 1_000_000 - 100
        with tempfile.TemporaryDirectory() as directory:
            timing = Path(directory) / "timing.json"
            self.assertEqual(
                0,
                run_with_timeout.run(
                    [sys.executable, "-c", "pass"], 2, "owner", timing, started,
                ),
            )
            record = json.loads(timing.read_text(encoding="utf-8"))
        self.assertEqual(started, record["started_at_ms"])
        self.assertGreaterEqual(record["duration_ms"], 100)


if __name__ == "__main__":
    unittest.main()
