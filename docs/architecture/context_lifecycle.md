# Shared conversation compaction

> Read this when setup chat or fleet runtime approaches the selected model's working window. Parent: [Architecture](README.md).

## Approved behavior

```text
 Private setup chat                  Fleet execution
        |                                   |
        +----------- afd_context -----------+
                     |       |
               summarize     checkpoint
                     |       |
              metered call   guarded daemon storage
                     |       |
                     +-------+--> continue the same work
```

The selected model supplies its physical window. Preflight compacts at 90 percent input fill, or earlier when input plus reserved output would exceed 95 percent.

Preparation measures the serialized provider request. This covers request fields but does not reproduce every model's native token framing.

Finite output, money, physical-call, runtime and cancellation limits still apply.
No independent 32,768-token input allowance or lifetime 64-message refusal remains.

`afd_context` owns history selection, summary format, checkpoint validation and restoration.
`HistorySource`, `TokenEstimator`, `Summarizer` and `CheckpointStore` adapt effects.
Known adapters use generics; selected runtime effects use object-safe trait objects.

Cancellation uses `Fn`; progress observations use `FnMut`; an owned prepared publication consumes a `FnOnce` callback.
Database generation and source guards make repeated writes idempotent.

Original history remains available. Current instructions, authoritative state and the current request remain pinned.
Summaries and quoted prior conversation are untrusted data.

Whole tool call/result groups remain atomic; opaque native continuations never replay against a changed prefix.
Retained tool results keep their image blocks. A prefix change clears native replay from existing assistant messages; replay produced afterward stays with its turn.

Summarization disables tools and uses the normal physical-request meter.
Unknown usage cannot become zero or authorize a resend.

## How a summary is produced

The planner reads chronological history in pages of at most 64 complete groups, preserving each assistant/tool-result group.
This page size does not limit conversation lifetime.
After checking an existing checkpoint against its source, preparation measures the remaining groups with that summary and current request.

When preparation reaches the end of history, compaction retains up to two recent groups verbatim and summarizes an older prefix.
At least one group must enter a summary.

If the threshold is reached before history ends, the selected prefix can contain all groups read so far.
Selection follows chronology; it does not rank messages by keywords or importance.

The planner measures the summary request separately, including its reserved output.
An oversized request repeatedly halves the selected prefix until it fits; material beyond that prefix remains in original history for subsequent preparation.
If no complete group fits, preparation returns a window error and sends no summary request.

The selected conversation model receives one tools-disabled request with fixed instructions from `afd_context/src/summary.rs`.
The input quotes the previous summary and selected complete conversation groups as JSON.
The instructions request the goal, constraints, decisions, completed work, remaining work and exact references, preserving uncertainty and unresolved failures.

The model writes the summary. The shared Rust planner selects its source material; a local text-shortening algorithm does not generate the notes.
The response must contain only the expected JSON fields and pass bounded validation.

Output is capped at 2,048 tokens, subject to the remaining host allowance; the validated summary is limited to 32,768 bytes.
An incomplete response, tool call, malformed summary or missing usage cannot publish a checkpoint.

Validation proves the document's shape and source identity; it cannot prove that the model preserved every relevant fact.

Token measurement uses `tiktoken-rs` through `afd_inference/src/budget.rs` for recognized model encodings.
Other models reserve one token per UTF-8 byte.
Measurement covers the serialized provider request, including tools and replay data, plus 32 tokens per history message.

This is an admission estimate; provider-reported usage determines accounting.
Each physical request is checked, including ordinary turns, tool continuations and summary calls.
After publishing a summary, preparation measures the shorter continuation again before sending it.

An OpenAI-compatible transport does not establish a model's tokenizer: unrecognized models still use the byte fallback, which can trigger compaction early.

Serialized JSON tokenization and byte fallback are estimates, not proven upper bounds on the provider's formatted input.
The [token counting decision rubric](../reviews/pr784-tokenizer-evaluation.md) records the candidate comparison and required model/request tests.

Near capacity, both hosts can confirm the prepared request through the selected native Responses, Messages or Gemini API.
Confirmation starts at 80 percent of input plus reserved output and has a ten-second deadline.
Other routes retain their local estimate; a routed request is never forwarded elsewhere for counting.

Each count binds to the request bytes, model, route settings, physical window and counting revision.
Authoring releases its transaction before counting, then rechecks source, checkpoint and credential authority before creating paid intent.
Count failure stops that attempt; it cannot become a zero count or an automatic paid retry.

After publication, preparation inserts the summary as explicitly untrusted conversation data before the retained suffix and current request.
Current installed instructions and authoritative state remain separate.

Setup chat loads its draft checkpoint during preparation; runtime loads only its own lease and branch checkpoint.
A new runtime lease does not inherit another lease's conversation checkpoint.

