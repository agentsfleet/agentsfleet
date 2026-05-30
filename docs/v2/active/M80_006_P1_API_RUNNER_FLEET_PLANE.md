# M80_006: Fleet operator plane + heartbeat-driven reassignment + per-lease renewal

**Prototype:** v2.0.0
**Milestone:** M80
**Workstream:** 006
**Date:** May 27, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — operators can't see/revoke runners, dead runners are only reclaimed lazily after the TTL, and any agent running longer than 30s is killed + redone (the renewal gap M80_002 ships with).
**Categories:** API
**Batch:** B1
**Branch:** feat/m80-006-fleet-plane
**Depends on:** M80_002 (lease/fencing/reclaim + the activity stream this renews from), M80_005 (trust fields the inventory surfaces)
**Provenance:** agent-generated (Opus 4.7, May 27, 2026 — from `runner_fleet.md` S5 + the M80_002 §6 renewal-gap gating decision)

> **Provenance is load-bearing.** LLM-drafted. The renewal half is the documented fix for the gap recorded in `M80_002` Failure Modes + §6 — the implementing agent reads that gating note and `src/lib/common/constants.zig` first.

**Canonical architecture:** `docs/architecture/runner_fleet.md` (S5 FLEET row + "System Guarantees / Failure Recovery Model" — SLA target · mechanism · tradeoff · the M80_006 path).

---

## Implementing agent — read these first

1. `docs/v2/done/M80_002_P1_API_RUNNER_CUTOVER.md` — Failure Modes ("Agent outruns the lease TTL") + the §6 gating decision; this workstream is the named fix. Do not flip §6 runner-as-default for >30s agents until this lands. (M80_002/005 landed in `done/` since this spec was drafted.)
2. `src/lib/common/constants.zig` `LEASE_TTL_MS` — the 30s lease/affinity validity window; renewal decouples it from execution duration.
3. `src/zombied/fleet/assign.zig` + `reclaim.zig` — **reclaim is PULL-triggered, not a sweep.** A runner's `lease` poll → `listCandidates` → `affinity.claim` wins iff `leased_until < now` → `reclaimPriorActive` marks the dead holder's `active` lease `expired` and re-leases under a higher fencing token. There is NO background timer; proactive reassignment (§2) adds heartbeat-lapse detection on top, the pull-triggered claim stays the backstop.
4. `src/zombied/fleet/affinity.zig` — reclaimability is gated by `runner_affinity.leased_until < now`, a **separate row** from `runner_leases.lease_expires_at`. Renewal MUST extend both atomically (see Invariant 1).
5. `src/zombied/fleet/service.zig` — lease issue debits billing once (`balanceCoversEstimate` → `debitReceive`/`debitStage`); a reclaim reuses prior billing (no re-charge). Renewal reuses `balanceCoversEstimate` as the credit guard.
6. `src/zombied/fleet/service_activity.zig` — the per-lease activity stream is **best-effort/cosmetic** (202 no-ack, no fence, no status filter). Renewal does NOT ride it; it gets a dedicated fenced verb so the cosmetic path stays cosmetic.
7. `src/runner/daemon/loop.zig` + `child_supervisor.zig` — the runner `heartbeat→lease→execute→report` loop; `drain` is SIGTERM-only today (cordon wires the heartbeat reply into it). The child read-loop uses one fixed deadline; renewal makes it track a shared deadline updated by `/renew` responses.
8. `docs/AUTH.md` §287 — the operator fleet plane is **`platformAdmin()`-authenticated** (Clerk JWT `platform_admin=true`), the same gate as runner enrollment, NOT tenant-`admin`: the fleet is operator-owned cross-tenant infra holding every tenant's secrets. Addressing a runner by `{id}` is legitimate (not a runner authenticating about itself).

---

## PR Intent & comprehension handshake

