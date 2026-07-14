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

> **Provenance is load-bearing.** LLM-drafted against a live codebase sweep of the event-ingress, webhook-verify, and Fleet-CRUD paths. Re-verify the QStash Schedules API surface and the Upstash signature scheme against current Upstash docs before EXECUTE — external API shapes drift.

**Canonical architecture:** `docs/architecture/data_flow.md` (single-ingress event model — cron is already a named producer) and `docs/architecture/capabilities.md §1.1` (`trigger.type` vs `event_type`, orthogonal). The scheduler is **out-of-process by design**: agentsfleet owns the schedule records and the fire ingress; Upstash QStash owns the clock.

This spec uses Cron Expression, Hash-based Message Authentication Code (HMAC), JSON Web Token (JWT), Finite State Machine (FSM), Pull Request (PR), Command-Line Interface (CLI), Coordinated Universal Time (UTC), and Dead-Letter Queue (DLQ) below.

## Implementing agent — read these first

1. `src/agentsfleetd/http/handlers/fleets/{create,patch,delete,list}.zig` — the workspace-scoped CRUD + status-FSM pattern the schedule handlers mirror (atomic INSERT, `authorizeWorkspace`, `verifyFleetInWorkspace`).
2. `src/agentsfleetd/http/handlers/webhooks/fleet.zig` + `src/agentsfleetd/cmd/serve_webhook_lookup.zig` — the **signature-verify → dedup (SET NX) → XADD** ingress the cron-fire endpoint reuses verbatim; the per-fleet secret-from-vault lookup is the model for the QStash signing key.
3. `src/lib/contract/event_envelope.zig` + `src/agentsfleetd/http/handlers/fleets/messages.zig` — `EventType.cron` already exists; the fire endpoint builds that envelope (`actor=cron:<schedule_id>`) instead of `messages.zig`'s `chat`.
4. `src/agentsfleetd/fleet_runtime/config_helpers.zig` (`parseFleetTriggers`) — already parses + caps the `type: cron` trigger and its `schedule`; §5 reconciles that parsed value into a schedule row. Cron-expression *validation* lives here too (currently absent).
5. `schema/007_core_fleets.sql`, `schema/embed.zig`, and `docs/SCHEMA_CONVENTIONS.md` — the nearest migration to mirror for `core.fleet_schedules` (uuidv7 PK, BIGINT millis timestamps, app-enforced enums, no static-string `CHECK`).

## PR Intent & comprehension handshake

