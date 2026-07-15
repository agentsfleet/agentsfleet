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
**Status:** IN_PROGRESS
**Priority:** P1 — customer-facing: a `type: cron` Fleet has been parseable but never fires; this lights up time-based wakes.
**Categories:** API, CLI, DOCS, INFRA
**Batch:** B1 — standalone capability.
**Branch:** feat/m105-fleet-schedules
**Test Baseline:** unit=2642 integration=334
**Depends on:** M104_001 (DONE — `schedule:read` / `schedule:write` extend its scope catalog).
**Provenance:** agent-generated (Indy design chat, Jun 29, 2026) — architecture fork (agentsfleet-owns-CRUD + QStash-fires) and provider (QStash) chosen by Indy via AskUserQuestion.

> **Provenance is load-bearing.** Large Language Model (LLM)-drafted against a live codebase sweep of the event-ingress, webhook-verify, and Fleet-CRUD paths. Re-verify the QStash Schedules API surface and the Upstash signature scheme against current Upstash docs before EXECUTE — external API shapes drift.

**Canonical architecture:** `docs/architecture/data_flow.md` (single-ingress event model — cron is already a named producer) and `docs/architecture/capabilities.md §1.1` (`trigger.type` vs `event_type`, orthogonal). The scheduler is **out-of-process by design**: agentsfleet owns the schedule records and the fire ingress; Upstash QStash owns the clock.

This spec uses Create, Read, Update, Delete (CRUD), Hash-based Message Authentication Code (HMAC), JSON Web Token (JWT), Finite State Machine (FSM), Pull Request (PR), Command-Line Interface (CLI), Coordinated Universal Time (UTC), Dead-Letter Queue (DLQ), Representational State Transfer (REST), input/output (I/O), Teletypewriter (TTY), and Universally Unique Identifier version 7 (UUIDv7) below.

## Implementing agent — read these first

1. `src/agentsfleetd/http/handlers/fleets/{create,patch,delete,list}.zig` — the workspace-scoped CRUD + status-FSM pattern the schedule handlers mirror (atomic INSERT, `authorizeWorkspace`, `verifyFleetInWorkspace`).
2. `src/agentsfleetd/http/handlers/webhooks/fleet.zig` + `src/agentsfleetd/cmd/serve_webhook_lookup.zig` — the **signature-verify → dedup (SET NX) → XADD** ingress the cron-fire endpoint reuses verbatim; the per-fleet secret-from-vault lookup is the model for the QStash signing key.
3. `src/lib/contract/event_envelope.zig` + `src/agentsfleetd/http/handlers/fleets/messages.zig` — `EventType.cron` already exists; the fire endpoint builds that envelope (`actor=cron:<schedule_id>`) instead of `messages.zig`'s `chat`.
4. `src/agentsfleetd/fleet_runtime/config_helpers.zig` (`parseFleetTriggers`) — already parses + caps the `type: cron` trigger and its `schedule`; §5 turns that parsed value into a schedule row. Cron-expression, timezone, and message validation live here too (currently absent).
5. `schema/005_core_fleets.sql`, `schema/embed.zig`, and `docs/SCHEMA_CONVENTIONS.md` — the nearest migration to mirror for `core.fleet_schedules` (uuidv7 PK, BIGINT millis timestamps, app-enforced enums, no static-string `CHECK`).

## PR Intent & comprehension handshake

