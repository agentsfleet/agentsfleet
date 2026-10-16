---
type: explanation
audience: contributor
verified: 2026-10-14
product_version: 0.60.0
executable: false
---

# Runner authoring and private-copy limits

## What it is

Authoring is a tagged subject on the existing runner lease claim. It uses the same host's worker capacity and the workspace owner's selected model, but has no fleet tool authority. The daemon owns draft state, candidate validation, cancellation, and accounting. Implementation is pending.

## Why it exists

A browser request cannot safely own a long model call. A runner needs bounded work, durable accepted messages, renewal, replay-safe reporting, and authority narrower than a runtime fleet. Existing runner paths are declared at `rustd/crates/afd_http/src/route/runner.rs:36–42`; there is no need for another claim endpoint.

## How it behaves

### One claim and one worker capacity

POST `/v1/runners/me/leases` returns `kind:"fleet"` or `kind:"authoring"`. All claim paths lock the runner row before checking unexpired active leases across both kinds, and insert the selected lease in that same transaction. The existing local worker loop is not daemon enforcement. A host's authenticated worker_count is server-owned registration/heartbeat data, not a per-poll capacity override. Reducing it stops new claims while old leases drain.

Both fleet and authoring renewal also take that runner-row guard before their lease/affinity or authoring locks. Sample authoritative database time after acquiring the guard. Require the original lease and matching affinity deadline to exceed that time; equality or expiry loses the lease. Extend both deadlines atomically, then commit before releasing the host guard. Expired work requires a new capacity-checked claim. The inspected runtime renewal only checks status/fence/capped time at `rustd/crates/afd_fleet/src/lease/sql/renew.rs:78–102`; this is a required repair. A two-replica barrier test orders expiry → authoring admission → delayed runtime renewal and asserts rejection, one live slot, and no deadlock.

Authoring is creator-private work. Resolve its workspace owner, recheck creator membership, choose the oldest eligible queued message within that draft, and refuse another live attempt for the same draft. Selection across kinds must avoid starvation with a bounded alternating eligible-kind choice; an empty kind never blocks the other. Authoring messages remain a Postgres queue; Dragonfly publishes wake/progress hints, not another authoritative message copy.

Use proposed private tables `core.authoring_drafts`, `core.authoring_messages`, `core.authoring_attempts`, `core.authoring_receipts`, and `core.authoring_usage`; immutable setup/revision records join the existing fleet/Library domains. The authoring progress channel is `draft:{draft_id}:events`; its ownership is checked before subscription and every reconnect. No private event joins workspace broadcast. These are proposed names, not claims that these tables/channels already exist.

Each authoring lease has an attempt identity, fence, deletion generation, expiry, pinned model/tokenizer/prices, and budget. Renewal/report/activity guard all of them. No runtime credential mint, tenant tools, memory, repository checkout, schedule access, or fleet message capability is issued to this lease. Source support files are read-only metadata; instructions are untrusted content, never system authority.

### Scoped provider boundary

The daemon issues an opaque lease capability and serves the fourth machine route, `/model-calls`. [The resource reference](../../architecture/api_resources.md) owns its issuance, bindings, wire, expiry, revocation, dispatch receipts, and replay behavior. The daemon holds provider keys and dispatches each authorized call; the runner receives bounded text/proposals/usage. Existing raw-key runtime transport is not reused for authoring. Keep automatic transport retries disabled on this path, count every physical send, and stop on an unresolved dispatch. Real-router integration tests must deny foreign runner/model bindings, expired/revoked tokens, duplicate physical sends, and runtime credential minting. A local capability type alone supplies no boundary proof.

### Token and time bounds

| Bound | Proposed value | Enforcement |
|---|---|---|
| Input per model call | 32,768 tokens | Selected model tokenizer counts system, tool schemas, bounded history, current text, and candidate excerpts before dispatch. |
| Output across a turn | 8,192 tokens | Sum provider output over all calls; reserve only remaining output for each next call. |
| Calls per attempt | 8 | Daemon and runner counters; no tool loop beyond this count. |
| Wall time per attempt | 300 seconds | Monotonic runner timer plus daemon expiry/fence; timeouts become timed_out. |
| Provider request | 30 seconds | Request timeout and local abort; upstream cancellation is not assumed confirmed. |
| Lease TTL / renewal | 30 seconds /10 seconds | No overlapping admitted lease while the prior live attempt remains counted. |
| First instructions / edited | 12 KiB /32 KiB | Shared validated text bounds, independently of tokenizer counts. |
| System / tool schemas | 2,048 /4,096 tokens maximum | Pin assembled prompt version; validate these counts on model eligibility and before each call. |
| Progress frame / terminal envelope | 4 KiB /128 KiB | Typed bounds before append or spool. |

