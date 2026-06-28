<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh.
-->

# M102_002: Concurrency hardening — lock-free reads on the daemon's hot paths (Read-Copy-Update / RCU), lock-free queues on the producer/consumer paths

**Prototype:** v2.0.0
**Milestone:** M102
**Workstream:** 002
**Date:** Jun 26, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — the JSON Web Key Set (JWKS) key set is read on **every authenticated request**; an exclusive `common.Mutex` there caps auth throughput at scale. This retires the lock on the hot path.
**Categories:** API
**Batch:** B2 — rides after M102_001's credential-broker work (§1–§8); its own commits/sections, kept separate for reviewability.
**Branch:** feat/m102-concurrency-hardening
**Depends on:** none hard. Composes with M102_001 (same worktree; the credential cache already moved to `cache.zig`'s shared-read model, the in-repo precedent this generalises).
**Provenance:** agent-generated (interactive CTO design session with Indy, Jun 26 2026; the architecture below is confirmed, not proposed). Re-confirm at PLAN.
**Test Baseline:** unit=2214 integration=213

> **Scope honesty.** This is a daemon-wide concurrency change touching auth (`jwks`), event delivery, metrics, connection pools, and logging. It introduces **no new feature and no behaviour change** — same outputs, fewer locks on the read path. `jwks` is the auth boundary and is reviewed hardest. The work lands as its own clearly-delineated workstream (separate commits), never invisibly folded into the credential-broker diff.

**Canonical architecture:** `docs/AUTH.md` (jwks is the JWKS RS256 verifier — auth-flow; read before touching) + `dispatch/write_zig.md` (atomics carry a `// safe because:` comment; tagged-union results; cross-compile both linux targets) + `docs/architecture/runner_fleet.md` (the daemon's threading model). This workstream changes **how** shared read-mostly state is synchronised; it adds no new trust plane and no new wire surface.

---

## Implementing agent — read these first

1. `docs/AUTH.md` §"Flow 1/2" + §"Backend validation" — `jwks` validates a Clerk JWT on every CLI + dashboard request. **Auth-flow file — read before any change to `jwks.zig`.**
2. `dispatch/write_zig.md` §Concurrency + §"Allocator Ownership" — weak atomic orderings need `// safe because:`; init/deinit lifecycle; the LIFECYCLE GATE.
3. `src/agentsfleetd/credentials/broker.zig` (M102_001) — the in-repo precedent: `cache.zig`'s sharded shared-read + refcounted entries. RCU is the next step (lock-free reads) for read-mostly state that is a single snapshot rather than a keyed map.
4. **Bun reference (read, do not vendor wholesale):** `~/Projects/oss/bun/src/ptr/ref_count.zig` (intrusive atomic refcount), `~/Projects/oss/bun/src/ptr/shared.zig` (`AtomicShared` Arc), `~/Projects/oss/bun/src/threading/unbounded_queue.zig` (the lock-free queue vendored here, with attribution).

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `perf(m102): lock-free reads on hot paths (RCU) + lock-free producer/consumer queues`
- **Intent (one sentence):** retire the exclusive `common.Mutex` on the daemon's read-mostly hot paths so a read is a single atomic load (no lock, no contention), and move producer/consumer sites to a lock-free queue — same behaviour, far less serialization.
- **Handshake (agent fills at PLAN):** restate intent; list `ASSUMPTIONS I'M MAKING:`. Confirm: (a) `Io.RwLock` is deliberately NOT used (it taxes the hot reader; RCU pays only the rare writer); (b) RCU correctness rests on read critical sections being bounded-tiny (pure CPU, no alloc/IO) vs the long write interval; (c) the vendored queue is attributed to bun and lives in `src/lib/common/`. A `[?]` blocks EXECUTE.

---

## Product Clarity

1. **Successful user moment** — invisible to the user by design: at 10× the current concurrent-request load, authenticated-request throughput scales with cores instead of plateauing on one lock; the operator sees lower p99 auth latency under load and no new failures.
2. **Preserved behaviour** — every read returns exactly what the mutex version returned; JWKS refresh-on-`kid`-miss is unchanged; event delivery ordering, metric values, pool semantics, and log output are byte-identical. This is a synchronization swap, not a semantics change.
3. **Optimal-way check** — RCU (lock-free atomic-load reads + a write-mutex on the rare refresh + bounded retention) is the most direct fit for read-mostly state: the reader pays nothing, only the 6-hourly writer pays. The gap to "perfect" (epoch/hazard-pointer reclamation for unbounded read critical sections) is unnecessary here and explicitly out of scope.
4. **Rebuild-vs-iterate** — iterate. The sites exist and work; this swaps their synchronization primitive. Verdict: targeted refactor, hottest-path-first, one site per commit.
5. **What we build** — a `common.snapshot` RCU helper (atomic-publish + bounded retention) + a vendored lock-free `unbounded_queue`; then migrate jwks → event hubs → metrics → pools → logging, each with a concurrency test + a read-path micro-benchmark.
6. **What we do NOT build** — epoch-based reclamation / hazard pointers (the bounded-retention grace suffices); any behaviour change; any change to sites where an exclusive mutex is correct (write-heavy pool mutation); a new generic lock-free hashmap (read-mostly here is a single snapshot, not a keyed map — `cache.zig` already covers the keyed case).
7. **Fit** — compounds with the M102_001 credential cache (same shared-read philosophy) and the existing `common.sync` toolkit (which gains the missing shared-read primitives); must not destabilise auth validation, event delivery, or pool liveness.
8. **Surface order** — foundation first (the primitive must be proven before any hot site uses it), then **hottest-first**: jwks (every request) → event hubs → metrics → pools → logging.
9. **Dashboard restraint** — N/A (no user surface). The only visible artefact is the benchmark delta recorded in Verification Evidence.
10. **Confused-user next step** — N/A (internal). A maintainer reading a migrated site finds the `// safe because:` atomic-ordering comment and the `RESOURCE BUDGET` block explaining the read/write asymmetry.

---

## Applicable Rules

- **`dispatch/write_zig.md`** — **Concurrency** (weak atomic orderings `\.(acquire|release|monotonic|acq_rel)` each need an adjacent `// safe because:`; document which side does the release-store and which the acquire-load), **Memory Safety** (no use-after-free across a publish; `std.testing.allocator` leak-checks), **Type Design** (tagged-union results), **Allocator Ownership** (the snapshot holder + queue store `alloc`), **LIFECYCLE** (init/deinit pairing on every new owning type), cross-compile both linux targets.
- **`docs/greptile-learnings/RULES.md`** — **RULE NDC** (remove the retired mutex at write time, no dead lock fields), **RULE NLR** (touch-it-fix-it on each migrated file), **RULE UFS** (retention bound `K`, segment/queue constants → named consts), **RULE VLT** (jwks key bytes never logged), **TGU** (tagged-union outcomes where applicable).
- **`docs/AUTH.md`** — auth-flow: the JWKS verifier's external behaviour (claims checked, refresh-on-`kid`-miss, error taxonomy) is unchanged; only its key-set synchronization changes.
- Bun attribution: the vendored `unbounded_queue.zig` carries a header crediting `oven-sh/bun` + the upstream path + commit, per the project's vendoring convention.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — every file is `*.zig` | atomics with `// safe because:`; tagged-union results; cross-compile both linux targets |
| PUB / Struct-Shape | yes — `common.Snapshot`, `common.Queue`, their re-exports | shape verdict per new pub surface; file-as-struct where one primary type |
| LIFECYCLE GATE | yes — snapshot holder + queue own heap | `init`/`deinit` pairing; `errdefer` on multi-step init; idempotency test |
| File & Function Length (≤350/≤50/≤70) | yes | one file per primitive; tests in `_test.zig` (FLL-exempt) |
| UFS | yes — retention `K`, queue/segment bounds, ordering constants | named constants; tests import them |
| LOGGING / ERROR REGISTRY | yes — refresh failure path | reuse existing JWKS error codes; no secret in any frame (VLT) |
| SCHEMA / UI / DESIGN TOKEN | no — no schema, no UI | — |

---

## Overview

**Goal (testable):** `jwks.Verifier.verify(token)` resolves the signing key via a single `@atomicLoad(.acquire)` of an immutable key-set snapshot with **no lock on the read path**; concurrent verification across N threads scales without contending a shared lock; a refresh publishes a new snapshot via `@atomicStore(.release)` under a write-only mutex and the prior snapshot is reclaimed via bounded retention with **zero use-after-free** (leak-checked, race-tested). The same RCU helper backs the logging-sink registry; the event/metrics/pool sites move to a lock-free queue/stack. Behaviour is byte-identical to the mutex versions.

**Problem:** the daemon synchronises read-mostly shared state with an **exclusive** `common.Mutex` (no `RwLock` exists in `common.sync`). The worst case is `jwks`: the RS256 key set, read on every authenticated request, refreshed every ~6 hours, sits behind one exclusive lock — every request serialises on it, and the lock's cache line bounces between cores under load. Producer/consumer sites (event bus, metrics) similarly fight a mutex where a lock-free queue belongs.

**Solution summary:** add an **RCU** primitive (`common.snapshot`) — lock-free atomic-load reads, a write-mutex on the rare publish, bounded-retention reclamation — and a **vendored lock-free queue** (`common.Queue`, from bun). Migrate the read-mostly sites (jwks, logging registry) to RCU and the producer/consumer sites (event hubs, metrics, pool idle list) to the queue/stack. Reads stop taking locks; only rare writers serialise; behaviour is unchanged.

---

## Prior-Art / Reference Implementations

- **RCU / atomic-snapshot** — the canonical read-copy-update pattern: readers atomic-load an immutable snapshot, writers publish a new one and defer reclamation. Reclamation here is **bounded retention** (keep the last `K` snapshots, free the oldest at each publish) — sound because read critical sections are nanoseconds and the publish interval is hours.
- **Bun** — `~/Projects/oss/bun/src/ptr/ref_count.zig` + `ptr/shared.zig` (the atomic-Arc family the read path's publish discipline mirrors) and `~/Projects/oss/bun/src/threading/unbounded_queue.zig` (the lock-free queue vendored here).
- **In-repo precedent** — `cache.zig` (M102_001) for shared-read + refcounted entries on the *keyed* credential path; RCU is the *single-snapshot* analogue. `src/lib/common/sync.zig` is the home the new `RwLock`-free primitives extend.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/lib/common/snapshot.zig` | CREATE | RCU atomic-publish helper: `load()` (lock-free), `publish()` (write-mutex + `@atomicStore(.release)`), bounded retention reclaim |
| `src/lib/common/unbounded_queue.zig` | CREATE | lock-free multi-producer single-consumer (MPSC)/multi-producer multi-consumer (MPMC) queue vendored from bun (attributed header) |
| `src/lib/common/constants.zig` | EDIT | re-export `Snapshot` + `Queue` from the `common` facade (next to `Mutex`/`Condition`) |
| `src/agentsfleetd/auth/jwks.zig` | EDIT | key set → `Snapshot`; reader is a single atomic-load; refresh publishes; retire the read-path mutex [**auth-critical**] |
| `src/agentsfleetd/events/bus.zig` · `subscription_hub.zig` · `subscription.zig` | EDIT | publish→subscriber handoff → lock-free queue; retire the mutex on the hot path |
| `src/agentsfleetd/observability/otel_metrics_cardinality.zig` · `metrics_redis_pool.zig` | EDIT | metric recording → lock-free queue; pool idle list → lock-free stack |
| `src/agentsfleetd/queue/redis_pool.zig` | EDIT | idle-connection list → lock-free stack/queue |
| `src/lib/logging/sinks.zig` | EDIT | sink **registry** (read per log, written at startup) → `Snapshot`; the per-sink buffer mutex is assessed separately |
| `tests/bench/*` | CREATE | read-path micro-benchmark (mutex vs RCU under N reader threads) per migrated read-mostly site |
| _colocated tests (`*_test.zig`, `test {}`)_ | CREATE/EDIT | one test per Dimension, incl. a concurrency/race test per migrated site |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** six Sections. §1 is the foundation (the primitive + the vendored queue), proven in isolation before any site uses it. §2–§6 migrate sites **hottest-first**, one per Section, each its own commit + tests + benchmark.
- **Alternatives considered:** (a) `Io.RwLock` shared reads — rejected: it still makes the *reader* take a lock (atomic RMW + cache-line bounce); RCU makes only the rare writer pay. (b) refcounted `AtomicShared` on the read path — rejected: the load-then-refcount step reintroduces the `atomic<shared_ptr>` race; RCU's plain atomic-load sidesteps it. (c) epoch/hazard-pointer reclamation — rejected: unnecessary when read critical sections are bounded-tiny vs the publish interval; bounded retention is simpler and provably safe here. (d) leave it as a mutex — rejected: the jwks lock is a measured scale ceiling on the busiest path.
- **Patch-vs-refactor verdict:** **targeted refactor** — swap the synchronization primitive site-by-site, behaviour preserved, each migration independently revertable.

---

## Sections (implementation slices)

### §1 — Foundation: `common.snapshot` (RCU) + vendored lock-free `Queue`
The RCU helper: `Snapshot(T)` holds an atomic pointer to an immutable `*const T`; `load()` is one `@atomicLoad(.acquire)` (no lock); `publish(new)` takes a write-only mutex, `@atomicStore(.release)`s the new pointer, and frees the `K`-th-oldest retained snapshot. The vendored `Queue` is a lock-free MPSC/MPMC queue (bun, attributed).
- **Dimension 1.1** — `load()` returns the most recently published snapshot and takes no lock (the read fn body contains no `.lock()`) → `test_snapshot_load_is_lockfree`
- **Dimension 1.2** — `publish()` makes the new snapshot visible to subsequent `load()`s; ordering is release/acquire → `test_snapshot_publish_visible`
- **Dimension 1.3** — bounded retention frees old snapshots with zero use-after-free and zero leak under `std.testing.allocator` (incl. readers loading during a publish) → `test_snapshot_reclaim_no_uaf`
- **Dimension 1.4** — the vendored `Queue` enqueues/dequeues correctly under concurrent producers (lock-free; no lost or duplicated items) → `test_queue_mpsc_concurrent`

### §2 — `jwks` key set → RCU (auth-critical)
The JWKS verifier holds its key set in a `Snapshot`; `verify()`'s key lookup is a lock-free `load()`; refresh-on-`kid`-miss builds a new immutable key set and `publish()`es it under the write mutex. External behaviour (claims, errors, refresh trigger) is unchanged.
- **Dimension 2.1** — `verify()` resolves a known `kid` with no lock on the read path; output identical to the pre-change verifier → `test_jwks_verify_lockfree_identical`
- **Dimension 2.2** — a `kid` miss triggers exactly one refresh+publish under the write mutex; concurrent misses do not double-publish a torn set → `test_jwks_refresh_single_publish`
- **Dimension 2.3** — N threads verifying concurrently during a refresh each see a valid (old or new) key set, never a freed/torn pointer → `test_jwks_concurrent_verify_during_refresh`

### §3 — Event hubs → lock-free queue
`events/bus.zig` + `subscription_hub.zig` + `subscription.zig`: the publish→subscriber handoff moves to the lock-free `Queue`; producers enqueue without a lock; the consumer drains. Delivery semantics + ordering preserved.
- **Dimension 3.1** — concurrent publishers enqueue without a lock; the consumer drains every event exactly once, in order → `test_event_bus_lockfree_delivery`
- **Dimension 3.2** — a subscriber added/removed concurrently with publishes loses no event and reads no freed node → `test_event_subscription_race`

### §4 — Metrics recording → lock-free queue
`otel_metrics_cardinality.zig` + the metric-recording path: recorders enqueue metric events lock-free; the flush consumer drains. Cardinality/values unchanged.
- **Dimension 4.1** — concurrent metric recorders enqueue without a lock; the flusher sees every recorded sample, values identical to the mutex version → `test_metrics_record_lockfree`

### §5 — Connection-pool idle list → lock-free stack/queue
`queue/redis_pool.zig` + `metrics_redis_pool.zig`: the idle-connection list becomes a lock-free stack (acquire = pop, release = push). Pool liveness + max-size semantics preserved; the mutex stays only where it guards genuine multi-field mutation that the stack cannot express (called out per site).
- **Dimension 5.1** — concurrent acquire/release never loses or double-hands-out a connection; idle count is consistent → `test_redis_pool_lockfree_idle`

### §6 — Logging sink registry → RCU
`src/lib/logging/sinks.zig`: the **registry** (read on every log, written at startup) holds its sink list in a `Snapshot`; logging reads lock-free. The per-sink buffer lock is assessed separately and kept if it guards write-side buffering (documented).
- **Dimension 6.1** — a log emit reads the sink registry lock-free; a sink registered concurrently with emits is either fully visible or fully absent, never partial → `test_log_registry_rcu`

---

## Interfaces

```
common.Snapshot(comptime T: type)   // RCU holder for an immutable *const T
  load(self)            -> *const T            // lock-free: one @atomicLoad(.acquire)
  publish(self, alloc, new: *const T) !void    // write-mutex; @atomicStore(.release); retain/reclaim
  deinit(self)                                  // frees all retained snapshots

common.Queue(comptime T: type)      // lock-free queue (vendored from bun, attributed)
  push(self, node) void            // lock-free producer
  pop(self) ?*Node                 // consumer
  deinit(self)

// jwks (UNCHANGED external surface):
jwks.Verifier.verify(token) -> VerifiedClaims | VerifyError   // key lookup now lock-free internally
```

The external surfaces of `jwks.Verifier`, the event bus, the metrics recorder, the redis pool, and the logger are **unchanged** — only their internal synchronization changes. Retention bound `K`, atomic orderings, and queue/stack capacities are named constants (RULE UFS). No HTTP endpoint, no wire format, no schema is touched.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Reader races a publish | a `verify()`/`log` loads while a refresh publishes | reader sees a complete old-or-new snapshot (release/acquire); never a torn or freed pointer |
| Use-after-free on reclaim | a snapshot freed while a reader still holds it | impossible by construction: bounded retention frees only the `K`-th-oldest, hours after its last possible reader (ns critical section); race-tested + leak-checked |
| Refresh fetch fails | JWKS endpoint 5xx / network | keep the current published snapshot; do not publish a partial/broken set; existing JWKS error path unchanged |
| Concurrent `kid` misses | two requests miss the same `kid` together | the write mutex serialises refresh; at most one publish; the second observes the freshly-published set |
| Producer/consumer race | concurrent enqueue/dequeue on the queue | lock-free queue: no lost, duplicated, or torn item; consumer drains all |
| OOM building a snapshot | allocator fails mid-rebuild | publish aborts, current snapshot stays live, error returned to the refresh caller; no half-published state |
| Pool exhaustion race | concurrent acquire on an empty idle list | lock-free pop returns null → existing "create/wait" path unchanged; no double-hand-out |
| Weak ordering bug | a missing acquire/release | guarded by the `// safe because:` audit + the concurrent race tests that fail on a visibility bug |

---

## Invariants

1. **No lock on the read hot path** — the migrated read fns (`jwks` key lookup, log registry read, `Snapshot.load`) contain **no** `.lock()`; enforced by a grep-gate test asserting the read-path functions are lock-free, plus the lock field's removal (RULE NDC).
2. **Every weak atomic ordering carries `// safe because:`** — enforced by the `dispatch/write_zig.md` self-audit grep (`\.(acquire|release|monotonic|acq_rel)` → comment within 3 lines) run in HARNESS VERIFY.
3. **Bounded retention** — the retention ring is a fixed `K` (named const, `comptime` size); a test proves memory does not grow across many publishes and the leak detector is clean.
4. **Published snapshots are immutable** — the holder stores `*const T`; the compiler rejects mutation after publish.
5. **Behaviour identical to the mutex version** — each migrated site has a test asserting byte-identical output vs the pre-change path (regression).
6. **No secret in any frame** — jwks key material is never logged (RULE VLT); only `kid`/status appears.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_snapshot_load_is_lockfree` | `load()` returns last `publish()`; read fn body has no `.lock()` |
| 1.2 | unit | `test_snapshot_publish_visible` | after `publish(B)`, every subsequent `load()` returns B (release/acquire) |
| 1.3 | unit | `test_snapshot_reclaim_no_uaf` | N reader threads loading across many publishes → zero leak, zero use-after-free (`std.testing.allocator`) |
| 1.4 | unit | `test_queue_mpsc_concurrent` | M producers push K items each; consumer pops exactly M·K, none lost/duplicated |
| 2.1 | unit | `test_jwks_verify_lockfree_identical` | a token with a known `kid` verifies to the same claims as the mutex version; read path lock-free |
| 2.2 | integration | `test_jwks_refresh_single_publish` | concurrent `kid` misses → exactly one refresh+publish; no torn key set |
| 2.3 | integration | `test_jwks_concurrent_verify_during_refresh` | N verifiers during a publish each get a valid set; leak-checked |
| 3.1 | unit | `test_event_bus_lockfree_delivery` | concurrent publishers → consumer drains every event once, in order |
| 3.2 | integration | `test_event_subscription_race` | sub add/remove during publishes → no lost event, no freed-node read |
| 4.1 | unit | `test_metrics_record_lockfree` | concurrent recorders → flusher sees every sample, values identical to mutex version |
| 5.1 | integration | `test_redis_pool_lockfree_idle` | concurrent acquire/release → no lost/double-handed connection; idle count consistent |
| 6.1 | unit | `test_log_registry_rcu` | concurrent register + emit → registry read lock-free; sink fully visible or absent |

**Regression:** each migrated site keeps a test asserting identical external output to the pre-change (mutex) version — this is the safety net for a behaviour-preserving refactor. **Idempotency/replay:** repeated `publish()` retains/reclaims without growth; repeated drain on an empty queue is a no-op. **Benchmark (Verification Evidence, not a pass/fail Dimension):** a read-path micro-benchmark (`tests/bench/`) records throughput under N reader threads, mutex vs RCU, to evidence the win.

---

## Acceptance Criteria

- [ ] `common.snapshot` + vendored `Queue` pass unit + concurrency + leak tests — verify: `make test && make memleak`
- [ ] jwks read path is lock-free and output-identical; refresh single-publishes — verify: `make test && make test-integration`
- [ ] event/metrics/pool/logging sites migrated, each behaviour-identical + race-tested — verify: `make test-integration`
- [ ] read-path benchmark shows RCU throughput scaling past the mutex baseline under N readers — verify: the `tests/bench` delta in Verification Evidence
- [ ] every weak atomic ordering has a `// safe because:` comment — verify: `git diff -U0 origin/main -- '*.zig' | grep -E '\.(acquire|release|monotonic|acq_rel)'` each annotated
- [ ] no retired mutex field left dead (RULE NDC) — verify: orphan grep clean
- [ ] Cross-compile clean — verify: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `make lint` clean · `gitleaks detect` clean · no non-md file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: unit + integration + memleak (RCU has zero use-after-free across publishes)
make test && make test-integration && make memleak 2>&1 | tail -5
# E2: cross-compile both targets
zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux && echo "XC PASS"
# E3: every weak atomic ordering is annotated (empty = pass)
git diff -U0 origin/main -- '*.zig' | grep -E '^\+.*\.(acquire|release|monotonic|acq_rel)\b' \
  | grep -v 'safe because' | head
# E4: lint + gitleaks
make lint 2>&1 | grep -E "✓|FAIL"; gitleaks detect 2>&1 | tail -3
# E5: read-path benchmark (records the throughput delta, mutex vs RCU)
zig build -Dwith-bench-tools bench 2>&1 | tail -20
```

---

## Dead Code Sweep

**1. Orphaned files** — none deleted; this is an in-place primitive swap. New files per Files Changed.

**2. Orphaned references** — after each migration, the retired mutex field/lock calls must be gone (RULE NDC).

| Removed/renamed symbol | Grep | Expected |
|------------------------|------|----------|
| jwks read-path `mutex` lock | `grep -n "mutex" src/agentsfleetd/auth/jwks.zig` | only the write-side refresh mutex remains; no read-path lock |
| logging registry `sinks_mutex` on the read path | `grep -n "sinks_mutex" src/lib/logging/sinks.zig` | only write-side registration; reads are lock-free |
| dead lock fields on migrated structs | `grep -rn "common.Mutex" src/agentsfleetd/events src/agentsfleetd/observability \| head` | only sites where an exclusive mutex is still correct (documented) |

---

## Discovery (consult log)

> **Empty at creation.** Populate as work surfaces consults, skill outcomes, and Indy-acked deferrals.

- **Origin (Indy + Orly/CTO, Jun 26 2026):** surfaced during M102_001's broker review. Indy challenged the exclusive-lock read path; the CTO argument landed on **RCU (lock-free reads, write-mutex on the rare publish, bounded retention)** as strictly better than `Io.RwLock` for read-mostly state, and **lock-free queues** for producer/consumer sites. Indy: confirmed the architecture and directed this be specced as its own workstream in the M102 worktree.
- **Why not RwLock (recorded so a reviewer can check the call):** RwLock makes the *reader* take a shared lock (atomic RMW + cache-line bounce) to guard against a writer 6 hours apart; RCU pays only the rare writer. The `atomic<shared_ptr>` race that motivates refcounted Arc is sidestepped because RCU's read is a plain atomic-load (no refcount). Correctness rests on read critical sections being bounded-tiny vs the publish interval — true for the jwks `kid` lookup (pure CPU) and the log-registry read.
- **Scope boundary (Indy-directed):** lands in the M102 worktree as separate commits/sections, reviewed apart from the credential broker; jwks reviewed hardest (auth boundary).
- **Bench requirement:** each read-mostly migration carries a micro-benchmark recording the mutex-vs-RCU read-throughput delta into Verification Evidence — the only externally-visible artefact of an otherwise behaviour-preserving change.
- **Perf-proof directive (Indy, Jun 28 2026):** the verification bar is raised from "record a delta" to "**prove the concurrency win**". Every migrated hot path — jwks, event bus, metrics, **and redis_pool** — gets a dedicated concurrent benchmark written against its *public* API (unchanged by this workstream, so the same bench exe compiles on both branches). The proof method is **`main` (mutex) vs this `m102` branch (RCU/lock-free)**, measured under N reader/producer threads `[1,2,4,8,16]`, with the deltas captured into Verification Evidence and a non-regression assertion added so the win can't silently erode. Verbatim ack: _"i think you will have to build the relevant testing even redis_pool has an impact i think comparing between main and this m102 branch. You can now continue with these answers in mind."_ — context: how to benchmark a before/after when the mutex is deleted from the hot path.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs this Test Specification (≥50% negative; every Failure Mode + concurrency race covered) | Clean; iteration count in Discovery |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec, `docs/AUTH.md` (jwks), `dispatch/write_zig.md` (atomics/lifecycle), Failure Modes, Invariants (esp. no-lock-on-read + no-use-after-free) | Clean or every finding dispositioned |
| After `gh pr create` | `/review-pr` | Review-comments the open PR | Comments addressed before human review/merge |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit | `make test` | {paste snippet} | |
| Integration | `make test-integration` | {paste snippet} | |
| Memleak (no use-after-free across publishes) | `make memleak` | {paste snippet} | |
| Read-path benchmark (mutex vs RCU) | `zig build -Dwith-bench-tools bench` | {paste delta} | |
| Cross-compile (Zig) | `zig build -Dtarget=x86_64-linux && -Dtarget=aarch64-linux` | {paste snippet} | |
| Lint | `make lint` | {paste snippet} | |
| Gitleaks | `gitleaks detect` | {paste snippet} | |
| Atomic-ordering annotations | E3 grep above | {paste snippet} | |

---

## Out of Scope

- **Epoch-based / hazard-pointer reclamation** — the bounded-retention grace suffices while read critical sections stay bounded-tiny; revisit only if a future read path blocks or allocates.
- **Sites where an exclusive mutex is correct** — write-heavy / multi-field-mutation pool internals that a lock-free stack cannot express stay on the mutex; each kept mutex is documented as deliberate.
- **A generic lock-free hashmap** — read-mostly state here is a single snapshot; the keyed case is already covered by `cache.zig` (M102_001).
- **Behaviour changes** — this workstream changes synchronization only; any semantic change to auth, events, metrics, pools, or logging is a separate spec.
- **`common.RwLock`** — deliberately not added; RCU + the lock-free queue cover the read-mostly and producer/consumer shapes without it.
