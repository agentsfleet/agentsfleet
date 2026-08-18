import tempfile
import unittest
from pathlib import Path

import check_verification_graph as check
import verification_evidence as evidence


def listing(skip=None, duplicate=None, extra=None):
    lines = []
    for index, lane in enumerate(check.POLICY):
        if lane == skip:
            continue
        root = f"root-{index}"
        lines += [f"ROOT\t{lane}\t{root}", f"TEST\t{lane}\t{root}\tpkg.integration: case {index}"]
        if lane == duplicate:
            lines.append(f"ROOT\t{lane}\t{root}")
    if extra:
        lines.append(f"ROOT\t{extra}\tother")
    return "\n".join(lines)


class GraphTests(unittest.TestCase):
    def test_registered_roots_have_one_execution_owner(self):
        graph = check.build_graph([("fixture", listing())])
        self.assertEqual(list(check.POLICY), [root["lane"] for root in graph["roots"]])
        self.assertEqual(64, len(graph["graph_digest"]))

    def test_graph_digest_is_independent_of_concurrent_listing_order(self):
        records = listing().splitlines()
        forward = check.build_graph([("fixture", "\n".join(records))])
        reverse = check.build_graph([("fixture", "\n".join(reversed(records)))])
        self.assertEqual(forward, reverse)

    def test_duplicate_or_missing_root_fails_closed(self):
        with self.assertRaises(check.VerificationError) as caught:
            check.build_graph([("fixture", listing(skip="agentsfleet-s3-tests", duplicate="agentsfleetd-tests", extra="unknown"))])
        self.assertIn("agentsfleet-s3-tests", str(caught.exception))
        self.assertIn("agentsfleetd-tests:root-0", str(caught.exception))
        self.assertIn("unknown", str(caught.exception))

    def test_two_distinct_roots_for_one_lane_are_duplicates(self):
        fixture = listing() + "\nROOT\tagentsfleetd-tests\tanother-root"
        with self.assertRaisesRegex(check.VerificationError, "another-root"):
            check.build_graph([("fixture", fixture)])

    def test_malformed_records_name_every_offender(self):
        with self.assertRaises(check.VerificationError) as caught:
            check.parse_listing("BAD\tx\nROOT\tonly", "wire")
        self.assertIn("wire:1", str(caught.exception)); self.assertIn("wire:2", str(caught.exception))

    def test_tests_without_matching_root_are_ignored(self):
        roots, tests = check.parse_listing("TEST\tlane\troot\tname\nROOT\tother\troot")
        self.assertEqual([("other", "root")], roots); self.assertEqual([], tests)


class ShardTests(unittest.TestCase):
    def test_fnv_matches_zig_constants(self):
        self.assertEqual(0xa430d84680aabd0b, check.fnv1a64("hello"))

    def test_shard_union_matches_unsharded_discovery(self):
        candidates = [f"suite.integration: case {index}" for index in range(30)]
        candidates.append("suite.dependency.test_0")
        candidates.append("cmd.test.integration: daemon boot -> SIGTERM -> drain")
        shards = check.assign_shards(candidates, 4)
        self.assertTrue(check.validate_shards(candidates, shards))
        self.assertEqual([candidates[-1]], shards[-1])

    def test_compiler_implicit_module_tests_are_shard_candidates(self):
        graph = {"roots": [{
            "lane": "agentsfleetd-integration-tests",
            "tests": ["suite.test_0", "suite.test.unit only", "suite.test.integration: live"],
        }]}
        self.assertEqual(
            ["suite.test_0", "suite.test.integration: live"],
            check.shard_candidates(graph),
        )

    def test_empty_duplicate_and_missing_shards_fail(self):
        with self.assertRaisesRegex(check.VerificationError, "duplicate tests.*missing tests.*empty shards"):
            check.validate_shards(["a", "b"], [["a", "a"], []])

    def test_runtime_shard_union_matches_unsharded_compiler_discovery(self):
        outputs = {
            (0, 1): ["a", "b", "lifecycle"],
            (0, 3): ["a"],
            (1, 3): ["b"],
            (2, 3): ["lifecycle"],
        }
        def run(command, **options):
            index = int(command[1].partition("=")[2])
            count = int(command[2].partition("=")[2])
            options["stderr"].write("\n".join(outputs[(index, count)]) + "\n")
            return type("Result", (), {})()
        self.assertEqual(3, len(evidence.validate_runtime_shards("binary", 3, outputs[(0, 1)], run)))
        outputs[(1, 3)] = ["a"]
        with self.assertRaisesRegex(evidence.EvidenceError, "omissions or duplicates"):
            evidence.validate_runtime_shards("binary", 3, outputs[(0, 1)], run)


