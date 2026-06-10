<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh,
  which also assert the determinism-critical sections below are present and filled (not left as {placeholders}).
-->

# M90_001: Blocking-wait & race hardening — bounded waits, torn-write-free telemetry, rotation-safe auth

**Prototype:** v2.0.0
**Milestone:** M90
**Workstream:** 001
**Date:** Jun 10, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — three production-reachable defects: shutdown deadlock, auth outage on signing-key rotation, unkillable runner child past its lease deadline
**Categories:** API, OBS
**Batch:** B1 — lands first; M90_002 builds on the gate-outcome seam this workstream reshapes
**Branch:** feat/m90-001-hardening
**Test Baseline:** unit=1886 integration=164 (recorded at the §7–§10 scope amendment — the spec predates the CHORE(open) baseline convention; counts are the `make _lint_zig_test_depth` evidence row at `b720a78b`)
**Depends on:** none
**Provenance:** LLM-drafted (Claude Fable 5, Jun 10, 2026) — from the Jun 10 full audit of `src/lib`, `src/zombied`, `src/runner`

**Canonical architecture:** `docs/architecture/runner_fleet.md` (lease/renewal timing), `docs/architecture/observability.md` (OpenTelemetry (OTel) export path), `docs/AUTH.md` (JSON Web Key Set (JWKS) verify policy — the doc already promises refresh-on-kid-miss; this workstream makes the code conform, no doc change).

---

## Implementing agent — read these first

1. `src/zombied/observability/metrics_workspace.zig` — the in-repo compare-and-swap (CAS) slot pattern (occupied flag + ready flag, RULE CAS) the OTel rings must adopt.
2. `vendor/pg/CHANGES.md` + the patched `Pool.acquire` — summed-slice deadline math immune to wall-clock steps; mirror for any new wait-with-deadline.
3. `src/zombied/fleet/approval_gate.zig` + `src/zombied/zombie/approval_gate.zig` — the current synchronous gate flow being made async; the `UPDATE … WHERE status='pending'` single-winner resolution already exists and is reused, not reinvented.
4. `src/runner/daemon/renew_driver.zig` + `src/runner/daemon/loop.zig` — the tick loop whose deadline enforcement must never be starved by a blocked network call.
5. `docs/AUTH.md` — the JWKS caching/refresh behaviour the code must match verbatim.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `fix(m90): bound every blocking wait — bus stop, OTel rings, JWKS rotation, pool queue, SSE fan-out, admission, drain`
- **Intent (one sentence):** No zombied or runner thread can hang unboundedly, lose a wakeup, tear a telemetry write, 401 valid users after an identity-provider key rotation, starve behind a parked pool worker, exhaust Redis connections per viewer, shed an ops probe, or strand a live stream at shutdown.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate the intent and list `ASSUMPTIONS I'M MAKING: …`; mismatch with the Intent above → STOP and reconcile.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — **RULE CAS** (ring slots), **RULE ECL** (timeout ≠ fatal ≠ retryable on control-plane calls), **RULE TIM** (heartbeat < socket timeout < proxy idle; new cap/timeout relations documented), **RULE OBS** (every shed/reject/stale-serve branch logs), **RULE UFS** (new knobs and caps are named consts), **RULE TGU** (gate outcome variants), **RULE EMS/ERR** (new registry codes for 429/503 responses), **RULE HLP** (no helper without a consumer), **RULE TST/TST-NAM**, **RULE XCC**.
- `dispatch/write_zig.md` — Concurrency (`// safe because:` on every weak ordering touched), Panic/Hang/Shutdown policy (every wait names its wake path), Multi-Step Init errdefer, Resource Budget (new caps/queues), SSE Heartbeat Timing, Listener-shutdown wake.
- `docs/AUTH.md` — JWKS verify contract section, before touching `src/zombied/auth/**`.
- `docs/LOGGING_STANDARD.md` — scoped-logger shape for new branches.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — all-Zig diff | read façade; cross-compile both linux targets + linux test graphs |
| PUB / Struct-Shape | yes — new pub surface (batcher, gate outcome variant) | per-surface shape verdict before each new `pub` |
| File & Function Length | yes — `jwks.zig`/`approval_gate.zig` are mid-size | split per Module Split Pattern if a file approaches 350 |
| UFS | yes — new caps, timeouts, retry-after values | named consts at ownership site; grep before declaring |
| LOGGING | yes — new reject/stale/timeout branches | `log.scoped` + `<scope>.<state> key=value` per RULE OBS |
| LIFECYCLE | yes — persistent runner HTTP client | init/deinit pairing + errdefer chain |
| ERROR REGISTRY | yes — backpressure + SSE-cap response codes | new `UZ-*` entries via registry conventions |
| SCHEMA / UI / DESIGN TOKEN | no — no SQL, no UI | N/A |

---

## Overview

**Goal (testable):** Event-bus `stop()` always joins its consumer; every runner control-plane call carries a deadline and a blocked renewal can never delay the child kill; JWKS verifies tokens minted after a key rotation without restart; concurrent OTel log/trace pushes never tear an entry; HTTP dispatch sheds load at the configured in-flight ceiling and Server-Sent Events (SSE) streams are capped below thread-pool starvation. **§7–§10 (amendment):** no httpz pool worker can black-hole queued requests; SSE viewers cost one shared Redis pub/sub connection process-wide, not one each; ops probes are never shed; live streams drain at shutdown and are listable by operators.

**Problem:** Shutdowns can wedge forever on a lost wakeup (`events/bus.zig` mutates its wait predicate outside the mutex — `std.Io.Condition.broadcast` is a no-op with zero registered waiters). Clerk key rotation 401s every new token for up to the 6h cache time-to-live (TTL) (`auth/jwks.zig` never refreshes on kid miss, and holds its mutex across the network fetch). A hung control plane wedges a runner worker inside `fetch` with no timeout, starving the same-thread deadline kill — the child burns the tenant's provider key uncapped. The OTel rings are pushed from many threads but `push` is load-head/write-slot/store-head — torn entries. One pending human approval parks an httpz worker thread for up to an hour (uncapped `timeout_ms`), and the configured backpressure guard (`api_in_flight_requests`, env knob, gauge) enforces nothing.

**Solution summary:** Surgical fixes per surface: predicate mutation under the mutex; CAS-claimed ring slots with ready flags; JWKS kid-miss forced refresh (rate-limited) + single-flight fetch outside the lock + stale-serve on fetch failure; the approval gate becomes a persisted pending state re-evaluated on later lease polls (the blocking wait is deleted); the runner gains required deadlines on every control-plane call, one persistent HTTP client per worker, and batched activity frames; dispatch enforces the in-flight ceiling (429) and a dedicated SSE stream cap (503), wiring the already-exported metric.

