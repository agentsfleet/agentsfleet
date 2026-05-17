# Architecture Direction

> Parent: [`README.md`](./README.md)

The design constants. Every spec under `docs/v2/` lives inside the constraints below. When a spec proposes something that conflicts with these constants, the spec gets amended — not the constants.

The architecture optimises for one generic operational runtime, not bespoke typed flows per use case.

Principles. Each links to where it's enforced — when a spec contradicts one of these, the spec is wrong:

- **One zombie is a durable runtime, not a one-shot prompt.** Enforced by the worker model: per-zombie thread, blocking `XREADGROUP`, persistent through worker restarts via `core.zombie_sessions`. See [`high_level.md`](./high_level.md) §1 and [`data_flow.md`](./data_flow.md) §"The two agents in play".
- **Trigger sources can differ; execution enters one common event-processing path.** Webhook, cron, steer, and continuation all `XADD zombie:{id}:events`; the worker's `processEvent` doesn't branch on actor. See [`data_flow.md`](./data_flow.md) §B (TRIGGER) and §C (EXECUTE).
- **Behaviour is primarily defined in natural language through `SKILL.md` and `TRIGGER.md`.** The platform parses `TRIGGER.md` frontmatter (tools, credentials, network, budget, context, model); `SKILL.md` is advisory prose the agent reads at stage open. See [`capabilities.md`](./capabilities.md) §1.
- **Secrets are injected at execution time, never embedded in prompt text or written into the agent's context.** Tool bridge substitutes `${secrets.NAME.FIELD}` after sandbox entry; `args_redacted` rebuilds the placeholder before progress frames leave the executor RPC. See [`data_flow.md`](./data_flow.md) §C step 4 + step 7, [`capabilities.md`](./capabilities.md) §3 "Credential vault" row, and [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §8.2 (api_key visibility boundary).
- **History is durable with actor provenance.** Every event lands in `core.zombie_events` with `actor=(steer:<user>|webhook:<source>|cron:<schedule>|continuation:<original>)`. See [`data_flow.md`](./data_flow.md) §"The three durable stores".
- **Checkpoints are durable; mid-stage state survives via `memory_store`.** Layer 1 of the context lifecycle. See [`capabilities.md`](./capabilities.md) §4 ("Layer 1 — `memory_checkpoint_every`").
- **Context is bounded — no unbounded growth across long-running incidents.** Three layers: memory_checkpoint (L1), tool-result rolling window (L2), stage chunking (L3) with a chain cap at 10 continuations. See [`capabilities.md`](./capabilities.md) §4.
- **Approvals are first-class.** Risky actions block at the gate; state machine survives worker restarts. See [`capabilities.md`](./capabilities.md) §3 "Approval gating" row and [`data_flow.md`](./data_flow.md) §C step 3.
- **Destructive actions are never assumed safe just because the model suggested them.** The `approval_required` policy lives in `TRIGGER.md`; the worker enforces it before `executor.startStage`. SKILL.md prose may ask for approval explicitly. See [`capabilities.md`](./capabilities.md) §3 "Approval gating".

The runtime keeps only a thin typed envelope:

- trigger source + actor
- zombie id / workspace id
- timestamps
- idempotency key
- raw payload
- approval state
- execution state
- context budget knobs (defaults inherited from the active model's tier; user-overridable in `x-usezombie.context`)

Everything else stays prompt-driven and iterated by editing the zombie's documents and policies.