class ResultTests(unittest.TestCase):
    identity = {"source_revision": "source", "toolchain_identity": "zig", "graph_digest": "graph", "environment_identity": {"system": "x", "machine": "y"}}

    def manifest(self, execution, **changes):
        labels = {"unit": "unit", "runner-kernel": "runner-kernel",
                  "integration-shard": "integration"}
        system = "Linux" if execution == "runner-kernel" else "x"
        value = dict(self.identity, execution=execution, outcome="success",
                     environment_identity={"system": system, "machine": "y",
                                           "label": labels.get(execution)},
                     reports=[{"path": "report", "sha256": "digest"}])
        value.update(changes)
        return value

    def complete(self):
        return [self.manifest("unit"), self.manifest("runner-kernel"),
                self.manifest("integration-shard", shard_index=0, shard_count=2, isolation_key="a"),
                self.manifest("integration-shard", shard_index=1, shard_count=2, isolation_key="b")]

    def test_complete_results_are_accepted(self):
        self.assertTrue(check.validate_results(self.complete(), self.identity, 2, lambda _: True))

    def test_stale_or_partial_artifact_is_rejected(self):
        fields = [field for field in self.identity if field != "environment_identity"]
        for field in fields:
            manifests = self.complete(); manifests[0][field] = "changed"
            with self.subTest(field=field), self.assertRaisesRegex(check.VerificationError, field):
                check.validate_results(manifests, self.identity, 2, lambda _: True)
        manifests = self.complete()[:-1]
        with self.assertRaisesRegex(check.VerificationError, "missing shards: 1"):
            check.validate_results(manifests, self.identity, 2, lambda _: True)

    def test_duplicate_shard_empty_report_and_failed_outcome_are_rejected(self):
        cases = []
        duplicate = self.complete(); duplicate[-1]["shard_index"] = 0; cases.append((duplicate, "duplicate shards"))
        empty = self.complete(); cases.append((empty, "empty report"))
        failed = self.complete(); failed[0]["outcome"] = "failed"; cases.append((failed, "failed outcome"))
        for manifests, message in cases:
            with self.subTest(message=message), self.assertRaisesRegex(check.VerificationError, message):
                check.validate_results(manifests, self.identity, 2, (lambda _: False) if message == "empty report" else (lambda _: True))

    def test_unknown_execution_and_out_of_range_shard_are_rejected(self):
        manifests = self.complete()
        manifests.append(self.manifest("unknown"))
        manifests[-2]["shard_index"] = 4
        with self.assertRaises(check.VerificationError) as caught:
            check.validate_results(manifests, self.identity, 2, lambda _: True)
        self.assertIn("unknown executions: unknown", str(caught.exception))
        self.assertIn("unexpected shards: 4", str(caught.exception))

    def test_missing_or_duplicate_isolation_keys_are_rejected(self):
        missing = self.complete(); missing[-1]["isolation_key"] = ""
        with self.assertRaisesRegex(check.VerificationError, "missing shard isolation keys"):
            check.validate_results(missing, self.identity, 2, lambda _: True)
        duplicate = self.complete(); duplicate[-1]["isolation_key"] = "a"
        with self.assertRaisesRegex(check.VerificationError, "duplicate shard isolation keys"):
            check.validate_results(duplicate, self.identity, 2, lambda _: True)

    def test_execution_environment_class_is_enforced(self):
        manifests = self.complete(); manifests[1]["environment_identity"]["system"] = "Darwin"
        manifests[-1]["environment_identity"]["label"] = "unit"
        with self.assertRaises(check.VerificationError) as caught:
            check.validate_results(manifests, self.identity, 2, lambda _: True)
        self.assertIn("runner-kernel: execution environment is not Linux", str(caught.exception))
        self.assertIn("integration-shard: mismatched environment label", str(caught.exception))

    def test_write_result_requires_nonempty_reports(self):
        with tempfile.TemporaryDirectory() as directory:
            empty = Path(directory) / "empty"; empty.touch()
            with self.assertRaisesRegex(check.VerificationError, "empty or missing"):
                check.build_result(self.identity, "unit", "success", 1, "cold", [empty])
        with self.assertRaisesRegex(check.VerificationError, "one report"):
            check.build_result(self.identity, "unit", "success", 1, "cold", [])

    def test_single_result_reuse_requires_matching_successful_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            report = Path(directory) / "report"; report.write_text("coverage", encoding="utf-8")
            manifest = check.build_result(self.identity, "unit", "success", 1, "warm", [report])
            with self.assertRaisesRegex(check.VerificationError, "mismatched source_revision"):
                check.validate_result(manifest, {**self.identity, "source_revision": "new"}, "unit")
            self.assertIsNone(check.validate_result(manifest, self.identity, "unit"))

    def test_tampered_report_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            report = Path(directory) / "report"; report.write_text("original", encoding="utf-8")
            manifest = check.build_result(self.identity, "unit", "success", 1, "warm", [report])
            report.write_text("replacement", encoding="utf-8")
            with self.assertRaisesRegex(check.VerificationError, "tampered report"):
                check.validate_result(manifest, self.identity, "unit")