- **PR title (eventual):** add the fleet operator plane and decouple lease liveness from execution duration
- **Intent (one sentence):** operators can list and revoke runners, a dead runner's leases are reassigned the moment its heartbeat lapses (not after a lazy TTL sweep), and a healthy long-running agent renews its lease so it is never killed + redone mid-run.
- **Handshake (completed at PLAN, May 29 2026 — Codex adversarial review + Indy decisions, see Discovery):** intent confirmed. Key assumption confirmed and sharpened: renewal extends the lease from a **progress-bearing** signal, with a **separate hard max-runtime cap**. Refinements that landed in the design: renewal is a **dedicated fenced verb** (not the cosmetic activity path), it **atomically extends both `affinity.leased_until` and `lease.lease_expires_at`** under the live fence (the single highest-risk invariant), heartbeat-lapse reassignment **expires the affinity slot only** (never the lease row, which is the durable no-re-bill reclaim source), the operator plane is **`platformAdmin()`**, cordon **drains to other healthy hosts** (never back to itself), and renewal is **credit-gated** + **not extended during true dormancy**.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (TTL/renewal-window/max-runtime constants single-sourced in `constants.zig`, shared verbatim runner↔zombied), NLG (extend the pull-triggered reclaim in place; no `_v2` twin).
- **`docs/ZIG_RULES.md`** — handlers + reclaim + renewal + the `/renew` verb are `*.zig` (pg-drain on every query — heartbeat/activity/renewal are hot paths, `.drain()` discipline is mandatory; tagged-union results, multi-step `errdefer`, cross-compile).
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — the new `/v1/fleet/runners` operator routes + the runner-self `/v1/runners/me/leases/{id}/renew` verb: §URL design, 5-place route registration, `Hx` handler signature, error envelopes.
- **`docs/SCHEMA_CONVENTIONS.md`** — no schema change planned: the hard cap reuses the lease's existing `created_at`, and cordon/revoke reuse the app-enforced `status` column (no static `CHECK`). `last_renewed_at` is observability-only and out of scope.
- **`docs/AUTH.md`** — operator routes are **`platformAdmin()`** (Clerk JWT `platform_admin=true`, the runner-enrollment gate), NOT tenant-`admin`; the runner self-plane is unchanged; the `/renew` verb is `runnerBearer` like the other self-plane verbs.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — handlers/reclaim/renewal/`/renew` `*.zig` | cross-compile; pg-drain audit on every query (hot paths) |
| PUB / Struct-Shape | yes — fleet handlers + `/renew` handler + renewal helper surfaces | shape verdict per new pub; mirror existing fleet handler shape |
| File & Function Length | yes | split reclaim/renewal/fleet logic if a file nears 350; methods ≤50 |
| UFS | yes — `LEASE_TTL_MS` / `RENEWAL_WINDOW_MS` / `MAX_RUNTIME_MS` / `HEARTBEAT_LAPSE_MS` | named constants in `constants.zig`, shared verbatim runner↔zombied |
| SCHEMA GUARD | no — cap reuses `created_at`; cordon/revoke reuse app-enforced `status` | no `*.sql`/`embed.zig` change; `last_renewed_at` is out of scope |
| ERROR REGISTRY | yes — `UZ-RUN-009` runner_revoked · `010` lease_exceeded_max_runtime · `011` lease_lost (renew on a reclaimed lease) · `012` lease_renewal_no_credits | declare before use; mirror in runner `client_errors.zig` where the runner observes them (009/011/012 are runner-observed) |
| LOGGING | yes — reassignment + renewal + revoke + cordon-drain emits | logfmt with `error_code`/`runner_id`/`lease_id`/`fencing_token`; no secrets |

---

## Overview

**Goal (testable):** (a) `GET /v1/fleet/runners` returns the fleet with liveness + current-lease state and `PATCH` cordons (drain) or revokes (hard cut) a runner, `platformAdmin()`-only; (b) when a runner's heartbeat lapses, its affinity slots are expired so the next poll reassigns its event to *another* healthy runner within one poll cycle; (c) a runner auto-renews via the fenced `/renew` verb so a >30s healthy run is never reclaimed (both rows extended atomically), while a hard max-runtime cap and a credit gate still terminate a runaway or a broke tenant — asserted by `test_fleet_inventory_lists_runners`, `test_heartbeat_lapse_reassigns_to_other_host`, `test_active_lease_renews_past_ttl`, `test_renew_extends_both_affinity_and_lease`, `test_hard_max_runtime_caps_renewal`.

**Problem:** three operator-plane gaps the cutover shipped with. (1) No fleet visibility/control — an operator can't list runners or revoke a compromised one. (2) Recovery is lazy — a dead runner's event waits out the full `LEASE_TTL_MS` before reclaim. (3) The renewal gap — `LEASE_TTL_MS` doubles as both liveness and max execution time, so any agent running >30s is killed at its deadline and its event reclaimed + re-run (state stays correct via fencing, but the work is wasted and capped).

**Solution summary:** add a `platformAdmin()`-authenticated fleet plane (`GET`/`PATCH /v1/fleet/runners`) where cordon drains in-flight work to *other* healthy hosts and revoke is a hard cut; make heartbeat-lapse detection (piggybacked on existing poll/heartbeat traffic — there is no sweep to make faster) proactively reassign a lapsed runner's leases by **expiring its affinity slot only** so the next poll's `reclaimPriorActive` recovers the event with no re-bill (the pull-triggered claim stays the backstop); and decouple liveness from execution duration via a **dedicated fenced `/renew` verb** that atomically extends both `affinity.leased_until` and `lease.lease_expires_at` while the lease is still `active` and the runner is still the fencing holder. The runner auto-renews on progress-bearing frames (with a synthetic keepalive for quiet-but-active LLM calls) inside a renewal window, gated by a credit check and a separate hard `MAX_RUNTIME_MS` cap; the child's kill-deadline tracks the renewed deadline; renew failure degrades to today's TTL-expiry behavior and a `409 lease_lost` makes a reassigned runner self-terminate.

---

## Prior-Art / Reference Implementations

