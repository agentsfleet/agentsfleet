# Handoff — M173_001 error-path coverage sweep

> Ephemeral. Delete at CHORE(close), per `AGENTS.md` §Required outputs.

## Scope / status

Spec: `docs/v2/active/M173_001_P1_API_ERROR_PATH_COVERAGE_SWEEP.md` (IN_PROGRESS).
Goal: every unhit line in the Zig tree is either executed by a test that asserts
something, or proven unreachable and deleted, with floors raised in the same
commit.

**Indy's standing decisions (do not re-open):**
- **Full sweep on one long branch** — all four classes to zero before the PR opens.
- **Cover positive, edge, performance and concurrency cases** for every module touched.
- **The four single-request-unreachable connection failures are closed on paper**,
  not with a fault seam and not with a racing thread. Decided Aug 21 after the
  class was measured at four, not the fourteen a bad heuristic first reported.

**Class counts** — measured Aug 21 from the 09:53 lane (`make test-coverage-zig`
+ `make test-integration` + `make test-coverage-grade`, exit 0: merged 91.39% /
floor 89, `agentsfleetd` 91.04% / floor 90, `lib` 95.30%, `runner` 92.68%;
integration 1003 passed, 8 skipped, 0 failed):

| class | Aug 21 measured | Aug 21 re-measured | mechanism |
|---|---|---|---|
| errdefer | 295 | **295** | `checkAllAllocationFailures` |
| failure-response | 463 | **454** | inject the failure the path answers |
| failure-log | 303 | **300** | same tests, assert the log line |
| error-return | 128 | **128** | construct the triggering input |
| other | 1094 | **1088** | triage: test / delete / annotate |
| brace | 16 | **16** | report artefact, no test owed |
| **total** | **2299** | **2281** | |

**Re-measured Aug 21 after `ab2e251f7`** against a green lane (merged 91.46%,
`agentsfleetd` 91.11%, `lib` 95.41%, `runner` 92.74%; integration 1006 passed,
8 skipped, 0 failed). Re-measure again before grading anything — and read the
deltas as NET, since leak fixes add `errdefer` rungs and those rungs carry log
lines.

### Connection-failure (`pool.acquire`) class — the part worked this session

39 connection-failure paths had never run. Current state:

| state | count | detail |
|---|---|---|
| ✅ closed earlier | 50 endpoints | the original probe table |
| ✅ closed `133d6731d` | 5 | runner plane: `self`, `heartbeat`, `memory` ×2, `credentials_mint` |
| ✅ closed `fbf466d99` | 1 | `fleets/patch` |
| ✅ closed `e4a336aad` | 2 | `ingress/github` |
| ✅ closed this session | 7 | Slack `connectors/slack/events` ×1 — a probe row, it acquires before it verifies. Svix `identity_events_clerk` ×2 + `identity_events_delete` ×4 — the existing signer wired to a drained pool, in `handlers/auth/identity_events_pool_exhaustion_integration_test.zig`. Mutation-checked: all three go red without the drain/headers. |
| ⏳ SSE | 2 | `fleets/events_stream:211`, `workspaces/events_stream:105` |
| 📝 recorded, not tested | 4 | see spec Discovery; R2 amended `0` → `4` |

## Working tree

`feat/m173-error-path-coverage`, clean but for this handoff. Four commits this
session, all hooks-green. `e4a336aad` (github ingress) was unpushed at the time
of writing — push it with this handoff.

## Branch / PR (GitHub)

- Branch: `feat/m173-error-path-coverage`. Four commits this session:
  `133d6731d` (runner plane), `fbf466d99` (fleet patch), `f2e1f7f79` (spec
  record), `e4a336aad` (github ingress).
- PR: **none yet** — correct, nowhere near CHORE(close).
- **The branch is BEHIND `origin/main`, which moved twice today** — it was
  `faf563fa3` at session start and is now `224fa15e6` (PR #622
  `fix/runner-nullclaw-process-io` merged). Merging main is a pre-PR gate.
  Never force-push.

## Running processes

No tmux. Docker for this worktree: postgres :25796, redis :25797, qstash :25798.

