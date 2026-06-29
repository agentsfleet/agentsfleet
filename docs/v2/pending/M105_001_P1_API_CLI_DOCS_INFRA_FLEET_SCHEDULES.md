<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh.
-->

# M105_001: Manage Fleet schedules (create/update/delete) backed by Upstash QStash

**Prototype:** v2.0.0
**Milestone:** M105
**Workstream:** 001
**Date:** Jun 29, 2026
**Status:** PENDING
**Priority:** P1 — customer-facing: a `type: cron` Fleet has been parseable but never fires; this lights up time-based wakes.
**Categories:** API, CLI, DOCS, INFRA
**Batch:** B1 — standalone capability.
**Depends on:** M104_001 (the `schedule:read` / `schedule:write` scopes ride the scope catalog it introduces; until then, gate on `platform_admin`).
**Provenance:** agent-generated (Indy design chat, Jun 29, 2026) — architecture fork (agentsfleet-owns-CRUD + QStash-fires) and provider (QStash) chosen by Indy via AskUserQuestion.

> **Provenance is load-bearing.** LLM-drafted against a live codebase sweep of the event-ingress, webhook-verify, and Fleet-CRUD paths. Re-verify the QStash Schedules API surface and the Upstash signature scheme against current Upstash docs before EXECUTE — external API shapes drift.

**Canonical architecture:** `docs/architecture/data_flow.md` (single-ingress event model — cron is already a named producer) and `docs/architecture/capabilities.md §1.1` (`trigger.type` vs `event_type`, orthogonal). The scheduler is **out-of-process by design**: agentsfleet owns the schedule records and the fire ingress; Upstash QStash owns the clock.

This spec uses Cron Expression, Hash-based Message Authentication Code (HMAC), JSON Web Token (JWT), Finite State Machine (FSM), Pull Request (PR), Command-Line Interface (CLI), Coordinated Universal Time (UTC), and Dead-Letter Queue (DLQ) below.

---

## Implementing agent — read these first

