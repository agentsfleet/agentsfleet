# M88_002: Runner worker-thread pool — N concurrent agents per host

**Prototype:** v2.0.0
**Milestone:** M88
**Workstream:** 002
**Date:** Jun 08, 2026
**Status:** PENDING
**Priority:** P1 — a host runner executes exactly one agent at a time; this lifts per-host throughput to ~N concurrent without the async rewrite (M88_001), and is the boring lever the M88 pivot identified as the real fix.
**Categories:** API
**Batch:** B1 — sibling of M88_001 (gated); independent, may run concurrently.
**Branch:** {feat/m88-runner-worker-pool — added when work begins}
**Depends on:** none for correctness (the per-zombie `affinity.claim` already serializes N pollers). Composes with M84_002 — its singular `current_lease_id` busy-marker assumes one lease per runner and must become "≥1 active lease" under this pool (flagged in that spec, see Discovery).
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
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent + `ASSUMPTIONS I'M MAKING: …`. Load-bearing assumptions: (1) fixed-N from `RUNNER_WORKER_COUNT`, not capacity-aware; (2) each worker runs the existing `pollAndProcess` verbatim — heartbeat stays **one per host** on the control loop, never per worker; (3) **no control-plane change for correctness** (the per-zombie claim admits one winner); (4) each worker owns an independent allocator scope; (5) `.stop`/`.drain`/SIGTERM finish in-flight work, take no new lease, and join every worker. A mismatch → STOP and reconcile.

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

**Goal (testable):** with `RUNNER_WORKER_COUNT=N`, one host concurrently executes N independent leases — N forked sandboxed children running at once — and reports all N, where the single-threaded path serialised them; on `.stop`/`.drain`/SIGTERM every worker finishes its in-flight child, takes no new lease, and is joined with zero leaked threads, file descriptors, or children.

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
| `src/runner/daemon/loop.zig` | EDIT | split `runLoop` into the control loop (heartbeat on cadence + `.stop`/`.drain` → atomics + spawn/join) and an extracted `workerLoop` wrapping the existing `pollAndProcess`; add a `stop_requested` atomic beside `drain_requested`. `pollAndProcess`/`executeAndReport` retained verbatim. |
| `src/runner/daemon/worker_pool.zig` | CREATE | the fixed-N `std.Thread` pool façade: spawn `cfg.worker_count` workers (each its own allocator scope + control-plane client), run `workerLoop`, join all on shutdown; `errdefer` joins any already-spawned worker on a partial-spawn failure. |
| `src/runner/daemon/config.zig` | EDIT | add `worker_count` (env `RUNNER_WORKER_COUNT`, default 1, clamped `[1, MAX]`, invalid → default + warn; RULE UFS). |
| `src/lib/common/constants.zig` | EDIT | add `HEARTBEAT_INTERVAL_MS` (control-loop cadence) with the invariant `< RUNNER_OFFLINE_AFTER_MS`. |
| `src/runner/main.zig` | EDIT | thread `cfg.worker_count` into `runLoop`; N=1 keeps the call site behaviourally identical. |
| `docs/architecture/scaling.md` | EDIT | document the runner worker-pool as the per-host throughput lever (sibling to the zombied handler-pool knob). |
| `deploy/baremetal/zombie-runner.service` | EDIT | document the `RUNNER_WORKER_COUNT` operator knob (commented default). |

> Line numbers/symbols omitted by design — the agent reads current code.

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** a contained refactor of the runner daemon loop into control + N-worker-pool, reusing `pollAndProcess` / `executeAndReport` / `child_supervisor` unchanged, plus one config knob and one cadence constant. Four Sections.
- **Alternatives considered:** (a) an evented/async runner (the M88_001 shape) — rejected: the runner's cost is the **forked child** (core- and memory-bound work), not idle connections, so OS threads are the correct model for parallel children; (b) a capacity-aware dynamic pool sized to host load — rejected for now (explicitly out of scope per the locked design; fixed-N first); (c) one daemon process per agent (fork N daemons) — rejected: it duplicates the heartbeat identity, config load, and drain coordination, where N threads share one `zrn_` identity and one heartbeat cleanly.
- **Patch-vs-refactor verdict:** **scoped refactor** of one file's loop structure + an additive pool module and config field. The execution path (`child_supervisor`) and the control plane are untouched; nothing larger is silently bundled.

---

## Sections (implementation slices)

### §1 — `RUNNER_WORKER_COUNT` config knob

