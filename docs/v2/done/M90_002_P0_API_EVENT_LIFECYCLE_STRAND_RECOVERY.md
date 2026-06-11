<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh,
  which also assert the determinism-critical sections below are present and filled (not left as {placeholders}).
-->

# M90_002: Event lifecycle completion — terminal gate writes, strand recovery, loss-proof ingress

**Prototype:** v2.0.0
**Milestone:** M90
**Workstream:** 002
**Date:** Jun 10, 2026
**Status:** DONE
**Priority:** P0 — accepted deliveries can silently strand forever on the golden path (gate refusals, crashed runners, paused zombies, failed enqueues after dedup-claim); the architecture docs promise the missing half
**Categories:** API
**Batch:** B2 — after M90_001 (consumes its async-gate outcome variants in `fleet/service.zig`)
**Branch:** feat/m90-002-event-lifecycle (stacked on feat/m90-001-hardening at f18fa582 — rebase onto main after M90_001 merges)
**Test Baseline:** unit=1901 integration=168
**Depends on:** M90_001 (gate outcome seam: pending/denied/expired variants this workstream persists as terminal rows)
**Provenance:** LLM-drafted (Claude Fable 5, Jun 10, 2026) — from the Jun 10 full audit of `src/lib`, `src/zombied`, `src/runner`

**Canonical architecture:** `docs/architecture/data_flow.md` (`core.zombie_events` lifecycle: received → processed | agent_error | gate_blocked; gate_blocked never reopened), `docs/architecture/scenarios/03_balance_gate.md` (terminal write + XACK sequence), `docs/architecture/billing_and_provider_keys.md` (`failure_label='balance_exhausted'`). The code conforms to these docs; no architecture change.

---

## Implementing agent — read these first

1. `src/zombied/fleet/service_report.zig` — `markTerminal` + XACK pairing: the existing terminal-write pattern §1 mirrors for gate refusals.
2. `docs/architecture/scenarios/03_balance_gate.md` — the canonical blocked-event sequence (UPDATE status, failure_label, XACK terminal) this workstream implements.
3. `src/zombied/fleet/liveness_sweeper.zig` — the in-repo sweeper-loop shape (interval, shutdown, join) the reclaim sweep mirrors.
4. `src/zombied/queue/redis_zombie.zig` — `xautoclaimZombie` already exists with zero callers; §2 wires it rather than rewriting it.
5. `src/zombied/http/handlers/webhooks/github.zig` — header comments document the dedup-slot protection intent §3 completes; also the 200-ignored response shape §4 reuses.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m90): terminal gate writes, stranded-delivery reclaim, loss-proof webhook dedup`
- **Intent (one sentence):** Every accepted delivery ends in a terminal row or stays leasable — users see why nothing ran instead of waiting on a silent forever-pending event.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate the intent and list `ASSUMPTIONS I'M MAKING: …`; mismatch → STOP and reconcile.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — **RULE OBS** (every refusal/ignore/reclaim branch logs with scope + ids), **RULE IDMP** (terminal statuses must not block re-request: a resent event after gate_blocked is a NEW delivery), **RULE ECL** (enqueue failure ≠ duplicate), **RULE UFS** (failure_label values + ignore reasons are named consts at one ownership site), **RULE EMS/ERR** (new registry code for paused-zombie 409), **RULE TGU**, **RULE NLR** (dead blocking-read variant deleted in the same diff that touches the file), **RULE ORP/CHR** (deleted symbols swept), **RULE ITF** (integration fixtures through real schema), **RULE DRAIN**, **RULE TST/TST-NAM**.
- `dispatch/write_zig.md` — pg lifecycle (PgQuery, drain), Resource Budget (sweep loop bounds), Concurrency, Comptime-Gated Assertions (constant relations).
- `docs/REST_API_DESIGN_GUIDELINES.md` — 409 response shape for the paused-zombie refusal.
- `docs/LOGGING_STANDARD.md` — scoped-logger shape for the new branches.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — all-Zig diff | read façade; cross-compile both linux targets |
| ERROR REGISTRY | yes — paused-zombie 409 code; failure_label consts | registry entry with hint; labels as named consts (not registry codes — they are row values) |
| UFS | yes — failure_label / ignore-reason strings | one declaration site; webhook + steer + tests import it |
| LOGGING | yes — refusal/reclaim/ignore branches | `<scope>.<state>` + req_id/zombie_id/delivery per RULE OBS |
| PUB / Struct-Shape | yes — sweep entrypoint surface | shape verdict per new pub |
| File & Function Length | yes — `service.zig` mid-size | extract per Module Split Pattern if approaching caps |
| SCHEMA | no — status/label are application-level values (RULE STS keeps them out of SQL) | N/A |
| UI / DESIGN TOKEN | no | N/A |