- **PR title (eventual):** Manage Fleet schedules backed by Upstash QStash
- **Intent:** Let an operator create, update, and delete a recurring schedule on a Fleet so it wakes on a cron without any human present — delivered by registering the cron with QStash, which calls a signed agentsfleet ingress that drops a `cron` event on the Fleet's stream.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate the intent and list `ASSUMPTIONS I'M MAKING: …`; reconcile any mismatch before edits.

## Product Clarity (answer in order, at authoring)

1. **Successful user moment** — Indy installs the `zoho-sprint-daily-summarizer` Fleet (`TRIGGER.md`: `type: cron`, `schedule: "0 9 * * *"`, `timezone: "Asia/Kolkata"`). At 09:00 local time every day, with nobody watching, it wakes, reads the day's Zoho activity, posts the summary — history shows `actor=cron:<schedule_id>`, `event_type=cron`.
2. **Preserved user behaviour** — Steer, webhook ingest, lease/execute/report are untouched; a no-cron Fleet behaves as today; one-lease-per-Fleet holds (a fire mid-execution just queues like a steer).
3. **Optimal-way check** — Direct shape: visible schedule state, a synchronous idempotent QStash call, one signed ingress, and the existing Fleet event stream. `agentsfleet` does not calculate fire times or run a repair loop.
4. **Rebuild-vs-iterate** — Refactor the scheduling slice: reuse event ingress and Fleet lifecycle patterns, add one native provider client, and remove NullClaw's hosted local-fallback cron path.
5. **What we build** — `core.fleet_schedules`; one synchronous schedule service; QStash create/overwrite/delete calls with explicit recovery; CRUD plus `sync` REST routes; a signed cron-fire ingress enqueuing `event_type=cron`; declarative Fleet lifecycle synchronization; an optimized `agentsfleet schedule` CLI group; a QStash operator playbook; concise architecture updates.
6. **What we do NOT build** — An `agentsfleet` cron clock; an automatic schedule reconciler, poller, or retry thread; timezone UI; speculative provider abstraction; missed-fire backfill; dashboard editor; conversational NullClaw scheduling or local cron state.
7. **Fit with existing features** — Compounds with Fleet install (the moment) and the event/history surface (`cron` renders today). Must not destabilize the webhook ingress it borrows — the fire route is additive and separately keyed.
8. **Surface order** — State and synchronous provider facade first; ingress/API/Fleet lifecycle second; hosted cron-tool removal and CLI third; operator and architecture docs last.
9. **Dashboard restraint** — No schedule-editing UI; managed via API/CLI; history's `cron` events are the only evidence surface that ships.
10. **Confused-user next step** — Validation names the offending field; `schedule status` shows desired state, sync state, generation, provider error, and recovery; `docs.agentsfleet.net` documents the grammar and customer flow.

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — `NDC` (no dead code), `NLR` (touch-it-fix-it on the config-parser cron path), `UFS` (cron field names, scope strings, QStash route fragments, dedup-key prefix as named constants shared verbatim cross-runtime), `ERR` (registry entries for every schedule failure), `ECL` (distinct error classes per failure), `LOG` (no signing key / QStash token in logs), `FLL` (file/function length — split adapter / handlers / validator), `PRI`, `TST-NAM`, `ORP` (sweep if any config-parser symbol moves).
- **`dispatch/write_auth.md`** + **`docs/AUTH.md`** — the cron-fire ingress is a new credential-verifying entrypoint (QStash signature); it must fail closed exactly like the webhook verifier. CRUD routes gate on `schedule:*` scopes.
- **`dispatch/write_zig.md`** — handlers, adapter, ingress, validator, schema embed (pg-drain lifecycle, tagged-union results, multi-step `errdefer`, cross-compile both Linux targets).
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — route design + registration for the four CRUD routes and the fire route; `403`/`404`/`409`/`422` error shapes.
- **`docs/SCHEMA_CONVENTIONS.md`** — `core.fleet_schedules` migration + `schema/embed.zig` + migration array; no static-string `CHECK`.
- **`dispatch/write_ts_adhere_bun.md`** — the `agentsfleet schedule` CLI group.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | Read `dispatch/write_zig.md`; cross-compile both Linux targets; tagged outcomes; exhaustive allocation-failure and cleanup proofs. |
| PUB / Struct-Shape | yes | Shape verdict on the schedule model/service, provider result, and scope-gated handler exports; minimise pub surface. |
| File & Function Length (≤350/≤50/≤70) | yes | Split domain, provider client, ingress, handlers, and CLI rendering by responsibility. |
| UFS (repeated/semantic literals) | yes | `cron:` actor prefix, dedup-key prefix, scope strings, QStash path fragments → named constants; cron-related identifiers shared verbatim Zig↔CLI. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | yes | `UZ-SCHED-NNN` registry entries; pg-drain on every `conn.query()`; secret-free logs; new migration follows `SCHEMA_CONVENTIONS.md`. |
| SCHEMA GUARD | yes | New table only (no DROP/ALTER); update `schema/embed.zig` + the migration array in the same diff. |

## Overview

**Goal (testable):** Creating a Fleet schedule durably records visible state, synchronously configures QStash without an automatic worker, and turns each signed QStash delivery for the active generation into at most one `cron` event without runner affinity.

**Problem:** A `TRIGGER.md` may declare `type: cron, schedule: "0 18 * * 1-5"` — the value is parsed, capped, and stored in `config_json`, then **never read**. Customers who want a daily-summary agent have no way to make it fire; the schedule string is dead config.

**Solution summary:** Add a first-class schedule service and visible-state table. API, Fleet-install, Fleet-lifecycle, and CLI mutations call QStash immediately through one native client using the `agentsfleet` schedule identifier as the provider identifier. A short database synchronization lease serializes provider mutations for one schedule; it expires for explicit recovery and never drives background work. Provider failure or an uncertain response is stored and returned with `agentsfleet schedule sync` recovery. QStash owns timing and calls one signed ingress URL with the schedule identifier plus generation. The ingress loads current state, verifies the active generation, atomically deduplicates and appends the existing `cron` event, then returns. Runners stay disposable; hosted NullClaw cron tools are unavailable.

## Prior-Art / Reference Implementations

- **API** → `src/agentsfleetd/http/handlers/fleets/{create,patch,delete}.zig` (workspace-scoped CRUD and lifecycle) + `docs/REST_API_DESIGN_GUIDELINES.md`.
- **Ingress / signature verify / dedup** → `src/agentsfleetd/http/handlers/webhooks/fleet.zig` + `serve_webhook_lookup.zig` — the cron-fire route reuses its signature and stream patterns, but combines deduplication plus append in one atomic Redis operation; the QStash signing key resolves from vault the same way the per-fleet webhook secret does.
- **Bounded provider I/O** → existing native HTTP clients under `src/agentsfleetd/` — caller-owned buffers, hard timeout, typed outcomes, and no subprocess.
- **Hosted tool admission** → `src/runner/engine/{tool_bridge,tool_builders}.zig` — remove local cron builders from hosted resolution rather than forwarding or falling back.
- **Module layout** → `~/Projects/oss/ghostty/src/{crash,terminal}/main.zig` — cohesive topic directory with `main.zig` as the public façade and private concern files beside it.
- **Schema** → `schema/005_core_fleets.sql` + `docs/SCHEMA_CONVENTIONS.md`.
- **CLI** → `cli/src/program/cli-tree-fleet.ts` (`fleet.steer`, the `credential` group) + the **7 Pillars** of CLI developer experience (handler purity, output-as-a-service, structured JSON errors, auto-JSON when piped).

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/{028_core_fleet_schedules.sql,embed.zig}` | CREATE/EDIT | Visible state, generation/sync-lease fields, and migration registration. |
| `src/agentsfleetd/cron/*.zig` | CREATE/EDIT | Cron façade, credentials, value modules, stateful file-as-struct types, synchronous provider boundary, signed verification, explicit recovery, and tests. |
| `src/agentsfleetd/cron/*_test.zig` | CREATE | Failure, leak, concurrency, and performance proofs. |
| `src/agentsfleetd/tests.zig` | EDIT | Register the cron facade and its private test graph in the canonical test binary. |
| `src/agentsfleetd/http/handlers/{schedules/api,ingress/qstash}.zig` | CREATE | Thin workspace, explicit-sync, and signed-ingress adapters. |
| `src/agentsfleetd/http/handlers/common.zig` + `src/agentsfleetd/cmd/{serve,serve_qstash}.zig` | CREATE/EDIT | Load QStash credentials once and borrow them from immutable request context. |
| `src/agentsfleetd/http/{routes,router,route_matchers,route_matchers_schedules,route_table,route_scopes,route_table_invoke,route_table_invoke_schedules}.zig` | EDIT | Route and authorize management plus ingress surfaces. |
| `public/openapi/{root.yaml,paths/schedules.yaml,components/schemas.yaml}` + `public/{openapi.json,llms.txt,skill.md}` | CREATE/EDIT | Document and bundle the schedule management and signed-ingress API surfaces. |
| `scripts/check_openapi_url_shape.py` | EDIT | Recognize schedule collections as resource-shaped URLs. |
| `src/agentsfleetd/{auth/scopes.zig,auth/scopes_test.zig,http/test_scope_tokens.zig}` + `docs/AUTH.md` + `scripts/mint-scope-personas.mjs` | EDIT | Schedule scopes. |
| `src/agentsfleetd/{state/model_library/sql,fleet/budget,fleet/budget_test,fleet/budget_integration_test,fleet_library/github_source_test,http/handlers/pagination,http/handlers/library/catalog,http/handlers/library/gallery,http/handlers/fleet_bundles/api_integration_test}.zig` + `src/runner/engine/stream_redactor.zig` | EDIT | Formatter-only unblock approved by Indy after the full-tree Zig gate exposed pre-existing drift. |
| `src/agentsfleetd/fleet_runtime/{config_helpers,config_types}.zig` + `src/agentsfleetd/http/handlers/fleets/{create,patch,delete,cron_sync,cron_lifecycle_integration_test}.zig` | CREATE/EDIT | Shared cron/timezone/message validation and Fleet lifecycle synchronization. |
| `src/agentsfleetd/errors/{error_registry,error_entries}.zig` | EDIT | Schedule errors and recovery text. |
| `src/runner/engine/{tool_bridge,tool_builders}.zig` + runner tool tests | EDIT | Reject hosted `cron_*` tools so local fallback is unreachable. |
| `cli/src/{commands/fleet_schedule,lib/api-paths,program/cli-tree,program/cli-tree-schedule,program/cli-tree-types,program/handlers-bind,program/handlers-bind-schedule}.ts` + `cli/test/{fleet-schedule,fleet-schedule.integration,cli-tree-schedule,json-contract,helpers-cli-tree}.ts` | CREATE/EDIT | Add/list/update/rm/status/sync commands and rendering. |
| `tests/fixtures/fleetbundle/zoho-sprint-daily-summarizer/TRIGGER.md` | EDIT | Daily 09:00 Asia/Kolkata declarative example. |
| `playbooks/operations/qstash_registration/001_playbook.md` | CREATE | Account, vault, ingress, and rotation setup. |
| `docs/REST_API_DESIGN_GUIDELINES.md` | EDIT | Register QStash as an inline-signature raw-handler exception. |
| `docs/architecture/{data_flow,user_flow,capabilities,high_level}.md` | EDIT | One concise hosted scheduling model. |

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Eight Sections split state, synchronous provider I/O, API, ingress, Fleet integration, CLI, hosted tool removal, and operator/architecture documentation. Handlers delegate provider I/O to the service.
- **Alternatives considered:** A background reconciler was rejected by Indy because invisible polling, retry, replica contention, and shutdown behavior are unnecessary for a QStash facade. A storage-less wrapper was rejected because auth, lifecycle, status, and recovery would be invisible to `agentsfleet`. NullClaw's local scheduler was rejected because its daemon and `cron.json` die with the disposable child.
- **Refactor verdict:** **structural refactor** — one synchronous service becomes the only mutation path; stable identifiers make retries idempotent; short per-schedule synchronization leases reject concurrent provider mutations; explicit `sync` repairs uncertain outcomes. QStash still owns the clock, so `agentsfleet` implements no scheduler thread.