**Unrelated stacks also up on this machine** — do not disturb:
- `agentsfleet-*` on :5432/:6379/:3000 — a live product stack brought up on the
  MAIN worktree this session (see "Live run" below).
- `agentsfleet-fix-runner-io-*` on :22622-4 — **orphaned**. Its worktree no
  longer exists on disk; #622 merged and the tree was pruned without `make down`.
  Safe to `docker compose -p agentsfleet-fix-runner-io down` once Indy confirms.
- `e2e-observability-platform_*` — a different project. Its api holds **:8080**,
  which is what breaks `make up` (see below).

## Tests / checks

- ✅ `make test-coverage-zig` + `make test-integration` + `make test-coverage-grade`
  — exit 0, numbers above.
- ✅ Filtered lane `-Dtest-filter=pool_exhaustion` — 84 tests, all pass, 52 probes.
- ✅ Red-proof on the runner test: removing the drain makes all five probes MISS
  (401/401/404/404/404 from their own post-acquire paths); restored green.
- ✅ `make harness-verify` — all gates green on each staged diff.
- ✅ Depth gate `unit=4236 integration=727` (CHORE(open) baseline 4205/719).
- ✅ Full lane green after `ab2e251f7` — `test-coverage-zig`, `test-integration`
  and `test-coverage-grade` all exit 0. Integration 1006 passed, 8 skipped,
  0 failed. Merged 91.46% (floor 89) / `agentsfleetd` 91.11% (floor 90) /
  `lib` 95.41% / `runner` 92.74%.
- ✅ `db/pool_test.zig:943` "migration lock serializes" failed once under load
  (kcov + docker + concurrent polling) and passed on a quiet box. It is a
  wall-clock assertion — `elapsed_ms < 1_000` — on advisory-lock contention,
  with its own two pools and no HTTP. A flake, not a regression; worth a
  tolerance if it recurs in CI.
- ⚠️ **`TEST_FILTER` is per-graph, and the two graphs are easy to confuse.**
  `make test-integration TEST_FILTER=x` filters the INTEGRATION binary only —
  the files registered in `src/agentsfleetd/integration_tests.zig`. Anything
  registered through a product file's own `test { _ = @import(...); }` block
  (`db/pool_test.zig`, `state/tenant_provider_test.zig` and its siblings) is in
  the UNIT graph and matches nothing there, which reads as "the filter is
  broken" — it is not. For a unit-graph file, run it directly:

  ```
  ZIG_GLOBAL_CACHE_DIR=~/.cache/agentsfleet/zig-global-cache \
  ZIG_LOCAL_CACHE_DIR=.tmp/zig-local-cache LIVE_DB=1 \
  TEST_DATABASE_URL="postgres://agentsfleet:agentsfleet@localhost:25796/agentsfleetdb?sslmode=disable" \
  zig build test -Dtest-filter=<token> --summary all
  ```

  `--summary all` is load-bearing: without it a passing run prints nothing, and
  a filter that matched ZERO tests prints exactly the same nothing.
- ⏳ `make lint-all`, `make test-unit-all`, `make memleak`, `make check-version`
  — NOT run. Every rubric S-row is still ungraded.

## Next steps

1. `git push origin feat/m173-error-path-coverage`.
2. Re-measure (both producers + grade) before trusting any class count.
3. **§1's 295 `errdefer` lines** — the largest class, and the one the milestone
   was originally justified on. The signature-blocked seven are closed, so this
   is the remaining leverage. Pick targets by reading the function, never by the
   signature heuristic.
4. §3, §4, §5 remain untouched — ~1,220 lines, §4 the bulk. Still multi-session.

## Risks / gotchas

Carried forward from the prior session (all still true):

- **A skipped integration test reports as PASSING.** `TestHarness.start` returns
  `SkipZigTest` when Postgres or Redis is unconfigured and the lane exits 0.
  `make test-integration` is the only lane that configures both. Never trust a
  green tick on a new integration test — mutation-check it.
- **`make test-integration TEST_FILTER=…` exits 1 on a clean run.** With zero
  skips Zig prints `All N tests passed.`, which the lane's tally check does not
  recognise, so it reports "no passing tests". The tests DID run. Confirm by
  running `zig-out/bin/agentsfleetd-integration-tests` directly with the lane's
  env, or read the line above the error.
