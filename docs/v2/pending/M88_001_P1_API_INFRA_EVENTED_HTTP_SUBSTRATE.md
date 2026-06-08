# M88_001: Evented HTTP substrate for zombied (connection density for SSE + runner long-poll)

**Prototype:** v2.0.0
**Milestone:** M88
**Workstream:** 001
**Date:** Jun 08, 2026
**Status:** PENDING
**Priority:** P1 — one zombied caps out on threads held by idle long-lived connections (Server-Sent Events viewers, runner long-polls), not on CPU; this lifts the per-node ceiling.
**Categories:** API, INFRA
**Batch:** B1
**Branch:** feat/m88-scale-zombied-runner
**Depends on:** none for the spike; composes with M84_002 (its advisory-lock single-flight is what makes the horizontal-replica story safe, but is not required to land this).
**Provenance:** LLM-drafted (Opus 4.8, Jun 08 2026) — from the CEO-reviewed plan `are-these-threads-async-iterative-willow.md`, owner-directed (Indy: "I wanna go async, httpz is slow").

> **Provenance is load-bearing.** LLM-drafted — cross-check every claim against the codebase. The substrate (libxev) is a new dependency whose shape this spec defines.

> **⚠️ STATUS — GATED, not the flagship (updated Jun 08, 2026).** A code-grounded eng review + a codex adversarial pass reframed this milestone. The felt "httpz is slow" pain was a config default: `API_HTTP_THREADS`/`API_HTTP_WORKERS` shipped at **1/1** on the prod 1gb box, so a single Server-Sent Events (SSE) stream saturated the per-worker handler pool. That is **fixed by config** — the boring lever, now landed (fly.toml `[env]` raising the handler pool to 32 on a single worker, + playbooks). This async-substrate workstream is **deferred behind an evidence gate** (see Discovery, Jun 08): implement only when metrics prove per-node SSE-tail density is the binding constraint *after* the thread bump, a larger VM, and horizontal replicas — and even then the design below needs the §2 fix. Several original premises were wrong (httpz is already evented at the connection layer; the runner lease does NOT pin a thread). Do **not** CHORE(open) this without re-reading Discovery.

**Canonical architecture:** `docs/architecture/scaling.md` (the binding constraint — API replicas + Postgres write throughput — and the `API_HTTP_THREADS/WORKERS` handler-pool knobs, which this spec reconciles with) + `docs/architecture/data_flow.md` (the synchronous request → handler → pg/redis write model this preserves). A NEW `docs/architecture/concurrency.md` (the event-loop + blocking-worker-pool model) is **deferred with this workstream**, created only if the gate opens. The threaded executor seam is `src/lib/common/sync.zig:8`.

---

## Implementing agent — read these first