- **PR title (eventual):** Manage Fleet schedules backed by Upstash QStash
- **Intent:** Let an operator create, update, and delete a recurring schedule on a Fleet so it wakes on a cron without any human present — delivered by registering the cron with QStash, which calls a signed agentsfleet ingress that drops a `cron` event on the Fleet's stream.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate the intent and list `ASSUMPTIONS I'M MAKING: …`; reconcile any mismatch before edits.

## Product Clarity (answer in order, at authoring)

1. **Successful user moment** — Indy installs the `zoho-sprint-daily-summarizer` Fleet (`TRIGGER.md`: `type: cron, schedule: "0 18 * * 1-5"`). At 18:00 UTC next weekday, with nobody watching, it wakes, reads the day's Zoho activity, posts the summary — history shows `actor=cron:<schedule_id>`, `event_type=cron`.
2. **Preserved user behaviour** — Steer, webhook ingest, lease/execute/report are untouched; a no-cron Fleet behaves as today; one-lease-per-Fleet holds (a fire mid-execution just queues like a steer).
3. **Optimal-way check** — Direct shape: desired schedule state, idempotent QStash convergence, one signed ingress, and the existing Fleet event stream. `agentsfleet` proves reconciliation but does not calculate fire times.
4. **Rebuild-vs-iterate** — Refactor the scheduling slice: reuse event ingress, dedup, runner framing, and background-loop patterns while replacing NullClaw's hosted local-fallback cron path.
5. **What we build** — `core.fleet_schedules`; one schedule service; a generation-fenced QStash reconciler; CRUD REST routes; a signed cron-fire ingress enqueuing `event_type=cron`; declarative Fleet lifecycle reconciliation; an optimized `agentsfleet schedule` CLI group; hosted NullClaw cron tools; a QStash operator playbook; concise architecture updates.
6. **What we do NOT build** — An `agentsfleet` cron clock; timezone UI; speculative provider abstraction; missed-fire backfill; dashboard editor; NullClaw one-shot delay/run-history tools.
7. **Fit with existing features** — Compounds with Fleet install (the moment) and the event/history surface (`cron` renders today). Must not destabilize the webhook ingress it borrows — the fire route is additive and separately keyed.
8. **Surface order** — Desired state and convergence first; ingress/API/Fleet lifecycle second; runner tools and CLI third; operator and architecture docs last.
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
| PUB / Struct-Shape | yes | Shape verdict on the schedule model/service, reconciler result, hosted tool channel, and scope-gated handler exports; minimise pub surface. |
| File & Function Length (≤350/≤50/≤70) | yes | Split domain, provider, reconciler, ingress, runner channel, and CLI rendering by responsibility. |
| UFS (repeated/semantic literals) | yes | `cron:` actor prefix, dedup-key prefix, scope strings, QStash path fragments → named constants; cron-related identifiers shared verbatim Zig↔CLI. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | yes | `UZ-SCHED-NNN` registry entries; pg-drain on every `conn.query()`; secret-free logs; new migration follows `SCHEMA_CONVENTIONS.md`. |
| SCHEMA GUARD | yes | New table only (no DROP/ALTER); update `schema/embed.zig` + the migration array in the same diff. |

## Overview

**Goal (testable):** Creating a Fleet schedule durably records the desired state, converges QStash to the same generation across concurrent `agentsfleetd` replicas, and turns each signed QStash delivery into at most one `cron` event without runner affinity.

**Problem:** A `TRIGGER.md` may declare `type: cron, schedule: "0 18 * * 1-5"` — the value is parsed, capped, and stored in `config_json`, then **never read**. Customers who want a daily-summary agent have no way to make it fire; the schedule string is dead config.

**Solution summary:** Add a first-class schedule service and desired-state table. API, Fleet-install, Fleet-lifecycle, CLI, and NullClaw tool requests all write through that service. A generation-fenced reconciler converges QStash using the `agentsfleet` schedule identifier as the provider identifier; no database/network atomicity is claimed. QStash owns timing and calls one signed ingress URL with only the schedule identifier. The ingress loads current state, verifies and deduplicates the delivery, then appends the existing `cron` event. Runners stay disposable.

## Prior-Art / Reference Implementations

- **API** → `src/agentsfleetd/http/handlers/fleets/{create,patch,delete}.zig` (workspace-scoped CRUD and lifecycle) + `docs/REST_API_DESIGN_GUIDELINES.md`.
- **Ingress / signature verify / dedup** → `src/agentsfleetd/http/handlers/webhooks/fleet.zig` + `serve_webhook_lookup.zig` — the cron-fire route mirrors its verify→`SET NX` dedup→`XADD` shape; the QStash signing key resolves from vault the same way the per-fleet webhook secret does.
- **Reconciliation** → `src/agentsfleetd/fleet/{liveness,reclaim}_sweeper.zig` + `cmd/serve_background.zig` — bounded, shutdown-aware background work with concurrency tests.
- **Runner bridge** → `src/runner/engine/credential_request.zig` + `child_supervisor_read.zig` — fail-closed child→runner→`agentsfleetd` requests over existing framed pipes.
- **Schema** → `schema/007_core_fleets.sql` + `docs/SCHEMA_CONVENTIONS.md`.
- **CLI** → `cli/src/program/cli-tree-fleet.ts` (`fleet.steer`, the `credential` group) + the **7 Pillars** of CLI developer experience (handler purity, output-as-a-service, structured JSON errors, auto-JSON when piped).

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/{028_core_fleet_schedules.sql,embed.zig}` | CREATE/EDIT | Desired state, generation/claim fields, and migration registration. |
| `src/agentsfleetd/schedules/{model,store,service,cron_validate,qstash_client,qstash_verify,reconciler}.zig` | CREATE | Schedule domain, native provider boundary, signed verification, convergence. |
| `src/agentsfleetd/schedules/*_test.zig` | CREATE | Failure, leak, concurrency, and performance proofs. |
| `src/agentsfleetd/http/handlers/{schedules/manage,ingress/qstash,runner/schedules}.zig` | CREATE | Thin workspace, signed-ingress, and runner-identity adapters. |
| `src/agentsfleetd/http/{routes,router,route_matchers,route_table,route_scopes,route_table_invoke_fleets,route_table_invoke_runner,route_table_invoke_webhooks}.zig` | EDIT | Route and authorize the three surfaces. |
| `src/agentsfleetd/{auth/scopes.zig,auth/scopes_test.zig,cmd/serve_background.zig}` | EDIT | Schedule scopes and reconciler lifecycle. |
| `src/agentsfleetd/fleet_runtime/config_helpers.zig` + `src/agentsfleetd/http/handlers/fleets/{create,patch,delete,stop,resume,kill}.zig` | EDIT | Shared validation and Fleet lifecycle reconciliation. |
| `src/agentsfleetd/errors/{error_registry,error_entries}.zig` | EDIT | Schedule errors and recovery text. |
| `src/lib/contract/{protocol,protocol_schedules}.zig` | CREATE/EDIT | Runner request/reply shapes and route. |
| `src/runner/{pipe_proto,child_supervisor,child_supervisor_read}.zig` | EDIT | Carry typed schedule frames over existing pipes. |
| `src/runner/engine/{schedule_request,platform_cron_tools,tool_bridge,tool_builders,run_context,runner_helpers}.zig` | CREATE/EDIT | Hosted cron tools with no local fallback. |
| `src/runner/daemon/control_plane_client_schedule.zig` | CREATE | Lease-bound runner API forwarding. |
| `cli/src/program/cli-tree-fleet.ts` + `cli/src/commands/fleet_schedule*.ts` | CREATE/EDIT | Wait/no-wait schedule commands and rendering. |
| `playbooks/operations/qstash_registration/001_playbook.md` | CREATE | Account, vault, ingress, and rotation setup. |
| `docs/architecture/{data_flow,user_flow,capabilities,high_level}.md` | EDIT | One concise hosted scheduling model. |

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Eight Sections split desired state, provider convergence, API, ingress, Fleet integration, CLI, runner tools, and operator/architecture documentation. Handlers do no provider orchestration.
- **Alternatives considered:** Direct database-plus-QStash writes were rejected because two systems cannot commit atomically. NullClaw's local scheduler was rejected because its daemon and `cron.json` die with the disposable child. A storage-less QStash wrapper was rejected because auth, lifecycle, status, and recovery would be invisible to `agentsfleet`.
- **Refactor verdict:** **structural refactor** — one service becomes the only mutation path; a generation fence plus reclaimable work lease makes concurrent replicas converge; the runner replaces unsafe local-fallback cron tools with a central fail-closed bridge. QStash still owns the clock, so `agentsfleet` does not implement cron timing.