1. `src/agentsfleetd/http/handlers/fleets/{create,patch,delete,list}.zig` — the workspace-scoped CRUD + status-FSM pattern the schedule handlers mirror (atomic INSERT, `authorizeWorkspace`, `verifyFleetInWorkspace`).
2. `src/agentsfleetd/http/handlers/webhooks/fleet.zig` + `src/agentsfleetd/cmd/serve_webhook_lookup.zig` — the **signature-verify → dedup (SET NX) → XADD** ingress the cron-fire endpoint reuses verbatim; the per-fleet secret-from-vault lookup is the model for the QStash signing key.
3. `src/lib/contract/event_envelope.zig` + `src/agentsfleetd/http/handlers/fleets/messages.zig` — `EventType.cron` already exists; the fire endpoint builds that envelope (`actor=cron:<schedule_id>`) instead of `messages.zig`'s `chat`.
4. `src/agentsfleetd/fleet_runtime/config_helpers.zig` (`parseFleetTriggers`) — already parses + caps the `type: cron` trigger and its `schedule`; §5 reconciles that parsed value into a schedule row. Cron-expression *validation* lives here too (currently absent).
5. `schema/007_core_fleets.sql`, `schema/embed.zig`, and `docs/SCHEMA_CONVENTIONS.md` — the nearest migration to mirror for `core.fleet_schedules` (uuidv7 PK, BIGINT millis timestamps, app-enforced enums, no static-string `CHECK`).

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Manage Fleet schedules backed by Upstash QStash
- **Intent:** Let an operator create, update, and delete a recurring schedule on a Fleet so it wakes on a cron without any human present — delivered by registering the cron with QStash, which calls a signed agentsfleet ingress that drops a `cron` event on the Fleet's stream.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate the intent and list `ASSUMPTIONS I'M MAKING: …`; reconcile any mismatch before edits.

---

## Product Clarity (answer in order, at authoring)

1. **Successful user moment** — Indy installs the `zoho-sprint-daily-summarizer` Fleet (`TRIGGER.md`: `type: cron, schedule: "0 18 * * 1-5"`). At 18:00 UTC next weekday, with nobody watching, it wakes, reads the day's Zoho activity, posts the summary — history shows `actor=cron:<schedule_id>`, `event_type=cron`.
2. **Preserved user behaviour** — Steer, webhook ingest, lease/execute/report are untouched; a no-cron Fleet behaves as today; one-lease-per-Fleet holds (a fire mid-execution just queues like a steer).
3. **Optimal-way check** — Direct shape: a schedule record agentsfleet owns, a thin adapter mirroring it into QStash, a signed ingress enqueuing `cron`. The unconstrained-optimal is an in-process scheduler thread; the gap (external dependency, per-schedule wiring) is accepted now — it ships the moment without building/proving an exactly-once distributed scheduler, the function Indy chose not to build.
4. **Rebuild-vs-iterate** — Iterate. Event-ingress, dedup, and signature-verify already exist and are reused; this adds one table, one adapter, four CRUD routes, one ingress route, a CLI group. No runtime refactor.
5. **What we build** — `core.fleet_schedules`; a provider-agnostic backend + QStash adapter; CRUD REST routes; a signed cron-fire ingress enqueuing `event_type=cron`; schedule auto-provision from a `type: cron` `TRIGGER.md` at install; an `agentsfleet schedule` CLI group; a cron validator.
6. **What we do NOT build** — In-process scheduler thread (explicit non-goal); timezone UI; multi-provider abstraction beyond the swap seam; backfill of fires missed while stopped (QStash at-least-once + dedup is the floor); a dashboard editor.
7. **Fit with existing features** — Compounds with Fleet install (the moment) and the event/history surface (`cron` renders today). Must not destabilize the webhook ingress it borrows — the fire route is additive and separately keyed.
8. **Surface order** — API + schema first; CLI second; auto-provision rides the existing install path; dashboard is later read-only (out of scope).
9. **Dashboard restraint** — No schedule-editing UI; managed via API/CLI; history's `cron` events are the only evidence surface that ships.
10. **Confused-user next step** — A failed create names the cause (`UZ-SCHED-001`, offending field); `schedule list` prints QStash-side state; `docs.agentsfleet.net` documents the grammar + customer flow.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — `NDC` (no dead code), `NLR` (touch-it-fix-it on the config-parser cron path), `UFS` (cron field names, scope strings, QStash route fragments, dedup-key prefix as named constants shared verbatim cross-runtime), `ERR` (registry entries for every schedule failure), `ECL` (distinct error classes per failure), `LOG` (no signing key / QStash token in logs), `FLL` (file/function length — split adapter / handlers / validator), `PRI`, `TST-NAM`, `ORP` (sweep if any config-parser symbol moves).
- **`dispatch/write_auth.md`** + **`docs/AUTH.md`** — the cron-fire ingress is a new credential-verifying entrypoint (QStash signature); it must fail closed exactly like the webhook verifier. CRUD routes gate on `schedule:*` scopes.
- **`dispatch/write_zig.md`** — handlers, adapter, ingress, validator, schema embed (pg-drain lifecycle, tagged-union results, multi-step `errdefer`, cross-compile both Linux targets).
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — route design + registration for the four CRUD routes and the fire route; `403`/`404`/`409`/`422` error shapes.
- **`docs/SCHEMA_CONVENTIONS.md`** — `core.fleet_schedules` migration + `schema/embed.zig` + migration array; no static-string `CHECK`.
- **`dispatch/write_ts_adhere_bun.md`** — the `agentsfleet schedule` CLI group.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | Read `dispatch/write_zig.md`; cross-compile both Linux targets; tagged-union adapter result; `errdefer` rollback on partial QStash failure. |
| PUB / Struct-Shape | yes | Shape verdict on the `ScheduleRecord`, the `ScheduleBackend` seam, and `requireScope`-gated handler exports; minimise pub surface. |
| File & Function Length (≤350/≤50/≤70) | yes | Adapter, validator, each CRUD verb, and the fire ingress are separate files; the QStash HTTP calls live in their own module. |
| UFS (repeated/semantic literals) | yes | `cron:` actor prefix, dedup-key prefix, scope strings, QStash path fragments → named constants; cron-related identifiers shared verbatim Zig↔CLI. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | yes | `UZ-SCHED-NNN` registry entries; pg-drain on every `conn.query()`; secret-free logs; new migration follows `SCHEMA_CONVENTIONS.md`. |
| SCHEMA GUARD | yes | New table only (no DROP/ALTER); update `schema/embed.zig` + the migration array in the same diff. |

---

## Overview

**Goal (testable):** Creating a schedule on a Fleet persists a `core.fleet_schedules` row and registers a matching QStash schedule whose destination is the Fleet's signed cron-fire ingress; at the cron instant QStash POSTs that ingress, the signature verifies, a `cron` event lands on `fleet:{id}:events`, and the Fleet wakes — exactly once per fire (dedup-guarded). Updating mutates both row and QStash schedule; deleting removes both.

**Problem:** A `TRIGGER.md` may declare `type: cron, schedule: "0 18 * * 1-5"` — the value is parsed, capped, and stored in `config_json`, then **never read**. Customers who want a daily-summary agent have no way to make it fire; the schedule string is dead config.

**Solution summary:** Add a schedule record owned by agentsfleet and a thin provider-backed adapter that mirrors each create/update/delete into QStash. QStash becomes the clock; a new signature-verified ingress turns each QStash callback into a `cron` event on the existing single-ingress stream. The runtime downstream of the enqueue is unchanged. A `type: cron` `TRIGGER.md` auto-provisions a schedule at install so the documented customer flow works end-to-end.

---

## Prior-Art / Reference Implementations

- **API** → `src/agentsfleetd/http/handlers/fleets/{create,patch,delete}.zig` (workspace-scoped CRUD, status FSM, atomic INSERT) + `docs/REST_API_DESIGN_GUIDELINES.md`.
- **Ingress / signature verify / dedup** → `src/agentsfleetd/http/handlers/webhooks/fleet.zig` + `serve_webhook_lookup.zig` — the cron-fire route mirrors its verify→`SET NX` dedup→`XADD` shape; the QStash signing key resolves from vault the same way the per-fleet webhook secret does.
- **Schema** → `schema/007_core_fleets.sql` + `docs/SCHEMA_CONVENTIONS.md`.
- **CLI** → `cli/src/program/cli-tree-fleet.ts` (`fleet.steer`, the `credential` group) + the **7 Pillars** of CLI developer experience (handler purity, output-as-a-service, structured JSON errors, auto-JSON when piped).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/0NN_core_fleet_schedules.sql` | CREATE | The schedule record table (next free migration number). |
| `schema/embed.zig` | EDIT | Register the new migration in the embed + migration array. |
| `src/agentsfleetd/fleet_runtime/schedule/record.zig` | CREATE | `ScheduleRecord` type, status FSM constants, per-fleet cap. |
| `src/agentsfleetd/fleet_runtime/schedule/cron_validate.zig` | CREATE | 5-field cron-expression validator (shared by CRUD + config parser). |
| `src/agentsfleetd/fleet_runtime/schedule/backend.zig` | CREATE | Provider-agnostic `ScheduleBackend` seam (register/update/remove). |
| `src/agentsfleetd/fleet_runtime/schedule/qstash.zig` | CREATE | QStash adapter: Schedules API calls + signature verify; creds from vault. |
| `src/agentsfleetd/http/handlers/fleets/schedules/{create,list,patch,delete}.zig` | CREATE | The four workspace-scoped CRUD handlers. |
| `src/agentsfleetd/http/handlers/fleets/schedules/fire.zig` | CREATE | Signature-verified cron-fire ingress → enqueue `event_type=cron`. |
| `src/agentsfleetd/http/route_table.zig` (or its registrar) | EDIT | Register the five new routes. |
| `src/agentsfleetd/fleet_runtime/config_helpers.zig` | EDIT | Wire cron validation into `parseFleetTriggers`; expose the parsed cron for §5. |
| `src/agentsfleetd/http/handlers/fleets/create.zig` | EDIT | Auto-provision a schedule when the installed `TRIGGER.md` carries a `type: cron`. |
| `src/agentsfleetd/errors/error_registry.zig` | EDIT | `UZ-SCHED-NNN` codes. |
| `cli/src/program/cli-tree-fleet.ts` + `cli/src/commands/fleet_schedule*.ts` | CREATE/EDIT | The `agentsfleet schedule add\|list\|update\|rm` group. |

