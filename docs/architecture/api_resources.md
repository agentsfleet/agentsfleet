---
type: explanation
audience: contributor
verified: 2026-11-25
product_version: 0.90.0
executable: false
---

# API resources: fleets, their access and the run verbs

> Read this when you need the HTTP route that creates or changes a fleet, or the run verb a fleet's tool calls. Parent: [`README.md`](./README.md) · Wire types live in `af_wire` and `afd_api_wire`; routes in `rustd/crates/daemon/afd_http/src/route/`.

## What it is

Two kinds of caller change a fleet.
A person (or their API key) uses the person routes under a workspace.
A running fleet uses the run verbs, which its tools call through the runner and which are fenced to that one run.

| Prefix | Path | Caller |
|---|---|---|
| `W` | `/v1/workspaces/{workspace_id}` | a person or an API key |
| `F` | `W/fleets/{fleet_id}` | a person or an API key |
| `L` | `/v1/runners/me/runs/{run_id}` | a runner, for one run it holds |

Every identifier this system mints is a UUIDv7.
The committed OpenAPI document, `public/openapi.json`, is generated from the code and lists every route; this page explains the fleet routes and run verbs.

## Why it exists

A fleet is created by its first message and grows by asking: it adds its own schedules and triggers and requests its own access.
The person routes let a person read, steer, approve and remove; the run verbs let the fleet ask.
Keeping both on one page shows which side may do what.

## How it behaves

### Authorization

Person routes authorize through workspace membership.
Fleet reads need `fleet:read`, writes need `fleet:write`, and deleting a fleet needs `fleet:admin`.
Schedules take `schedule:read` and `schedule:write`; grants take `grant:read` and `grant:write`.

Four writes need a signed-in person, never an API key.
They approve or deny an access card, revoke a grant, open a fleet's network, or edit a task's statements.
An API key on one of them gets `403 UZ-AUTH-001`, and nothing changes (`afd_http/src/auth/person.rs`).

Access is checked again on every read, write and stream reconnect.

### Example: creating a fleet

The example omits authorization headers. The body names no model, so the fleet takes the workspace's selected model.

```http
POST /v1/workspaces/0199a801-1000-7000-8000-000000000001/fleets
Content-Type: application/json

{"message":"Watch failed checkout deployments.","operation_id":"0199a801-1000-7000-8000-000000000002"}
```

```text
201 Created
{
  "fleet": {
    "id": "0199a801-1000-7000-8000-000000000003",
    "name": "fleet-000000000003",
    "status": "active",
    "model_ref": "<varies>",
    "http_egress": "restricted",
    "package_domains": false,
    "schedules": [], "triggers": [], "grants": [],
    …
  },
  "accepted": {"turn_id": "<varies>", "disposition": "started"},
  "replayed": false
}
```

With no `name`, the name is `fleet-` and the last 12 hex digits of the fleet id.
Sending the same body with the same `operation_id` again answers `200` with the same fleet and `"replayed": true`.

### Shared wire rules

| Topic | Rule |
|---|---|
| Unknown fields | JSON requests refuse them |
| Numbers as strings | money, message sequences, fences and event counts; spend uses `Nanos`, balances `SignedNanos` |
| Times | RFC 3339 instants in UTC, millisecond precision |
| Paging | `limit` and `starting_after`; default 50, at most 100; a page is `{items, total: null, next_cursor}` |
| Versions | `GET F` returns a strong entity tag (ETag) in the `ETag` header and the `etag` field; compression may weaken the header, so clients match the body's tag |
| Conditional writes | every `PATCH` and `DELETE F` sends the strong tag in `If-Match` |
| Retry keys | `POST W/fleets` and `POST F/messages` take the key only as the body's `operation_id` |

A create's receipt is scoped to the workspace and the person, and authorization runs before replay.
The same key with another body is `409 UZ-SETUP-005`, and a repeat after that fleet was deleted is `410 UZ-SETUP-013`.
The receipt holds for `[setup] replay_window_hours`, 24 hours by default ([configuration](./config_reference.md)).

