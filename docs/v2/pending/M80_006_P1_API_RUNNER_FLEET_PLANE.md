# M80_006: Fleet operator plane + heartbeat-driven reassignment + per-lease renewal

**Prototype:** v2.0.0
**Milestone:** M80
**Workstream:** 006
**Date:** May 27, 2026
**Status:** PENDING
**Priority:** P1 — operators can't see/revoke runners, dead runners are only reclaimed lazily after the TTL, and any agent running longer than 30s is killed + redone (the renewal gap M80_002 ships with).
**Categories:** API
**Batch:** B1
**Branch:** {feat/mNN-name — added when work begins}
**Depends on:** M80_002 (lease/fencing/reclaim + the activity stream this renews from), M80_005 (trust fields the inventory surfaces)
**Provenance:** agent-generated (Opus 4.7, May 27, 2026 — from `runner_fleet.md` S5 + the M80_002 §6 renewal-gap gating decision)

> **Provenance is load-bearing.** LLM-drafted. The renewal half is the documented fix for the gap recorded in `M80_002` Failure Modes + §6 — the implementing agent reads that gating note and `src/lib/common/constants.zig` first.

**Canonical architecture:** `docs/architecture/runner_fleet.md` (S5 FLEET row + "System Guarantees / Failure Recovery Model" — SLA target · mechanism · tradeoff · the M80_006 path).

---

## Implementing agent — read these first