## User-visible progress

The engine offers `Read`, `Summarizing` and `Committed` observations in `afd_context/src/compact.rs`.
Setup sends `context.progress` through its creator-private stream; fleet runs send `context_progress` through the authenticated activity stream.
Each notice contains a generation and a closed state: `summarizing`, `resuming` or `interrupted`.

React announces “Context summarizing…” and, after the checkpoint commits, “Context summarized, resuming…”.
An unsuccessful operation can announce “Context summarization interrupted.”
Runtime child summaries remain private; root summaries announce progress without publishing generated notes.

Setup notices recheck the current creator, membership, lease and message before publication and forwarding.
The browser accepts notices only for a known running message, rejects older generations and clears visible hints on reconnect, cancellation or completion.

Ordinary reply text replaces the provisional status. Saved history remains authoritative.

## Operational traces

`afd_context::Trace` creates a `context.compact` span around a physical summary and its guarded publication.
The paired events are `context_compaction_started`, `context_compaction_completed` and `context_compaction_failed`.
Dropping unfinished work records failure; completion is recorded only after the durable commit succeeds.

Fields include host, caller-owned operation identifier, expected and committed generation, group count, reserved output, elapsed milliseconds and registry error code.
Prompts, generated notes, source history and private error prose are excluded.

Each finished model turn with reported input usage also emits `model_input_calibration`.
It compares the input allowance with actual usage and records the measurement method, revision, provider API, request kind and hashed model/route identities.

Missing usage produces no fabricated zero; prompts, credentials, URLs and request digests are excluded.

## Checkpoint ownership and storage

The additive `memory.context_checkpoints` table holds conversation working state, separate from semantic memory and immutable fleet revisions.
A draft row belongs to its private draft.
A runtime row belongs to a live fleet lease and one branch of that run.

Checkpoint restoration checks model/inference identity, trusted pinned-state identity, source coverage, source digest and generation.
A runtime checkpoint never imports a private draft summary or another lease's notes.

Foreign keys remove private checkpoints with their draft, fleet, workspace or tenant.
Runtime reads and conditional writes use authenticated runner routes; the runner receives no datastore credentials.

Both hosts check live authority in the checkpoint write's transaction after the model responds.
A stale lease, changed source or changed generation cannot replace current state.
An incurred charge survives a rejected publication.

The daemon assembles authoring history; the bounded runner envelope carries current-turn work.
Paging bounds each read without limiting conversation lifetime.
Authoring records each compaction dispatch before provider access, then commits the result and checkpoint through its guarded receipt path.

Setup runners opt into summary replies with `supports_compaction: true`.
Ordinary replies omit `compacted` when false so older runners can read them during deployment.
An older runner whose input needs a summary receives `AUTHORING_TOKEN` before summary funding or provider access.

## Tenant boundaries and sensitive buffers

Runtime routes authenticate the runner; the daemon checks its active lease and fencing token before reading or writing notes.
The daemon derives the fleet from that lease, then scopes storage by fleet, lease and branch.
Setup preparation checks the draft creator, current workspace membership and tenant before loading the draft checkpoint.

The checkpoint table uses the shared daemon database role; these application checks enforce tenant access.

Runtime credential wrappers use `Zeroizing<String>`, and the run's secret scrubber cleans model-written summary output before decoding.
Debug formatting for groups, summaries and checkpoints omits private contents.
These protections do not cover every conversation allocation.

Groups and summary fields use ordinary strings and vectors, and compaction serialization returns an ordinary string.
The checkpoint layer does not guarantee overwriting those allocations or tokenizer temporaries when they are released.

Saved checkpoint documents are ordinary database JSON; this layer does not encrypt individual documents.
Access isolation, buffer wiping and protection against a compromised daemon or host require separate evidence.

## Implementation scope

