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

# M126_003: The leak gate sees every suite and every allocator, the drain lint checks the pair it exists for, and the five untested race surfaces get deterministic lifecycle tests

**Prototype:** v2.0.0
**Milestone:** M126
**Workstream:** 003
**Date:** Jul 11, 2026
**Status:** IN_PROGRESS
**Priority:** P2 — test and gate infrastructure; closes the blind spots that let the M126_001 defect classes pass every existing gate.
**Categories:** API, INFRA
**Batch:** B2 — after M126_001 merges (its fixes make these tests passable; tripwire is available). May run parallel with M126_002 (coordinate the shared `lint-zig.py`/`make/quality.mk` touch at PLAN).
**Branch:** `feat/m126-001-shutdown-race-leak-fixes` — folded into the M126_001 worktree/PR per Indy (Jul 11, 2026)
**Test Baseline:** unit=2500 integration=299 — recorded at CHORE(open), Jul 11, 2026, via `make _lint_zig_test_depth` on `feat/m126-001-shutdown-race-leak-fixes` @ `35dc828e1`.
**Depends on:** M126_001 (fixed shutdown/hub/install-worker behavior is what the new lifecycle tests assert; tripwire module).
**Provenance:** agent-generated (pre-spec, `docs/v2/reviews/m126-ghostty-adversarial-review.md` §4 — coverage assessment; Indy directed improving concurrency-race, memleak, unit, and integration coverage).
**Canonical architecture:** `docs/architecture/concurrency.md` (created in M126_002) — the lifecycle tests assert the choreography that doc records; if 002 has not merged yet, the review record §5 is the interim reference.

---

## Overview

**Goal (testable):** `make memleak` exercises the daemon, runner, and lib suites (valgrind on Linux) and fails on any General Purpose Allocator (GPA) leak verdict; the three `page_allocator` singletons are leak-checkable under `std.testing.allocator`; the drain lint fails on a `PgQuery.from` without its `defer q.deinit()`; the five untested race surfaces from the review each have a deterministic real-thread lifecycle test (bounded event waits, zero sleeps); and Resident Set Size (RSS) growth probes bound the process-level layer no in-process oracle sees — with a positive unit and integration test delta over the CHORE(open) baseline.
**Problem:** The review's coverage assessment (`docs/v2/reviews/m126-ghostty-adversarial-review.md` §4) found the leak gate valgrinds only the daemon suite; leak verdicts from both long-lived GPAs are printed and discarded; three `page_allocator` singletons are invisible to every detector; the drain lint passes on the mere presence of `PgQuery.from(`; and the shutdown choreography, hub reconnect races, production install-worker configuration, OpenTelemetry Protocol (OTLP) exporter thread lifecycle, and sink unregister have zero tests — so a regression in any of them ships green today.
**Solution summary:** Extend the memleak lane to all three test graphs; turn the discarded GPA verdicts into hard failures; inject allocators into the three singletons; make the drain lint verify the wrap/deinit pair with a seeded-violation fixture; add one deterministic lifecycle test suite per untested surface, including a full boot → SIGTERM → drain daemon test that runs inside the valgrind lane; add allocation-failure injection over the security-critical `crypto_store.load/store`; and add Resident Set Size (RSS) growth probes — Bun's process-level leak layer (baseline → workload ×N → assert bounded growth), the only detector that sees `c_allocator`/`page_allocator`/native-library (openssl, sqlite, wasm3) growth on macOS where valgrind cannot run.

## PR Intent & comprehension handshake