---

## Prior-Art / Reference Implementations

- **CAS slots** → `src/zombied/observability/metrics_workspace.zig` (occupied/ready flags, bounded spin, documented budget). Mirror exactly; divergence needs a stated reason.
- **Single-winner state transition** → `fleet/approval_gate.zig` resolution `UPDATE … WHERE status='pending'` + the append-only trigger; the async gate reuses this, adding no new race surface.
- **Deadline math** → `vendor/pg` patched acquire (summed slept slices, not wall-clock subtraction).
- **API error envelopes** → `docs/REST_API_DESIGN_GUIDELINES.md` + nearest handler using `common.errorResponse` for the new 429/503 bodies.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/zombied/events/bus.zig` | EDIT | stop() mutates predicate under mutex; ordering comments per Concurrency rule |
| `src/zombied/observability/otel_logs.zig` | EDIT | multi-producer-safe ring push |
| `src/zombied/observability/otel_traces.zig` | EDIT | same ring fix |
| `src/zombied/auth/jwks.zig` | EDIT | kid-miss refresh, single-flight, stale-serve, fetch outside lock |
| `src/zombied/zombie/approval_gate.zig` | EDIT | delete blocking wait; pending-state read/apply helpers |
| `src/zombied/zombie/config_gates.zig` | EDIT | upper-bound the gate `timeout_ms` (named const) |
| `src/zombied/fleet/approval_gate.zig` | EDIT | async flow: persist pending, return outcome variants |
| `src/zombied/fleet/service.zig` | EDIT | gate outcome handling → no-work on pending (seam consumed by M90_002) |
| `src/runner/daemon/control_plane_client.zig` | EDIT | required deadline per call; persistent client lifecycle |
| `src/runner/daemon/loop.zig` | EDIT | batched activity forwarding |
| `src/runner/daemon/renew_driver.zig` | EDIT | renew bounded so deadline kill cannot be starved |
| `src/zombied/http/server.zig` | EDIT | in-flight inc/dec + 429 shed at dispatch |
| `src/zombied/http/handlers/common.zig` | EDIT | backpressure fields become enforced state |
| `src/zombied/http/handlers/zombies/events_stream.zig` | EDIT | SSE stream cap → 503 at cap |
| `src/zombied/config/runtime_loader.zig` | EDIT | cap/timeout knobs parsed + validated at boot |
| `src/zombied/config/runtime_types.zig` + `runtime_validate.zig` | EDIT | `InvalidSseMaxStreams` error + printer row |
| `src/zombied/observability/metrics.zig` (+ counters file) | EDIT | wire `incApiBackpressureRejections` to the shed branch |
| `src/zombied/errors/error_registry.zig` + `error_entries.zig` | EDIT | registry codes for 429 shed + SSE cap |
| `vendor/httpz/src/thread_pool.zig` | EDIT | §7 shared MPMC injector queue (owner-authorized vendor patch) |
| `vendor/httpz/CHANGES.md` | EDIT | §7 patch documented per the vendor/pg precedent |
| `src/zombied/events/subscription_hub.zig` (+ sibling test) | CREATE | §8 hub: shared conn, fan-out, refcounts |
| `src/zombied/cmd/serve_background.zig` | EDIT | §8 hub lifecycle (start/stop with the other background threads) |
| `src/zombied/cmd/serve.zig` | EDIT | §8 hub wiring into Context; §10 registry wiring |
| `src/zombied/cmd/serve_shutdown.zig` | EDIT | §10 drain hook before server stop |
| `src/zombied/http/route_table.zig` | EDIT | §9 RouteClass per route |
| `src/zombied/http/server.zig` (split on touch — at FLL cap) | EDIT | §9 class-gated admission at dispatch |
| `src/zombied/http/stream_registry.zig` (+ sibling test) | CREATE | §10 registry |
| `src/zombied/http/router.zig` + `routes.zig` + `route_table_invoke.zig` | EDIT | §10 fleet streams route |
| `src/zombied/http/handlers/fleet/streams_list.zig` (or nearest fleet handler home) | CREATE | §10 listing handler |
| `public/openapi/**` | EDIT | §10 new path row (`make check-openapi`) |
| `docs/architecture/data_flow.md` + `scaling.md` | EDIT | §6 thread-model reconciliation; §8 connection topology; §9 admission; §10 drain — each landing same-commit with its section |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** six independent slices, one per hang/race surface; each is shippable alone and none changes the runner↔zombied wire shape. **Amended (owner-directed, Discovery Jun 10):** four hardening refactors §7–§10 follow as their own commits on the same branch — vendor injector queue, SubscriptionHub, route-class admission, StreamRegistry — surfaced by §6's pivot; same wire-shape guarantee except the one additive admin read endpoint (§10).
- **Alternatives considered:** (a) a unified async-Io runtime rework solving §4/§6 structurally — rejected: puts the verified-correct money core and supervision plane back into play for marginal extra gain; (b) fix only the three P0s and defer gate/backpressure — rejected: the gate's blocking wait is the largest single thread-starvation source and M90_002's terminal writes need its async seam.
- **Patch-vs-refactor verdict:** **patch** on §1/§2/§3/§5/§6; §4 (async gate) is a contained **refactor** of one flow, justified because the wait-loop shape cannot be fixed in place. The larger work-indexed leasing redesign is named in Out of Scope, not smuggled in.

---

## Sections (implementation slices)

### §1 — Event-bus stop/wake correctness

`Bus.stop()` must take the mutex around the `running` store so a consumer between predicate check and waiter registration cannot sleep forever; broadcast after release. Ordering comments document the release-store/acquire-load pairing per the Concurrency rule.

- **Dimension 1.1** — stop-during-wait always joins → Test `test_bus_stop_joins_with_blocked_consumer` — ✅ DONE
- **Dimension 1.2** — repeated start/stop under contention never hangs or double-frees → Test `test_bus_stop_start_stress` — ✅ DONE

### §2 — OTel ring multi-producer safety

Producers claim a slot via CAS on head, write, then release-store a per-slot ready flag; the flush thread skips unready slots. Mirror `metrics_workspace.zig`. Applies to both logs and traces rings; drop counter still counts overflow.

- **Dimension 2.1** — N threads × M pushes: every flushed entry intact, none torn, drops counted exactly → Test `test_otel_ring_concurrent_push_integrity` — ✅ DONE
- **Dimension 2.2** — flush during push storm never reads a half-written slot → Test `test_otel_ring_flush_skips_unready` — ✅ DONE

### §3 — JWKS rotation resilience

kid miss on a fresh cache forces one rate-limited refresh and rescans before returning not-found (per `docs/AUTH.md`). The fetch happens outside the verifier mutex with single-flight (concurrent verifiers wait on the one fetch or serve stale). A failed refresh keeps serving the prior key set; the expired set is freed only after a successful replacement.

- **Dimension 3.1** — token with rotated kid verifies without restart; refresh is rate-limited under a kid-miss storm → Test `test_jwks_kid_miss_forces_refresh` — ✅ DONE
- **Dimension 3.2** — verifier mutex never held across fetch; concurrent verifies during refresh proceed on the old set → Test `test_jwks_single_flight_no_lock_across_fetch` — ✅ DONE
- **Dimension 3.3** — fetch failure serves stale keys and logs; no empty-cache retry storm → Test `test_jwks_stale_serve_on_fetch_failure` — ✅ DONE

### §4 — Approval gate goes async

Delete the polling wait. The gate check persists/reads the pending decision (existing Redis state + single-winner resolution) and returns immediately with a tagged outcome: proceed, pending, denied, or expired. The lease path responds no-work on pending; a later poll re-evaluates and proceeds on approve. `timeout_ms` parsing gains a named upper bound. Terminal `gate_blocked` rows for denied/expired land in M90_002 — this workstream only surfaces the variants. **Implementation default:** decision state stays in the existing Redis keys; expiry is evaluated at gate-check time (no new sweeper), because the per-poll re-check makes a dedicated waker redundant.

- **Dimension 4.1** — `waitForDecision` deleted; no sleep/poll loop remains in the lease path → Test `test_gate_pending_returns_no_work_immediately` — ✅ DONE
- **Dimension 4.2** — approve → next poll leases; deny/expiry → outcome variant surfaced (consumed by M90_002) → Test `test_gate_decision_applied_on_next_poll` — ✅ DONE
- **Dimension 4.3** — configured `timeout_ms` above the cap clamps to the named const and logs → Test `test_gate_timeout_clamped` — ✅ DONE

### §5 — Runner control-plane deadlines and connection thrift

Every control-plane call constructs with a required deadline (compiler-enforced parameter, no default); timeout classifies as retryable per RULE ECL. A blocked renew can never delay the child deadline kill — bound renew well inside the tick interval (named-const relation) or move it off the supervision thread; the agent picks per code shape, the invariant is the kill fires on time. One persistent HTTP client per worker (connection reuse); activity frames batch per flush window instead of one POST per frame.

- **Dimension 5.1** — unresponsive control plane: call returns timeout within the bound; classified retryable → Test `test_cp_call_deadline_fires` — ✅ DONE
- **Dimension 5.2** — renew blocked at its bound while child passes deadline: kill still fires on schedule → Test `test_deadline_kill_not_starved_by_renew` — ✅ DONE (bounded-renew form: the comptime window relation + config clamp + the watchdog bound make the kill delay ≤ the renew bound by construction; the hung-plane bound is the 5.1 test)
- **Dimension 5.3** — N activity frames across one flush window produce one POST; client reused across calls (single connection observed by fake server) → Test `test_activity_batching_and_client_reuse` — ✅ DONE

### §6 — HTTP backpressure made real

Dispatch increments/decrements the in-flight counter; above `api_max_in_flight_requests` respond 429 with `Retry-After` and increment `incApiBackpressureRejections` (today flatlined). SSE streams get a dedicated cap (`SSE_MAX_STREAMS`, default 64, 0 rejected at boot) → 503 at cap, and run on **dedicated detached threads** (`startEventStream`) so a stream can never occupy — or poison — the handler pool (amended mid-EXECUTE: the original "cap < thread count, clamp + warn" design presumed `startEventStreamSync`; a vendor thread-pool defect invalidated parking outright — Discovery, Jun 10).

- **Dimension 6.1** — requests above the ceiling get 429 + `Retry-After`; metric increments; gauge tracks live count → Test `test_dispatch_backpressure_429` — ✅ DONE
- **Dimension 6.2** — SSE connections at cap get 503; non-SSE routes still served while streams are parked; cap knob validated at boot (0 rejected) → Test `test_sse_cap_503_healthz_alive` — ✅ DONE

### §7 — vendor/httpz shared injector queue (owner-authorized vendor patch)

Replace `thread_pool.zig`'s per-Worker private rings + blind round-robin `flush()` with ONE shared bounded multi-producer/multi-consumer (MPMC) queue on the existing `Io.Mutex`/`Io.Condition` primitives, so any idle worker can execute any queued batch and a parked worker can never black-hole its round-robin share (the §6 pivot's root cause, kept reachable by any future long-running handler). Preserve `spawn`/`spawnOne`/`flush`/`empty`/`stop` semantics and the websocket `acquireProcessing` path; delete the dead `peer` field (its `i + i` wiring typo means stealing never worked). Expose a `pending()` depth read for §9. Document in `vendor/httpz/CHANGES.md` per the `vendor/pg/CHANGES.md` precedent; upstream filing is flagged in Discovery for the owner's call.

- **Dimension 7.1** — a worker parked inside a job cannot starve queued batches: remaining workers drain them → Test `test_pool_parked_worker_no_starvation` (vendor pool test)
- **Dimension 7.2** — `spawn`/`spawnOne`/`flush`/`empty`/`stop` semantics and full-queue producer backpressure preserved → existing vendor pool tests (`batch add`, `small fuzz`, `large fuzz`) green unmodified
- **Dimension 7.3** — patch documented in `vendor/httpz/CHANGES.md`; `peer` field gone; `pending()` exposed → CHANGES.md entry + `grep -n "peer" vendor/httpz/src/thread_pool.zig` → 0

### §8 — SubscriptionHub: one shared Redis pub/sub connection

One boot-started hub (in `serve_background`, like the OTel exporters) owns the process's single pub/sub connection and reader thread. Mutex-protected `channel → subscribers + refcount` map: refcount 0→1 issues the wire SUBSCRIBE, 1→0 the UNSUBSCRIBE; everything between is map-only. The reader thread fans each `["message", channel, payload]` out by copy into each subscriber's **bounded** local queue — full queue drops oldest + counts (the §2 ring discipline); the reader never blocks on a slow consumer. `StreamJob` consumes a hub subscription instead of dialing its own connection (no Redis dial, no TLS handshake on the request path); the stream thread's loop becomes a timed wait on its local queue — timeout → heartbeat write, preserving RULE TIM. Connection loss: reader logs, redials with backoff, re-SUBSCRIBEs every channel with refcount > 0; streams heartbeat through the gap (messages in the gap are lost — same loss semantics as today's per-stream connection drop; clients backfill via the events cursor).

- **Dimension 8.1** — N streams (mixed channels) share exactly ONE Redis connection; no per-stream dial → Test `test_hub_n_streams_one_connection`
- **Dimension 8.2** — per-channel refcount: first subscriber SUBSCRIBEs, last UNSUBSCRIBEs, middle ones are wire-silent → Test `test_hub_refcount_subscribe_unsubscribe`
- **Dimension 8.3** — slow consumer: drop-oldest + drop counter; sibling subscribers receive everything → Test `test_hub_slow_consumer_drops_counted`
- **Dimension 8.4** — reader reconnects on connection loss and re-subscribes live channels → Test `test_hub_reconnect_resubscribes`
- **Dimension 8.5** — `StreamJob` rides the hub; teardown ordering (job freed before slot release) and SSE heartbeat cadence preserved → existing SSE streaming + backpressure integration suites green

### §9 — route-class admission {ops, stream, api}

Every route carries a class on the route table: `healthz`/`readyz`/`metrics` = **ops** (never shed — an admission storm must not blind the operators diagnosing it), the SSE tail = **stream** (gated by its own §6 cap, exempt from the api ceiling), everything else = **api** (in-flight ceiling → 429). Dispatch order: match first (cheap), class-gate before invoke — unmatched paths 404 without consuming admission (documented choice: a 404 costs less than the gate). If §7's `pending()` composes cleanly at the dispatch seam, queue depth becomes the api admission signal and the in-flight counter stays as telemetry; otherwise the counter remains the signal — decision recorded in Discovery.

- **Dimension 9.1** — at api saturation, `/healthz` + `/readyz` + `/metrics` answer 200 while api routes shed 429 → Test `test_ops_routes_never_shed`
- **Dimension 9.2** — stream class is exempt from the api ceiling (gated only by the SSE cap) → Test `test_stream_class_exempt_from_api_ceiling`
- **Dimension 9.3** — every Route variant maps to a class (exhaustive switch — compile-enforced); 404s bypass admission → Test `test_route_class_exhaustive_and_404_ungated`

### §10 — StreamRegistry: drain on shutdown + admin listing

A mutex-map registry replaces the bare SSE slot counter as the owner of live streams: entries `{workspace_id, zombie_id, started_ms, client fd}`. `StreamJob` registers on start and deregisters in its teardown defer (ordering with slot release preserved); the SSE gauge derives from registry size; admission reads the same count. Shutdown drain: `serve_shutdown` walks the registry and `shutdown()`s each client fd so the stream threads' next write fails fast and they exit — closing the accepted §6 exit window (detached threads outliving a clean shutdown) honestly. New platform-admin endpoint `GET /v1/fleet/streams` lists live streams (OpenAPI row; `make check-openapi`).

- **Dimension 10.1** — registry owns liveness: register/deregister paired; gauge equals registry size; cap admission unchanged → Test `test_registry_register_deregister_gauge`
- **Dimension 10.2** — shutdown with live streams: drain `shutdown()`s client fds, stream threads exit, shutdown completes bounded → Test `test_shutdown_drains_live_streams`
- **Dimension 10.3** — `GET /v1/fleet/streams` (platform-admin) lists entries; non-admin 403; OpenAPI green → Test `test_fleet_streams_listing_admin_gated`

---

## Interfaces

```
Runner↔zombied wire shape: UNCHANGED (timing/transport behaviour only).
Lease response on gate-pending: existing no-work shape, unchanged.
NEW HTTP responses (error envelope per REST guidelines, codes registered):
  429 {"error":{"code":"UZ-…","message":…}} + Retry-After: <seconds>   (in-flight ceiling)
  503 {"error":{"code":"UZ-…","message":…}}                            (SSE stream cap)
Gate check outcome (internal): tagged union { proceed, pending, denied, expired } — RULE TGU.
control_plane_client call signatures: deadline parameter required at every call site.
NEW env knob: SSE_MAX_STREAMS (u32, default 64, 0 rejected) — concurrent SSE
  streams per instance; streams run on dedicated detached threads.
NEW HTTP (§10): GET /v1/fleet/streams (platform-admin) → live SSE stream listing
  [{workspace_id, zombie_id, started_ms}] (client fd is internal, never serialized).
SubscriptionHub (internal, §8): subscribe(channel) → subscription handle with a
  bounded local queue + timed pop; unsubscribe via the handle. One process-wide
  pub/sub connection; per-channel refcounted wire SUBSCRIBE/UNSUBSCRIBE.
RouteClass (internal, §9): enum { ops, stream, api } — total function over Route
  (exhaustive switch); dispatch gates admission per class after match.
StreamRegistry (internal, §10): register/deregister/drain/snapshot; owns the live
  count the gauge and cap admission read.
vendor/httpz ThreadPool (§7): one shared bounded MPMC queue; pub fn pending()
  exposes queue depth; spawn/spawnOne/flush/empty/stop signatures unchanged.
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Identity provider down | JWKS fetch fails | stale keys served; `log.warn` once per refresh attempt; verifies keep working |
| Identity provider hangs | slow fetch | single-flight: one fetch waits, other verifiers use old set; hot path never blocks on network |
| Control plane unresponsive | network/peer hang | call times out at deadline; retryable; child kill unaffected |
| Renew blocked at deadline | control plane slow | kill fires on schedule; renew outcome discarded; logged |
| Push storm on OTel ring | bursty logging | entries intact; overflow counted in drop counter, never torn |
| Stop during consumer wait | shutdown race | consumer wakes, drains, joins; no deadlock |
| Backpressure ceiling hit | burst traffic | 429 + Retry-After + metric + log; gauge accurate |
| SSE cap hit | many dashboard tabs | 503 + metric + log; other routes unaffected |
| Gate decision races expiry | approve at deadline | existing single-winner UPDATE decides exactly one outcome |
| Pool worker parked in a long job | future long-running handler | shared queue: any idle worker drains queued batches; no request black-holed (§7) |
| Hub pub/sub connection lost | Redis restart / network | reader redials with backoff, re-SUBSCRIBEs refcounted channels; streams heartbeat through the gap; in-gap messages lost — clients backfill via events cursor (§8) |
| Slow SSE consumer | stalled client socket | its bounded queue drops oldest + counts; reader thread and sibling streams unaffected (§8) |
| Admission saturated during incident | burst traffic | ops routes (healthz/readyz/metrics) still answer — operators keep eyes (§9) |
| Shutdown with live streams | deploy / SIGTERM | registry drain `shutdown()`s client fds; stream threads exit on failed write; shutdown bounded (§10) |

---

## Invariants

1. Every control-plane call site passes a deadline — enforced by the client's required parameter (compile error to omit).
2. `grep -rn "waitForDecision" src/` returns zero — the blocking gate wait is structurally gone (Eval E8).
3. OTel ring slots are readable only after their ready flag's release-store — RULE CAS shape + concurrency test.
4. Bus predicate mutations happen under the bus mutex — concurrency test + ordering comments; zlint-clean.
5. Concurrent SSE streams ≤ `sse_max_streams` (env knob, default 64, 0 rejected at boot) — and every stream runs on a dedicated detached thread (`startEventStream`), never on a handler-pool thread, so pool poisoning is structurally impossible (vendor pool round-robins private queues with no stealing — Discovery).
6. JWKS mutex is never held across a network fetch — single-flight test with injected slow fetcher proves hot-path latency independent of fetch latency.
7. No httpz pool worker owns a private queue — any idle worker can execute any queued batch; producer backpressure at the shared bound is preserved (§7 vendor patch, starvation test).
8. Exactly one Redis pub/sub connection exists per zombied process regardless of live stream count; wire SUBSCRIBE count per channel is 0 or 1 (§8 refcount test).
9. ops-class routes are never shed by admission control, at any load (§9).
10. Every live SSE stream is a StreamRegistry entry; the SSE gauge equals registry size; shutdown drains the registry before process exit (§10).

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_bus_stop_joins_with_blocked_consumer` | consumer parked in waitNext; stop() → join completes |
| 1.2 | unit | `test_bus_stop_start_stress` | repeated cycles under load → no hang, leak detector clean |
| 2.1 | unit | `test_otel_ring_concurrent_push_integrity` | N threads push distinct payloads → flushed set = pushed set minus counted drops, no torn bytes |
| 2.2 | unit | `test_otel_ring_flush_skips_unready` | claimed-but-unready slot → flush skips it, picks it up next pass |
| 3.1 | unit | `test_jwks_kid_miss_forces_refresh` | unknown kid + injected fetcher with new key → verify succeeds; second miss inside window does not re-fetch |
| 3.2 | unit | `test_jwks_single_flight_no_lock_across_fetch` | slow fetcher + concurrent verifies → old-kid verifies complete before fetch returns |
| 3.3 | unit | `test_jwks_stale_serve_on_fetch_failure` | failing fetcher at TTL expiry → old keys still verify; warn logged |
| 4.1 | integration | `test_gate_pending_returns_no_work_immediately` | lease with approval-gated zombie → no-work response, no sleep |
| 4.2 | integration | `test_gate_decision_applied_on_next_poll` | approve in Redis → next lease proceeds; deny → denied variant |
| 4.3 | unit | `test_gate_timeout_clamped` | config timeout above cap → clamped value + warn log |
| 5.1 | integration | `test_cp_call_deadline_fires` | fake server never responds → error within bound, classified retryable |
| 5.2 | integration | `test_deadline_kill_not_starved_by_renew` | renew held at bound, child past deadline → kill observed on time |
| 5.3 | integration | `test_activity_batching_and_client_reuse` | 10 frames in window → 1 POST; fake server sees 1 connection across calls |
| 6.1 | integration | `test_dispatch_backpressure_429` | in-flight saturated → 429 + Retry-After + counter delta |
| 6.2 | integration | `test_sse_cap_503_healthz_alive` | streams at cap → 503; `/healthz` 200 concurrently |
| 7.1 | unit (vendor) | `test_pool_parked_worker_no_starvation` | one worker parked in a job; subsequently spawned jobs all complete on remaining workers |
| 7.2 | unit (vendor) | existing `batch add` / `small fuzz` / `large fuzz` | green unmodified — spawn/flush/empty/stop semantics pinned |
| 8.1 | integration | `test_hub_n_streams_one_connection` | N subscriptions (mixed channels) → broker observes exactly 1 client connection |
| 8.2 | unit | `test_hub_refcount_subscribe_unsubscribe` | 3 subscribers, 1 channel → 1 wire SUBSCRIBE; drop to 0 → 1 wire UNSUBSCRIBE |
| 8.3 | unit | `test_hub_slow_consumer_drops_counted` | full bounded queue → oldest dropped + counter delta; sibling subscriber receives all |
| 8.4 | integration | `test_hub_reconnect_resubscribes` | reader connection severed → redial + re-SUBSCRIBE; messages flow after recovery |
| 8.5 | integration | existing SSE streaming + backpressure suites | green on the hub-backed StreamJob; teardown drain ordering intact |
| 9.1 | integration | `test_ops_routes_never_shed` | api ceiling saturated → `/healthz`/`/readyz`/`/metrics` 200, api route 429 |
| 9.2 | integration | `test_stream_class_exempt_from_api_ceiling` | api ceiling saturated → SSE connect still admitted (until its own cap) |
| 9.3 | unit | `test_route_class_exhaustive_and_404_ungated` | classFor total over Route variants; unmatched path → 404 with admission untouched |
| 10.1 | unit | `test_registry_register_deregister_gauge` | register N, deregister M → size N−M; gauge matches; double-deregister safe |
| 10.2 | integration | `test_shutdown_drains_live_streams` | live stream + shutdown → client fd shut down, thread exits, drain bounded |
| 10.3 | integration | `test_fleet_streams_listing_admin_gated` | platform-admin GET → entries; tenant token → 403; non-live after close |

Regression: existing SSE heartbeat, lease/renewal, and gate-resolution integration suites must stay green (they pin the behaviour around every touched seam). Idempotency/replay: 4.2 re-poll after approve is idempotent (single lease).

---

## Acceptance Criteria

- [ ] `make lint-all` clean · `make test-unit-all` passes (all lanes)
- [ ] `make test-integration` passes (HTTP + Redis surfaces touched)
- [ ] `make memleak` clean (threads/allocator wiring touched)
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` + linux test graphs per façade
- [ ] `gitleaks detect` clean · no production file over 350 lines
- [ ] `grep -rn "waitForDecision" src/` → 0 — verify: Eval E8
- [ ] 429/503 codes present in registry with hints — `make check-openapi` green (existing ErrorBody shape, no YAML churn)
- [ ] §7 vendor diff confined to `vendor/httpz/src/thread_pool.zig` + `vendor/httpz/CHANGES.md` — `git diff origin/main -- vendor/ --name-only` shows exactly those two
- [ ] §8: one pub/sub connection process-wide (Invariant 8 test) · §9: ops probes answer at saturation · §10: shutdown drain bounded with live streams
- [ ] `make check-openapi` green including the §10 `GET /v1/fleet/streams` row

(Boxes unchecked at the §7–§10 amendment: they re-certify the whole branch at CHORE(close). The §1–§6 evidence at `b720a78b` stands in the Verification Evidence table.)

## Eval Commands (post-implementation)

```bash
# E1: gate wait eliminated
test -z "$(grep -rn 'waitForDecision' src/)" && echo PASS || echo FAIL
# E2: Build — zig build
# E3: Tests — make test && make test-integration
# E4: Lint — make lint 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile — zig build -Dtarget=x86_64-linux 2>&1 | tail -3
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: 350-line gate (exempts .md) —
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
# E8: orphan sweep for deleted wait symbols (empty = pass)
grep -rn "waitForDecision" src/ | head
```

## Dead Code Sweep

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `waitForDecision` | `grep -rn "waitForDecision" src/ \| head` | 0 matches |
| `peer` (vendor Worker field, §7) | `grep -n "peer" vendor/httpz/src/thread_pool.zig` | 0 matches |
| `classifyIdle` + `SSE_TIMEOUT_MIN_ELAPSED_MS` (§8 — queue wait replaces socket-timing heuristic) | `grep -rn "classifyIdle" src/` | 0 matches |
| `connectFromConfig` SSE call site (§8 — hub owns the dial; fn stays for the hub itself) | `grep -rn "connectFromConfig" src/zombied/http/` | 0 matches |

Per RULE NLR, files opened by this diff shed any other dead surface they carry (bus/jwks/approval_gate), recorded in Discovery.

---

## Discovery (consult log)

- **Consults** — (append Architecture/Legacy-Design/gate-flag consults + Indy decisions here.)
- Jun 10, 2026 — §1 landed. Dimension 1.1's assertion is carried by the pre-existing test `"integration: event bus run thread exits when stopped while idle"` (parked consumer + stop → join), made race-free by this fix; Dimension 1.2 is the new no-sleep start/stop stress loop `"integration: stop never loses the wakeup under repeated start/stop"`. `stop()` now orders its `running` store under the bus mutex; all weak orderings in the file carry pairing comments. Discovery bonus: `events/bus.zig` was missing from `src/zombied/tests.zig`'s explicit import list, so its four pre-existing tests had **never executed** (same class as M90_003 §3's dead src/lib lane) — import added in the same diff per RULE TST; zombied suite grew 1516 → 1521, all passing.
- Jun 10, 2026 — §2 landed. Both rings now CAS-claim the head slot and publish a per-slot ready flag (mirrors `metrics_workspace.zig`); pop treats an unready head-of-line slot as empty for the pass. Tests: 4-thread integrity (no torn/lost/duplicate entries, drops exact), wraparound flag recycle, claimed-but-unready delivery — duplicated across both rings since the code is duplicated. Second discovery gap: `observability/otel_traces.zig` was also missing from `tests.zig` — its ten pre-existing tests had never run; wired in this diff (RULE TST). NLR cleanup on touch: `postWithBasicAuth`'s dead no-op tail branch now returns `error.OtlpExportRejected` so the existing caller warn logs non-2xx rejections (module-header claim becomes true). **Surfaced, not bundled** (per the NLR perf/structure discipline): (a) the two rings are byte-identical and want a shared `Ring(comptime Entry, capacity)` module — structure follow-up for Indy's fold-in-or-file call; (b) the audit's boot-time config-string leak in both `uninstall`s is a cross-file ownership question — proposed home: M90_003 §4 (amend at its CHORE(open)).
- Jun 10, 2026 — §3 landed. `lookupKey` is now a tri-state cache scan (hit / miss-on-fresh / miss-stale-or-none): kid miss on a fresh cache forces a refresh (per `docs/AUTH.md`), all refreshes are single-flight with the network round-trip outside the verifier mutex, attempts are rate-limited by `JWKS_REFRESH_MIN_INTERVAL_MS`, and a failed fetch keeps serving the prior key set (`jwks_stale_serve` warn). The old code destroyed the expired cache before fetching — an identity-provider outage left no keys at all. FLL split: standard-claims helpers extracted to `auth/jwks_standard_claims.zig` (re-exported; `claims.zig` consumers unchanged). Tests ride the existing inline-JWKS fixtures: rotation, rate-limit window, stale-serve, fail-closed-no-cache, and an 8-thread cold-cache storm asserting exactly one fetch.
- Jun 10, 2026 — **Pre-existing suite flake — diagnosis corrected:** the "expected .worker_started / expected 5, found 1" stderr lines are noise from *passing* negative tests (`expectError` catches the assert, the message still prints) — red herrings. The real failure: `zig build test` intermittently reports `failed command` while the binary itself reports `1216 passed; 0 failed` and **exits 0 when run standalone** — the failure exists only under the build-runner's `--listen=-` result protocol, i.e. something in the suite writes to stdout and intermittently corrupts the protocol stream. Reproduced on the unmodified base commit (`ebe6b4f6`, main + specs only) — pre-existing on main, not this branch. Follow-up candidate: find the stdout writer (CLI command tests / harness prints) and route it to stderr; until then `make test` can flake repo-wide.
- Jun 10, 2026 — §4 landed. The blocking `waitForDecision` poll loop is deleted; gate state machine: first encounter requests approval + records a `zombie:gate:byevent:` ref (`action_id|deadline_ms`, TTL ≥ deadline + grace) and answers `pending` → lease returns no-work; every later poll evaluates the ref (`approval_gate_async.zig`): approved → proceeds, denied → blocked, deadline passed → resolves `timed_out` via the existing single-winner `resolve()` (sweeper-compatible attribution). Redis blips stay `pending` — a transient read failure can never deny an approved gate. `GateResult` deleted with its pin test (only the wait consumed it); gate `timeout_ms` parse clamps at new `GATE_TIMEOUT_MS_MAX` (24h) + warn. Live-Redis tests gate on `REDIS_URL` (skip otherwise); ref parsing is covered pure. Terminal `gate_blocked` rows for denied/expired remain M90_002 scope, consuming this seam.
- Jun 10, 2026 — §5 landed. The client is persistent per owner (keep-alive pool; one TCP handshake across verbs — pinned by a two-heartbeats-one-accept test) and every verb takes a REQUIRED `deadline_ms` (compile-enforced). Design discovery: SO_RCVTIMEO is unusable — the threaded Io's recv path panics on EAGAIN ("programmer bug caused syscall error") — so the bound is a per-client watchdog (`daemon/call_deadline.zig`) that `shutdown()`s the in-flight pooled socket at deadline (the repo's accept-wake pattern); residual unbounded window: name-resolution/TCP-connect inside fetch. A second design bug was caught by the upgraded guard test: `Client.connect` checks the connection out into the pool's used list — the pre-fix arming would have leaked one connection per call; fixed with connect→pin→release. Deadlines are env-configurable (`RUNNER_CP_*_DEADLINE_MS`, parse+clamp per the worker-count template; renew clamped into the `renew + tick < window` relation, comptime-asserted on the defaults; per Indy: only deadlines with distinct rationale get names — default/report/activity/renew). Activity frames batch per flush window (16 frames / 64 KiB / 1s, tick-driven staleness flush via TickFanout, final flush before report). FLL splits: `daemon/forwarders.zig` (both forwarders) + `daemon/call_deadline.zig`; orphaned `activity(frames)` wrapper deleted (NDC). **Harness note for Indy:** the fd-statelessness tripwire in `control_plane_client_test.zig` fired on the new fields, as designed — 🎯 flagged: persistent pool fields + watchdog · 🔧 fix: guard upgraded to a live FD_CLOEXEC pin (stdlib threaded Io opens sockets SOCK_CLOEXEC; bwrap closes unpassed fds besides) + field allowlist with review notes · 🏆 gain: the property that actually protects the forked child is now asserted on a real pooled connection · ⚠️ if reverted: per-call handshakes return and no call bound exists.
- Jun 10, 2026 — §6 landed. Dispatch claims an in-flight slot before any routing (paired defer releases it); above the ceiling the request sheds an allocation-light 429. Per `docs/REST_API_DESIGN_GUIDELINES.md` §4 the 429 carries the full header set — `Retry-After: 1` plus `X-RateLimit-Limit`/`-Remaining`/`-Reset` with instance-ceiling semantics (dynamic values on the request arena; httpz borrows header slices) — and the body is the canonical problem+json envelope via `common.errorResponse` (the spec's Interfaces sketch showed the older `{"error":…}` shape; Prior-Art's `common.errorResponse` instruction wins, rules-over-spec). New registry codes `UZ-API-001` (429) / `UZ-API-002` (503) with paired `MSG_*` consts (RULE EMS). SSE streams claim a dedicated slot before any backend work (bearer authn already ran in middleware; shedding precedes the two authorize queries so a tab-storm can't hammer the pool) — cap is the named const `SSE_MAX_STREAMS_DEFAULT = 16` (half the prod 32-thread pool), boot-clamped to `threads − 1` with a `sse_cap_clamped` warn (RULE TIM relation documented at the const). `API_HTTP_THREADS=1` (the local default) clamps to 0 — SSE structurally disabled there, which is the M88 incident invariant, not a bug; the warn names the fix. 6.2 got dedicated metrics (`zombie_sse_in_flight_streams` gauge + `zombie_sse_backpressure_rejections_total`) rather than reusing the api counter: SSE-tab storms and API backpressure want different operator knobs, and the gauge is the exact per-node SSE-density evidence M88's deferral gate asks for. **Dormant-guard consult, resolved:** with prod config (`API_HTTP_WORKERS=2 × API_HTTP_THREADS=32` = 64 concurrent dispatch; dev 1×32 = 32) the 429 guard is *dormant* — httpz bounds concurrent dispatch at workers×threads, which never exceeds the 256 default ceiling; the guard only fires when the knob is set below that product. Wiring is correct and tested (tests override the ceiling). Options put to Indy: (A) lower the prod default below the pool, (B) keep the default and document the relation as an incident lever. > Indy (2026-06-10): "B" — context: ceiling default stays 256; relation + incident-lever semantics documented in `deploy/fly/zombied-{prod,dev}/fly.toml` comment blocks in this branch; tuning a live shed ceiling deferred until real traffic data exists.
- Jun 10, 2026 — §6 discovery bonus, third dark test lane this milestone (same class as §1 bus.zig / §2 otel_traces.zig): `config/runtime.zig` was missing from `src/zombied/tests.zig`, so its four-file test fan-out (`runtime_loader_test`, `runtime_env_parse_test`, `runtime_validate_test`, `runtime_pepper_loader_test`) had **never compiled** — 12 stale one-arg `ServeConfig.load` calls plus `std.posix.setenv` (removed in 0.16) survived two migrations unnoticed. Wired per RULE TST and repaired: tests now build hermetic env maps via `common.env.fromPairs` (the seam built for exactly this — no process-env mutation, no cross-test pollution); tests of the M11_006-deleted `api_keys` env auth were removed and replaced with an `OidcRequired` assertion the current loader actually has; clamp coverage added (32→16 pass, 17→16 boundary, 16→15 clamp, 8→7, defaults→0). Zombied suite grew 1253 standalone-green. NLR cleanup on touch: `server.zig`'s `max_body_size = 2 * DEFAULT_MAX_CLIENTS * DEFAULT_MAX_CLIENTS` (a units pun that happened to equal 2 MiB) now references `common.MAX_BODY_SIZE`, deleting the "must match" drift hazard its comment admitted. UFS extraction: the operator token/JWKS/workspace fixtures the new backpressure suite shares with the SSE streaming suite moved to `sse_test_fixtures.zig` (webhook_test_fixtures precedent) instead of duplicating the literals.
- Jun 10, 2026 — **Scope amendment §7–§10 (owner-directed).** > Indy (2026-06-10): "Yes i want you to start on the large refactor sequencing you gave 1, 2, 4, 5 … each one committed in this PR. … i dont want a new PR for these. Just 1 PR … tested and then /write-unit-test and then /review chore(close) PR" — context: the four hardening refactors surfaced by the §6 pivot (vendor injector queue, SubscriptionHub, route-class admission, StreamRegistry), one commit each on this branch, single PR. The §7 vendor edit is explicitly authorized by this quote (vendor-patch practice per `vendor/pg/CHANGES.md`); **filing the pool defect upstream (missing work-stealing + the `i + i` peer typo) remains Indy's open call** — flagged, not bundled. Carried over from the session handoff so it survives the handoff's deletion: when M88_001 (evented SSE substrate, GATED) reopens, its premise needs amending — the measured pain was pool poisoning (§6/§7) + per-stream Redis connections (§8), not raw httpz throughput. Acceptance boxes reset to unchecked at this amendment; §1–§6 evidence at `b720a78b` stands in Verification Evidence. Test-depth anchor for the close-time delta: unit=1886, integration=164.
- Jun 10, 2026 — **§6.2 mid-EXECUTE pivot: production pool-poisoning defect found via the new integration test; SSE moved to dedicated stream threads.** The first-ever live run of `test_sse_cap_503_healthz_alive` wedged: after one parked stream, roughly every other request to the harness server was accepted but never served (and never timed out). Traced with throwaway instrumentation in the vendored worker loop (reverted; `vendor/` ships untouched): httpz's `ThreadPool.flush` assigns each request batch to ONE pool worker by blind round-robin; each pool worker has a private queue and **no work-stealing** (a vestigial `peer` field is wired with an `i + i` typo — stealing was never finished). Our `startEventStreamSync` usage parked the pool thread for the stream's lifetime, so the parked worker's queue black-holed its round-robin share of all later requests. **Platform-independent (queues sit above kqueue/epoll) and live in prod today**: at `API_HTTP_THREADS=32`, each open dashboard tail poisons ~1/32 of its httpz-worker's request batches for as long as the tab is open. No prior test ever issued a request *while* a stream was parked — the spec's `/healthz`-alive assertion was the first observer. Bonus defect the pivot also fixes: `server.stop()` joins pool threads, so a parked stream hung clean shutdown. Fix: switch to httpz's intended primitive `startEventStream` (headers + blocking mode + disown + **dedicated detached thread** per stream) — no vendor patch. Ownership shape: `StreamJob` (subscriber + channel) allocated on `ctx.alloc` (NOT the request arena, which dies at handler return — the old sync code's `hx.alloc` use would have been a use-after-free under the new design and was a latent smell anyway), created on the request thread, destroyed by the stream thread; slot release is the thread's last defer so an observer of a freed slot has also observed teardown (test drain-polls rely on this). Consult: options 1 (keep the now-vestigial boot clamp) vs 2 (drop the clamp, amend the spec) put to Indy with diagrams. > Indy (2026-06-10): "Option 2" — and on the cap value: "that means just 16 users? that is pretty low… we must with more number of connection or do an adverse review to see the optimal count" — resolved as: default raised 16 → **64** with budget math at the const (~0.5 MiB + 1 Redis conn + 2 fds per stream ≈ 32 MiB total on the 4 GB box), promoted to an **env knob `SSE_MAX_STREAMS`** (0 rejected, `InvalidSseMaxStreams`) so capacity moves without a rebuild, and the empirical optimum (Redis fan-out CPU) deferred to the M88-gated load test, fed by the new `zombie_sse_in_flight_streams` gauge. Invariant 5 + §6.2 amended accordingly; fly.toml comment blocks rewritten to the dedicated-thread truth. **Vendor follow-up surfaced, not bundled:** the pool's missing work-stealing (and the `i + i` peer typo) is an upstream httpz issue worth filing/patching independently — any future long-running handler would re-trip it; Indy's call on filing upstream vs vendor-patching in a follow-up workstream.
- **Skill chain outcomes** — `/write-unit-test`, `/review`, `/review-pr`, `kishore-babysit-prs` results.
- **Deferrals** — Indy-acked verbatim quotes only.

## Skill-Driven Review Chain (mandatory)

| When | Skill | Required output |
|------|-------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | clean; iteration count in Discovery |
| After tests pass, before CHORE(close) | `/review` | clean or every finding dispositioned |
| After `gh pr create` | `/review-pr` | comments addressed before human review |

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test-unit-all` (the `make test` name in older docs is stale) | all lanes ✓ — zombied 1253 pass / 0 fail (324 skip), runner 268 (6 skip), lib 28, website 883 + 149 + 403, coverage + bundle gates ✓ | ✅ |
| Integration tests | `make test-integration` | ✓ passed (live Postgres + Redis); suite binary re-run standalone → exit 0, incl. all four SSE tests on the dedicated-thread design + the 429 dispatch test. Build-runner `--listen` stderr flake observed mid-run (pre-existing, Discovery Jun 10) | ✅ |
| Memleak | `make memleak` | ✓ gate passed — 1253/0 under the leaks pass; detached stream threads drain before teardown (fixture gauge-poll); SIP "not debuggable" notice expected per façade | ✅ |
| Lint | `make lint-all` | ✓ all linters + quality gates (zig fmt, ZLint, pg-drain, line-limit, openapi, schema gate, gh-actions, playbooks) | ✅ |
| Cross-compile | both targets × `zig build` + `build_runner.zig` graphs; linux test graphs | all clean; test graphs end at `unable to execute binaries from the target` (the documented pass signal) | ✅ |
| Gitleaks | `gitleaks detect` | no leaks found (2563 commits) | ✅ |
| Dead code sweep | `grep -rn "waitForDecision\|startEventStreamSync" src/` | 0 matches (incl. stale sweeper comment cleaned per RULE ORP) | ✅ |
| Harness gates | `make harness-verify` (staged) | ALL GATES GREEN (UFS, DESIGN TOKEN, SPEC TEMPLATE, ERROR REGISTRY, LOGGING, LIFECYCLE, CROSS-TIER RATES, MS-ID + UI) | ✅ |
| Test depth | `make _lint_zig_test_depth` | zombied_test_cases=1886, integration_cases=164 (no CHORE(open) baseline header in this spec — predates the convention; suite grew +37 from the resurrected config lane alone) | ✅ |

## Out of Scope

- Terminal `gate_blocked` rows for denied/expired gate outcomes → **M90_002** (consumes this workstream's outcome variants).
- Work-indexed leasing (replacing the per-poll candidate scan) — separate design workstream; named here so §4 isn't mistaken for it.
- Lease deadline clock-skew tolerance (server-relative TTL in the lease payload) — follow-up candidate from the audit (`src/runner/child_supervisor.zig`).
- RESP header stack-parsing and logger filter-before-format hot-path savings — perf follow-ups, audit P2s.
- Per-frame memory-capture POST cadence — only activity frames batch here.