## Sections (implementation slices)

### §1 — Schedule record + cron validation

The persisted visible state and shared cron guard. **Implementation default:** the `agentsfleet` identifier is also the QStash schedule identifier; no premature provider identifier or provider interface is stored. Desired lifecycle and provider sync state are separate so users can see uncertain outcomes honestly.

- **Dimension 1.1** — `core.fleet_schedules` persists identity, ownership, source, cron, timezone, message, desired state, sync state, generation, synchronization lease, error, and audit timestamps → Test `test_schedule_row_roundtrips` → **DONE** (database integration test green)
- **Dimension 1.2** — a malformed cron expression is rejected at write with `UZ-SCHED-001` naming the field → Test `test_cron_validate_rejects_malformed` → **DONE** (unit test green)
- **Dimension 1.3** — the 33rd schedule on a Fleet is rejected with `UZ-SCHED-003` → Test `test_schedule_per_fleet_cap` → **DONE** (100-way integration test admits exactly one create from a starting count of 31)

### §2 — Synchronous QStash facade + explicit recovery

A native QStash client performs create/overwrite/delete inside the caller request with one hard-deadline attempt. The service commits visible `provisioning`/`deleting` state first, acquires a short per-schedule synchronization lease, performs provider I/O outside a database transaction, and finalizes only the matching generation and lease token. A concurrent mutation gets `409`; a crash or uncertain response leaves visible `error` state for explicit `sync` after the lease expires. No automatic thread reads the table. QStash credentials load from the platform administrative vault key `qstash`. One configured destination `/v1/ingress/qstash/schedules` receives every fire and QStash carries `schedule_id` plus `generation` in the body. Registration requests set three QStash delivery retries; these are fire-delivery retries handled by ingress idempotency, not hidden schedule-mutation retries.

