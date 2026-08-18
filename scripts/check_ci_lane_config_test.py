"""Continuous Integration (CI) lane configuration gates.

Each lane defect these assert against cost real wall clock and was invisible
until someone read the run history:

  * a workflow with no `main` push trigger can only ever save caches under
    `refs/pull/<n>/merge`, which no other branch can restore — so every new
    branch rebuilds from cold;
  * a pre-warm step naming the wrong build step compiles a binary the job
    never runs, and leaves the one it does run cold;
  * `--privileged` grants a job that checks out branch code far more than the
    relaxations it actually needs, so every use of it is named here with the
    kernel operation that earns it and an unlisted one fails;
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


# Flags that widen what a job holds beyond the default container posture.
GRANT_FLAGS = ("--privileged", "--security-opt", "--cap-add")

# The only lanes allowed to hold `--privileged`, each against the kernel
# operation that earns it. A workflow absent from this table fails the sweep
# below: the gate's job is that privilege is argued for once, in the open,
# rather than acquired by a flag nobody re-reads afterwards. Both entries are
# operations the kernel refuses to an unprivileged container — neither is a
# convenience, and neither has a narrower capability that substitutes (measured:
# `--cap-add=SYS_ADMIN` alone does not).
PRIVILEGED_LANES = {
    # Delegating a cgroup-v2 controller subtree.
    "test-integration.yml": "cgroup-v2 controller delegation",
    # bubblewrap creating the namespaces the real-sandbox proofs run in. The CI
    # image has baked bwrap in since r3, so without this the four
    # `selftest_integration_test` cases do not FAIL here — they skip, on the
    # `probeRanHere` guard that reads a silent probe as a harness fact rather
    # than a verdict. Skipping is correct behaviour and it left
    # `selftest_exec.run` — the entire spawn/bound/reap half of the self-test —
    # measured at 33%, which then read as structurally unreachable rather than
    # as one missing flag. Measured on this image: unprivileged all four skip,
    # privileged all four pass.
    "test.yml": "bubblewrap namespace creation",
}

# Lanes whose privilege is cgroup delegation must also SCOPE it. bubblewrap
# needs no cgroup namespace of its own, so this is asserted per lane rather
# than demanded of every privileged grant.
CGROUP_DELEGATION_LANES = ("test-integration.yml",)


def logical_lines(body: str) -> list[str]:
    """Workflow lines with backslash continuations joined, comments excluded.

    `docker run` spreads its flags across continuations, so a per-physical-line
    scan would read `--privileged` and the `--cgroupns=private` that justifies
    it as unrelated grants. Comments go first: the prose explaining why a flag
    was dropped must not count as a use of that flag.
    """
    joined: list[str] = []
    buffer = ""
    for line in body.splitlines():
        stripped = line.strip()
        if stripped.startswith("#"):
            continue
        if stripped.endswith("\\"):
            buffer += stripped[:-1].strip() + " "
            continue
        joined.append((buffer + stripped).strip())
        buffer = ""
    if buffer:
        joined.append(buffer.strip())
    return joined


def privilege_grants(body: str) -> list[str]:
    """Every privilege grant in a workflow, in either spelling it can take.

    A grant reaches a job two ways: a job-level `container.options:` key, or a
    `docker run` the step issues itself. Lanes needing `--network host` are
    forced into the second — a job container is placed on GitHub's managed
    network, which rejects that flag. Reading only `options:` left every
    `docker run` grant unseen, which is most of them now.
    """
    values = []
    for line in logical_lines(body):
        if line.startswith("options:"):
            values.append(line[len("options:"):].strip())
        elif any(flag in line for flag in GRANT_FLAGS):
            values.append(line)
    return values


class TestContainerPrivilege(unittest.TestCase):
    def test_coverage_job_keeps_kcovs_two_relaxations_explicit(self) -> None:
        # kcov needs personality(ADDR_NO_RANDOMIZE), which the default seccomp
        # profile's personality allow-list omits, plus ptrace on its child.
        # `--privileged` now subsumes both, but they stay spelled out: the day
        # the privilege is narrowed back, kcov's own needs must not leave with
        # it. These two also carry the non-vacuity load: on a parser that
        # matched nothing, `grants` is empty, `joined` is empty, and both fail.
        grants = privilege_grants(read_workflow("test.yml"))
        joined = " ".join(grants)
        self.assertIn("--security-opt seccomp=unconfined", joined)
        self.assertIn("--cap-add=SYS_PTRACE", joined)

    def test_a_docker_run_grant_is_visible_to_the_sweep(self) -> None:
        # The sweep below is only worth running if it can see the spelling the
        # lanes actually use. Driven over synthetic text rather than a live
        # workflow, so it keeps proving this after the lanes change again.
        body = "\n".join(
            (
                "      - name: Gate coverage",
                "        run: |",
                "          # --privileged here is prose, not a grant",
                "          docker run --rm --network host \\",
                "            --privileged \\",
                "            image sh -c 'make test-coverage-zig'",
            )
        )
        grants = privilege_grants(body)
        self.assertTrue(grants, "a `docker run` grant must not be invisible")
        self.assertTrue(
            any("--privileged" in value for value in grants),
            "the continuation carrying --privileged was dropped",
        )
        self.assertFalse(
            any(value.startswith("#") for value in grants),
            "commented-out prose was counted as a grant",
        )

    def test_no_workflow_is_privileged_without_a_kernel_reason(self) -> None:
        for path in sorted(WORKFLOWS.glob("*.yml")):
            for value in privilege_grants(path.read_text(encoding="utf-8")):
                if "--privileged" not in value:
                    continue
                # A defensible use is one the kernel leaves no unprivileged
                # route to, and it is named in PRIVILEGED_LANES with which one.
                # A new lane reaching for the flag lands here first.
                self.assertIn(
                    path.name,
                    PRIVILEGED_LANES,
                    f"{path.name} declares `{value}` — privileged with no "
                    "recorded kernel reason. Name the lane and the operation "
                    "that earns it in PRIVILEGED_LANES, or drop the flag.",
                )
                if path.name in CGROUP_DELEGATION_LANES:
                    self.assertIn(
                        "--cgroupns=private",
                        value,
                        f"{path.name} is privileged for "
                        f"{PRIVILEGED_LANES[path.name]} but does not scope it",
                    )

    def test_a_privileged_lane_cannot_be_listed_without_being_used(self) -> None:
        # The table grants privilege, so a stale entry silently pre-approves a
        # lane that no longer takes the flag — and the next edit to that
        # workflow inherits the grant without argument.
        for name in PRIVILEGED_LANES:
            grants = privilege_grants(read_workflow(name))
            self.assertTrue(
                any("--privileged" in value for value in grants),
                f"{name} is listed in PRIVILEGED_LANES but no longer takes "
                "`--privileged` — drop the entry rather than leaving the grant",
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