- **Never run a lane while a Zig commit is in its hooks** — the pre-commit
  self-test runs a real `make test-integration`, and a concurrent lane moves the
  coverage `source_digest` and reds a commit whose diff is fine.
- **A filtered lane clobbers the merged report.** Re-run both producers in full
  before grading.
- **The harness cannot send a bodiless PUT/POST.** Those "body required" paths
  are unreachable from it and deliberately left open.
- **Read the validator before writing a body**, and **read the error registry
  before asserting a status** (`UZ-APIKEY-007` is 409, not 400).
- **Milestone markers are banned in code comments** (RULE TST-NAM), and
  **`zig fmt` before committing** or the pre-commit `make-graph` lane fails
  unhelpfully.
- **Class counts are NET, not progress.**

New this session:

- **Do not classify a connection-failure path by its position in the file.**
  Twice this produced a wrong answer. `memory.zig:181` and
  `credentials_mint.zig:195` looked like second acquires but are separate
  functions sharing a file — both closed by plain draining.
  `identity_events_delete`'s four lines looked like two-plus-two but are all
  reachable in one starved request, because `enumerateTenantFleets` returns null
  and `runDelete` carries on to its own failed acquire. **Read the call order.**
- **A handler that acquires before it verifies is free to reach.**
  `ingress/github` takes the connection first, because the secret its signature
  check needs is what that connection loads. Headers present is the whole
  requirement. Check this before assuming a signature fixture is owed.
- **Runner-plane paths need a pool-free lookup.** `cmd/serve_runner_lookup.zig`
  resolves the `agt_r` by acquiring a connection itself, so a drained pool
  answers `UZ-AUTH-004` at the middleware and no handler runs. Wiring the real
  lookup produces a green test proving nothing. The middleware's own path is
  already proven in `runner_bearer.zig`.

## Product findings this session — NOT fixed, and not this branch's scope

Both are written up in the spec's Discovery section. R6 forbids behaviour
changes here, so both belong to a follow-up workstream.

1. **Fleet delete can silently half-complete.** `innerDeleteFleet` cancels the
   fleet's schedules through the cron service *before* purging its rows, and
   releases its connection across that network call. When the re-acquire fails
   the caller is told the delete failed while the schedules are already gone —
   the fleet still lists and never fires again until someone retries. The
   ordering is the defect: purging first would leave an orphan schedule that
   fires at a removed fleet, answers not-found and retires itself. `create.zig`
   has the same shape and says so in its own log (`HINT_ROW_ORPHANED_MANUAL_RECOVERY`).
2. **`make up` cannot start the daemon from scratch** (found by trying to do one
   full live run; the API does boot and answer correctly once past it):
   - `OIDC_ISSUER` / `OIDC_AUDIENCE` unset — both derivable from
     `ui/packages/app/.env.local`: the issuer is the base64 payload of
     `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY`, the audience is `NEXT_PUBLIC_API_URL`.
   - `AUTH_SESSION_CODE_PEPPER` and `AUDIT_LOG_PEPPER` unset — any local random
     value works; they are hashing salts for a disposable database.
   - `MIGRATE_ON_START=1` with `DATABASE_URL_MIGRATOR` never set — **a pure
     compose bug, no credential involved, fails for everyone every time.**
   - qstash hard-binds :8080; a collision surfaces as a raw docker networking
     error. Override with `AGENTSFLEET_QSTASH_HOST_PORT`.
   - `docker-compose.yml:130` claims the inline block "already satisfies a
     from-scratch `make up`". It does not.
   A generator writing `.env.agentsfleetd.local` from the app env, wired as a
   `make up` prerequisite, closes all of it (~20 lines). Not started — needs its
   own branch off `main`.

## Not ours

`deploy (dev)` / `cli-acceptance-dev` is red on `main` and has failed on **every
run back to Aug 19** (`3c98605ba` onward) — it is not a #622 regression. The live
lane's `steer` test gets `status: fleet_error`, `failure_label: runner_crash`,
`failure_detail: ApiError`, zero tokens, 200 ms. **Another agent owns this.**
