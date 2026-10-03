<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M210_003: The Rust unit lane runs as three parallel shards beside lint, and the required check `test-unit-rustd` is green only when every one of them is

**Prototype:** v2.0.0
**Milestone:** M210
**Workstream:** 003
**Date:** Oct 03, 2026
**Status:** IN_PROGRESS
**Priority:** P2 — developer tooling: the required Rust unit check runs lint and tests in one serial job
**Categories:** INFRA
**Batch:** B2 — folded into M210_002 and shipped in its Pull Request; independent of its Sections, so either lands first
**Branch:** `feat/m210-agent-loop-hosted-tools`
**Folded-into:** `M210_002`
**Baseline revision:** `4339afb59fe83a20fb643004e432b9755e1b14a7`
**Test Baseline:** pending — measured before the Pull Request, with M210_002's
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** none
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 03, 2026) from Indy's in-session request, a source trace on `main` at `4339afb59`, and the last five green `test` runs
**Canonical architecture:** `docs/architecture/testing.md` §Public lanes, §Coverage

---

## Overview

**Goal (testable):** `test_ci_unit_lane_runs_in_parallel` — on this branch's Pull Request, `test-unit-rustd.yml` runs `test-unit-rustd-runner`, `test-unit-rustd-daemon`, `test-unit-rustd-daemon-libs` and `lint-rustd` at once, and the required check `test-unit-rustd` reports green after all of them and sooner than the serial job's fastest recent run (6m32s).
**Problem:** Every Pull Request waits on one `test-unit-rustd` job that runs `make lint-rustd` and then `make test-unit-rustd` back to back (`.github/workflows/test.yml:87-90`). Over the last five green runs it took 6m32s to 8m46s; in the two split by step, lint took 139 s and 171 s and the tests 258 s and 306 s. The integration lane already measures in parallel shards (`.github/workflows/test-integration-rustd.yml`); the unit lane does not.
**Solution summary:** `make test-unit-rustd` becomes three targets, one per crate family: `test-unit-rustd-runner` (`afr_*`, `agentsfleet_runner`), `test-unit-rustd-daemon` (`agentsfleetd`) and `test-unit-rustd-daemon-libs` (`afd_*`), with `test-unit-rustd` the three as prerequisites, so local runs still cover the whole workspace. The lane leaves `test.yml` for its own workflow, `test-unit-rustd.yml`, in the integration lane's shape: one job per shard, named after its target, runs it, `make lint-rustd` runs as its own job beside them, and a verdict job keeps the required name `test-unit-rustd`.

## PR Intent & comprehension handshake

- **PR title (eventual):** rides M210_002's title; its own commits read `ci(rustd): run the unit lane in three parallel shards beside lint`
- **Intent (one sentence):** A contributor's Pull Request learns whether the Rust workspace lints and passes in the time of its slowest shard rather than lint plus every test in sequence, with the same tests run and the same required check deciding.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `make/test-integration-rustd.mk` — the Shards block the coverage lane splits on, and `_rust_lane` with its zero-tests guard, which every unit shard runs through.
2. `.github/workflows/test-integration-rustd.yml` — shard jobs → one verdict job under the lane's required name, `if: !cancelled()`.
3. `.github/workflows/test.yml` — the serial `test-unit-rustd` job this moves out, its `rust-cache` reasoning, and the `test` aggregate's `needs.*.result` loop the verdict reuses.
4. `make/test-unit.mk` and `make/test.mk` — today's `test-unit-rustd` recipe, and the include order that puts `test-unit.mk` before `test-integration-rustd.mk`.
5. `docs/architecture/testing.md` §Public lanes and §Coverage — the lane prose that changes with it.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `make/test-unit.mk` | EDIT | Three shard targets, each through `_rust_lane`; `test-unit-rustd` is the three |
| `.github/workflows/test-unit-rustd.yml` | CREATE | Three shard jobs and `lint-rustd` in parallel → the verdict `test-unit-rustd`; the `rust-cache` reasoning moves with it, its stale `push: branches: [main]` claim corrected |
| `.github/workflows/test.yml` | EDIT | The serial `test-unit-rustd` job leaves; nothing else in the file changes |
| `scripts/rustd_unit_shards_test.py` | CREATE | The partition, aggregate, job-to-target, empty-shard and verdict proofs, run by `lint-scripts` |
| `make/quality.mk` | EDIT | The `lint-runner-fmt` comment that says `test.yml` runs `make lint-rustd` inside the `test-unit-rustd` job |
| `docs/architecture/testing.md` | EDIT | §Public lanes names the shards and the Continuous Integration (CI) job graph |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (each family is one prefix in make; the workflow's three jobs name the three targets, and the self-test fails when they drift), NLR (comments naming the serial job are corrected where touched), ORP (every reference to the serial job's shape is swept), MSID (no milestone identifier in make, workflow or test comments), TST-NAM (milestone-free test names), FLL, NDC.
- `dispatch/write_python.md` — the self-test: standard-library parsing, context-managed temporary directories, specific exceptions.
- `dispatch/write_shell.md` — the verdict step's inline shell and the make recipe lines: quoted expansions, no untrusted `eval`.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| CI/CD edit guard | yes — `.github/workflows/test.yml`, `.github/workflows/test-unit-rustd.yml` | Authorised in-session by Indy (Discovery, Oct 03, 2026); `make check-gh-actions-valid` (in `make lint-all`) runs actionlint over both |
| UFS / MILESTONE-ID | yes | Families are one prefix each in make; no milestone identifiers in comments |
| File & Function Length (≤350/≤50/≤70) | yes — `test.yml` is 323 lines, `make/test-integration-rustd.mk` 393 | The lane leaves `test.yml` rather than growing it past 350; the coverage makefile is not edited; the new workflow and the self-test stay under the cap |

