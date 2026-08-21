# Handoff — M173_001 error-path coverage sweep

> Ephemeral. Delete at CHORE(close), per `AGENTS.md` §Required outputs.

## Scope / status

Spec: `docs/v2/active/M173_001_P1_API_ERROR_PATH_COVERAGE_SWEEP.md` (IN_PROGRESS).
Goal: every unhit line in the Zig tree is either executed by a test that asserts
something, or proven unreachable and deleted, with floors raised in the same
commit.

- ✅ Connection-failure class closed except 2 SSE arms — the 7 filed as "blocked
  on signature fixtures" needed no signer written (see Discovery).
- ✅ §1 Dimension 1.1: **44 of 69** `state/**` `errdefer` rungs proven, each
  mutation-checked.
- ✅ **R6 and R7 graded** — first 2 of 15 rubric rows.
- ✅ One real defect found and fixed (`account_teardown`), severity corrected —
  see "The arena finding" below, it is the most important thing in this doc.
- ⏳ Dimension 1.1 tail: `vault` (3), `user_preferences` (3),
  `model_library_store` (2), `tenant_provider` (2), 3 singles.
- 🛑 `repair_evidence` (8) **parked** — append-only triggers block the per-run
  reset its wrapper needs. Reason recorded in Discovery.
- ⏳ §1 Dimensions 1.2–1.5, §2–§5 untouched.

### Indy's standing decisions (do not re-open)

- **Full sweep on one long branch** — all four classes to zero before the PR opens.
- **Cover positive, edge, performance and concurrency cases** for every module touched.
- **The four single-request-unreachable connection failures are closed on paper.**
- **Sequencing, decided Aug 21:** land THIS branch first → then **M174**, a
  test-fixture dedup milestone → then fan M173's remainder out across parallel
  component milestones. Dedup and the sweep touch the same 53 test files, so
  they cannot run concurrently, and every sweep commit manufactures more of the
  duplication.

## ⚠️ The arena finding — read before writing another leak up as severe

`hx.alloc` is a **per-request arena** (`http/server.zig:278`, `defer arena.deinit()`).
Every HTTP handler path gets one. A missing `errdefer` rung on a handler-only
path therefore leaks NOTHING in production — it is a latent defect, correct the
moment a non-arena caller appears, but not operator-visible memory growth.

This dents the spec's own P1 justification ("a leak reaches operators as
unexplained memory growth"). 103 of the 295 `errdefer` rungs are in
`http/handlers/**` outright; an unknown share of `state/**`'s 69 are only ever
reached through handlers. Roughly 123 rungs sit behind long-lived callers (cron,
queue workers, runner daemon, boot) and those are the ones carrying the original
severity.

**Open question for Indy, asked and not yet answered:** run a caller/allocator
audit BEFORE writing the component milestones, so the fan-out targets the ~123
rungs that carry the justification instead of spreading agents evenly across 295
where a large share is cosmetic. Estimated one pass, grep-able.

The severity check is written into §1 so no agent repeats the mistake.

## Working tree

`feat/m173-error-path-coverage`. One uncommitted file: this handoff plus the
spec's leak-log severity correction (`docs/v2/active/M173_001_...md`). Commit
them together.

Seven commits this session, all pushed, all hooks-green:
`bec5d62c0` (merge main), `ab2e251f7` (7 ingress arms), `993865dda` (re-measure),
`54c0819dd` (21 rungs), `6d3c4839e` (14 rungs), `0853d4fee` (R6/R7 graded),
`67c5ad2b9` (leak fix + 9 rungs).

## Branch / PR (GitHub)

- Branch: `feat/m173-error-path-coverage`, level with its remote.
- PR: **none yet**. Per the sequencing decision this branch is meant to LAND —
  so the next milestone-level action is CHORE(close) for a park, then the PR.
- ⚠️ **Behind `origin/main` again** — it moved to `d0617c999` (5 commits) after
  the merge in `bec5d62c0`. Merging is a pre-PR gate. **Never force-push.**

## Running processes

**None.** Indy deleted all containers mid-session. `make test-integration`
brings its own stack up (postgres :25796, redis :25797, qstash :25798) and
recovers from an empty docker cleanly — verified this session. Only
`buildx_buildkit_ci-zig-builder0` is up.

No tmux.

## Tests / checks

- ✅ `make test-integration TEST_FILTER=pool_exhaustion` — **86/86**.
- ✅ `make test-integration TEST_FILTER=alloc_test` — **16/16**.
- ✅ `make test-integration TEST_FILTER=signup_teardown_alloc` — 16/16, and red
  under mutation both ways.
- ✅ `make harness-verify` green on every staged diff.
- ✅ Full lane green at `ab2e251f7`: merged 91.46% (floor 89), `agentsfleetd`
  91.11% (floor 90), `lib` 95.41%, `runner` 92.74%; integration 1006 passed,
  8 skipped, 0 failed.
