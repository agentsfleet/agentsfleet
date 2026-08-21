# Handoff — M173_001 error-path coverage sweep

> Ephemeral. Delete at CHORE(close), per `AGENTS.md` §Required outputs.

## Scope / status

Spec: `docs/v2/active/M173_001_P1_API_ERROR_PATH_COVERAGE_SWEEP.md` (IN_PROGRESS).
Goal: every unhit line in the Zig tree is either executed by a test that asserts
something, or proven unreachable and deleted, with floors raised in the same
commit.

- ✅ **The arena audit is done** — the question the last handoff left open is
  answered, and the answer reorders the remaining work. See below.
- ✅ Connection-failure class closed except 2 SSE arms.
- ✅ §1 Dimension 1.1: **47 of 69** `state/**` rungs proven, each mutation-checked.
- ✅ R6 and R7 graded — 2 of 15 rubric rows.
- ⏳ Dimension 1.1 tail: `tenant_provider` (2 of its 3 — the third is a txn
  abort needing a write path), `user_preferences` (2), `model_library_store` (3),
  3 singles.
- 🟢 `repair_evidence` (6 rungs) **stays parked, and the audit says that is free**
  — it is arena-backed, so none of those rungs can leak in production.
- ⏳ §1 Dimensions 1.2–1.5, §2–§5 untouched.

### Indy's standing decisions (do not re-open)

- **Full sweep on one long branch** — all four classes to zero before the PR.
- **Cover positive, edge, performance and concurrency cases** per module touched.
- **The four single-request-unreachable connection failures are closed on paper.**
- **Sequencing:** land THIS branch → **M174** (test-fixture dedup) → fan the
  remainder out across parallel component milestones. Dedup and the sweep touch
  the same 53 test files, so they cannot run concurrently.
- **Aug 21, four more, all quoted verbatim in the spec's Discovery:** run the
  caller/allocator audit before writing the component milestones · fix both
  product findings here, then **stop and rethink** the fleet-delete one · guard
  `repl.ts` rather than pin Bun · fix only `make up` blocker #4 and document the
  other three.

## The arena audit — what it changed

One reverse-reachability pass over the import graph, cut at `http/server.zig:278`
(the ONLY per-request arena in production code; every other `ArenaAllocator.init`
is function-local or a test). Over all 535 non-test `errdefer` rungs in 161 files:

| Class | Rungs | Files | What a missing rung costs |
|-------|-------|-------|---------------------------|
| Repeating (cron, queue, sweepers, daemon, boot loops) | 256 | 78 | compounds — **this set carries §1's P1 justification** |
| Boot-once | 45 | 17 | leaks once, dies with the process |
| Arena-backed | 219 | 60 | **cannot leak in production at all** |
| Unreached by any root | 15 | 6 | §4 triage input, not §1 work |

It self-checks on two rungs whose severity was already known and was NOT fed to
the classifier: `auth/jwks.zig` (a proven compounding leak) lands in Repeating;
`state/account_teardown.zig` (arena-masked) lands in Arena-backed.

**Granularity caveat.** Reachability is per FILE. A Repeating file may hold
functions only handlers call, so **256 is an upper bound on severe**; an
Arena-backed file is reached by no long-lived root at all, so **219 is a firm
lower bound on cosmetic**. Sharpening the middle needs a per-function call graph
and is not worth it before the fan-out.

**The rung counts are over ALL rungs, not unhit ones.** Intersecting with the
classifier's unhit set needs a fresh merged coverage report; the on-disk one is
stale, so that intersection lands with the R1–R4 re-measure.

**How to order the rest of Dimension 1.1 with it:** `tenant_provider` is
Repeating (do it); `user_preferences` and `model_library_store` are Arena-backed
(cosmetic — do them last or not at all).

## Working tree

`feat/m173-error-path-coverage`. Clean apart from this file.

Five commits this session, all hooks-green:
`2352d9836` (arena audit), `a99a90d1f` (fleet-delete withdrawn),
`b26354cb7` (Bun 1.4 readline guard), `e803087ec` (`make up` fails loudly),
`26600662d` (vault metadata proof).

Merged `origin/main` (`d0617c999`) — the pre-PR gate the last handoff flagged.

## Branch / PR (GitHub)

- Branch: `feat/m173-error-path-coverage`.
- PR: **none yet.** Per the sequencing decision this branch is meant to LAND, so
  the next milestone-level action is CHORE(close) for a park, then the PR.
- **Never force-push.**

## Running processes

`agentsfleet-m173-{postgres,redis,qstash}-1` are UP on **25796 / 25797 / 25798**,
migrated (47 versions), left running deliberately so the next agent can run
unit-graph proofs without a cold start. `make down` when finished.

`agentsfleetd-api` is NOT running — it cannot boot without the four variables
below. That is expected, not a fault.

No tmux. Note `buildx_buildkit_ci-zig-builder0`, and that a SECOND Claude session
was running a full Docker stack in `~/Projects/e2e-observability-platform`, which
is why every lane tonight was slow.

## Tests / checks

- ✅ Vault proof: `69 pass` green; mutation-checked BOTH ways — deleting the
  outer rung fails at `fail_index 2/4` with 2 leaks, deleting the optional
  `provider` rung fails at `1/4` with 1 leak of 17 bytes (the length of
  `"openai-compatible"`, i.e. the fixture proving it made that rung a site).
- ✅ CLI unit lane `1624 pass, 0 fail` on Bun **1.4.0 AND 1.3.14**.
- ✅ `make harness-verify` green on every staged diff; full `make test-integration`
  ran green inside the vault commit's pre-commit hook.