The model's context window must also cover the counted input plus remaining output reservation. Reject ineligible models rather than choosing a fallback. If context cannot fit, trim only disclosed eligible history with a recorded summary boundary, or fail before calling the provider; never truncate the policy or tool schema. Output includes hidden/reasoning tokens if the provider charges/counts them. Eligibility requires a provider-supported hard output bound and reliable usage semantics. Byte counts cannot prove these caps. Count every physical provider dispatch, including retries, against the call limit; disable hidden SDK retries. An uncertain provider call ends the attempt instead of allowing another call to consume an unknown output remainder.

The model has one proposal tool: initialize once when no candidate exists, then small edits against the candidate ID and revision. Claim includes the current candidate; every proposal acknowledgement returns its accepted version. Exact replay returns that original version, even after later edits. It may ask clarifying questions. It cannot grant access, publish, Save, call arbitrary functions, or choose different credentials. Tool operations pass the same validator as drawer edits. Assistant answer and candidate changes are separate records; terminal report does not re-emit the whole definition.

### Model policy, wallet, and aggregate budgets

Resolve models and billing from `Owned.tenant`, the workspace owner's account, rather than the joined member's account. Send captures the selected reference; admission pins provider/model/tokenizer/prices. Idle version-guarded draft PATCH can replace a disabled selection; earlier turns retain their pins. Model disable, wallet absence, membership removal, and policy-marker drift fail closed. Queue acceptance can precede admission; refusal records the durable bounded Message failure/action from the resource reference.

The locked rule is `balance >= (running + 1) * worst_case_turn_cost`. Under the owner's wallet-row lock, count outstanding running or provider-uncertain attempts, compute the maximum cost bound among them and the proposed attempt, then atomically record admission. Include 8×32,768 input tokens, a total 8,192 output tokens, and 300 seconds of runtime fees. Never use a cheap proposed model to price an expensive active attempt. No fixed draft or per-person slot limit is introduced.

Pin rational rates before dispatch; do not parse prices through floating point. For each token class, round its physical-dispatch cost upward to integer nanos. Round a turn's cumulative runtime fee upward once. A conservative output bound is `ceil(8192 * output_rate / 1000000) + 7` nanos, covering eight independent roundings; input is `8 * ceil(32768 * input_rate / 1000000)`. Rates expressed fractionally use their exact numerator/denominator. Checked multiply/add/ceil must refuse overflow. Fixtures distinguish per-dispatch from aggregate rounding, fractional rates, varied prices, runtime debit interleaving, and unknown usage. Runtime billing outside authoring retains its existing semantics.

All authoring paths use wallet → subject → attempt order; claim and renewal first take the runner guard. Runtime renewal takes runner → runtime lease/affinity → wallet; reports retain runtime lease → wallet without acquiring runner or authoring rows. Cancellation/deletion never acquires a runner row after taking the wallet. Capacity reads use the guarded decision time while all capacity-changing writers serialize on that runner row. Prove competing claims, renewal, cancellation, dispatch, and settlement cannot deadlock or exceed capacity. Do not hold wallet locks during model/network calls.

Runtime debit wallet locks exist at `rustd/crates/afd_fleet/src/lease/sql/report.rs:130–135` and `lease/sql/renew.rs:79–84`; the old runtime admission estimate at `lease/admit/gates.rs:42–55` supplies no authoring guarantee. This design does not add a spend reservation ledger. After unlock, runtime debits can reduce the funds backing authoring. The multiplication rule is an admission bound, not a promise of zero overdraft.

Charge cumulative known usage once, with monotone counters and exact replay identity. Provider timeout/crash without trusted usage records `knowledge:"unknown"`, not a fabricated zero. Keep its full bound counted until the attempt is safely stopped and accounted for; it may block later admission. Fence expiry prevents candidate writes but does not prove the provider stopped billing. A replacement attempt cannot silently overlap uncertain work; explicit recovery must retain both obligations until resolved. Content-free usage records include tenant, model, prices, attempt identity, known charges, and unknown liability, never prompts or answers.

### Stop and terminal races