class TimingTests(unittest.TestCase):
    @staticmethod
    def worker(label, started, finished, outcome="success", exit_code=0):
        return {"label": label, "started_at_ms": started,
                "finished_at_ms": finished, "duration_ms": finished - started,
                "outcome": outcome, "exit_code": exit_code}

    def samples(self, candidate=60):
        return [{"scope": "local", "cache_state": state, "kind": kind,
                 "source_revision": "same", "image_identity": "image", "duration_ms": duration}
                for state in ("cold", "warm") for kind, duration in (("baseline", 100), ("candidate", candidate)) for _ in range(3)]

    def test_local_and_ci_critical_path_improves_by_threshold(self):
        result = evidence.compare_samples(self.samples(), "local")
        self.assertEqual({"cold": 40, "warm": 40}, result)

    def test_threshold_miss_fails(self):
        with self.assertRaisesRegex(evidence.EvidenceError, "below 35"):
            evidence.compare_samples(self.samples(66), "local")

    def test_unlike_samples_fail(self):
        samples = self.samples(); samples[0]["image_identity"] = "other"
        with self.assertRaisesRegex(evidence.EvidenceError, "unlike source/image"):
            evidence.compare_samples(samples, "local")

    def test_insufficient_samples_fail(self):
        with self.assertRaisesRegex(evidence.EvidenceError, "insufficient positive"):
            evidence.compare_samples(self.samples()[:-1], "local")

    def test_worker_overlap_summary_uses_real_intervals(self):
        records = [self.worker("runner-kernel", 100, 180),
                   self.worker("integration-shard-0", 110, 250),
                   self.worker("integration-shard-1", 120, 220)]
        summary = evidence.summarize_timings(
            records, ["runner-kernel", "integration-shard-0", "integration-shard-1"]
        )
        self.assertEqual(3, summary["peak_concurrency"])
        self.assertEqual(150, summary["fanout_wall_ms"])
        self.assertEqual(320, summary["summed_worker_ms"])

    def test_serial_failed_or_missing_worker_timing_fails(self):
        serial = [self.worker("runner-kernel", 100, 150),
                  self.worker("integration-shard-0", 150, 200)]
        with self.assertRaisesRegex(evidence.EvidenceError, "do not overlap"):
            evidence.summarize_timings(
                serial, ["runner-kernel", "integration-shard-0"]
            )
        failed = [self.worker("runner-kernel", 100, 150, "failed", 7)]
        with self.assertRaisesRegex(evidence.EvidenceError, "did not succeed"):
            evidence.summarize_timings(
                failed, ["runner-kernel", "integration-shard-0"]
            )


class WorkflowTests(unittest.TestCase):
    def test_ci_workflows_call_each_graph_owner_once(self):
        fixture = {"good.yml": "run: make test-integration\nrun: zig build test-integration-bin\n"}
        self.assertEqual([], evidence.find_orphans(fixture))

    def test_direct_integration_execution_is_orphaned(self):
        offenders = evidence.find_orphans({"bad.yml": "run: zig build test-integration --summary all\n"})
        self.assertEqual(["bad.yml:1"], offenders)


if __name__ == "__main__":
    unittest.main()
