# Architecture Direction

> Parent: [`README.md`](./README.md)

The design constants. Every spec under `docs/v2/` lives inside the constraints below. When a spec proposes something that conflicts with these constants, the spec gets amended, not the constants.

The architecture optimises for one generic operational runtime, not bespoke typed flows per use case.

Principles. Each links to where it's enforced; when a spec contradicts one of these, the spec is wrong:

- **One Fleet is a durable runtime, not a one-shot prompt.** The run model enforces it. `agentsfleetd` hands a runner one turn at a time. Fleet state survives runner restarts in the fleet's memory, loaded at each run ([`runner_fleet.md`](./runner_fleet.md) §"Memory continuity").

  See [`high_level.md`](./high_level.md) §1 and [`data_flow.md`](./data_flow.md) §"The coding fleet and the Fleet runtime".

- **All trigger sources enter the same turn-processing path.** Webhook, cron, steer, and continuation all commit to `thread.turns`; the assign/run path treats every actor alike. See the [intake and run flows](./data_flow.md).

- **Behaviour comes from the conversation and the fleet's pinned memory.** A fleet starts from a person's first message, with nothing written down. Standing guidance is a `core` memory entry the fleet stores when a person asks, and every later run's prompt carries it ([pinned memory](./memory.md#6-what-reaches-the-model)).

  The rest of the prompt is fixed text naming the fleet. See [`capabilities.md`](./capabilities.md#1-what-the-model-is-told-and-what-the-run-holds).

- **Capability comes only from the run's policy.** `agentsfleetd` builds it from the fleet row and its approved grants. It holds the published tool catalog, the approved credentials and the network the fleet's two toggles allow. A message or a pinned entry grants nothing.

- **Secrets are injected at run time, never embedded in prompt text or fleet context**. The supervisor substitutes `${secrets.NAME.FIELD}` at send time, outside the sandbox. The activity frame carries `args_redacted`, never the bytes ([`runner_execution.md`](./runner_execution.md#credentials)).

  See [`data_flow.md`](./data_flow.md) §`C. EXECUTE`, [`capabilities.md`](./capabilities.md) §3 "Secret vault" row, and [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §8.2 (api_key visibility boundary).

- **History is durable with actor provenance.** Every turn lands in the turn store (`thread.turns`) with its actor. Actors read `steer:<user>`, `fleet:<fleet_id>`, `webhook:<source>`, `github-app`, `cron:<schedule>`, `continuation:<original>` or `slack:<user>`. See [`data_flow.md`](./data_flow.md) §"The five durable stores".

- **Conversation checkpoints preserve current working notes.** Fleet runs use [conversation compaction](./context_lifecycle.md). Semantic memory supplies facts for later runs; see [semantic memory](./capabilities.md#semantic-memory-remains-separate).

- **Context follows the selected model's working window.** Near the limit, compaction summarizes older groups and keeps the fixed prompt, pinned memory and tools. The payer's balance, output limits and the run deadline still bound each run; see [context limits](./capabilities.md#limits-and-defaults).

- **Approvals are first-class.** A run asks a person with a question, or asks for a credential with `request_access`, and either raises a card. The asking turn settles waiting, and a person's answer or Approve resumes it. The card and the turn are Postgres rows, so both survive runner restarts; see [`capabilities.md`](./capabilities.md) §3 "Questions and access cards".

- **Destructive actions are never assumed safe just because the model suggested them.** No per-fleet rule holds an event for a person. A fleet acts only with credentials a person approved for it, and an approved GitHub scope bounds its repositories and its write reach.

  See [where a grant comes from](./connectors.md#where-a-grant-comes-from). The model can still stop and ask before acting.

- **Fleet-memory recall has no search infrastructure; the language model is the search engine**. Memory list/recall returns raw entries; the model reads and decides relevance. No vector search, embeddings, pgvector, pg_trgm, or full-text indexes for memory.

  The ceiling is a plain case-insensitive substring match on key and content, with no index behind it. Sequential scans over one Fleet's rows are not a reason to add search infrastructure; a spec or review proposing it gets amended.

  See [`capabilities.md`](./capabilities.md) §4 (memory checkpoint layer).

- **Slack uses the existing runtime.** A fleet's `mention` trigger row selects the recipient; connecting Slack creates no fallback agent. [Mention routing](scenarios/slack-incident-responder.md#4-routing-a-mention) owns intake and fixed notices.

The runtime keeps only a thin typed envelope:

- trigger source and actor

- Fleet id and workspace id, with `fleet_id` as the canonical Fleet identifier

- timestamps

- idempotency key

- raw payload

- approval state

- execution state

- context limits, read from the selected model's saved catalogue entry

Everything else stays prompt-driven and changes in the conversation or in the fleet's pinned memory.

Prose cannot grant capabilities.

Only the fleet row and its approved grants become `ExecutionPolicy` on run. Runners remain infrastructure capacity; they run Fleets but are not child resources of a customer Fleet.