Every message is 1 to 1,048,576 characters (`af_validate::TEXT_MAX_CHARS`); a longer one is refused `UZ-REQ-004`.
A body carrying one text may be up to `TEXT_BODY_BYTES_MAX` bytes (`afd_http/src/handler/setup_input.rs`).
A run carrying that text reaches its runner in one frame of at most 32 MiB.

### Types

| Type | Fields | Notes |
|---|---|---|
| `Fleet` | `id, name, status, etag, model_ref, http_egress, package_domains, schedules[], triggers[], grants[], memory_access, created_at, updated_at, events_processed, budget_used_nanos, pending_approvals` | `status` is `active` or `archived` |
| `FleetSummary` | `id, name, status, created_at, updated_at, events_processed, budget_used_nanos` | one row of `GET W/fleets` |
| `Grant` | `id, kind, name, status, scope?, requested_scope?, approved_at?` | one `connector.integration_grants` row |
| `Trigger` | `id, kind, source, events?, repositories?, channel?, url?, created_at` | one `fleet.triggers` row |
| `Schedule` | `schedule_id, fleet_id, cron, timezone, message, source, once, status, sync, etag, description, run_state, next_run_at` | one schedule row |

`http_egress` is `restricted` or `public`, `package_domains` is a boolean, and both start off.
`memory_access` is `{read, publish}`; `budget_used_nanos` is what the fleet's runs have spent, never a cap ([billing](./billing.md)).
`pending_approvals` counts the fleet's unanswered approval cards.

A name is 1 to 64 bytes of lowercase ASCII letters, digits and hyphens, unique in its workspace.
A taken name is `409 UZ-AGT-006` with nothing written.
`GET W/fleets?name=<name>` answers that one fleet, archived included, or no item; the dashboard's and the command line's `@name` use it.

A grant's `kind` is `secret` for a workspace secret or `integration` for a mintable token.
Its `status` is `pending`, `approved` or `revoked`, and a deny is stored as `revoked`.

`scope` and `requested_scope` are set for GitHub only, naming the repositories and read or write.
A trigger's `kind` is `webhook`, `app_event` or `mention`; a webhook's `url` is `/v1/webhooks/{fleet_id}/{source}`.

A schedule's `source` is `api` for a person's schedule and `fleet` for one the fleet added.
`run_state` is `scheduled`, `paused`, `activation_pending` or `next_run_unavailable`; `next_run_at` is set only when it is `scheduled`.
`sync` is `syncing`, `synced` or `failed`, and `status` is `active`, `paused` or `deleting`.

### Workspace routes

| Route | Input | Success | Failures |
|---|---|---|---|
| `GET W/models` | optional `purpose=runtime`; paging | 200 page of `{ref, label, provider, model, context_window_tokens, max_output_tokens, eligibility: {runtime}, pricing}` | AUTH, INPUT, CURSOR, DEPENDENCY |
| `POST W/fleets` | `{message, operation_id, model_ref?, name?}`; `fleet:write` | 201 `{fleet, accepted: {turn_id, disposition}, replayed: false}`; a repeat answers 200 | INPUT, REPLAY, GONE, MODEL, NAME, AUTH, DEPENDENCY |
| `GET W/fleets` | paging; optional `?name=` | 200 page of `FleetSummary` | INPUT, CURSOR, AUTH, DEPENDENCY |

`POST W/fleets` writes the fleet, its thread and its first turn in one transaction.
With no usable model it answers `409 UZ-SETUP-008` and writes nothing.
[Data flow](./data_flow.md) owns the create transaction and its recovery.

`W/secrets`, `W/turns`, `W/turns/stream` and `W/approvals` also stand.
`POST W/approvals/{gate_id}/{decision}` takes `approve`, `deny` or `answer`; approve and deny need a person.

### Fleet routes

