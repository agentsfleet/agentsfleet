# 🚧 Architecture Consult & Update Gate

**Family:** Architecture grounding. **Source:** `AGENTS.md` (project-side guard); canonical body of architectural decisions lives at `docs/architecture/` (the directory; topic files linked from `docs/architecture/README.md`).

**Triggers** — grep/read the relevant `docs/architecture/` topic file before:

- Naming a stream / pub-sub channel / Redis key namespace / consumer group / queue / RPC method / Postgres schema / table.
- Asserting cardinality ("one row per X", "exactly one consumer per stream", "fleet-wide vs per-tenant").
- Describing a flow ("on crash → X reclaims via Y", "trigger source A lands on stream B with actor C").
- Answering a user question about how data flows between components.
- Proposing a change to any of the above as part of a spec or implementation.
- A new architecture-adjacent question that arises mid-task — re-consult per topic, not once per task.

**Override:** none — doc wins until reconciled.

## Why

`docs/architecture/` (the directory; topic files linked from `docs/architecture/README.md`) is canonical for stream/channel/queue names, table cardinality, ownership, and end-to-end flows. The failure mode is reinventing terms or asserting flow shapes from training data instead of grounding in the doc. Specs are *instances*; this doc is the *constant* — when they disagree, the doc wins until reconciled.

## Behavior

- **Doc answers** → proceed (no citation block).
- **Doc silent** → proceed with extra care; land doc decision in same commit.
- **Doc conflicts** → surface with a one-line citation (`grounded in §X.Y, proposal extends/conflicts because <reason>`) and wait.
- **Greenfield** → land initial doc + code in same commit.

## Landing rule (non-negotiable)

An architecture decision lands its `docs/architecture/` edit either:

- **(a)** immediate doc-only commit on the active branch (preferred), OR
- **(b)** same commit as the implementation.

**Never** (c) follow-up commit AFTER the code.

## Required output (only when doc conflicts or is silent)

```
ARCH: grounded in <topic-file>:<§X.Y> | proposal: <one-line> | status: <extends|conflicts|silent> | landing: <a|b>
```

When the doc directly answers, no output is required.

## CHORE(close) check

Every M-spec branch that touched flow-defining code produces a non-empty `git diff origin/main..HEAD -- docs/architecture/`. Else PR Session Notes documents why nothing architectural changed.