## Sections (implementation slices)

### §1 — Schedule record + cron validation

The persisted desired state and shared cron guard. **Implementation default:** the `agentsfleet` identifier is also the QStash schedule identifier; no premature provider identifier or provider interface is stored. Desired lifecycle and provider sync state are separate so users can see convergence honestly.

- **Dimension 1.1** — `core.fleet_schedules` persists identity, ownership, source, cron, timezone, message, desired state, sync state, generation, retry/claim timestamps, error, and audit timestamps → Test `test_schedule_row_roundtrips`
- **Dimension 1.2** — a malformed cron expression is rejected at write with `UZ-SCHED-001` naming the field → Test `test_cron_validate_rejects_malformed`
- **Dimension 1.3** — the (N+1)th schedule on a Fleet is rejected with `UZ-SCHED-003` → Test `test_schedule_per_fleet_cap`

### §2 — Generation-fenced QStash convergence

A native QStash client and bounded background reconciler converge rows without holding a database transaction over network input/output. Workers claim due rows with expiring leases; provider operations use the stable `agentsfleet` schedule identifier; completion updates are generation-guarded. QStash credentials load from the platform administrative vault key `qstash`. **Implementation default:** one configured destination `/v1/ingress/qstash/schedules` receives every fire and QStash carries only `schedule_id` in the body.