| Route | Scope | Input | Success | Failures |
|---|---|---|---|---|
| `GET F` | `fleet:read` | none | 200 `Fleet` and ETag | 404 `UZ-AGT-009`, AUTH, DEPENDENCY |
| `PATCH F` | `fleet:write` | `If-Match`; at least one of `{status: "archived", name, model_ref, http_egress, package_domains}`; the last two need a person | 200 `Fleet`; a model change writes one `model_changed` turn | PRECONDITION, STALE, INPUT, MODEL, NAME, PERSON; archived 409 `UZ-AGT-020` |
| `DELETE F` | `fleet:admin` | `If-Match` | 204; deletes the fleet and its history in any status | PRECONDITION, STALE, 404 |
| `POST F/messages` | `fleet:write` | `{message, operation_id?}` | 202 `{status, turn_id, replayed, disposition, folded_into_turn_id?}` | INPUT, REPLAY (`UZ-AGT-016`), STATE, AUTH |
| `GET F/messages` | `fleet:read` | paging, default 25; optional `event_type` | 200 message page | INPUT, CURSOR, AUTH |
| `GET F/turns`, `GET F/turns/stream` | `fleet:read` | paging, or a stream | the turn history, or the live tail | AUTH |
| `GET`/`PATCH F/turns/{turn_id}` | `fleet:read` / `fleet:write` | `PATCH` body `{status: "cancelled"}` | the turn; `PATCH` stops it | AUTH, 404 |
| `GET F/turns/{turn_id}/items/{item_id}` | `fleet:read` | none | one tool call in full | AUTH, 404 |
| `GET F/memories`, `DELETE F/memories/{key}` | `fleet:read` / `fleet:write` | optional search | the entries; delete forgets one | AUTH, 404 |
| `PATCH F/memory-access` | `fleet:write` | `{read?, publish?}` | 200 `{read, publish}` | INPUT, AUTH |
| `GET F/integration-grants` | `grant:read` | none | 200 `{items: Grant[]}` | AUTH |
| `DELETE F/integration-grants/{id}` | `grant:write`, a person | none | 204; a live run keeps what it was issued | PERSON, 404 |
| `GET F/triggers`, `DELETE F/triggers/{id}` | `fleet:read` / `fleet:write` | none | the triggers; after delete its URL answers 404 | AUTH |
| `GET`/`POST F/schedules` | `schedule:read` / `schedule:write` | `POST` body `{cron, timezone, message, …}` | the schedules, or 201 `Schedule` | INPUT, AUTH |
| `GET`/`PATCH`/`DELETE F/schedules/{id}` | `schedule:read` / `schedule:write` | `PATCH` needs `If-Match`; an `override` body is 400 | the `Schedule` and ETag | PRECONDITION, STALE, STATE, 404 `UZ-SCHED-002` |
| `POST F/schedules/{id}/sync` | `schedule:write` | none | forces a sync now | AUTH, 404 |
| `GET F/durable_tasks`, `GET F/durable_tasks/{id}` | `fleet:read` | paging, default 20, at most 100 | unfinished tasks, or one task with an ETag | AUTH, 404 |
| `PATCH F/durable_tasks/{id}` | `fleet:write`; a statement edit needs a person | `If-Match`; `{state, expected_generation}` or `{questions, expected_generation}` | 200; a statement edit raises the generation by 1 | stale 412 `UZ-DT-013`, PERSON |

A message sent while the fleet's run is live folds into that run (`disposition: folded`), and the run's answer is its reply.
A message behind a task-bound message queues instead.
`POST F/messages` deduplicates on its key; omitting the key creates new work on every request.

A deleted fleet takes its turns, runs, approvals, grants, keys and memory with it, and cannot be undone.
To keep the history, set `status` to `archived` instead.
Task reads answer `private, no-store` for the list and `private, no-cache` with `If-None-Match` support for one task.

### Run verbs

Each verb is fenced to its run and carries the run's `fencing_token`.
A refusal returns to the model as a tool result of the form `[kind] sentence`, and the run continues.

