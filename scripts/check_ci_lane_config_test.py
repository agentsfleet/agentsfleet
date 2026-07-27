"""Continuous Integration (CI) lane configuration gates.

Each lane defect these assert against cost real wall clock and was invisible
until someone read the run history:

  * a workflow with no `main` push trigger can only ever save caches under
    `refs/pull/<n>/merge`, which no other branch can restore — so every new
    branch rebuilds from cold;
  * a pre-warm step naming the wrong build step compiles a binary the job
    never runs, and leaves the one it does run cold;
  * `--privileged` grants a job that checks out branch code far more than the
    two container relaxations kcov actually needs;
  * the cache reclamation selector must never delete an entry that is still
    restorable.

The first three are configuration assertions. The fourth drives the real
selector script over synthetic listings, because that is where the judgement
lives.
"""

import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WORKFLOWS = ROOT / ".github" / "workflows"
SELECTOR = ROOT / "scripts" / "select-prunable-caches.sh"

# Zig workflows whose jobs restore a cache keyed on the same hashFiles inputs.
# A `main` push trigger is what makes those caches reachable from a fresh branch.
CACHE_WARMED_WORKFLOWS = ("test.yml", "test-integration.yml", "memleak.yml")


def read_workflow(name: str) -> str:
    return (WORKFLOWS / name).read_text(encoding="utf-8")


class TestCacheWarming(unittest.TestCase):
    def test_memleak_workflow_warms_from_main(self) -> None:
        memleak = read_workflow("memleak.yml")
        self.assertIn("push:", memleak)
        self.assertIn("branches: [main]", memleak)
        # Without a paths filter a docs-only merge re-warms for nothing; with
        # the wrong one the cache key's own inputs can change without a rewarm.
        for required in ("'src/**'", "'build.zig'", "'build.zig.zon'", "'build_runner.zig'"):
            self.assertIn(required, memleak, f"memleak paths filter is missing {required}")

    def test_every_zig_workflow_warms_from_main(self) -> None:
        for name in CACHE_WARMED_WORKFLOWS:
            body = read_workflow(name)
            self.assertIn("branches: [main]", body, f"{name} never warms its cache from main")


class TestPrewarmArtifacts(unittest.TestCase):
    def test_integration_workflow_prewarms_integration_binary(self) -> None:
        body = read_workflow("test-integration.yml")
        self.assertIn("zig build install test-integration-bin", body)
        # `test-bin` is the unit artifact since the daemon roots were split;
        # warming it here compiles a binary this job never executes.
        self.assertNotIn("zig build install test-bin", body)


def container_options(body: str) -> list[str]:
    """Every `options:` value in a workflow, comments excluded.

    Matching raw file text would count the prose explaining why a flag was
    dropped as a use of that flag.
    """
    values = []
    for line in body.splitlines():
        stripped = line.strip()
        if stripped.startswith("#") or not stripped.startswith("options:"):
            continue
        values.append(stripped[len("options:"):].strip())
    return values


class TestContainerPrivilege(unittest.TestCase):
    def test_coverage_job_is_not_privileged(self) -> None:
        options = container_options(read_workflow("test.yml"))
        self.assertTrue(options, "the coverage job must still declare container options")
        for value in options:
            self.assertNotIn("--privileged", value)
        # kcov needs personality(ADDR_NO_RANDOMIZE), which the default seccomp
        # profile's personality allow-list omits, plus ptrace on its child.
        joined = " ".join(options)
        self.assertIn("--security-opt seccomp=unconfined", joined)
        self.assertIn("--cap-add=SYS_PTRACE", joined)

    def test_no_workflow_is_privileged_without_a_kernel_reason(self) -> None:
        for path in sorted(WORKFLOWS.glob("*.yml")):
            for value in container_options(path.read_text(encoding="utf-8")):
                if "--privileged" not in value:
                    continue
                # The only defensible use is delegating a cgroup-v2 controller
                # subtree, which genuinely cannot be done unprivileged.
                self.assertIn(
                    "--cgroupns=private",
                    value,
                    f"{path.name} declares `options: {value}` — privileged "
                    "without the cgroup delegation that justifies it",
                )