- **Dimension 2.1** — create/update/delete converge idempotently by stable schedule identifier and expected generation → Test `test_qstash_reconcile_converges_generation`
- **Dimension 2.2** — a stale worker cannot overwrite a newer mutation; an expired claim is reclaimable → Test `test_reconciler_generation_fence`
- **Dimension 2.3** — provider timeout, rate limit, server error, malformed reply, and allocation failure leave durable retry/error state without leaks → Test `test_reconciler_failure_matrix`
- **Dimension 2.4** — 100 concurrent claimers produce one committed generation and bounded provider calls without a global lock → Test `test_reconciler_100_way_concurrency`

### §3 — Schedule CRUD REST surface

`POST`/`GET`/`PATCH`/`DELETE` under `…/fleets/{id}/schedules`, workspace-authorized, gated on `schedule:write` (mutations) / `schedule:read` (list). Mirrors the Fleet-CRUD handlers.

- **Dimension 3.1** — `POST` persists desired state and returns `202` with identifier, generation, and `provisioning` status → Test `test_create_schedule_202`
- **Dimension 3.2** — `PATCH` atomically advances desired generation and returns current sync status → Test `test_patch_schedule_generation`
- **Dimension 3.3** — `DELETE` marks durable deletion and returns `202`; the row disappears only after provider deletion → Test `test_delete_schedule_converges`
- **Dimension 3.4** — a principal lacking `schedule:write` gets `403` naming the scope; cross-workspace access gets `403` → Test `test_schedule_authz`

### §4 — Cron-fire ingress + idempotency

A signed ingress verifies current and next QStash signing keys against the raw body, exact configured URL, issuer, expiry, not-before, and body hash. It cross-checks the body/header schedule identifiers, loads current state, then deduplicates before `XADD`.

- **Dimension 4.1** — a valid signed fire enqueues exactly one `cron` event on `fleet:{id}:events` → Test `test_fire_enqueues_cron_event`
- **Dimension 4.2** — an invalid/absent signature is rejected `401` and enqueues nothing → Test `test_fire_rejects_bad_signature`
- **Dimension 4.3** — a replayed fire (same message id) is an idempotent no-op (`200`, no second event) → Test `test_fire_dedupes_replay`
- **Dimension 4.4** — a fire for missing, stopped, killed, paused, deleting, or unsynced state enqueues nothing and returns `200` → Test `test_fire_skips_inactive`
- **Dimension 4.5** — 100 concurrent copies of one signed delivery create one stream entry with no serialized global lock → Test `test_fire_100_way_exactly_once`

### §5 — Declarative cron and Fleet lifecycle

Fleet install and patch reconcile the single declarative cron schedule by source identity; stop/kill suppress fires, resume restores the desired schedule, and hard delete removes provider schedules before deleting the Fleet. All paths call the same service.

- **Dimension 5.1** — installing or patching a Fleet with `type: cron` creates or updates exactly one declarative desired schedule → Test `test_fleet_cron_reconciles_schedule`
- **Dimension 5.2** — installing a Fleet with no cron trigger creates no schedule → Test `test_install_no_cron_no_schedule`
- **Dimension 5.3** — stop/resume/kill/delete never leave an effective or orphaned provider schedule → Test `test_fleet_lifecycle_reconciles_schedules`

### §6 — Optimized `agentsfleet schedule` CLI

`add | list | update | rm | status` over the §3 routes, following the 7 Pillars. Human mode waits by default for `active` or actionable `error`; `--no-wait` returns the durable `202` state immediately; piped mode emits JSON without a spinner.

- **Dimension 6.1** — `agentsfleet schedule add <fleet_id> --cron "0 18 * * 1-5"` creates a schedule and prints its id → Test `test_cli_schedule_add` (e2e, subprocess)
- **Dimension 6.2** — `agentsfleet schedule list <fleet_id>` renders human + (piped) JSON → Test `test_cli_schedule_list_json`
- **Dimension 6.3** — `update`/`rm` round-trip to the API and surface structured errors → Test `test_cli_schedule_update_rm`
- **Dimension 6.4** — wait/no-wait, provider error guidance, and piped output are deterministic → Test `test_cli_schedule_wait_experience`

### §7 — NullClaw hosted cron tool bridge