> The next free migration number and exact route-registrar file are the agent's to resolve from the repo at EXECUTE.

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Six Sections split along the data → backend → API → ingress → integration → CLI seam, so each is independently testable and the runtime stays untouched.
- **Alternatives considered:** (a) An in-process scheduler thread modelled on `approval_gate_sweeper.zig` — rejected now: it owns exactly-once across replicas, missed-fire policy, and a cron evaluator, the very function Indy chose not to build. (b) A pure CLI wrapper with no stored record — rejected: schedules would be invisible to agentsfleet's data model, history, and authz, and the customer's `TRIGGER.md` cron could not auto-provision.
- **Patch-vs-refactor verdict:** **patch** (additive feature) — it reuses the event-ingress and verify/dedup machinery wholesale and adds one table plus thin handlers. The in-process scheduler remains a named future spec if the external dependency ever becomes unacceptable.

---

## Sections (implementation slices)

### §1 — Schedule record + cron validation

The persisted unit and the guard that a stored cron expression is well-formed. **Implementation default:** schedules ride their own table (not `config_json`) because they carry a QStash schedule id, a status FSM, and a per-fleet count that must be queryable. Per-fleet cap mirrors `MAX_TRIGGERS_PER_AGENT`. Timezone stored per row, UTC the app-supplied default, passed through to QStash.