- **Dimension 2.1** — create/update/delete call QStash immediately and idempotently by stable schedule identifier → Test `test_qstash_facade_roundtrip` → **DONE** (live PostgreSQL lifecycle test covers create, pause, resume, and delete while a one-connection pool probe proves no connection crosses provider I/O)
- **Dimension 2.2** — one schedule admits one provider mutation; 100 concurrent mutations yield one provider call and deterministic `409` responses without a process-global lock → Test `test_schedule_sync_100_way_serialization` → **DONE** (live 100-way service test: one synced, 99 busy, one provider call)
- **Dimension 2.3** — timeout, rate limit, server error, malformed reply, response loss, and allocation failure leave durable actionable state without leaks → Test `test_qstash_facade_failure_matrix` → **DONE** (typed client matrix, response-loss one-attempt assertion, allocation-failure sweep, and live durable out-of-memory test green)
- **Dimension 2.4** — explicit sync reads the newest generation, overwrites provider state, and clears the prior error; a stale generation can never execute → Test `test_explicit_sync_recovers_latest_generation` → **DONE** (live failure-to-sync test recovers generation 2 and clears the error)

### §3 — Schedule CRUD REST surface

`POST`/`GET`/`PATCH`/`DELETE` plus `POST …/{sid}:sync` under `…/fleets/{id}/schedules`, workspace-authorized, gated on `schedule:write` (mutations) / `schedule:read` (list). Mirrors the Fleet-CRUD handlers.