Replace the pinned NullClaw local cron builders for hosted execution. `cron_add`, `cron_list`, `cron_update`, and `cron_remove` use a typed child→runner request over existing framed pipes; the runner binds Fleet/workspace/lease identity and calls the runner schedule route. Missing channel, expired lease, unsupported local-only operation, or control-plane failure returns an explicit tool error; it never falls back to `cron.json` or launches `curl`.

- **Dimension 7.1** — hosted cron tools round-trip through child, runner, and schedule service with lease-bound identity → Test `test_hosted_cron_tool_roundtrip`
- **Dimension 7.2** — missing channel, forged Fleet identity, expired lease, and daemon failure fail closed with no local file → Test `test_hosted_cron_tool_failure_matrix`

### §8 — Operator setup and architecture accuracy

The platform operator gets one repeatable QStash registration playbook, while the architecture documents name one scheduling model consistently for humans and agents. NullClaw retains the cron tool vocabulary, but hosted schedules are stored by `agentsfleet`, timed by QStash, and delivered through `agentsfleetd`.

- **Dimension 8.1** — the playbook covers QStash account setup, API token plus current/next signing keys, vault storage, public ingress verification, and rotation without printing credentials → Test `make check-playbooks`
- **Dimension 8.2** — architecture documents no longer claim a disposable NullClaw child owns or fires hosted cron schedules → Test `architecture_schedule_ownership`

## Interfaces

