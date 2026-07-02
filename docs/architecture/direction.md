# Architecture Direction

> Parent: [`README.md`](./README.md)

The design constants. Every spec under `docs/v2/` lives inside the constraints below. When a spec proposes something that conflicts with these constants, the spec gets amended — not the constants.

The architecture optimises for one generic operational runtime, not bespoke typed flows per use case.

Principles. Each links to where it's enforced — when a spec contradicts one of these, the spec is wrong:

- **One Fleet is a durable runtime, not a one-shot prompt.** Enforced by the lease model: `agentsfleetd` hands a runner one event at a time and Fleet state survives runner restarts via `core.fleet_sessions`. See [`high_level.md`](./high_level.md) §1 and [`data_flow.md`](./data_flow.md) §"The two fleets in play".
- **Trigger sources can differ; execution enters one common event-processing path.** Webhook, cron, steer, and continuation all `XADD fleet:{id}:events`; the lease/execute path doesn't branch on actor. See [`data_flow.md`](./data_flow.md) §B (TRIGGER) and §C (EXECUTE).
- **Behaviour is primarily defined in natural language through `SKILL.md`, optional `TRIGGER.md`, and bundle support files.** The platform parses `TRIGGER.md` frontmatter when present (tools, credentials, network, budget, context, model); if absent, install creates the default manual/API trigger. `SKILL.md` is advisory prose the fleet reads at run open. See [`capabilities.md`](./capabilities.md) §1.
- **Secrets are injected at execution time, never embedded in prompt text or written into the fleet's context.** Tool bridge substitutes `${secrets.NAME.FIELD}` after sandbox entry; `args_redacted` rebuilds the placeholder before progress frames leave the runner's sandboxed child. See [`data_flow.md`](./data_flow.md) §C step 4 + step 7, [`capabilities.md`](./capabilities.md) §3 "Credential vault" row, and [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §8.2 (api_key visibility boundary).
- **History is durable with actor provenance.** Every event lands in the Fleet event store (`core.fleet_events`) with `actor=(steer:<user>|webhook:<source>|cron:<schedule>|continuation:<original>|slack:<user>)`. See [`data_flow.md`](./data_flow.md) §"The three durable stores".
- **Checkpoints are durable; mid-run state survives via `memory_store`.** Layer 1 of the context lifecycle. See [`capabilities.md`](./capabilities.md) §4 ("Layer 1 — `memory_checkpoint_every`").
- **Context is bounded — no unbounded growth across long-running incidents.** Three layers: memory_checkpoint (L1), tool-result rolling window (L2), and fleet-owned run chunking (L3) — the runtime observes context fill and logs it but cannot interrupt the in-run model loop, so the fleet snapshots and wraps up; a runaway run is bounded by the `budget` caps and the lease runtime deadline, not a continuation counter. See [`capabilities.md`](./capabilities.md) §4.
- **Approvals are first-class.** Risky actions block at the gate; state machine survives runner restarts. See [`capabilities.md`](./capabilities.md) §3 "Approval gating" row and [`data_flow.md`](./data_flow.md) §C step 3.
- **Destructive actions are never assumed safe just because the model suggested them.** The `approval_required` policy lives in `TRIGGER.md`; the runner enforces it before starting the run. SKILL.md prose may ask for approval explicitly. See [`capabilities.md`](./capabilities.md) §3 "Approval gating".
- **Fleet-memory recall has no search infrastructure — the language model is the search engine.** Memory list/recall returns raw entries; the model reads them and decides relevance. No vector search, embeddings, pgvector, pg_trgm, or full-text indexes for memory — a plain `ILIKE` substring filter on key name is the ceiling. Sequential scans over one Fleet's rows are not a reason to add search infrastructure; a spec or review proposing it gets amended. See [`capabilities.md`](./capabilities.md) §4 (memory checkpoint layer).
- **A reactive surface is not a second runtime — agency is the boundary.** A Slack-resident channel bot answers on `@mention`, read-only, and never acts unattended; it is one more producer into the single ingress, and its memory scope is the **channel** (a per-channel resident fleet — memory is keyed by `fleet_id`, so memory persists thread→thread). It does not make v2 "a chat UI over tools" ([`high_level.md`](./high_level.md) §1): the reactive bot is the on-ramp, the durable hired teammate is the product, and what separates them is acting unattended, not memory. See `docs/v2/done/M106_001_P1_API_DOCS_INFRA_UI_SLACK_RESIDENT_CHANNEL_BOT.md`.

The runtime keeps only a thin typed envelope:

- trigger source + actor
- Fleet id / workspace id, with `fleet_id` as the canonical Fleet identifier
- timestamps
- idempotency key
- raw payload
- approval state
- execution state
- context budget knobs (defaults inherited from the active model's tier; user-overridable in `x-agentsfleet.context`)

Everything else stays prompt-driven and iterated by editing the Fleet's documents and policies.

Fleet Bundles are an import/template layer above the runtime, not a second runtime entity. A bundle snapshot may carry `SOUL.md`, provider playbooks, scripts, examples, or assets, but those files cannot grant capabilities by prose. Only parsed trigger policy plus workspace grants become `ExecutionPolicy` on lease. Runners remain infrastructure capacity; they execute Fleets but are not child resources of a customer Fleet.