- **Dimension 1.1** — `core.fleet_schedules` persists (id uuidv7, fleet_id FK, workspace_id, cron, timezone, status, message, provider, provider_schedule_id, timestamps) → Test `test_schedule_row_roundtrips`
- **Dimension 1.2** — a malformed cron expression is rejected at write with `UZ-SCHED-001` naming the field → Test `test_cron_validate_rejects_malformed`
- **Dimension 1.3** — the (N+1)th schedule on a Fleet is rejected with `UZ-SCHED-003` → Test `test_schedule_per_fleet_cap`

### §2 — Provider-agnostic backend + QStash adapter

A `ScheduleBackend` seam (register/update/remove) with one QStash implementation, so the firing provider is swappable without touching handlers. QStash REST token + signing key resolve from vault (platform-scoped account; the steer/webhook secret lookup is the model). **Implementation default:** the QStash destination URL is the Fleet's `…/schedules/{schedule_id}/fire` ingress.

- **Dimension 2.1** — `register` creates a QStash schedule and returns its id, stored on the row → Test `test_qstash_register_returns_schedule_id`
- **Dimension 2.2** — `update`/`remove` patch/delete the QStash schedule by stored id → Test `test_qstash_update_remove`
- **Dimension 2.3** — a QStash failure during create leaves **no** orphan row (atomic: both persist or neither) → Test `test_create_rolls_back_on_provider_failure`

### §3 — Schedule CRUD REST surface

`POST`/`GET`/`PATCH`/`DELETE` under `…/fleets/{id}/schedules`, workspace-authorized, gated on `schedule:write` (mutations) / `schedule:read` (list). Mirrors the Fleet-CRUD handlers.

- **Dimension 3.1** — `POST` creates row + QStash schedule, returns `201` with the schedule id → Test `test_create_schedule_201`
- **Dimension 3.2** — `PATCH` updates cron/message/status on row + QStash → Test `test_patch_schedule`
- **Dimension 3.3** — `DELETE` removes row + QStash schedule, returns `204` → Test `test_delete_schedule_204`
- **Dimension 3.4** — a principal lacking `schedule:write` gets `403` naming the scope; cross-workspace access gets `403` → Test `test_schedule_authz`

