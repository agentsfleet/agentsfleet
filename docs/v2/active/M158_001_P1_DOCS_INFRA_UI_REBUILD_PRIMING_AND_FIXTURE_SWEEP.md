<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M158_001: A rebuilt environment comes back primed, and the acceptance suite leaves nothing behind

**Prototype:** v2.0.0
**Milestone:** M158
**Workstream:** 001
**Date:** Aug 04, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — operator-facing: a rebuild currently comes back with an empty model catalogue, a wipe can be silently undone, and two Continuous Integration (CI) lanes are red on main
**Categories:** DOCS, INFRA, UI, API
**Batch:** B1 — one Pull Request; the five slices share no files and can land in any order
**Branch:** `feat/m158-priming-and-sweep`
**Test Baseline:** `unit=3424 integration=587` — recorded at CHORE(open) via `make _lint_zig_test_depth`
**Depends on:** none — M154_001 is merged and this builds on the playbooks it left in place
**Provenance:** LLM-drafted (Claude Opus 5, Aug 04, 2026), from a read of the teardown and founding playbooks while planning the dev and production rebuild
**Canonical architecture:** `playbooks/ARCHITECTURE.md` §the route selector and script shape

---

## Overview

**Goal (testable):** A dev environment torn down and rebuilt from the playbooks comes back with a populated `core.model_library`, an acceptance suite run against it leaves zero fixture fleets behind — including when the run is interrupted — and the `memleak` and `test-coverage-zig` lanes pass deterministically on main.

**Problem:** Five gaps, the first three found while planning the M154 rebuild and the last two by the red lanes that rebuild left on main. Each costs an operator a cycle, leaves residue nobody sees, or trains the team to re-run a gate:

1. The `app-dev` database holds 400+ fleets. The acceptance suite's per-spec cleanup works, but the backstop sweep that covers interrupted runs reaps six name prefixes while the specs mint about twenty-two. Leaked fleets are not inert — each carries a seeded cron trigger that keeps waking runners.
2. A freshly rebuilt environment has an empty model catalogue. The priming tool exists and works, but it lives in the local-development Makefile fragment and no playbook references it, so nothing tells an operator to run it. Every fleet needs a model, so the environment looks deployed and is not usable.
3. Both teardown playbooks require "stop traffic and every writer" as a precondition and give no command for it. A running `agentsfleetd` machine that Fly.io restarts against the just-emptied database re-runs its own older migrations, and the next deployment then fails `ensureCanonical` with `error.MigrationSchemaAhead` — so the teardown has to be run a second time. This is not hypothetical: it is why `deploy (dev)` is red on main.
4. Every Redis dial routes through `std.Io.net.HostName.connect`, whose happy-eyeballs fan-out awaits an `Io.Group`. Zig 0.16.0's group await parks on a futex word in the awaiter's own stack frame, and the finishing worker publishes the wake *before* dereferencing that word — so the awaiter can return and pop the frame first. The `memleak` lane catches the resulting `futex(2)`-on-reclaimed-stack intermittently and `make/bench.mk` deliberately refuses to suppress it.
5. `catalog_etag_integration_test`'s lock probe waits five seconds for a lock waiter to appear in `pg_stat_activity` and calls its absence `CatalogPatchNeverBlocked`. Under the coverage lane's kcov instrumentation the patch is slower than that bound, so a timing budget stands in for a correctness claim and the lane fails on a green codebase.

**Solution summary:** Five independent slices. The acceptance sweep stops matching fleet names and sweeps by ownership instead, which cannot rot as specs are added. The model catalogue priming becomes a first-class operations playbook with the approval gates every other destructive-or-billing operation already has, referenced from the deployment step that needs it. The stop-the-writer precondition becomes an executable, verified step of both teardown gates rather than a sentence. The Redis dial resolves and races addresses itself, so no stdlib futex word ever outlives its frame. The lock probe waits on the worker's own completion rather than on a clock, so it fails when the patch does not block and only then.

## PR Intent & comprehension handshake

