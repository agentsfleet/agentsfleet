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
Each estimate includes complete provider framing.

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
Summarization disables tools and uses the normal physical-request meter.
Unknown usage cannot become zero or authorize a resend.

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

## Implementation scope

| Area | Files |
|---|---|
| Shared engine | `rustd/crates/afd_context/Cargo.toml`, `src/{lib,error,window,ports,plan,compact,checkpoint,summary}.rs`, tests and daemon repository modules |
| Registration and storage | `rustd/Cargo.toml`, `rustd/Cargo.lock`, `schema/2160_memory_context_checkpoints.sql`, `rustd/crates/afd_db/src/migration.rs` and migration tests |
| Authoring admission and accounting | `rustd/crates/afd_authoring/Cargo.toml`, `src/lease/claim/{context,issue/payload}.rs`, claim history SQL, `src/model/budget.rs`, report usage and model preparation/receipt/continuation modules and new context adapters |
| Runner protocol | `rustd/crates/afd_wire/src/authoring/{model,meter,lease}.rs`, shared context payloads/paths, runner route declarations and generated `public/openapi.json` |
| Runtime | `rustd/crates/afr_agent/Cargo.toml`, context and loop modules, provider request estimation, `rustd/crates/afd_gate/src/policy/context.rs`, checkpoint adapter on `AgentRun` |
| Trusted transport | `rustd/crates/afd_http/src/services/leasing` and its manifests, `rustd/crates/afd_api_runner` context handlers, `rustd/crates/afd_fleet` fenced context methods, `rustd/crates/afr_supervisor` authoring turn, client and context adapter modules, related manifests |
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

**Implementation proposal:** Replace repeated cumulative estimates with geometrically spaced probes and a final complete provider estimate. Give each runtime conversation an owned encoded-history index with prefix digests. Reuse one history view and the final measured continuation.

Retain exact authoring source revalidation while avoiding a second provider-body preparation. Evaluate whether extracting checkpoint storage improves a real dependency boundary; the existing optional PostgreSQL feature already keeps that dependency out of runtime builds.

**Concurrency:** No global conversation lock or shared mutable history cache. One runtime loop owns its index.

Database writes retain generation checks and live-lease fencing. Cancellation and failed publication retain accounted model work.

**Quality ceiling:** Measure work with deterministic counters before claiming throughput improvements. The full browser scenario and real storage races remain required. A native guest engine informed by SPACE needs its own implementation and real-backend evidence.

**Files:** `afd_context` planner, ports and tests; authoring preparation/funding adapters; runtime context index/estimator; affected workspace manifests; architecture and this spec. Existing executor and sandbox interfaces provide part of the backend boundary; repository preparation still requires a host-visible path.

A new checkpoint-storage crate is conditional on a demonstrated dependency benefit. No acceptance limits or gate implementation changes.

