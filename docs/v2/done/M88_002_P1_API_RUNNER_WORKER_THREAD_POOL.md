# M88_002: Runner worker-thread pool — N concurrent agents per host

**Prototype:** v2.0.0
**Milestone:** M88
**Workstream:** 002
**Date:** Jun 08, 2026
**Status:** DONE
**Priority:** P1 — a host runner executes exactly one agent at a time; this lifts per-host throughput to ~N concurrent without the async rewrite (M88_001), and is the boring lever the M88 pivot identified as the real fix.
**Categories:** API
**Batch:** B1 — sibling of M88_001 (gated); independent, may run concurrently.
**Branch:** feat/m88-runner-worker-pool
**Depends on:** none for correctness (the per-zombie `affinity.claim` already serializes N pollers; the runner memory plane is already lease/zombie-scoped — see Discovery). Composes with M84_002 — both specs model "busy"/capacity as **derived** from `fleet.runner_leases` (no singular `current_lease_id`), so a pooled runner holding 0..N leases needs no control-plane schema change. **Capacity-aware scheduling (`worker_count` reporting / a per-runner lease cap) is NOT wired by either spec** — see Discovery. May run concurrently in a separate worktree (file-disjoint from M84_002's `src/zombied/fleet/*` + UI).
**Provenance:** LLM-drafted (Opus 4.8, Jun 08 2026) — from the M88 scaling pivot; design LOCKED by Indy: fixed-N `std.Thread` pool, reuse `pollAndProcess`, not capacity-aware.

> **Provenance is load-bearing.** LLM-drafted — every claim below was cross-checked against the runner code on Jun 08 2026 (fork-safety, the per-zombie claim, the stateless client). Re-verify before EXECUTE.

**Canonical architecture:** `docs/architecture/scaling.md` (the per-node lever order — handler threads → bigger VM → replicas → async; this is the runner-side analog of the zombied handler-pool lever) + `docs/architecture/runner_fleet.md` (the lease / `affinity.claim` / fencing / derived-liveness model the pool relies on, unchanged).

---

## Implementing agent — read these first

1. `src/runner/daemon/loop.zig` — `runLoop` (the single-threaded heartbeat→poll→execute loop being refactored), and `pollAndProcess` + `executeAndReport` (the lease→execute→report unit each worker reuses **verbatim**), plus the `drain_requested` atomic + `installDrainHandlers` (the drain primitive the pool extends).
2. `src/runner/main.zig` — process startup, the single `DebugAllocator`, and the `loop.runLoop` call site (where the pool is spawned/joined; N=1 keeps this path behaviourally identical).
3. `src/zombied/fleet/assign.zig` + `reclaim.zig` — the atomic per-zombie `affinity.claim` ("exactly one of N racing runners wins the slot; a loser gets `.taken` and moves on") + reclaim fencing: this is why N concurrent pollers sharing one `zrn_` need **no** control-plane change.
4. `src/runner/child_supervisor.zig` + `child_process.zig` — the fork-per-lease path; `forkExec` spawns via `std.process.spawn` (`pipe → fork → dup2 → setpgid → execvpe`), whose post-fork child path is async-signal-safe — the property that makes forking from a multithreaded daemon safe.
5. `src/zombied/cmd/serve.zig` (≈40, 317–334) — the existing `shutdown_requested` atomic + `std.Thread.spawn`/`.join()` background-thread lifecycle to mirror for the pool's spawn/join.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Run N agents concurrently per host via a runner worker-thread pool
- **Intent (one sentence):** A single host runner executes up to N leased agents at once instead of one-at-a-time, raising per-host throughput without the evented substrate.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent + `ASSUMPTIONS I'M MAKING: …`. Load-bearing assumptions: (1) fixed-N from `RUNNER_WORKER_COUNT`, not capacity-aware; (2) each worker runs the existing `pollAndProcess` verbatim — heartbeat stays **one per host** on the control loop, never per worker; (3) **no control-plane change for correctness** (the per-zombie claim admits one winner); (4) each worker owns an independent allocator scope (`deinit`'d on join — the shared GPA is already thread-safe, so this removes contention, not a race); (5) `.drain`/SIGTERM finish in-flight work while `.stop` evicts (abandons) it — both take no new lease and join every worker (today's distinct semantics, preserved). A mismatch → STOP and reconcile.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC (no dead code: the loop split is in-place, nothing orphaned); UFS (`RUNNER_WORKER_COUNT` env name + default + clamp bounds, and the new `HEARTBEAT_INTERVAL_MS`, are named constants — never re-spelled literals); NLG (no "legacy" framing for the single-threaded path pre-2.0); LOGGING.
- **`dispatch/write_zig.md`** — diff is `*.zig`: tagged-union results, multi-step `errdefer` on thread spawn/join, file ≤350 / fn ≤50 / method ≤70, cross-compile both linux targets, no data races (the determinism anchor for the pool).
- **`docs/LIFECYCLE_PATTERNS.md`** — the worker-pool spawn/join + shutdown-flag lifecycle mirrors zombied's background-thread lifecycle in `serve.zig`.
- **`docs/LOGGING_STANDARD.md`** — per-worker logs carry a worker index via the logfmt envelope; never log the `zrn_` token.

REST_API_DESIGN, SCHEMA_CONVENTIONS, and ERROR REGISTRY do **not** apply — no HTTP handler, schema, or error-code change.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | cross-compile x86_64-linux + aarch64-linux; tagged-union results; `errdefer` on partial pool spawn; no data races. |
| PUB / Struct-Shape | yes | shape verdict for the new `worker_pool` façade (spawn + join + the shared stop/drain handle); one module, minimal pub surface. |
| File & Function Length (≤350/≤50/≤70) | yes | the `runLoop` split (control loop + extracted `workerLoop`) keeps each fn ≤50; the pool lives in a new file so `loop.zig` stays under 350. |
| UFS | yes | `RUNNER_WORKER_COUNT` name/default/clamp in `config.zig`; `HEARTBEAT_INTERVAL_MS` in `common/constants.zig` next to `RUNNER_OFFLINE_AFTER_MS` (the relationship is an invariant). |
| LOGGING | yes | worker-scoped structured logs; no token/secret. |
| LIFECYCLE | yes | thread spawn/join with `errdefer`; clean shutdown joins **all** workers and reaps **all** in-flight children. |
| SCHEMA / ERROR REGISTRY / UI / DESIGN TOKEN | no | no schema, error code, or UI surface touched. |

---

## Overview

**Goal (testable):** with `RUNNER_WORKER_COUNT=N`, one host concurrently executes N independent leases — N forked sandboxed children running at once — and reports all N, where the single-threaded path serialised them; on `.drain`/SIGTERM every worker finishes its in-flight child and takes no new lease, while on `.stop` every worker abandons its in-flight child promptly (reclaim re-leases it under fencing) — in all cases workers take no new lease and are joined with zero leaked threads, file descriptors, or children.

**Problem:** a host runner runs exactly one agent at a time. `runLoop` calls `pollAndProcess` → `executeAndReport`, which **blocks on the forked child** until the agent finishes before the next lease is even polled. A host with spare cores and memory sits idle while one long agent run monopolises it; per-host throughput is fixed at 1, so fleet throughput scales only by adding hosts.

**Solution summary:** refactor the runner daemon's single loop into (1) a **control loop** (the main thread) that owns the host heartbeat — one per host, on an explicit cadence — maps the heartbeat's `.stop`/`.drain` directives to shared atomics, and spawns then joins the pool; and (2) **N worker threads**, each running the existing `pollAndProcess` (lease→execute→report) verbatim, each with its own allocator scope and control-plane client. The atomic per-zombie `affinity.claim` already guarantees no two pollers — across hosts or threads — win the same zombie, so the control plane is untouched for correctness. N is a fixed operator-set knob; capacity-aware sizing is out of scope.

---

## Prior-Art / Reference Implementations

- **Concurrency lifecycle** → `src/zombied/cmd/serve.zig` (`shutdown_requested` atomic + `std.Thread.spawn` + deferred `.join()` of the signal / event-bus / approval-sweeper threads) is the exact spawn/join/shutdown-flag pattern the pool mirrors. The httpz handler-pool-per-worker (a fixed pool draining a shared work source) is the conceptual sibling on the zombied side.
- **The single-flight claim** → `src/zombied/fleet/assign.zig` + `reclaim.zig` — the existing known-good per-zombie atomic claim; the runner side adds **no** new claim logic.
- **Concurrency test prior art** → `src/runner/child_supervisor_concurrency_test.zig` (concurrent forked children) + `src/zombied/fleet/concurrency_lease_test.zig` (N racing claims) — the harnesses the new pool tests extend. The daemon loop runs without an LLM under `-Dexecutor-provider-stub` (`ZOMBIE_RUNNER_STUB_BIN`).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/runner/daemon/loop.zig` | EDIT ✅ | split `runLoop` into the control loop (heartbeat-first on cadence + `.stop`/`.drain` → atomics + lazy pool spawn/join); `pollAndProcess` made `pub` + retained verbatim; added a `stop_requested` atomic beside `drain_requested`. |
| `src/runner/daemon/worker_pool.zig` | CREATE ✅ | the fixed-N `std.Thread` pool: `spawn(io, alloc, cfg, env_map, stop, drain) → Pool` + `Pool.join()` (each worker its own `DebugAllocator` scope + control-plane client, runs `workerLoop`; `errdefer` joins any already-spawned worker on partial-spawn failure). |
| `src/runner/daemon/config.zig` | EDIT ✅ | add `worker_count` (env `RUNNER_WORKER_COUNT`, default 1, clamped `[1, 64]`, invalid → default + warn; RULE UFS), tagged-union parse. |
| `src/lib/common/constants.zig` | EDIT ✅ | add `HEARTBEAT_INTERVAL_MS` (10 s) with a comptime assertion `< RUNNER_OFFLINE_AFTER_MS`. |
| `src/runner/daemon/control_plane_client.zig` | EDIT ✅ (OUT OF ORIGINAL SCOPE — see Discovery) | `lease()` parse switched to `.alloc_always` — fixes a latent use-after-free (unescaped lease strings referenced the freed `res.body`) that the concurrent pool surfaced. Matches `getSelf`/`memoryHydrate`. |
| `build_runner.zig` | EDIT ✅ | add the test-only `executor-provider-stub` build flag + `stub_runner_exe_path`; build a stub-flagged `zombie-runner-execstub` exe and feed its path to the integration target (lets the pool drive the real fork→execute→report path with no LLM). |
| `src/runner/child_exec.zig` | EDIT ✅ | comptime-gated stub: an `executor_provider_stub` build emits a canned `result` frame instead of running the engine. |
| `src/runner/sandbox_args.zig` | EDIT ✅ | comptime-gated child-exec redirect to `stub_runner_exe_path` (the integration `zig test` binary has no `__execute` dispatch). |
| `deploy/baremetal/zombie-runner.service` | EDIT ✅ | document the `RUNNER_WORKER_COUNT` operator knob. |
| `docs/architecture/scaling.md` | EDIT ✅ (carried from Indy's WIP) | the `RUNNER_WORKER_COUNT` lever row + failure-domain note (already authored by Indy; carried into this PR). |
| `docs/architecture/runner_fleet.md` | EDIT ✅ (carried from Indy's WIP) | derived-capacity (`0..N` leases, `active < worker_count`) + multi-lease memory-isolation invariant (Indy's WIP; carried). |
| tests | CREATE ✅ | `worker_pool_test.zig` (unit lifecycle), `worker_pool_integration_test.zig` (Linux/real-process: N concurrent leases→reports + clean drain + concurrent fork/reap); inline config tests; `loop_test`/`child_exec_edge`/`sandbox_args_edge` Config literals updated for the new field. |

> **`src/runner/main.zig` NOT edited** — `runLoop` kept its signature and reads `cfg.worker_count`, so the call site is unchanged (cleaner than the originally-listed EDIT).
> Line numbers/symbols omitted by design — the agent reads current code.

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** a contained refactor of the runner daemon loop into control + N-worker-pool, reusing `pollAndProcess` / `executeAndReport` / `child_supervisor` unchanged, plus one config knob and one cadence constant. Four Sections.
- **Alternatives considered:** (a) an evented/async runner (the M88_001 shape) — rejected: the runner's cost is the **forked child** (core- and memory-bound work), not idle connections, so OS threads are the correct model for parallel children; (b) a capacity-aware dynamic pool sized to host load — rejected for now (explicitly out of scope per the locked design; fixed-N first); (c) one daemon process per agent (fork N daemons) — rejected: it duplicates the heartbeat identity, config load, and drain coordination, where N threads share one `zrn_` identity and one heartbeat cleanly.
- **Patch-vs-refactor verdict:** **scoped refactor** of one file's loop structure + an additive pool module and config field. The execution path (`child_supervisor`) and the control plane are untouched; nothing larger is silently bundled.

---

## Sections (implementation slices)

### §1 — `RUNNER_WORKER_COUNT` config knob — DONE

Add `worker_count` to `Config`, read from `RUNNER_WORKER_COUNT`. **Implementation default:** default **1** (zero behaviour change without opt-in — N=1 is exactly today's daemon) because capacity-awareness is out of scope, so the operator sizes N to the host; clamp to `[1, MAX]` so a fat-fingered value can't fork unbounded children. Name, default, and bounds single-sourced (UFS).

- **Dimension 1.1** — `RUNNER_WORKER_COUNT` parses; unset → 1; above MAX or `0` → clamped into `[1, MAX]` → Test `worker count parses default and clamps`.
- **Dimension 1.2** — a non-numeric value fails safe to the default with a logged warning, never crashing startup → Test `worker count invalid falls back to default`.

### §2 — Control/heartbeat loop split — DONE

Refactor `runLoop` so the main thread is the control loop: it heartbeats **once per host** on an explicit `HEARTBEAT_INTERVAL_MS` cadence (decoupled from worker execution, since workers now own polling), maps the heartbeat response's `.stop`/`.drain` to the shared atomics, and spawns then joins the pool. Derived liveness is already busy-before-offline (`constants.zig` `RUNNER_OFFLINE_AFTER_MS`), so an in-flight host is never falsely offline; the cadence only needs to keep an **idle** host live. **Implementation default — preserve today's distinct `.stop` vs `.drain` semantics (do NOT collapse them):** today `.drain` finishes the current lease then exits (`loop.zig` sets `draining`), while `.stop` is an immediate `break :outer`. Generalized to the pool: `.drain`/SIGTERM = graceful (stop taking leases, let in-flight children finish, join); `.stop` = immediate evict (stop taking leases, signal workers to abandon in-flight children — the always-reap `defer` reaps each, the lease lapses and reclaim re-leases it under fencing — join promptly). Collapsing both to graceful drain would change fleet-`.stop` from evict-now to wait-up-to-`MAX_RUNTIME_MS`; that is a behavior change, not parity.

- **Dimension 2.1** — with N workers the host emits one heartbeat stream (not N), and a busy pool never delays it (heartbeat is independent of worker execution) → Test `control loop heartbeats once independent of workers`.
- **Dimension 2.2** — `.drain`/SIGTERM set the drain flag → every worker finishes its in-flight child, takes no new lease, the control loop joins all → Test `drain finishes inflight and joins pool`; `.stop` sets the stop flag → every worker abandons its in-flight child (reclaim re-leases under fencing), takes no new lease, the control loop joins all promptly → Test `stop abandons inflight and joins pool`.

### §3 — Fixed-N worker pool (the concurrency) — DONE

`worker_pool.zig` spawns N `std.Thread` workers; each runs `workerLoop` = (while not stop/drain: `pollAndProcess`), with its **own** allocator scope and control-plane client (the client is stateless — `{base_url, io}`, a fresh `std.http.Client` per call — so sharing the `io` is safe). The per-zombie `affinity.claim` serialises claims; no control-plane change.

- **Dimension 3.1** — N=4 with ≥4 queued events → 4 children execute concurrently and 4 reports land; no zombie is claimed by two workers → Test `pool runs n leases concurrently without double claim`.
- **Dimension 3.2** — each worker uses an independent allocator scope (removing shared-GPA mutex contention; the daemon allocator is already thread-safe), and each is `deinit`'d on join; the concurrency harness shows no cross-worker leak → Test `workers do not share allocator state`.
- **Dimension 3.3** — fork-from-multithreaded is safe: N children spawn via `std.process.spawn` and are reaped cleanly, with no fork deadlock → Test `concurrent forked children spawn and reap`.

### §4 — Lifecycle parity (clean shutdown, no leaks) — DONE

The pool starts and stops exactly like the single-threaded daemon: on `.stop`/`.drain`/SIGTERM every worker thread is joined, every in-flight child reaped (the supervisor's always-reap `defer`), every per-lease workspace cleaned. A partial-spawn failure joins the workers already up. **Invariant:** shutdown leaks nothing.

- **Dimension 4.1** — on stop/drain/signal all N workers join, all in-flight children are reaped, and no thread, file descriptor, or child leaks → Test `pool clean shutdown leaks nothing` + `make memleak`.

---

## Interfaces

```
Config: + worker_count: u32   (env RUNNER_WORKER_COUNT; default 1; clamped [1, MAX]).
common/constants: + HEARTBEAT_INTERVAL_MS   (control-loop cadence; invariant: < RUNNER_OFFLINE_AFTER_MS).

Runner daemon (internal):
  runLoop(io, alloc, cfg, env_map)        — SAME signature; now the control loop:
                                            heartbeat (1/host, HEARTBEAT_INTERVAL_MS) +
                                            .stop/.drain → atomics + spawn/join pool.
  worker_pool.run(io, alloc, cfg, env_map, stop, drain)
                                          — spawn cfg.worker_count threads; each owns an
                                            allocator scope + client; loops pollAndProcess
                                            until stop/drain; joins all (errdefer joins
                                            partial spawns).
  Shared state: drain_requested (exists) + stop_requested (new), std.atomic.Value(bool).

Control plane: UNCHANGED — no new/changed endpoint; N pollers share one zrn_; the
  per-zombie affinity.claim admits exactly one winner. Lease/report/heartbeat shapes UNCHANGED.
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Concurrent fork | N workers fork at once | `std.process.spawn`'s post-fork child path is async-signal-safe (argv/envp built pre-fork, exec immediately); no process-global lock is touched in the child → Test 3.3 |
| Two workers poll the same instant | concurrent `cp.lease` | per-zombie `affinity.claim` → exactly one wins; the loser gets `.taken`, reads no event, moves on; never double-run → Test 3.1 |
| Worker dies mid-lease | unexpected thread death | `child_supervisor.run` never errors (maps every failure to a failed `ExecutionResult`); a worker that still dies is isolated — its child is reaped by the always-reap `defer`, the pool degrades to N−1 and the host keeps serving (respawn is out of scope; logged) |
| `RUNNER_WORKER_COUNT` absurdly high | operator misconfig | clamped to MAX; each child is still bounded by its cgroup memory limit; over-provisioning is operator-owned (capacity-aware out of scope) → Test 1.1 |
| Drain/SIGTERM during a full pool | graceful shutdown with N in-flight | each worker finishes its child at the between-lease boundary, takes no new lease; control loop joins all; no orphan child or thread → Test 2.2 / 4.1 |
| `.stop` during a full pool | operator evict-now with N in-flight | each worker abandons its in-flight child (the always-reap `defer` reaps it; the lease lapses and reclaim re-leases it under fencing), takes no new lease; control loop joins all promptly → Test 2.2 / 4.1 |
| Allocator mutex contention | workers sharing one allocator | the daemon's `DebugAllocator` is already thread-safe (mutex-guarded), so a shared GPA is a **contention** point, not a race; each worker owning its own allocator scope removes the global-mutex bottleneck → Test 3.2 |
| Idle host stops heartbeating | control loop starved | heartbeat runs on the control loop independent of worker execution, on a cadence `< RUNNER_OFFLINE_AFTER_MS`, so a busy pool never lapses liveness → Test 2.1 |
| Host loss with full pool | crash/network partition of a host running N children | **failure domain = N**: all N in-flight executions are lost at once (vs 1 on a single-lease host). M84_002's liveness sweeper marks the host offline and expires its per-zombie affinity, so all N zombies re-lease to healthy hosts; no work is dropped, but N runs are interrupted and restart. Operator-owned tradeoff (the `RUNNER_WORKER_COUNT` knob trades isolation for utilization). |
| Resource contention across workers | N heavy children share one host | CPU / RAM / disk IOPS / network are not isolated across workers, so N concurrent heavy agents slow each other; each child is still bounded by its cgroup memory limit. `RUNNER_WORKER_COUNT` is a capacity knob, not a throughput guarantee — sizing N to the host is operator-owned. |

---

## Invariants

1. **No two workers (or hosts) execute the same zombie's event** — enforced by the existing atomic per-zombie `affinity.claim` (claim precedes read) + Test 3.1; the runner adds no claim logic.
2. **Children are created only via `std.process.spawn`** (never a bare fork) — enforced by `child_process.forkExec` remaining the sole spawn path + Test 3.3 (concurrent spawn/reap clean).
3. **Each worker owns an independent allocator scope** — the daemon GPA is already thread-safe, so this removes mutex *contention*, not a data race; `worker_pool` constructs N per-worker allocators and **`deinit`s each on join** (the daemon's single top-level `gpa.deinit()` does not cover them) — enforced by the pool's per-worker construction/teardown + Test 3.2.
4. **Clean shutdown joins every worker and reaps every in-flight child** — no leaked thread/fd/child — enforced by join-all (+ `errdefer` on partial spawn) + the supervisor's always-reap `defer` + Test 4.1 + `make memleak`.
5. **Exactly one heartbeat stream per host regardless of N** — enforced by the heartbeat living only on the control loop, never in `workerLoop` + Test 2.1.
6. **N=1 is behaviourally identical to today's single-threaded daemon** — enforced by `worker_count` default 1 + the existing runner suites passing unchanged (regression).
7. **`HEARTBEAT_INTERVAL_MS < RUNNER_OFFLINE_AFTER_MS`** — an idle host always heartbeats before it would be derived offline — enforced by a comptime assertion in `constants.zig`.

---

## Test Specification (tiered)

> Daemon-loop tests run without an LLM via `-Dexecutor-provider-stub` (`ZOMBIE_RUNNER_STUB_BIN`). Linux-gated tests cross-compile the aarch64 test graph and run in a native arm64 container (qemu x86_64 is a false oracle).

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `worker count parses default and clamps` | unset → 1; "8" → 8; "0"/"99999" → clamped into [1, MAX]. |
| 1.2 | unit | `worker count invalid falls back to default` | "abc" → 1 + warn; startup never crashes. |
| 2.1 | integration | `control loop heartbeats once independent of workers` | N=4: one heartbeat stream; a busy pool still heartbeats within cadence. |
| 2.2 | integration | `drain finishes inflight and joins pool` + `stop abandons inflight and joins pool` | drain/SIGTERM mid-run → in-flight completes; `.stop` mid-run → in-flight abandoned + reclaimed; both → no new lease, all workers join. |
| 3.1 | integration | `pool runs n leases concurrently without double claim` | N=4, ≥4 events → 4 concurrent children + 4 reports; no zombie claimed twice. |
| 3.2 | integration | `workers do not share allocator state` | concurrency harness over N workers → each worker's allocator `deinit`'d on join; no cross-worker leak. |
| 3.3 | integration | `concurrent forked children spawn and reap` | N concurrent `std.process.spawn` children → all reaped, no deadlock. |
| 4.1 | integration | `pool clean shutdown leaks nothing` | stop/drain/signal → all joined, all children reaped; `make memleak` clean. |

**Regression:** the existing single-runner lease/execute/report + drain suites pass unchanged at N=1 (Invariant 6). **Idempotency:** per-worker lease re-poll/backoff semantics are unchanged from `pollAndProcess`.

---

## Acceptance Criteria

- [x] N concurrent leases execute + report on one host; the single-threaded path serialised them — `test-integration` pool harness: N=4 leases → 4 reports, `max_in_lease ≥ 2`
- [x] N=1 behaviourally identical to today (regression) — existing runner suites pass unchanged at default `worker_count=1`; pool unit test covers the N=1 boundary
- [x] Clean shutdown joins all workers, reaps all children, leaks nothing — pool integration test drains + joins (no leak/hang); testing.allocator clean
- [x] `RUNNER_WORKER_COUNT` parsed / clamped / fail-safe — `make test` (config inline tests 1.1/1.2)
- [x] `make lint-zig` clean · runner unit graph 235/241 (0 failed, 6 skipped)
- [x] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` (exe, both targets, exit 0)
- [x] no non-`.md` file over 350 lines added (FLL gate ✓) · `gitleaks` runs in pre-commit

---

## Eval Commands (post-implementation)

```bash
# E1: concurrency win + clean drain
make test-integration 2>&1 | grep -iE "worker|pool|concurrent|double.?claim|drain|heartbeat" | tail -20
# E2: Build
zig build && echo "PASS" || echo "FAIL"
# E3: Cross-compile
zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux && echo "PASS" || echo "FAIL"
# E4: Lint
make lint 2>&1 | grep -E "✓|FAIL"
# E5: Leak
make memleak 2>&1 | tail -5
# E6: Gitleaks
gitleaks detect 2>&1 | tail -3
# E7: 350-line gate (exempts .md)
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
```

---

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.** N/A — no files deleted. The `runLoop` split is in-place; `pollAndProcess` / `executeAndReport` / `child_supervisor` are retained verbatim.

**2. Orphaned references — zero remaining.** N/A — additive (a pool module, a config field, a cadence constant); no public symbol is removed or renamed.

---

## Discovery (consult log)

> **Empty at creation.** Append as work surfaces consults/decisions.

- **Provenance (Jun 08 2026)** — the M88 scaling pivot: the async substrate (M88_001) is gated behind an evidence gate; this worker-thread pool is the boring, real per-host throughput fix. Design LOCKED by Indy — fixed-N `std.Thread` pool, reuse `pollAndProcess`, no capacity-awareness.
- **Code-grounded validation (Jun 08 2026)** — verified in the runner code before authoring: (1) **fork-safety** — `forkExec` uses `std.process.spawn` (async-signal-safe post-fork path), so a multithreaded daemon forking is safe by construction; (2) **no control-plane change for correctness** — `assign.zig`: "the atomic per-zombie CLAIM. Exactly one of N racing runners wins the slot; a loser gets `.taken` and moves on, having read no event"; (3) **stateless client** — `control_plane_client.zig` is `{base_url, io}` with a fresh `std.http.Client` per call.
- **Cross-spec note (M84_002) — reconciled (Jun 09 2026).** M84_002 originally proposed a singular `current_lease_id` column; it has been **amended to drop it**. Both specs now model "busy" as **derived** from `fleet.runner_leases`: `busy = EXISTS(active lease)`, `active = COUNT(active)`. A pooled host holding 0..N leases needs no control-plane schema change (the per-zombie `affinity.claim` admits exactly one winner). **Capacity scheduling is NOT wired (adversarial review Jun 09 caught a circular reference):** `RUNNER_WORKER_COUNT` is a runner-local env knob and is **not** transported to `zombied` (the heartbeat body stays empty — Out of Scope), so the control plane cannot compute `available = worker_count − active` and there is **no server-side per-runner lease cap**. A pooled host is bounded only by how fast its N workers each independently poll-and-win (correctness holds regardless — the claim never double-runs a zombie). Reporting `worker_count` + enforcing a cap + capacity-ranked placement is the scheduler workstream (M85_001), explicitly out of scope for both specs. No singular-lease assumption remains in either spec.
- **Runner memory plane is already multi-lease-safe (Jun 08 2026, code-grounded).** Hydrate is keyed by `payload.event.zombie_id` (`loop.zig`), capture is fenced by `lease_id` + `fencing_token` (`runner/memory.zig`), writes are idempotent (`ON CONFLICT (key, zombie_id)`), and the durable store lives in `zombied`'s Postgres — not runner-local. M84_005's Jun 06 decision already moved `zombie_id` into the path explicitly ("does not depend on a one-live-lease invariant"). Each worker owns its own allocator scope, per-lease workspace (`{base}/{lease_id}`), and per-call forwarders. **There is no phantom "memory must be lease-scoped first" blocker.** Memory isolation rests on the per-zombie **affinity-slot claim admitting a single live holder** — `uq_runner_affinity_zombie UNIQUE(zombie_id)` + the `leased_until < now` time-gate — plus **capture-time `fencing_token` rejection** of a stale holder (`runner/memory.zig` rejects `fencing_token < live_seq`). This is **not** a DB unique constraint on `fleet.runner_leases` (multiple lease rows per zombie are normal — expired + re-leased) and it is **not** `zombie_id` scoping alone: a slow old holder and a reclaimer can transiently coexist, which is *why* fencing exists — the fence ensures only one writer ever *durably persists* into one zombie's namespace. A future retry / speculative / failover / takeover-lease feature that broke the single-live-holder property would have to scope memory by `lease_id` first. Documented in `runner_fleet.md`.
- **Code-grounded concurrency evidence (Jun 09 2026, adversarial review).** The "verbatim reuse is safe under N threads" claim was verified against std + runner code: (1) **stdout isolation confirmed** — each child gets a fresh `pipe2` stdout fd (`child_process.zig` `.stdout = .pipe`), the supervisor reads `child.stdout.?.handle` (a per-child fd), and `pipe_proto` is a stateless reader taking `fd` as an arg; the only shared fd is `.stderr = .inherit`, which is benign (no framed protocol on stderr, logs interleave at line granularity). (2) **Fork-from-N-threads is safe by construction** — `std.process.spawn` does all allocation (argv/envp/pipes) **before** `fork()`, the post-fork child path calls only async-signal-safe syscalls (dup2/setpgid/execvpe, no malloc/`std.debug`), and its pipes are `CLOEXEC` *explicitly* for the "another thread is racing to spawn" case; `scanEnviron` is mutex-guarded + idempotent. (3) **Allocator is already thread-safe** — `DebugAllocator(.{}).thread_safe = !single_threaded = true`, mutex-guarded alloc/free; the per-worker scope removes *contention*, not a race. (4) **cgroup/workspace per-child are collision-free** — exec cgroup is `…/exec-{hex}` with a fresh CSPRNG `ExecutionId` per child, workspace is `{base}/{lease_id}` (unique), each child `setpgid(0,0)`s into its own group. No shared fixed path.
- **Operational tradeoff — failure domain widens with N (CTO review).** `worker_count=1` = maximum isolation: one host loss = **one** execution lost. `worker_count=N` = better utilization but a **larger failure domain**: one host loss = **N** executions lost (all re-leased by M84_002's sweeper, but interrupted). This is the headline cost of the utilization win and is operator-owned via the knob. Captured in Failure Modes + Out of Scope.
- **Capacity ≠ throughput (CTO review).** `RUNNER_WORKER_COUNT` is a capacity knob, not a throughput guarantee: CPU, RAM, disk IOPS, and network are **not** isolated across workers, so N agents doing heavy work (npm install / cargo build) contend. Per-lease cost / resource class / weight is future work; today a lease is treated as one undifferentiated capacity unit. Out of Scope.
- **DB-scale note (CTO review).** `active = COUNT(active leases)` is derived now (cheap at current fleet size). A materialized counter/index for a capacity-ranked scheduler (`ORDER BY available`) may be introduced **later, for scheduler scale only** — explicitly not today, to avoid counter drift.
- **Fixed-N vs capacity-units (CTO review).** Heterogeneous hosts (2 vs 64 cores) point toward capacity *units* rather than a fixed worker count; flagged as future direction, not a blocker for this spec.
- **Deferrals** — none at authoring.
- **PLAN handshake confirmed (Jun 08 2026).** All five "Implementing agent" pointers read; the five load-bearing assumptions hold against the code: fixed-N from `RUNNER_WORKER_COUNT`; workers run `pollAndProcess` verbatim with heartbeat 1/host on the control loop; no control-plane change for correctness; each worker owns an independent `DebugAllocator` scope; `.stop`/`.drain`/SIGTERM finish in-flight, take no new lease, join all. No mismatch.
- **Interface refinement — `spawn`/`join` instead of a single blocking `run` (Jun 08 2026).** The Interfaces sketch listed `worker_pool.run(...)` spawning AND joining; but §2 requires the control loop (main thread) to interleave heartbeats between spawn and join, so the pool exposes `spawn(io, alloc, cfg, env_map, stop, drain) → Pool` + `Pool.join()`. `runLoop` spawns the pool lazily after the first `.ok` heartbeat (preserving the boot test's "first contact is a heartbeat, never register") and joins it via `defer` on exit. Behaviour matches the spec; only the call shape is split.
- **Latent use-after-free in `control_plane_client.lease()` — found + fixed (Jun 08 2026, OUT OF ORIGINAL BLAST RADIUS).** `lease()` parsed `LeaseResponse` with default JSON options (`.alloc_if_needed`), so unescaped strings (`lease_id`/`event_id`/`zombie_id`) **referenced `res.body`**, which the method frees on return — leaving the returned `LeasePayload` dangling. The single-threaded daemon survived by luck (freed-not-reused); the concurrent pool's per-worker allocator reuses the buffer, so the first read of a payload string (a `log.info`) SEGV'd. Fix: `.alloc_always` (copies into the Parsed arena), matching `getSelf`/`memoryHydrate`, which already do this for the same reason. The pool feature is correct only with this fix — surfaced to Indy for sign-off as an out-of-scope but load-bearing edit.
- **Executor-stub harness built (Path B, Indy-chosen Jun 08 2026).** The documented `-Dexecutor-provider-stub`/`ZOMBIE_RUNNER_STUB_BIN` mechanism did NOT exist; built it as a comptime-gated build flag (no env backdoor): a stub-flagged child emits a canned `result` frame (`child_exec.zig`), and a stub-flagged daemon redirects the forked child's exec target to a prebuilt stub exe (`sandbox_args.zig`), wired in `build_runner.zig`. Production builds (flag false) comptime-eliminate both seams. This lets `worker_pool_integration_test.zig` drive the real lease→fork→execute→report path with N concurrent children and no LLM.
- **Workspace-base creation is `main.zig`'s job (Jun 08 2026).** The pool integration test drives `worker_pool.spawn` directly (bypassing `main`), so it must create `cfg.workspace_base` itself or every `prepareWorkspace` fails closed and no lease executes — captured in the test setup.
- **Carried-in WIP (Jun 08 2026, Indy direction).** Per Indy, this PR carries his uncommitted `scaling.md` + `runner_fleet.md` WIP (the M88_002/M84_002 arch reconciliation) and the untracked `M84_008` pending spec (a separate M84 workstream — carried at his explicit request, not an M88 dependency).
- **`/review` adversarial pass (Jun 09 2026) — ship-as-is, one P2 fixed.** Independent fresh-context review confirmed: no data race (per-worker allocator + `cp` + by-value `WorkerContext`; only the two `seq_cst` atomics are shared), the `.alloc_always` UAF fix is complete (no sibling bug — `heartbeat` returns enum-only, `report`/`activity`/`renew` escape no parsed slice), `errdefer` partial-spawn has no double-free, shutdown is bounded (the lease verb is non-blocking → `join` can't hang) and reaps all children, the stub seam is comptime-eliminated in prod (no env backdoor), and all 7 Invariants hold. **Fixed P2:** `executeAndReport` returned with no backoff on `prepareWorkspace`/`report` failure → a persistent failure hot-spun the poll loop ×N; added the existing `TRANSPORT_ERROR_BACKOFF_MS` sleep on both paths. **Noted (not fixed):** (a) the partial-spawn `errdefer` sets the process-global `stop_requested` — harmless under the one-shot `runLoop` but a latent footgun if spawn ever becomes retryable; (b) fleet `.stop`/`.drain` has up to `HEARTBEAT_INTERVAL_MS` (~10 s) control-loop detection latency before workers are told (workers then drain within one `NO_WORK_RETRY_AFTER_MS`).

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Coverage vs this Test Specification (esp. no-double-claim, concurrent fork/reap, clean shutdown, N=1 regression). | Clean; counts in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial review vs spec, `dispatch/write_zig.md`, LIFECYCLE, Failure Modes, Invariants (esp. "no two workers run the same zombie", "shutdown leaks nothing"). | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open Pull Request. | Comments addressed before merge. |

---

## Verification Evidence

> Filled during VERIFY.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Concurrency + reports + clean drain | `zig build --build-file build_runner.zig test-integration` | 54/59 passed, 5 skipped (Linux-only sandbox), 0 failed; pool test asserts 4 reports + `max_in_lease ≥ 2` | ✅ |
| Concurrent fork/reap (3.3) | `test-integration` | 8 threads `std.process.spawn`+reap, all clean, no deadlock | ✅ |
| Config parse/clamp/invalid | `zig build --build-file build_runner.zig test` | 235/241 passed, 0 failed (incl. 1.1/1.2) | ✅ |
| Zig lint (fmt/ZLint/FLL/drain/role/legacy) | `make lint-zig` | Lint passed — 0 errors | ✅ |
| Cross-compile x86_64-linux | `zig build --build-file build_runner.zig -Dtarget=x86_64-linux` | exit 0 | ✅ |
| Cross-compile aarch64-linux | `zig build --build-file build_runner.zig -Dtarget=aarch64-linux` | exit 0 | ✅ |

> Note: `make memleak` / the native-arm64-container run of the Linux-gated graph are the CI VERIFY tier; locally the pool/fork tests run on macOS (dev_none, no bwrap) and the testing.allocator + DebugAllocator scopes report no leak.

---

## Out of Scope

- **Capacity-aware / dynamic pool sizing** (autoscale N to host cores/memory) — future refinement; fixed-N here. Heterogeneous hosts point toward capacity *units* over a fixed count, but that is future direction.
- **Per-lease resource accounting** (lease cost / resource class / weight; CPU/RAM/disk/network isolation across workers) — out. `RUNNER_WORKER_COUNT` is a capacity knob, not a throughput guarantee; a lease is one undifferentiated capacity unit here.
- **A materialized active-lease count or capacity column** — `busy`/`active`/`available` derive from `fleet.runner_leases`; materialization is deferred (for scheduler scale only, if ever proven necessary).
- **Per-worker capacity reporting in the heartbeat** (capacity/version fields) — a later workstream; the heartbeat body stays empty.
- **Worker respawn-on-death supervision** — default is degrade-to-N−1 + log; a self-healing pool is future work.
- **The async/evented substrate** — M88_001 (gated behind its evidence gate).
- **Control-plane per-runner lease caps / fairness / scheduling** — out; the per-zombie claim suffices and the fleet non-goals fence holds.