- ✅ `make up` now exits 1 and quotes the daemon's own `UZ-STARTUP-002` line.
- ⏳ **Classifier report still stale.** Re-measure before grading R1–R4.
- ⏳ `make lint-all`, `make test-unit-all`, `make memleak`, `make check-version`
  — STILL NEVER RUN on this branch. S1, S2, S5, S6, S7, S9 all ungraded. This
  remains the real distance to CHORE(close), independent of rungs remaining.

## Next steps

1. Grade the S-rows — the ungraded gates are the blocker, not the rungs.
2. Re-measure the classifier, then grade R1–R4.
3. Finish the Repeating half of the 1.1 tail (`tenant_provider`, 2 rungs at
   `state/tenant_provider.zig:257` and `:288`, both plain dupes, fixtures already
   `pub` in `tenant_provider_test.zig`).
4. CHORE(close) for a park, then M174, then the fan-out.

## Risks / gotchas

### Bun version drift — read before running any CLI lane

Local `bun` resolves through the GLOBAL mise config to `latest` (1.4.0); CI pins
**1.3.14**. The repo pins nothing. `repl.ts` is now guarded so both pass, but any
OTHER 1.4 behaviour change will surface locally and not in CI, or vice versa.
Prefix with `mise exec bun@1.3.14 --` to reproduce CI exactly. Indy declined a
repo pin this session; it is still the standing gap.

### The four §1 traps — all in the spec's Discovery, §1 points at them

1. **An optional rung is only an allocation site when the column is non-null.**
   For an optional rung the fixture IS the proof: seed every guarded column
   non-null, and a list read needs >1 row. The vault proof this session is the
   worked example, including the byte count that demonstrates it.
2. **The counting run COMMITS.** `checkAllAllocationFailures` runs once on a
   working allocator to count sites; if the function writes, that run commits and
   every failing run afterwards takes the replay branch. Reset at the top of each
   run — through the connection, never the failing allocator. (A pure read, like
   vault's, needs no reset.)
3. **A randomised generator aborts the proof** as `NondeterministicMemoryUsage`
   before failing any site. Drive the inner function and inject a fixed-length one.
4. **Mutation-check by deleting the RUNG**, not by breaking the test. One per
   module, minimum.

### Harness traps

- **A skipped integration test reports as PASSING.** Mutation-check every new one.
- **`TEST_FILTER` is per-graph.** `make test-integration TEST_FILTER=x` filters
  only files registered in `integration_tests.zig`. Anything registered through
  `src/agentsfleetd/tests.zig` or a product file's own `test {}` block is
  UNIT-graph and matches nothing there. For those:

  ```
  ZIG_GLOBAL_CACHE_DIR=~/.cache/agentsfleet/zig-global-cache \
  ZIG_LOCAL_CACHE_DIR=.tmp/zig-local-cache LIVE_DB=1 \
  TEST_DATABASE_URL="postgres://agentsfleet:agentsfleet@localhost:25796/agentsfleetdb?sslmode=disable" \
  zig build test -Dtest-filter=<token> --summary all
  ```

  `--summary all` is load-bearing: a pass and a zero-match run print identically.
  Read the `Build Summary: N/N steps succeeded; M/M tests passed` line, and make
  the filter specific enough that M is not the whole suite.
- **To bring the datastores up without running a lane:** `make _ensure-test-infra`,
  then `zig build run -- migrate` with `DATABASE_URL_MIGRATOR` set. There is no
  `migrate` build step — it is `run -- migrate`.
- **`make test-integration TEST_FILTER=…` exits non-zero on a clean run** — the
  tally check does not recognise `All N tests passed.`. Read the line above.
- **Never run a lane while a Zig commit is in its hooks** — pre-commit runs a real
  filtered `make test-integration` under kcov and a concurrent lane moves the digest.
- **A filtered lane clobbers the merged report.** Re-run both producers before grading.
- **The harness cannot send a bodiless PUT/POST.** Those arms stay open.
- **Milestone markers are banned in code comments** (RULE TST-NAM) and **UFS
  rejects bare numeric literals** — fix the code, never the gate.
- **`db/pool_test.zig:943` "migration lock serializes" is a wall-clock flake.**
- **Class counts are NET, not progress.**

### `make up` needs four variables

`OIDC_ISSUER`, `OIDC_AUDIENCE`, `AUTH_SESSION_CODE_PEPPER`, `AUDIT_LOG_PEPPER`
(both peppers 64 hex) in `.env.agentsfleetd.local`, via `provision-env-1password`.
`make up` now says so instead of printing a URL for a dead API. No dev default was
invented for any of them and no scanner suppression was added — Indy's call.

### The fleet-delete follow-up — start from the code, not the write-up

Withdrawn this session because its premise was false. `ingress/qstash.zig` answers
`hx.ok(200, accepted:true)` for EVERY outcome and logs `.schedule_missing` at
**debug**, so an orphan schedule fires forever, unobserved and billed. Three things
the follow-up must settle together, all in Discovery: the swap cannot be naive
(`removeAll` enumerates from the rows the purge cascades away); nothing is
observable while `schedule_missing` stays at debug; and `DELETE` on a fleet nobody
killed first cancels every schedule and THEN answers 409. `create.zig` carries the
same shape.

### Known residue

`inline_test_lines` drops lines inside a `test {}` block but not helpers beside
one: **86 lines** of test support in the coverage denominator, ~100% covered,
≈0.03 points of rate inflation. Left deliberately. Matters to §5 — a floor raised
on an inflated rate cannot be met once the inflation goes.

### Not ours

`deploy (dev)` / `cli-acceptance-dev` red on `main` since Aug 19.
