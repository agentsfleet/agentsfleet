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

- **PR title (eventual):** `fix(m90): bound every blocking wait; fix bus stop, OTel ring, JWKS rotation`
- **Intent (one sentence):** No zombied or runner thread can hang unboundedly, lose a wakeup, tear a telemetry write, or 401 valid users after an identity-provider key rotation.
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

**Goal (testable):** Event-bus `stop()` always joins its consumer; every runner control-plane call carries a deadline and a blocked renewal can never delay the child kill; JWKS verifies tokens minted after a key rotation without restart; concurrent OTel log/trace pushes never tear an entry; HTTP dispatch sheds load at the configured in-flight ceiling and Server-Sent Events (SSE) streams are capped below thread-pool starvation.

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
| `src/zombied/observability/metrics.zig` (+ counters file) | EDIT | wire `incApiBackpressureRejections` to the shed branch |
| `src/zombied/errors/error_registry.zig` + `error_entries.zig` | EDIT | registry codes for 429 shed + SSE cap |
| sibling `*_test.zig` per touched module | CREATE/EDIT | per Test Specification |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** six independent slices, one per hang/race surface; each is shippable alone and none changes the runner↔zombied wire shape.
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

- **Dimension 5.1** — unresponsive control plane: call returns timeout within the bound; classified retryable → Test `test_cp_call_deadline_fires`
- **Dimension 5.2** — renew blocked at its bound while child passes deadline: kill still fires on schedule → Test `test_deadline_kill_not_starved_by_renew`
- **Dimension 5.3** — N activity frames across one flush window produce one POST; client reused across calls (single connection observed by fake server) → Test `test_activity_batching_and_client_reuse`

### §6 — HTTP backpressure made real

Dispatch increments/decrements the in-flight counter; above `api_max_in_flight_requests` respond 429 with `Retry-After` and increment `incApiBackpressureRejections` (today flatlined). SSE streams get a dedicated lower cap (named const) → 503 at cap, so dashboard tabs cannot starve `/healthz`. Boot validation: SSE cap < worker thread count, else clamp + warn.

- **Dimension 6.1** — requests above the ceiling get 429 + `Retry-After`; metric increments; gauge tracks live count → Test `test_dispatch_backpressure_429`
- **Dimension 6.2** — SSE connections at cap get 503; non-SSE routes still served; boot clamps a misconfigured cap → Test `test_sse_cap_503_healthz_alive`

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

---

## Invariants

1. Every control-plane call site passes a deadline — enforced by the client's required parameter (compile error to omit).
2. `grep -rn "waitForDecision" src/` returns zero — the blocking gate wait is structurally gone (Eval E8).
3. OTel ring slots are readable only after their ready flag's release-store — RULE CAS shape + concurrency test.
4. Bus predicate mutations happen under the bus mutex — concurrency test + ordering comments; zlint-clean.
5. SSE cap < HTTP worker threads — boot-time validation clamps and warns (runtime check, logged).
6. JWKS mutex is never held across a network fetch — single-flight test with injected slow fetcher proves hot-path latency independent of fetch latency.

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

Regression: existing SSE heartbeat, lease/renewal, and gate-resolution integration suites must stay green (they pin the behaviour around every touched seam). Idempotency/replay: 4.2 re-poll after approve is idempotent (single lease).

---

## Acceptance Criteria