- **API (operator plane)** → `register.zig` + `platformAdmin()` (the runner-enrollment gate) for authz; the runner-self handlers under `src/zombied/http/handlers/runner/` for the `Hx` signature + 5-place registration the new routes mirror.
- **Reclaim** → `src/zombied/fleet/reclaim.zig` `reclaimPriorActive` (the atomic select-and-expire from M80_002) — proactive reassignment reuses it unchanged; heartbeat-lapse only expires the *affinity slot* (`affinity.zig`) so this stays the recovery path.
- **Renewal fencing** → `src/zombied/fleet/service_report.zig` — `report` already trusts the stored lease row + live affinity fence (it does NOT read a body token); the new `/renew` verb reuses exactly this fencing shape. The `activity` verb is NOT the renewal carrier (it stays cosmetic).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/zombied/http/handlers/fleet/*.zig` | CREATE | `GET /v1/fleet/runners` inventory + `PATCH /v1/fleet/runners/{id}` cordon/revoke (`platformAdmin()` plane) |
| `src/zombied/http/handlers/runner/renew.zig` | CREATE | `POST /v1/runners/me/leases/{id}/renew` — fenced dual-row renewal verb |
| `src/zombied/fleet/renewal.zig` | CREATE | atomic `affinity.leased_until` + `lease.lease_expires_at` extend under fence + status guard + credit/cap checks |
| route table + `router` + invoke wiring | EDIT | register the two operator routes + the `/renew` self-route (5-place registration per REST guide) |
| `src/zombied/fleet/assign.zig` | EDIT | `listCandidates` excludes cordoned/lapsed hosts; piggyback heartbeat-lapse affinity-slot expiry on the poll |
| `src/zombied/http/handlers/runner/heartbeat.zig` | EDIT | piggyback lapse scan; heartbeat reply carries `drain` for cordoned runners |
| `src/zombied/fleet/affinity.zig` | EDIT | helper to expire a lapsed peer's affinity slot (`leased_until := now`), fence-guarded |
| `src/runner/daemon/loop.zig` | EDIT | honor heartbeat-reply `drain` (cordon); drive `/renew` on progress/keepalive |
| `src/runner/child_supervisor.zig` | EDIT | read-loop tracks a shared renewed deadline; emit keepalive on quiet LLM calls; honor `409 lease_lost` |
| `src/lib/common/constants.zig` | EDIT | `RENEWAL_WINDOW_MS` + `MAX_RUNTIME_MS` + `HEARTBEAT_LAPSE_MS` alongside `LEASE_TTL_MS` |
| `src/zombied/errors/error_entries.zig` + `error_registry.zig` | EDIT | `UZ-RUN-009/010/011/012`; mirror runner-observed codes in `client_errors.zig` |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three slices — operator plane, proactive reassignment, lease renewal — landable independently; renewal is the one §6 blocks on.
- **Alternatives considered:** (a) just raise `LEASE_TTL_MS` to a large value — rejected, it slows dead-runner recovery (the backstop), and Indy's "make the lease N minutes" instinct is answered instead by §2: detection speed now comes from heartbeat-lapse (seconds), decoupled from the lease window, so the lease stays short; (b) renew on the per-runner heartbeat rather than per-lease progress — rejected, a runner can heartbeat while its agent is wedged, so renewal must be **progress-bearing**, not mere liveness; (c) renew by overloading the best-effort `activity` 202 response (the original plan lean) — **rejected** after Codex review: a best-effort, no-ack, no-fence verb is "fragile by design" as an authoritative deadline channel, and renewal makes that path safety-critical. Chosen instead: a **dedicated fenced `/renew` verb** that reuses `service_report`'s stored-lease + live-affinity fencing pattern and leaves `activity` cosmetic; (d) heartbeat-lapse pre-expiring the **lease row** — **rejected**: it destroys the durable reclaim source (`xreadgroupZombieOnce` only reads `>` undelivered, so the fresh path can't see the pending event and would re-bill it). Expire the **affinity slot only**.
- **Patch-vs-refactor verdict:** **refactor of the lease lifecycle** — it decouples two concerns (liveness vs execution duration) that `LEASE_TTL_MS` currently conflates, and makes renewal a fenced first-class operation across two rows. That is the right-sized change; a TTL bump or an activity-piggyback would be the mud-patch.

---

## Sections (implementation slices)

### §1 — Fleet operator plane

Delivers operator visibility + control: list runners with liveness + current lease, and cordon/revoke one. Why: a compromised or misbehaving runner must be removable, and an operator must see fleet state. **Authz:** `platformAdmin()` (Clerk JWT `platform_admin=true`) — the same gate as runner enrollment, NOT tenant-`admin`; the fleet is operator-owned cross-tenant infra. **Cordon vs revoke (resolves the auth landmine):** `runnerBearer` rejects any non-`active` runner row at auth, so a status flip alone hard-fails the runner's next call instead of draining. Therefore: **cordon** = `status='cordoned'`, which `runnerBearer` still accepts (auth-valid) so the runner can drain — `listCandidates` excludes it (no new leases) and the heartbeat reply returns `drain` so in-flight work finishes and reports; **revoke** = `status='revoked'`, which auth then rejects (`UZ-RUN-009`, hard cut, in-flight reclaimed via §2). Both reuse the app-enforced `status` column (no schema change). A cordoned/lapsed host's work reassigns to **other** healthy hosts, never back to itself (Indy: "must get leashed to the other hosts that are good to go").

- **Dimension 1.1** — `GET /v1/fleet/runners` (`platformAdmin()`) returns each runner's id, trust_class, status, liveness (`last_seen_at`/alive), and current lease → Test `test_fleet_inventory_lists_runners`
- **Dimension 1.2** — `PATCH /v1/fleet/runners/{id}` cordon: `status='cordoned'`, auth still valid, no new leases issued, heartbeat reply drains in-flight to other healthy hosts → Test `test_fleet_cordon_drains_to_other_host`
- **Dimension 1.3** — `PATCH /v1/fleet/runners/{id}` revoke: `status='revoked'`, runner's next call `403 UZ-RUN-009`, in-flight reclaimed via §2 → Test `test_fleet_revoke_hard_cuts_runner`
- **Dimension 1.4** — the plane is `platformAdmin()`-only: a tenant-`admin` JWT and a `zmb_t_` api_key both get `403` (not just a runner token) → Test `test_fleet_plane_rejects_tenant_admin_and_apikey`

### §2 — Heartbeat-driven proactive reassignment

Delivers fast recovery: when a runner's heartbeat lapses (`last_seen_at` older than `HEARTBEAT_LAPSE_MS`), mark it dead and make its leases reclaimable *now* rather than waiting out the TTL. **Mechanism (there is no sweep to speed up — reclaim is pull-triggered):** detection piggybacks on existing traffic — a surviving runner's poll/heartbeat opportunistically scans for lapsed peers and **expires their affinity slots only** (`leased_until := now`), leaving the `active` lease rows intact so the next poll's `reclaimPriorActive` recovers the event durably with no re-bill. "Within one detection cycle" = one poll interval (`NO_WORK_RETRY_AFTER_MS`), well under the TTL. The pull-triggered claim is the backstop for a runner that vanishes silently. The lapsed host is excluded from candidate selection, so work moves to other healthy hosts.

- **Dimension 2.1** — a lapsed-heartbeat runner's affinity slots are expired (lease rows untouched), so the next poll reclaims the event under a higher fencing_token within one cycle, on a *different* healthy runner → Test `test_heartbeat_lapse_reassigns_to_other_host`
- **Dimension 2.2** — the pull-triggered reclaim remains correct as the backstop (silent disappearance still recovers, no re-bill) → Test `test_pull_reclaim_still_backstops` (regression of M80_002 reclaim)
- **Dimension 2.3** — lapse detection expires the affinity slot ONLY, never the lease row; the reclaimed event is re-leased (not re-pulled fresh) so it is not re-billed → Test `test_lapse_expires_affinity_not_lease_no_rebill`

### §3 — Per-lease renewal (the §6 gating fix)

Delivers liveness/execution decoupling via a **dedicated fenced verb** `POST /v1/runners/me/leases/{id}/renew` (`runnerBearer`): it **atomically extends both `affinity.leased_until` AND `lease.lease_expires_at`** in one statement, guarded by `status='active'` AND the presenting runner still being the live fencing holder (mirrors `service_report`'s stored-lease + live-affinity fence). Returns the authoritative new deadline, or `409 UZ-RUN-011 lease_lost` if the lease was already reclaimed — on which the runner kills its child. The runner auto-renews (transparently, nobody renews by hand) inside a renewal window (`RENEWAL_WINDOW_MS` before expiry), driven by **child-liveness-attested progress**: a real progress frame, OR a **synthetic keepalive** emitted while the child is alive and a genuine operation is in flight (model call, long-running tool, inter-step processing). "Progress-bearing, not mere liveness" means *the runner attests an operation is genuinely in flight* — it cannot fake that for a dead host. Only a **truly stuck/dead** child emits nothing and is NOT renewed (it expires + reclaims — the recovery path, never hit by a healthy agent). Renewal is **credit-gated** (`balanceCoversEstimate`; exhausted → `UZ-RUN-012`, run terminates) and bounded by a hard `MAX_RUNTIME_MS` cap (`UZ-RUN-010`), both checked server-side. The child's kill-deadline tracks the renewed deadline (the supervisor read-loop polls a shared deadline updated by `/renew` responses). **Renew-fail is fail-safe:** a transient failure retries on the next tick (the window leaves slack); if it can't renew by the deadline, the child is killed and the event reclaimed + redone elsewhere (never double-run — fencing).

- **Dimension 3.8** — **smooth-transition (no customer-visible flip).** A legitimate long-running agent (model call / long tool / inter-step) keeps its lease renewed via keepalive, so the Chat SSE tail and the terminal REPL never see a false restart. On a *real* host-death reclaim (§2), `zombied` publishes a `lease_reassigned` activity frame on `zombie:{id}:activity` so the UI/REPL renders continuity ("resuming on another runner") rather than a duplicate restart → Test `test_reassign_emits_resume_marker_no_visible_restart`

- **Dimension 3.1** — a lease whose runner renews past `LEASE_TTL_MS` is NOT reclaimed; both rows extend atomically; the same agent run completes once → Test `test_active_lease_renews_past_ttl`
- **Dimension 3.2** — renewal extends **both** `affinity.leased_until` and `lease.lease_expires_at`; a healthy long run is not reclaimed by the affinity-slot path → Test `test_renew_extends_both_affinity_and_lease`
- **Dimension 3.3** — `/renew` on an already-reclaimed lease (status≠active or stale fence) returns `409 UZ-RUN-011`; the runner kills its child; no resurrection / no double-exec → Test `test_renew_on_reclaimed_lease_rejected`
- **Dimension 3.4** — the child's kill-deadline tracks the renewed deadline (a renewed lease's child is not killed at the original 30s) → Test `test_child_deadline_tracks_renewal`
- **Dimension 3.5** — a hard `MAX_RUNTIME_MS` cap (from `created_at`) terminates a still-emitting runaway and reports it (`UZ-RUN-010`); renewal cannot extend past it → Test `test_hard_max_runtime_caps_renewal`
- **Dimension 3.6** — renewal refused when the tenant's credit balance is exhausted (`UZ-RUN-012`); the run terminates gracefully → Test `test_renew_refused_on_no_credits`
- **Dimension 3.7** — a dormant (non-renewing) lease still expires + reclaims exactly as today → Test `test_non_renewing_lease_still_reclaims` (regression)

---

## Interfaces

```
GET   /v1/fleet/runners        (platformAdmin) → [{ runner_id, trust_class, status, alive, last_seen_at, current_lease }]
PATCH /v1/fleet/runners/{id}   (platformAdmin) → { status: cordoned|revoked }
    cordon  → status=cordoned (auth-valid; no new leases; heartbeat reply drains to other hosts)
    revoke  → status=revoked  (auth rejects next call UZ-RUN-009; in-flight reclaimed via §2)

POST  /v1/runners/me/leases/{id}/renew   (runnerBearer; auto-driven by the runner)
    guard:   status='active' AND presenting runner is the live fencing holder
    effect:  ATOMIC, single statement —
               affinity.leased_until    := min(now + LEASE_TTL_MS, created_at + MAX_RUNTIME_MS)
               lease.lease_expires_at    := <same value>
    gates:   balanceCoversEstimate (else 402-style UZ-RUN-012)  ·  created_at+MAX_RUNTIME_MS cap (UZ-RUN-010)
    200 → { lease_expires_at }     409 UZ-RUN-011 lease_lost (reclaimed/stale fence → runner kills child)
  Runner renews on a progress-bearing frame (or synthetic keepalive for a quiet-but-active LLM call) inside
  RENEWAL_WINDOW_MS of expiry; never during true dormancy. Child kill-deadline tracks the renewed value.

constants (src/lib/common/constants.zig, shared verbatim runner↔zombied):
  LEASE_TTL_MS (lease/affinity window + renew increment) · RENEWAL_WINDOW_MS · MAX_RUNTIME_MS · HEARTBEAT_LAPSE_MS

errors (new): UZ-RUN-009 runner_revoked · UZ-RUN-010 lease_exceeded_max_runtime
              · UZ-RUN-011 lease_lost · UZ-RUN-012 lease_renewal_no_credits
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Runner heartbeat lapses | host crash / partition | lapse scan expires its **affinity slots only**; next poll reclaims to *another* host (higher fencing_token); old runner's late report fenced (`UZ-RUN-005`) → `test_heartbeat_lapse_reassigns_to_other_host` |
| Runner vanishes with no final heartbeat | hard kill | the pull-triggered claim backstops (TTL reclaim, no re-bill) → `test_pull_reclaim_still_backstops` |
| Healthy long agent (>30s) | slow legitimate run, emitting progress | runner auto-renews via `/renew`; both rows extend; not reclaimed; runs once → `test_active_lease_renews_past_ttl` |
| Late frame from a reclaimed runner | runner A lost lease to B, still emitting | A's `/renew` returns `409 UZ-RUN-011`; A kills its child; no resurrection, no double-exec/double-bill → `test_renew_on_reclaimed_lease_rejected` |
| Wedged agent still emitting | tight loop emitting frames | hard `MAX_RUNTIME_MS` cap terminates + reports (`UZ-RUN-010`); renewal can't extend past it → `test_hard_max_runtime_caps_renewal` |
| Tenant credits exhausted mid-run | concurrent spend drains balance | `/renew` refuses (`UZ-RUN-012`); run terminates at the deadline; no incremental over-spend → `test_renew_refused_on_no_credits` |
| Quiet-but-active LLM call | long model latency, no tool frames | synthetic keepalive renews so it isn't falsely reclaimed; a *truly dormant* agent is NOT renewed and expires |
| Renew call fails transiently | network/server blip mid-run | retried next tick (window leaves slack); if unrenewed by deadline → child killed, event reclaimed + redone elsewhere (fenced, never double-run) |
| Cordon a runner mid-lease | operator drains for maintenance | `status=cordoned`, auth still valid, no new leases, heartbeat `drain` finishes in-flight then moves to other hosts → `test_fleet_cordon_drains_to_other_host` |
| Revoke a runner mid-lease | compromised runner | `status=revoked`, next call `403 UZ-RUN-009`, in-flight reclaimed via §2 → `test_fleet_revoke_hard_cuts_runner` |
| Operator route hit by wrong principal | runner token / tenant-admin / api_key | `platformAdmin()` rejects all three; `403` → `test_fleet_plane_rejects_tenant_admin_and_apikey` |

---

## Invariants

1. **(Highest-risk)** Renewal is an **atomic, fenced extension of BOTH rows** — `affinity.leased_until` and `lease.lease_expires_at` are set in one statement, guarded by `status='active'` AND the live fencing holder. They can never diverge (if they could, a "renewed" run is still reclaimed via the affinity path). Enforced by the single-statement renewal + `test_renew_extends_both_affinity_and_lease` + `test_renew_on_reclaimed_lease_rejected`.
2. Renewal extends only on a **progress-bearing** signal (or in-flight-LLM keepalive), never mere liveness or true dormancy — enforced by the renewal call site being driven by progress frames, not the heartbeat path, + `test_active_lease_renews_past_ttl` / `test_non_renewing_lease_still_reclaims`.
3. Renewal can never extend past `created_at + MAX_RUNTIME_MS`, and is refused when credits are exhausted — enforced by `min(now+TTL, cap)` + the `balanceCoversEstimate` gate + `test_hard_max_runtime_caps_renewal` / `test_renew_refused_on_no_credits`.
4. The cosmetic `activity` verb stays cosmetic — renewal lives in the dedicated fenced `/renew` verb, so a dropped/forged activity frame can never mutate a deadline. Enforced by keeping `service_activity.zig` free of any lease-row write.
5. Heartbeat-lapse reassignment expires the **affinity slot only**, never the lease row — so the reclaim stays durable + no-re-bill. Enforced by the lapse path touching only `runner_affinity` + `test_lapse_expires_affinity_not_lease_no_rebill`.
6. A non-renewing lease expires + reclaims exactly as in M80_002 — enforced by the unchanged pull-triggered reclaim + `test_pull_reclaim_still_backstops`.
7. Fleet operator routes are **`platformAdmin()`-only** — a runner token, a tenant-`admin` JWT, and a `zmb_t_` api_key all get 403. Enforced by the route middleware + `test_fleet_plane_rejects_tenant_admin_and_apikey`.
8. A cordoned/lapsed host never receives its own work back — enforced by `listCandidates` excluding it + `test_heartbeat_lapse_reassigns_to_other_host` / `test_fleet_cordon_drains_to_other_host`.
9. `LEASE_TTL_MS` / `RENEWAL_WINDOW_MS` / `MAX_RUNTIME_MS` / `HEARTBEAT_LAPSE_MS` are single-sourced constants shared verbatim runner↔zombied — enforced by UFS (one `constants.zig` identifier each).
10. **No customer-visible flip for a healthy agent** — renewal is child-liveness-attested (keepalive covers every genuine in-flight state), so a legitimate long run is never falsely reclaimed; the Chat SSE tail / terminal REPL stay continuous. A real host-death reclaim publishes a `lease_reassigned` resume marker so the live-tail renders continuity, not a restart. Enforced by the keepalive trigger + `test_reassign_emits_resume_marker_no_visible_restart`.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_fleet_inventory_lists_runners` | seeded fleet → `GET` returns each runner's id/trust/status/liveness/lease |
| 1.2 | integration | `test_fleet_cordon_drains_to_other_host` | `PATCH` cordon → auth still valid, no new lease to it, heartbeat drains, in-flight moves to another host |
| 1.3 | integration | `test_fleet_revoke_hard_cuts_runner` | `PATCH` revoke → next call `403 UZ-RUN-009`; in-flight reclaimed via §2 |
| 1.4 | integration | `test_fleet_plane_rejects_tenant_admin_and_apikey` | tenant-`admin` JWT → 403; `zmb_t_` api_key → 403; runner token → 403; only `platform_admin` JWT passes |
| 2.1 | integration | `test_heartbeat_lapse_reassigns_to_other_host` | runner stops heartbeating → affinity slots expired → next poll reclaims to a *different* runner, higher token, within one cycle |
| 2.2 | integration | `test_pull_reclaim_still_backstops` | runner vanishes silently → next claim reclaims, no re-bill (M80_002 regression) |
| 2.3 | integration | `test_lapse_expires_affinity_not_lease_no_rebill` | lapse path touches `runner_affinity` only; lease row stays `active`; reclaimed event reuses billing (no re-charge) |
| 3.1 | integration | `test_active_lease_renews_past_ttl` | runner renews at t≈20s,40s → not reclaimed at 30s; one completion |
| 3.2 | integration | `test_renew_extends_both_affinity_and_lease` | `/renew` → both `affinity.leased_until` and `lease.lease_expires_at` advance to the same value atomically |
| 3.3 | integration | `test_renew_on_reclaimed_lease_rejected` | lease reclaimed by B → A's `/renew` → `409 UZ-RUN-011`; A's child killed; no double-exec/double-bill |
| 3.4 | integration | `test_child_deadline_tracks_renewal` | renewed lease → child not killed at original 30s deadline |
| 3.5 | integration | `test_hard_max_runtime_caps_renewal` | runner renews past `created_at+MAX_RUNTIME_MS` → terminated + `UZ-RUN-010` |
| 3.6 | integration | `test_renew_refused_on_no_credits` | balance exhausted → `/renew` → `UZ-RUN-012`; run terminates gracefully |
| 3.7 | integration | `test_non_renewing_lease_still_reclaims` | dormant lease emits nothing → expires + reclaims as today |

Regression: all of M80_002's fencing/reclaim tests stay green (renewal + proactive reassignment must not break stale-report fencing or row-equivalence). Replay: a reassigned lease's old holder's late report stays fenced (`UZ-RUN-005`). **Drain audit:** the `/renew` and lapse-scan queries are on hot paths — `make check-pg-drain` must stay clean.

---

## Acceptance Criteria

- [ ] Long healthy agent not reclaimed (both rows renewed); runaway capped; broke tenant stopped — verify: `test_active_lease_renews_past_ttl` + `test_renew_extends_both_affinity_and_lease` + `test_hard_max_runtime_caps_renewal` + `test_renew_refused_on_no_credits`
- [ ] Reclaimed runner can't resurrect its lease — verify: `test_renew_on_reclaimed_lease_rejected`
- [ ] Dead/cordoned runner reassigned to another host; silent death still backstopped, no re-bill — verify: `test_heartbeat_lapse_reassigns_to_other_host` + `test_pull_reclaim_still_backstops` + `test_lapse_expires_affinity_not_lease_no_rebill`
- [ ] Fleet inventory + cordon/revoke work and are `platformAdmin()`-only — verify: `test_fleet_inventory_lists_runners` + `test_fleet_cordon_drains_to_other_host` + `test_fleet_revoke_hard_cuts_runner` + `test_fleet_plane_rejects_tenant_admin_and_apikey`
- [ ] `make lint` clean · `make test` passes · `make test-integration` passes · `make check-pg-drain` clean
- [ ] Cross-compile both linux targets · `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: renewal + reassignment — make test-integration 2>&1 | grep -E "renews_past_ttl|reassigns_to_other_host|extends_both|max_runtime|lease_lost|no_credits|PASS|FAIL"
# E2: Build  — zig build
# E3: Tests  — make test && make test-integration
# E4: Lint   — make lint 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile — zig build -Dtarget=aarch64-linux 2>&1 | tail -3
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: pg-drain — make check-pg-drain 2>&1 | tail -3
```

---

## Dead Code Sweep

N/A — no files deleted. The pull-triggered reclaim is extended in place with a heartbeat-lapse affinity-expiry path (RULE NLR); the TTL-as-max-runtime coupling is removed, not twinned. The `activity` verb is left cosmetic (renewal does NOT fold into it), so no dead/duplicated renewal path is created.

---

## Discovery (consult log)

> **Empty at creation.** Append consults, skill-chain outcomes, and Indy-acked deferral quotes as work proceeds.

- **Provenance (May 27, 2026):** authored with M80_003/004/005 to formalize the remaining roadmap before M80_002's CHORE(close). The renewal half is the named fix for the gap recorded in M80_002 Failure Modes + §6; §6 (runner-as-default for >30s agents) blocks on this workstream.
- **Indy (May 27, 2026):** "fix cleanly in M80_006" — the renewal gap is to be solved here, not patched in M80_002 (TTL bump is the rejected mud-patch). — context: M80_002 ships with the documented gap; this spec owns the fix.
- **PLAN review (May 29, 2026):** codebase read surfaced that the spec's framing diverged from reality, confirmed by a Codex adversarial pass (`codex exec`, read-only, high reasoning). All five PLAN assumptions confirmed; refinements folded into §1–§3, Interfaces, Invariants, Failure Modes, Tests:
  - **A2 incomplete → Invariant 1.** Reclaimability is gated by `runner_affinity.leased_until` (`affinity.zig:53-67`), a *separate row* from `lease.lease_expires_at`. Renewing one row leaves a healthy run reclaimable at 30s. Renewal must atomically extend **both**, fenced + status-guarded. Codex: "renewal must be a fenced, atomic extension of both the affinity slot and the lease row… if they can diverge, the rest of the plan is bullshit."
  - **A3 corrected.** No sweep exists; reclaim is pull-triggered (`assign.zig:70-110`). Heartbeat-lapse must expire the **affinity slot only** — pre-expiring the lease row destroys the durable reclaim source (`xreadgroupZombieOnce` reads `>` undelivered only) and re-bills via the fresh path.
  - **A4 redesigned.** No mid-execution feedback channel exists; the `activity` verb is best-effort/no-ack/no-fence and "fragile by design" as an authoritative deadline channel. Chosen: a dedicated fenced `/renew` verb (reuses `service_report`'s fence pattern); `activity` stays cosmetic.
  - **A1 confirmed → `platformAdmin()`** (not tenant-`admin`); also rejects `zmb_t_` api_keys by design.
  - **Cordon landmine.** `runnerBearer` rejects non-`active` rows at auth (`serve_runner_lookup.zig:37-55`), so a status flip hard-fails instead of draining → cordon (`status=cordoned`, auth-valid) vs revoke (`status=revoked`, hard cut).
- **Indy decisions (May 29, 2026), verbatim:**
  > "Just using the standard auth with platformAdmin() to manage runners." — A1 locked: `platformAdmin()` JWT-only; key-based scripting is a future spec.
  > "when we cordon, it shouldnt go to the same host? it must get leashed to the other hosts that are good to go?" — cordon/lapse reassign to *other* healthy hosts only (Invariant 8); `listCandidates` excludes the cordoned/lapsed host.
  > "what happens if renew fails? and i am in the middle of something? I suggest we assume an automatic renewal? or if the billing credits are none? Should we make the lease by default <x> mins since all autonomous agents are > 2 mins? and what happens if nothing the agent does? I suppose the lease gets extended even during remaining dormant?"
  Resolutions: renewal is **automatic** (runner-driven, never manual); renew-fail is **fail-safe** (retry in-window → else deadline-kill + reclaim, never double-run); **credit-gated** (`UZ-RUN-012`, minimal guard; incremental metering deferred to its own spec — flagged for ack); base lease **stays short** (detection = `HEARTBEAT_LAPSE_MS`, not the lease window); a **truly dormant** agent is **NOT** renewed (progress-bearing only, Invariant 2), a quiet-but-active LLM call uses a synthetic keepalive. `MAX_RUNTIME_MS` proposed at 30 min (tunable).

---

## EXECUTE progress (checkpoint — May 30, 2026 · §3 complete, DB-verified)

Committed on `feat/m80-006-fleet-plane`, all green (HARNESS VERIFY + `make lint-zig`; both binaries build):

- ✅ **Foundation** (`ab7da4a8`): `constants.zig` (`RENEWAL_WINDOW_MS`, `MAX_RUNTIME_MS`, `HEARTBEAT_LAPSE_MS`); `UZ-RUN-009/010/011/012` + runner mirrors.
- ✅ **§3 server-side renewal** (`004e32e0`): the highest-risk invariant is implemented — `renewal.zig` extends both `affinity.leased_until` and `lease.lease_expires_at` atomically (one writable-CTE, fence + status + cap guarded); `service_renew.zig` (credit gate + `last_seen_at` bump); `/renew` verb 5-place wired; `RenewResponse`.
- ✅ **§3 renewal integration tests** (`8c578302`): `renewal_integration_test.zig` drives `renewal.renew` with deterministic `now_ms`. Covers both-rows-advance (the divergence guard), stale-fence → lost, cap-reached → max_runtime + the clamp-to-cap boundary. Registered in `main.zig`; milestone-free (RULE TST-NAM); build/fmt/lint/HARNESS-VERIFY clean. **Compiles + skips without DB; runs green pending a live DB run (`LIVE_DB=1`).**

- ✅ **§3 runner client `renew()`** (`3dbcb774`): `control_plane_client.zig` `renew()` — 2xx → `renewed(deadline)`, definitive 4xx → `terminal(status)` (kill child), transport/5xx → error (retry next tick, fail-safe).
- ✅ **§3 runner-side renewal driving** (`7b609ffc`): `pipe_proto.waitReadable` (tick only in the idle gap between frames — never mid-frame, no desync); `child_supervisor.RenewHook{ctx, onTick, tick_ms}` + the tick loop in `readResult` (tick or progress frame → keep/extend/terminate; live-but-quiet child still ticks = synthetic keepalive = Invariant 10); `loop.zig`/`renew_driver.zig RenewDriver` renews inside the window via `cp.renew`, fail-safe on transient errors. Deterministic unit tests (injected 10ms tick): terminate-on-tick, extend-past-deadline.
- ✅ **§3 DB-verified + headroom split** (`ebde68ce`): first live `make test-integration` run found two test bugs (missing `seedTenant`; double-open `pg.Conn` result in `readDeadlines`) — both fixed. Extracted `child_process.zig` (fork/exec/kill/writeAll) out of `child_supervisor.zig` (349→294) for real line-budget headroom. **Full suite 1409/1409 green against real DB + Redis.**

**§3 is COMPLETE and DB-verified** (server dual-row renewal + runner client + desync-safe supervisor driving + unit + integration tests all green on live DB). Dimensions 3.1–3.5/3.7 are exercised by `renewal_integration_test.zig`; 3.2/3.4 (child-deadline tracking) + 3.8 (smooth-transition) by the supervisor unit tests. Mark DONE at CHORE(close) after `/review`.

**Remaining:**
- **§1 operator plane**: `GET/PATCH /v1/fleet/runners` (`platformAdmin()`), cordon (`status=cordoned`, auth-valid, drain) / revoke (`status=revoked`, hard cut), `listCandidates` exclusion of cordoned, heartbeat-reply `drain` wiring, + tests (inventory, cordon-drains-to-other-host, revoke-hard-cut, rejects-tenant-admin-and-apikey).
- **§2 heartbeat-lapse**: affinity-slot-only expiry piggyback on poll/heartbeat + `listCandidates` exclusion of lapsed + `lease_reassigned` activity frame; tests (reassign-to-other-host, pull-reclaim-backstop, expires-affinity-not-lease-no-rebill).
- **VERIFY/CLOSE**: `/write-unit-test` coverage audit, cross-compile both linux targets, `gitleaks`, `/review`, mark Dimensions DONE, spec → `done/`, changelog `<Update>`, CHORE(close), PR + `/review-pr` + `kishore-babysit-prs`.

**Open for Indy:** `MAX_RUNTIME_MS = 30 min` default OK? · incremental per-renewal metering deferred to its own spec (confirm)?

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | audits coverage vs this Test Specification (esp. renewal/cap boundary + reassignment race) | clean; iteration count in Discovery |
| After tests pass, before CHORE(close) | `/review` | adversarial review vs fencing invariants, the liveness≠progress distinction, REST guide | clean OR every finding dispositioned |
| After `gh pr create` | `/review-pr` | review-comments the open PR | comments addressed before merge |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Integration (renewal/fleet) | `make test-integration` | {paste at VERIFY} | |
| pg-drain | `make check-pg-drain` | {paste at VERIFY} | |
| Lint | `make lint` | {paste at VERIFY} | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` | {paste at VERIFY} | |

---

## Out of Scope

- Placement on labels/capacity + autoscale by queue depth — M80_007 (this workstream does inventory + cordon/revoke + recovery, not scheduling).
- **Incremental per-renewal metering** — M80_006 ships only the minimal credit *guard* (`/renew` refuses when balance is exhausted, reusing `balanceCoversEstimate`). Charging long runs incrementally per renewal window (vs the current single pre-execution debit) is a billing-system change and gets its own spec (per the split-backend-features rule). Flagged for Indy's confirmation.
- **`last_renewed_at` observability column** — a persisted "runner alive, lease going stale" timestamp would aid the inventory view but needs a schema change; deferred (the cap math works off `created_at`).
- Raising the base `LEASE_TTL_MS` to minutes — rejected; detection speed comes from `HEARTBEAT_LAPSE_MS` (§2), so the lease stays short and renewal handles duration.
- Zero-trust scoped/proxied secrets — beyond the trusted-fleet model.
- Multi-region fleet topology / HA control plane — future.