Add `worker_count` to `Config`, read from `RUNNER_WORKER_COUNT`. **Implementation default:** default **1** (zero behaviour change without opt-in — N=1 is exactly today's daemon) because capacity-awareness is out of scope, so the operator sizes N to the host; clamp to `[1, MAX]` so a fat-fingered value can't fork unbounded children. Name, default, and bounds single-sourced (UFS).

- **Dimension 1.1** — `RUNNER_WORKER_COUNT` parses; unset → 1; above MAX or `0` → clamped into `[1, MAX]` → Test `worker count parses default and clamps`.
- **Dimension 1.2** — a non-numeric value fails safe to the default with a logged warning, never crashing startup → Test `worker count invalid falls back to default`.

### §2 — Control/heartbeat loop split

Refactor `runLoop` so the main thread is the control loop: it heartbeats **once per host** on an explicit `HEARTBEAT_INTERVAL_MS` cadence (decoupled from worker execution, since workers now own polling), maps the heartbeat response's `.stop`/`.drain` to the shared atomics, and spawns then joins the pool. Derived liveness is already busy-before-offline (`constants.zig` `RUNNER_OFFLINE_AFTER_MS`), so an in-flight host is never falsely offline; the cadence only needs to keep an **idle** host live. **Implementation default:** `.stop` and `.drain` both trigger graceful pool drain (finish in-flight, no new lease) — matching today's finish-current-then-exit semantics.

- **Dimension 2.1** — with N workers the host emits one heartbeat stream (not N), and a busy pool never delays it (heartbeat is independent of worker execution) → Test `control loop heartbeats once independent of workers`.
- **Dimension 2.2** — a `.stop`/`.drain` directive (and SIGTERM) sets the flag; every worker finishes its in-flight child, takes no new lease, and the control loop joins all → Test `drain finishes inflight and joins pool`.

### §3 — Fixed-N worker pool (the concurrency)

`worker_pool.zig` spawns N `std.Thread` workers; each runs `workerLoop` = (while not stop/drain: `pollAndProcess`), with its **own** allocator scope and control-plane client (the client is stateless — `{base_url, io}`, a fresh `std.http.Client` per call — so sharing the `io` is safe). The per-zombie `affinity.claim` serialises claims; no control-plane change.

- **Dimension 3.1** — N=4 with ≥4 queued events → 4 children execute concurrently and 4 reports land; no zombie is claimed by two workers → Test `pool runs n leases concurrently without double claim`.
- **Dimension 3.2** — each worker uses an independent allocator scope; the concurrency harness shows no cross-worker shared-allocator race or leak → Test `workers do not share allocator state`.
- **Dimension 3.3** — fork-from-multithreaded is safe: N children spawn via `std.process.spawn` and are reaped cleanly, with no fork deadlock → Test `concurrent forked children spawn and reap`.

### §4 — Lifecycle parity (clean shutdown, no leaks)

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
| Stop during a full pool | signal/`.stop`/`.drain` with N in-flight | each worker finishes its child at the between-lease boundary, takes no new lease; control loop joins all; no orphan child or thread → Test 2.2 / 4.1 |
| Allocator contention/race | workers sharing one allocator | each worker owns an allocator scope → no global allocator-mutex bottleneck or cross-thread race → Test 3.2 |
| Idle host stops heartbeating | control loop starved | heartbeat runs on the control loop independent of worker execution, on a cadence `< RUNNER_OFFLINE_AFTER_MS`, so a busy pool never lapses liveness → Test 2.1 |

---

## Invariants

1. **No two workers (or hosts) execute the same zombie's event** — enforced by the existing atomic per-zombie `affinity.claim` (claim precedes read) + Test 3.1; the runner adds no claim logic.
2. **Children are created only via `std.process.spawn`** (never a bare fork) — enforced by `child_process.forkExec` remaining the sole spawn path + Test 3.3 (concurrent spawn/reap clean).
3. **Each worker owns an independent allocator scope** — no cross-worker shared mutable allocator state — enforced by the pool constructing per-worker allocators + Test 3.2.
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
| 2.2 | integration | `drain finishes inflight and joins pool` | drain/stop mid-run → in-flight completes, no new lease claimed, all workers join. |
| 3.1 | integration | `pool runs n leases concurrently without double claim` | N=4, ≥4 events → 4 concurrent children + 4 reports; no zombie claimed twice. |
| 3.2 | integration | `workers do not share allocator state` | concurrency harness over N workers → no cross-worker race/leak. |
| 3.3 | integration | `concurrent forked children spawn and reap` | N concurrent `std.process.spawn` children → all reaped, no deadlock. |
| 4.1 | integration | `pool clean shutdown leaks nothing` | stop/drain/signal → all joined, all children reaped; `make memleak` clean. |

**Regression:** the existing single-runner lease/execute/report + drain suites pass unchanged at N=1 (Invariant 6). **Idempotency:** per-worker lease re-poll/backoff semantics are unchanged from `pollAndProcess`.

---

## Acceptance Criteria

- [ ] N concurrent leases execute + report on one host; the single-threaded path serialised them — verify: `make test-integration` + the pool harness
- [ ] N=1 behaviourally identical to today (regression) — verify: `make test-integration`
- [ ] Clean shutdown joins all workers, reaps all children, leaks nothing — verify: `make test-integration` (Test 4.1) + `make memleak`
- [ ] `RUNNER_WORKER_COUNT` parsed / clamped / fail-safe — verify: `make test`
- [ ] `make lint` clean · `make test` passes
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `gitleaks detect` clean · no non-`.md` file over 350 lines added

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
- **Cross-spec note (M84_002)** — M84_002 introduces a singular `current_lease_id` "busy" marker that assumes one lease per runner. Under this pool a host holds **≤N concurrent leases**, so M84_002 (pending, being amended in the same effort) must make its busy-marker "has ≥1 active lease" (or a count), and any future capacity/heartbeat fields report N. Owned by M84_002; flagged here so the interaction is not lost.
- **Deferrals** — none at authoring.

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
| Concurrency + no double-claim | `make test-integration` | {paste} | |
| N=1 regression | `make test-integration` | {paste} | |
| Clean shutdown / leaks | `make memleak` | {paste} | |
| Config parse/clamp | `make test` | {paste} | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` | {paste} | |

---

## Out of Scope

- **Capacity-aware / dynamic pool sizing** (autoscale N to host cores/memory) — future refinement; fixed-N here.
- **Per-worker capacity reporting in the heartbeat** (capacity/version fields) — a later workstream; the heartbeat body stays empty.
- **Worker respawn-on-death supervision** — default is degrade-to-N−1 + log; a self-healing pool is future work.
- **The async/evented substrate** — M88_001 (gated behind its evidence gate).
- **M84_002's `current_lease_id` semantics under the pool** — owned by M84_002 (flagged in Discovery).
- **Control-plane per-runner lease caps / fairness / scheduling** — out; the per-zombie claim suffices and the fleet non-goals fence holds.