---

## Overview

**Goal (testable):** A delivery refused by any lease gate gets `status='gate_blocked'` + a named `failure_label` + XACK and an `event_complete` frame; a delivery stuck in a dead consumer's Pending Entries List (PEL) is reclaimed and re-leased; a webhook whose enqueue fails is deliverable on the sender's retry; steering a paused zombie returns 409 and a webhook to a paused zombie returns 200-ignored — nothing accepted ever waits forever silently.

**Problem:** `fleet/service.zig` returns no-work on balance-exhausted / tenant-resolve-failure / approval-block with no terminal write and no XACK — the row stays `received` forever and the stream entry strands (the reclaim helper has zero callers; consumer ids are minted per probe so PEL entries are orphaned under throwaway names and consumer groups grow unboundedly). A missing credential silently ships a lease with no secrets map — the run "succeeds" with a garbage diagnosis. Webhook handlers claim the dedup key before XADD, so a transient enqueue failure makes the sender's retry come back "duplicate" — the event is lost permanently. Steering a paused zombie 202s and hangs forever. The docs (scenario 03, data_flow, billing) promise exactly the missing behaviour.

**Solution summary:** Implement the documented terminal half of the lifecycle: gate refusals write `gate_blocked` rows with named labels, XACK, and emit the completion frame; secret-resolution failure refuses the lease the same way instead of degrading silently; a background sweep (existing helper, new caller) reclaims stale PEL entries under a stable per-runner consumer identity; dedup keys are claimed only after a successful enqueue; paused zombies refuse ingress loudly on both the steer and webhook paths.

---

## Prior-Art / Reference Implementations