- **Dimension 3.1** — confirmed `POST` returns `201 active`; provider failure returns `502 UZ-SCHED-004` with schedule identifier and sync recovery → Test `test_create_schedule_provider_outcomes` → **DONE**
- **Dimension 3.2** — confirmed `PATCH` returns `200 active`; a busy synchronization lease returns deterministic `409` → Test `test_patch_schedule_serialization` → **DONE**
- **Dimension 3.3** — confirmed `DELETE` returns `204`; failure leaves an inert visible `deleting` row and actionable `502` → Test `test_delete_schedule_provider_outcomes` → **DONE**
- **Dimension 3.4** — a principal lacking `schedule:write` gets `403` naming the scope; cross-workspace access gets `403` → Test `test_schedule_authz` → **DONE**
- **Dimension 3.5** — `POST …/{sid}:sync` repairs only the current generation and returns `200 active` or an actionable `502` → Test `test_schedule_sync_route` → **DONE**

### §4 — Cron-fire ingress + idempotency

A signed ingress verifies current and next QStash signing keys against the raw body, exact configured URL, issuer, expiry, not-before, and body hash. It cross-checks the body/header schedule identifiers, requires the body generation to equal the current active generation, then uses one atomic Redis script to deduplicate and append the event without a crash gap.

- **Dimension 4.1** — a valid signed fire enqueues exactly one `cron` event on `fleet:{id}:events` → Test `test_fire_enqueues_cron_event` → **DONE**
- **Dimension 4.2** — an invalid/absent signature is rejected `401` and enqueues nothing → Test `test_fire_rejects_bad_signature` → **DONE**
- **Dimension 4.3** — a replayed fire (same message id) is an idempotent no-op (`200`, no second event) → Test `test_fire_dedupes_replay` → **DONE**
- **Dimension 4.4** — a fire for missing, stopped, killed, paused, deleting, or unsynced state enqueues nothing and returns `200` → Test `test_fire_skips_inactive` → **DONE**
- **Dimension 4.5** — 100 concurrent copies of one signed delivery create one stream entry with no serialized global lock → Test `test_fire_100_way_exactly_once` → **DONE**

### §5 — Declarative cron and Fleet lifecycle

Fleet install and patch synchronously configure the single declarative cron schedule by source identity; stop/kill suppress fires, resume explicitly restores the schedule, and hard delete removes provider schedules before deleting the Fleet. All paths call the same service and surface provider errors rather than scheduling hidden retries.

- **Dimension 5.1** — installing or patching a Fleet with `type: cron` creates or updates exactly one declarative desired schedule → Test `test_fleet_cron_syncs_schedule_and_lifecycle` → **DONE**
- **Dimension 5.2** — installing a Fleet with no cron trigger creates no schedule → Test `test_install_no_cron_no_schedule` → **DONE**
- **Dimension 5.3** — stop/resume/kill/delete never leave an effective or orphaned provider schedule → Test `test_fleet_cron_syncs_schedule_and_lifecycle` → **DONE**

### §6 — Optimized `agentsfleet schedule` CLI

`add | list | update | rm | status | sync` over the §3 routes, following the 7 Pillars. Provider operations complete inside the request; human mode prints the confirmed next run or an exact `sync` recovery command; piped mode emits stable JSON without a spinner.

- **Dimension 6.1** — `agentsfleet schedule add <fleet_id> --cron "0 18 * * 1-5"` creates a schedule and prints its id → Test `` `schedule add` posts the schedule and prints the schedule id `` → **DONE**
- **Dimension 6.2** — `agentsfleet schedule list <fleet_id>` renders human + (piped) JSON → Test `` `schedule list` emits JSON when stdout is redirected `` → **DONE**
- **Dimension 6.3** — `update`/`rm` round-trip to the API and surface structured errors → Test `` `schedule update` and `schedule rm` use item routes `` → **DONE**
- **Dimension 6.4** — confirmed success, provider error guidance, explicit sync, and piped output are deterministic → Test `` `schedule status` and `schedule sync` read and reapply the item route `` + `cli-tree-schedule.unit.test.ts` → **DONE**

### §7 — Hosted NullClaw cron tools fail closed

Remove the pinned NullClaw cron builders from hosted tool resolution. Declarative `TRIGGER.md` and explicit CLI/API calls are the only scheduling entrypoints. A hosted Fleet that lists any `cron_*` tool fails admission with a clear unsupported-tool error; no child receives a local cron tool, touches `cron.json`, launches `curl`, or creates durable work without operator intent.

- **Dimension 7.1** — every hosted `cron_*` tool name is rejected before child execution with no local file → Test `test_hosted_cron_tools_rejected`
- **Dimension 7.2** — the Zoho declarative schedule installs and fires without exposing any NullClaw cron tool → Test `test_declarative_schedule_has_no_cron_tool`

### §8 — Operator setup and architecture accuracy