Stop carries the last accepted sequence and atomically raises `cancelled_through`. Queue entries at or below it become cancelled. A later send receives a greater sequence and is not cancelled by a delayed old Stop. Stop, report, and candidate proposal share the guarded attempt/subject rows. If completion commits first it stays completed; otherwise cancellation fences later text/proposal writes. Repeated lower cutoffs return the standing watermark.

A terminal report can settle already-incurred usage after Stop without displaying a late answer. Replayed reports never charge twice. Timeout, cancellation, and provider errors are separate visible states. Stop does not claim it can undo provider work already started.

### Private deletion and copy inventory

DELETE draft returns 204 after the daemon transaction fences every attempt, removes private text/candidates, writes a content-free deletion generation/tombstone, and makes further private reads404. It does not delete the shared fleet or billing records. Remove the private transcript from server-rendered/browser caches and close its stream. The target stores no unsent text in browser local storage; in-memory text is scoped to account/workspace/subject and cleared on logout/revocation/delete.

| Copy | Owner / removal | Disclosed residual bound |
|---|---|---|
| Draft/message/candidate rows and private replay bodies | Daemon DELETE/expiry, content-free receipts remain | Gone from ordinary reads at 204; database backups follow infrastructure retention, not an instant physical erase. |
| Private progress channel / buffered deltas | Daemon drops draft-specific buffer and closes subscribers | No new private read after 204; buffered or already rendered client text cannot be recalled. |
| Runner memory / in-flight request | Delete generation at renewal, report, activity; runner abort/zeroes local buffers | Online runner notices at next10-second renewal; disconnected runner lease expires within 30 seconds. Upstream completion may outlive local abort. |
| Typed report spool, temporary files, rejected/quarantined copies | Supervisor owns encrypted authoring content plus a separate content-free settlement envelope | Logical expiry is 24 hours from first write, unchanged by retry/rename. Online cleanup runs at the deadline; stopped machines clean before content is opened at next startup. Physical files may remain while powered off. |
| Logs / traces / metrics / crash diagnostics | Do not capture prompts, answers, proposal content, tokens, or secret values | No private text is intentionally written; test redaction and disabling automatic body/crash capture. |
| Provider requests | Provider/service account policy, disclosed on delete | Local abort cannot establish upstream erasure. Provider-specific retention is shown, never claimed zero. |
| Published revision / applied fleet setup | Deliberately shared definition, independent authority | Draft deletion does not unpublish or delete shared setup; disclose this before Save/publication. |
| Backups / legal accounting | Infrastructure backup schedule; billing domain content-free ledger | Follow published retention; no invented physical-purge deadline. |

The 24-hour rule is logical expiry enforced by the supervisor, not guaranteed cryptographic unreadability or physical removal while stopped. Encryption at rest uses a separate local content key; someone retaining that key and ciphertext may decrypt offline. Cleanup deletes the content/key and all temporary/quarantined copies, though storage snapshots follow their own retention. No timer can run on a powered-off host.

Write only encrypted private bytes, including temporary files. Put authenticated attempt/generation/first-write/deadline metadata outside the encrypted payload; keep the `kind:"accounting"` reports variant separately without text, content digests or capabilities. Its immutable replay identity survives private-content removal; an acknowledged terminal submission and an original never received both settle once against daemon receipts, including after draft deletion. Altered usage conflicts. Startup scans every spool category before reading/decrypting/uploading content. Expired, corrupt, orphaned, or clock-uncertain content is removed; content-free settlement remains. During uptime use a monotonic deadline; across restart use UTC plus a saved high-water clock, and fail closed on rollback. Retry, quarantine, and rename never extend expiry. If cleanup fails, stop private draining and report only a bounded cleanup error. Apply these rules to supervisor spool paths, not only the authoring loop.

These are implementation requirements, not fresh retention proofs. With an offline runner, DELETE still204s at the server text boundary with the actual retention disclosure. A tombstone rejects stale generation before accepting text. A content-free settlement may acknowledge known usage without recreating messages. Closed private history expires30 days after close, only when no active work exists.

## Limits

A wallet lock does not reserve funds; a fence does not cancel upstream billing; a 204 is not physical erasure of every copy. The implementation must expose these distinctions without burdening the main chat flow with backend details. No authoring tool calls into tenant/runtime authority are permitted.

## Related pages

- [Resource reference](../../architecture/api_resources.md)
- [Creation](../../architecture/data_flow.md#fleet-creation-and-setup-recovery)
- [Runtime baseline](runtime-baseline.md)
- [End-to-end flow](../../architecture/data_flow.md#fleet-setup-and-runtime-sequence)