class TestCacheSelection(unittest.TestCase):
    def select(self, caches: list[tuple], pr_states: dict[str, str], retain: int = 2) -> list[list[str]]:
        listing = "".join("\t".join(str(field) for field in row) + "\n" for row in caches)
        with tempfile.TemporaryDirectory() as raw:
            state_file = Path(raw) / "pr-state.tsv"
            state_file.write_text(
                "".join(f"{pr}\t{state}\n" for pr, state in pr_states.items()),
                encoding="utf-8",
            )
            result = subprocess.run(
                ["bash", str(SELECTOR), str(state_file), str(retain)],
                input=listing,
                capture_output=True,
                text=True,
                check=True,
            )
        return [line.split("\t") for line in result.stdout.splitlines() if line]

    def test_cache_prune_workflow_targets_closed_and_superseded(self) -> None:
        hash_a = "a" * 64
        hash_b = "b" * 64
        rows = self.select(
            [
                (1, "refs/pull/10/merge", f"zig-Linux-test-{hash_a}", "2026-07-20T00:00:00Z", 100),
                (2, "refs/pull/11/merge", f"zig-Linux-test-{hash_a}", "2026-07-20T00:00:00Z", 200),
                (3, "refs/heads/main", f"zig-Linux-test-{hash_a}", "2026-07-24T00:00:00Z", 300),
                (4, "refs/heads/main", f"zig-Linux-test-{hash_b}", "2026-07-23T00:00:00Z", 400),
                (5, "refs/heads/main", "zig-Linux-test-" + "c" * 64, "2026-07-22T00:00:00Z", 500),
            ],
            {"10": "CLOSED", "11": "OPEN"},
        )
        selected = {row[0]: row[4] for row in rows}
        self.assertEqual(selected.get("1"), "closed-pr", "a closed PR's cache is unreachable")
        self.assertNotIn("2", selected, "an open PR's cache is still restorable")
        self.assertNotIn("3", selected, "the newest generation must survive")
        self.assertNotIn("4", selected, "the second-newest generation is retained")
        self.assertEqual(selected.get("5"), "superseded", "the third generation is dead weight")

    def test_unknown_pull_request_state_is_left_alone(self) -> None:
        rows = self.select(
            [(1, "refs/pull/99/merge", "zig-" + "a" * 64, "2026-07-20T00:00:00Z", 100)],
            {"99": "UNKNOWN"},
        )
        self.assertEqual(rows, [], "an unresolvable state must never be guessed at")

    def test_families_are_grouped_independently(self) -> None:
        # Two different jobs, three generations each: each family keeps its own
        # newest two rather than competing for one shared budget.
        rows = self.select(
            [
                (1, "refs/heads/main", "zig-Linux-memleak-" + "a" * 64, "2026-07-24T00:00:00Z", 1),
                (2, "refs/heads/main", "zig-Linux-memleak-" + "b" * 64, "2026-07-23T00:00:00Z", 1),
                (3, "refs/heads/main", "zig-Linux-memleak-" + "c" * 64, "2026-07-22T00:00:00Z", 1),
                (4, "refs/heads/main", "zig-Linux-coverage-" + "a" * 64, "2026-07-24T00:00:00Z", 1),
                (5, "refs/heads/main", "zig-Linux-coverage-" + "b" * 64, "2026-07-23T00:00:00Z", 1),
                (6, "refs/heads/main", "zig-Linux-coverage-" + "c" * 64, "2026-07-22T00:00:00Z", 1),
            ],
            {},
        )
        self.assertEqual({row[0] for row in rows}, {"3", "6"})

    def test_an_entry_matching_both_rules_is_emitted_once(self) -> None:
        rows = self.select(
            [
                (1, "refs/pull/7/merge", "zig-" + "a" * 64, "2026-07-24T00:00:00Z", 1),
                (2, "refs/pull/7/merge", "zig-" + "b" * 64, "2026-07-23T00:00:00Z", 1),
                (3, "refs/pull/7/merge", "zig-" + "c" * 64, "2026-07-22T00:00:00Z", 1),
            ],
            {"7": "CLOSED"},
        )
        self.assertEqual(len(rows), 3)
        self.assertEqual(len({row[0] for row in rows}), 3, "no entry may be listed twice")

    def test_empty_listing_selects_nothing(self) -> None:
        self.assertEqual(self.select([], {}), [])

    def test_malformed_retain_count_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            state_file = Path(raw) / "pr-state.tsv"
            state_file.write_text("", encoding="utf-8")
            result = subprocess.run(
                ["bash", str(SELECTOR), str(state_file), "not-a-number"],
                input="",
                capture_output=True,
                text=True,
                check=False,
            )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("retain-per-family", result.stderr)


if __name__ == "__main__":
    unittest.main()