- **Terminal write + XACK pairing** → `fleet/service_report.zig` `markTerminal` path; §1 adds the blocked variant beside processed/agent_error, same row-update discipline (`fleet/event_rows.zig`).
- **Sweeper loop** → `fleet/liveness_sweeper.zig` (interval, stop signal, join) — mirror; do not invent a new loop shape.
- **409 + error envelope** → `docs/REST_API_DESIGN_GUIDELINES.md` + nearest zombies handler using `common.errorResponse`.
- **200-ignored webhook response** → existing `{"ignored": …}` shape in `handlers/webhooks/github.zig`.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/zombied/fleet/service.zig` | EDIT | gate refusals → terminal write + XACK; secret-missing refuses lease |
| `src/zombied/fleet/event_rows.zig` | EDIT | blocked terminal write (status + failure_label, guarded UPDATE) |
| `src/zombied/fleet/secrets_resolve.zig` | EDIT | all-or-nothing failure surfaces a typed refusal (no silent null map) |
| `src/zombied/fleet/assign.zig` | EDIT | stable consumer identity threaded into reads; own-PEL (`"0"`) read before `">"` |
| `src/zombied/fleet/reclaim_sweeper.zig` | CREATE | background reclaim sweep loop (mirrors `liveness_sweeper`; needs `pg.Pool` for zombie enumeration — wrong layer for `queue/`) |
| `src/zombied/queue/redis_client.zig` | EDIT | consumer id stable per `zombied` instance (host-derived, timestamp-free), reused across probes |
| `src/zombied/queue/redis_zombie.zig` | EDIT | sweep caller wiring; delete dead blocking-read variant (RULE NLR) |
| `src/zombied/queue/constants.zig` | EDIT | sweep cadence/min-idle consts; delete dead block-ms const |
| `src/zombied/cmd/serve_background.zig` | EDIT | reclaim sweep joins the background worker set |
| `src/zombied/http/handlers/webhooks/zombie.zig` | EDIT | dedup claim moved after successful enqueue |
| `src/zombied/http/handlers/webhooks/github.zig` | EDIT | same ordering fix incl. normalize-failure path; paused → 200-ignored |
| `src/zombied/http/handlers/zombies/messages.zig` | EDIT | paused zombie → 409 with registry code |
| `src/zombied/errors/error_registry.zig` + `error_entries.zig` | EDIT | paused-zombie code + hint |
| `src/zombied/http/handlers/common.zig` | EDIT | `errorResponseConflict` — 409s carry the REST §4 `current_state` extension via a private `writeProblem` core |
| `public/openapi/components/schemas.yaml` | EDIT | `ErrorBody` gains the optional `current_state` property (typed access for the 409) |
| `public/openapi/paths/zombies.yaml` | EDIT | messages endpoint documents 409 (UZ-ZMB-012); generic-webhook 200 documents the `ignored` shape; stale UZ-WH-003 409 removed |
| `public/openapi/paths/webhooks.yaml` | EDIT | github 200 gains `zombie_paused` ignore reason; svix path documents its 200 duplicate/ignored shape |
| `public/openapi.json` | REGEN | bundle-in-sync artifact (`make check-openapi`) |
| `make/test-integration.mk` | EDIT | `_reset-test-db` flushes Redis with the schema drop — strand recovery makes prior-run stream state reachable |
| sibling `*_test.zig` / integration tests per touched module | CREATE/EDIT | per Test Specification; incl. C1 generic-route dedup injection + `signLinear` helper + gates-table teardown fix |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** four slices along the delivery lifecycle: terminal writes (§1), reclaim (§2), ingress ordering (§3), ingress refusal (§4). Each independently testable against the documented sequence.
- **Alternatives considered:** (a) dead-letter stream instead of terminal rows — rejected: contradicts the documented `gate_blocked` row design the Command-Line Interface (CLI)/dashboard already query; (b) `XGROUP DELCONSUMER` on every release instead of stable ids — rejected: stable identity also fixes PEL orphaning and is one change instead of a per-release round-trip.
- **Patch-vs-refactor verdict:** **patch** — every slice lands inside the existing lease/report architecture; no flow is redesigned, the documented missing half is filled in.

---

## Sections (implementation slices)

### §1 — Terminal writes for gate refusals

Every lease-path refusal that is not retryable-by-waiting writes the documented terminal row (status `gate_blocked`, named `failure_label`, guarded `UPDATE … WHERE status='received'`), XACKs the stream entry, and publishes the `event_complete` frame with the blocked status — mirroring `markTerminal`. Labels (named consts, one ownership site): balance exhausted, tenant resolve failed, secret missing, approval denied, approval expired. Secret-missing is a behaviour change: the lease is refused instead of issued with a null secrets map (RULE ESO — no silent default substitution). Every branch logs per RULE OBS.

- **Dimension 1.1** — balance-exhausted delivery → gate_blocked row + label + XACK + frame → Test `test_balance_gate_writes_terminal_row` — **DONE**
- **Dimension 1.2** — tenant-resolve failure → same shape, its own label → Test `test_tenant_resolve_failure_blocks_event` — **DONE**
- **Dimension 1.3** — missing credential → lease refused, gate_blocked + secret-missing label; no lease ships without its declared secrets → Test `test_secret_missing_refuses_lease` — **DONE**
- **Dimension 1.4** — denied/expired gate outcome (M90_001 variants) → gate_blocked + label → Test `test_approval_denied_blocks_event` — **DONE**

### §2 — Stranded-delivery reclaim under stable consumer identity

**Amended at PLAN (see Discovery):** Redis streams have no requeue — an entry XAUTOCLAIMed by a sweep lands in the claiming consumer's Pending Entries List (PEL) and stays invisible to `XREADGROUP ">"`, so a background sweep alone cannot re-enter events into the lease flow. Per `docs/architecture/data_flow.md` (`zombied` is the Redis consumer; runners never touch Redis), the consumer identity is stable per `zombied` instance (host-derived, timestamp-free — no per-probe minting → consumer-group growth bounded, PEL entries recoverable). The lease read checks the stable consumer's own PEL (`"0"`) before `">"` — this is what makes M90_001's "next lease poll re-evaluates the recorded gate ref" true. The existing `xautoclaimZombie` gains its production caller: a background sweep (new `fleet/reclaim_sweeper.zig`, mirrors `liveness_sweeper`, joins `serve_background.Threads`) claims entries idle past a min-idle bound that comptime-relates to the lease window (reclaim must never race a live lease; the per-zombie `affinity.claim` is the first belt — the PEL read runs only after winning a claim with no active lease) from dead consumer names into the live consumer, re-entering them into the lease flow, logging each reclaim. Dead blocking-read variant + its constant are deleted in the same diff (RULE NLR).

- **Dimension 2.1** — repeated probes use one consumer id; group size stays constant → Test `test_consumer_identity_stable_across_probes` — **DONE**
- **Dimension 2.2** — delivery in a dead consumer's PEL beyond min-idle → reclaimed, re-leased, processed → Test `test_reclaim_sweep_recovers_stranded_delivery` — **DONE**
- **Dimension 2.3** — entry inside a live lease window is never reclaimed (min-idle > lease deadline, comptime-asserted) → Test `test_reclaim_respects_live_lease` — **DONE**

### §3 — Loss-proof webhook dedup ordering

The dedup slot is claimed only after the event is durably enqueued (or released on every post-claim failure path — including normalize-failure). A transient enqueue failure leaves the sender's retry deliverable; a genuinely duplicate delivery still dedupes. Applies to both webhook handlers.

- **Dimension 3.1** — injected enqueue failure → 5xx, dedup not burned; sender retry delivers exactly one event → Test `test_enqueue_failure_keeps_retry_deliverable` — **DONE**
- **Dimension 3.2** — duplicate delivery id after success → deduped (regression pin) → Test `test_duplicate_delivery_still_deduped` — **DONE**

### §4 — Paused-zombie ingress refusal

Steer on a paused zombie returns 409 with a registered error code + hint (resume instruction). Webhook to a paused zombie returns the existing 200-ignored shape with a named reason const, does not increment the triggered metric, and logs. In-flight leases for a zombie paused mid-run are untouched (only ingress refuses). **Implementation default:** webhook gets 200-ignored (not 4xx) because sender retry queues add no value for an intentionally-paused zombie; steer gets 409 because an interactive caller can act on it.

- **Dimension 4.1** — steer paused → 409 + code + hint; resumed zombie steers fine → Test `test_steer_paused_zombie_409` — **DONE**
- **Dimension 4.2** — webhook paused → 200-ignored + reason + no trigger metric + log → Test `test_webhook_paused_zombie_ignored` — **DONE**

---

## Interfaces

```
POST /v1/workspaces/{ws}/zombies/{id}/messages  (paused zombie)
  → 409 problem-details {error_code:"UZ-ZMB-012", detail, current_state:"<actual status>"}
    (code registered this workstream; current_state per REST guide §4 conflict extension)