### §4 — Cron-fire ingress + idempotency

A signed ingress QStash calls at each cron instant. Verifies the QStash signature against the vault signing key (fail closed), dedups on `(schedule_id, qstash_message_id)`, and enqueues `EventEnvelope{event_type=cron, actor=cron:<schedule_id>}`. Reuses the webhook verify→`SET NX`→`XADD` path.

- **Dimension 4.1** — a valid signed fire enqueues exactly one `cron` event on `fleet:{id}:events` → Test `test_fire_enqueues_cron_event`
- **Dimension 4.2** — an invalid/absent signature is rejected `401` and enqueues nothing → Test `test_fire_rejects_bad_signature`
- **Dimension 4.3** — a replayed fire (same message id) is an idempotent no-op (`200`, no second event) → Test `test_fire_dedupes_replay`
- **Dimension 4.4** — a fire for a stopped/killed Fleet (or paused schedule) enqueues nothing and returns `200` → Test `test_fire_skips_inactive`

### §5 — Auto-provision from `TRIGGER.md` cron (the customer flow)

On Fleet install, when the `TRIGGER.md` carries a `type: cron` trigger, create the matching schedule + QStash registration in the same atomic create path — so the documented Zoho-summarizer flow fires without a separate CRUD call.

- **Dimension 5.1** — installing a Fleet with `type: cron` creates exactly one active schedule whose cron equals the trigger's → Test `test_install_cron_autoprovisions_schedule`
- **Dimension 5.2** — installing a Fleet with no cron trigger creates no schedule → Test `test_install_no_cron_no_schedule`

### §6 — `agentsfleet schedule` CLI group

`add | list | update | rm` over the §3 routes, following the 7 Pillars: handler purity, structured JSON errors, auto-JSON when piped.

- **Dimension 6.1** — `agentsfleet schedule add <fleet_id> --cron "0 18 * * 1-5"` creates a schedule and prints its id → Test `test_cli_schedule_add` (e2e, subprocess)
- **Dimension 6.2** — `agentsfleet schedule list <fleet_id>` renders human + (piped) JSON → Test `test_cli_schedule_list_json`
- **Dimension 6.3** — `update`/`rm` round-trip to the API and surface structured errors → Test `test_cli_schedule_update_rm`

---

## Interfaces

