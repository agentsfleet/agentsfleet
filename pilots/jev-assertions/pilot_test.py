"""Refusal, isolation and accounting checks; never requests model advice."""

import datetime
import json
import shutil
import sys
import tempfile
import unittest
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from unittest.mock import patch

sys.dont_write_bytecode = True

import pilot_checks as checks
import pilot_measure as measure

PILOT = checks.ROOT / checks.PREFIX
FROZEN = "freeze.json"
CASE_IDENTIFIER = "p01"


class PilotTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.pilot = self.root / checks.PREFIX
        shutil.copytree(PILOT, self.pilot, ignore=shutil.ignore_patterns("receipts", ".proof-*", "__pycache__"))
        checks.save(self.pilot / FROZEN, checks.freeze_payload(self.root))

    def approval(self):
        return {"frozen_digest": checks.verify_freeze(self.root),
                "runner_digest": measure.runner_digest(self.root), "model": measure.MODEL,
                "provider_requests_max": measure.REQUEST_CAP, "ceiling_usd": str(measure.SPENDING_CAP),
                "input_usd_per_million": str(measure.INPUT_RATE), "output_usd_per_million": str(measure.OUTPUT_RATE),
                "max_input_tokens_per_request": measure.TOKEN_CAP, "approved_by": "Kishore",
                "owner_quote": "Synthetic authorization used only in isolated unit tests.",
                "pricing_checked_url": measure.CONFIRMED_PRICING,
                "pricing_checked_at": datetime.datetime.now(datetime.timezone.utc).isoformat()}

    def test_frozen_cases(self):
        ledger, uploads = checks.validate_inputs(self.root)
        self.assertEqual(20, len(uploads))
        self.assertEqual(4, len(ledger["families"]))
        self.assertLessEqual(max(row["state_bytes"] for row in uploads), checks.STATE_CAP)
        for case in ledger["cases"]:
            self.assertEqual(case["expected"] == "insufficient", bool(case["omitted_context"]))

    def test_fault_discrimination(self):
        receipt = checks.load(PILOT / "receipts/checks.json")
        self.assertEqual({"passed": 20}, receipt["correct"]["counts"])
        self.assertEqual({"passed": 12, "failed": 8}, receipt["faulty"]["counts"])
        ledger = checks.load(self.pilot / "cases.json")
        report = {"numRuntimeErrorTestSuites": 1, "testResults": []}
        with self.assertRaisesRegex(ValueError, "Runtime/import"):
            checks.validate_proof(ledger, report, 1, "faulty")

    def test_freeze_refusal(self):
        path = self.pilot / f"fixtures/{CASE_IDENTIFIER}/test.tsx"
        path.write_text(path.read_text() + "\n// changed\n")
        with self.assertRaisesRegex(ValueError, "Frozen bytes changed"):
            checks.verify_freeze(self.root)

    def test_answer_leak_refused(self):
        path = self.pilot / "manifests/a.json"
        manifest = checks.load(path)
        manifest["items"][0]["expected"] = "weak"
        checks.save(path, manifest)
        with self.assertRaisesRegex(ValueError, "model-input fields"):
            checks.validate_inputs(self.root)

    def test_symlink_escape_refused(self):
        path = self.pilot / f"fixtures/{CASE_IDENTIFIER}/implementation.tsx"
        outside = Path(self.temporary.name).parent / (self.root.name + "-outside")
        outside.write_text(path.read_text())
        self.addCleanup(outside.unlink)
        path.unlink()
        path.symlink_to(outside)
        with self.assertRaisesRegex(ValueError, "escapes"):
            checks.validate_inputs(self.root)

    def test_insufficient_helper_cannot_enter_upload(self):
        path = self.pilot / "manifests/a.json"
        manifest = checks.load(path)
        item = next(item for item in manifest["items"] if item["id"] == "p03")
        item["evidence"].append({"role": "context", "path": str(checks.PREFIX / "helpers/check-classes.ts"),
                                 "selector": {"kind": "file"}})
        checks.save(path, manifest)
        with self.assertRaisesRegex(ValueError, "evidence changed"):
            checks.validate_inputs(self.root)

    def test_review_freeze(self):
        review = checks.load(self.pilot / "reviews/unaided-author.json")
        self.assertFalse(review["advice_seen"])
        self.assertFalse(review["independent"])
        self.assertFalse(review["blinded"])
        self.assertEqual(20, len(review["cases"]))
        self.assertIsNone(review["review_seconds"])

    def test_live_approval(self):
        with self.assertRaisesRegex(ValueError, "Approval does not match"):
            measure.approved(self.root, {})
        with patch.object(measure, "run_process", return_value=(0, "0.13.0\n", "")):
            self.assertEqual(checks.verify_freeze(self.root), measure.approved(self.root, self.approval()))
            for field, bad in (("model", "other"), ("ceiling_usd", "1"), ("runner_digest", "changed")):
                approval = self.approval()
                approval[field] = bad
                with self.assertRaisesRegex(ValueError, "Approval does not match"):
                    measure.approved(self.root, approval)

    def test_stale_price_and_changed_engine_refused(self):
        approval = self.approval()
        approval["pricing_checked_at"] = "2020-01-01T00:00:00+00:00"
        with self.assertRaisesRegex(ValueError, "stale"):
            measure.approved(self.root, approval)
        with (patch.object(measure, "run_process", return_value=(0, "0.12.0\n", "")),
              self.assertRaisesRegex(ValueError, "engine pin")):
            measure.approved(self.root, self.approval())

    def test_request_reservations(self):
        with patch.object(measure, "approved", return_value="frozen-test"):
            first = measure.reserve(self.root, "a", {})
            self.assertEqual(10, sum(row["request_cap"] for row in first["batches"]))
            with self.assertRaisesRegex(ValueError, "already reserved"):
                measure.reserve(self.root, "a", {})
            second = measure.reserve(self.root, "b", {})
            self.assertEqual(20, sum(row["request_cap"] for row in second["batches"]))
            with self.assertRaisesRegex(ValueError, "Unknown batch"):
                measure.reserve(self.root, "c", {})
        self.assertEqual(2, len(checks.load(self.pilot / "receipts/reservations.json")["batches"]))

    def test_concurrent_duplicate_reservation(self):
        def reserve():
            try:
                measure.reserve(self.root, "a", {})
                return True
            except ValueError:
                return False
        with (patch.object(measure, "approved", return_value="frozen-test"),
              ThreadPoolExecutor(max_workers=2) as pool):
            outcomes = list(pool.map(lambda _: reserve(), range(2)))
        self.assertEqual([False, True], sorted(outcomes))

    def test_corrupt_reservations_fail_closed(self):
        path = self.pilot / "receipts/reservations.json"
        checks.save(path, {"frozen_digest": "frozen-test", "batches": [
            {"batch": "a", "request_cap": -10, "conservative_usd": "0"}]})
        with (patch.object(measure, "approved", return_value="frozen-test"),
              self.assertRaisesRegex(ValueError, "malformed")):
            measure.reserve(self.root, "b", {})

    def test_attempt_accounting(self):
        case = checks.load(self.pilot / "cases.json")["cases"][0]
        result = {"status": "complete", "answer": {"choice": "weak", "confidence": 0.6,
                  "probabilities": {"weak": 0.9}}, "assessment": {"strength": 0.6, "uncertain": True, "concern": False},
                  "requests": 1, "elapsedMs": 12, "usage": {"input_tokens": 100, "output_tokens": 8}}
        row = measure.case_measurement(case, result)
        self.assertTrue(row["native_matches"])
        self.assertTrue(row["low_strength"])
        self.assertFalse(row["concern"])
        self.assertEqual("0.0000042", row["known_usd"])
        failed = measure.case_measurement(case, {"status": "incomplete", "requests": 1, "usage": None})
        self.assertEqual(1, failed["requests"])
        self.assertIsNone(failed["known_usd"])
        self.assertIsNone(failed["native"])

    def test_insufficient_is_not_just_low_confidence(self):
        case = checks.load(self.pilot / "cases.json")["cases"][2]
        result = {"status": "complete", "answer": {"choice": "insufficient", "confidence": 1},
                  "assessment": {"strength": 1, "uncertain": True, "concern": False}, "usage": None}
        row = measure.case_measurement(case, result)
        self.assertFalse(row["low_strength"])
        self.assertTrue(row["insufficient_requires_inspection"])
        self.assertTrue(row["uncertain"])

    def test_comparison_metrics(self):
        ledger = checks.load(self.pilot / "cases.json")
        rows = [measure.case_measurement(case, {}) for case in ledger["cases"]]
        metrics = measure.group_metrics(rows)
        self.assertEqual(20, metrics["cases"])
        self.assertEqual(20, metrics["unavailable"])
        self.assertEqual(12, metrics["unavailable_weaknesses"])
        self.assertEqual(0, metrics["native_misses"])
        self.assertEqual("unmeasured", measure.paired_reviews(self.pilot, ledger["cases"])["status"])

    def test_model_classification_confusion_counts(self):
        ledger = checks.load(self.pilot / "cases.json")
        rows = []
        for case, native in zip(ledger["cases"][:3], ("weak", "weak", "exact")):
            rows.append(measure.case_measurement(case, {"status": "complete", "answer": {"choice": native},
                        "assessment": {"strength": 1, "concern": native == "weak"}, "usage": None}))
        metrics = measure.group_metrics(rows)
        self.assertEqual(1, metrics["confirmed_findings"])
        self.assertEqual(1, metrics["false_alarms"])
        self.assertEqual(1, metrics["native_matches"])

    def test_live_failure_consumes_reservation_without_retry(self):
        with (patch.object(measure, "approved", return_value="frozen-test"),
              patch.object(measure, "run_process", side_effect=TimeoutError("timed out")) as command):
            receipt = measure.judge_batch(self.root, "a", True, {})
        command.assert_called_once()
        self.assertEqual(10, receipt["reserved_requests"])
        self.assertIsNone(receipt["report"]["requests"])
        self.assertEqual(10, len(receipt["report"]["results"]))
        with self.assertRaisesRegex(ValueError, "cannot be replaced"):
            measure.judge_batch(self.root, "a", True, {})

    def test_subprocess_timeout_stops_child(self):
        arguments = [sys.executable, "-c", "import time; time.sleep(30)"]
        with self.assertRaisesRegex(TimeoutError, "process group was stopped"):
            checks.run_process(arguments, timeout=0.05)

    def test_native_report_counts_and_choices(self):
        manifest = checks.load(self.pilot / "manifests/a.json")
        report = measure.unavailable(manifest["items"], "synthetic test failure")
        report["requests"] = 10
        for result in report["results"]:
            result["requests"] = 1
        measure.validate_report(report, manifest, True)
        report["requests"] = 9
        with self.assertRaisesRegex(ValueError, "reconcile"):
            measure.validate_report(report, manifest, True)
        report["requests"] = 10
        first = report["results"][0]
        first.update(status="complete", model=measure.MODEL, mode="live", answer={"choice": "unknown"})
        with self.assertRaisesRegex(ValueError, "answer"):
            measure.validate_report(report, manifest, True)
        first["answer"]["choice"] = "weak"
        measure.validate_report(report, manifest, True)
        with self.assertRaisesRegex(ValueError, "request count"):
            measure.validate_report(report, manifest, False)

    def test_foreign_live_receipt_refused(self):
        with patch.object(measure, "approved", return_value=checks.verify_freeze(self.root)):
            measure.reserve(self.root, "a", {})
        checks.save(self.pilot / "receipts/live-a.json", {"frozen_digest": "foreign", "mode": "live",
                                                        "reserved_requests": 10})
        with self.assertRaisesRegex(ValueError, "frozen reservation"):
            measure.summarize(self.root)

    def test_review_contamination_seal(self):
        paths = [self.pilot / f"reviews/{name}.json" for name in ("unaided", "assisted")]
        unaided = {"frozen_at": "2026-10-08T01:00:00+00:00"}
        checks.save(paths[0], unaided)
        assisted = {"advice_first_seen_at": "2026-10-08T02:00:00+00:00",
                    "unaided_sha256": checks.digest(paths[0].read_bytes()), "advice_sources": {}}
        for batch in checks.BATCHES:
            path = self.pilot / f"receipts/live-{batch}.json"
            checks.save(path, {"synthetic_unit_test": True})
            assisted["advice_sources"][batch] = checks.digest(path.read_bytes())
        measure.review_seal(self.pilot, paths, unaided, assisted)
        unaided["frozen_at"] = "2026-10-08T03:00:00+00:00"
        with self.assertRaisesRegex(ValueError, "before advice"):
            measure.review_seal(self.pilot, paths, unaided, assisted)
        unaided["frozen_at"] = "2026-10-08T01:00:00+00:00"
        paths[0].write_text(paths[0].read_text() + "\n")
        with self.assertRaisesRegex(ValueError, "bind the frozen"):
            measure.review_seal(self.pilot, paths, unaided, assisted)

    def test_malformed_output_retains_attempt_receipt(self):
        for output in ("[]", '{"stage":"verify","advisory":true,"results":[null]}'):
            with patch.object(measure, "run_process", return_value=(1, output, "native diagnostic")):
                receipt = measure.judge_batch(self.root, "a")
            retained = checks.load(self.pilot / "receipts/offline-a.json")
            self.assertEqual(receipt, retained)
            self.assertEqual(output, retained["stdout"])
            self.assertEqual("native diagnostic", retained["stderr"])
            self.assertEqual(1, retained["exit"])
            self.assertTrue(retained["collection_error"])
            self.assertEqual(10, len(retained["report"]["results"]))
            self.assertIsNone(retained["report"]["requests"])

    def test_unexpected_exit_retains_valid_native_answers(self):
        manifest = checks.load(self.pilot / "manifests/a.json")
        report = measure.unavailable(manifest["items"], "synthetic")
        report["requests"] = 10
        for result in report["results"]:
            result.update(requests=1, status="complete", model=measure.MODEL, mode="live",
                          answer={"choice": "weak"})
        with (patch.object(measure, "approved", return_value=checks.verify_freeze(self.root)),
              patch.object(measure, "run_process", return_value=(1, json.dumps(report), "native diagnostic"))):
            receipt = measure.judge_batch(self.root, "a", True, {})
        self.assertEqual(report, receipt["report"])
        self.assertEqual(report, receipt["raw_report"])
        self.assertEqual(1, receipt["exit"])
        self.assertEqual("native diagnostic", receipt["stderr"])
        summary = measure.summarize(self.root)
        self.assertEqual(10, summary["reported_requests"])
        self.assertEqual(10, summary["native"]["available"])

    def test_review_findings_are_separate_from_classifications(self):
        rows = [{"native": "exact", "expected": "exact", "finding": True,
                 "native_matches": True, "low_strength": False, "concern": True},
                {"native": "weak", "expected": "weak", "finding": False,
                 "native_matches": True, "low_strength": False, "concern": False}]
        metrics = measure.group_metrics(rows)
        self.assertEqual(2, metrics["native_matches"])
        self.assertEqual(1, metrics["false_alarms"])
        self.assertEqual(0, metrics["confirmed_findings"])
        self.assertEqual(1, metrics["finding_misses"])

    def test_summary_rejects_corrupt_reservation_totals(self):
        path = self.pilot / "receipts/reservations.json"
        frozen = checks.verify_freeze(self.root)
        for ledger in ({"frozen_digest": "foreign", "batches": []},
                       {"frozen_digest": frozen, "batches": [
                           {"batch": "a", "request_cap": -10, "conservative_usd": "-99"}]}):
            checks.save(path, ledger)
            with self.assertRaises(ValueError):
                measure.summarize(self.root)


if __name__ == "__main__":
    unittest.main(verbosity=2)