```
POST   /v1/workspaces/{ws}/fleets/{id}/schedules        → 202 Schedule
GET    /v1/workspaces/{ws}/fleets/{id}/schedules        → 200 {schedules:[Schedule]}
GET    /v1/workspaces/{ws}/fleets/{id}/schedules/{sid}  → 200 Schedule
PATCH  /v1/workspaces/{ws}/fleets/{id}/schedules/{sid}  → 202 Schedule
DELETE /v1/workspaces/{ws}/fleets/{id}/schedules/{sid}  → 202 Schedule
POST   /v1/ingress/qstash/schedules                     → 200 {accepted:true}
POST   /v1/runners/me/schedules                         → 200 typed tool response

Create body: { "cron": "0 18 * * 1-5", "message": "summarize today's Zoho Sprints", "timezone": "UTC" }
  - cron      : required, 5-field expression, validated (UZ-SCHED-001)
  - message   : required, the steer text delivered as the cron event payload (≤8192 bytes)
  - timezone  : optional, app-default UTC

Schedule: {schedule_id, fleet_id, source, cron, timezone, message,
           desired_status, sync_status, generation, last_error?, created_at, updated_at}

Event enqueued on fire: EventEnvelope{ event_type=.cron, actor="cron:<schedule_id>",
                        request_json={ "message": <message>, "schedule_id": <sid>, "fired_at": <ms> } }
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Invalid cron | Malformed expression on create/update | Reject `422` `UZ-SCHED-001` naming the field; nothing persisted. |
| Per-fleet cap | (N+1)th schedule | Reject `409` `UZ-SCHED-003`; nothing persisted. |
| Schedule not found | Bad `{sid}` on workspace management route | `404` `UZ-SCHED-002`. Signed ingress treats an absent row as an idempotent `200`. |
| Provider unavailable | QStash timeout, rate limit, or server error | Desired state remains durable; `sync_status=error`, retry is scheduled, CLI prints `UZ-SCHED-004` recovery. |
| Concurrent mutation | Reconciler completes an older generation | Guarded update affects zero rows; newer desired state remains pending and wins. |
| Reconciler crash | Worker dies after claim or provider call | Claim expires; another replica idempotently resumes using the stable schedule identifier. |
| Fire signature invalid | Forged/absent QStash signature | `401` `UZ-SCHED-005`; nothing enqueued (fail closed). |
| Replayed fire | QStash at-least-once retry | Idempotent `200`; dedup key suppresses the second `XADD`. |
| Fire for inactive Fleet | Fleet stopped/killed or schedule paused | `200`; nothing enqueued (no backlog on resume). |
| Cross-workspace / missing scope | Wrong workspace or no `schedule:write` | `403` naming the scope; no state change. |
| Hosted cron bridge unavailable | Child lacks a schedule channel or lease is invalid | Tool fails closed; no local `cron.json`, subprocess, or provider mutation. |

## Invariants

1. Provider completion for generation N cannot overwrite desired generation N+1 — enforced by generation-guarded updates.
2. A stored cron expression is always well-formed — enforced by the validator at every write (CRUD and the `TRIGGER.md` auto-provision share one `cron_validate`); rejection is `UZ-SCHED-001`.
3. The fire ingress enqueues at most one `cron` event per `(schedule_id, qstash_message_id)` — enforced by the `SET NX` dedup key, the same primitive the webhook ingress uses.
4. The fire ingress enqueues only for an `active` Fleet + `active` schedule — enforced by a status read before `XADD`.
5. No QStash token or signing key is ever logged — enforced by the logging discipline (vault refs only; `LOG` rule).
6. Cron field names, the `cron:` actor prefix, scope strings, and the dedup-key prefix are named constants, shared verbatim across Zig and the CLI — enforced by `UFS`.
7. Hosted NullClaw cron tools cannot write local scheduler state — enforced by replacing every hosted cron builder with a typed platform tool and testing the workspace remains unchanged.
8. Reconciliation performs a constant bounded number of database/provider operations per claimed schedule — enforced by round-trip counters at 1/10/100 rows.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_schedule_row_roundtrips` | INSERT then SELECT returns identical field set incl. uuidv7 PK. |
| 1.2 | unit | `test_cron_validate_rejects_malformed` | `"99 * * * *"`, `"* * *"`, `""` → rejected; `"0 18 * * 1-5"` → accepted. |
| 1.3 | integration | `test_schedule_per_fleet_cap` | (cap+1)th create → `409 UZ-SCHED-003`. |
| 2.1 | integration | `test_qstash_reconcile_converges_generation` | desired create/update/delete converge by stable identifier. |
| 2.2 | integration | `test_reconciler_generation_fence` | stale completion loses; expired claim is reclaimed. |
| 2.3 | integration | `test_reconciler_failure_matrix` | injected timeout/429/5xx/malformed/allocation failures preserve retryable state and leak nothing. |
| 2.4 | concurrency | `test_reconciler_100_way_concurrency` | barrier-started claimers preserve one generation, bounded calls, and parallel progress. |
| 3.1 | e2e | `test_create_schedule_202` | real HTTP `POST` → `202` + provisioning schedule row. |
| 3.2 | integration | `test_patch_schedule_generation` | `PATCH` advances generation exactly once. |
| 3.3 | integration | `test_delete_schedule_converges` | delete remains visible until provider removal, then row disappears. |
| 3.4 | integration | `test_schedule_authz` | no `schedule:write` → `403` naming scope; other workspace → `403`. |
| 4.1 | integration | `test_fire_enqueues_cron_event` | signed fire → exactly one stream entry, `event_type=cron`, `actor=cron:<sid>`. |
| 4.2 | integration | `test_fire_rejects_bad_signature` | bad signature → `401`; stream length unchanged. |
| 4.3 | integration | `test_fire_dedupes_replay` | same message id twice → one stream entry; second is `200` no-op. |
| 4.4 | integration | `test_fire_skips_inactive` | killed Fleet / paused schedule → `200`, stream length unchanged. |
| 4.5 | concurrency | `test_fire_100_way_exactly_once` | 100 identical signed requests → one stream event and parallel verification. |
| 5.1 | integration | `test_fleet_cron_reconciles_schedule` | install/patch declarative cron → one sourced desired schedule at latest generation. |
| 5.2 | integration | `test_install_no_cron_no_schedule` | install non-cron Fleet → zero schedules. |
| 5.3 | integration | `test_fleet_lifecycle_reconciles_schedules` | stop/resume/kill/delete suppress or remove provider state without orphan. |
| 6.1 | e2e | `test_cli_schedule_add` | subprocess `schedule add … --cron` → exit 0, prints id. |
| 6.2 | e2e | `test_cli_schedule_list_json` | piped stdout → JSON array; tty → human table. |
| 6.3 | e2e | `test_cli_schedule_update_rm` | update + rm round-trip; structured error on bad cron. |
| 6.4 | e2e | `test_cli_schedule_wait_experience` | TTY waits cleanly; piped/no-wait returns stable JSON; provider error includes recovery. |
| 7.1 | e2e | `test_hosted_cron_tool_roundtrip` | child request reaches lease-bound service and returns typed result. |
| 7.2 | failure | `test_hosted_cron_tool_failure_matrix` | missing channel/forged identity/expired lease/downstream failure creates no local state. |
| 8.1 | documentation | `make check-playbooks` | QStash registration playbook passes repository playbook checks. |
| 8.2 | documentation | `architecture_schedule_ownership` | Four architecture documents consistently name QStash, `agentsfleetd`, and disposable runner ownership. |