## Prior-Art / Reference Implementations

- **Reference:** `make/test-integration-rustd.mk` + `.github/workflows/test-integration-rustd.yml` (commit `01968204f`, "ci(rustd): measure coverage in three parallel shards, grade the floors once") — the job shape copied: parallel shard jobs, one verdict under the required name. Two divergences: shards select by crate family rather than the coverage lane's exclusion, so a new `afr_*` crate joins `runner` rather than the libraries; and `afd_bench`'s tests run, where the coverage lane drops its lines.
- **Reference:** `scripts/rustd_coverage_test.py` — how a self-test in `lint-scripts` drives a lane's helper without the lane's datastores.

## Sections (implementation slices)

### §1 — `test-unit-rustd` is three shard targets, one per crate family

The unit shards select what `cargo test --workspace --all-features` selects today, by crate-name prefix: `runner` is `afr_*` and `agentsfleet_runner`, `daemon` is `agentsfleetd`, and `daemon-libs` is `afd_*`, `afd_bench` included, since the unit run executes its tests today. The directory name is the crate name (`rustd/Cargo.toml`, M-CRATES-FLAT-FOLDER), so a crate added under a family's prefix joins its shard with no edit, and a crate in no family fails Dimension 1.1. Each shard is a target, `test-unit-rustd-<shard>`, and `test-unit-rustd` is the three as prerequisites in partition order: the whole workspace, through the invocations CI makes, and no selector to validate, since an unknown shard is make's own "No rule to make target". Each shard runs through `_rust_lane`, so a selection that runs no tests fails. **Implementation default:** each family is a `$(wildcard)` over `crates/<prefix>*`, a recursive variable read at recipe time; the coverage makefile stays as it is.

- **Dimension 1.1** — Every workspace member lands in exactly one unit shard, and `afd_bench` lands in `daemon-libs` → Test `test_unit_shards_partition_the_workspace` — DONE (`scripts/rustd_unit_shards_test.py`)
- **Dimension 1.2** — `test-unit-rustd` plans the `runner`, `daemon` and `daemon-libs` invocations in that order → Test `test_unit_shards_default_to_every_shard` — DONE (`scripts/rustd_unit_shards_test.py`)
- **Dimension 1.3** — The workflow's shard jobs are exactly the targets `test-unit-rustd` runs, each resolves, and an unknown one is refused before any cargo invocation → Test `test_every_shard_job_has_a_unit_target` — DONE (`scripts/rustd_unit_shards_test.py`)
- **Dimension 1.4** — A shard whose selection runs no tests fails with "ran no tests" → Test `test_empty_unit_shard_fails` — DONE (`scripts/rustd_unit_shards_test.py`)

### §2 — The shards run what the workspace runs