- **PR title (eventual):** Prime the catalogue on rebuild, sweep fixture fleets by ownership, green the red lanes
- **Intent (one sentence):** An operator who tears down and rebuilds an environment gets a usable one back without remembering an undocumented command, the acceptance suite stops accumulating fleets in it, and the two lanes that went red on the rebuild fail only when something is actually wrong.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `ui/packages/app/tests/e2e/acceptance/fixtures/teardown.ts` — the sweep being changed. Read the comment on `cleanWorkspaceFleets` explaining why the *per-spec* path needs prefix scoping; that reason does not hold for the global-teardown path, and §1 turns on exactly that distinction.
2. `playbooks/operations/ip_allowlisting/` — the reference shape for the new operations playbook: `00_gate.sh` with an explicit ordered dispatch, numbered check/apply/verify steps, and a co-located `*_test.sh`.
3. `playbooks/operations/teardown/redis/00_gate.sh` and `02_teardown.sh` — the approval pattern §2 must mirror: an `ALLOW_*` variable, `playbooks_require_vault_read_approval`, `playbooks_require_op_auth`, one environment only, and a typed confirmation before a write.
4. `scripts/seed-models.mjs` — the tool §2 wraps. Note it contains a byte that makes `file` report it binary, so plain `grep` skips it silently; use `grep -a` when reading it.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/tests/e2e/acceptance/fixtures/teardown.ts` | EDIT | Sweep by workspace ownership rather than by name prefix; count and report failed deletes instead of swallowing them |
| `ui/packages/app/tests/e2e/acceptance/global-teardown.ts` | EDIT | Its doc comment describes the prefix-based sweep; it must describe what the sweep now does |
| `ui/packages/app/tests/e2e-teardown-sweep.test.ts` | CREATE | Unit proof that a fleet under any spec-minted name is reaped, and that a failed delete is surfaced |
| `playbooks/operations/model_catalogue/001_playbook.md` | CREATE | The runbook: owner, executor, verifier, required evidence |
| `playbooks/operations/model_catalogue/00_gate.sh` | CREATE | Explicit ordered dispatch entry point |
| `playbooks/operations/model_catalogue/01_diff.sh` | CREATE | Read-only catalogue diff against the target environment |
| `playbooks/operations/model_catalogue/02_apply.sh` | CREATE | The guarded write, behind approval and a typed environment confirmation |
| `playbooks/operations/model_catalogue/03_verify.sh` | CREATE | Proves the catalogue is non-empty and matches the allowlist |
| `playbooks/operations/model_catalogue/model_catalogue_test.sh` | CREATE | Local regression tests for the four scripts above |
| `playbooks/README.md` | EDIT | The auto-checked inventory block gains the new operations row |
| `playbooks/founding/04_deploy_dev/001_playbook.md` | EDIT | Required result gains the catalogue priming step |
| `playbooks/founding/07_deploy_prod/001_playbook.md` | EDIT | Same, for the production route |
| `playbooks/operations/teardown/database/001_playbook.md` | EDIT | The stop-the-writer precondition gains its command |
| `playbooks/operations/teardown/database/00_gate.sh` | EDIT | Stop-the-writer becomes a dispatched step |
| `playbooks/operations/teardown/database/01_stop_writers.sh` | CREATE | The executable precondition, verified rather than assumed |
| `playbooks/operations/teardown/redis/001_playbook.md` | EDIT | Same precondition, same command |
| `playbooks/operations/teardown/redis/00_gate.sh` | EDIT | Same dispatch change |
| `playbooks/operations/teardown/database/03_verify.sh` | EDIT | Its closing guidance names the catalogue priming step as the next action |
| `src/agentsfleetd/cmd/serve_lifecycle_integration_test.zig` | EDIT | Takes the serial `common.globalIo()` so no worker thread can lose the stdlib futex race; the detach bookkeeping the owned Io required goes with it, and the comment citing an `std.Io.Select` that `9ee3a075b` deleted is corrected (RULE NLR) |
| `src/agentsfleetd/http/handlers/library/catalog_etag_integration_test.zig` | EDIT | The lock probe waits on the worker's completion instead of a five-second clock |
| `bun.lock` | EDIT | In-range dependency refresh across the workspaces and the CLI project |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **NDC** (removing `LEAKED_FLEET_PREFIXES` must leave no unreferenced helper behind), **ORP** (orphan sweep for the removed constant and its imports), **UFS** (the new shell names its environment labels, vault item paths and the approval variable as constants rather than repeating literals), **FLL** (every new shell script and the new test stay under the 350-line cap), **NLR** (the two teardown playbooks are being touched, so their stale prose is corrected in the same edit rather than left).
- **`~/Projects/dotfiles/dispatch/write_shell.md`** — all five new shell scripts: quoted expansions, array arguments, temporary-file cleanup, no untrusted `eval`, repository shell compatibility.
- **`~/Projects/dotfiles/dispatch/write_ts_adhere_bun.md`** — the TypeScript sweep change and its test: `const` and import discipline, and the TypeScript file-shape decision recorded at PLAN.
- **`~/Projects/dotfiles/dispatch/write_zig.md`** — §4 and §5: lifecycle discipline on the Io the fixture stops owning (the detach bookkeeping is deleted, not left inert), atomic publication ordering in §5's `PatchOutcome`, the ≤350/≤50/≤70 caps, and the mandatory cross-compile to both linux targets.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — §4 and §5 each edit one Zig test file | Both are test-only edits with no allocation added; cross-compile to `x86_64-linux` and `aarch64-linux` still runs before the Pull Request |
| PUB / Struct-Shape | no — neither Zig edit adds public surface; §5's `PatchOutcome` is file-private | N/A |
| File & Function Length (≤350/≤50/≤70) | yes — five new shell scripts and one new test file | Each new script does one action; the diff/apply/verify split keeps every file well under the cap. §4 is a net deletion and §5 adds under ten lines |
| UFS (repeated/semantic literals) | yes — environment labels, vault references and the approval variable appear in more than one place | Declare them once at the top of each script and in a shared block where two scripts need the same value; §5's poll bound stays the single existing named constant |
| UI Substitution / DESIGN TOKEN | no — test files only, no components or styling | N/A |
| LIFECYCLE | yes — §4 removes an owned `std.Io.Threaded` and its teardown bookkeeping | The replacement io is process-immortal and owns no joinable worker, so the detach flag and conditional `deinit` are deleted rather than left inert (RULE NDC) |
| LOGGING / ERROR REGISTRY / SCHEMA | no — no new operator log surface, no new `UZ-XXX-NNN` code, no `schema/*.sql` change | N/A |
| MILESTONE-ID (RULE TST-NAM) | yes — new playbook shell and markdown | No `M158_001` string in any `playbooks/` or `ui/` file; the identifier stays in this spec and in commit messages only |

## Prior-Art / Reference Implementations

- **Reference:** `playbooks/operations/ip_allowlisting/` — the fullest expression of the operations playbook shape in the repository: `00_gate.sh` plus numbered inventory/target/apply/verify steps plus a co-located test. §2 mirrors its file layout and its separation of the read-only arm from the guarded write.
- **Reference:** `playbooks/operations/teardown/redis/02_teardown.sh` — the approval pattern for a write that must not happen by accident: an `ALLOW_*` variable checked twice, vault-read approval, `op` authentication, one environment only, and a typed confirmation read from the operator. §2 adopts it because catalogue rows are billing rates.
- **Divergence:** the sweep in §1 has no prior art to mirror; it is a deletion of a mechanism rather than an addition. The shape it moves to already exists in the same file as the `JOURNEY_WORKSPACE_RE` branch, which sweeps a fixture-owned workspace whole.

## Sections (implementation slices)

### §1 — The acceptance sweep reaps by ownership, not by name

The backstop sweep matches fleet names against a hand-maintained list of six prefixes while the specs mint roughly twenty-two, so most leaked fleets are never reaped and the list must be edited every time a spec is added. That is the same rot pattern `playbooks/operations/teardown/database/03_verify.sh` already records for its role list, and it has the same outcome: a check that silently covers less than it appears to.

The list is unnecessary. `sweepLeakedFixtureFleets` authenticates as each persistent fixture user and lists only workspaces that user owns, and `global-setup.ts` seeds no persistent fleets — so every fleet it can see is a test artifact by construction. Prefix scoping is load-bearing in the *per-spec* `afterEach` path, where parallel workers share one workspace and an unscoped delete would remove a sibling spec's fleet mid-test; at global teardown no test is running and that reason does not hold. The `JOURNEY_WORKSPACE_RE` branch in the same function already sweeps a fixture-owned workspace whole for exactly this reason.

**Implementation default:** delete `LEAKED_FLEET_PREFIXES` and sweep every fixture-owned workspace whole, because a keep-nothing rule cannot fall behind the specs. `cleanWorkspaceFleets` keeps its optional prefix parameter — the per-spec callers still need it.

Silent failure compounds the gap: `cleanWorkspaceFleets` swallows every delete error, so a fleet the sweep matched but failed to remove leaks with no signal even on a green run.

- **Dimension 1.1** — a fleet whose name matches no previously listed prefix is reaped by the sweep → Test `test_sweep_reaps_a_fleet_under_any_name` — **DONE.** `LEAKED_FLEET_PREFIXES` is deleted and `sweepLeakedFixtureFleets` calls `cleanWorkspaceFleets` with no prefix, so every fleet in every fixture-owned workspace is reaped. The test seeds `console-ab12`, `pulse-cd34` and `nav-ef56` — three names no entry in the old list would have matched — and asserts each one's identifier reaches a delete call, rather than only checking a total, so a count that happened to add up cannot pass it.
- **Dimension 1.2** — the sweep reports the count it removed and the count it failed to remove, and a failed delete appears in that second count rather than being discarded → Test `test_sweep_reports_failed_deletes` — **DONE.** `cleanWorkspaceFleets` returns `SweepCounts` instead of a bare number and its `catch` increments `failed` and logs the fleet identifier and name, rather than swallowing. `sweepLeakedFixtureFleets` aggregates both counts and prints its summary through `console.error` when anything failed, so a leaking run cannot look clean.
- **Dimension 1.3** — a workspace listing that fails for one fixture user does not stop the sweep for the others → Test `test_sweep_continues_past_a_dead_fixture` — **DONE.** The listing rejection for the first fixture user is caught per user, counted as a failure, and the loop continues; the test asserts both that later users were still swept and that the dead one is not silent.
- **Dimension 1.4** — the destructive-target guard still refuses a non-development Application Programming Interface (API) host before any listing or deletion → Test `test_sweep_refuses_an_unsafe_target` — **DONE.** The test asserts the throw AND that `listWorkspaces`, `listFleets` and `delete` were never called — the guard has to fire before the first read, since listing production with real fixture credentials is already wrong.

### §2 — Catalogue priming is an operations playbook with the guard rails it lacks

`core.model_library` ships empty by design and is populated by `scripts/seed-models.mjs`, driven by `scripts/model-library-allowlist.json`. The tool is correct and already has a committed byte-stable fixture. What is missing is discoverability and approval: it is declared in the local-development Makefile fragment, no playbook references it, and `APPLY=1` writes billing rates with no approval variable and no confirmation of which environment is being written — a lower bar than the Redis flush, which only deletes cache.

Making it an operations playbook rather than a founding step is deliberate: `seed-models.mjs` documents the rate refresh as a recurring job, so it is a maintenance operation that the deployment steps merely reference.

**Implementation default:** the gate takes an action argument (`diff` or `apply`) and an `ENV` of exactly `dev` or `prod`, mirroring `ip_allowlisting`; the apply arm requires its own `ALLOW_*` variable plus vault-read approval plus a typed environment confirmation, mirroring the Redis teardown.

- **Dimension 2.1** — the gate refuses an unknown or absent `ENV` before executing any step → Test `test_should_reject_unknown_environment_before_dispatch`
- **Dimension 2.2** — the gate refuses `ENV=all`, so the two environments can never be written in one invocation → Test `test_should_reject_all_environments`
- **Dimension 2.3** — the apply arm refuses to write without its approval variable → Test `test_should_require_apply_approval`
- **Dimension 2.4** — the diff arm performs no write and needs no approval variable → Test `test_diff_arm_writes_nothing`
- **Dimension 2.5** — the verify arm fails when the catalogue is empty and passes when it matches the allowlist → Test `test_verify_fails_on_empty_catalogue`
- **Dimension 2.6** — the playbooks inventory in `playbooks/README.md` names the new directory, and reference integrity resolves every path it cites → Test `make check-playbooks`
- **Dimension 2.7** — the development and production deployment steps name the priming step in their required result, so an operator following the founding sequence reaches it → Test `test_deploy_steps_reference_catalogue_priming`

### §3 — Stop-the-writer is an executable step, not a sentence

Both teardown playbooks list "stop traffic and every writer" as a precondition and neither gives a command. The failure it guards against is concrete and expensive: a live `agentsfleetd` machine restarted by Fly.io against the emptied database re-applies its own older migrations, and the next deployment then refuses to migrate with `error.MigrationSchemaAhead`, forcing a second teardown.

**Implementation default:** a `01_stop_writers.sh` step dispatched by both teardown gates before the credential check, using `flyctl scale count 0` — the form `.github/workflows/release.yml` already uses for scaling — and *verifying* the machine count reached zero rather than assuming the command worked. The step is idempotent: an already-stopped application is a pass, not a failure.

- **Dimension 3.1** — the step scales the environment's application to zero and confirms zero machines are running before the teardown proceeds → Test `test_stop_writers_verifies_zero_machines`
- **Dimension 3.2** — an application that is already stopped passes rather than erroring → Test `test_stop_writers_is_idempotent`
- **Dimension 3.3** — a failure to reach zero blocks the teardown instead of warning → Test `test_stop_writers_blocks_on_failure`
- **Dimension 3.4** — both teardown gates dispatch the step in explicit order before the credential check → Test `test_teardown_gates_dispatch_stop_writers_first`

### §4 — The lifecycle test stops manufacturing a race it no longer needs

`std.Io.net.HostName.connect` is happy-eyeballs: it resolves the host, fans a connect task out per address into an `Io.Group`, and awaits the group. Zig 0.16.0's `Group.Task.start` publishes the wake and *then* dereferences the awaiter's futex word:

```
_ = to_signal.fetchAdd(1, .release);   // awaiter may now return and pop its frame
Thread.futexWake(&to_signal.raw, 1);   // ...and this reads that frame
```

The word lives in the awaiter's own stack frame, so the `futex(2)` syscall can land on reclaimed stack. `make/bench.mk:20-28` records the owner's verdict: a suppression was tried and reverted because *"valgrind was right"*. M143 closed the two test call sites that hit it by moving them to `common.globalIo()`, where `noGroupAsync` runs group tasks inline and no worker thread exists to lose the race.

The boot→drain lane reaches it through a third site. `serve_lifecycle_integration_test.zig` builds its own `std.Io.Threaded`, and the daemon's Redis dial then goes through `HostName.connect` on it. The justification recorded in that file is now false: it cites an `std.Io.Select` in `subscription_hub_wire.connectBounded` that commit `9ee3a075b` deleted. `serve.zig`, `subscription_hub.zig` and `subscription_hub_reader.zig` carry no reference to `io.async`, `concurrent`, `Select` or `ConcurrencyUnavailable` — nothing in the boot path needs io concurrency any more, so the fixture is manufacturing the only concurrency that can lose the race.

**Implementation default:** the test takes `common.globalIo()`, matching the remedy M143 applied to its siblings. This is a net deletion — a process-immortal io needs no `deinit`, so the `daemon_detached` flag, its conditional defer, and the deliberate leak on the detach paths all go with it, along with the use-after-free hazard that bookkeeping existed to dodge.

**What this deliberately does not do:** the daemon still dials Redis through `HostName.connect` on a threaded io in production, so the upstream use-after-scope still ships. Fixing that means the dial owning its own resolve-and-race, which is scoped out by decision — see Discovery. The honest cost is that the lane stops being able to observe the defect; the honest counterweight is that it was never fixing it either, only tripping over it about one run in ten.

- **Dimension 4.1** — the lifecycle test drives the real `serve.run` on the serial io and still reaches boot → SIGTERM → drain, proven by its run marker → Test `SERVE_LIFECYCLE_BOOT_DRAIN_RAN` present in the gated run — **DONE.** `serve_io` is now `common.globalIo()`, passed unchanged into `RunArgs.io`, so the test still drives the production `serve.run` rather than a mirror. The marker assertion at the end of the test is untouched, so a run that skipped on misconfigured infrastructure still cannot pass vacuously. Graded against a live lane at VERIFY (4.3).
- **Dimension 4.2** — no Io lifecycle bookkeeping survives: the detach flag and its conditional teardown are gone, not merely unused → Test `test_lifecycle_fixture_owns_no_io` — **DONE.** All four `daemon_detached` assignments and the `defer if (!daemon_detached) serve_io.deinit()` are deleted; `grep -c 'daemon_detached\|serve_io.deinit'` is `0`. The three failure paths keep their `thread.detach()` — that is about not hanging the suite on a wedged daemon, which is unrelated to Io ownership. RULE NDC: the flag is removed rather than left inert.
- **Dimension 4.3** — the boot→drain lane runs clean under valgrind → Test `make memleak` — graded at VERIFY.
- **Dimension 4.4** — the fixture constructs no Io of its own, so there is no owned concurrency left to justify (RULE NLR: the surviving prose explains why the old `Io.Select` justification lapsed, which is history rather than a live claim) → Test `grep -c "Threaded.init" src/agentsfleetd/cmd/serve_lifecycle_integration_test.zig` is `0` — **DONE.** Verified `0`. The two surviving `std.Io.Threaded` / `Io.Select` mentions are both inside the replacement comment explaining why the fixture stopped owning an Io; neither is a live reference. The comment also records explicitly that this moves the test out of the defect's reach and not the daemon.

### §5 — The catalog lock probe fails on the claim, not on the clock

`waitForCatalogLockWaiter` polls `pg_stat_activity` 250 times at 20 ms and returns `error.CatalogPatchNeverBlocked` if no lock waiter appeared inside five seconds. The claim under test is "the If-Match check serializes with a concurrent write" — but the assertion actually made is "a lock waiter appears within five seconds", and those differ the moment the run is slow. Under the coverage lane's kcov instrumentation the worker's PATCH does not reach its blocking statement inside the budget, so the lane fails on a codebase where the same test passes uninstrumented.

The worker itself carries the real signal. If the PATCH completed without ever blocking, the claim is genuinely false and the test must fail; if it has not completed, waiting longer is correct rather than generous. Publishing the worker's completion through an atomic lets the poll distinguish the two, and the wait bound stops being load-bearing.

**Implementation default:** `PatchOutcome` gains an atomic `done` flag the worker releases after storing its status; the poll acquires it each round and fails fast with `CatalogPatchNeverBlocked` only on an observed completion. The round limit stays as a backstop against a genuine hang and is raised to cover an instrumented run, returning a distinct `CatalogPatchLockWaitTimedOut` so the two causes are never confused again.

- **Dimension 5.1** — a patch that completes without ever taking the lock fails as `CatalogPatchNeverBlocked` → Test `test_probe_fails_when_patch_never_blocks` — **DONE.** The round decision is extracted into a pure `classifyPollRound(lock_waiters, worker_done, rounds_left) PollVerdict`, so the verdict is provable with no live database — the named test landed as `unit: a completed patch that never took the lock fails immediately`, asserting `.never_blocked` both with rounds remaining and with none, since the verdict must not depend on where in the budget completion is observed. `PatchOutcome` gained an atomic `done` the worker releases on **every** exit including its early `catch return` paths, so a worker that died building its request fails the probe immediately rather than burning the backstop.
- **Dimension 5.2** — a patch still in flight keeps the probe waiting rather than failing → Test `test_probe_waits_while_patch_in_flight` — **DONE.** Landed as `unit: an in-flight patch keeps the probe waiting rather than failing`. This is the round kcov produces for thousands of iterations, and it is now `.keep_waiting` rather than an assertion failure — the whole reason the lane was red. A companion test pins that a lock waiter observed on the same round a completion lands still reads `.blocked`: the worker parked, which is the claim.
- **Dimension 5.3** — exhausting the backstop reports the timeout distinctly from the never-blocked verdict → Test `test_probe_distinguishes_timeout_from_never_blocked` — **DONE.** Landed as `unit: exhausting the backstop is reported apart from never having blocked`. `waitForCatalogLockWaiter` now returns `CatalogPatchLockWaitTimedOut` on exhaustion and `CatalogPatchNeverBlocked` only on an observed completion, so a genuine hang can never again be reported as a disproved serialization claim. The completion load is deliberately sequenced **after** the lock read, so a worker released between the two observations still counts as having blocked.
- **Dimension 5.4** — the coverage lane passes with the suite instrumented → Test `make test-coverage-zig` — graded at VERIFY.

## Interfaces

```
playbooks/operations/model_catalogue/00_gate.sh
  ACTION   diff | apply          (required; unknown value exits 2)
  ENV      dev | prod            (required; absent, unknown, or "all" exits 2)
  ALLOW_VAULT_READS=1            (required for both arms — both read the vault)
  ALLOW_MODEL_CATALOGUE_WRITES=1 (required for ACTION=apply only)

  Usage:
    ACTION=diff  ENV=dev ALLOW_VAULT_READS=1 \
      ./playbooks/operations/model_catalogue/00_gate.sh
    ACTION=apply ENV=dev ALLOW_VAULT_READS=1 ALLOW_MODEL_CATALOGUE_WRITES=1 \
      ./playbooks/operations/model_catalogue/00_gate.sh

  Exit: 0 success · 1 step failure · 2 invalid input (before any step runs)

playbooks/operations/teardown/{database,redis}/01_stop_writers.sh
  ENV      dev | prod            (required; inherited from the gate)
  Exit: 0 zero machines running (including already-stopped) · 1 could not reach zero

ui/packages/app/tests/e2e/acceptance/fixtures/teardown.ts
  sweepLeakedFixtureFleets(): Promise<{ removed: number; failed: number }>
    // was Promise<void>; the counts are what makes a silent failure observable
  cleanWorkspaceFleets(handle, workspaceId, namePrefix?): Promise<{ removed: number; failed: number }>
    // namePrefix retained — the per-spec afterEach callers still scope by it

src/agentsfleetd/http/handlers/library/catalog_etag_integration_test.zig
  const PatchOutcome = struct {
      status: std.atomic.Value(u16)   // was a plain u16 read after join
      done:   std.atomic.Value(bool)  // released by the worker, acquired by the poll
  }
  fn waitForCatalogLockWaiter(conn: *pg.Conn, outcome: *const PatchOutcome) !void
    // was (conn) — the outcome is what tells "never blocked" apart from "slow"
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Sweep target is not a development host | `NEXT_PUBLIC_API_URL` points at production | `assertDestructiveTargetIsSafe` throws before any listing or deletion; no fleet is read or removed |
| Fixture user's workspace listing fails | Tenant purged, token expired, API unavailable | Logged with the fixture key; the sweep continues to the remaining fixture users; the failure count is non-zero |
| Fleet delete refused | Fleet in a state the delete path rejects, or a transient API error | Counted as failed, named in the summary line; the sweep continues and the run reports a non-zero failed count |
| Catalogue apply without approval | `ALLOW_MODEL_CATALOGUE_WRITES` unset | Exits 2 before reading the vault or contacting the database; nothing is written |
| Catalogue apply to the wrong environment | Operator sets `ENV=prod` intending `dev` | The typed confirmation must match the environment label exactly; a mismatch aborts before any write |
| Catalogue verify against an empty table | Priming was skipped or silently failed | Verify exits non-zero and names the row count it found, so the deployment step cannot be recorded as green |
| Writer still running at teardown | `flyctl scale count 0` failed or partially applied | `01_stop_writers.sh` exits 1 and the gate stops; the teardown never reaches the destructive step |
| Writer application does not exist | Environment was never deployed | Treated as zero machines running — a pass, so a first-time teardown is not blocked by a missing application |
| Boot path needs io concurrency after all | A future change reintroduces `io.async` or `Io.Select` under `serve.run` | The serial io fails the call with `ConcurrencyUnavailable` and `serve.run` exits non-zero, so the lifecycle test goes red immediately rather than silently running a different shape than production |
| Upstream futex race reaches another lane | Any test that dials on its own threaded Io | Unchanged by this milestone — the lane reports it, and the remedy is the dial rewrite scoped out in Discovery, not a suppression |
| Lock probe's patch completes without blocking | The If-Match check stopped taking the row lock — a real regression | The observed completion fails the test as `CatalogPatchNeverBlocked`, immediately rather than after the backstop expires |
| Lock probe exhausts its backstop | A genuine hang, or an environment far slower than instrumented Continuous Integration (CI) | `CatalogPatchLockWaitTimedOut` — distinct from the never-blocked verdict, so the cause is never inferred from the wrong signal again |

## Invariants

1. The acceptance sweep never deletes a fleet outside a workspace owned by a persistent fixture user — enforced by the sweep listing workspaces only through an authenticated fixture handle, never by name or identifier supplied elsewhere.
2. No destructive or billing-rate write runs against more than one environment per invocation — enforced by both gates rejecting an absent, unknown, or `all` value for `ENV` with exit 2 before dispatching any step.
3. A catalogue write cannot happen without an explicit approval variable — enforced by the check running before the vault read, so the failure path cannot reach a credential.
4. A teardown cannot reach its destructive step while a writer is running — enforced by `01_stop_writers.sh` being a dispatched gate step whose non-zero exit halts the ordered list, not a documented precondition.
5. Every path the playbooks cite resolves, and the README inventory matches the directory tree exactly — enforced by `make check-playbooks`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product or operator signal changes | not applicable | This milestone adds test-harness cleanup and operator playbooks; it emits no product analytics, adds no dashboard surface, and renames no existing event | not applicable | No credential, vault value, or connection string is printed by any new script — the catalogue scripts pass the database Uniform Resource Locator (URL) by environment name, never as an argument | `test_scripts_print_no_credentials` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_sweep_reaps_a_fleet_under_any_name` | A fixture workspace containing fleets named `console-ab12`, `pulse-cd34` and `nav-ef56` — none matching a previously listed prefix — is swept empty |
| 1.2 | unit | `test_sweep_reports_failed_deletes` | Delete rejects for one of three fleets → the returned counts are `removed: 2, failed: 1`, and the summary line names the failure |
| 1.3 | unit | `test_sweep_continues_past_a_dead_fixture` | Workspace listing throws for the first fixture user → the remaining users are still swept and their fleets removed |
| 1.4 | unit | `test_sweep_refuses_an_unsafe_target` | `NEXT_PUBLIC_API_URL` set to a production host → the sweep throws before any list or delete call is made |
| 2.1 | unit | `test_should_reject_unknown_environment_before_dispatch` | `ENV=staging` → exit 2 and no step executed |
| 2.2 | unit | `test_should_reject_all_environments` | `ENV=all` → exit 2 and no step executed |
| 2.3 | unit | `test_should_require_apply_approval` | `ACTION=apply` without `ALLOW_MODEL_CATALOGUE_WRITES` → exit 2, and no vault read is attempted |
| 2.4 | unit | `test_diff_arm_writes_nothing` | `ACTION=diff` runs the diff step only; the apply step is never dispatched |
| 2.5 | unit | `test_verify_fails_on_empty_catalogue` | Verify against a catalogue reporting zero rows → non-zero exit naming the count; against a populated one → exit 0 |
| 2.6 | unit | `make check-playbooks` | README inventory matches the tree exactly and every cited path resolves |
| 2.7 | unit | `test_deploy_steps_reference_catalogue_priming` | Both `04_deploy_dev` and `07_deploy_prod` playbooks cite the catalogue playbook path |
| 3.1 | unit | `test_stop_writers_verifies_zero_machines` | Scale reports success but a machine remains → exit 1; zero machines → exit 0 |
| 3.2 | unit | `test_stop_writers_is_idempotent` | An application already at zero machines, and an application that does not exist → both exit 0 |
| 3.3 | unit | `test_stop_writers_blocks_on_failure` | The step exits non-zero → the gate stops and the teardown step is never executed |
| 3.4 | unit | `test_teardown_gates_dispatch_stop_writers_first` | Both teardown gates list the stop-writers step before the credential check in their explicit command list |
| 4.1 | integration | boot→drain run marker | The gated run prints `SERVE_LIFECYCLE_BOOT_DRAIN_RAN`, so the lifecycle test is proven to have executed rather than skipped on the serial io |
| 4.2 | unit | `test_lifecycle_fixture_owns_no_io` | `grep -c "daemon_detached\|serve_io.deinit" src/agentsfleetd/cmd/serve_lifecycle_integration_test.zig` is `0` — the bookkeeping is deleted, not left inert |
| 4.3 | integration | `make memleak` | Exit 0, and the boot→drain lane reports no memcheck finding of any class |
| 4.4 | unit | owned-Io sweep | `grep -c "Threaded.init" src/agentsfleetd/cmd/serve_lifecycle_integration_test.zig` is `0` — the fixture constructs no Io |
| 5.1 | unit | `test_probe_fails_when_patch_never_blocks` | An outcome whose `done` is already released with no lock waiter present → `CatalogPatchNeverBlocked` on the first poll round, not after the backstop |
| 5.2 | unit | `test_probe_waits_while_patch_in_flight` | `done` unset and no lock waiter yet → the probe keeps polling rather than failing |
| 5.3 | unit | `test_probe_distinguishes_timeout_from_never_blocked` | Backstop exhausted with `done` never released → `CatalogPatchLockWaitTimedOut`, a different error than 5.1's |
| 5.4 | integration | `make test-coverage-zig` | Exit 0 with the suite under kcov — the lane that failed on `ae511e71c` |
| 2.3 | unit | `test_apply_aborts_on_confirmation_mismatch` | `ACTION=apply ENV=prod` with the operator typing `dev` → aborts before any write, and the catalogue is untouched |
| — | unit | `test_scripts_print_no_credentials` | Every new script run with a stubbed vault emits no vault value, connection string, or Application Programming Interface (API) key on stdout or stderr |
| — | regression | `test_per_spec_cleanup_still_scopes_by_prefix` | `cleanWorkspaceFleets` called with a prefix removes only matching fleets, so a parallel worker's rows survive — the behaviour §1 must not break |
| — | regression | `test_existing_teardown_gates_still_reject_unknown_env` | The database and Redis gates keep their existing `ENV` rejection after gaining a step |
| — | idempotency | `test_catalogue_apply_is_repeatable` | Applying twice against the same catalogue leaves the same row count, since the generated statements upsert |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The name-prefix list is gone from the sweep (§1) | `grep -rn "LEAKED_FLEET_PREFIXES" ui/ \| wc -l` | `0` | P0 | |
| R2 | The sweep and its failure reporting are proven (§1) | `make test-unit-app` | exit 0 | P0 | |
| R3 | The catalogue playbook exists and its regression tests pass (§2) | `bash playbooks/operations/model_catalogue/model_catalogue_test.sh` | exit 0 | P0 | |
| R4 | Both teardown playbooks name the stop-writer command (§3) | `grep -rln "flyctl scale count" playbooks/operations/teardown/ \| wc -l` | `2` | P0 | |
| R5 | Playbooks stay internally consistent — inventory, references, shellcheck, every playbook test | `make check-playbooks` | exit 0 | P0 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| R7 | The memleak lane is green and carries no new suppression (§4) | `make memleak` and `git diff origin/main...HEAD -- make/bench.mk \| wc -l` | exit 0, and `0` | P0 | |
| R8 | The coverage lane is green with the suite instrumented (§5) | `make test-coverage-zig` | exit 0 | P0 | |
| R9 | Dependencies are at latest across the workspaces and the CLI project | `bun outdated --filter '*'` and `cd cli && bun outdated` | no rows | P1 | |
| S1 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S2 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S3 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S4 | Orphan sweep | Dead Code Sweep greps below | 0 matches | P0 | |
| S5 | No milestone identifier leaked into shipped files | `grep -rnE "M[0-9]+_[0-9]+" playbooks/ ui/packages/app/tests/e2e/ \| wc -l` | `0` | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| none — this milestone deletes a constant and a code path, not a file | `true` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `LEAKED_FLEET_PREFIXES` | `grep -rn "LEAKED_FLEET_PREFIXES" ui/ \| head` | 0 matches — **verified 0** |
| `JOURNEY_WORKSPACE_RE` | `grep -rn "JOURNEY_WORKSPACE_RE" ui/ \| head` | 0 matches — **verified 0.** Not anticipated when this spec was written: the regular expression existed only to identify workspaces safe to sweep whole, and once *every* fixture-owned workspace is swept whole its branch and its sole reference both became unreachable. Left in place it would have read as live scoping logic (RULE NDC). |

## Out of Scope

- **Pruning the 400+ fleets already in `app-dev`** — the rebuild this milestone was written alongside empties that database outright, so a migration to clean them would be dead on arrival. If a future environment accumulates them again with this sweep in place, that is a bug in §1, not a cleanup task.
- **Stale QStash schedules pointing at deleted fleets** — a database teardown removes `core.fleet_schedules` while the provider may still hold its own schedules. Real, but it belongs with the QStash registration playbook rather than here.
- **Re-encoding `scripts/seed-models.mjs`** — the stray byte that makes `grep` skip it is worth fixing, but it is a one-character change in a file this milestone only reads, and bundling it would put an unrelated edit in the blast radius.
- **The fork-pull-request approval policy and the coverage lane's host Docker access** — a repository setting and a Continuous Integration (CI) hardening item respectively, both tracked separately.
- **The production Redis dial's exposure to the Zig 0.16.0 futex use-after-scope** — all three call sites (`redis_connection.zig:245`, `redis_subscriber.zig:109`, `redis_subscriber.zig:189`) keep routing through `std.Io.net.HostName.connect`, whose `Io.Group` await carries the defect. §4 moves the *test* out of its reach, not the daemon. Closing it means the dial owning its own resolve-and-race — a new module plus three call-site swaps, which also retires the connect bound `redis_subscriber.zig:180` records as given up. Deliberately deferred per the Discovery quote; **this is unowned work with no spec, not a tracked follow-up.**
- **The out-of-range dependency majors** — `@tanstack/react-table` 9.0.0, `@assistant-ui/react` 0.15.4, and seven exactly-pinned `@radix-ui/react-*` packages are bumped in this milestone at Indy's explicit instruction ("just update the packages to the latest as well"), but they are *not* what M158 is about; any behavioural fallout they cause is triaged as its own concern rather than folded into a section here.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Indy tears down dev, runs the founding steps, and signs in to a dashboard where the model picker is already populated — without having remembered a command that appears in no playbook.
2. **Preserved user behaviour** — every existing gate invocation keeps working unchanged: the database and Redis teardown gates take the same variables and prompt the same way, and per-spec `afterEach` cleanup keeps scoping by prefix so parallel acceptance workers do not delete each other's fleets.
3. **Optimal-way check** — the most direct shape would be for the deployment itself to prime the catalogue, with no operator step at all. That is rejected for now because catalogue rows are billing rates and the repository's standing posture is that rate writes are reviewed before they are applied; an operations playbook with a diff arm preserves that review while making the step discoverable. The gap is one deliberate command, not a missing capability.
4. **Rebuild-vs-iterate** — iterate. All three slices remove or wrap existing mechanisms that work; nothing here needs redesigning. §1 is a deletion, §2 is a wrapper around a working tool, §3 is a command the playbook already asked for in prose.
5. **What we build** — a sweep that reaps by ownership; one operations playbook with four scripts and a test; one stop-writers step shared by both teardown gates; the playbook edits that make all three discoverable.
6. **What we do NOT build** — no cleanup migration for the existing fleets, no automatic priming inside the deployment pipeline, no change to `seed-models.mjs` itself, no new provider or catalogue user interface.
7. **Fit with existing features** — compounds with the founding rebuild sequence, which becomes complete rather than nearly complete. The feature it must not destabilize is the acceptance suite: §1 touches shared teardown code, so the per-spec cleanup path must keep behaving exactly as it does today.
8. **Surface order** — Command-Line Interface (CLI) first, matching the repository default. All three slices are operator surfaces reached from a shell; none has or needs a dashboard control.
9. **Dashboard restraint** — nothing is added to the dashboard. The catalogue is populated by an operator command and the existing model pages render whatever rows exist, so no control is introduced ahead of the data behind it.
10. **Confused-user next step** — an operator who reaches an empty catalogue gets a verify step that fails with the row count it found and names the playbook that fills it; an operator whose teardown is blocked gets the stop-writers step naming the application it could not scale.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one Workstream with three Sections rather than three Workstreams. They share no files, but they were all found in one read of the same rebuild path and they are individually small; splitting them would produce three Pull Requests to babysit for what is one afternoon of operator-facing hygiene.
- **Alternatives considered:** (a) extending `LEAKED_FLEET_PREFIXES` to cover all twenty-two prefixes — rejected, it preserves the rot and needs editing on every new spec; (b) priming the catalogue inside the deployment pipeline — rejected for now, it would write billing rates with no review step, and the reasoning is recorded under Product Clarity item 3; (c) leaving stop-the-writer as prose and relying on the operator — rejected, the failure it causes costs a full second teardown and is not obvious from the error.
- **Patch-vs-refactor verdict:** this is a **patch**, because each slice is a small correction to a mechanism that already exists and works. §1 is the closest to a refactor and is still a net deletion — it removes a list and a branch rather than restructuring the sweep. The one genuine refactor in the area, folding the priming into the deployment pipeline, is named above and deliberately deferred with a reason rather than mud-patched around.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`. An agent-unilateral deferral is **incomplete scope, not deferral**, and blocks CHORE(close) until the item lands or the quote is captured.

  > Indy (2026-08-05 07:1x): "Okay lets do the first row?" — context: §4. Three options were tabled for the Zig 0.16.0 `Io.Threaded` futex use-after-scope that reddens the `memleak` lane: (row 1) move the lifecycle test to the serial io — one line, lane green, daemon still carries the defect and the lane stops observing it; (row 2) the dial resolves and connects sequentially — production fixed, happy-eyeballs lost; (row 3) the dial resolves and races addresses itself — production fixed, racing kept, plus it closes the connect bound `redis_subscriber.zig:180` documents as given up. Row 1 chosen. §4 had been specced as row 3 and was rewritten; no Zig had been written, so nothing was reverted. **The production dial fix is therefore out of scope for M158 and unowned** — it needs its own spec.

- **Gate-flag triage** — the `memleak` valgrind finding is judgment-class, not mechanical, so it was surfaced rather than actioned unilaterally. `make/bench.mk:20-28` records a prior suppression attempt that was reverted with the verdict *"valgrind was right"*; no suppression was added here, and the allowlist was not widened for §5 either. Both lanes go green by making the assertions honest, not by muting them.
