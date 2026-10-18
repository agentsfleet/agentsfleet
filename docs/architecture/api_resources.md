---
type: reference
audience: contributor
verified: 2026-10-18
product_version: 0.62.1
executable: false
---

# API resources — workspace setup, fleets, Library and runner

## Synopsis

This common reference specifies the active setup and runtime cutover. This is the target reference. The [focused check ledger](../reviews/workspace-setup/checks.md) distinguishes served draft/message/activity/model-call behavior from pending Review, stream, admission, renewal, settlement, Save and Library flows. Repository and browser acceptance proofs remain pending. Exactly 17 person routes are new, 7 change, and 2 are removed. [Source findings](../reviews/workspace-setup/source-findings.md) identify existing behavior; a new route has no invented source-file reference. [Owner decisions](../reviews/workspace-setup/decisions.md) govern all choices here.

Prefixes: `W=/v1/workspaces/{workspace_id}`, `D=W/drafts/{draft_id}`, `F=W/fleets/{fleet_id}`, `G=W/fleet-libraries/{library_id}`. Workspace, draft, candidate, fleet, revision, lease, and operation identifiers are UUIDv7. Library identifiers are opaque: platform slugs and owned UUIDs are both valid; literal `owned` is mounted before a member path.

Person authorization uses workspace ownership resolved by the existing layer. Draft reads and writes additionally require the authenticated creator; workspace membership alone does not expose another person's draft. Draft access requires `fleet:write`; Library reads require `fleet:read`, writes/deletes require `library:write`; fleet reads and writes retain `fleet:read`/`fleet:write`. Fleet deletion retains `fleet:admin` and the killed-state rule. Revalidate current access on every read, write, stream reconnect, and setup step.

## Example with output

The example omits authorization headers. The model reference is illustrative and must resolve through GET W/models.

```http
POST /v1/workspaces/0199a801-1000-7000-8000-000000000001/drafts
Content-Type: application/json

{"source":{"kind":"blank"},"authoring_model_ref":"owner-model-1","operation_id":"0199a801-1000-7000-8000-000000000002","message":"Watch failed checkout deployments."}
```

```json
{
  "id":"0199a801-1000-7000-8000-000000000003",
  "state":"editing",
  "etag":"\"draft:0199a801-1000-7000-8000-000000000003:1\"",
  "authoring_model_ref":"owner-model-1",
  "accepted":{"status":"accepted","sequence":"1","replayed":false}
}
```

A candidate/review is absent until there is a validated candidate. A queued message does not make the definition usable immediately.

## Options

### Shared wire rules

- JSON requests reject unknown fields. Omit absent optional success fields. Page envelopes always include `items`, `total:null`, and `next_cursor` (null on the last page).
- Money, message sequences, candidate revisions, setup revisions, and fences use decimal strings. Every nanos amount the API serves or accepts (balances, charges, costs, rates, budgets) is the `Nanos` schema: digits only, matching `^(0|[1-9][0-9]*)$` (`rustd/crates/afd_wire/src/nanos/mod.rs`). Times are RFC 3339 instants in UTC at millisecond precision under REST guide §4 (`rustd/crates/afd_wire/src/instant.rs`). Narrow caps/limits use safe JSON integers.
- Lists accept `limit` and `starting_after` using `afd_core::paging::Cursor` (`rustd/crates/afd_core/src/paging/mod.rs:30`, `paging/cursor.rs:26`). General lists default 50/max 100; draft messages default 20/max 25. Cursors bind resource, creator, ordering, filters, and limit; no cross-subject cursor reuse.
- An entity tag (ETag) is a strong opaque version. GET D/F/G returns its resource tag in `ETag` and `etag`. Compression may weaken the response header to `W/"value"`; the body retains the strong `"value"` tag. Fleet and platform Library clients accept that header only when its opaque value matches the strong body tag. Conditional writes retain the strong body tag. GET F additionally exposes `setup_etag`. Fleet revision writes use the setup tag; status/name PATCH uses the fleet resource tag. A creation retry may advance progress but preserves identity and original accepted inputs.
- Draft creation and both conversation sends use **only** body `operation_id`; reject an `Idempotency-Key` header on those routes. Setup creation, fleet revision application, and Library create/revision use **only** `Idempotency-Key` (UUIDv7); reject body replay keys there. PATCH uses `If-Match`; Stop uses its monotone cutoff.
- Collection creation keys are scoped by actor + workspace + operation type + parent collection. Member message/revision keys are scoped by workspace + operation type + resource, with the accepting actor bound in the receipt. Hash the normalized input. Same key/body/actor replays the same accepted resource or message; changed body or actor reuse on a member resource conflicts. Authorization runs before replay. Keep content-free identity/digest receipts while their resource exists and a 24-hour tombstone after purge. Tombstone replay returns 410; after its disclosed expiry, the key is no longer protected.
- `If-Match` missing →428; stale →412 with current `etag`; state blocks mutation →409 with `current_state`. Check preconditions before external side effects. A recognized idempotent replay recovers the already accepted resource despite a now-stale original tag, after authorization and key/body checks; it performs no second mutation.