- [ ] `make lint` clean · `make test` passes
- [ ] `make test-integration` passes (HTTP + Redis surfaces touched)
- [ ] `make memleak` clean (threads/allocator wiring touched)
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` + linux test graphs per façade
- [ ] `gitleaks detect` clean · no production file over 350 lines
- [ ] `grep -rn "waitForDecision" src/` → 0 — verify: Eval E8
- [ ] 429/503 codes present in registry with hints — verify: `make check-openapi` if response shapes touch OpenAPI

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

Per RULE NLR, files opened by this diff shed any other dead surface they carry (bus/jwks/approval_gate), recorded in Discovery.

---

## Discovery (consult log)

- **Consults** — (append Architecture/Legacy-Design/gate-flag consults + Indy decisions here.)
- Jun 10, 2026 — §1 landed. Dimension 1.1's assertion is carried by the pre-existing test `"integration: event bus run thread exits when stopped while idle"` (parked consumer + stop → join), made race-free by this fix; Dimension 1.2 is the new no-sleep start/stop stress loop `"integration: stop never loses the wakeup under repeated start/stop"`. `stop()` now orders its `running` store under the bus mutex; all weak orderings in the file carry pairing comments. Discovery bonus: `events/bus.zig` was missing from `src/zombied/tests.zig`'s explicit import list, so its four pre-existing tests had **never executed** (same class as M90_003 §3's dead src/lib lane) — import added in the same diff per RULE TST; zombied suite grew 1516 → 1521, all passing.
- Jun 10, 2026 — §2 landed. Both rings now CAS-claim the head slot and publish a per-slot ready flag (mirrors `metrics_workspace.zig`); pop treats an unready head-of-line slot as empty for the pass. Tests: 4-thread integrity (no torn/lost/duplicate entries, drops exact), wraparound flag recycle, claimed-but-unready delivery — duplicated across both rings since the code is duplicated. Second discovery gap: `observability/otel_traces.zig` was also missing from `tests.zig` — its ten pre-existing tests had never run; wired in this diff (RULE TST). NLR cleanup on touch: `postWithBasicAuth`'s dead no-op tail branch now returns `error.OtlpExportRejected` so the existing caller warn logs non-2xx rejections (module-header claim becomes true). **Surfaced, not bundled** (per the NLR perf/structure discipline): (a) the two rings are byte-identical and want a shared `Ring(comptime Entry, capacity)` module — structure follow-up for Indy's fold-in-or-file call; (b) the audit's boot-time config-string leak in both `uninstall`s is a cross-file ownership question — proposed home: M90_003 §4 (amend at its CHORE(open)).
- Jun 10, 2026 — §3 landed. `lookupKey` is now a tri-state cache scan (hit / miss-on-fresh / miss-stale-or-none): kid miss on a fresh cache forces a refresh (per `docs/AUTH.md`), all refreshes are single-flight with the network round-trip outside the verifier mutex, attempts are rate-limited by `JWKS_REFRESH_MIN_INTERVAL_MS`, and a failed fetch keeps serving the prior key set (`jwks_stale_serve` warn). The old code destroyed the expired cache before fetching — an identity-provider outage left no keys at all. FLL split: standard-claims helpers extracted to `auth/jwks_standard_claims.zig` (re-exported; `claims.zig` consumers unchanged). Tests ride the existing inline-JWKS fixtures: rotation, rate-limit window, stale-serve, fail-closed-no-cache, and an 8-thread cold-cache storm asserting exactly one fetch.
- Jun 10, 2026 — **Pre-existing suite flake — diagnosis corrected:** the "expected .worker_started / expected 5, found 1" stderr lines are noise from *passing* negative tests (`expectError` catches the assert, the message still prints) — red herrings. The real failure: `zig build test` intermittently reports `failed command` while the binary itself reports `1216 passed; 0 failed` and **exits 0 when run standalone** — the failure exists only under the build-runner's `--listen=-` result protocol, i.e. something in the suite writes to stdout and intermittently corrupts the protocol stream. Reproduced on the unmodified base commit (`ebe6b4f6`, main + specs only) — pre-existing on main, not this branch. Follow-up candidate: find the stdout writer (CLI command tests / harness prints) and route it to stderr; until then `make test` can flake repo-wide.
- Jun 10, 2026 — §4 landed. The blocking `waitForDecision` poll loop is deleted; gate state machine: first encounter requests approval + records a `zombie:gate:byevent:` ref (`action_id|deadline_ms`, TTL ≥ deadline + grace) and answers `pending` → lease returns no-work; every later poll evaluates the ref (`approval_gate_async.zig`): approved → proceeds, denied → blocked, deadline passed → resolves `timed_out` via the existing single-winner `resolve()` (sweeper-compatible attribution). Redis blips stay `pending` — a transient read failure can never deny an approved gate. `GateResult` deleted with its pin test (only the wait consumed it); gate `timeout_ms` parse clamps at new `GATE_TIMEOUT_MS_MAX` (24h) + warn. Live-Redis tests gate on `REDIS_URL` (skip otherwise); ref parsing is covered pure. Terminal `gate_blocked` rows for denied/expired remain M90_002 scope, consuming this seam.
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
| Unit tests | `make test` | — | |
| Integration tests | `make test-integration` | — | |
| Memleak | `make memleak` | — | |
| Lint | `make lint` | — | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` | — | |
| Gitleaks | `gitleaks detect` | — | |
| Dead code sweep | `grep -rn waitForDecision src/` | — | |

## Out of Scope

- Terminal `gate_blocked` rows for denied/expired gate outcomes → **M90_002** (consumes this workstream's outcome variants).
- Work-indexed leasing (replacing the per-poll candidate scan) — separate design workstream; named here so §4 isn't mistaken for it.
- Lease deadline clock-skew tolerance (server-relative TTL in the lease payload) — follow-up candidate from the audit (`src/runner/child_supervisor.zig`).
- RESP header stack-parsing and logger filter-before-format hot-path savings — perf follow-ups, audit P2s.
- Per-frame memory-capture POST cadence — only activity frames batch here.