- **PR title (eventual):** test(gates): full-suite memleak lane, GPA verdict enforcement, deterministic lifecycle coverage (M126_003)
- **Intent (one sentence):** A leak or lifecycle race anywhere in the daemon, runner, or lib — including the paths that pass every gate today — fails a deterministic local gate before it can ship.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/v2/reviews/m126-ghostty-adversarial-review.md` §4 — the coverage blind spots this spec closes, with file:line cites; §5 item 10 — the ghostty deterministic-test pattern to mirror.
2. `make/bench.mk` (the `memleak` target: `_ensure-test-bin`, valgrind flags, macOS lane) and `build_runner.zig` — how test binaries are built per graph; the runner/lib lanes replicate the daemon lane's shape.
3. `~/Projects/oss/ghostty/src/terminal/search/Thread.zig` (test at lines 848-905) — the canonical deterministic real-thread lifecycle test: spawn → mailbox push → `ResetEvent.timedWait` (bounded) → stop → join → assert; zero sleeps, zero polling.
4. `src/runner/worker_pool_integration_test.zig` — the in-repo real-thread + real-fork harness shape (stub control plane, overlap witness, clean drain join) the new lifecycle tests match in style.
5. `lint-zig.py` — the drain check to tighten; keep its output shape (file:line, nonzero exit) so `make lint` semantics are unchanged.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `make/bench.mk` | EDIT | memleak lane iterates daemon + runner + lib test binaries; boot→SIGTERM→drain test included in the valgrind run |
| `build.zig` / `build_runner.zig` | EDIT | test-bin targets per graph as needed by the new lanes (extend existing steps; no new build surface beyond that) |
| `src/agentsfleetd/main.zig` | EDIT | GPA deinit verdict checked — `.leak` fails (nonzero exit in debug/test-facing builds) instead of being discarded |
| `src/runner/daemon/worker_pool.zig` | EDIT | per-worker GPA verdict surfaced and failed, same policy |
| `src/agentsfleetd/state/model_rate_cache.zig` | EDIT | allocator injected (production default unchanged); rebuild/swap leak-checkable |
| `src/agentsfleetd/fleet_runtime/approval_gate_db.zig` | EDIT | allocator injected for the resolveGateDecision sink |
| `src/agentsfleetd/db/pool.zig` | EDIT | allocator injected for the page_allocator site |
| `lint-zig.py` | EDIT | drain check requires the `defer <q>.deinit()` pair per `PgQuery.from(` site |
| `src/agentsfleetd/cmd/serve_lifecycle_integration_test.zig` | CREATE | boot → SIGTERM → drain full-daemon test; runs in the valgrind lane |
| `src/agentsfleetd/cmd/serve_shutdown.zig` | EDIT | `publishedEvent()` test seam (~6 lines, Event set in `publishServer`, re-armed by `reset()`) — the deterministic boot witness the 5.1 test waits on before raising SIGTERM |
| `src/agentsfleetd/events/subscription_hub_test.zig` | EDIT | 5.2 satisfied by the existing deterministic hub suite (001, folded into this PR); 7.2's churn soak extends this file — the planned `subscription_hub_lifecycle_test.zig` CREATE is obsoleted by the de-dup rule |
| `src/agentsfleetd/http/handlers/fleets/create_install_steps_lifecycle_test.zig` | CREATE | production (null-WaitGroup-history) configuration vs teardown — asserts 001's tracked-worker behavior |
| `src/agentsfleetd/observability/otlp/exporter_test.zig` | EDIT | 5.4: drain-empties-ring leg added to the existing lifecycle suite (planned `exporter_lifecycle_test.zig` CREATE obsoleted — de-dup rule) |
| `src/lib/logging/sinks_test.zig` | EDIT | 5.5: soak-shaped unregister-under-N-emitters added to the existing suite (planned `sinks_lifecycle_test.zig` CREATE obsoleted — de-dup rule) |
| `src/agentsfleetd/secrets/crypto_store_test.zig` | EDIT | `checkAllAllocationFailures` over load/store paths |
| `src/agentsfleetd/fleet/liveness_sweeper_integration_test.zig` | EDIT | concurrent-sweep variant moves off `page_allocator` onto `testing.allocator`; mid-query failure injection via tripwire |
| `src/lib/common/rss.zig` | CREATE | Cross-platform RSS reader (`currentBytes()`: Linux `/proc/self/statm`, macOS `task_info`) — the soak probes' measurement seam |
| `src/agentsfleetd/tests.zig`, `src/runner/tests.zig`, `src/lib/tests.zig` | EDIT | new test files reachable from their test roots (RULE TST) |
| `src/agentsfleetd/db/test_fixtures.zig` | EDIT | `seedFleetWithStatus` + `fleetStatusOwned` — fleet seed/read SQL centralized out of test bodies (Indy's direction) |
| `make/test-integration.mk` | EDIT | `TEST_DATABASE_URL_LOCAL` gains `?sslmode=disable` — local docker pg has no TLS; without it every local DB-lane test failed at connect (found while validating 5.3) |

Test-file basenames follow the repo's colocated `*_test.zig` convention; extend an existing
file instead of creating a near-duplicate where one already covers the surface.

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — TST (every new test file imported from a test root — the reachability gate is the backstop), TST-NAM (milestone-free test identifiers), NDC (no dead fixtures), UFS (timeouts/thread counts in the new tests are named constants), OBS (GPA-verdict failure logs its verdict before exiting), ESO (allocator injection must not introduce silent defaults on OOM), ORP (build-target edits sweep for orphaned step names), NLR (touch-it-fix-it inside edited functions).
- `dispatch/write_zig.md` — fires on every Zig edit; if M126_002 has merged, rules A1–A6/C1–C5 bind these edits and the new tests demonstrate A6's loop-all-failpoints shape.
- `docs/VERIFY_TIERS.md` — this spec **changes** it: the memleak tier description gains the runner/lib lanes; update the doc in the same PR.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — Zig files edited/created | cross-compile both linux targets; new tests compile under both |
| PUB / Struct-Shape | yes — allocator-injection changes three init signatures | FILE SHAPE DECISION at PLAN per touched struct; production call sites updated in the same diff |
| File & Function Length (≤350/≤50/≤70) | yes | lifecycle tests split by surface (one file each, listed above) to stay under caps |
| UFS (repeated/semantic literals) | yes | bounded-wait durations, worker counts, valgrind lane names → named constants |
| UI Substitution / DESIGN TOKEN | no — no UI files | — |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | LOGGING yes (GPA verdict log line); LIFECYCLE yes (injection touches init/deinit pairs — `audits/deinit-pairs.sh` green); ERROR REGISTRY / SCHEMA no | per standards |

## Prior-Art / Reference Implementations

- **Reference:** `~/Projects/oss/ghostty/` — the deterministic lifecycle-test pattern (`terminal/search/Thread.zig:848-905`: ResetEvent + bounded timedWait over the exact production stop→join sequence) and the valgrind build-step shape (`build.zig:40-51,235-250`: baseline-CPU rebuild, documented suppressions). Divergence: ghostty gates valgrind in Continuous Integration (CI) via its test workflow; we gate via `make memleak` locally and leave CI wiring out of scope (workflow edits need explicit approval).
- **In-repo:** `make/bench.mk` daemon memleak lane (the shape to replicate per graph); `worker_pool_integration_test.zig` (real-thread harness style); `connector_outbound.zig:289-293` (`checkAllAllocationFailures` usage to mirror for crypto_store).

## Sections (implementation slices)

### §1 — Memleak gate covers every suite

`make memleak` builds and runs all three test graphs (daemon, runner, lib) under valgrind on
Linux with the existing flags; macOS keeps the blocking `testing.allocator` run + advisory
`leaks` per lane. **Implementation default:** replicate the existing `_ensure-test-bin` +
valgrind invocation per graph rather than one merged binary — the graphs have different build
files and the per-lane output names the failing suite.

- **Dimension 1.1** — runner suite runs under the valgrind lane; a seeded leak in a scratch branch fails it (verified during EXECUTE, not committed) → Test: `make memleak` output names all three lanes; rubric grep
- **Dimension 1.2** — lib suite same → same verification
- **Dimension 1.3** — `docs/VERIFY_TIERS.md` memleak tier updated to name the three lanes → rubric grep

### §2 — GPA leak verdicts become failures

`agentsfleetd/main.zig:140` and `runner/daemon/worker_pool.zig:104` currently discard
`gpa.deinit()`. **Implementation default:** a shared small helper asserts the verdict — `.leak`
logs and exits nonzero (daemon) / fails the worker teardown path (pool) — active wherever the
DebugAllocator is the backing allocator, so tests and debug runs fail loudly while ReleaseSafe
production (c-allocator-free path unchanged) is unaffected.

- **Dimension 2.1** — daemon teardown with a leaked allocation exits nonzero and logs the verdict → Test `test_gpa_verdict_fails_on_leak` — DONE. Shared helper `src/lib/logging/leak_guard.zig` (`check`/`verdictToError`) — placed in the `log` module, not `common`, because a guard in `common` would close a `common → log → common` import cycle. `main.zig` verdict now runs `leak_guard.check(gpa.deinit(), "daemon") catch std.process.exit(EXIT_GPA_LEAK)` (Debug fails; ReleaseSafe logs). Tests assert `verdictToError` (pure) + `check`'s log line via a `BufferedSink` capture — a real seeded leak's own `deinit()` emits an `err` log the harness counts as a failure, so the enum verdict is passed directly.
- **Dimension 2.2** — worker pool teardown surfaces a per-worker leak as a failure → Test `test_worker_pool_gpa_verdict_fails` — DONE. Each worker stores `deinit() == .leak` into a per-worker `Pool.leak_flags` slot; `Pool.join` folds them (`foldWorkerVerdict`, pure) and routes the verdict through `leak_guard.check` (`join` now returns `LeakError!void`; callers updated). Test asserts the pure fold (no log) so the fail-on-warn runner is satisfied.

### §3 — page_allocator singletons become leak-checkable

The three sites take an injected allocator with the production default unchanged
(`page_allocator`); tests construct them on `std.testing.allocator` so rebuild/swap cycles are
leak-audited for the first time.

- **Dimension 3.1** — `model_rate_cache` repeated populate/swap under `testing.allocator` is leak-free → Test `test_rate_cache_rebuild_cycles_leak_free` — DONE. Injection is a module `backing_allocator` var + `setBackingAllocatorForTest` (NOT a `populate` parameter — the file's doc comment documents why a caller-supplied allocator was removed as a footgun; the module knob keeps that closed while staying overridable). DB-gated integration test (`model_rate_cache_integration_test.zig`) drives 20 populate/swap rounds under `testing.allocator` — **validated leak-free against live pg** (exit 0).
- **Dimension 3.2** — `approval_gate_db` decision-sink cycle leak-free → Test `test_approval_gate_sink_leak_free` — DONE. `resolveGateDecision` gains a `sink_alloc: Allocator` param (2 production callers pass `page_allocator`). Unit test asserts `ResolvedRow.deinit` frees all six owned fields with zero residue/double-free under `testing.allocator` — the exact ownership the injected sink allocator relies on (DB-free).
- **Dimension 3.3** — `db/pool` connect-string allocation leak-free under injection → Test `test_db_pool_alloc_injected_leak_free` — DONE (as a `parseUrl` unit test). Divergence: the page_allocator site (`initFromEnvForRole`) legitimately stays `page_allocator` — `pg.Pool.init` borrows the connect strings for the pool's whole life and returns no handle to free them, so a full init/deinit cannot leak-audit them (that is *why* they are process-lifetime page_allocator). `parseUrl` IS the injectable allocator seam; the unit test drives its dupe path on `testing.allocator` and frees it, proving the allocation side leak-clean.

### §4 — Drain lint verifies the pair

`lint-zig.py`'s drain check fails any function whose `PgQuery.from(` lacks a matching
`defer <binding>.deinit()`, with a seeded-violation fixture proving detection. Tree is clean
today (268/268 verified in the review) so the tightened check lands green.

- **Dimension 4.1** — seeded wrap-without-defer fixture fails the check naming file:line → Test `test_drain_lint_detects_missing_defer` (fixture-driven) — DONE (comment-stripped so a commented `defer q.deinit()` can't mask a real gap)
- **Dimension 4.2** — full tree passes the tightened check → Test: `make lint` exit 0 — DONE (the tightened check found + fixed a real latent gap: `db/pool_migration_lock.zig` `release` used a bare `result.deinit()` where its sibling `probeAvailable` used `defer`)

### §5 — Deterministic lifecycle tests for the five untested surfaces

Every test follows the ghostty pattern: real `std.Thread.spawn`, `std.Thread.ResetEvent` with
bounded `timedWait`, the exact production stop→join→deinit sequence, zero sleeps/polling. All
run under `std.testing.allocator` so each is simultaneously a leak proof.

- **Dimension 5.1** — full-daemon boot → SIGTERM → drain: server up, streams attached, sweepers running; SIGTERM; asserts joined threads, freed state, exit path — and the test executes inside the §1 valgrind lane → Test `test_daemon_boot_sigterm_drain_clean`
- **Dimension 5.2** — hub reconnect racing attach/unsubscribe/stop, plus stop-with-undrained-channel → Test `test_hub_reconnect_races_attach_unsubscribe_stop` — DONE (satisfied by M126_001's deterministic hub suite in `subscription_hub_test.zig`, folded into this PR; no new file per the de-dup rule). Each race is pinned deterministically: `hub_stop_undrained_no_conn_teardown_race` (stop with a live channel → drain-timeout → post-stop unsubscribe observes null under the wire lock, the 001 UAF fix; correctly runs the drain-warn under `testing.log_level = .err`); `integration: hub reconnects after its connection is killed and delivery resumes` (reconnect); `attach_with_stalled_peer_does_not_block_dispatch` + `integration: concurrent_read_write_one_subscriber_conn` (`WireChurn` — attach/unsubscribe racing the reader). A single combined 4-way race is deliberately NOT added: it cannot be made deterministic (zero-sleeps mandate), whereas the seam-controlled tests above prove each interaction exactly.
- **Dimension 5.3** — install worker in the production configuration racing teardown (asserts M126_001's await-before-deinit) → Test `test_install_worker_lifecycle_vs_teardown` — DONE (`create_install_steps_lifecycle_test.zig`, DB+Redis-gated integration). Drives the real detached worker on the harness pool+queue with the drain WaitGroup exactly as serve.zig passes it; `wg.wait()` — the production barrier itself — is the only synchronization (zero test sleeps), then asserts the guarded installing→active flip landed before `finish()`. Seed/read SQL centralized in `db/test_fixtures.zig` (`seedFleetWithStatus`, `fleetStatusOwned` — per Indy's SQL-placement direction, Jul 11). **Validated pass against live pg+redis.** Also fixed en route: `TEST_DATABASE_URL_LOCAL` lacked `?sslmode=disable`, so every local DB-lane test failed at connect (`SSLNotSupportedByServer`) — `make/test-integration.mk` amended (mechanical gate-triage fix, Files-Changed extended).
- **Dimension 5.4** — OTLP exporter: install spawns the flush thread, uninstall joins it, drain empties the ring, double-install stays single-threaded → Test `test_exporter_thread_lifecycle` — DONE (by EXTENDING `exporter_test.zig`, not a new file: M126_001 already added install/uninstall/join + double-install + racing-install there; only the drain-empties-ring leg was missing). New test `exporter uninstall drains the ring before the flush thread joins` uses a fake ring (collect drains + returns null → no network/warn); the uninstall join is the barrier, zero sleeps. Validated pass.
- **Dimension 5.5** — sink unregister while N emitter threads run: pre-removal emits complete before free; no lost or double-freed sink → Test `test_sinks_unregister_under_concurrent_emits` — DONE (by EXTENDING `sinks_test.zig`: M126_001's `sink_unregister_waits_for_inflight_emit` already proves the epoch-ticket drain deterministically with one gated emitter; this adds the soak-shaped variant — 8 emitters × 256 emits racing `unregisterByCtx`). testing.allocator is the leak/double-free oracle; the append-under-lock means every landed emit is a whole line, so the length-invariant assertion catches a torn concurrent append. Bounded loops + join, zero sleeps.

### §6 — Failure injection for crypto_store and the sweeper

- **Dimension 6.1** — `checkAllAllocationFailures` over `crypto_store.load` and `store`: every allocation-failure point unwinds leak-free with key material zeroed → Test `test_crypto_store_alloc_failures_leak_free` — DONE (`crypto_store_test.zig`, two tests). The SELECT/INSERT runs on the conn (pool) allocator, so only load/store's own dupe/AAD/encrypt/decrypt allocations fail through the injected allocator — each surfaces OutOfMemory with zero residue (deferred free + `secureZero` ladder + `result.deinit` drain). Key-material zeroing stays structurally guaranteed by the deferred `secureZero` on kek/dek/dek_plain (+ the existing zeroization source test). **Validated pass vs live pg.**
- **Dimension 6.2** — `liveness_sweeper` integration: concurrent-sweep variant runs on `testing.allocator` (off `page_allocator`), and a tripwire mid-query failure during a sweep leaves zero residue → Test `test_concurrent_sweep_with_midquery_failure_leak_free` — DONE. `SweepWorker` gains an injected `alloc` (concurrent variants pass `testing.allocator`, a thread-safe DebugAllocator — nothing rides `page_allocator`). New test sequences strictly: **stage 1 (main thread only)** arms `fetch_tw.errorAfter(.dupe_id, …, 1)`, asserts OOM, resets — the tripwire is unsynchronized module-global state, so it is never armed while workers run; **stage 2** N concurrent sweeps whose success + exactly-one-event dedup IS the residue oracle for stage 1 (memory residue fails `testing.allocator`; an undrained result poisoning a pooled conn would break the sweeps). **Validated pass vs live pg.**

### §7 — RSS growth probes (the Bun layer: process-level leak signal)

Adopted from Bun's memory-test model (Indy's direction, Jul 11, 2026): Bun pairs exact
in-process leak oracles with coarse **RSS-growth probes** — baseline `process.memoryUsage.rss()`,
run the workload N times, force collection, assert bounded growth. Our exact oracle
(`std.testing.allocator`) is stronger than Bun's (no garbage collector nondeterminism), but we
have NO process-level layer: growth in `c_allocator`, `page_allocator`, or native libraries
(openssl, sqlite, wasm3) is invisible to `testing.allocator`, and valgrind cannot run on the
macOS dev machines. The probe pattern: read RSS via `common.rss.currentBytes()`, run the
workload `SOAK_ITERATIONS` times, re-read, assert growth under a named per-probe bound
(generous — RSS is coarse; the probe catches unbounded growth, not byte-exact leaks).

- **Dimension 7.1** — `src/lib/common/rss.zig` reads the process RSS on Linux (`/proc/self/statm`) and macOS (`task_info`); returns null on unsupported platforms so probes skip, never false-fail → Test `test_rss_reader_returns_plausible_value` — DONE. Linux path is a raw-syscall `/proc/self/statm` read (`std.posix.openatZ`/`read`, no libc, no `io`); macOS path is mach `task_info(MACH_TASK_BASIC_INFO)` returning `resident_size` (flavor `20` pinned — `std.c.MACH_TASK_BASIC_INFO` is a redirect placeholder; libSystem is always linked on Darwin so it resolves in the libc-free lib graph too). Re-exported as `common.rss`. Verified: `resident_size` reads ~1.5 MiB live; compiles x86_64-linux + aarch64-linux + native.
- **Dimension 7.2** — two seed probes establish the pattern: `model_rate_cache` rebuild/swap soak (complements the §3 injectability with the native-side view) and hub subscribe/unsubscribe churn soak → Tests `test_rate_cache_rebuild_rss_bounded`, `test_hub_churn_rss_bounded`

## Interfaces

```
No HTTP endpoint, wire shape, or CLI change. Surfaces this spec pins:

make memleak — exit nonzero if ANY of the three lanes (agentsfleetd, runner, lib) fails;
output prefixes each lane ("→ [agentsfleetd]", "→ [runner]", "→ [lib]").

GPA verdict policy — DebugAllocator-backed teardowns fail on .leak; the log line carries the
verdict. Production ReleaseSafe allocator selection is unchanged.

Allocator injection — the three singletons' init signatures gain an allocator parameter with
the production call sites passing the previous default; no behavioral change in production.

lint-zig.py drain check — nonzero exit + file:line on a PgQuery.from without its paired
defer deinit in the same function.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Valgrind unavailable / non-Linux host | macOS dev machine | lane falls back to the existing blocking `testing.allocator` run + advisory `leaks`, exactly as the daemon lane does today |
| Lifecycle test hangs | regression in stop→join ordering | every wait is a bounded `timedWait`; expiry fails the test with the stage name — never a hung suite |
| Seeded-leak fixture escapes into the tree | fixture mismanagement | seeded leaks live only inside test bodies/fixtures exercised by the lint/test lane; S9 orphan sweep + NDC cover strays |
| Injection changes production allocator behavior | wrong default at a call site | production call sites pass the previous allocator explicitly; integration suite green is the regression net |
| Drain-lint false positive on a legitimate split | drain in a helper fn | the check scopes to same-function pairing by design (matching the rule's text); a legitimate cross-function case is a judgment flag → Indy per gate-flag triage |
| tripwire unavailable (001 not merged) | dependency ordering | blocked by Depends-on; do not vendor a second copy |

## Invariants

1. No new test sleeps or polls — every synchronization is a `ResetEvent`/join with a named bounded timeout constant; enforced by review against the pattern and by the tests' own bounded-wait failure messages.
2. The memleak gate fails if any lane fails — enforced by the make recipe's exit chaining; proven by the seeded-leak EXECUTE check.
3. New tests are reachable from a test root — enforced by `make _lint_zig_test_depth` reachability (the gate that produces the baseline numbers).
4. Unit AND integration deltas over the CHORE(open) baseline are positive — enforced at VERIFY by the Test Delta row; zero/negative delta returns this spec to EXECUTE by definition.
5. Injection sites default to prior production behavior — enforced by explicit allocator arguments at production call sites and the unchanged integration suite.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `gpa_leak_verdict` (log) | ops | a DebugAllocator teardown reports `.leak` | component (daemon/worker), verdict | no allocation contents | `test_gpa_verdict_fails_on_leak` |
| product analytics | not applicable | — | — | — | no product signal changes; gate/test infrastructure only |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1–1.2 | gate | `make memleak` lane output | three lane prefixes present; nonzero on any lane failure |
| 1.3 | audit (grep) | rubric R5 | VERIFY_TIERS names the three lanes |
| 2.1 | unit | `test_gpa_verdict_fails_on_leak` | seeded leak → helper returns/exits failure; verdict logged |
| 2.2 | unit | `test_worker_pool_gpa_verdict_fails` | seeded per-worker leak → pool teardown fails |
| 3.1 | unit | `test_rate_cache_rebuild_cycles_leak_free` | N populate/swap cycles under `testing.allocator` → zero leaks |
| 3.2 | unit | `test_approval_gate_sink_leak_free` | decision-sink write/read/clear cycle → zero leaks |
| 3.3 | unit | `test_db_pool_alloc_injected_leak_free` | pool init/deinit under injection → zero leaks |
| 4.1 | unit (fixture) | `test_drain_lint_detects_missing_defer` | seeded wrap-without-defer → lint nonzero with file:line |
| 4.2 | gate | `make lint` | exit 0 on the clean tree with the tightened check |
| 5.1 | integration | `test_daemon_boot_sigterm_drain_clean` | full boot, SIGTERM, drain → all threads joined, zero leaks; runs inside the valgrind lane |
| 5.2 | unit | `test_hub_reconnect_races_attach_unsubscribe_stop` | reconnect concurrent with attach/unsubscribe/stop → no deadlock, no use-after-free, bounded completion |
| 5.3 | integration | `test_install_worker_lifecycle_vs_teardown` | worker in flight at teardown → awaited; zero post-free touches |
| 5.4 | unit | `test_exporter_thread_lifecycle` | install/drain/uninstall/double-install → one thread ever; ring empty; zero leaks |
| 5.5 | unit | `test_sinks_unregister_under_concurrent_emits` | N emitters + unregister → pre-removal emits complete; zero leaks |
| 6.1 | unit | `test_crypto_store_alloc_failures_leak_free` | every allocation-failure point in load/store → error surfaces, zero leaks, key material zeroed |
| 6.2 | integration | `test_concurrent_sweep_with_midquery_failure_leak_free` | concurrent sweeps + tripwire mid-query failure on `testing.allocator` → zero leaks |
| 7.1 | unit | `test_rss_reader_returns_plausible_value` | currentBytes() non-null on Linux/macOS and > 1 MiB for a live test process |
| 7.2 | unit | `test_rate_cache_rebuild_rss_bounded`, `test_hub_churn_rss_bounded` | baseline RSS → SOAK_ITERATIONS workload cycles → growth < named per-probe bound |
| regression | integration | `make test-integration` | pre-existing suites unchanged and green |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Memleak gate exercises all three suites (§1) | `make memleak 2>&1 \| grep -cE '^→ \[(agentsfleetd\|runner\|lib)\]'` | ≥ 3 | P0 | |
| R2 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| R3 | Five lifecycle surfaces covered (§5) | `make test && make test-integration` | exit 0 (decisive lines: the five named tests) | P0 | |
| R4 | Drain lint catches the pair violation (§4) | fixture lane run named in the test spec | nonzero on fixture; `make lint` exit 0 on tree | P0 | |
| R5 | VERIFY_TIERS documents the new lanes (§1) | `grep -c 'runner' docs/VERIFY_TIERS.md` | ≥ 1 in the memleak tier section | P1 | |
| R6 | Positive unit + integration delta vs baseline | `make _lint_zig_test_depth` | unit > baseline AND integration > baseline from the CHORE(open) header | P0 | |
| S1 | Unit tests pass | `make test` | exit 0 | P0 | |
| S2 | Lint clean | `make lint` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S5 | No leaks | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile (Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line; long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

N/A — no symbols removed; build-target edits are checked for orphaned step names via ORP during VERIFY.

## Out of Scope

- CI workflow wiring for the new lanes (`.github/workflows/**` edits are forbidden without explicit approval) — proposed as a follow-up once the make lanes prove stable; local `make memleak` is the gate of record until then.
- ThreadSanitizer lane — deliberate non-goal per the review (structural discipline + deterministic tests, matching ghostty's evidence).
- Defect fixes (M126_001) and rule codification/retrofit (M126_002).
- Crash capture — parked, future milestone.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a contributor introduces a leak in the runner's supervisor path, runs `make memleak`, and gets a red lane naming the suite and stack — instead of shipping it green as today.
2. **Preserved user behaviour** — zero runtime behavior change in production paths; allocator injection defaults preserve current allocators; endpoints/CLI/wire untouched.
3. **Optimal-way check** — extending the existing make lanes and lint is the direct path; the unconstrained-optimal (CI-enforced valgrind + sanitizer matrix) is gated on workflow-edit approval and lane stability, named as follow-up.
4. **Rebuild-vs-iterate** — iterate: every deliverable extends an existing gate, script, or harness shape already proven in this repo.
5. **What we build** — three memleak lanes, GPA verdict enforcement, three injectable singletons, a pair-checking drain lint, five lifecycle test suites, two failure-injection suites, one docs update.
6. **What we do NOT build** — ThreadSanitizer; CI wiring (approval-gated follow-up); new test frameworks (std.Thread.ResetEvent + existing harnesses only).
7. **Fit with existing features** — compounds with M126_001 (asserts its fixes stay fixed) and M126_002 (demonstrates rule A6/C2 in practice); must not destabilize `make bench`/`make memleak` semantics existing workflows depend on.
8. **Surface order** — N/A — no user surface; developer gates.
9. **Dashboard restraint** — N/A — no user surface.
10. **Confused-user next step** — a failing lane prints the suite lane prefix and the valgrind/allocator trace; the drain lint prints file:line with the rule name; both are self-serve.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream for all coverage gates, sectioned by blind spot — the lanes, verdict policy, injectability, lint pair-check, and lifecycle suites are one reviewable idea ("nothing leak- or race-shaped ships green"), and several share files.
- **Alternatives considered:** folding into M126_001 (rejected: doubles that PR's blast radius and delays P1 fixes behind gate plumbing); one spec per blind spot (rejected: five near-trivial PRs with a shared make/lint surface invites conflicts).
- **Patch-vs-refactor verdict:** this is a **patch** set over existing gate infrastructure — each change extends a proven shape (`bench.mk` lane, `lint-zig.py` check, ghostty test pattern); no gate architecture is redesigned.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
  - Fold decision — > Indy (2026-07-11 ~12:26): "go, i chore open pull those M126_002,M126_003 into your worktree of M126_001 and commit in your PR." — all three workstreams execute on this branch, one PR.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`. An agent-unilateral deferral is **incomplete scope, not deferral**, and blocks CHORE(close) until the item lands or the quote is captured.