```
POST   /v1/workspaces/{ws}/fleets/{id}/schedules        → 201 {schedule_id, cron, status, next_fire_hint?}
GET    /v1/workspaces/{ws}/fleets/{id}/schedules        → 200 {schedules:[{schedule_id, cron, timezone, status, message}]}
PATCH  /v1/workspaces/{ws}/fleets/{id}/schedules/{sid}  → 200 {schedule_id, ...}   (cron|message|status optional)
DELETE /v1/workspaces/{ws}/fleets/{id}/schedules/{sid}  → 204
POST   /v1/workspaces/{ws}/fleets/{id}/schedules/{sid}/fire  (QStash-signed; not user-callable) → 200 {event_id?}

Create body: { "cron": "0 18 * * 1-5", "message": "summarize today's Zoho Sprints", "timezone": "UTC" }
  - cron      : required, 5-field expression, validated (UZ-SCHED-001)
  - message   : required, the steer text delivered as the cron event payload (≤8192 bytes)
  - timezone  : optional, app-default UTC

ScheduleBackend (seam):  register(record) -> provider_schedule_id
                         update(record)   -> void
                         remove(provider_schedule_id) -> void

Event enqueued on fire: EventEnvelope{ event_type=.cron, actor="cron:<schedule_id>",
                        request_json={ "message": <message>, "schedule_id": <sid>, "fired_at": <ms> } }
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Invalid cron | Malformed expression on create/update | Reject `422` `UZ-SCHED-001` naming the field; nothing persisted. |
| Per-fleet cap | (N+1)th schedule | Reject `409` `UZ-SCHED-003`; nothing persisted. |
| Schedule not found | Bad `{sid}` on patch/delete/fire | `404` `UZ-SCHED-002`. |
| Provider unavailable | QStash API down during create/update/delete | `502` `UZ-SCHED-004`; on create, no orphan row (atomic rollback); on delete, row kept + retriable. |
| Fire signature invalid | Forged/absent QStash signature | `401` `UZ-SCHED-005`; nothing enqueued (fail closed). |
| Replayed fire | QStash at-least-once retry | Idempotent `200`; dedup key suppresses the second `XADD`. |
| Fire for inactive Fleet | Fleet stopped/killed or schedule paused | `200`; nothing enqueued (no backlog on resume). |
| Cross-workspace / missing scope | Wrong workspace or no `schedule:write` | `403` naming the scope; no state change. |

---

## Invariants

1. An `active` schedule row always carries a non-null `provider_schedule_id` — enforced by code: create persists the row and the QStash registration in one path with `errdefer` rollback; neither half survives alone.
2. A stored cron expression is always well-formed — enforced by the validator at every write (CRUD and the `TRIGGER.md` auto-provision share one `cron_validate`); rejection is `UZ-SCHED-001`.
3. The fire ingress enqueues at most one `cron` event per `(schedule_id, qstash_message_id)` — enforced by the `SET NX` dedup key, the same primitive the webhook ingress uses.
4. The fire ingress enqueues only for an `active` Fleet + `active` schedule — enforced by a status read before `XADD`.
5. No QStash token or signing key is ever logged — enforced by the logging discipline (vault refs only; `LOG` rule).
6. Cron field names, the `cron:` actor prefix, scope strings, and the dedup-key prefix are named constants, shared verbatim across Zig and the CLI — enforced by `UFS`.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_schedule_row_roundtrips` | INSERT then SELECT returns identical field set incl. uuidv7 PK. |
| 1.2 | unit | `test_cron_validate_rejects_malformed` | `"99 * * * *"`, `"* * *"`, `""` → rejected; `"0 18 * * 1-5"` → accepted. |
| 1.3 | integration | `test_schedule_per_fleet_cap` | (cap+1)th create → `409 UZ-SCHED-003`. |
| 2.1 | integration | `test_qstash_register_returns_schedule_id` | register (injected QStash double) → row's `provider_schedule_id` set. |
| 2.2 | integration | `test_qstash_update_remove` | update patches schedule; remove deletes it; double records the calls. |
| 2.3 | integration | `test_create_rolls_back_on_provider_failure` | QStash double errors → 0 rows in `core.fleet_schedules`. |
| 3.1 | e2e | `test_create_schedule_201` | real HTTP `POST` → `201` + schedule id; row present. |
| 3.2 | integration | `test_patch_schedule` | `PATCH` cron → row + QStash double both updated. |
| 3.3 | integration | `test_delete_schedule_204` | `DELETE` → `204`; row gone; remove called. |
| 3.4 | integration | `test_schedule_authz` | no `schedule:write` → `403` naming scope; other workspace → `403`. |
| 4.1 | integration | `test_fire_enqueues_cron_event` | signed fire → exactly one stream entry, `event_type=cron`, `actor=cron:<sid>`. |
| 4.2 | integration | `test_fire_rejects_bad_signature` | bad signature → `401`; stream length unchanged. |
| 4.3 | integration | `test_fire_dedupes_replay` | same message id twice → one stream entry; second is `200` no-op. |
| 4.4 | integration | `test_fire_skips_inactive` | killed Fleet / paused schedule → `200`, stream length unchanged. |
| 5.1 | integration | `test_install_cron_autoprovisions_schedule` | install `type: cron` Fleet → one active schedule, cron matches trigger. |
| 5.2 | integration | `test_install_no_cron_no_schedule` | install non-cron Fleet → zero schedules. |
| 6.1 | e2e | `test_cli_schedule_add` | subprocess `schedule add … --cron` → exit 0, prints id. |
| 6.2 | e2e | `test_cli_schedule_list_json` | piped stdout → JSON array; tty → human table. |
| 6.3 | e2e | `test_cli_schedule_update_rm` | update + rm round-trip; structured error on bad cron. |