The platform operator gets one repeatable QStash registration playbook, while the architecture documents name one scheduling model consistently for humans and agents. Hosted schedules are stored and synchronously configured by `agentsfleet`, timed by QStash, and delivered through `agentsfleetd`; no NullClaw child or background worker owns them.

- **Dimension 8.1** — the playbook covers QStash account setup, API token plus current/next signing keys, vault storage, public ingress verification, and rotation without printing credentials → Test `make check-playbooks`
- **Dimension 8.2** — architecture documents no longer claim a disposable NullClaw child owns or fires hosted cron schedules → Test `architecture_schedule_ownership`

## Interfaces

```
POST   /v1/workspaces/{ws}/fleets/{id}/schedules        → 201 Schedule
GET    /v1/workspaces/{ws}/fleets/{id}/schedules        → 200 {items:[Schedule],total,next_cursor}
GET    /v1/workspaces/{ws}/fleets/{id}/schedules/{sid}  → 200 Schedule
PATCH  /v1/workspaces/{ws}/fleets/{id}/schedules/{sid}  → 200 Schedule
DELETE /v1/workspaces/{ws}/fleets/{id}/schedules/{sid}  → 204
POST   /v1/workspaces/{ws}/fleets/{id}/schedules/{sid}:sync → 200 Schedule
POST   /v1/ingress/qstash/schedules                     → 200 {accepted:true}

Create body: { "cron": "0 9 * * *", "message": "summarize today's Zoho Sprints", "timezone": "Asia/Kolkata" }
  - cron      : required, 5-field expression, validated (UZ-SCHED-001)
  - message   : required, the steer text delivered as the cron event payload (≤8192 bytes)
  - timezone  : optional, app-default UTC; local syntax validation, QStash validates the Internet Assigned Numbers Authority name

Declarative `TRIGGER.md` uses the same `schedule`, `timezone`, and `message` fields under its single `type: cron` entry.

Schedule: {schedule_id, fleet_id, source, cron, timezone, message,
           desired_status, sync_status, generation, last_error?, created_at, updated_at}

QStash body: { "schedule_id": <sid>, "generation": <generation> }

Event enqueued on fire: EventEnvelope{ event_type=.cron, actor="cron:<schedule_id>",
                        request_json={ "message": <message>, "schedule_id": <sid>, "generation": <generation>, "fired_at": <ms> } }
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Invalid cron | Malformed expression on create/update | Reject `422` `UZ-SCHED-001` naming the field; nothing persisted. |
| Per-fleet cap | 33rd schedule | Reject `409` `UZ-SCHED-003`; nothing persisted. |
| Schedule not found | Bad `{sid}` on workspace management route | `404` `UZ-SCHED-002`. Signed ingress treats an absent row as an idempotent `200`. |
| Provider unavailable | QStash timeout, rate limit, or server error | State remains durable; `sync_status=error`; API returns `502 UZ-SCHED-004` and CLI prints the exact `schedule sync` recovery command. No hidden retry. |
| Concurrent mutation | Another request owns the unexpired synchronization lease | Reject `409 UZ-SCHED-006`; the caller retries after the bounded lease. |
| QStash not configured | Administrative vault entry is absent or invalid at daemon boot | Schedule management and fire ingress reject `503 UZ-SCHED-007`; the operator follows the QStash registration playbook and rolls the daemon. |
| Process crash / response loss | Request dies after the state write or QStash call | Row remains `provisioning`, `deleting`, or `error`; ingress rejects non-active or stale generations; explicit `schedule sync` safely overwrites by stable identifier. |
| Fire signature invalid | Forged/absent QStash signature | `401` `UZ-SCHED-005`; nothing enqueued (fail closed). |
| Replayed fire | QStash at-least-once retry | Idempotent `200`; one atomic Redis operation suppresses the second event. |
| Fire for inactive Fleet | Fleet stopped/killed or schedule paused | `200`; nothing enqueued (no backlog on resume). |
| Cross-workspace / missing scope | Wrong workspace or no `schedule:write` | `403` naming the scope; no state change. |
| Hosted cron tool requested | Fleet declares `cron_add`, `cron_update`, or another local scheduler tool | Reject admission as unsupported; no child, local `cron.json`, subprocess, or provider mutation. |

## Invariants

1. One schedule has at most one in-flight provider mutation inside the synchronization lease; finalization must match its generation and lease token.
2. A stored cron expression is always well-formed — enforced by the validator at every write (CRUD and the `TRIGGER.md` auto-provision share one `cron_validate`); rejection is `UZ-SCHED-001`.
3. The fire ingress enqueues at most one `cron` event per `(schedule_id, qstash_message_id)` — enforced by one atomic deduplicate-plus-append Redis operation.
4. The fire ingress enqueues only for an `active` Fleet plus `active` schedule plus exact current generation — stale or uncertain provider state cannot execute.
5. No QStash token or signing key is ever logged — enforced by the logging discipline (vault refs only; `LOG` rule).
6. Cron field names, the `cron:` actor prefix, scope strings, and the dedup-key prefix are named constants, shared verbatim across Zig and the CLI — enforced by `UFS`.
7. Hosted NullClaw cron tools cannot write local scheduler state — enforced by removing every hosted cron builder and rejecting those tool names before execution.
8. No schedule reconciler, polling loop, retry thread, or automatic database reader exists; recovery is an explicit API/CLI mutation.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_schedule_row_roundtrips` | INSERT then SELECT returns identical field set including the UUIDv7 primary key. |
| 1.2 | unit | `test_cron_validate_rejects_malformed` | `"99 * * * *"`, `"* * *"`, `""` → rejected; `"0 18 * * 1-5"` → accepted. |
| 1.3 | integration | `test_schedule_per_fleet_cap` | 33rd create → `409 UZ-SCHED-003`. |
| 2.1 | integration | `test_qstash_facade_roundtrip` | create/overwrite/delete happen in the caller request by stable identifier. |
| 2.2 | concurrency | `test_schedule_sync_100_way_serialization` | 100 barrier-started mutations → one provider call, 99 deterministic busy outcomes, no global lock. |
| 2.3 | integration | `test_qstash_facade_failure_matrix` | injected timeout/429/5xx/malformed/response-loss/allocation failures preserve actionable state and leak nothing. |
| 2.4 | integration | `test_explicit_sync_recovers_latest_generation` | sync after uncertainty writes latest generation; old-generation fire is ignored. |
| 3.1 | e2e | `test_create_schedule_provider_outcomes` | real HTTP `POST` → `201 active`; provider failure → `502` plus persisted schedule identifier. |
| 3.2 | integration | `test_patch_schedule_serialization` | confirmed `PATCH` returns `200`; concurrent mutation returns `409`. |
| 3.3 | integration | `test_delete_schedule_provider_outcomes` | confirmed delete returns `204`; failure leaves inert `deleting` row. |
| 3.4 | integration | `test_schedule_authz` | no `schedule:write` → `403` naming scope; other workspace → `403`. |
| 3.5 | integration | `test_schedule_sync_route` | explicit sync repairs the current generation or returns actionable `502`. |
| 4.1 | integration | `test_fire_enqueues_cron_event` | signed fire → exactly one stream entry, `event_type=cron`, `actor=cron:<sid>`. |
| 4.2 | integration | `test_fire_rejects_bad_signature` | bad or absent signature → `401`; stream length unchanged. |
| 4.3 | integration | `test_fire_dedupes_replay` | same message id twice → one stream entry; second is `200` no-op. |
| 4.4 | integration | `test_fire_skips_inactive` | missing schedule, killed Fleet, and inactive schedule states → `200`, stream length unchanged. |
| 4.5 | concurrency | `test_fire_100_way_exactly_once` | 100 identical signed requests → one stream event and parallel verification. |
| 5.1 | integration | `test_fleet_cron_syncs_schedule_and_lifecycle` | install/patch declarative cron → one sourced desired schedule at latest generation. |
| 5.2 | integration | `test_install_no_cron_no_schedule` | install non-cron Fleet → zero schedules. |
| 5.3 | integration | `test_fleet_cron_syncs_schedule_and_lifecycle` | stop/resume/kill/delete suppress or remove provider state without orphan. |
| 6.1 | e2e | `` `schedule add` posts the schedule and prints the schedule id `` | `runCli` over loopback API sends `POST /schedules` with cron/timezone/message and prints the schedule id. |
| 6.2 | e2e | `` `schedule list` emits JSON when stdout is redirected `` | redirected stdout emits the API envelope. |
| 6.3 | e2e | `` `schedule update` and `schedule rm` use item routes `` | update + rm round-trip through PATCH and DELETE item routes. |
| 6.4 | e2e | `` `schedule status` and `schedule sync` read and reapply the item route `` | status GETs the item; sync POSTs the `:sync` route; parser tests expose all verbs. |
| 7.1 | failure | `test_hosted_cron_tools_rejected` | each `cron_*` declaration is rejected before child start and creates no local state. |
| 7.2 | e2e | `test_declarative_schedule_has_no_cron_tool` | declarative create/fire path succeeds with only the Fleet's declared non-cron tools. |
| 8.1 | documentation | `make check-playbooks` | QStash registration playbook passes repository playbook checks. |
| 8.2 | documentation | `architecture_schedule_ownership` | Four architecture documents consistently name QStash, `agentsfleetd`, and disposable runner ownership. |