1. `src/zombied/http/server.zig` — the current httpz `Server(App)` init (worker/thread-pool config). This is the integration point being replaced; mirror its `Context`/handler wiring onto the new substrate.
2. `src/zombied/cmd/serve.zig` (≈320-367) — how the server is started/stopped and how `signal`, `event bus`, `approval sweeper` threads are spawned + joined. The new loop owns the same lifecycle.
3. `src/zombied/http/handlers/zombies/events_stream.zig`, `…/create_stream.zig` — the SSE streams, the ONLY long-lived handlers that pin a per-worker handler thread today; the sole migration beneficiaries. (`src/zombied/http/handlers/runner/lease.zig` is NOT in scope — it is a non-blocking single poll, not a long-poll; see Discovery Jun 08.)
4. `src/zombied/queue/redis_pool.zig` + `src/lib/common/sync.zig` — the blocking pools and the `globalIo()` seam; pg/redis stay blocking behind a worker pool the loop dispatches to.
5. https://github.com/mitchellh/libxev + https://github.com/tardy-org/zzz — the substrate to adopt (event loop + async HTTP framework). NOT Zig's built-in `std.Io.Evented` (experimental; upstream advises Threaded for critical workloads).

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Adopt evented HTTP substrate for zombied SSE + runner long-poll
- **Intent (one sentence):** Stop pinning an operating-system thread per idle long-lived connection so one zombied serves many more concurrent Server-Sent Events viewers and runner long-polls, without rewriting the data layer.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent + `ASSUMPTIONS I'M MAKING: …`. Load-bearing assumptions: (1) substrate is libxev + zzz/tardy, not `std.Io.Evented`; (2) pg AND redis stay blocking behind a bounded worker pool — no async client is written; (3) only SSE + runner long-poll endpoints migrate in this workstream; the rest stay on the threaded path until a follow-on; (4) the spike's measured win gates the cutover. Mismatch → STOP and reconcile.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal discipline; NLG (no "legacy" framing for the retired httpz path pre-2.0).
- **`dispatch/write_zig.md`** — diff is `*.zig`: pg-drain lifecycle on any handler touched, tagged-union results, multi-step `errdefer`, file ≤350 / fn ≤50, cross-compile both linux targets.
- **`docs/LIFECYCLE_PATTERNS.md`** — the loop + worker-pool start/stop/join lifecycle mirrors the existing background-thread lifecycle in `serve.zig`.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — handler signature + route registration parity (the public HTTP contract must not change).
- **`docs/LOGGING_STANDARD.md`** — connection lifecycle + offload events logged via the logfmt envelope; never log secrets.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | cross-compile x86_64-linux + aarch64-linux; tagged-union results on the new loop/offload API. |
| PUB / Struct-Shape | yes | shape verdict for the new server + offload-pool public surface (one façade module). |
| File & Function Length (≤350/≤50/≤70) | yes | the loop bootstrap + offload pool factor into helpers; split before the cap. |
| LIFECYCLE | yes | loop + worker-pool init/deinit, `errdefer` placement; clean shutdown joins all workers. |
| LOGGING | yes | structured connection/offload logs; no secret/token in logs. |
| UFS | yes | offload-pool size + timeouts as named constants; shared verbatim where cross-referenced. |
| SCHEMA / ERROR REGISTRY | no | no schema or new error codes in this workstream. |

---

## Overview

**Goal (testable, gated):** when per-node concurrent SSE-tail density is the proven binding constraint (after `API_HTTP_THREADS`, VM size, and replicas are exhausted), zombied serves SSE streams on an event loop where a parked stream costs a file descriptor, not a handler thread — a load test holds ≥2000 concurrent idle SSE connections on a small fixed worker + memory budget with stable p99, where the thread-per-SSE-handler path is bounded by `API_HTTP_THREADS × per-thread memory`. The runner lease is **out of scope** — it is already a non-blocking single poll.