| Area | Files |
|---|---|
| Shared engine | `rustd/crates/afd_context/Cargo.toml`, `src/{lib,error,window,ports,plan,compact,checkpoint,summary,trace}.rs`, tests and daemon repository modules |
| Registration and storage | `rustd/Cargo.toml`, `rustd/Cargo.lock`, `schema/2160_memory_context_checkpoints.sql`, `rustd/crates/afd_db/src/migration.rs` and migration tests |
| Authoring admission and accounting | `rustd/crates/afd_authoring/Cargo.toml`, `src/lease/claim/{context,issue/payload}.rs`, claim history SQL, `src/model/budget.rs`, report usage and model preparation/receipt/continuation modules and new context adapters |
| Runner protocol | `rustd/crates/afd_wire/src/authoring/{model,meter,lease}.rs`, shared context payloads/paths, runner route declarations and generated `public/openapi.json` |
| Runtime | `rustd/crates/afr_agent/Cargo.toml`, context and loop modules, provider request estimation, `rustd/crates/afd_gate/src/policy/context.rs`, checkpoint adapter on `AgentRun` |
| Trusted transport | `rustd/crates/afd_http/src/services/leasing` and its manifests, `rustd/crates/afd_api_runner` context handlers, `rustd/crates/afd_fleet` fenced context methods, `rustd/crates/afr_supervisor` authoring turn, client and context adapter modules, related manifests |
| Status and traces | `afd_context/src/trace.rs`, `afd_authoring/src/model/{dispatch,progress}`, tenant private-stream forwarding, shared activity payloads, runtime/supervisor activity handling, app draft/fleet stream reducers, projections, accessible status components and their tests |
| Behavior and evidence | Host unit/integration tests, existing unchanged continuity acceptance, `docs/architecture/{README,capabilities,runner_fleet,api_resources,billing_and_provider_keys}.md`, active M230 spec and relevant published guidance |

## Required proof

Run the original unchanged `make acceptance-local` after integration. Its single-fleet continuity scenario must continue past the former turn-42 failure and retain its original 181-message and four-save assertions. Record that result separately from focused proofs that force repeated compactions in each host. 

Retain the historical failed run.

Prove source paging past 64 turns and input past 32,768 when the physical window fits.
Test exact trigger/output boundaries, repeated compaction and restore, model changes, atomic tool groups and native replay.

Also test private scope isolation and deletion, stale authority/generation, cancellation, output and physical-call budgets, known/unknown usage and rolling financial reservation. The separate concurrent-browser failure remains an independent acceptance concern.

## Preparation and future sandbox boundary

**Owner request, Oct 24, 2026: 10:21 PM:**

```text
i request you to do a large refactor of this area considering the fact that we will support or may move to use SPACE
```

This extends the approved compaction implementation in PR #784.

**Goal:** Both hosts share a storage-independent context lifecycle whose preparation work grows linearly with retained history. Independent conversations make progress concurrently.

**Owner-selected direction:** Use SPACE (`Sandboxed Platform for Agentic Code Execution`) approaches and selected source in an `agentsfleet`-owned sandbox. Retain control over performance and future changes. Broad platform integration and operating SPACE as a service are outside this direction.

No copied component or native backend implementation has been selected. Conversation notes remain application-owned behind the daemon's authenticated checkpoint API. The [sandbox evaluation](sandbox_evaluation.md) owns the source-reuse criteria and proposed Git/recovery walkthrough; the [working brief](../../concepts_brief.md) records brainstorming inputs.

**Reference checked:** Local SPACE revision `20d321385dac0855b39f86cd93c008ff5ec4ce3c`, README, architecture request flow and lifecycle, native command-session recovery, and PostgreSQL contention classification. Accepted sandbox operations require readiness checks. Command sessions, attachment credentials and sandbox snapshots have separate lifetimes.

**Golden path:** Read the current authority and model. Read original conversation groups or verify an owned immutable prefix. Prepare complete provider input.

Fund the measured request outside datastore locks. Revalidate source and authority before dispatch. Record the dispatch before provider access.

Save known usage and conditionally publish notes. Continue through the same executor interface, independently of sandbox backend.

**Implemented preparation:** The shared planner uses geometrically spaced probes and a final complete provider estimate.
Each runtime conversation owns an encoded-history index with prefix digests.
The index re-encodes the mutable final group and newly appended history.

Authoring funding owns the prepared body and source snapshot.
Freshness verification returns that body after checking authority and source history; it does not prepare the request again.
The estimator-work regression measures linear encoding work, while freshness verification still reads and hashes original history.

**Future sandbox work:** Checkpoint storage remains behind the daemon API; the optional PostgreSQL feature keeps database dependencies out of runtime builds.
Git worktree recovery across leases and a Go/Firecracker backend remain separate proposal and implementation work.

**Concurrency:** No global conversation lock or shared mutable history cache. One runtime loop owns its index.

Database writes retain generation checks and live-lease fencing. Cancellation and failed publication retain accounted model work.

**Quality ceiling:** Measure work with deterministic counters before claiming throughput improvements. The full browser scenario and real storage races remain required. A native guest engine informed by SPACE needs its own implementation and real-backend evidence.

**Files:** `afd_context` planner, ports and tests; authoring preparation/funding adapters; runtime context index/estimator; affected workspace manifests; architecture and this spec. Existing executor and sandbox interfaces provide part of the backend boundary; repository preparation still requires a host-visible path.

A new checkpoint-storage crate is conditional on a demonstrated dependency benefit. No acceptance limits or gate implementation changes.

