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

# M126_001: Daemon survives SIGTERM at any lifecycle point, hub never blocks under its mutex, and every reviewed error path frees what it acquired

**Prototype:** v2.0.0
**Milestone:** M126
**Workstream:** 001
**Date:** Jul 11, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — three timing-dependent production defects (permanent loss of graceful shutdown, daemon-wide Server-Sent Events (SSE) stall, use-after-free during teardown) plus an unbounded slow leak in a forever-retrying sweeper. Customer-visible as stuck deploys, 429 storms, and shutdown crashes.
**Categories:** API
**Batch:** B1 — runs alone; M126_002 and M126_003 both depend on this branch (shared files, vendored tripwire).
**Branch:** `feat/m126-001-shutdown-race-leak-fixes`
**Test Baseline:** unit=2500 integration=299 — recorded at CHORE(open), Jul 11, 2026, via `make _lint_zig_test_depth` on `feat/m126-001-shutdown-race-leak-fixes` @ `35dc828e1`.
**Depends on:** none.
**Provenance:** agent-generated (pre-spec, `docs/v2/reviews/m126-ghostty-adversarial-review.md` — five-agent adversarial review, Jul 11, 2026; Indy directed "all findings fixed").
**Canonical architecture:** `docs/architecture/` carries no concurrency doc yet — M126_002 §3 creates it; this workstream's shutdown-ordering decisions feed that doc.

---

## Overview

**Goal (testable):** A SIGTERM delivered at any point after signal-handler install — including before the server is published — stops the daemon cleanly with every background thread joined before any shared aggregate is deinitialized; all 13 findings in the review record (R1–R10, L1–L3) have named regression tests that fail on `main` and pass on this branch; `make memleak` stays green.
**Problem:** The adversarial review (`docs/v2/reviews/m126-ghostty-adversarial-review.md` §1–§2) confirmed three P1 defects — a boot-window SIGTERM permanently disables graceful shutdown while the server keeps serving (R1); the subscription hub holds its mutex across an unbounded blocking Transport Layer Security (TLS) write, stalling every SSE operation daemon-wide behind one slow Redis peer (R2); a detached install worker uses the Postgres pool and Redis queue after teardown frees them (R3) — plus seven P2 races and three leaks, one of which (L1) accumulates per sweep interval forever whenever Postgres flaps.
**Solution summary:** Apply ghostty's shutdown discipline — stop-signal → join → deinit, never free shared state while a thread that touches it can still run — to the watcher, install worker, hub, and streaming teardown; move hub wire writes outside the mutex with a bounded send timeout; repair the errdefer ladder and decoder ownership in the sweeper and outbound decoder; single-flight the broker mint; vendor a ghostty-style tripwire fault-injection module so every fixed error path is proven leak-free by looping all fail points under `std.testing.allocator`.

## PR Intent & comprehension handshake