**Problem (corrected — see Discovery Jun 08):** httpz is **already evented at the connection layer**: `API_HTTP_WORKERS` accept/event-loop threads (epoll/kqueue) multiplex idle connections as file descriptors. What pins a thread is a **long-lived handler**, and there is exactly one — the SSE stream (`events_stream.zig` runs an inline read/write loop on a per-worker handler-pool thread for the connection's life). The runner lease is a **non-blocking single poll** that returns immediately (`assign.zig`: "no server-side long-poll loop"; `redis_zombie.zig`: `XREADGROUP` WITHOUT `BLOCK`) — it does NOT pin a thread. The per-node SSE ceiling is therefore `API_HTTP_THREADS` (per worker), which shipped at the default of **1**, so a single SSE viewer saturated the pool. Raising it to 32 (config, landed) lifts the ceiling without touching the substrate; this workstream earns its keep only when 32+ concurrent SSE tails per node become the wall after the box and replica levers.

**Solution summary:** Adopt an event-loop HTTP substrate (libxev + zzz/tardy) for the connection/accept layer so idle connections cost a file descriptor, not a thread. Keep pg and redis blocking behind a bounded worker pool the loop dispatches to and awaits, so the data layer and its 566 call sites are untouched. Migrate the SSE endpoints only (the sole connection-density win — the runner lease is already non-blocking); leave all other endpoints on the threaded path until a follow-on workstream.

---

## Prior-Art / Reference Implementations

- **Substrate** → `libxev` (mitchellh; powers Ghostty's event loop on io_uring/epoll/kqueue) for the loop; `zzz`/`tardy` for the async HTTP framing (benchmarked materially faster than http.zig at high connection counts). Name the alignment in PLAN; justify any divergence.
- **Integration point** → existing `src/zombied/http/server.zig` httpz wiring is the pattern to mirror (Context, routes, handler signature) so the public contract is unchanged.
- **Offload pattern** → the existing blocking pools (`redis_pool.zig`, pg pool) are reused as-is behind the loop; this is a hybrid, not a rewrite.
- New shape recorded in `docs/architecture/concurrency.md` (greenfield substrate doc).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `build.zig`, `build.zig.zon` | EDIT | add libxev + zzz/tardy dependencies |
| `src/zombied/http/server.zig` | EDIT | swap the httpz `Server(App)` for the evented substrate; preserve Context/routes/handler signature |
| `src/zombied/http/offload.zig` | CREATE | bounded blocking worker pool the loop dispatches pg/redis calls to and awaits |
| `src/zombied/cmd/serve.zig` | EDIT | start/stop the event loop + offload pool; same lifecycle as today's threads |
| `src/zombied/http/handlers/zombies/events_stream.zig`, `…/create_stream.zig` | EDIT | run SSE on the loop; emit per event via the loop, blocking redis reads offloaded |
| `src/zombied/http/handlers/runner/lease.zig` | NONE | NOT in scope — already a non-blocking single poll, does not pin a thread (Discovery Jun 08) |
| `vendor/httpz` | EDIT/KEEP | retained for the not-yet-migrated endpoints during the transition; removed in the follow-on |
| `docs/architecture/concurrency.md` | CREATE | the event-loop + blocking-worker-pool model + the offload seam |

> Line numbers/symbols omitted by design — the agent reads current code.

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** a contained substrate swap at the accept/serve layer + an offload pool, migrating only the two connection-density endpoints. Everything else (handlers, pg, redis) is reused.
- **Alternatives considered:** (a) full big-bang migration of all endpoints + an async pg client — rejected: no mature async Zig Postgres client exists, and it would rewrite 566 query sites on an unproven win; (b) Zig built-in `std.Io.Evented` at the `sync.zig:8` seam — rejected: experimental, upstream advises Threaded for critical workloads; (c) stay threaded, scale only horizontally — valid and complementary (separate workstream) but does not fix per-node waste on idle long-lived connections.
- **Patch-vs-refactor verdict:** **scoped refactor** of one layer (accept/serve) + an additive offload pool. The broad endpoint cutover and httpz removal are a named follow-on, not silently bundled here.

---

## Sections (implementation slices)

### §1 — Baseline + substrate spike (the gate)

Measure the **tuned** httpz ceiling and prove the evented substrate clears it on our workload before any cutover. **Implementation default:** spike one SSE endpoint on libxev+zzz; compare against httpz **at `API_HTTP_THREADS=32` (the shipped config), NOT the default of 1** — a default-1 baseline is a strawman that measures the config lever, not the substrate. Record per-thread memory so the headline reads "fd-cost vs thread-cost", not "32 vs 1". The win must be real on our endpoint, net of the config lever. The runner lease is excluded (already non-blocking).

- **Dimension 1.1** — under a load holding many idle SSE connections, the tuned httpz path (`API_HTTP_THREADS=32`) saturates its handler pool and queues new requests at ~32 concurrent SSE tails → Test `baseline_sse_handler_pool_saturates`.
- **Dimension 1.2** — the evented substrate holds ≥2000 idle SSE connections on a small fixed worker + memory budget with stable p99 and accepts new requests → Test `evented_holds_many_idle_connections`.

### §2 — Blocking pg/redis offload seam

The loop must never block on a pg/redis call. **Implementation default:** option (a) offload-and-await — each blocking pg/redis call is dispatched to a bounded worker pool and the connection's task awaits the result; pg/redis client code is unchanged. (Option (b), running handler bodies on a worker-thread scheduler, is the fallback if the spike shows (a)'s per-call overhead dominates; the spike decides.)

> **Design hole (codex, Jun 08): offloading the SSE subscriber read does NOT deliver SSE density.** The SSE stream's defining cost is the dedicated Redis `SUBSCRIBE` socket it blocks on. If that blocking read is merely dispatched to the offload pool, **each parked stream pins one offload-pool thread for its whole life** — the thread-pin moves from the httpz handler pool to `offload.zig`, same bucket, zero density win. The offload seam is correct for *request-path* pg/redis calls (acquire → query → release), but for SSE to cost a file descriptor instead of a thread **the subscriber socket itself must be registered on the event loop** (libxev-native Redis pub/sub read), which is a larger change than "redis reads behind the offload pool". The spike MUST resolve this before any cutover; if evented pub/sub is infeasible, this workstream does not achieve its goal.

- **Dimension 2.1** — a handler issuing a blocking pg query on the loop completes without stalling concurrent connections (a slow query on one connection does not freeze others) → Test `offload_does_not_block_loop`.
- **Dimension 2.2** — the offload pool is bounded; saturation applies backpressure (callers wait) rather than unbounded thread growth → Test `offload_pool_bounded_backpressure`.

### §3 — Migrate SSE + runner long-poll to the loop

Move the two connection-density endpoints onto the substrate; the public HTTP contract is byte-for-byte unchanged. **Implementation default:** SSE event delivery is driven by the loop; redis reads behind the offload pool.

- **Dimension 3.1** — SSE stream emits the same `text/event-stream` framing/ordering/reconnection as today, verified against the existing SSE test suite → Test `sse_contract_unchanged_on_loop`.
- **Dimension 3.2** — **REMOVED.** The runner lease is already a non-blocking single poll that returns immediately (`assign.zig`, `redis_zombie.zig`: `XREADGROUP` WITHOUT `BLOCK`); it does not pin a thread and is not a long-lived connection. The original premise was false (see Discovery Jun 08). Only the SSE stream (§3.1) migrates in this workstream.

### §4 — Lifecycle parity (start/stop/drain)

The loop + offload pool start and stop exactly like today's threaded lifecycle. **Implementation default:** reuse the `shutdown_requested` flag + join semantics from `serve.zig`; in-flight connections drain before exit.

- **Dimension 4.1** — on shutdown signal, the loop stops accepting, drains in-flight requests/streams, joins the offload pool, and exits cleanly (no leaked tasks/threads) → Test `evented_clean_shutdown`.

---

## Interfaces

```
Public HTTP contract: UNCHANGED.
  GET  (SSE) zombie event streams  → text/event-stream, same framing/ordering/reconnect.
  POST /v1/runners/me/leases       → same long-poll lease/no-work response shape.
Internal (new, this workstream):
  http/server: evented Server over libxev/zzz exposing the SAME Context + route + handler signature
               as the current httpz Server(App) (drop-in for callers).
  http/offload: bounded blocking worker pool — submit(fn, args) → awaitable result; bounded size;
               backpressure on saturation; clean drain on shutdown.
pg / redis client code, request/response shapes, route table: UNCHANGED.
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Blocking call on loop thread | handler calls pg/redis directly | must route via offload; a slow query delays only its own connection, not the loop → Test 2.1 |
| Offload pool saturated | more in-flight blocking calls than pool slots | bounded backpressure (callers wait); no unbounded thread growth; surfaced in metrics → Test 2.2 |
| SSE client vanishes mid-stream | network drop / tab closed | loop detects closed fd, frees the connection + its offload state; no leak |
| Shutdown with live SSE/long-poll | signal during many open connections | drain in-flight, stop accepting, join offload pool, exit clean → Test 4.1 |
| Substrate spike underperforms | (a) offload overhead dominates | spike falls back to seam option (b) before any broad cutover (§2 default) |
| io_uring unavailable on host kernel | older Linux | libxev falls back to epoll; behaviour unchanged, lower ceiling — logged at startup |

---

## Invariants

1. **Public HTTP contract unchanged** — same routes, request/response shapes, SSE framing — enforced by reusing the existing handler signatures + the existing SSE/lease test suites passing unmodified.
2. **The event loop never makes a blocking pg/redis call** — enforced by the offload seam (§2) + Test 2.1 (a slow query must not freeze concurrent connections).
3. **Offload pool is bounded** — no unbounded thread growth under load — enforced by a fixed pool size + Test 2.2 backpressure.
4. **No async Postgres/redis client is introduced** — pg/redis client code is byte-identical — enforced by diff review (those files untouched) + the orphan/ blast-radius table.
5. **Clean shutdown leaks nothing** — loop tasks + offload threads all joined — enforced by Test 4.1 + leak audit.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `baseline_thread_per_conn_saturates` | httpz refuses/queues new requests once idle held conns reach worker count. |
| 1.2 | integration | `evented_holds_many_idle_connections` | ≥2000 idle SSE conns on small fixed workers; new requests still accepted; p99 stable. |
| 2.1 | integration | `offload_does_not_block_loop` | one slow pg query on conn A does not delay conn B on the loop. |
| 2.2 | integration | `offload_pool_bounded_backpressure` | calls beyond pool size wait; thread count stays bounded. |
| 3.1 | integration | `sse_contract_unchanged_on_loop` | SSE framing/ordering/reconnect identical to current suite. |
| 3.2 | — | **REMOVED** | runner lease is already a non-blocking single poll; not in scope (see §3.2). |
| 4.1 | integration | `evented_clean_shutdown` | signal → drain → join → exit; zero leaked tasks/threads. |

**Regression:** the full existing zombied HTTP/SSE/lease suites must pass unchanged on the migrated endpoints (contract parity). **Idempotency:** lease long-poll retry semantics unchanged. Load fixtures live under `samples/fixtures/m88-fixtures/`.

---

## Acceptance Criteria

- [ ] Evented substrate holds ≥2000 idle SSE conns on a small fixed worker count; httpz baseline saturates earlier — verify: `make test-integration` + the load harness in §1
- [ ] SSE + lease long-poll contracts byte-unchanged — verify: `make test-integration`
- [ ] Loop never blocks on pg/redis; offload bounded — verify: `make test-integration` (Tests 2.1/2.2)
- [ ] Clean shutdown, no leaks — verify: `make test-integration` (Test 4.1) + `make memleak`
- [ ] `make lint` clean · `make test` passes
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `gitleaks detect` clean · no non-`.md` file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: connection-density win
make test-integration 2>&1 | grep -iE "evented|idle_connection|saturat|offload|sse|longpoll" | tail -20
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

**1. Orphaned files — deleted from disk and git.** N/A this workstream — httpz is retained for the not-yet-migrated endpoints; its removal is the follow-on workstream's Dead Code Sweep.

**2. Orphaned references — zero remaining.** N/A — no symbols removed here (additive substrate + two endpoint migrations).

---

## Discovery (consult log)

> **Empty at creation.** Append as work surfaces consults/decisions.

- **Provenance (Jun 08 2026)** — CEO-reviewed plan; owner-directed async. Off-the-shelf scaling (replicas + PgBouncer + cache) and the runner worker-thread pool are sibling M88 workstreams, sequenced separately.
- **Eng review + codex adversarial pass (Jun 08 2026) — premises corrected, milestone gated.**
  - **httpz is already evented at the connection layer.** `server.zig` configures two pools: `API_HTTP_WORKERS` accept/event-loop threads (epoll/kqueue, multiplexing idle connections as fds) and a `API_HTTP_THREADS` handler pool. The original "thread-per-connection" framing was wrong.
  - **The handler pool is PER worker** (`vendor/httpz/src/worker.zig:466`, instantiated inside `Worker.init`, looped per worker in `httpz.zig:421`). Total handler concurrency = `WORKERS × THREADS`. **Prod** runs `WORKERS=2 / THREADS=32 = 64` handler threads on its 4-core / 4gb machines — 2 workers for accept parallelism across cores; **dev** runs `WORKERS=1 / THREADS=32 = 32` on its single 512mb box. Tradeoff acknowledged: per-worker pools can fragment (an SSE-heavy worker strands the other worker's idle threads), accepted on a multi-core box for the accept parallelism — a fractional-core box should stay at 1 worker.
  - **Prod shipped at the default `1/1`** (`runtime_loader.zig:40-41`, no fly.toml override), so one SSE stream saturated the handler pool. **Fixed by config** (fly.toml `[env]` + deploy playbooks, this branch) — the boring lever and the actual fix for the felt "httpz is slow" pain at current scale.
  - **The runner lease does NOT pin a thread.** `assign.zig:3` "no server-side long-poll loop; the runner re-polls via `retry_after_ms`"; `redis_zombie.zig:1` "XREADGROUP ... WITHOUT BLOCK". The lease returns immediately. The original Problem/Goal and §3.2 claim that it pins a worker per waiting runner is false. The ONLY long-lived-handler thread-pinner is the SSE stream.
  - **The offload-pool design does not solve SSE density** (see §2 design hole): the SSE subscriber's blocking `SUBSCRIBE` read must become evented on the loop, not be offloaded — offloading just moves the thread-pin to `offload.zig`.
  - **Reconciled with `docs/architecture/scaling.md`** (canonical): post-M80_002 the binding constraint is `zombied` API replicas + Postgres write throughput, and SSE is the only dedicated-Redis long-lived tier. Async connection-density sits below those on the ROI ranking. `scaling.md` now documents the `API_HTTP_THREADS/WORKERS` knobs as the first SSE-density lever.
  - **Gate to CHORE(open) this workstream:** metrics prove per-node concurrent SSE-tail density is the binding constraint AFTER (1) `API_HTTP_THREADS` raised, (2) VM sized up, (3) horizontal replicas (M84_002 single-flight) — AND per-thread memory is the proven ceiling. Until then this stays PENDING.
- **Deferrals** — the entire async substrate is deferred behind the gate above (owner-directed reframe — Indy, Jun 08 2026: pull the config lever, gate async on evidence). `docs/architecture/concurrency.md` creation is deferred with it.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Coverage vs this Test Specification (esp. offload-does-not-block, contract parity, clean shutdown). | Clean; counts in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial review vs spec, `dispatch/write_zig.md`, LIFECYCLE, Failure Modes, Invariants (esp. "loop never blocks", "contract unchanged"). | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open Pull Request (PR). | Comments addressed before merge. |

---

## Verification Evidence

> Filled during VERIFY.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Connection density | `make test-integration` | {paste} | |
| Contract parity (SSE/lease) | `make test-integration` | {paste} | |
| Offload non-blocking + bounded | `make test-integration` | {paste} | |
| Clean shutdown / leaks | `make memleak` | {paste} | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` | {paste} | |

---

## Out of Scope

- **Broad endpoint cutover + httpz removal** — only SSE + runner long-poll migrate here; the rest stay threaded until a follow-on M88 workstream (which owns httpz's Dead Code Sweep).
- **Async-native Postgres/redis client** — pg/redis stay blocking behind the offload pool; an async client is built only if later measurement proves pg I/O is the next ceiling.
- **Folding signal watcher / event bus / approval + liveness sweepers onto libxev** — a follow-on once the loop is proven; they stay on `std.Thread` for now.
- **Horizontal replicas + PgBouncer + cache (Layers 1-3)** — sibling M88 workstreams.
- **Runner worker-thread pool (~100 agents/host)** — sibling M88 workstream.
