# Architecture Direction

> Parent: [`ARCHITECHTURE.md`](../ARCHITECHTURE.md)

The design constants. Every spec under `docs/v2/` lives inside the constraints below. When a spec proposes something that conflicts with these constants, the spec gets amended — not the constants.

The architecture optimises for one generic operational runtime, not bespoke typed flows per use case.

Principles:

- one zombie is a durable runtime, not a one-shot prompt
- trigger sources can differ, but execution enters one common event-processing path
- behavior is primarily defined in natural language through `SKILL.md` and `TRIGGER.md` (or merged frontmatter under `x-usezombie:`)
- secrets are injected at execution time, never embedded in prompt text or written into the agent's context
- history is durable with actor provenance
- checkpoints are durable; mid-stage state survives via `memory_store`
- context is bounded — no unbounded growth across long-running incidents
- approvals are first-class
- destructive actions are never assumed safe just because the model suggested them

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