- **PR title (eventual):** fix(daemon): shutdown ordering, hub lock discipline, errdefer ladders (M126_001)
- **Intent (one sentence):** Operators get a daemon that always honors SIGTERM, never stalls all event streams behind one slow Redis peer, and never leaks or crashes on its error and teardown paths.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/v2/reviews/m126-ghostty-adversarial-review.md` — the findings record; §1–§2 carry every file:line and failure interleaving this spec fixes; §5 carries the ghostty patterns to mirror.
2. `~/Projects/oss/ghostty/src/tripwire.zig` — the module to port in §4: comptime-erased fail points, `errorAlways`/`errorAfter`, `end(.reset)`; usage exemplars in `~/Projects/oss/ghostty/src/terminal/PageList.zig` (test "PageList init error") and `Tabstops.zig` (state-rollback assertion).
3. `~/Projects/oss/ghostty/src/termio/mailbox.zig` (`send`, lines 61-93) — the unlock-push-relock shape for §2: instant-try under the lock, fall back to unlock → blocking send → relock, lock state as an explicit parameter.
4. `src/agentsfleetd/fleet/reclaim_sweeper.zig` — the in-repo correct errdefer shape (`errdefer { freeIdItems; ids.deinit(alloc); }`) that `liveness_sweeper.zig` must match in §3.
5. `src/agentsfleetd/auth/jwks.zig` — the in-repo single-flight pattern (concurrent cold verifies → exactly one fetch) that `credentials/broker.zig` mirrors in §5.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/cmd/serve.zig` | EDIT | Shutdown choreography: watcher lifetime, streaming teardown ordering (R1, R7) |
| `src/agentsfleetd/cmd/serve_shutdown.zig` | EDIT | Watcher loop semantics — survives pre-publish signal, stops a later-published server (R1) |
| `src/agentsfleetd/cmd/serve_background.zig` | EDIT | Background threads keep running until a real shutdown, not a boot-window flag blip (R1) |
| `src/agentsfleetd/events/subscription_hub.zig` | EDIT | Wire writes moved outside the mutex; stop() teardown under lock / after confirmed drain (R2, R4, R5) |
| `src/agentsfleetd/queue/redis_subscriber.zig` | EDIT | Bounded send timeout on the subscriber socket (R2) |
| `src/agentsfleetd/http/handlers/fleets/create_install_steps.zig` | EDIT | Install worker lifetime bounded by teardown (R3) |
| `src/agentsfleetd/http/handlers/common.zig` | EDIT | Worker tracking set in production, not only in the test harness; stale comment corrected (R3) |
| `src/agentsfleetd/fleet/liveness_sweeper.zig` | EDIT | errdefer ladder: backing list freed on error; struct-literal unwind (L1, L2) |
| `src/agentsfleetd/queue/connector_outbound.zig` | EDIT | Duplicate-key dupe freed before overwrite (L3) |
| `src/lib/logging/sinks.zig` | EDIT | Drain tickets count only pre-removal emits (R6) |
| `src/agentsfleetd/credentials/broker.zig` | EDIT | Single-flight cold-miss mint (R8) |
| `src/agentsfleetd/observability/otlp/exporter.zig` | EDIT | Idempotent/guarded install (R9) |
| `src/agentsfleetd/observability/metrics_runner.zig` | EDIT | Explicit spin-exhaustion handling — no duplicate slot (R10) |
| `src/lib/tripwire/tripwire.zig` | CREATE | Vendored fault-injection module (§4), named module for both build graphs (directory layout per src/lib convention) |
| `src/lib/tests.zig` | EDIT | Wire tripwire self-tests into the lib test root (RULE TST) |
| `src/build/shared.zig` | EDIT | tripwire module constructed in SharedDeps (named-module wiring) |
| `build.zig` | EDIT | tripwire import on the daemon exe + test graphs |
| `build_runner.zig` | EDIT | tripwire import on the runner graphs (lands with the base-adherence fail points) |
| `src/agentsfleetd/cmd/serve_shutdown_test.zig` | CREATE | R1 regression: boot-window SIGTERM lifecycle test |
| `src/agentsfleetd/events/subscription_hub_test.zig` | EDIT | R2/R4/R5 regressions: stalled peer, undrained stop, concurrent read+write |
| `src/agentsfleetd/http/handlers/fleets/create_install_steps_test.zig` | CREATE | R3 regression: production configuration vs teardown |
| `src/agentsfleetd/fleet/liveness_sweeper_integration_test.zig` | EDIT | L1/L2 regressions: loop-all-failpoints leak proofs (the fetch ladder needs real rows; L3's dup-key test is in-file in connector_outbound.zig — its decode fns are private) |
| `src/lib/logging/sinks_test.zig` | EDIT | R6 regression: unregister under concurrent emits (real threads) |
| `src/agentsfleetd/credentials/broker_test.zig` | EDIT | R8 regression: concurrent cold misses → exactly one mint |
| `src/agentsfleetd/observability/otlp/exporter_test.zig` | EDIT | R9 regression: double install → one thread, no leaked handle |
| `src/agentsfleetd/observability/metrics_runner_test.zig` | EDIT | R10 regression: no duplicate slot under contention |
| `src/agentsfleetd/tests.zig` | EDIT | New test files reachable from the daemon test root (RULE TST) |

Test-file names above follow the repo's colocated `*_test.zig` convention; where an
equivalent test file already exists under a different name, extend it instead of creating
a duplicate (the table pins roles, not final basenames).

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — ZIG (init/deinit/ownership conventions), OWN (one owner per resource — the install-worker and hub-teardown fixes are ownership clarifications), ZAL (Zig 0.15 ArrayList API for the L1 fix), ESO (error returns must not silently substitute defaults on OOM), FLS (drain all results), OBS (each new observable state gets a log entry), NDC (no dead code — remove the stale "harmless" comment, don't neutralize it), NLR (touch-it-fix-it applies inside edited functions only), UFS (new literals become named constants — send-timeout value, spin policy), TST (new test files imported from a test root), TST-NAM (test identifiers milestone-free).
- `dispatch/write_zig.md` — full Zig discipline fires on every edited file: errdefer placement, tagged-union results, file ≤350 / fn ≤50 lines, cross-compile both linux targets.
- `docs/LOGGING_STANDARD.md` — the new observable states (slow-path wire write, watcher re-arm, sink drain timeout) log per standard.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — every file is Zig | cross-compile `x86_64-linux` + `aarch64-linux`; lifecycle/errdefer discipline per dispatch façade |
| PUB / Struct-Shape | yes — `src/lib/tripwire.zig` is a new pub surface | FILE SHAPE DECISION at PLAN; mirror ghostty's comptime-module shape (namespace with comptime factory, no instance state) |
| File & Function Length (≤350/≤50/≤70) | yes | `subscription_hub.zig` and `serve.zig` are near-cap candidates: split helpers out rather than growing; tripwire stays one file (~290 lines) |
| UFS (repeated/semantic literals) | yes | send-timeout, drain-wait, and spin-cap values are named constants at module top |
| UI Substitution / DESIGN TOKEN | no — no UI files | — |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | LOGGING + LIFECYCLE yes; ERROR REGISTRY only if a new `UZ-` code is minted; SCHEMA no | new log lines per `docs/LOGGING_STANDARD.md`; init/deinit pairs audited (`audits/deinit-pairs.sh`) |

## Prior-Art / Reference Implementations

- **Reference:** `~/Projects/oss/ghostty/src/` — shutdown ordering (`Surface.zig:772-798` stop→join→deinit), lock-release-before-blocking-send (`termio/mailbox.zig:61-93`), tripwire (`tripwire.zig` + `PageList.zig` loop-all-failpoints tests). Divergence: our hub is many-producer (request threads) rather than ghostty's Single-Producer Single-Consumer (SPSC) mailboxes — we fix the lock discipline here and leave channel-shape alignment to M126_002's conventions.
- **In-repo:** `fleet/reclaim_sweeper.zig` (correct errdefer shape), `auth/jwks.zig` (single-flight), `events/bus.zig` (stop ordering with mutex-ordered store, tested 200×).

## Sections (implementation slices)

### §1 — Shutdown ordering: stop → join → deinit, everywhere

The daemon's teardown must never free an aggregate while a thread that touches it can still
run, and its shutdown trigger must work at every lifecycle point. **Implementation default:**
the watcher waits for a published server rather than exiting on a pre-publish flag, and the
install worker is tracked (the existing WaitGroup, set in production) and awaited before pool/
queue deinit — because both mirror proven in-repo shapes (`events/bus.zig` stop ordering; the
test harness already sets the WaitGroup) rather than inventing new machinery.

- **Dimension 1.1** — SIGTERM delivered between signal-handler install and server publish still stops the daemon: background loops keep running until a real shutdown; the watcher never exits without having stopped a live server (R1) → Test `test_sigterm_before_publish_stops_server` (+ disarm-retires-watcher teardown test) — DONE
- **Dimension 1.2** — the install-progression worker cannot outlive the pool/queue it uses: production sets the tracking WaitGroup (serve.zig owns it; `defer install_wg.wait()` unwinds before the pool/queue defers); the stale "harmless" comment is corrected (R3) → code DONE; the lifecycle proof is `test_install_worker_lifecycle_vs_teardown` (M126_003 Dimension 5.3, same PR)
- **Dimension 1.3** — `hub.stop()` never touches `conn` outside the mutex; the undrained-timeout path defers connection teardown until stragglers are confirmed gone instead of deiniting under a live racer (R4) → Test `test_hub_stop_undrained_no_conn_teardown_race`
- **Dimension 1.4** — `deinitStreaming` frees hub/registry storage only after every registered stream thread has exited; a straggler past the bounded wait blocks the free, is logged, and the wait re-arms (R7) → Test `streaming_teardown_outlasts_straggler` (awaitEmptyRounds re-arm proof) — DONE

### §2 — Hub wire-write discipline: no blocking socket write under the mutex

One slow Redis peer must cost at most one subscriber's latency, never the daemon. This slice
applies ghostty's rule: never block while holding a lock the consumer needs.
**Implementation default:** instant-try under the lock, else release the mutex for the
blocking send and re-acquire (mailbox.zig shape), plus a bounded `SO_SNDTIMEO` on the
subscriber socket so even the unlocked write cannot hang a stream thread forever — the
timeout constant lives beside the existing `SO_RCVTIMEO`.

- **Dimension 2.1** — `attach`/`unsubscribe` complete their Redis wire writes without holding the hub mutex; `dispatch`, other subscribes, `channelCount`, and `stop` proceed while one peer stalls (R2) → Test `test_attach_with_stalled_peer_does_not_block_dispatch`
- **Dimension 2.2** — the subscriber socket carries a bounded send timeout; a write to a non-reading peer errors within the bound and routes through the existing reconnect path (R2) → Test `test_send_timeout_errors_within_bound`
- **Dimension 2.3** — the single TLS connection's write half is serialized by a dedicated writer lock (distinct from the map mutex) and the read half stays reader-thread-confined; the split is documented on the struct per the lock-invariant convention (R5) → Test `test_concurrent_read_write_one_subscriber_conn`

### §3 — Errdefer ladder and decoder ownership (L1, L2, L3)

Error paths free exactly what they acquired. **Implementation default:** match
`reclaim_sweeper.zig`'s compound errdefer for the list, and `connector_outbound.zig:194`'s
documented per-local pattern for struct-literal fields — both already canonical in-repo.

- **Dimension 3.1** — `fetchDueRunners` frees both the duped items and the ArrayList backing buffer on every error path (L1) → Test `test_fetch_due_runners_error_paths_leak_free` — DONE
- **Dimension 3.2** — a row-shape failure after the `.id` dupe orphans nothing: fields land in errdefer-covered locals before the append (L2) → Test `test_fetch_due_runners_partial_row_no_orphan` — DONE
- **Dimension 3.3** — `parseJobFields` frees the earlier dupe when a duplicate key overwrites it, for every owned field (L3) → Test in-file: "parseJobFields frees the earlier dupe on a repeated key (last wins)" + OOM sweep — DONE

### §4 — Tripwire fault injection, vendored

Port ghostty's `tripwire.zig` into `src/lib/` so error-path proofs stop depending on
allocation-failure order and start naming their fail points. Zero production cost is a hard
requirement (comptime `enabled = builtin.is_test`, inline call convention when disabled).

- **Dimension 4.1** — the module self-tests: armed-and-untripped expectations fail the test; `errorAfter` counts; `end(.reset)` clears state → in-file tripwire tests, wired via the lib test root — DONE
- **Dimension 4.2** — §3's fixes carry named fail points, and their tests loop every fail point injecting `error.OutOfMemory` under `std.testing.allocator`, asserting the error surfaces AND observable state rolled back → covered by `test_fetch_due_runners_error_paths_leak_free` (the loop-all-failpoints body) — DONE

### §5 — Latent race hardening (R6, R8, R9, R10)

- **Dimension 5.1** — sink unregister waits only on pre-removal in-flight emits (generation or ticket-epoch scheme); a pre-removal emit still running blocks the free (R6) → Test `test_sink_unregister_waits_for_inflight_emit`
- **Dimension 5.2** — concurrent cold misses for one broker key mint exactly once (single-flight, `jwks.zig` shape); losers wait for the winner's value (R8) → Test `test_broker_cold_miss_single_mint`
- **Dimension 5.3** — concurrent/double `exporter.install` spawns exactly one flush thread and leaks no handle (R9) → Test `test_exporter_double_install_one_thread`
- **Dimension 5.4** — slot resolution under contention never yields a duplicate slot: spin exhaustion becomes an explicit error or a documented saturation policy, not a fall-through (R10) → Test `test_metrics_runner_no_duplicate_slot_under_contention`

## Interfaces

```
No HTTP endpoint, wire shape, or CLI change. Internal surfaces:

src/lib/tripwire.zig (new pub surface, mirrors ghostty):
  pub fn module(comptime FailPoint: type, comptime Error: type) type
    → struct { check(point) Error!void; errorAlways(point, err); errorAfter(point, err, min);
               end(mode: enum { reset }) error{Expectation}!void }
  Comptime-erased outside test builds.

subscription_hub public API: unchanged signatures; new documented invariant — no wire write
while holding the channel-map mutex; write half serialized by a writer lock.

handlers/common.zig install-worker tracking: the WaitGroup field becomes production-set;
its doc comment states teardown ordering (await before pool/queue deinit).
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| SIGTERM in boot window | signal lands before server publish | daemon completes boot, then stops cleanly; exit 0; shutdown log line — never a half-dead server |
| SIGTERM mid-install | worker sleeping between step publishes | teardown awaits the worker; worker finishes or observes shutdown and exits; no touch of freed pool |
| Redis peer stops reading | partition / provider stall | send errors within the bounded timeout; only that subscriber reconnects; dispatch and other subscribers unaffected |
| Drain timeout with wedged stream thread | stream thread stuck in client write | teardown logs and re-arms the wait; storage is never freed under a live thread |
| Duplicate keys in a stream entry | operator tooling / foreign writer | second value replaces the first; the first is freed; job proceeds; no leak |
| OOM at any acquisition in `fetchDueRunners` | allocator failure injected via tripwire | error surfaces; items + backing list freed; sweeper retries next interval with zero residue |
| Concurrent cold-miss mint | two requests race an empty broker cache | one provider mint; loser reuses the winner's credential; no token-family revocation |
| Double exporter install | future second caller | second call is a no-op or error; one flush thread; no leaked handle |
| Spin-cap exhaustion in slot resolve | initializer suspended past the cap | explicit error/saturation path; never a duplicate slot |

## Invariants

1. No shared aggregate is freed while a thread that touches it can still run — enforced by join/await-before-deinit in the teardown code paths, proven by the §1 lifecycle tests (real threads, `std.Thread.ResetEvent` + bounded `timedWait`, no sleeps).
2. No blocking wire write while holding the hub channel-map mutex — enforced by the code shape (write outside lock) and the stalled-peer test, which bounds `dispatch` latency while a peer stalls.
3. `tripwire` compiles to nothing outside test builds — enforced at comptime (`builtin.is_test` gate); the disabled path uses the inline call convention.
4. Every multi-step acquisition in the edited functions unwinds completely on every fail point — proven by loop-all-failpoints tests under `std.testing.allocator` (leak = test failure).
5. Regression tests are milestone-free in their identifiers (RULE TST-NAM) and reachable from a test root (RULE TST) — enforced by `make _lint_zig_test_depth` reachability.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `hub_wire_write_slowpath` (log) | ops | a hub wire write takes the unlocked slow path or times out | channel name, duration, outcome | no payload contents | `test_attach_with_stalled_peer_does_not_block_dispatch` |
| `shutdown_straggler_wait` (log) | ops | streaming teardown re-arms past the bounded wait | stream count remaining | no tenant data | `test_streaming_teardown_outlasts_straggler` |
| product analytics | not applicable | — | — | — | no product signal changes; operational logs only, per RULE OBS |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_sigterm_before_publish_stops_server` | shutdown flag raised pre-publish → server still stops post-publish; background threads join; process would exit 0 |
| 1.2 | integration | `test_install_worker_awaited_before_pool_deinit` | worker in-flight at teardown → teardown blocks until worker done; no post-deinit pool touch (run under `testing.allocator`) |
| 1.3 | unit | `test_hub_stop_undrained_no_conn_teardown_race` | one wedged channel at stop → conn teardown deferred; concurrent unsubscribe completes safely |
| 1.4 | unit | `test_streaming_teardown_outlasts_straggler` | registered straggler past bounded wait → storage not freed; wait re-arms; log emitted |
| 2.1 | unit | `test_attach_with_stalled_peer_does_not_block_dispatch` | writer blocked on full socket (stub peer stops reading) → dispatch and second subscribe complete within bound |
| 2.2 | integration | `test_send_timeout_errors_within_bound` | non-reading peer → send errors ≤ the named timeout constant; reconnect path entered |
| 2.3 | unit | `test_concurrent_read_write_one_subscriber_conn` | reader thread + two writer threads over one conn → no interleaved wire corruption; writer lock serializes |
| 3.1 | unit | `test_fetch_due_runners_error_paths_leak_free` | for every tripwire fail point: inject OOM → error surfaces; `testing.allocator` reports zero leaks |
| 3.2 | unit | `test_fetch_due_runners_partial_row_no_orphan` | row-shape failure after id dupe → zero leaks; refs state unchanged |
| 3.3 | unit | `test_parse_job_fields_duplicate_key_frees_prior` | entry with two `provider` fields → parse succeeds with last value; zero leaks |
| 4.1 | unit | `test_tripwire_arm_fire_reset_semantics` | armed + untripped → `end(.reset)` errors; `errorAfter(n)` fires on the nth pass exactly |
| 5.1 | unit | `test_sink_unregister_waits_for_inflight_emit` | real threads: emit in flight at unregister → free deferred until that emit completes |
| 5.2 | unit | `test_broker_cold_miss_single_mint` | N threads race one cold key → mint counter == 1; all N get the credential |
| 5.3 | unit | `test_exporter_double_install_one_thread` | two installs → one flush thread observed; uninstall joins it; zero leaks |
| 5.4 | unit | `test_metrics_runner_no_duplicate_slot_under_contention` | contended resolve with stalled initializer → distinct slots or explicit saturation error; never duplicates |
| regression | integration | existing SSE/hub/sweeper integration suites | pre-existing behavior unchanged: `make test-integration` stays green |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Every review finding R1–R10/L1–L3 maps to a named test above and all pass (§1–§5) | `make test` | exit 0 | P0 | |
| R2 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| R3 | Boot-window SIGTERM lifecycle proven with real threads (§1) | `make test-integration` | exit 0 | P0 | |
| S1 | Unit tests pass | `make test` | exit 0 | P0 | |
| S2 | Lint clean | `make lint` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S5 | No leaks | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile (Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S9 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

N/A — no files deleted; the stale "harmless" comment in `handlers/common.zig` is corrected in place, and the orphan sweep in S9 confirms no symbol was removed with references left behind.

## Out of Scope

- Rule codification, deterministic lint checks, architecture doc, and the lifecycle-spine retrofit → `M126_002`.
- Memleak-gate lanes, General Purpose Allocator (GPA) verdict enforcement, allocator injectability, drain-lint tightening, and the five deterministic lifecycle test suites beyond this spec's regressions → `M126_003`.
- Crash capture (breakpad-style local-only transport) — deliberately parked; future milestone per Indy's direction.
- Reshaping the hub into SPSC mailboxes — M126_002 records the convention; a channel-shape refactor is a separate decision.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator sends SIGTERM during a deploy that raced boot, and the daemon exits 0 with a clean shutdown log instead of hanging until SIGKILL with orphaned leases.
2. **Preserved user behaviour** — every endpoint, SSE stream semantics, CLI verb, and wire shape is unchanged; only timing/teardown behavior and error-path memory handling improve.
3. **Optimal-way check** — fixes apply proven in-repo and ghostty shapes at the defect sites; the unconstrained-optimal (full SPSC channel redesign) is deferred to the conventions workstream because these defects are fixable without it.
4. **Rebuild-vs-iterate** — iterate: 13 targeted fixes framed by one ordering rule; a teardown rewrite would trade determinism for elegance mid-P1.
5. **What we build** — the fixes, the vendored tripwire module, and one named regression test per finding.
6. **What we do NOT build** — ThreadSanitizer lanes (neither we nor ghostty run one; structural fixes + deterministic tests instead); crash capture; channel refactor.
7. **Fit with existing features** — compounds with the SSE streaming stack and sweeper reliability; must not destabilize the happy-path SSE ring/registry choreography verified clean in the review.
8. **Surface order** — N/A — no user surface; daemon-internal.
9. **Dashboard restraint** — N/A — no user surface.
10. **Confused-user next step** — `journalctl` shutdown log lines name the choreography stage (`shutdown_straggler_wait`, `hub_wire_write_slowpath`), so a stuck-shutdown report is self-diagnosable.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream for all 13 findings, sectioned by root cause (shutdown ordering / lock discipline / errdefer ladder / injection harness / latent races) — the findings share files and one framing rule, and a single PR keeps the regression-test suite reviewable as a set.
- **Alternatives considered:** per-subsystem PRs (rejected: 13 micro-PRs churn reviewers and serialize worktrees); folding fixes into the M126_002 retrofit (rejected: P1 correctness must not wait on P2 codification).
- **Patch-vs-refactor verdict:** this is a **patch** because every fix lands at a defect site using an existing proven shape; the structural follow-up (conventions, gates, coverage) is exactly M126_002/M126_003 rather than silent mud-patching.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
  - Fold decision — > Indy (2026-07-11 ~12:26): "go, i chore open pull those M126_002,M126_003 into your worktree of M126_001 and commit in your PR." — all three workstreams execute on this branch, one PR.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`. An agent-unilateral deferral is **incomplete scope, not deferral**, and blocks CHORE(close) until the item lands or the quote is captured.