At one revision, the three shards together run the tests the unsharded workspace run does. Cargo unifies dependency features over the selected packages only, so a shard can compile a dependency with fewer features than the workspace run; the count comparison catches a selection drift, and a shard that fails to compile or pass is red on its own.

- **Dimension 2.1** — The three shards' passed and ignored counts sum to the unsharded `cargo test --workspace --all-features` run's → Test `test_unit_shards_run_what_the_workspace_runs` — DONE (counts in Discovery)

### §3 — Continuous Integration runs the shards and lint at once, under the required name

`test-unit-rustd.yml` runs on the same triggers as `test.yml`, with its own concurrency group. It holds one job per shard, named after the target it runs, and a `lint-rustd` job; none depends on another, so every one reports when one fails. Named jobs rather than a matrix, because `make check-gh-actions-valid` resolves each `run: make <target>` statically, and a matrix-built `test-unit-rustd-${{ matrix.shard }}` failed it at commit as an unknown `test-unit-rustd-`. Each job keys its own compiler cache (`rustd-unit-<shard>`, `rustd-lint`), saved on `main` only as today: one shared key over four different builds would race on `main`'s save and hand each job another's artifacts, and the serial job's restore found nothing to lose (`gh run view 37106595881 --log`: "No cache found."). The verdict job is named `test-unit-rustd`, needs the shards and lint, runs `if: !cancelled()`, and fails unless every needed result is `success`. `gh api repos/agentsfleet/agentsfleet/branches/main/protection/required_status_checks` lists `test-unit-rustd`, so that name stays the lane's verdict.

- **Dimension 3.1** — The verdict fails on any needed result other than `success`, and passes on all `success` → Test `test_verdict_fails_unless_every_needed_job_succeeded` — DONE (`scripts/rustd_unit_shards_test.py`)
- **Dimension 3.2** — The Pull Request's run shows the three shard jobs and `lint-rustd` overlapping, then the verdict; the required checks list is unchanged → Test `test_ci_unit_lane_runs_in_parallel`
- **Dimension 3.3** — The lane's wall-clock on the Pull Request's run is below 6m32s → Test `measure_unit_lane_wall_clock`

## Interfaces