**Regression:** steer (`event_type=chat`), webhook ingest, and lease/execute/report paths unchanged — `test_steer_unaffected`, existing webhook + lease suites stay green.
**Idempotency/replay:** Dimensions 2.1–2.4 and 4.3–4.5 prove explicit provider retries and delivery replay are safe under concurrency.
Cron and QStash fixtures: `samples/fixtures/m105-fixtures/{valid_crons.json, qstash_fire.json}`.

## Acceptance Criteria

- [ ] Create→fire→wake end-to-end (QStash double) lands one `cron` event — verify: `make test-integration 2>&1 | grep test_fire_enqueues_cron_event`
- [ ] Synchronous create/update/delete plus explicit sync handle crash, stale generation, provider failure, and 100-way contention without a background worker
- [ ] Installing the Zoho-summarizer fixture creates one declarative schedule and Fleet lifecycle leaves no provider orphan
- [ ] Bad signature / replay / inactive Fleet all enqueue nothing — verify: `make test-integration 2>&1 | grep -E "bad_signature|dedupes_replay|skips_inactive"`
- [ ] CLI `schedule add|list|update|rm|status|sync` gives deterministic human and JSON output with exact recovery guidance
- [ ] Hosted NullClaw cron tools are rejected before execution and never touch local scheduler state
- [ ] QStash human setup and rotation are repeatable without credentials in arguments or output — verify: `make check-playbooks`
- [ ] Architecture docs name QStash as clock, `agentsfleetd` as synchronous facade and signed ingress, and state that no schedule worker or NullClaw cron owner exists
- [ ] `make lint` clean · `make test` passes · `make test-integration` passes
- [ ] 100-way concurrency gates pass repeatedly; query/provider round-trip counters stay bounded
- [ ] `make memleak` reports zero for daemon, runner, library, boot-drain, and injected error paths
- [ ] `make check-pg-drain` clean (new `conn.query()` sites)
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `gitleaks detect` clean · no file over 350 lines added

