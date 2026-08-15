#!/usr/bin/env python3
"""Self-tests for publish_coverage_badge.py.

The badge is public and it is the first thing a reader sees. The tests that
matter are the ones proving it refuses to publish a number it cannot stand
behind — a stale file, a partial capture, a run that never graded. A badge that
falls back to zero, or to yesterday's figure, is worse than no badge.

Run: python3 -m unittest discover -s scripts -t scripts -p 'publish_coverage_badge_test.py'
"""

import io
import json
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

import publish_coverage_badge as badge

GRADED = (
    "zig_line_coverage_pct=90.18\n"
    "zig_line_coverage_min_pct=89\n"
    "zig_measured_files=548\n"
    "zig_measured_lines=31079\n"
    "zig_components_measured=7\n"
    "zig_components_total=7\n"
    "zig_components_empty=\n"
)


def run(summary_text: str | None) -> tuple[int, str, str, dict | None]:
    """Invoke main() over a summary; return (code, stdout, stderr, payload)."""
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        summary = root / "zig-coverage.txt"
        if summary_text is not None:
            summary.write_text(summary_text, encoding="utf-8")
        output = root / "badges" / "coverage.json"
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            code = badge.main(["--summary-file", str(summary), "--output", str(output)])
        payload = json.loads(output.read_text(encoding="utf-8")) if output.exists() else None
        return code, out.getvalue(), err.getvalue(), payload


class PublishesWhatWasMeasured(unittest.TestCase):
    def test_badge_payload_reflects_measured_run(self) -> None:
        code, out, err, payload = run(GRADED)
        self.assertEqual(code, 0, err)
        self.assertEqual(payload["message"], "90.18%")
        self.assertEqual(payload["schemaVersion"], 1)
        # The denominator rides along so anyone opening the raw document sees
        # what the percentage was measured over.
        self.assertEqual(payload["measuredLines"], 31079)
        self.assertEqual(payload["measuredFiles"], 548)
        self.assertIn("90.18%", out)

    def test_the_message_is_the_gate_figure_not_a_rounding(self) -> None:
        """A badge reading 90% where the gate measured 89.6% is a lie by rounding."""
        code, _, err, payload = run(GRADED.replace("90.18", "89.63"))
        self.assertEqual(code, 0, err)
        self.assertEqual(payload["message"], "89.63%")

    def test_colour_degrades_before_the_floor_does(self) -> None:
        """A rate in the seventies must not render as a healthy badge."""
        self.assertEqual(badge.colour_for(96.0), "brightgreen")
        self.assertEqual(badge.colour_for(90.18), "green")
        self.assertEqual(badge.colour_for(86.0), "yellowgreen")
        self.assertEqual(badge.colour_for(76.0), "yellow")
        self.assertEqual(badge.colour_for(61.0), "orange")
        self.assertEqual(badge.colour_for(12.0), "red")


class RefusesWhatItCannotStandBehind(unittest.TestCase):
    def test_badge_refuses_an_absent_run(self) -> None:
        code, _, err, payload = run(None)
        self.assertEqual(code, 1)
        self.assertIsNone(payload)
        self.assertIn("the gate did not run", err)

    def test_badge_refuses_a_summary_missing_the_rate(self) -> None:
        code, _, err, payload = run("zig_measured_files=548\nzig_measured_lines=31079\n")
        self.assertEqual(code, 1)
        self.assertIsNone(payload)
        self.assertIn("zig_line_coverage_pct", err)

    def test_badge_refuses_a_rate_over_an_empty_denominator(self) -> None:
        """The failure that started this lane: a rate graded over almost nothing."""
        code, _, err, payload = run(
            GRADED.replace("zig_measured_lines=31079", "zig_measured_lines=0")
        )
        self.assertEqual(code, 1)
        self.assertIsNone(payload)
        self.assertIn("a rate over nothing is not a rate", err)

    def test_badge_refuses_a_partial_capture(self) -> None:
        """Two components graded ~92% where all of them measure ~90%."""
        code, _, err, payload = run(
            GRADED.replace("zig_components_measured=7", "zig_components_measured=2")
        )
        self.assertEqual(code, 1)
        self.assertIsNone(payload)
        self.assertIn("subset flatters", err)

    def test_badge_refuses_a_non_numeric_measurement(self) -> None:
        code, _, err, payload = run(GRADED.replace("90.18", "unknown"))
        self.assertEqual(code, 1)
        self.assertIsNone(payload)
        self.assertIn("non-numeric", err)

    def test_badge_refuses_an_impossible_rate(self) -> None:
        code, _, err, payload = run(GRADED.replace("90.18", "1000"))
        self.assertEqual(code, 1)
        self.assertIsNone(payload)
        self.assertIn("outside 0-100", err)

    def test_a_refusal_leaves_no_stale_badge_behind(self) -> None:
        """Nothing is written on refusal, so the branch keeps its last good value
        rather than gaining a zero."""
        _, _, _, payload = run(None)
        self.assertIsNone(payload)


if __name__ == "__main__":
    unittest.main()