| Verb | Tool | Input | Success | Refusals |
|---|---|---|---|---|
| `POST L/access` | `request_access` | `{name, scope?}` | 201 `{grant_id, gate_id}`; 200 when a grant or request already covers it | 404 name not in the vault or integrations; 409 integration not connected; a GitHub request with no or bad scope |
| `POST L/triggers` | `trigger_add` | `{kind, source?, events?, repositories?, verifier?, secret_name?, channel?}` | 201 `Trigger` | 409 provider not connected; 404 signing secret missing; 409 trigger exists; a webhook source of `approval` or `github` |
| `GET L/triggers`, `DELETE L/triggers/{id}` | `trigger_list`, `trigger_remove` | none | the triggers, or 204 | the fence only |
| `GET`/`POST L/schedules`; `PATCH`/`DELETE L/schedules/{id}`; `GET`/`POST L/schedules/{id}/runs` | the schedule tools, such as `cron_add` | schedule fields | the schedule or its runs | the fence; schedule validation |
| `POST L/messages` | a message to the thread | `{fencing_token, item_id, call_number, text}` | the line said to the held turn's thread | the fence; the text ceiling |
| `POST L/steers` | a steer to another fleet | `{fencing_token, fleet_id, text}` | work put on another fleet in the workspace; a resend answers the first turn | the fence; the text ceiling |
| `GET L/fleets`, `GET L/fleets/{fleet_id}/turns` | reading other fleets | paging | fleets in the workspace, or one fleet's turns | the fence |
| `POST L/items` | none; the runner records finished calls | the calls' full records | 200 | the fence |
| `POST L/durable_tasks/bindings` | `task_start`, `task_input` | `{fencing_token, source_turn_id, plan_digest, items: [one]}` | 201 the bindings; a repeat answers 200 | `UZ-DT-004`, `UZ-DT-005`, `UZ-DT-003`, `UZ-DT-001`; 400 a new item with no statement |

`POST L/access` ends the run waiting on a card, as a run's question does.
The other `L/durable_tasks/*` verbs (decisions, inputs, effects and checkpoints) belong to [durable tasks](./durable_tasks.md).
The runner's stream, renewals, folds and reports are not HTTP verbs; [runner protocol](./runner_protocol.md) owns them.

### Errors

Every failure is `application/problem+json`: `{docs_uri, title, detail, error_code, request_id, user_message?}`.
A 409 adds `current_state`, and a 412 adds the current `etag`.

| Symbol | Code | HTTP | Meaning |
|---|---|---|---|
| AUTH | `UZ-AUTH-002` / `UZ-AUTH-022` | 401 / 403 | invalid authentication, or a missing scope |
| PERSON | `UZ-AUTH-001` | 403 | an API key on a write that needs a signed-in person |
| INPUT | `UZ-REQ-001` / `UZ-UUIDV7-009` | 400 | bad body, bound or unknown field; a bad path UUIDv7 |
| STATE | `UZ-SETUP-002` | 409 | wrong lifecycle state, such as a schedule being deleted |
| STALE | `UZ-SETUP-003` | 412 | the `If-Match` tag is stale |
| PRECONDITION | `UZ-SETUP-004` | 428 | `If-Match` is missing; read the resource first |
| REPLAY | `UZ-SETUP-005` | 409 | a create key reused with another body or person |
| MODEL | `UZ-SETUP-008` | 409 | no usable model; nothing is written |
| CURSOR | `UZ-SETUP-011` | 400 | a cursor that is malformed or bound to another query |
| GONE | `UZ-SETUP-013` | 410 | the fleet a create key made was deleted |
| NAME | `UZ-AGT-006` | 409 | the name is taken in the workspace |
| TASK | `UZ-DT-001` / `003` / `004` / `005` / `013` | 404 / 409 / 409 / 429 / 412 | finished or foreign task; second plan; source not a person's message; unfinished-task cap; stale tag |
| DEPENDENCY | `UZ-INTERNAL-001` | 503 | a required datastore is unavailable |

An oversized body is `413 UZ-REQ-002`.
Validation detail names the field and never echoes private message content.

## Limits

- **A create key expires.** After the replay window, a repeated create makes a second fleet, so read `GET W/fleets` before resending.
- **A revoked grant does not reach into a live run.** The run keeps what it was issued. The next run and the next token mint do not.

## Related pages

- [User flow](./user_flow.md): what a fleet is
- [Data flow](./data_flow.md): the create transaction and recovery
- [Connectors](./connectors.md): grants and triggers
- [Durable tasks](./durable_tasks.md): task verbs and reads
- [Runner protocol](./runner_protocol.md): the runner connection
- [`../AUTH.md`](../AUTH.md): scopes and tokens