```
make test-unit-rustd             every shard, in sequence: runner, daemon, daemon-libs
make test-unit-rustd-runner      afr_*, agentsfleet_runner
make test-unit-rustd-daemon      agentsfleetd
make test-unit-rustd-daemon-libs afd_*, afd_bench included
make test-unit-rustd-<other>     make's "No rule to make target"

test-unit-rustd.yml:  test-unit-rustd-runner        ─┐
                      test-unit-rustd-daemon        ─┼─ test-unit-rustd   (verdict, required check)
                      test-unit-rustd-daemon-libs   ─┤
                      lint-rustd                    ─┘
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| The jobs and the targets drift | A shard added to one and not the other | The self-test fails, and `make check-gh-actions-valid` refuses a job whose target does not exist (Dimension 1.3) |
| A crate in no family | A crate named outside `afr_`, `afd_` and the two binaries | It runs in no shard, and the partition self-test fails (Dimension 1.1) |
| A shard selects no tests | A prefix that matches nothing | `_rust_lane` fails it: "ran no tests" (Dimension 1.4) |
| A shard's dependency features differ from the workspace run's | Cargo unifies features per selection | The shard fails on its own; a selection drift fails the count check (Dimension 2.1) |
| A shard or lint fails, is skipped or is cancelled | A test, a Clippy finding, a runner fault | Every shard still reports; the verdict `test-unit-rustd` is red (Dimension 3.1) |

## Invariants

1. Every test the unsharded workspace run selects runs in exactly one shard — the families are disjoint prefixes, and Dimension 1.1 fails on a member in zero or two shards.
2. The required check `test-unit-rustd` is green only when every shard and lint succeeded — the verdict step's result loop, with `if: !cancelled()` so a failed dependency cannot skip it into a pass.
3. The workflow runs exactly the shards `test-unit-rustd` does — the self-test compares the two lists and fails on any difference.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes; the lane's wall-clock is measured once, from the Pull Request's run | not applicable | not applicable | not applicable | not applicable | `measure_unit_lane_wall_clock` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_unit_shards_partition_the_workspace` | `cargo metadata` members vs each shard's dry-run package set → each member in one shard; `afd_bench` in `daemon-libs` |
| 1.2 | unit | `test_unit_shards_default_to_every_shard` | `make -n test-unit-rustd` → three `cargo test` invocations: runner, daemon, daemon-libs |
| 1.3 | unit | `test_every_shard_job_has_a_unit_target` | shard jobs = `test-unit-rustd-{runner,daemon,daemon-libs}`; each → `make -n test-unit-rustd-<name>` exits 0; `test-unit-rustd-bogus` → non-zero, "No rule to make target", no `cargo` line |
| 1.4 | unit | `test_empty_unit_shard_fails` | `RUSTD_DIR` at a temporary workspace with one test-free crate as the runner package → exit 1, "ran no tests" |
| 2.1 | integration | `test_unit_shards_run_what_the_workspace_runs` | same revision: Σ shards' passed and ignored = the unsharded run's passed and ignored; both totals recorded in Discovery |
| 3.1 | unit | `test_verdict_fails_unless_every_needed_job_succeeded` | the verdict step's script from `test-unit-rustd.yml` with results `success`×2 → exit 0; one `failure`, `skipped` or `cancelled` → exit 1 |
| 3.2 | e2e | `test_ci_unit_lane_runs_in_parallel` | `gh run view` on the Pull Request's `test-unit-rustd` run → the three shard jobs and `lint-rustd` overlap in time; `test-unit-rustd` reports; required checks list unchanged |
| 3.3 | manual | `measure_unit_lane_wall_clock` | first unit-lane job start → verdict end < 392 s on the Pull Request's run; the implementing agent records the number and the run URL in Discovery |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | `test-unit-rustd` is three shard targets that stay total (§1) | `python3 -m unittest discover -s scripts -t scripts -p 'rustd_unit_shards_test.py'` | `OK` | P0 | |
| R2 | The shards run what the workspace runs (§2) | named manual check: `make test-unit-rustd` shard tallies vs `cargo test --workspace --all-features` in `rustd/` | equal passed and ignored totals, both in Discovery | P0 | |
| R3 | CI runs three shards and lint under the required name (§3) | `gh pr checks --json name --jq '[.[].name \| select(startswith("test-unit-rustd") or . == "lint-rustd")] \| length'` | `5` | P0 | |
| R4 | The lane is faster than the serial job (§3) | named manual check: Dimension 3.3's number in Discovery | below 392 s | P1 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from this table or M210_002's | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. See `dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes. **Ship gate:** every required check passes before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may be MOVED only under the transfer rule in `docs/TEMPLATE.md`; a MOVED row is never ✅.

## Dead Code Sweep

N/A — no files deleted. The serial job is replaced in place, and its comment references are swept under ORP.

## Out of Scope

- Sharding Clippy or `cargo fmt` — lint runs as one job; splitting it is a later call if lint becomes the critical path.
- A per-crate matrix, `cargo nextest --partition`, or `sccache` — see Decomposition; `test.yml` already records why `sccache` was declined.
- Changing what the coverage lane runs or grades — `make/test-integration-rustd.mk` is not edited.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A contributor pushes a Rust change and watches three `test-unit-rustd-…` checks and `lint-rustd` run side by side; the required `test-unit-rustd` turns green before the serial job's fastest recent run would have.
2. **Preserved user behaviour** — `make test-unit-rustd` and `make test-unit-all` still run the whole workspace locally; the required check names are unchanged; pre-push still runs `lint-rustd` alone.
3. **Optimal-way check** — Three shards by crate family. The gap: the `daemon` shard compiles nearly the whole workspace, so compilation is not divided; the win is lint off the critical path plus split test execution, measured by Dimension 3.3.
4. **Rebuild-vs-iterate** — Iterate: the lane shape exists one workflow over.
5. **What we build** — three shard targets behind `test-unit-rustd`, the lane's own workflow, one self-test, two prose updates.
6. **What we do NOT build** — Clippy shards, per-crate jobs, a selector variable (see Out of Scope).
7. **Fit with existing features** — Runs beside the coverage lane's shards; must not destabilise the coverage lane, whose makefile it leaves untouched.
8. **Surface order** — N/A — no user surface; contributor-facing CI only.
9. **Dashboard restraint** — N/A — no user surface; a red shard is a red check with its name.
10. **Confused-user next step** — The verdict prints each needed job's result and names the one that failed; the shard's own log has the test.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three Sections — make, proof of equivalence, CI — because the make change is testable alone and the CI graph only reads it. The lane gets its own workflow, as the integration lane has one: `test.yml` is 323 lines and the job graph would take it past the 350-line cap rubric S7 checks.
- **Alternatives considered:** one job per crate (rejected: forty-four jobs, each compiling most of the graph); the coverage lane's `substrate`-by-exclusion partition (rejected: a new `afr_*` crate would land in the libraries shard, and Indy asked for a name that says what it holds); `cargo nextest --partition count:N` (rejected: each slice compiles the whole workspace and adds a tool); folding into M210_002 as §8 (rejected by the gate: M210_002 reaches 350 lines against the 320 cap, `audits/spec-template.sh`).
- **Patch-vs-refactor verdict:** this is a **patch** because it reshapes one job and one recipe on a partition that already exists.

## Discovery (consult log)

- **Consults** — Indy (in-session, Oct 03, 2026), at M210_002's CHORE(open): "also in the next spec i want the lint, test-unit-rustd (broken down and run in parallel like we do for test-integration-rustd) too so this is run in parallel and faster". That request authorises the `.github/workflows/test.yml` edit. His brief asked to fold it into M210_002 as a Section and broke off at "test-unit-rustd shards on the same partition (RUSTD_SHARD; unset = every shard, so"; this spec reads the rest as the coverage lane's rule, unset runs everything locally. The fold as §8 took M210_002 to 350 lines against the cap; asked where §8 should live (folded M210_003, trim M210_002, or override the cap), Indy did not answer within the prompt's window, and the agent took the first. Reversible: fold back by trimming M210_002, or an override he records.
- **Three targets** — Indy (in-session, Oct 03, 2026), on the first draft's `RUSTD_SHARD` recipe: "just have 3 targets why do you need if statements?". The three targets replace it, and the unknown-shard refusal goes with the selector. Then, of the third: "what is test-unit-substrate? name this appropriately"; it is the `afd_*` library crates, so it is `daemon-libs`, selected by prefix. With the names no longer the coverage lane's, the plan job that read `rustd-coverage-shards` went too. The workflow names the three jobs after their targets, and the self-test holds it to them.
- **Evidence** — Required checks on `main`: `gitleaks`, `lint`, `test`, `test-unit-app`, `test-unit-cli`, `test-unit-design-system`, `test-unit-rustd`. Serial durations from `gh run list --workflow test.yml --status success --limit 5`: 8m20s, 6m54s, 6m32s, 8m46s, 8m44s; step split from `gh run view 37104801714` and `37106595881`. `afd_bench` carries 158 `#[test]`/`#[tokio::test]` markers, and the coverage lane's `_RUSTD_SUBSTRATE` excludes it (`make/test-integration-rustd.mk:310`). The families today: 5 runner crates, 1 daemon, 38 `afd_*` (`make -n test-unit-rustd`). `wc -l`: `test.yml` 323, `make/test-integration-rustd.mk` 393, `make/test-unit.mk` 81. `lint-scripts` runs in no workflow (`grep -rn lint-scripts .github/workflows` is empty), so the self-test runs where `make lint-all` does, locally and at `orly gate pr`.
- **Agent defaults** — lint as one parallel job, not sharded (Indy's parenthetical names `test-unit-rustd`); the shard targets' caller is their workflow job, their distinct caller under the no-new-target rule; the cache is keyed per job (§3); the unpushed `fix/m210-runner-coverage-library` branch edits `.github/workflows/lint.yml`, which this spec does not touch.
- **Shards run what the workspace runs (Dimension 2.1)** — Oct 03, 2026, on `f9266a50b`'s tree, macOS arm64, warm target directory: `make test-unit-rustd` ran runner 264, daemon 130 and daemon libraries 2944 passed, 3338 in all, with 804 ignored and 0 failed; `cargo test --workspace --all-features` in `rustd/` ran 3338 passed, 804 ignored, 0 failed. Local cost: the three shards in sequence took 7m22s against 6m02s unsharded, since each selection unifies its own features and compiles its own graph; Continuous Integration runs them on three runners, where that cost buys the parallelism.
- **Metrics review** — No analytics or funnel playbook update required: no product or operator signal changes.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