**Regression:** steer (`event_type=chat`), webhook ingest, and lease/execute/report paths unchanged — `test_steer_unaffected`, existing webhook + lease suites stay green.
**Idempotency/replay:** Dimensions 2.1–2.4 and 4.3–4.5 prove provider and delivery idempotency under retries and concurrency.
Cron and QStash fixtures: `samples/fixtures/m105-fixtures/{valid_crons.json, qstash_fire.json}`.

## Acceptance Criteria

- [ ] Create→fire→wake end-to-end (QStash double) lands one `cron` event — verify: `make test-integration 2>&1 | grep test_fire_enqueues_cron_event`
- [ ] Desired state converges across crash, stale generation, provider failure, and 100-way contention
- [ ] Installing the Zoho-summarizer fixture creates one declarative schedule and Fleet lifecycle leaves no provider orphan
- [ ] Bad signature / replay / inactive Fleet all enqueue nothing — verify: `make test-integration 2>&1 | grep -E "bad_signature|dedupes_replay|skips_inactive"`
- [ ] CLI `schedule add|list|update|rm|status` gives deterministic wait/no-wait human and JSON output
- [ ] NullClaw cron tools use the central lease-bound bridge and never touch local scheduler state
- [ ] QStash human setup and rotation are repeatable without credentials in arguments or output — verify: `make check-playbooks`
- [ ] Architecture docs name QStash as clock, `agentsfleetd` as signed ingress, and NullClaw children as disposable executors
- [ ] `make lint` clean · `make test` passes · `make test-integration` passes
- [ ] 100-way concurrency gates pass repeatedly; query/provider round-trip counters stay bounded
- [ ] `make memleak` reports zero for daemon, runner, library, boot-drain, and injected error paths
- [ ] `make check-pg-drain` clean (new `conn.query()` sites)
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `gitleaks detect` clean · no file over 350 lines added

## Metrics & Observability

- Counters: reconciliation claims/success/retry/stale-generation, QStash request outcome, signed fires accepted/rejected/deduplicated/skipped.
- Gauges: schedules by sync state and oldest due reconciliation age; no per-schedule labels.
- Logs carry schedule/Fleet identifiers, generation, outcome, and error code; never message text, token, signing key, or signed body.

## Discovery (consult log)

> **Empty at creation.** Append as the work surfaces consults and decisions.

- **Architecture fork** — Indy (Jun 29, 2026, AskUserQuestion): chose "agentsfleet owns CRUD, QStash fires" over an in-process scheduler or a storage-less wrapper; provider = Upstash QStash.
- **QStash consult** — Jul 15, 2026: current Upstash documentation confirms token-authenticated schedule registration; signed HTTP delivery with issuer, subject, expiry, not-before, and raw-body hash verification; current/next signing keys support rotation.
- **NullClaw consult** — Jul 15, 2026: the pinned scheduler requires a long-running daemon and local persisted jobs; the `agentsfleet-runner` child is one-shot, so NullClaw cron tools must target the central schedule API for hosted Fleets.
- **Skill chain outcomes** — (pending) `/write-unit-test`, `/review`, `kishore-babysit-prs`.
- **Deferrals** — none yet (every deferral needs an Indy-acked verbatim quote here).

## Out of Scope

- In-process scheduler thread (the `approval_gate_sweeper.zig`-style waker) — named future spec if the external dependency becomes unacceptable.
- Catch-up/backfill of fires missed while a Fleet was stopped — QStash at-least-once + dedup is the floor.
- Dashboard schedule editor — history already renders `cron` events; a read-only schedule view is later work.
- A second scheduling provider; the desired-state service isolates QStash without a speculative provider interface.
- Per-resource scope strings and a scope-management UI (carried by M104_001's out-of-scope list).