- ✅ Depth gate `unit=4269 integration=737` (CHORE(open) baseline 4205/719).
- ⏳ **Not re-measured since `54c0819dd`.** The on-disk classifier report is
  stale — it still lists files already closed. Re-measure before grading R1–R4.
- ⏳ `make lint-all`, `make test-unit-all`, `make memleak`, `make check-version`
  — NEVER RUN on this branch. S1, S2, S5, S6, S7, S9 all ungraded. This is the
  real distance to CHORE(close), independent of lines remaining.

## Next steps

1. Commit + push this handoff and the spec correction.
2. **Get Indy's answer on the arena audit** — it may materially shrink §1 and
   changes what the component milestones should contain.
3. Merge `origin/main` (`d0617c999`), pre-PR gate.
4. Finish the Dimension 1.1 tail (~10 rungs) OR stop line-work and grade the
   S-rows so the distance to CHORE(close) is visible.
5. CHORE(close) for a park: Dimensions marked, spec stays `active/`, changelog
   `<Update>`, `~/Projects/docs` branch, PR `## Session notes`, delete this file.
6. Then M174 (dedup), then the fan-out.

## Risks / gotchas

### The four §1 traps — all now in the spec's Discovery, §1 points at them

Each produces a proof that passes while proving nothing, and none is visible
from a green run.

1. **An optional rung is only an allocation site when the column is non-null.**
   Cost a proof that passed with the rung DELETED. For an optional rung the
   fixture IS the proof: seed every guarded column non-null, and a list read
   needs >1 row.
2. **The counting run COMMITS.** `checkAllAllocationFailures` runs once on a
   working allocator to count sites; if the function writes, that run commits,
   and every failing run afterwards takes the replay branch. Reset at the top of
   each run — through the connection, never the failing allocator.
3. **A randomised generator aborts the proof** as `NondeterministicMemoryUsage`
   before failing any site. Drive the inner function that takes the generator
   and inject a fixed-length one. Check for the seam before concluding a
   function cannot be proven.
4. **Mutation-check by deleting the RUNG**, not by breaking the test. It is the
   only signal separating a real proof from a decorative one. One per module,
   minimum.

### Harness traps

- **A skipped integration test reports as PASSING.** Mutation-check every new one.
- **`TEST_FILTER` is per-graph.** `make test-integration TEST_FILTER=x` filters
  only files registered in `integration_tests.zig`. Anything registered through
  a product file's own `test { _ = @import(...); }` block is UNIT-graph and
  matches nothing there — `db/pool_test.zig`, `state/tenant_provider_test.zig`
  and siblings. For those:

  ```
  ZIG_GLOBAL_CACHE_DIR=~/.cache/agentsfleet/zig-global-cache \
  ZIG_LOCAL_CACHE_DIR=.tmp/zig-local-cache LIVE_DB=1 \
  TEST_DATABASE_URL="postgres://agentsfleet:agentsfleet@localhost:25796/agentsfleetdb?sslmode=disable" \
  zig build test -Dtest-filter=<token> --summary all
  ```

  `--summary all` is load-bearing: a pass and a zero-match run print identically.
- **`make test-integration TEST_FILTER=…` exits non-zero on a clean run** — the
  tally check does not recognise `All N tests passed.`. Read the line above.
- **Never run a lane while a Zig commit is in its hooks** — pre-commit runs a
  real `make test-integration` and a concurrent lane moves the coverage digest.
- **A filtered lane clobbers the merged report.** Re-run both producers before grading.
- **The harness cannot send a bodiless PUT/POST.** Those arms stay open.
- **Milestone markers are banned in code comments** (RULE TST-NAM) and **UFS
  rejects bare numeric literals** — both fired this session; fix the code, never
  the gate.
- **`db/pool_test.zig:943` "migration lock serializes" is a wall-clock flake**
  (`elapsed_ms < 1_000`). Failed once under load, passed on a quiet box. Worth a
  tolerance if CI hits it.
- **Class counts are NET, not progress** — leak fixes add rungs, and rungs carry
  log lines.

### Deferred, needing Indy's verbatim ack before CHORE(close)

Two product findings written into Discovery, NOT fixed, R6 forbids them here:
the **fleet-delete ordering defect** (delete can silently half-complete) and the
**four `make up` blockers** (one is a pure compose bug, no credential involved).
Agent-unilateral deferral is incomplete scope, not deferral — CHORE(close)
blocks until Indy's quote is in PR Session Notes.

### Known residue

`inline_test_lines` drops lines inside a `test {}` block but not helpers beside
one: **86 lines** of test support in the coverage denominator, ~100% covered,
≈0.03 points of rate inflation. Left deliberately (moving them costs 3 new files
for 0.03 points). Matters to §5 — a floor raised on an inflated rate cannot be
met once the inflation goes.

### Not ours

`deploy (dev)` / `cli-acceptance-dev` red on `main` since Aug 19 — another
agent's.
