---
type: reference
audience: contributor
verified: 2026-11-07
product_version: 0.75.0
executable: false
---

# API resources: fleets, their access and the runner verbs

## Synopsis

This reference lists the person routes that create and change a fleet, and the run verbs a fleet's run calls. [What a fleet is](user_flow.md#81-what-a-fleet-is) owns the record, and [fleet creation and recovery](data_flow.md#fleet-creation-and-recovery) owns the create transaction.

Prefixes: `W=/v1/workspaces/{workspace_id}`, `F=W/fleets/{fleet_id}`, `L=/v1/runners/me/runs/{run_id}`. Workspace, fleet, turn, grant, trigger, task and run identifiers are UUIDv7.

Person routes authorize through workspace membership, resolved by the existing layer. Fleet reads need `fleet:read`, writes need `fleet:write`, and deletion keeps `fleet:admin` in every state. Every member reads a new fleet and its messages at once, before any run.

Four writes need a signed-in person, not an API key. They approve or deny an access card, revoke a grant, open a fleet's network and edit a task's statements.

Each of those routes reads the caller through `afd_http`'s person extractor (`afd_http/src/auth/person.rs`). An API key on them gets 403 `UZ-AUTH-001`, and nothing changes.

Revalidate current access on every read, write and stream reconnect.

## Example with output

The example omits authorization headers. The model comes from the workspace's selected model, because the body names none.

```http
POST /v1/workspaces/0199a801-1000-7000-8000-000000000001/fleets
Content-Type: application/json

{"message":"Watch failed checkout deployments.","operation_id":"0199a801-1000-7000-8000-000000000002"}
```

The fleet object is shortened here; `Fleet` under Options lists every field.

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

The name is `fleet-` and the last 12 hex digits of the fleet id, because the body named none. Sending the same body with the same `operation_id` again answers 200 with the same fleet and `"replayed": true`.

## Options

### Shared wire rules

- JSON requests reject unknown fields. Omit absent optional success fields. Page envelopes always include `items`, `total:null`, and `next_cursor` (null on the last page).

- Money, message sequences and fences use decimal strings. Charges, costs, rates and a fleet's spend use unsigned `Nanos` decimal strings; wallet balances use `SignedNanos`, allowing negative debt (`af_wire`'s nanos module).