**Regression:** steer (`event_type=chat`), webhook ingest, and lease/execute/report paths unchanged — `test_steer_unaffected`, existing webhook + lease suites stay green.
**Idempotency/replay:** Dimension 4.3 is the replay test; QStash retries are at-least-once and the dedup key is the floor.
Cron and QStash fixtures: `samples/fixtures/m105-fixtures/{valid_crons.json, qstash_fire.json}`.

## Acceptance Criteria

- [ ] Create→fire→wake end-to-end (QStash double) lands one `cron` event — verify: `make test-integration 2>&1 | grep test_fire_enqueues_cron_event`
- [ ] Installing the Zoho-summarizer fixture auto-provisions one schedule — verify: `make test-integration 2>&1 | grep test_install_cron_autoprovisions`
- [ ] Bad signature / replay / inactive Fleet all enqueue nothing — verify: `make test-integration 2>&1 | grep -E "bad_signature|dedupes_replay|skips_inactive"`
- [ ] CLI `schedule add|list|update|rm` round-trips — verify: `make test-e2e 2>&1 | grep schedule`
- [ ] `make lint` clean · `make test` passes · `make test-integration` passes
- [ ] `make check-pg-drain` clean (new `conn.query()` sites)
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1 fire path:   make test-integration 2>&1 | grep test_fire_enqueues_cron_event
# E2 build+tests: zig build && make test && make test-integration && make test-e2e
# E3 lint+drain:  make lint && make check-pg-drain
# E4 cross:       zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux
# E5 hygiene:     gitleaks detect; git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l | awk '$1>350{print "OVER:"$2}'
```

## Dead Code Sweep

**1. Orphaned files** — N/A — no files deleted (additive feature).

**2. Orphaned references** — only if `parseFleetTriggers` symbols move when cron validation is wired in.

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| {any renamed config-parser symbol} | `grep -rn "{symbol}" src/ \| head` | 0 matches |

## Discovery (consult log)

> **Empty at creation.** Append as the work surfaces consults and decisions.

- **Architecture fork** — Indy (Jun 29, 2026, AskUserQuestion): chose "agentsfleet owns CRUD, QStash fires" over an in-process scheduler or a storage-less wrapper; provider = Upstash QStash.
- **Consults** — (pending) confirm QStash Schedules API + signature scheme against current Upstash docs at EXECUTE.
- **Skill chain outcomes** — (pending) `/write-unit-test`, `/review`, `/review-pr`, `kishore-babysit-prs`.
- **Deferrals** — none yet (every deferral needs an Indy-acked verbatim quote here).

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs this Test Specification. | Clean; iteration count + coverage in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec, `docs/architecture/`, REST guide, `dispatch/write_zig.md`, Failure Modes, Invariants. | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open PR against the immutable diff. | Comments addressed before human review/merge. |

---

## Verification Evidence

> Filled during VERIFY.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | {paste snippet} | |
| Integration tests | `make test-integration` | {paste snippet} | |
| e2e (CLI) | `make test-e2e` | {paste snippet} | |
| Lint | `make lint` | {paste snippet} | |
| pg-drain | `make check-pg-drain` | {paste snippet} | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` | {paste snippet} | |
| Gitleaks | `gitleaks detect` | {paste snippet} | |

---

## Out of Scope

- In-process scheduler thread (the `approval_gate_sweeper.zig`-style waker) — named future spec if the external dependency becomes unacceptable.
- Catch-up/backfill of fires missed while a Fleet was stopped — QStash at-least-once + dedup is the floor.
- Dashboard schedule editor — history already renders `cron` events; a read-only schedule view is later work.
- Multi-provider scheduling beyond the single `ScheduleBackend` seam needed to keep QStash swappable.
- Per-resource scope strings and a scope-management UI (carried by M104_001's out-of-scope list).