Webhook ingress (paused zombie)
  → 200 {"ignored": <named reason const>}                      (existing ignored shape)
core.zombie_events terminal rows (visible via existing events API/CLI):
  status='gate_blocked', failure_label ∈ {named consts: balance_exhausted, tenant resolve, secret missing,
  approval denied, approval expired} — balance_exhausted spelling pinned by billing doc.
Ordering: terminal DB write commits BEFORE XACK (crash between → delivery remains reclaimable, never lost).
Runner↔zombied wire shape: UNCHANGED.
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Enqueue (XADD) fails | Redis blip | 5xx to sender, dedup slot free; retry delivers |
| DB down during terminal write | Postgres blip | XACK not issued; row stays received; delivery reclaimed by sweep later — no loss |
| Sweep races a live lease | slow runner | min-idle > lease window (comptime relation) — reclaim impossible while leased |
| Re-steer after gate_blocked | user retries | NEW delivery accepted (RULE IDMP — terminal status never blocks re-request) |
| Paused mid-flight | user pauses during run | in-flight lease completes; only new ingress refused |
| Duplicate webhook delivery | sender retry after success | deduped; logged |
| Resolve/secret backend down | vault/provider lookup fails | gate_blocked + label; operator sees logs, user sees failure_label via events |

---

## Invariants