- Times are RFC 3339 instants in UTC at millisecond precision under REST guide §4 (`af_wire`'s instant type). Narrow caps and limits use safe JSON integers.

- Lists accept `limit` and `starting_after` using `af_core::paging::Cursor`. General lists default to 50 and allow at most 100.

- Fleet-message cursors carry position. The authorized workspace, fleet and `event_type` filter are reapplied on every read.

- An entity tag (ETag) is a strong opaque version. GET F returns its resource tag in `ETag` and `etag`. Compression may weaken the response header to `W/"value"`; the body keeps the strong `"value"` tag.

- Fleet clients accept that header only when its opaque value matches the strong body tag. Conditional writes send the strong body tag in `If-Match`.

- `POST W/fleets` and `POST F/messages` take their retry key only as the body `operation_id`. `PATCH` routes use `If-Match`, and Stop uses its monotone cutoff.

- The create's receipt is scoped `<workspace>:fleet:create:<person>`, so one key binds one workspace and one person. Authorization runs before replay.

- The same key, body and person get the first result back, with `replayed: true`. Another body under the key is 409 `UZ-SETUP-005`. A repeat after that fleet is deleted is 410 `UZ-SETUP-013`.

- The receipt answers its key for 24 hours from its creation by default. After that the key no longer prevents a second fleet, so read the fleet list before sending the create again.

### Fleet and common types

`Fleet = {id,name,status,etag,model_ref,http_egress,package_domains,schedules[],triggers[],grants[],memory_access,created_at,updated_at,events_processed,budget_used_nanos}`. The event count and the nanos are strings.

`status` is `active` or `archived`. `http_egress` is `restricted` or `public`, and `package_domains` is a boolean; both are off when the fleet is created ([the egress model](runner_fleet.md#egress-model-outbound-is-the-only-network-surface)). `memory_access` is `{read,publish}`.

`model_ref` is the body's `model_ref`, else the workspace's selected model, resolved at create and stored on the fleet. `budget_used_nanos` is what the fleet's runs have spent. No fleet has a budget: the payer's balance is the only ceiling ([billing](billing_and_provider_keys.md)).

**The fleet name.** The name is the body's `name`, else `fleet-` and the last 12 hex digits of the fleet id. A name is 1 to 64 bytes of lowercase ASCII letters, digits and hyphens, checked by `af_fleet_runtime`'s name module. Names are unique per workspace (`uq (workspace_id, name)`), and a taken name is 409 `UZ-AGT-006` with nothing written.

`FleetSummary = {id,name,status,created_at,updated_at,events_processed,budget_used_nanos}` is one row of `GET W/fleets`. The page reads it in the same statement as the counters (`afd_fleet_lifecycle`'s `select_fleet_page.sql`).

`?name=<name>` answers the one fleet with exactly that name in the workspace, archived included, or no item; `total` and `next_cursor` are `null`. A name outside the name rule is a 400. The dashboard's and the command line's `@name` resolve through it (`select_fleet_by_name.sql`).

`Grant = {id,kind,name,status,scope?,requested_scope?,approved_at?}` is one row of `connector.integration_grants`. `kind` says whether `name` is a static workspace secret (`secret`) or a mintable integration; the daemon derives it from the vault entry. `status` is `pending`, `approved` or `revoked`, and a deny is stored as `revoked`.

`scope` and `requested_scope` are set for GitHub only: the repositories, and read or write. `af_fleet_runtime`'s `config/repository.rs` parses them. `requested_scope` waits for Approve, and the approved `scope` stays in force until then ([where a grant comes from](connectors.md#where-a-grant-comes-from)).

`Trigger = {id,kind,source,events?,repositories?,channel?,url?,created_at}` is one row of `fleet.triggers`. `kind` is `webhook`, `app_event` or `mention`. A webhook's `url` is `/v1/webhooks/{fleet_id}/{source}` ([fleet triggers](connectors.md#fleet-triggers)).

`Schedule = {schedule_id,fleet_id,cron,timezone,message,source,once,status,sync,etag,description,run_state,next_run_at}`. `source` is `api` for a person's schedule and `fleet` for one the fleet added with `cron_add`. Each schedule is an independent row.

The last three fields are the server's reading at the request's instant (`afd_cron`'s run module). `description` is the expression in words from the scheduler's own cron parser, with its zone.

`run_state` is `scheduled|paused|activation_pending|next_run_unavailable`; `next_run_at` is set only when `run_state` is `scheduled`. Paused wins over sync state, and `deleting` or a `failed` sync never runs. `syncing` is activation pending, and a one-off runs at its stored instant or not at all.

Sync is `syncing|synced|failed` (`afd_cron`'s model); status is `active|paused|deleting`. A fleet with no schedule has an empty `schedules[]`.

**The text ceiling.** A first message and every later message are 1 to 1,048,576 characters, the one text ceiling `af_validate::TEXT_MAX_CHARS`. A longer one is refused `UZ-REQ-004`, reading "Input exceeds the maximum length of 1,048,576 characters."

A body carrying one text, such as a fleet message, may be 12,599,296 bytes (`TEXT_BODY_BYTES_MAX`). The daemon reads that much on those routes, and the app's proxies read the same (`afd_http/tests/fixtures/body_limits.json`).

A run carrying that text reaches its runner in one frame of at most 32 MiB (`af_wire::protocol::MESSAGE_BYTES_MAX`).

### Workspace routes

| # | Method and resource | Exact input / preconditions | Success output | Specific failures |
|---|---|---|---|---|
| W1 | GET W/models | No body; `purpose=runtime` optional, list paging; workspace authorization | 200 Page of `{ref,label,provider,model,context_window_tokens,max_output_tokens,eligibility:{runtime},pricing:{input_nanos_per_million,output_nanos_per_million,runtime_nanos_per_second}}`; the owner's eligible enabled entries only | AUTH, INPUT, CURSOR, DEPENDENCY |
| W2 | POST W/fleets | `{message,operation_id,model_ref?,name?}`; `fleet:write`; message within the text ceiling; body UUIDv7 key only | 201 `{fleet:Fleet,accepted:{turn_id,disposition:"started"|"queued"},replayed:false}`; a repeat answers 200 with `replayed:true` | INPUT, REPLAY, GONE, MODEL, NAME, AUTH, DEPENDENCY |
| W3 | GET W/fleets | No body; `fleet:read`; list paging; optional `?name=` | 200 Page of FleetSummary | INPUT, CURSOR, AUTH, DEPENDENCY |

W2 writes the fleet (`active`), its thread and its first turn (`queued`, actor `steer:<person>`) in one transaction. With no usable model, W2 answers 409 `UZ-SETUP-008` and writes no fleet, thread or turn. The fleet's answer arrives on `GET F/turns/stream`, which [the live tail](data_flow.md#d-watch--user-side-how-the-live-tail-surfaces) owns.

### Fleet routes

| # | Method and resource | Exact input / preconditions | Success output | Specific failures |
|---|---|---|---|---|
| F1 | GET F | No body; `fleet:read` | 200 Fleet and ETag | existing 404 `UZ-AGT-009`, AUTH, DEPENDENCY |
| F2 | PATCH F | Required If-Match resource tag; `{status?:"archived",name?,model_ref?,http_egress?,package_domains?}`; at least one; `http_egress` and `package_domains` need a person | 200 Fleet and ETag; a model change writes one `model_changed` turn | PRECONDITION, STALE, INPUT, MODEL, NAME, PERSON, AUTH, DEPENDENCY; an archived fleet returns 409 `UZ-AGT-020` with `current_state: archived` after write preconditions pass; a fleet that does not exist returns 404 `UZ-AGT-009` |
| F3 | POST F/messages | `{message,operation_id?}`; message within the text ceiling; operation_id 1..200 bytes when supplied; neither permits NUL; no header key | 202 `{status:"accepted",turn_id,replayed,disposition,folded_into_turn_id?}`; `disposition` is `started`, `folded` or `queued`, and `folded_into_turn_id` names the live run a folded message joined; Location F/messages; recovery through the existing message read and stream | INPUT (a message past the ceiling is `UZ-REQ-004`), REPLAY (`UZ-AGT-016`), lifecycle/state, AUTH, DEPENDENCY |
| F4 | GET F/messages | limit default 25, max 100, starting_after; optional event_type, 1..64 bytes without NUL; `fleet:read` | 200 message page, filtered before limit; `event_type=activity` excludes runtime bodies and queued turns | INPUT, CURSOR, AUTH, DEPENDENCY |
| F5 | GET F/integration-grants | No body; `grant:read` | 200 `{items:Grant[]}` | AUTH, DEPENDENCY |
| F6 | DELETE F/integration-grants/{id} | No body; `grant:write`; a person | 204; a live run keeps what it was issued, and the next run and the next token mint do not | PERSON, existing grant 404, AUTH, DEPENDENCY |
| F7 | GET F/triggers | No body; `fleet:read` | 200 `{items:Trigger[]}` | AUTH, DEPENDENCY |
| F8 | DELETE F/triggers/{id} | No body; `fleet:write` | 204; a delivery to its URL then answers 404 and writes no turn | AUTH, DEPENDENCY |
| F9 | PATCH F/durable_tasks/{id} | Required If-Match task tag; `{state,expected_generation}` or `{questions,expected_generation}`; a statement edit needs a person | 200; a statement edit writes new `questions` and `goal_digest` and raises the generation by 1 | stale 412 `UZ-DT-013`, PERSON, AUTH, DEPENDENCY |
| F10 | PATCH F/schedules/{schedule_id} | Required If-Match schedule tag; its own fields `{cron?,timezone?,message?,status?}`; an `override` body is 400 | 200 Schedule and ETag | PRECONDITION, STALE, INPUT, STATE, existing schedule 404 `UZ-SCHED-002`, DEPENDENCY |

The routes the spec keeps as they are also stand: `F/memories`, `W/secrets`, and `W/approvals` with its approve, deny and answer routes (`POST W/approvals/{gate_id}/approve|deny|answer`). Approve and deny need a person. On an access card they flip the grant and write the answer turn that resumes the asking run ([where a grant comes from](connectors.md#where-a-grant-comes-from)).

The existing GET F/messages includes queued steer turns alongside opened history in one statement snapshot (`afd_turns`'s thread-page statements). It keeps the queued-message shape, turn ID, ordering, cursor and authorization across opening and settlement.

A message that folded into a live run keeps its own turn. Once the run's report succeeds, that turn carries `resumes_turn_id`, naming the run's turn, and the run's answer as its reply.

F3 deduplicates on the turn's producer key in `thread.turns`. Reuse its key with the same message and sender; omitting it creates new work on every request.

A message sent while the fleet's run is live folds into that run (`disposition: folded`). The live run takes it at its next model call, and the run's answer is its reply. A task-bound message ahead of it is a barrier, so the message queues instead.

The browser and the terminal use the same routes, authorization, replay and billing rules. The terminal creates a fleet with W2 (`agentsfleet fleet create`) and then talks to it with F3 (`agentsfleet fleet steer`).

### Removed person routes

| # | Removed method and resource | Replacement |
|---|---|---|
| R1 | GET W/library-entries | None |
| R2 | DELETE W/library-entries/{entry_id} | None |
| R3 | /v1/admin/fleet-libraries and its members | None |
| R4 | W/fleet-libraries and its members, including /owned | None |
| R5 | W/plugins and its members (M253 removed the Plugin library) | None |

Removal is immediate; there is no alias. The following are absent:

- `/v1/operations` and public turn-status.

- Fleet creation/attempt resources.

- Deletion resources.

### Runner routes and wire

Runner route identities live in `afd_http`; path constants and native message types live in `af_wire`.
Each host opens one authenticated connection through `POST /v1/runners/me/stream` on the private listener.
The [runner guide](runner_protocol.md) owns connection setup, trust and recovery.

The daemon assigns fleet runs when the host advertises capacity, against the daemon-enforced worker limit.
Heartbeats, renewal batches, activity, folded inputs and reports use that connection.

Fleet renewal and report messages carry every provider usage record the daemon has not yet recorded, oldest first, with no count cap.
Each renewal answer includes its run's `recorded_through` acknowledgement.
The report carries the remaining records and uses its original run identity when retried.

The native `folds` request carries `fencing_token` and `after_sequence`. Its reply holds every pending input, oldest first, cut to one frame; the next fold returns the rest.
Only a successful report settles inputs through its `folded_through_sequence`.

`RunPayload` carries the fleet's `name` and no instructions; the runner writes the prompt from fixed text and the fleet's pinned memory ([capabilities](capabilities.md#1-what-the-model-is-told-and-what-the-run-holds)). When the runner is task-capable and the run carries a decision profile, an ordinary run's payload lists the fleet's unfinished tasks.

`NetworkPolicy` is built from the fleet's two toggles and the hosts of its approved secrets, and has no read-only mode. The turn checkpoint's scope names the turn and no fleet version, and the runner protocol has one `VERSION`.

A run loads and saves its thread's compacted context with `context_load` and `context_save`, under its fence.
A save is a compare-and-swap on `resource_version` over the thread's `thread.compactions` row.
A superseded run's save is refused with `UZ-RUN-005`, and the row stays as it was.

### Run verbs a fleet's tools call

Each verb is fenced to its run. The trigger verbs carry `item_id`, `call_number` and `fencing_token`, as the schedule verb (`POST L/schedules`) does.

| # | Verb | Tool | Input | Success output | Refusals |
|---|---|---|---|---|---|
| V1 | POST L/access | `request_access` | `{name,scope?}` | 201 `{grant_id,gate_id}`; 200 when an approved grant already covers it or a request is pending | 404 name not in the workspace vault or connected integrations; 409 integration not connected; a GitHub request with no scope, or one the parser refuses; a secret whose `host` `af_egress::allowlist_host` cannot read |
| V2 | POST L/triggers | `trigger_add` | `{kind:"webhook"|"app_event"|"mention",source?,events?,repositories?,verifier?,secret_name?,channel?}` | 201 Trigger | 409 provider not connected; 404 signing secret not in the workspace vault; 409 trigger exists; a webhook source of `approval` or `github`; a shape the kept trigger checks refuse |
| V3 | GET L/triggers | `trigger_list` | none | 200 `{items:Trigger[]}` | none beyond the fence |
| V4 | DELETE L/triggers/{id} | `trigger_remove` | none | 204 | none beyond the fence |
| V5 | POST L/durable_tasks/bindings | `task_start`, `task_input` | `{fencing_token,source_turn_id,plan_digest,items:[one]}`; a new-work item is `{title,source_start,source_end,target,statements[]}`, a steer item `{durable_task_id,source_start,source_end}` | 201 the bindings; a repeat answers 200 | `UZ-DT-004` a source turn that is not a person's message; `UZ-DT-005` the unfinished-task cap; `UZ-DT-003` a different plan for a turn that has one; `UZ-DT-001` a finished or foreign task; 400 a new item with no statement |

A refusal returns to the model as a tool result of the form `[kind] sentence`, and the run continues. V1 ends the run waiting when it raises a card, as a run's question does ([where a grant comes from](connectors.md#where-a-grant-comes-from)). V2 to V4 are owned by [fleet triggers](connectors.md#fleet-triggers), and V5 by [how a task starts](durable_tasks.md#how-a-task-starts).

V5 carries no decision call of its own and no fleet version. `POST L/durable_tasks/decisions` carries goal checks only, which judge whether a task is done ([goal evaluation](goal_evaluation.md#goal-evaluation)).

### Current task reads

The existing tenant plane serves these `FleetRead` routes under `/v1/workspaces/{workspace_id}/fleets/{fleet_id}`.
The [DT API target](durable_tasks.md#proposed-native-api) describes the remaining planned fields and mutations.

| Method and suffix | Current response and behavior |
|---|---|
| `GET /durable_tasks?starting_after=<cursor>&limit=20` | `{items,next_cursor}`; unfinished titles, states, versions and timestamps. Maximum page size 100; newest `(created_at,id)` first. Owned empty fleets return 200; missing or foreign scoped fleets return 404. |
| `GET /durable_tasks/{durable_task_id}` | `{title,resource_version,context}`; accepted objective, completion statements (`questions`), cumulative usage, limits and cursors. Strong ETag supports `If-None-Match` and empty 304; completed tasks remain readable. |

Collection responses use `private, no-store`; detail responses use `private, no-cache` with revalidation.
Dashboard proxies keep the resource tag and apply `no-store` without exposing bearer credentials to browser code.

Native reads run under one five-second client deadline that covers acquisition and commit (`afd_fleet`'s task fetch). The API login's statement bound applies beneath it ([concurrency](concurrency.md#every-daemon-statement-runs-under-the-api-logins-bounds)).

## Errors

Routine dependency and authorization failures and the named failure symbols apply to each table row. All failures use the existing `application/problem+json`: `{docs_uri,title,detail,error_code,request_id,user_message?}`. The extensions `current_state` on 409 and `etag` on 412 come from `afd_http`'s envelope.

The following codes name the route tables' error categories. Read the operation's response description for its precise refusal conditions; registration alone does not prove a live outcome.

| Symbol | Code | HTTP | Meaning / action |
|---|---|---|---|
| AUTH | Existing `UZ-AUTH-002` / `UZ-AUTH-022` | 401 / 403 | Invalid authentication or insufficient scope. |
| PERSON | Existing `UZ-AUTH-001` | 403 | An API key on a write that needs a signed-in person; nothing changes. |
| INPUT | Existing `UZ-REQ-001` / `UZ-UUIDV7-009` | 400 | Invalid body shape, key, bound or unknown field; path UUIDv7 validation uses `UZ-UUIDV7-009`. |
| STATE | `UZ-SETUP-002` | 409 | Wrong lifecycle state, such as a schedule being deleted; includes `current_state`. |
| STALE | `UZ-SETUP-003` | 412 | The `If-Match` version differs; includes the current `etag`. |
| PRECONDITION | `UZ-SETUP-004` | 428 | Required `If-Match` absent; read the resource before writing. |
| REPLAY | `UZ-SETUP-005` | 409 | A create key was accepted with another body or person; fleet messages keep `UZ-AGT-016`. |
| MODEL | `UZ-SETUP-008` | 409 | No usable model: the named model, or the workspace's selection when none is named, is missing, disabled or ineligible. No fallback; nothing is written. |
| CURSOR | `UZ-SETUP-011` | 400 | Cursor malformed or bound to another query or subject. |
| GONE | `UZ-SETUP-013` | 410 | The fleet a create key made was deleted; recreation is refused within the replay window. |
| NAME | `UZ-AGT-006` | 409 | The name is taken in the workspace; nothing is written. |
| TASK | `UZ-DT-001` / `UZ-DT-003` / `UZ-DT-004` / `UZ-DT-005` / `UZ-DT-013` | 404 / 409 / 409 / 429 / 412 | In order: a finished or foreign task, a second plan for one turn, a source turn that is not a person's message, the unfinished-task cap, a stale task tag. |
| DEPENDENCY | Existing `UZ-INTERNAL-001` | 503 | Required datastore unavailable; no guessed success. |

Four codes in `af_core`'s fleet error family serve the run verbs, each with its `REGISTRY` row and problem entry:

- a requested name not in the workspace (404);
- an integration or provider not connected (409);
- a trigger that already exists (409);
- a trigger's signing secret not in the workspace vault (404).

Oversized HTTP envelopes use the existing `UZ-REQ-002` with its registered 413. Validation detail names the offending field without echoing private message content.

## Related pages

- [What a fleet is](user_flow.md#81-what-a-fleet-is)
- [Fleet creation and recovery](data_flow.md#fleet-creation-and-recovery)
- [Where a grant comes from](connectors.md#where-a-grant-comes-from)
- [Fleet triggers](connectors.md#fleet-triggers)
- [How a task starts](durable_tasks.md#how-a-task-starts)
- [Runner connection and recovery](runner_protocol.md)