## Metrics & Observability

- Counters: QStash request outcome, explicit sync outcome, synchronization-lease contention, signed fires accepted/rejected/deduplicated/skipped.
- Gauges: schedules by sync state; no per-schedule labels and no worker-age gauge.
- Logs carry schedule/Fleet identifiers, generation, outcome, and error code; never message text, token, signing key, or signed body.

## Discovery (consult log)

> **Empty at creation.** Append as the work surfaces consults and decisions.

- **Architecture fork** — Indy (Jun 29, 2026, AskUserQuestion): chose "agentsfleet owns CRUD, QStash fires" over an in-process scheduler or a storage-less wrapper; provider = Upstash QStash.
- **QStash consult** — Jul 15, 2026: current Upstash documentation confirms token-authenticated schedule registration; signed HTTP delivery with issuer, subject, expiry, not-before, and raw-body hash verification; current/next signing keys support rotation.
- **Facade decision** — Jul 15, 2026: Indy rejected automatic schedule threads/loops and approved continuing with synchronous QStash calls plus explicit `schedule sync` recovery.
- **Retry decision** — Jul 15, 2026: one deadline-bounded QStash attempt runs per mutation; explicit `schedule sync` is the only retry, resolving the earlier conflict between a client retry budget and the no-hidden-retry invariant.
- **Cap-race correction** — Jul 15, 2026: the 100-way database test proved that locking the Fleet and counting schedules in one statement uses the statement snapshot from before the lock wait, admitting four winners from a starting count of 31. The store now locks and drains first, then counts in a second statement inside the same transaction so every lock owner sees the newest committed count.
- **Module naming** — Jul 15, 2026: Indy reserved scheduler terminology for a later distributed scheduler; this integration follows Ghostty's topic-folder convention as `src/agentsfleetd/cron/main.zig` plus private sibling concerns.
- **NullClaw consult** — Jul 15, 2026: the pinned scheduler requires a long-running daemon and local persisted jobs; hosted cron tools are removed because declarative `TRIGGER.md` and explicit CLI/API scheduling cover this workstream.
- **Skill chain outcomes** — (pending) `/write-unit-test`, `/review`, `kishore-babysit-prs`.
- **Deferrals** — none yet (every deferral needs an Indy-acked verbatim quote here).

## Out of Scope

- Any schedule reconciler, polling loop, retry thread, or in-process scheduler — explicit sync is the recovery model for this workstream.
- The future distributed scheduler and its worker/partition/leadership model; it will own a separate `scheduler/` domain when specified.
- Catch-up/backfill of fires missed while a Fleet was stopped — QStash at-least-once + dedup is the floor.
- Dashboard schedule editor — history already renders `cron` events; a read-only schedule view is later work.
- A second scheduling provider; the desired-state service isolates QStash without a speculative provider interface.
- Per-resource scope strings and a scope-management UI (carried by M104_001's out-of-scope list).