1. Terminal write commits before XACK — failure-injection test proves a crash between the two leaves the delivery reclaimable (never acked-and-lost).
2. `gate_blocked` rows are never reopened — guarded `UPDATE … WHERE status='received'`; test attempts a second transition and asserts zero rows affected.
3. Reclaim min-idle > lease deadline window — comptime assertion relating the two named consts.
4. Consumer-group cardinality is bounded by fleet size — integration test: N probes, one runner → exactly one consumer in `XINFO`.
5. No lease ships with a missing declared secret — refusal is typed; test asserts no lease payload with null secrets map when the zombie declares secrets.
6. Every failure_label string has exactly one declaration site — RULE UFS pre-edit grep; tests import the consts.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_balance_gate_writes_terminal_row` | exhausted tenant (injected now past trial) → row gate_blocked + balance label, PEL empty, frame published |
| 1.2 | integration | `test_tenant_resolve_failure_blocks_event` | unresolvable tenant provider → gate_blocked + label |
| 1.3 | integration | `test_secret_missing_refuses_lease` | zombie declares secret absent from vault → no lease, gate_blocked + label |
| 1.4 | integration | `test_approval_denied_blocks_event` | denied gate outcome → gate_blocked + label + XACK |
| 2.1 | integration | `test_consumer_identity_stable_across_probes` | 25 idle probes → XINFO consumer count == 1 |
| 2.2 | integration | `test_reclaim_sweep_recovers_stranded_delivery` | entry idle past bound in dead consumer → re-leased and processed |
| 2.3 | integration | `test_reclaim_respects_live_lease` | leased entry under bound → sweep claims nothing |
| 3.1 | integration | `test_enqueue_failure_keeps_retry_deliverable` | injected XADD failure then retry → exactly one event enqueued |
| 3.2 | integration | `test_duplicate_delivery_still_deduped` | same delivery id twice → second deduped response |
| 4.1 | e2e (real HTTP via TestHarness) | `test_steer_paused_zombie_409` | steer paused → 409 + code + `current_state:"paused"`; resume → 202 |
| 4.2 | integration | `test_webhook_paused_zombie_ignored` (A6 github + C2 generic) | webhook paused → 200 ignored, trigger metric unchanged, dedup slot not consumed; resume → delivers |

**Added at /review (post-implementation hardening):**

| Dimension | Tier | Test | Asserts |
|-----------|------|------|---------|
| 1.4 (expiry half) | integration | `approval deadline expiry writes the terminal row` | 1ms gate deadline lapses → gate_blocked + approval_expired + XACK (was denied-only) |
| Invariant 2 (terminal path) | integration | `terminal entry re-delivered from the PEL is re-acked, never re-executed` | a settled row whose XACK was lost → re-poll re-acks + issues no lease + row unchanged (red-green proven) |
| 3.1 (generic route) | integration | `C1: generic route — enqueue failure releases the dedup slot` | WRONGTYPE fault → 500, slot released, retry delivers once, replay dedupes |

Regression: existing webhook dedup, lease/report parity, and events-API suites stay green. Idempotency/replay: 3.2 (dedup) and the Failure-Modes re-steer row (new delivery accepted after terminal).

---

## Acceptance Criteria

- [x] `make lint` clean · `make test` passes (`make lint-all` + `make test-unit-all`, Jun 11, 2026)
- [x] `make test-integration` and `make test-integration-redis` pass (DB + Redis surfaces) — serialized local runs, Jun 11, 2026
- [x] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [x] `gitleaks detect` clean · no production file over 350 lines (over-350 files in diff are `*_test.zig`, exempt from the production cap)
- [x] Stranded-event scenario from scenario 03 reproduces the documented sequence — `markBlocked` guarded-transition pin (1.1 row mechanics) + `reclaim sweep recovers a stranded delivery from a dead consumer and re-leases it` (2.2) both green in the Jun 11, 2026 full-tier run
- [x] Dead blocking-read symbols gone — verify: Eval E8 (0 matches)

## Eval Commands (post-implementation)

```bash
# E1: gate refusal writes terminal rows (integration suite)
make test-integration 2>&1 | tail -5
# E2: Build — zig build
# E3: Tests — make test
# E4: Lint — make lint 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile — zig build -Dtarget=x86_64-linux 2>&1 | tail -3
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: 350-line gate (exempts .md) —
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
# E8: orphan sweep for deleted queue symbols (empty = pass)
grep -rnE "xreadgroupZombie\b|zombie_xread_block_ms" src/ | head
```

## Dead Code Sweep

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `xreadgroupZombie` (blocking variant; `xreadgroupZombieOnce` stays) | `grep -rnE "xreadgroupZombie\b" src/ \| head` | 0 matches |
| `zombie_xread_block_ms` | `grep -rn "zombie_xread_block_ms" src/ \| head` | 0 matches |

---

## Discovery (consult log)

- **Consults** — Architecture consult at PLAN (Jun 11, 2026, agent): §2's literal "background sweep re-enters events into the lease flow" is unimplementable — Redis streams have no requeue; XAUTOCLAIM moves an entry into the claiming consumer's PEL, invisible to `XREADGROUP ">"`. Reconciled against `data_flow.md` ("`zombied` is the consumer"; dead-RUNNER reclaim stays lease-layer via `reclaim.zig`, untouched): stable per-instance consumer id + own-PEL-first read + sweep consolidating dead-consumer strays. "Per runner identity" wording dropped — runners are not Redis consumers, and per-runner ids would orphan entries on runner retirement. ECL split: transient failures (pool acquire, Redis blip, gate `.unavailable`) and `.auto_killed` (zombie paused, event retained for resume) stay no-work, never terminal. §3 ordering: atomic `SET NX` claim + `DEL` release on every post-claim failure path (normalize + enqueue) — NOT check-then-claim-late: the existing B3 concurrency pin (5 concurrent identical deliveries → exactly one 202) and double-billing rule out a non-atomic window; the spec's §3 parenthetical blesses the release form. Surfaced to Indy in the PLAN message; auto-mode proceed.
- **REST §4 conflict-extension consult** (Jun 11, 2026, VERIFY): the new paused-409 omitted the guide-mandated `current_state` extension (envelope had no extension support; detail string is static). Surfaced as a gate flag; Indy: "Fix now in this branch". Implemented as a private `writeProblem` core (+`emit_null_optional_fields=false`, non-409 wire shape byte-identical) with `errorResponseConflict` carrying the row's actual status; test 4.1 asserts `"current_state":"paused"`. Pre-existing 409s → Out of Scope follow-up sweep.
- **Gates-table teardown poisoning** (Jun 11, 2026, VERIFY): two `state.tenant_billing*` tests failed deterministically in the full tier. Bisect (filtered sequential runs, no reset between): the §1 approval-denial test leaves a `core.zombie_approval_gates` row; the table is append-only (DELETE raises via trigger) and its zombie FK has no cascade → teardownZombies → teardownWorkspace → every later `teardownTenant` of the shared TEST_TENANT fails silently (`ignored: PG`) → billing rows leak across tests and insert-if-absent seeding no-ops against stale balances. Fix: `TRUNCATE core.zombie_approval_gates` in the lifecycle suite's cleanup (row triggers don't fire on truncate). Pre-existing latent twin: `approvals/inbox_integration_test.zig:92` plain DELETEs have failed silently against the trigger since they landed — follow-up, not in this diff's scope. Related infra fix in the same VERIFY pass: `_reset-test-db` now flushes Redis (fixed fixture ids persist streams/PELs across local runs; recovery makes them reachable).
- **Skill chain outcomes** — `/write-unit-test`: ledger surfaced 3 real gaps → added the generic-route dedup injection (C1/C2), the approval-expiry e2e, and the terminal-re-delivery regression. `/review` (6-agent adversarial: testing/maintainability/security/performance/api-contract + Claude adversarial): surfaced one HIGH (terminal re-execution F1/F2), two confirmed CRITICALs (stale UZ-WH-003 409, sweeper LIMIT-100 starvation), and a tail of perf/hygiene findings. Fixed in-PR: F1 terminal-recheck on PEL re-delivery (red-green proven) + F2 markTerminal `status='received'` guard; stale 409 removed + ErrorBody `current_state`; sweeper ArrayList leak on the error path; gethostname-fallback warning; `_reset-test-db` Redis flush; test-literal/dedup-prefix cleanups; the gates-table teardown TRUNCATE.
- **Deferred to a focused sweeper-hardening follow-up** (NOT spec Sections — implementation bounds on the new `reclaim_sweeper.zig`, fully correct at current pre-launch scale where the active-zombie set is < the batch limit): (1) `fetchActiveZombies` `LIMIT 100` with no keyset cursor never advances `updated_at` → fleets >100 active zombies leave the tail unswept; (2) `xautoclaimZombie` `COUNT 1` → up to 10 sequential round-trips/zombie where a batched `COUNT` would do one; (3) no shutdown check between zombies in `sweepOnce` → a Redis-down pass can delay graceful shutdown; (4) NOGROUP warn-spam for never-polled zombies. Surfaced to Indy in the /review report; none are reachable failures at current scale. **Other documented follow-ups:** webhook dedup keys on the unsigned `X-GitHub-Delivery` header (replay defense rests on HMAC secrecy — pre-existing); the ambiguous-XADD-outcome window (entry lands but client sees a transport error → redelivery double-enqueues — a Failure-Modes residual); `approvals/inbox_integration_test.zig:92` plain DELETEs that fail silently against the append-only trigger (pre-existing latent); 200-response `oneOf` modeling for sharper SDK codegen.
- **Deferrals (spec Sections)** — none; all spec Dimensions delivered + tested.

## Skill-Driven Review Chain (mandatory)

| When | Skill | Required output |
|------|-------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | clean; iteration count in Discovery |
| After tests pass, before CHORE(close) | `/review` | clean or every finding dispositioned |
| After `gh pr create` | `/review-pr` | comments addressed before human review |

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test-unit-all` | all lanes passed (zombied + zigrunner + ziglib + coverage + bundle); final test depth unit=1913 integration=169 vs baseline 1901/168 (+12/+1). One load-induced app-component flake on the final chain re-ran green in isolation (100% coverage; no TS touched by this diff) | ✅ Jun 11, 2026 |
| Integration tests | `make test-integration` | full suite green (1,604 tests, 10 env-skips) — after fixing the gates-table teardown poisoning (see Discovery); serialized run | ✅ Jun 11, 2026 |
| Redis integration | `make test-integration-redis` | green, serialized run | ✅ Jun 11, 2026 |
| Lint | `make lint-all` | all linters + quality gates green (incl. `check-openapi` after the OpenAPI edits: 42 paths, error-schema + URL-shape OK) | ✅ Jun 11, 2026 |
| Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | both targets build clean | ✅ Jun 11, 2026 |
| Memleak | `make memleak` | allocator leak-guard tests + macOS `leaks` gate passed | ✅ Jun 11, 2026 |
| Gitleaks | `gitleaks detect` | 2596 commits scanned, no leaks found | ✅ Jun 11, 2026 |
| Dead code sweep | `grep -rnE "xreadgroupZombie\b\|zombie_xread_block_ms" src/` | 0 matches | ✅ Jun 11, 2026 |

## Out of Scope

- CLI rendering of `failure_label` in the events table + the credits-exhausted hint line, and doctor's tenant-provider/free-trial blocks — CLI/docs workstream; the labels this spec writes are what it will render.
- Budget caps enforcement (`daily_dollars`/`monthly_dollars` parsed but decorative) — separate gate workstream.
- Grant-approval nonce ordering + rows-affected handling (`webhooks/grant_approval.zig`) — audit P2, own follow-up (security-boundary review profile).
- Webhook provider registry routing beyond github/generic/Svix — `user_flow.md` §8.3 drift, product decision first.
- Fencing predicate on runner memory writes (`handlers/runner/memory.zig`) — audit P2 follow-up.
- Pre-existing 409s (`UZ-ZMB-010` stop-conflict, api-keys, approvals) predate the REST §4 `current_state` mandate and omit it — follow-up sweep workstream; this spec brings only its own new 409 into compliance (Indy-directed, Jun 11, 2026).