### Definition, candidate, review, and common types

`Definition` is exactly `{schema_version:1,description:string,instructions_md:string,config:ConfigDocument}` from [the definition page](fleet_definitions.md#validated-definition). `ConfigDocument` preserves the existing typed `name`/`x-agentsfleet` shape, changing budget fields to decimal-string `daily_nanos`/`monthly_nanos`. Runtime model is `config["x-agentsfleet"].model`. Required daily ceiling is `1..1000000000000` nanos; optional monthly ceiling is `1..10000000000000`; omit monthly for no monthly ceiling. Zero, fractions, signs, exponent notation, and overflow are rejected. These preserve the inspected positive ceiling and cap semantics at `rustd/crates/afd_fleet_runtime/src/config/policy/mod.rs:19–35`.

`Source` is a tagged union:

```text
{kind:"blank"}
{kind:"library",library_id,revision_id}
{kind:"fleet",fleet_id,setup_revision}
{kind:"definition",definition:Definition}
```

Resolve and authorize source before allocating a row. Library and fleet sources capture immutable native Definitions; no mutable-head guessing. Browser and terminal chat share this authoring boundary. File, repository, template-bundle and exported-document sources are rejected. The typed definition source initializes the approved drawer; it is not a file-import adapter.

`Candidate = {id,revision,definition,canonical_digest,policy_semantics}`. Every accepted definition change gets a fresh immutable candidate ID and the next draft-local decimal revision. Initialization starts at `"1"`; unchanged metadata or model selection creates no candidate revision. Before the first proposal, a blank draft has no candidate. First drawer mutation creates a draft from an inline typed definition using shared explicit defaults displayed in the drawer. Empty instructions are allowed while editing but block Save/publication; review readiness includes a definition check.

`Review = {effective,requirements,readiness[],review_digest,policy_semantics,observed_at}`:

```text
effective = {definition,instance_name,runtime_model_ref,source_revision?,target_setup_revision?}
requirements = {credentials[],repositories[],tools[],network?,approval_rules?,budget}
readiness item = {kind:"definition"|"model"|"wallet"|"credential"|"connection"|"grant"|"policy",
                  subject,state:"ready"|"missing"|"blocked",observed_at,next_action?}
next_action = {kind:"account_models"|"billing"|"workspace_connections"|"approvals"|"review",
               label,href}
```

Requirements are derived immutable setup, not readiness results. Review digest excludes observation timestamps and includes candidate, selections, source/target revisions, policy marker, and effective access/model resolution versions. Effective setup changes →409 plus fresh `review_digest`; missing live checks instead appear in `installing` setup progress. GET library Inspect computes equivalent review context without pretending the Library's requirements changed.

`Draft = {id,state,title,etag,authoring_model_ref,cancelled_through,candidate?,review?,linked_fleet_id?,publication?,closed_at?,expires_at?}`. States: `editing|saving|closed`; message state is separate. `publication={library_id,revision_id,canonical_digest}` is a verified explicit publication receipt. Closing sets expiry 30 days later; linked active authoring/setup blocks expiry. Cancelled setup returns `editing`, removing close/expiry fields. Active authoring blocks PATCH and Save.

`Message = {sequence,operation_id,role:"user"|"assistant",state,text?,candidate_id?,usage?,failure?,created_at,finished_at?}`. User sequence names its request; answer shares that sequence. States `queued|running|completed|cancelled|failed|timed_out`. Usage is `{knowledge:"known"|"unknown",input_tokens?,output_tokens?,provider_cost_nanos?,runtime_cost_nanos?}`; unknown figures are absent, never zero estimates. Internal lease/attempt IDs are not public turn resources.

`failure={reason,error_code,recovery_action}` is required on failed/timed_out messages and absent otherwise. All values come from the bounded table below. The daemon records failure and terminal state atomically, including failures before a runner claim. It derives safe owner-facing labels and authorized links on read; reports cannot provide diagnostic text, URLs, credentials, or actions. GET messages and private failure events carry the same fields after reload.

| Failure reason | Stable code | Recovery action |
|---|---|---|
| `insufficient_balance` | `UZ-AUTHOR-007` | `owner_billing` |
| `model_unavailable` | `UZ-AUTHOR-008` | `choose_authoring_model` |
| `context_too_large` | `UZ-AUTHOR-010` | `reduce_context` |
| `provider_failed` | `UZ-AUTHOR-014` | `retry_message` |
| `provider_usage_unknown` | `UZ-AUTHOR-015` | `await_accounting` |
| `attempt_timed_out` | `UZ-AUTHOR-016` | `retry_message` |
| `policy_changed` | `UZ-AUTHOR-009` | `review_setup` |

An unresolved provider obligation takes precedence over a retry action. Stop/delete/revocation fences text writes; revoked callers receive no private failure detail. Retrying is an explicit new message/key after the blocking condition clears.

`Setup = {state:"checking"|"waiting"|"ready"|"cancelled",checks[]}`. Check = `{kind,subject,state:"pending"|"ready"|"blocked",next_action?}`. Check kinds: `source|policy|model|wallet|credentials|connections|grants|stream|activation`. Grant checks record actual approved access; missing approval cannot be logged as ready.

`Start` request is `{kind:"none"}` or `{kind:"run_now",message}` (message 1..32,768 UTF-8 bytes). Result is `{kind:"none"}` or `{kind:"run_now",state:"pending"|"accepted"|"blocked",event_id?,error_code?}`. Derive the initial admission's one identity from creation, not another client key. `start.accepted` requires durable admission, successful append, and saved receipt. This is stronger than ordinary fleet-message acceptance, which requires durable admission and can await append/receipt. Neither means completed work.

`Schedule = {schedule_id,fleet_id,cron,timezone,message,source,once,status,sync,etag,override?,description,run_state,next_run_at}`. The last three are the server's reading at the request's instant (`rustd/crates/afd_cron/src/run/mod.rs`): `description` is the expression in words from the scheduler's own cron parser, with its zone; `run_state` is `scheduled|paused|activation_pending|next_run_unavailable`; `next_run_at` is set only when `run_state` is `scheduled`. Paused wins over sync state, `deleting` or a `failed` sync never runs, `syncing` is activation pending, and a one-off runs at its stored instant or not at all. A fleet with no schedule has an empty `schedules[]`. Preserve existing schedule meanings from `rustd/crates/afd_api_wire/src/schedule/mod.rs:14`; sync is `syncing|synced|failed`, retaining the inspected enum at `rustd/crates/afd_cron/src/model.rs:147`, status is `active|paused|deleting`. Override is `{cron?,timezone?,message?,status?}`. Omitted override means inherited recipe. A null override is request-only Restore.

`Fleet = {id,name,status,etag,setup_etag,setup_revision,policy_semantics,needs_re_review,definition,setup,start,schedules[],webhook_urls[],created_at,updated_at,events_processed,budget_used_nanos}`; event count and nanos are strings. Status remains the existing lifecycle vocabulary, including `installing|active|paused|stopped|killed`. GET adds current setup and start outcomes, replaces editable Markdown with typed definition, and never includes another creator's private transcript.

`FleetSummary = {id,name,status,description,triggers?,created_at,updated_at,events_processed,budget_used_nanos}` is one row of `GET /v1/workspaces/{workspace_id}/fleets`. `description` is the head revision's definition `description`, the saved purpose; it is `null` while the fleet has no saved setup. The page reads it in the same statement as the counters (`rustd/crates/afd_fleet_lifecycle/src/sql/select_fleet_page.sql`).

`Fleet.name` and creation `name` retain the existing validated fleet slug. `Draft.title` is a separate human display title, 1..100 characters; it does not change authored slugs. Review exposes both without substituting a display title into definition metadata or runtime config. An idle `instance_name` draft selection chooses the validated instance slug independently of the recipe; it changes the review digest without creating a new candidate revision. An unresolved account default has a null `effective.runtime_model_ref` and a missing model readiness check.

`LibraryCard = {id,name,description,visibility:"platform"|"tenant",created_at,requirements,required_credentials_reasons,head_revision_id,etag}`. Retain gallery meaning from `rustd/crates/afd_api_wire/src/library/output.rs:9` (`Card`). `Library = {card,head:LibraryRevision,review,etag}`. `LibraryRevision = {id,library_id,revision,definition,canonical_digest,requirements,policy_semantics,created_at}`. Revision list items omit instructions, returning `{id,revision,canonical_digest,created_at}`. Member reads include definition and context review; no secret values.

`ConfigOp = {op:"add"|"replace",path,value} | {op:"remove",path}` restricted to `/config` descendants and a `replace` of `/description`; immutable identity/manifest/receipt fields are unwritable. `InstructionEdit = {start_byte,end_byte,text}` against the exact candidate. Maximum 64 config operations, 32 instruction edits; request text total 32 KiB and request envelope128 KiB. Unknown paths, permission receipt edits, overlapping ranges, invalid UTF-8 boundaries, and unsafe paths fail atomically. These are proposed validated constants, not inspected runtime limits.

### New person routes: 17

| # | Method and resource | Exact input / preconditions | Success output | Specific failures |
|---|---|---|---|---|
| N1 | GET W/models | No body; `purpose=authoring|runtime` optional, list paging; workspace authorization | 200 Page of `{ref,label,provider,model,context_window_tokens,max_output_tokens,eligibility:{authoring,runtime},pricing:{input_nanos_per_million,output_nanos_per_million,runtime_nanos_per_second}}`; owner's eligible enabled entries only | AUTH, INPUT, DEPENDENCY |
| N2 | POST W/drafts | `{source:Source,authoring_model_ref,message?,operation_id}`; body UUIDv7 key only. The chat's initial `title` is the first line of `message`, else of the source's description, else `New fleet`, cut to 100 characters; a later title edit is never overwritten | 201 Draft, optional `accepted:{status:"accepted",sequence,replayed}` when first message exists; `Location:D`, ETag | INPUT, HIDDEN, MODEL, REPLAY, GONE, DEPENDENCY |
| N3 | GET W/drafts | No body; `state`, `linked_fleet_id` and `q` optional, paging; creator only. `q` (1..200 characters, trimmed) matches the title or any turn's text as a case-insensitive substring, LIKE metacharacters literal, and is bound into the cursor | 200 Page of `{id,title,state,etag,linked_fleet_id?,updated_at,expires_at?,candidate_revision,setup_revision,publication_revision}`; the three revisions are null until the chat reaches each (no candidate, no saved fleet, not published) | INPUT, CURSOR, AUTH, DEPENDENCY |
| N4 | GET D | No body; creator + current membership | 200 Draft with latest candidate/review when available; ETag | HIDDEN, AUTH, DEPENDENCY |
| N5 | PATCH D | If-Match; `{title?,instance_name?,authoring_model_ref?,runtime_model_ref?,config_ops?:ConfigOp[],instruction_edits?:InstructionEdit[]}`; at least one, editing and no running/queued messages | 200 Draft with new tag and current candidate/review; recheck owner eligibility; authoring selection applies to future sends only | PRECONDITION, STALE, STATE, INPUT, MODEL, POLICY |
| N6 | DELETE D | No body; creator only, no If-Match | 204 after fencing and private-text deletion boundary; ongoing setup link may remain shared | HIDDEN, AUTH, DEPENDENCY; residual-copy bounds are in runner page |
| N7 | POST D/messages | `{message,operation_id}`; text1..32,768 bytes; body UUIDv7 only; `editing` | 202 `{status:"accepted",sequence,replayed}`; `Location:D/messages`; queued answer not implied | INPUT, STATE, REPLAY, GONE, HIDDEN, DEPENDENCY |
| N8 | GET D/messages | No body; creator; limit default 20/max 25, starting_after | 200 Page of Message; ordered sequence then role; states/usage included | HIDDEN, CURSOR, INPUT, DEPENDENCY |
| N9 | POST D/cancellations | `{through_sequence}` decimal string, at most current last accepted sequence; no If-Match | 200 `{cancelled_through,stopped:[{sequence,state:"cancelled"}]}`; lower repeated cutoff returns standing watermark | INPUT, HIDDEN, DEPENDENCY |
| N10 | GET D/events/stream | Creator; `Last-Event-ID` optional ephemeral hint; no body | 200 `text/event-stream`; typed progress described below; reconnect reconciles GET D/messages and GET D | HIDDEN, AUTH, DEPENDENCY |
| N11 | POST F/revisions | If-Match setup tag + Idempotency-Key; `{draft_candidate_id,review_digest}`; own authorized idle improvement draft targets this fleet | 201 `{fleet_id,setup_revision,setup_etag,etag,review_digest,schedules[]}`; Location F, ETag resource tag | PRECONDITION, STALE, REVIEW, POLICY, REPLAY, HIDDEN, STATE |
| N12 | GET W/fleet-libraries/owned | No tier/owned query; ordinary limit/starting_after allowed; fleet:read | 200 Page of LibraryCard, tenant visibility only | INPUT, CURSOR, AUTH, DEPENDENCY |
| N13 | GET G | No body; optional `revision_id` to Inspect a pinned revision; fleet:read | 200 Library; selected revision and review; ETag | HIDDEN, INPUT, DEPENDENCY |
| N14 | DELETE G | No body; library:write; owned only | 204 removes entry/history, retaining revisions referenced by installed fleets without shared entry discoverability | HIDDEN (platform or foreign), AUTH, DEPENDENCY |
| N15 | POST G/revisions | If-Match Library tag + Idempotency-Key; `{draft_candidate_id}`; owned only; candidate valid for Save and idle | 201 `{library_id,revision_id,head_revision_id,etag}`; Location G/revisions/{revision_id}, ETag | PRECONDITION, STALE, REPLAY, POLICY, HIDDEN, STATE |
| N16 | GET G/revisions | No body; fleet:read; list paging | 200 Page of revision summary | HIDDEN, INPUT, CURSOR, DEPENDENCY |
| N17 | GET G/revisions/{revision_id} | No body; fleet:read | 200 `{revision:LibraryRevision,review}`; immutable content plus live context readiness | HIDDEN, AUTH, DEPENDENCY |

GET D has a complete review only when a candidate exists. Queue acceptance can succeed before wallet admission: balance/model failure becomes a visible failed message rather than silent queued work. Draft creation with a first message atomically records both or neither.

Send atomically captures the draft's selected authoring model reference; admission pins its resolved provider, inference settings, context window, and prices. Earlier admitted turns keep their pins. Idle PATCH can replace a disabled model and survives reload. Title is a human display name, independent of validated definition slugs. Runtime/config changes invalidate setup review; an authoring-model-only change leaves its definition/digest unchanged.

### Changed person routes: 7

| # | Method and resource | Exact target input | Target success output | Specific failures |
|---|---|---|---|---|
| C1 | POST W/fleets | Idempotency-Key; `{source:{kind:"draft",draft_candidate_id}|{kind:"library",library_id,revision_id},name,review_digest,start:Start}`; draft must be creator-owned/idle, Library install can bypass drafts | 201 Fleet with `status:active|installing`, setup checks, start outcome, schedules; Location F and ETag; key retry resumes same identity | REPLAY, GONE, REVIEW, HIDDEN, INPUT, POLICY, DEPENDENCY |
| C2 | GET F | No body; fleet:read | 200 Fleet including current setup/start and both resource/setup tags | existing fleet404 `UZ-AGT-009`, AUTH, DEPENDENCY |
| C3 | PATCH F | Required If-Match resource tag; `{status?,name?}` only; at least one. `active` on installing retries checks, `killed` cancels setup, otherwise existing allowed transitions | 200 Fleet and ETag; cancelled linked draft returns editing | PRECONDITION, STALE, STATE, INPUT, AUTH, DEPENDENCY; a killed fleet returns 404 `UZ-AGT-009` after write preconditions pass |
| C4 | POST F/messages | `{message,operation_id}`; operation_id required UUIDv7; no header key | 202 existing `{status:"accepted",event_id,replayed}`; Location F/messages; recovery through existing message read/stream | INPUT, REPLAY (`UZ-AGT-016`), lifecycle/state, AUTH, DEPENDENCY |
| C5 | PATCH F/schedules/{schedule_id} | Required If-Match schedule tag; `{override:ScheduleOverride|null}` for inherited recipe; independent schedule uses `{cron?,timezone?,message?,status?}` and cannot Restore | 200 Schedule and ETag; tag covers recipe revision + instance setup revision + override revision; null restores exact inherited setup | PRECONDITION, STALE, INPUT, STATE, existing schedule404 `UZ-SCHED-002`, DEPENDENCY |
| C6 | POST W/fleet-libraries | Idempotency-Key; `{draft_candidate_id,label}`; creator-owned valid idle candidate; library:write | 201 `{library_id,revision_id,head_revision_id,etag}`; Location G; explicit publication receipt on linked draft | INPUT, REPLAY, GONE, POLICY, STATE, AUTH, DEPENDENCY |
| C7 | GET W/fleet-libraries | Existing gallery filters/paging; no body | 200 Page of LibraryCard (platform + owned), now head_revision_id and etag | existing library input/cursor errors, AUTH, DEPENDENCY |

The existing GET F/messages must include durable admissions with `delivered_at IS NULL`, including null receipts. Preserve its queued-message shape, logical event ID, ordering, cursor, and authorization. Merge admission/history rows by that identity so append, sweep, and lease transitions never display a duplicate. This is an internal read change, not another route. The inspected query excludes null receipts at `rustd/crates/afd_events/src/history/statement/queued.rs:10–14`; append deferral can still return success at `rustd/crates/afd_admission/src/admit_receipt.rs:43–48`. Retry after an uncertain receipt reuses the original operation ID.

C1 accepts a reviewed private candidate or a pinned Library revision. A selected Slack trigger must be part of the reviewed setup. Changed install name/model choices open a private draft before Save. No hidden trigger mutation or preview route is retained.

C6 publishes only a chat-created candidate. There is no file/GitHub/template import or Markdown/bundle export. Operator Library source writes also resolve an authorized creator-owned draft candidate; operator publication retains its separate platform scope. Source-repository fields, upload/import/re-import routes, bundle delivery and old client flags are removed with their dedicated implementations. GitHub runtime triggers and repository tools remain independent capabilities.

The terminal client starts or resumes a private draft, sends text through N7, follows N8/N10, reviews N4, and explicitly invokes C1 to Save. It can publish through C6/N15 or install a pinned Library revision through C1. Browser and terminal use the same authorization, replay, validation, billing and readiness rules. Terminal conversation text is the authoring input; scripts may submit text but cannot skip review or grant checks.

### Removed person routes: 2

| # | Removed method and resource | Replacement |
|---|---|---|
| R1 | GET W/library-entries | GET W/fleet-libraries/owned |
| R2 | DELETE W/library-entries/{entry_id} | DELETE G |

Removal is immediate; there is no alias. `/v1/operations`, public turn-status, fleet creation/attempt resources, draft revisions/events collections, preview POST, publication/deletion resources, definition-update resources, schedule-overrides resources, fleet revision reads, and a second Library collection are absent.

### Private Server-Sent Events (SSE)

Events contain `{draft_id,sequence?,candidate_id?,state?,usage?,failure?}` with event names `message.accepted|message.running|message.completed|message.cancelled|message.failed|message.timed_out|candidate.changed|draft.closed|draft.deleted`. No private content appears in workspace or operator streams. Recheck current membership/deletion generation before sending private frames; periodic 10-second checks also close quiet revoked streams. Already buffered/rendered text cannot be recalled. Authorized private answer deltas may contain text but are bounded per frame at 4 KiB; final GET messages is authoritative. Revocation closes the stream; deletion emits only identity then closes. A lost event hint triggers read reconciliation, never an invented durable event-history endpoint.

### Runner routes and wire

Existing route identities are at `rustd/crates/afd_http/src/route/runner.rs:36–42`; path constants are at `rustd/crates/afd_wire/src/paths.rs:27–30` and `:92–95`. There is one claim: `POST /v1/runners/me/leases`. Claim input is `{holds?:UUIDv7[<=64],accepted_kinds}` and adds `accepted_kinds:["fleet","authoring"]`; absence is invalid in the cutover. It returns 200 `{lease: TaggedLease|null,retry_after_ms?:integer}`; no-work uses `lease:null` plus the backoff hint. This keeps the existing poll envelope from `rustd/crates/afd_wire/src/lease/mod.rs:178`. A host may opt into fleet-only work; both kinds still count against daemon-enforced worker_count.

```text
{kind:"fleet", ...existing fleet lease payload}
{kind:"authoring",lease_id,attempt_id,draft_id,sequence,fence,deletion_generation,
 expires_at,model:{ref,provider,model,pricing},
 model_access:{token,expires_at,invocation_path},candidate?:Candidate,
 messages[],limits:{input_per_call:32768,
 output_per_turn:<saved model maximum>,max_calls:8,wall_seconds:300,provider_timeout_seconds:30}}
```

Authoring model access is a daemon-issued opaque capability plus the runner's existing bearer authentication. Fence, generation, and sequence are decimal strings. Internal attempt ID and capability are machine-only. The lease includes the current candidate identity/revision/definition when present, so a runner can address its first edit.

Four new machine routes under `/v1/runners/me/authoring-leases/{lease_id}`:

| Method / suffix | Request | Success | Failures |
|---|---|---|---|
| POST `/renew` | `{fence,deletion_generation,usage:Meter}`; current own unexpired lease | 200 `{expires_at,stop_requested,remaining_output_tokens,model_access_expires_at}`; capability hash remains bound to renewed lease | FENCE, HIDDEN, STATE, BALANCE, AUTH |
| POST `/activity` | `{fence,deletion_generation,sequence,activity_sequence,event:{kind:"progress",text}|{kind:"proposal",proposal:Proposal}}`; bounded and no arbitrary channels | 202 `{accepted,replayed,candidate?:{id,revision}}`; proposal receipt includes committed version, progress omits it | FENCE, HIDDEN, INPUT, STALE, STATE, REPLAY |
| POST `/reports` | `{kind:"terminal",fence,deletion_generation,report_id,outcome:"completed"|"failed"|"timed_out"|"cancelled",failure_reason?,answer?,usage:Meter}`; report_id UUIDv7; accounting-only variant and independent replay rules below | 200 `{settled,replayed,sequence,state}`; fenced stale body cannot mutate candidate, private text, or charge twice | FENCE, REPLAY, INPUT, HIDDEN, STATE |
| POST `/model-calls` | `ModelCall` below; runner bearer plus lease capability; one in-flight dispatch | 200 `ModelResult` after durable dispatch-result receipt; exact completed retry returns its stored result | AUTH, HIDDEN, FENCE, REPLAY, STATE, MODEL, TOKEN, PROVIDER, UNKNOWN, TIMEOUT, DEPENDENCY |

`Proposal={kind:"initialize",definition:Definition}` is allowed only when no candidate exists and caps first generated instructions at 12 KiB. Subsequent proposals are `{kind:"edit",base_candidate_id,base_candidate_revision,config_ops:ConfigOp[],instruction_edits:InstructionEdit[]}`. Match both base fields against the current version before atomically replacing it. Both use the same validator as drawer input. An activity receipt is keyed by attempt + activity_sequence and normalized body; sequences are contiguous decimal strings from `"1"`. Exact retry returns the originally accepted candidate version even after later edits. Changed-body reuse, gaps, stale bases, or reordered new proposals change nothing. Initialization retry returns its saved receipt instead of testing the now-existing candidate again. Invalid initialization creates no candidate, and no tool can set grants or manifests. Empty editing instructions block Save/publication.

Reports also accept `{kind:"accounting",fence,deletion_generation,report_id,outcome,failure_reason?,usage:Meter}` with no private text or content digest. The immutable accounting identity is attempt + report_id + normalized fence/generation/outcome/failure_reason/usage, validated against daemon dispatch receipts. Store its digest and accounting acknowledgement independently from optional answer submission. Exact accounting replay returns that acknowledgement whether the original terminal report committed, was never received, or the draft was deleted. Changed accounting under the same identity conflicts; a caller-supplied digest never substitutes for validated usage. First accounting-only settlement records terminal accounting once and cannot create a message, candidate or answer. A deleted/expired generation may confirm or settle authenticated attempt accounting but cannot restore private state. Terminal answer submission has a separate content digest; exact terminal replay may acknowledge it while retained, but accounting replay never opens or reconstructs it. Known daemon receipts override runner estimates; unknown liability remains until authoritative settlement.

`failure_reason` is required for failed/timed_out reports and absent otherwise. It uses the Message reason enum above. The daemon validates the reason against dispatch facts and controls the final code/action. A runner cannot downgrade unknown usage to an ordinary retryable failure.

### Scoped model invocation

`ModelCall={capability_token,attempt_id,fence,deletion_generation,dispatch_id,call_index,request:{messages:ModelMessage[],max_output_tokens}}`. `dispatch_id` is UUIDv7; `call_index` is a contiguous integer 1..8. `ModelMessage` is `{role:"user",text}`, `{role:"assistant",text,proposal_calls:[{id,proposal:Proposal}]}`, or `{role:"tool",call_id,text}`. Text is untrusted context; only the fixed proposal tool is callable. The daemon adds pinned system instructions/tool schema. The 128 KiB envelope and tokenizer/window checks bound the assembled request. Reject provider/model/host/key/header/system/tool-schema override fields.

Native provider replies stay in `authoring.dispatches.native_reply`; the runner receives only text, validated proposals and usage. A saved reply binds its assistant position and the complete normalized conversation prefix through a digest. Later calls must retain that exact prefix before the daemon restores the SDK message, including signed or encrypted reasoning and provider tool identifiers. Current membership, model and capability checks precede private reads. Expired or malformed state refuses dispatch. Deletion and private-result expiry clear both the public result and native reply; a fenced late response records usage without either private copy.

`ModelResult={dispatch_id,call_index,replayed,text,proposal_calls:[{id,proposal:Proposal}],usage:{knowledge,input_tokens?,output_tokens?,provider_cost_nanos?},cut}`. `cut:true` means incomplete proposals cannot run. Responses fit 128 KiB and the remaining output-token allowance. A proposal result is untrusted until `/activity` commits it. No separate model-call read route or raw provider response exists.

1. Admission creates a random 256-bit token, stores its hash, and binds it to runner, owner, draft, sequence, attempt, fence, generation, and pinned model. Issue it only in the lease over the authenticated daemon transport. Never log or spool it.
2. The daemon verifies runner identity, token hash/bindings, current membership/model policy, cancellation/deletion generation, and unexpired lease using authoritative database time. Capability expiry follows the lease and cannot exceed the attempt's 300-second deadline; renewal cannot revive expiry.
3. Under wallet → subject → attempt locks, reserve the next physical dispatch and its output allowance, then commit its request digest/identity before calling the provider. No network request holds these locks. Concurrent different dispatches and an unfinished dispatch replay return STATE without another provider call.
4. The daemon alone resolves the pinned owner's provider key and approved destination. Eligibility requires a credentialed configured model, a daemon-reachable approved transport, a valid saved context/output bound, and pinned prices. Known tokenizers count assembled input; unknown model names use one token per UTF-8 byte as a conservative input reservation. Apply the existing outbound address guard; reject private/custom destinations the daemon cannot safely reach. Authoring transport disables redirects and every automatic provider/SDK retry. Each new physical dispatch consumes one index and the shared input/output/time budget; ordinary runtime transport stays separate.
5. Save bounded result and authoritative usage before returning 200. Exact completed replay rechecks live authorization and returns the same result without dispatch/debit. Changed-body reuse returns REPLAY. A crash after dispatch intent, a lost provider reply, or missing usage marks the obligation unknown and fences further calls; never redispatch that intent.
6. Stop, delete, membership/model revocation, terminal settlement, and expiry invalidate the capability through the guarded attempt state. Authorization commits are the dispatch boundary: revocation first prevents dispatch; a previously authorized call may finish upstream and incur usage. Its late text cannot publish after fencing.

The capability grants no runtime tools, memory, schedules, fleet messages, or credential minting. Existing raw-key runtime delivery remains source evidence, not this implementation: `rustd/crates/afd_fleet/src/lease/answer/mod.rs:76–86` and `rustd/crates/afr_providers/src/connect/mod.rs:90–94`. The new machine route and daemon provider adapter are explicit additions in the file inventory; person-route counts remain 17/7/2.

`Meter={knowledge:"known"|"unknown",calls,input_tokens?,output_tokens?,runtime_ms,provider_cost_nanos?}` is cumulative, monotone, and bound-checked against pinned model prices. Daemon dispatch receipts supply authoritative provider usage; runner meters cannot replace those facts or clear an unknown obligation. Unknown usage is never relabeled known by local estimates. Report/cancel/deletion share the guarded attempt row; terminal replay can confirm accounting without resurrecting text. Daemon claim and both renewal kinds share host capacity enforcement. Details and deletion-copy limits: [runner design](runner_fleet.md#authoring-leases).

## Errors

Routine dependency/auth failures and the named specific failure symbols apply to each table row. All failures use existing `application/problem+json`: `{docs_uri,title,detail,error_code,request_id,user_message?}`. Existing extensions `current_state` on409 and `etag` on412 come from `rustd/crates/afd_http/src/envelope.rs:192–213`. Add `review_digest` only on a stale-review409. Do not invent nested errors or expose operator hints.

The following codes define the cutover error vocabulary. The central registry already contains the draft, replay, input, model, fence, token, provider, balance and unknown-usage codes exercised by the focused checks. Other rows remain requirements until their owning flows and status proofs are implemented; registration alone does not demonstrate behavior. Symbols in the route tables refer to these rows.

| Symbol | Code | HTTP | Meaning / action |
|---|---|---|---|
| AUTH | Existing `UZ-AUTH-002` / `UZ-AUTH-022` | 401 /403 | Invalid authentication / insufficient scope; creator mismatch is HIDDEN. |
| INPUT | Existing `UZ-REQ-001` / `UZ-UUIDV7-009` | 400 | Invalid body shape/key/bound/unknown field; existing path UUIDv7 validation uses UZ-UUIDV7-009. |
| HIDDEN | `UZ-AUTHOR-001` | 404 | Missing, foreign, creator-private, or non-writable platform resource; do not distinguish. |
| STATE | `UZ-AUTHOR-002` | 409 | Wrong lifecycle state; include current_state. |
| STALE | `UZ-AUTHOR-003` | 412 | Version differs; include current etag. |
| PRECONDITION | `UZ-AUTHOR-004` | 428 | Required If-Match absent; refetch before writing. |
| REPLAY | `UZ-AUTHOR-005` | 409 | Key was accepted with a different normalized body/actor; fleet messages keep UZ-AGT-016. |
| REVIEW | `UZ-AUTHOR-006` | 409 | Effective setup changed; include fresh review_digest and refetch review. |
| BALANCE | `UZ-AUTHOR-007` | 402 | Owner wallet fails bounded authoring admission; queued message records this outcome. |
| MODEL | `UZ-AUTHOR-008` | 409 | Selected owner model missing, disabled, or ineligible; no fallback. |
| POLICY | `UZ-AUTHOR-009` | 409 | Marker or supported setup differs; requires new review. |
| TOKEN | `UZ-AUTHOR-010` | 422 | Assembled context cannot fit tokenizer/window bounds; record failed message before call. |
| CURSOR | `UZ-AUTHOR-011` | 400 | Cursor malformed or bound to another query/subject. |
| FENCE | `UZ-AUTHOR-012` | 409 | Stale, cancelled, expired, or deleted lease attempt cannot publish. |
| GONE | `UZ-AUTHOR-013` | 410 | Accepted resource was purged; tombstone refuses recreation for 24 hours. |
| PROVIDER | `UZ-AUTHOR-014` | 502 | Provider failed with known usage; store bounded failure and permit explicit retry. |
| UNKNOWN | `UZ-AUTHOR-015` | 409 | Dispatch/usage unresolved; retain its bound and wait for accounting. |
| TIMEOUT | `UZ-AUTHOR-016` | 504 | Attempt timed out; safe retry only when its provider obligation is resolved. |
| DEPENDENCY | Existing `UZ-INTERNAL-001` | 503 | Required datastore unavailable; no guessed success. |

Oversized HTTP envelopes use existing `UZ-REQ-002` with its registered413. Validation detail names the offending field without echoing private message content. Provider failures become message state and accounting knowledge, not a fabricated completion.

## Related pages

- [API gaps](../reviews/workspace-setup/api.md)
- [Definition](fleet_definitions.md#validated-definition)
- [Creation barrier](data_flow.md#fleet-creation-and-setup-recovery)
- [Runner behavior](runner_fleet.md#authoring-leases)
- [Public change handoff](../reviews/workspace-setup/public-changes.md)
