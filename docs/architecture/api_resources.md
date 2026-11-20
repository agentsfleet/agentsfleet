---
type: reference
audience: contributor
verified: 2026-11-07
product_version: 0.75.0
executable: false
---

# API resources — workspace setup, fleets and runner

## Synopsis

This reference describes the implemented private setup and shared fleet APIs. The [verification ledger](shared_chat_verification.md#historical-verification-evidence) records observed checks and remaining live proof; implementation alone does not establish deployment success.

Prefixes: `W=/v1/workspaces/{workspace_id}`, `D=W/drafts/{draft_id}`, `F=W/fleets/{fleet_id}`. Workspace, draft, candidate, fleet, revision and run identifiers are UUIDv7.

Setup operation identifiers are UUIDv7; fleet-message keys follow C4. Revision identifiers are UUIDv7.

Person authorization uses workspace ownership resolved by the existing layer. Draft reads and writes additionally require the authenticated creator; workspace membership alone does not expose another person's draft.

Draft access requires `fleet:write`. Fleet reads and writes retain `fleet:read` and `fleet:write`. Fleet deletion retains `fleet:admin` in every lifecycle state.

Revalidate current access on every read, write, stream reconnect, and setup step.

## Example with output

The example omits authorization headers. The model reference is illustrative and must resolve through GET W/models.

```http
POST /v1/workspaces/0199a801-1000-7000-8000-000000000001/drafts
Content-Type: application/json

{"source":{"kind":"blank"},"setup_model_ref":"owner-model-1","operation_id":"0199a801-1000-7000-8000-000000000002","message":"Watch failed checkout deployments."}
```

```json
{
  "id":"0199a801-1000-7000-8000-000000000003",
  "state":"editing",
  "etag":"\"draft:0199a801-1000-7000-8000-000000000003:1\"",
  "setup_model_ref":"owner-model-1",
  "accepted":{"status":"accepted","sequence":"1","replayed":false}
}
```

A candidate/review is absent until there is a validated candidate. A queued message does not make the definition usable immediately.

## Options

### Shared wire rules

- JSON requests reject unknown fields. Omit absent optional success fields. Page envelopes always include `items`, `total:null`, and `next_cursor` (null on the last page).

- Money, message sequences, candidate revisions, setup revisions, and fences use decimal strings. Charges, costs, rates, and budgets use unsigned `Nanos` decimal strings; wallet balances use `SignedNanos`, allowing negative debt (`af_wire`'s nanos module).

- Times are RFC 3339 instants in UTC at millisecond precision under REST guide §4 (`af_wire`'s instant type). Narrow caps and limits use safe JSON integers.

- Lists accept `limit` and `starting_after` using `af_core::paging::Cursor`. General lists default 50/max 100; draft messages default 20/max 25.

- Private draft cursors bind resource, creator, ordering, filters, and limit. Fleet-message cursors carry position; the authorized workspace, fleet, and `event_type` filter are reapplied on every read.

- An entity tag (ETag) is a strong opaque version. GET D/F returns its resource tag in `ETag` and `etag`. Compression may weaken the response header to `W/"value"`; the body retains the strong `"value"` tag.

- Fleet clients accept that header only when its opaque value matches the strong body tag. Conditional writes retain the strong body tag.

- GET F additionally exposes `setup_etag`. Fleet revision writes use the setup tag; status/name PATCH uses the fleet resource tag. A creation retry may advance progress but preserves identity and original accepted inputs.

- Draft creation and both conversation sends use **only** body `operation_id`; reject an `Idempotency-Key` header on those routes. Setup creation and fleet revision application use **only** `Idempotency-Key` (UUIDv7); reject body replay keys there. PATCH uses `If-Match`; Stop uses its monotone cutoff.

- Setup creation keys bind actor, workspace, operation type and parent collection. Member keys bind workspace, operation type and resource, with the accepting actor recorded. Authorization runs before replay.

- The same setup key, normalized body and actor recover the accepted result. Changed body or member-resource actor conflicts. Protection lasts 24 hours from receipt creation; deletion within that window returns 410.

- After expiry, inspect the draft or fleet before another write. The old setup key no longer prevents duplication.

### Definition, candidate, review, and common types

`Definition` is exactly `{schema_version:1,description:string,instructions_md:string,config:ConfigDocument}` from [the definition page](fleet_definitions.md#validated-definition). `ConfigDocument` preserves the existing typed `name`/`x-agentsfleet` shape, changing budget fields to decimal-string `daily_nanos`/`monthly_nanos`. Runtime model is `config["x-agentsfleet"].model`.

Required daily ceiling is `1..1000000000000` nanos; optional monthly ceiling is `1..10000000000000`; omit monthly for no monthly ceiling. Zero, fractions, signs, exponent notation, and overflow are rejected. These preserve the positive ceiling and cap semantics in `af_fleet_runtime`'s config policy.

`Source` is a tagged union:

```text
{kind:"blank"}
{kind:"fleet",fleet_id,setup_revision}
{kind:"definition",definition:Definition}
```

Resolve and authorize the source before allocating a row. A fleet source pins an immutable Definition.

The typed definition source accepts native JSON, including the CLI's `--definition <PATH>` input and the browser drawer. Repository and template-bundle imports remain unsupported.

`Candidate = {id,revision,definition,canonical_digest,policy_semantics}`. Every accepted definition change gets a fresh immutable candidate ID and the next draft-local decimal revision. Initialization starts at `"1"`; unchanged metadata or model selection creates no candidate revision.

Before the first proposal, a blank draft has no candidate. First drawer mutation creates a draft from an inline typed definition using shared explicit defaults displayed in the drawer. Empty instructions are allowed while editing but block Save; review readiness includes a definition check.

`Review = {effective,requirements,readiness[],review_digest,policy_semantics,observed_at}`:

```text
effective = {definition,instance_name,runtime_model_ref,source_revision?,target_setup_revision?}
requirements = {credentials[],repositories[],tools[],network?,approval_rules?,budget}
readiness item = {kind:"definition"|"model"|"wallet"|"credential"|"connection"|"grant"|"policy",
                  subject,state:"ready"|"missing"|"blocked",observed_at,next_action?}
next_action = {kind:"account_models"|"billing"|"workspace_connections"|"approvals"|"review",
               label,href}
```

Requirements are derived immutable setup, not readiness results. Review digest excludes observation timestamps and includes candidate, selections, source/target revisions, policy marker, and effective access/model resolution versions.

Effective setup changes →409 plus fresh `review_digest`; missing live checks instead appear in `installing` setup progress.

Save and Apply accept optional `origin` display metadata, with values `cli`, `dashboard` or `chat`. The field grants no access. The first accepted revision records the origin in its immutable `setup_saved` activity.

Retries exclude `origin` when comparing request bodies and retain the original receipt. Changing only the origin with the same operation key creates no additional fleet, revision or receipt. Older clients may omit the field; older receipts retain their generic saved-version label.

`Draft = {id,state,title,etag,setup_model_ref,candidate?,review?,linked_fleet_id?,closed_at?,expires_at?}`. States: `editing|saving|closed`; message state is separate.

Closing sets expiry 30 days later; a linked active setup blocks expiry. Cancelled setup returns `editing`, removing close/expiry fields. A queued or running setup turn blocks PATCH and Save.

`Message = {sequence,operation_id,role:"user"|"assistant",state,text?,candidate_id?,usage?,failure?,save_requested,save_error_code?,created_at,finished_at?}`. User sequence names its request; answer shares that sequence. States `queued|running|completed|cancelled|failed|timed_out`.

Usage is `{knowledge:"known"|"unknown",input_tokens?,output_tokens?,provider_cost_nanos?,runtime_cost_nanos?}`; unknown figures are absent, never zero estimates. Internal run IDs are not public turn resources.

Create and Send accept an optional `save` boolean, defaulting to false. The accepted message records `save_requested`; replay includes this choice. Text alone grants no save authority.

For a completed save-requested turn, the daemon saves the answer, accounting, and immutable setup together. Closing the browser does not cancel that save. Activation follows the commit and is retried on report replay.

A checked save refusal retains the answer and editable candidate with a stable `save_error_code`. An internal save failure rolls back the report transaction. Cancelled or failed turns never save setup.

`failure={reason,error_code,recovery_action}` is required on failed/timed_out messages and absent otherwise.
Clients must not display provider bodies.

The daemon records terminal state atomically and returns the same bounded fields after reload.
Reports cannot supply diagnostic text, URLs, credentials, or actions.

| Failure reason | Stable code | Recovery action |
|---|---|---|
| `insufficient_balance` | `UZ-SETUP-007` | `owner_billing` |
| `model_unavailable` | `UZ-SETUP-008` | `choose_setup_model` |
| `provider_failed` | `UZ-SETUP-014` | `retry_message` |
| `provider_usage_unknown` | `UZ-SETUP-015` | `retry_message` |
| `frame_too_large` | `UZ-RUN-032` | `reduce_context` |

A failed message no longer holds the draft; only a live setup run does. `provider_usage_unknown` marks a turn whose runs all lapsed, and it reports `retry_message`.

Stop, delete and revocation fence text writes; revoked callers receive no private failure detail.

Exact accepted replay is checked first. Retrying uses an explicit new message and key.

`Setup = {state:"checking"|"waiting"|"ready"|"cancelled",checks[]}`. Check = `{kind,subject,state:"pending"|"ready"|"blocked",next_action?}`. Check kinds: `source|policy|model|wallet|credentials|connections|grants|stream|activation`.

Grant checks record actual approved access; missing approval cannot be logged as ready.

`Start` request is `{kind:"none"}` or `{kind:"run_now",message}` (message 1..1,048,576 characters). Result is `{kind:"none"}` or `{kind:"run_now",state:"pending"|"accepted"|"blocked",turn_id?,error_code?}`. Derive the initial turn's one identity from creation, not another client key.

`start.accepted` requires confirmation of the committed initial turn with the expected turn identity (`afd_setup`'s start confirm). Ordinary fleet-message acceptance also requires a committed turn. Wake notifications are best effort.

Neither acceptance nor a wake notification means completed work.

`Schedule = {schedule_id,fleet_id,cron,timezone,message,source,once,status,sync,etag,override?,description,run_state,next_run_at}`. The last three are the server's reading at the request's instant (`afd_cron`'s run module). `description` is the expression in words from the scheduler's own cron parser, with its zone.

`run_state` is `scheduled|paused|activation_pending|next_run_unavailable`; `next_run_at` is set only when `run_state` is `scheduled`. Paused wins over sync state, and `deleting` or a `failed` sync never runs. `syncing` is activation pending, and a one-off runs at its stored instant or not at all.

A fleet with no schedule has an empty `schedules[]`.

Sync is `syncing|synced|failed` (`afd_cron`'s model); status is `active|paused|deleting`. Override is `{cron?,timezone?,message?,status?}`. Omitted override means inherited recipe; a null override is request-only Restore.

`Fleet = {id,name,status,etag,setup_etag,setup_revision,policy_semantics,definition,setup,start,schedules[],webhook_urls[],created_at,updated_at,events_processed,budget_used_nanos}`; event count and nanos are strings. Status remains the existing lifecycle vocabulary, including `installing|active|paused|stopped|killed`.

GET reports current prerequisites in `setup.checks`; changes to a saved review digest do not require another approval. Private transcripts stay with their creator.

`FleetSummary = {id,name,status,description,triggers?,created_at,updated_at,events_processed,budget_used_nanos}` is one row of `GET /v1/workspaces/{workspace_id}/fleets`. `description` is the head revision's definition `description`, the saved purpose; it is `null` while the fleet has no saved setup. The page reads it in the same statement as the counters (`afd_fleet_lifecycle`'s `select_fleet_page.sql`).

`?name=<saved name>` answers the one fleet saved under exactly that name in the workspace, archived included, or no item; `total` and `next_cursor` are `null`. A name outside 1 to 64 lower-case letters, digits or hyphens is a 400. The dashboard's and the command line's `@name` resolve through it (`select_fleet_by_name.sql`).

`Fleet.name` and creation `name` retain the existing validated fleet slug. `Draft.title` is a separate human display title, 1..100 characters; it does not change authored slugs. Review exposes both without substituting a display title into definition metadata or runtime config.

An idle `instance_name` draft selection chooses the validated instance slug independently of the recipe. It changes the review digest without creating a new candidate revision. An unresolved account default has a null `effective.runtime_model_ref` and a missing model readiness check.

`ConfigOp = {op:"add"|"replace",path,value} | {op:"remove",path}` permits `/config` descendants and replacement of `/description`. Identity, manifest and receipt fields are unwritable.

`InstructionEdit = {start_byte,end_byte,text}` uses UTF-8 byte offsets against the exact candidate. A batch allows 64 configuration operations and 32 instruction edits, with 1,048,576 replacement characters total. Invalid paths, overlapping ranges, invalid boundaries or invalid final setup reject the entire batch.

The serialized Definition or edit batch is bounded at 6,400 KiB. A setup HTTP body may hold that Definition and a first message at the text ceiling. Each is counted at its widest JSON escape, so the body may be 12,861,440 bytes (`SETUP_BODY_BYTES_MAX`).

A body carrying one text, such as a fleet message, may be 6,307,840 bytes (`TEXT_BODY_BYTES_MAX`). The daemon reads that much on those routes, and the app's proxies read the same (`afd_http/tests/fixtures/body_limits.json`).

A run carrying that text reaches its runner in one frame of at most 32 MiB (`af_wire::protocol::MESSAGE_BYTES_MAX`). See [native and model edits](fleet_definitions.md#small-atomic-edits) for conversion and correction behavior.

The next two tables list the workspace-chat person routes that remain after M253 removed the Plugin library. Subsequent fleet editor and history reads are listed separately below.

### New person routes: 11

| # | Method and resource | Exact input / preconditions | Success output | Specific failures |
|---|---|---|---|---|
| N1 | GET W/models | No body; `purpose=setup|runtime` optional, list paging; workspace authorization | 200 Page of `{ref,label,provider,model,context_window_tokens,max_output_tokens,eligibility:{setup,runtime},pricing:{input_nanos_per_million,output_nanos_per_million,runtime_nanos_per_second}}`; owner's eligible enabled entries only | AUTH, INPUT, DEPENDENCY |
| N2 | POST W/drafts | `{source:Source,setup_model_ref,message?,save?,operation_id}`; `save:true` requires a message; body UUIDv7 key only. The chat's initial `title` is the first line of `message`, else of the source's description, else `New fleet`, cut to 100 characters; a later title edit is never overwritten | 201 Draft, optional `accepted:{status:"accepted",sequence,replayed}` when first message exists; `Location:D`, ETag | INPUT, HIDDEN, MODEL, REPLAY, GONE, DEPENDENCY |
| N3 | GET W/drafts | No body; `state`, `linked_fleet_id` and `q` optional, paging; creator only. `q` (1..200 characters, trimmed) matches the title or any turn's text as a case-insensitive substring, LIKE metacharacters literal, and is bound into the cursor | 200 Page of `{id,title,state,etag,linked_fleet_id?,updated_at,expires_at?,candidate_revision,setup_revision}`; each revision is null until the chat reaches it (no candidate, no saved fleet) | INPUT, CURSOR, AUTH, DEPENDENCY |
| N4 | GET D | No body; creator + current membership | 200 Draft with latest candidate/review when available; ETag | HIDDEN, AUTH, DEPENDENCY |
| N5 | PATCH D | If-Match; `{title?,instance_name?,setup_model_ref?,runtime_model_ref?,config_ops?:ConfigOp[],instruction_edits?:InstructionEdit[]}`; at least one, editing and no running/queued messages | 200 Draft with new tag and current candidate/review; recheck owner eligibility; the setup model selection applies to future sends only | PRECONDITION, STALE, STATE, INPUT, MODEL, POLICY |
| N6 | DELETE D | No body; creator only, no If-Match | 204 after fencing and private-text deletion boundary; ongoing setup link may remain shared | HIDDEN, AUTH, DEPENDENCY; residual-copy bounds are in runner page |
| N7 | POST D/messages | `{message,save?,operation_id}`; text 1..1,048,576 characters; body UUIDv7 only; `editing`; omitted `save` is false | 202 `{status:"accepted",sequence,replayed}`; `Location:D/messages`; queued answer not implied | INPUT, STATE, REPLAY, GONE, HIDDEN, UNKNOWN, DEPENDENCY |
| N8 | GET D/messages | No body; creator; limit default 20/max 25, starting_after | 200 Page of Message; ordered sequence then role; states/usage included | HIDDEN, CURSOR, INPUT, DEPENDENCY |
| N9 | PATCH D/turns/{turn_id} | `{"status":"cancelled"}`; no If-Match | A queued turn never runs, and a running turn's setup run ends; later turns stay queued. Repeating the call answers the same state | INPUT, HIDDEN, STATE (a finished turn), DEPENDENCY |
| N10 | GET D/turns/stream | Creator; `Last-Event-ID` optional ephemeral hint; no body | 200 `text/event-stream`; typed progress described below; reconnect reconciles GET D/messages and GET D | HIDDEN, AUTH, DEPENDENCY |
| N11 | POST F/revisions | If-Match setup tag + Idempotency-Key; `{draft_candidate_id,review_digest,origin?}`; own authorized idle improvement draft targets this fleet | 201 `{fleet_id,setup_revision,setup_etag,etag,review_digest,schedules[]}`; Location F, ETag resource tag | PRECONDITION, STALE, REVIEW, POLICY, REPLAY, HIDDEN, STATE |

GET D has a complete review only when a candidate exists. Queue acceptance can succeed before the wallet check: balance/model failure becomes a visible failed message rather than silent queued work. Draft creation with a first message atomically records both or neither.

Send atomically captures the draft's selected setup model reference. Earlier turns keep their own. Idle PATCH can replace a disabled model and survives reload.

Title is a human display name, independent of validated definition slugs. Runtime/config changes invalidate setup review; a setup-model-only change leaves its definition/digest unchanged.

### Changed person routes: 5

| # | Method and resource | Exact target input | Target success output | Specific failures |
|---|---|---|---|---|
| C1 | POST W/fleets | Idempotency-Key; `{source:{kind:"draft",draft_candidate_id},name,review_digest,start:Start,origin?}`; draft must be creator-owned/idle | 201 Fleet with `status:active|installing`, setup checks, start outcome, schedules; Location F and ETag; key retry resumes same identity | REPLAY, GONE, REVIEW, HIDDEN, INPUT, POLICY, DEPENDENCY |
| C2 | GET F | No body; fleet:read | 200 Fleet including current setup/start and both resource/setup tags | existing fleet404 `UZ-AGT-009`, AUTH, DEPENDENCY |
| C3 | PATCH F | Required If-Match resource tag; `{status?,name?}` only; at least one. `active` on installing retries checks, `archived` cancels setup and live work | 200 Fleet and ETag; cancelled linked draft returns editing | PRECONDITION, STALE, STATE, INPUT, AUTH, DEPENDENCY; an archived fleet returns 409 `UZ-AGT-020` with `current_state: archived` after write preconditions pass; a fleet that does not exist returns 404 `UZ-AGT-009` |
| C4 | POST F/messages | `{message,operation_id?}`; message 1..1,048,576 characters; operation_id 1..200 bytes when supplied; neither permits NUL; no header key | 202 `{status:"accepted",turn_id,replayed,disposition,folded_into_turn_id?}`; `disposition` is `started`, `folded` or `queued`, and `folded_into_turn_id` names the live run a folded message joined; Location F/messages; recovery through existing message read/stream | INPUT (a message past the ceiling is `UZ-REQ-004`), REPLAY (`UZ-AGT-016`), lifecycle/state, AUTH, DEPENDENCY |
| C5 | PATCH F/schedules/{schedule_id} | Required If-Match schedule tag; `{override:ScheduleOverride|null}` for inherited recipe; independent schedule uses `{cron?,timezone?,message?,status?}` and cannot Restore | 200 Schedule and ETag; tag covers recipe revision + instance setup revision + override revision; null restores exact inherited setup | PRECONDITION, STALE, INPUT, STATE, existing schedule404 `UZ-SCHED-002`, DEPENDENCY |

### Fleet editor and history reads

| # | Method and resource | Exact input / preconditions | Success output | Specific failures |
|---|---|---|---|---|
| E1 | GET F/setup-context | No body; fleet:write; optional starting_after for older creator conversations | 200 `{fleet,conversations[],activities:{items,total:null,next_cursor},next_cursor}`; private, no-store | HIDDEN, AUTH, INPUT, CURSOR, DEPENDENCY |
| E2 | GET F/messages | Existing paging; optional event_type, 1..64 bytes without NUL; fleet:read | 200 existing message page, filtered before limit; `event_type=activity` excludes runtime bodies and queued turns | INPUT, CURSOR, AUTH, DEPENDENCY |

E1 returns at most three creator-private conversations, each with at most 25 recent message rows, plus 25 shared activity receipts. Runtime request bodies are excluded.

The complete serialized response has a four-mebibyte ceiling. Older whole turns are trimmed first; older conversations move behind their continuation if metadata alone needs more space. The newest conversation keeps its current setup and latest turn.

Conversation cursors bind creator, workspace, fleet, and ordering. Each conversation includes `draft`, `messages`, `messages_starting_after`, and `truncated`. Reuse its message boundary with N8 and `limit=25`; read N8 without a cursor for older text.

The fleet, conversations, and receipts are bounded independent reads, rather than one database snapshot. Every private conversation checks current membership and creator ownership.

The existing GET F/messages includes queued steer turns alongside opened history in one statement snapshot (`afd_turns`'s thread-page statements). It preserves the queued-message shape, turn ID, ordering, cursor, and authorization across opening and settlement.

A message that folded into a live run keeps its own turn. Once the run's report succeeds, that turn carries `resumes_turn_id`, naming the run's turn, and the run's answer as its reply.

This is an internal read change, not another route. Retry after an uncertain acceptance reuses the original operation ID.

C1 accepts a reviewed private candidate. A selected Slack trigger must be part of the reviewed setup. Changed install name/model choices open a private draft before Save.

No hidden trigger mutation or preview route is retained.

C4 deduplicates on the turn's producer key in `thread.turns`, separately from setup replay receipts. Reuse its key with the same message and sender; omission creates new work on every request.

A message sent while the fleet's run is live folds into that run (`disposition: folded`). The live run folds it in at its next model call, and the run's answer is its reply. A parked (approval-gated) or task-bound message ahead of it is a barrier, so the message queues instead.

Native JSON can initialize a draft; GitHub/template-bundle import and Markdown/bundle export remain unsupported.

Source-repository fields, upload/import/re-import routes, bundle delivery and retired bundle flags are removed with their dedicated implementations. GitHub runtime triggers and repository tools remain independent capabilities.

The terminal client starts or resumes a private draft, sends text through N7, follows N8/N10 and reviews N4. It explicitly invokes C1 to Save.

Browser and terminal use the same authorization, replay, validation, billing and readiness rules. Terminal conversation text is the setup input; scripts may submit text but cannot skip review or grant checks.

### Removed person routes

| # | Removed method and resource | Replacement |
|---|---|---|
| R1 | GET W/library-entries | None |
| R2 | DELETE W/library-entries/{entry_id} | None |
| R3 | /v1/admin/fleet-libraries and its members | None |
| R4 | W/fleet-libraries and its members, including /owned | None |
| R5 | W/plugins and its members (M253 removed the Plugin library) | None |
| R6 | POST D/cancellations | PATCH D/turns/{turn_id} |

Removal is immediate; there is no alias. The following are absent:

- `/v1/operations` and public turn-status.

- Fleet creation/attempt resources.

- Draft revisions/turns collections and preview POST.

- Deletion resources and definition-update resources.

- Schedule-overrides resources and fleet revision reads.

### Private Server-Sent Events (SSE)

Durable notices contain `{draft_id,sequence?,candidate_id?,state?,usage?,failure?}`.
Their event names are `message.accepted|message.running|message.completed|message.cancelled|message.failed|message.timed_out|candidate.changed|draft.closed|draft.deleted`.
No private content enters workspace or operator streams.

Preview `message.delta` frames contain `{draft_id,sequence,stream_id,stream_seq,stream_start,text_kind:"answer",text}`.
`sequence` is a decimal string; `stream_seq` counts within one run, and a reassigned turn starts a new stream.
Answer text is at most 4,096 UTF-8 bytes; the serialized payload is bounded at 8,192 bytes.

Reasoning and tool arguments never enter the private preview.
`draft.resync` carries `{}` after a subscription gap and requests a durable read, without another model request.

Before each frame, recheck current membership, deletion generation, the run and running message state.
Quiet streams check each second; each access read has a ten-second deadline.
Revocation closes the stream; deletion emits only identity and closes.

Already rendered text cannot be recalled.
Final GET messages remains authoritative; previews never prove completion or usage.

### Runner routes and wire

Runner route identities live in `afd_http`; path constants and native message types live in `af_wire`.
Each host opens one authenticated connection through `POST /v1/runners/me/stream` on the private listener.
The [runner guide](runner_protocol.md) owns connection setup, trust and recovery.

The daemon assigns fleet and setup runs when the host advertises capacity.
Both kinds count against the same daemon-enforced worker limit.
Heartbeats, renewal batches, activity, folded inputs and reports use that connection.

Fleet renewal and report messages carry at most 512 provider usage records (`af_wire::usage::LINES_MAX`).
Each renewal answer includes its run's `recorded_through` acknowledgement.
The report carries the remaining records and uses its original run identity when retried.

The native `folds` request carries `fencing_token` and `after_sequence`, returning at most 32 inputs (`FOLDS_PER_ASSIGNMENT_MAX`).
Only a successful report settles inputs through its `folded_through_sequence`.

A setup turn rides the same connection as a fleet turn; [setup runs](#setup-runs) below covers its payload and report.

### Setup runs

A setup turn is a run its draft owns, assigned to a runner like a fleet turn. The daemon makes no model call; the runner makes every one.
The [runner guide](runner_protocol.md#setup-operations-on-the-native-session) owns renewal, fencing and report delivery.

The run payload's `owner` is `{kind:"draft",draft_id}`, and its `setup` part is `SetupWork`.
The message rides `event.request_json`, and earlier answered turns ride `history`.

`SetupWork={sequence,candidate?:{id,revision,definition},save_requested,tool,history_sequences?}`.
`tool` is the `propose_setup` definition, and its description is also the run's instructions.

The run's policy offers one tool, `propose_setup`, and the runner prepares no sandbox for it.
The run carries the pinned model's key, as a fleet run does, and no runtime credentials, memory or repository.
A tenant's runner is never assigned a turn on the platform model, whose key the run would carry.

Assignment checks that the pinned model takes tool calls and that the payer's balance covers one run.
Nothing is reserved; renewals and the report log the run's usage lines.
A refused model or balance fails the message as `model_unavailable` or `insufficient_balance`.

A setup run's maximum runtime is 300 seconds (`afd_setup::limits::SETUP_RUNTIME`).
A lapsed run is reassigned at the next fence, and the message reruns from its start.
A turn gets at most three runs (`af_core::limits::EVENT_RUNS_MAX`); when all three lapse, the message fails as `provider_usage_unknown`.

`propose_setup` takes `{kind:"initialize",definition}` before a candidate exists.
After that it takes `{kind:"edit",base_candidate_id,base_candidate_revision,config_ops,instruction_edits}`.
Each instruction edit adds text or replaces exact text, never a byte offset.

The runner applies each call in memory with `af_fleet_runtime::setup`'s rules, which the daemon applies again at report.
A refused call returns `[kind] sentence` to the model as the tool result, and the model may try again in the same run.
The proposal loop has no cap of its own; the 300-second runtime bounds it.

The refusal kinds are `invalid_arguments`, `instructions_missing`, `instructions_ambiguous`, `stale_candidate` and `edit_not_requested`.
A later edit applies to the setup the run last kept, on the same base.
A message that asked to save proposes nothing; every call is refused as `edit_not_requested`.

The report carries the answer, the usage and `setup:{proposal?:{base_candidate_id?,base_revision,definition}}`.
`definition` is the final Definition as JSON text, at most 6,553,600 bytes (`SETUP_DEFINITION_BYTES_MAX`). That is `af_validate::DEFINITION_MAX_BYTES`, the bound the Definition parser applies.
A text-only answer carries no proposal, and a save turn's report saves the head candidate.

The report's transaction locks the draft, settles the run, then checks the proposal again.
The base must still name the head candidate and its revision, and `Definition::parse` must accept the Definition.
A proposal that passes becomes candidate revision N+1.

A refused proposal writes no candidate, and the turn still completes with its answer.
The turn keeps the refusal's sentence as its failure detail, and the daemon logs `setup_proposal_refused` with `error_code`, `draft_id` and `run_id`.
A moved base refuses with `UZ-SETUP-003`; a Definition the parser refuses keeps the parser's code.

A report below the run's current fence is refused with `UZ-RUN-005` and writes nothing.
A replayed report finds its run settled and writes nothing a second time.

The run loads and saves the draft's compacted context with `context_load` and `context_save`, under its fence.
A save is a compare-and-swap on `resource_version` over the draft's `memory.context_checkpoints` row.
A superseded run's save is refused with `UZ-RUN-005`, and the row stays as it was.

The runner streams answer chunks on the run's activity verb, as a fleet run does.
The daemon rechecks the run, its message and the creator's access. It then republishes each chunk as a `message.delta` frame of at most 4,096 bytes.

Compaction progress reaches the same private stream as `context.progress`.

### Current task reads

The existing tenant plane serves these `FleetRead` routes under `/v1/workspaces/{workspace_id}/fleets/{fleet_id}`.
The [DT API target](durable_tasks.md#proposed-native-api) describes the remaining planned fields and mutations.

| Method and suffix | Current response and behavior |
|---|---|
| `GET /tasks?starting_after=<cursor>&limit=20` | `{items,next_cursor}`; unfinished titles, states, versions, accepted revision identities and timestamps. Maximum page size 100; newest `(created_at,id)` first. Owned empty fleets return 200; missing or foreign scoped fleets return 404. |
| `GET /tasks/{task_id}` | `{title,resource_version,setup_revision,context}`; accepted objective, questions, cumulative usage, limits and cursors. Strong ETag supports `If-None-Match` and empty 304; completed tasks remain readable. |

Collection responses use `private, no-store`; detail responses use `private, no-cache` with revalidation.
Dashboard proxies retain the resource tag and apply `no-store` without exposing bearer credentials to browser code.

Native reads run under one five-second client deadline that covers acquisition and commit (`afd_fleet`'s task fetch). The API login's statement bound applies beneath it ([concurrency](concurrency.md#every-daemon-statement-runs-under-the-api-logins-bounds)).

## Errors

Routine dependency/auth failures and the named specific failure symbols apply to each table row. All failures use existing `application/problem+json`: `{docs_uri,title,detail,error_code,request_id,user_message?}`. Existing extensions `current_state` on409 and `etag` on412 come from `afd_http`'s envelope.

Add `review_digest` only on a stale-review409. Do not invent nested errors or expose operator hints.

The following codes name the route tables' error categories. Read the operation's response description for its precise refusal conditions; registration alone does not prove a live outcome.

| Symbol | Code | HTTP | Meaning / action |
|---|---|---|---|
| AUTH | Existing `UZ-AUTH-002` / `UZ-AUTH-022` | 401 /403 | Invalid authentication / insufficient scope; creator mismatch is HIDDEN. |
| INPUT | Existing `UZ-REQ-001` / `UZ-UUIDV7-009` | 400 | Invalid body shape/key/bound/unknown field; existing path UUIDv7 validation uses UZ-UUIDV7-009. |
| HIDDEN | `UZ-SETUP-001` | 404 | Missing, foreign, creator-private, or non-writable platform resource; do not distinguish. |
| STATE | `UZ-SETUP-002` | 409 | Wrong lifecycle state; include current_state. |
| STALE | `UZ-SETUP-003` | 412 | Version differs; include current etag. |
| PRECONDITION | `UZ-SETUP-004` | 428 | Required If-Match absent; refetch before writing. |
| REPLAY | `UZ-SETUP-005` | 409 | Key was accepted with a different normalized body/actor; fleet messages keep UZ-AGT-016. |
| REVIEW | `UZ-SETUP-006` | 409 | Effective setup changed; include fresh review_digest and refetch review. |
| BALANCE | `UZ-SETUP-007` | 402 | Owner wallet fails the bounded setup billing check; queued message records this outcome. |
| MODEL | `UZ-SETUP-008` | 409 | Selected owner model missing, disabled, or ineligible; no fallback. |
| POLICY | `UZ-SETUP-009` | 409 | Marker or supported setup differs; requires new review. |
| TOKEN | `UZ-SETUP-010` | 422 | The context cannot fit the model's window; the message fails. |
| CURSOR | `UZ-SETUP-011` | 400 | Cursor malformed or bound to another query/subject. |
| GONE | `UZ-SETUP-013` | 410 | Accepted resource was purged; recreation is refused within the original 24-hour replay window. |
| PROVIDER | `UZ-SETUP-014` | 502 | Provider failed with known usage; store bounded failure and permit explicit retry. |
| UNKNOWN | `UZ-SETUP-015` | 409 | The message's usage stayed unknown after its runs lapsed; send it again. |
| DEPENDENCY | Existing `UZ-INTERNAL-001` | 503 | Required datastore unavailable; no guessed success. |

Oversized HTTP envelopes use existing `UZ-REQ-002` with its registered413. Validation detail names the offending field without echoing private message content. Provider failures become message state and accounting knowledge, not a fabricated completion.

## Related pages

- [API gaps](../reviews/workspace-setup/api.md)
- [Definition](fleet_definitions.md#validated-definition)
- [Creation barrier](data_flow.md#fleet-creation-and-setup-recovery)
- [Runner behavior](runner_fleet.md#setup-runs)
- [Public change handoff](../reviews/workspace-setup/public-changes.md)