1. `docs/v2/active/M80_002_P1_API_RUNNER_CUTOVER.md` — Failure Modes ("Agent outruns the lease TTL") + the §6 gating decision; this workstream is the named fix. Do not flip §6 runner-as-default for >30s agents until this lands.
2. `src/lib/common/constants.zig` `LEASE_TTL_MS` — the 30s liveness TTL; renewal decouples liveness from execution duration.
3. `src/zombied/fleet/reclaim.zig` — the lazy expiry-reclaim sweep; proactive reassignment is heartbeat-driven on top of it (not a replacement — the sweep stays the backstop).
4. `src/zombied/fleet/service_activity.zig` — the per-lease activity stream; `tool_call_progress` frames are the long-tool heartbeat the renewal keys off.
5. `docs/AUTH.md` — the operator fleet plane is admin-authenticated (distinct from the runner self-plane); addressing a runner by `{id}` in the path is legitimate here because it is NOT a runner authenticating about itself.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** add the fleet operator plane and decouple lease liveness from execution duration
- **Intent (one sentence):** operators can list and revoke runners, a dead runner's leases are reassigned the moment its heartbeat lapses (not after a lazy TTL sweep), and a healthy long-running agent renews its lease so it is never killed + redone mid-run.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent + `ASSUMPTIONS I'M MAKING: …`. Mismatch → STOP. Key assumption to confirm: renewal extends `lease_expires_at` from a **progress-bearing** signal (activity frames), with a **separate hard max-runtime cap** so a wedged-but-emitting agent still terminates.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (TTL/renewal/max-runtime constants single-sourced in `constants.zig`, shared verbatim across runner/zombied), NLG (replace the lazy-only reclaim in place; no `_v2` twin).
- **`docs/ZIG_RULES.md`** — handlers + reclaim + renewal are `*.zig` (pg-drain on every query, tagged-union results, multi-step `errdefer`, cross-compile).
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — the new `/v1/fleet/runners` operator routes: §URL design (admin plane, `{id}` allowed), route registration, `Hx` handler signature, error envelopes.
- **`docs/SCHEMA_CONVENTIONS.md`** — if heartbeat-liveness or renewal needs a column (e.g. `last_renewed_at`), additive + single-concern.
- **`docs/AUTH.md`** — operator routes are admin-authenticated; the runner self-plane is unchanged.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — handlers/reclaim/renewal `*.zig` | cross-compile; pg-drain audit on every query |
| PUB / Struct-Shape | yes — fleet handler + renewal helper surfaces | shape verdict per new pub; mirror existing fleet handler shape |
| File & Function Length | yes | split the reclaim/renewal logic if a file nears 350; methods ≤50 |
| UFS | yes — TTL / renewal-window / max-runtime constants | named constants in `constants.zig`, shared verbatim |
| SCHEMA GUARD | maybe — if `last_renewed_at`/liveness column added | append-only, single-concern, update embed + migration array |
| ERROR REGISTRY | yes — revoke/lease-not-found/over-cap `UZ-RUN-*` | declare before use; mirror in `client_errors.zig` where the runner observes it |
| LOGGING | yes — reassignment + renewal + revoke emits | logfmt with `error_code`/`runner_id`/`lease_id`; no secrets |

---

## Overview

**Goal (testable):** (a) `GET /v1/fleet/runners` returns the fleet with liveness + current-lease state and `PATCH` revokes a runner; (b) when a runner's heartbeat lapses, its active leases are reassigned within one detection cycle (well under the lazy TTL); (c) a lease whose agent keeps emitting activity has its `lease_expires_at` renewed so a >30s healthy run is never reclaimed, while a separate hard max-runtime cap still terminates a runaway — asserted by `test_fleet_inventory_and_revoke`, `test_heartbeat_lapse_reassigns_leases`, `test_active_lease_renews_past_ttl`, `test_hard_max_runtime_caps_renewal`.

**Problem:** three operator-plane gaps the cutover shipped with. (1) No fleet visibility/control — an operator can't list runners or revoke a compromised one. (2) Recovery is lazy — a dead runner's event waits out the full `LEASE_TTL_MS` before reclaim. (3) The renewal gap — `LEASE_TTL_MS` doubles as both liveness and max execution time, so any agent running >30s is killed at its deadline and its event reclaimed + re-run (state stays correct via fencing, but the work is wasted and capped).

**Solution summary:** add an admin-authenticated fleet plane (`GET`/`PATCH /v1/fleet/runners`); make the heartbeat path proactively reassign a lapsed runner's leases (the expiry sweep stays as the backstop); and decouple liveness from execution duration — renew `lease_expires_at` from progress-bearing activity frames, have the runner's child kill-deadline track the renewed deadline, add a synthetic keepalive for quiet LLM calls, and enforce a separate hard max-runtime cap so renewal can't run forever.

---

## Prior-Art / Reference Implementations

- **API** → the existing `/v1/runners/me/*` fleet handlers (M80_001/002) under `src/zombied/fleet/` + the nearest operator-plane handler under `src/zombied/http/handlers/`; the new routes mirror their `Hx` signature + registration.
- **Reclaim** → `src/zombied/fleet/reclaim.zig` (the atomic CTE reclaim from M80_002) — proactive reassignment reuses it, triggered by heartbeat-lapse instead of TTL.
- **Renewal signal** → `src/zombied/fleet/service_activity.zig` — the per-lease activity verb already carries `tool_call_progress`; renewal is a side effect of that existing flow.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/zombied/http/handlers/fleet/*.zig` | CREATE | `GET /v1/fleet/runners` inventory + `PATCH /v1/fleet/runners/{id}` revoke (admin plane) |
| route table + `router` wiring | EDIT | register the two operator routes (5-place registration per REST guide) |
| `src/zombied/fleet/service_activity.zig` | EDIT | renew `lease_expires_at` on a progress-bearing frame |
| `src/zombied/fleet/reclaim.zig` | EDIT | heartbeat-lapse-triggered proactive reassignment path (sweep stays backstop) |
| `src/zombied/fleet/` heartbeat handler | EDIT | detect lapse; mark runner dead; trigger reassignment |
| `src/runner/child_supervisor.zig` | EDIT | child kill-deadline tracks the renewed deadline; honor the hard max-runtime cap |
| `src/lib/common/constants.zig` | EDIT | renewal-window + hard max-runtime constants alongside `LEASE_TTL_MS` |
| `schema/*.sql` + `embed.zig` | EDIT (maybe) | a liveness/`last_renewed_at` column if needed (additive) |
| `src/zombied/errors/*` | EDIT | revoke / over-cap `UZ-RUN-*` |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three slices — operator plane, proactive reassignment, lease renewal — landable independently; renewal is the one §6 blocks on.
- **Alternatives considered:** (a) just raise `LEASE_TTL_MS` to a large value — rejected, it makes dead-runner recovery slow (liveness and execution stay coupled, the wrong tradeoff); (b) renew on the per-runner heartbeat rather than per-lease activity — rejected, a runner can heartbeat while its agent is wedged, so renewal must be **progress-bearing** (per-lease activity), not mere liveness.
- **Patch-vs-refactor verdict:** **refactor of the lease lifecycle** — it decouples two concerns (liveness vs execution duration) that `LEASE_TTL_MS` currently conflates. That is the right-sized change; a TTL bump would be the mud-patch.

---

## Sections (implementation slices)

### §1 — Fleet operator plane

Delivers admin visibility + control: list runners with liveness + current lease, and revoke one. Why: a compromised or misbehaving runner must be removable, and an operator must see fleet state. **Implementation default:** revoke is a `PATCH` status transition (the runner is cordoned — leases drain/reassign, no new leases), not a hard delete — because the row carries audit + trust history.

- **Dimension 1.1** — `GET /v1/fleet/runners` (admin) returns each runner's id, trust_class, liveness, and current lease → Test `test_fleet_inventory_lists_runners`
- **Dimension 1.2** — `PATCH /v1/fleet/runners/{id}` cordons/revokes: no new leases issued; in-flight lease drains or is reassigned → Test `test_fleet_revoke_cordons_runner`

### §2 — Heartbeat-driven proactive reassignment

Delivers fast recovery: when a runner's heartbeat lapses, mark it dead and reassign its active leases now, rather than waiting out the TTL. The expiry sweep stays as the backstop for a runner that vanishes without a final heartbeat.

- **Dimension 2.1** — a lapsed-heartbeat runner's active leases are reassigned within one detection cycle (a fresh higher fencing_token), well under the TTL → Test `test_heartbeat_lapse_reassigns_leases`
- **Dimension 2.2** — the lazy expiry sweep remains correct as the backstop (silent disappearance still recovers) → Test `test_expiry_sweep_still_backstops` (regression of M80_002 reclaim)

### §3 — Per-lease renewal (the §6 gating fix)

Delivers liveness/execution decoupling: a lease emitting progress frames renews `lease_expires_at`; the child kill-deadline tracks the renewed deadline; a quiet-but-alive LLM call gets a synthetic keepalive; a separate hard max-runtime cap terminates a runaway regardless of renewal. **Implementation default:** renew only on a **progress-bearing** frame (`tool_call_progress`/chunk), not on mere heartbeat — because liveness ≠ progress.

- **Dimension 3.1** — a lease emitting activity past `LEASE_TTL_MS` is NOT reclaimed; the same agent run completes once → Test `test_active_lease_renews_past_ttl`
- **Dimension 3.2** — the child's kill-deadline tracks the renewed `lease_expires_at` (a renewed lease's child is not killed at the original 30s) → Test `test_child_deadline_tracks_renewal`
- **Dimension 3.3** — a hard max-runtime cap terminates a still-emitting runaway and reports it (renewal cannot extend past the cap) → Test `test_hard_max_runtime_caps_renewal`
- **Dimension 3.4** — a stale (non-renewing) lease still expires + reclaims exactly as today → Test `test_non_renewing_lease_still_reclaims` (regression)

---

## Interfaces

```
GET   /v1/fleet/runners            (admin)  → [{ runner_id, trust_class, alive, last_seen_at, current_lease }]
PATCH /v1/fleet/runners/{id}       (admin)  → { status: active|cordoned|revoked }  (cordon = drain, no new leases)

Renewal (internal, on a progress-bearing activity frame for lease L):
  lease_expires_at(L) := now + LEASE_TTL_MS         -- bounded by:
  hard cap:  started_at(L) + MAX_RUNTIME_MS         -- renewal never extends past this
  child kill-deadline tracks lease_expires_at(L); synthetic keepalive frames cover quiet LLM calls.

constants (src/lib/common/constants.zig, shared verbatim runner↔zombied):
  LEASE_TTL_MS (liveness), RENEWAL_* (window), MAX_RUNTIME_MS (hard cap)

errors (new): UZ-RUN-009 runner_revoked · UZ-RUN-010 lease_exceeded_max_runtime
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Runner heartbeat lapses | host crash / partition | marked dead; active leases reassigned within one cycle (higher fencing_token); old runner's late report fenced (`UZ-RUN-005`) → `test_heartbeat_lapse_reassigns_leases` |
| Runner vanishes with no final heartbeat | hard kill | the lazy expiry sweep backstops (TTL reclaim) → `test_expiry_sweep_still_backstops` |
| Healthy long agent (>30s) | slow legitimate run, emitting progress | lease renews from activity frames; not reclaimed; runs once → `test_active_lease_renews_past_ttl` |
| Wedged agent still emitting | tight loop emitting frames | hard max-runtime cap terminates + reports (`UZ-RUN-010`); renewal can't extend past the cap → `test_hard_max_runtime_caps_renewal` |
| Quiet LLM call | long model latency, no tool frames | synthetic keepalive renews so it isn't falsely reclaimed |
| Revoke a runner mid-lease | operator cordons/revokes | no new leases; in-flight drains or is reassigned; `UZ-RUN-009` on its next lease attempt → `test_fleet_revoke_cordons_runner` |
| Operator route hit by a runner token | wrong principal | admin-only authz rejects (not the runner self-plane); 403 |

---

## Invariants

1. Renewal extends a lease only on a **progress-bearing** signal, never mere liveness — enforced by the renewal call site living in the activity (progress) path, not the heartbeat path + `test_active_lease_renews_past_ttl`.
2. Renewal can never extend a lease past `started_at + MAX_RUNTIME_MS` — enforced by the renewal computing `min(now+TTL, hard_cap)` + `test_hard_max_runtime_caps_renewal`.
3. A non-renewing lease expires + reclaims exactly as in M80_002 — enforced by the unchanged expiry sweep + `test_non_renewing_lease_still_reclaims`.
4. Fleet operator routes are admin-only — enforced by the route's auth middleware (admin role) + a negative authz test; a runner token gets 403.
5. TTL / renewal-window / max-runtime live as single-sourced constants shared verbatim runner↔zombied — enforced by UFS (one `constants.zig` identifier each).

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_fleet_inventory_lists_runners` | seeded fleet → `GET` returns each runner's id/trust/liveness/lease |
| 1.2 | integration | `test_fleet_revoke_cordons_runner` | `PATCH` revoke → no new lease issued to it; in-flight reassigned |
| 2.1 | integration | `test_heartbeat_lapse_reassigns_leases` | runner stops heartbeating → its lease reassigned with a higher token within one cycle |
| 2.2 | integration | `test_expiry_sweep_still_backstops` | runner vanishes silently → TTL sweep reclaims (M80_002 regression) |
| 3.1 | integration | `test_active_lease_renews_past_ttl` | lease emits a frame at t=20s,40s → not reclaimed at 30s; one completion |
| 3.2 | integration | `test_child_deadline_tracks_renewal` | renewed lease → child not killed at original 30s deadline |
| 3.3 | integration | `test_hard_max_runtime_caps_renewal` | agent emits frames past `MAX_RUNTIME_MS` → terminated + `UZ-RUN-010` |
| 3.4 | integration | `test_non_renewing_lease_still_reclaims` | lease emits nothing → expires + reclaims as today |

Regression: all of M80_002's fencing/reclaim tests stay green (renewal + proactive reassignment must not break stale-report fencing or row-equivalence). Replay: a reassigned lease's old holder's late report stays fenced (`UZ-RUN-005`).

---

## Acceptance Criteria

- [ ] Long healthy agent not reclaimed; runaway still capped — verify: `test_active_lease_renews_past_ttl` + `test_hard_max_runtime_caps_renewal`
- [ ] Dead runner reassigned fast; silent death still backstopped — verify: `test_heartbeat_lapse_reassigns_leases` + `test_expiry_sweep_still_backstops`
- [ ] Fleet inventory + revoke work and are admin-only — verify: `test_fleet_inventory_lists_runners` + the negative authz test
- [ ] `make lint` clean · `make test` passes · `make test-integration` passes · `make check-pg-drain` clean
- [ ] Cross-compile both linux targets · `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: renewal + reassignment — make test-integration 2>&1 | grep -E "renews_past_ttl|reassigns_leases|max_runtime|PASS|FAIL"
# E2: Build  — zig build
# E3: Tests  — make test && make test-integration
# E4: Lint   — make lint 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile — zig build -Dtarget=aarch64-linux 2>&1 | tail -3
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: pg-drain — make check-pg-drain 2>&1 | tail -3
```

---

## Dead Code Sweep

N/A — no files deleted. The lazy-only reclaim is extended in place with a proactive path (RULE NLR); the TTL-as-max-runtime coupling is removed, not twinned.

---

## Discovery (consult log)

> **Empty at creation.** Append consults, skill-chain outcomes, and Indy-acked deferral quotes as work proceeds.

- **Provenance (May 27, 2026):** authored with M80_003/004/005 to formalize the remaining roadmap before M80_002's CHORE(close). The renewal half is the named fix for the gap recorded in M80_002 Failure Modes + §6; §6 (runner-as-default for >30s agents) blocks on this workstream.
- **Indy (May 27, 2026):** "fix cleanly in M80_006" — the renewal gap is to be solved here, not patched in M80_002 (TTL bump is the rejected mud-patch). — context: M80_002 ships with the documented gap; this spec owns the fix.

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

- Placement on labels/capacity + autoscale by queue depth — M80_007 (this workstream does inventory + revoke + recovery, not scheduling).
- Zero-trust scoped/proxied secrets — beyond the trusted-fleet model.
- Multi-region fleet topology / HA control plane — future.
