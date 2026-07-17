# name_architecture.md — architecture-consult dispatch (LATENT façade)

This is the prose the AGENT reads **before naming an architectural element or
describing a data flow**. Like `verify`, it has **no deterministic `.sh` half** —
no script can detect the moment an agent is *about to name a stream* or *assert a
cardinality*. It is a pure **🔵 judgment** dispatch grounded in `docs/architecture/`.
(This is the former Architecture Consult & Update gate absorbed into the dispatch model.)

**Signal legend:**

- 🔵 DECIDE — judgment-only; the agent greps the relevant `docs/architecture/`
  topic file and grounds its naming/flow claim in it. Blocks the *turn*, not a
  commit.

## Trigger — grep/read the relevant `docs/architecture/` topic file before

- Naming a stream / pub-sub channel / Redis key namespace / consumer group /
  queue / RPC method / Postgres schema / table.
- Asserting cardinality ("one row per X", "exactly one consumer per stream",
  "fleet-wide vs per-tenant").
- Describing a flow ("on crash → X reclaims via Y", "trigger source A lands on
  stream B with actor C").
- Answering a user question about how data flows between components.
- Proposing a change to any of the above as part of a spec or implementation.
- A new architecture-adjacent question that arises mid-task — re-consult per
  topic, not once per task.

**Override:** none — doc wins until reconciled.

## Why

`docs/architecture/` (the directory; topic files linked from
`docs/architecture/README.md`) is canonical for stream/channel/queue names, table
cardinality, ownership, and end-to-end flows. The failure mode is reinventing
terms or asserting flow shapes from training data instead of grounding in the
doc. Specs are *instances*; this doc is the *constant* — when they disagree, the
doc wins until reconciled.

## Behavior

- **Doc answers** → proceed (no citation block).
- **Doc silent** → proceed with extra care; land doc decision in same commit.
- **Doc conflicts** → surface with a one-line citation (`grounded in §X.Y,
  proposal extends/conflicts because <reason>`) and wait.
- **Greenfield** → land initial doc + code in same commit.

## Landing rule (non-negotiable)

An architecture decision lands its `docs/architecture/` edit either:

- **(a)** immediate doc-only commit on the active branch (preferred), OR
- **(b)** same commit as the implementation.

**Never** (c) follow-up commit AFTER the code.

**Chat brainstorming counts as "describing a flow."** A multi-turn chat designing
a new pattern (naming a channel, asserting cardinality, tracing a flow, defining
an invariant) HAS fired the dispatch even if no code touched yet. Acceptable
resolutions: (a) doc-only commit BEFORE the next agent action, or (b) Indy-acked
deferral quote (per CHORE(close) deferral discipline) routing the capture into
same-commit-with-code. The failure mode this clause names explicitly: encoding the
brainstorm as a "punchlist task to land after the code ships" — that's exactly (c)
and is forbidden. Pickup agents reading a task whose description says "capture
architecture in docs/architecture/ AFTER <slice> lands" must rewrite it to
same-commit OR doc-only-now before continuing.

## Required output (only when doc conflicts or is silent)

```
ARCH: grounded in <topic-file>:<§X.Y> | proposal: <one-line> | status: <extends|conflicts|silent> | landing: <a|b>
```

When the doc directly answers, no output is required.

## CHORE(close) check — both doc homes

Architecture truth has TWO homes with different audiences; an architecture-touching
branch settles both before the Pull Request (PR) opens:

- **Product repo `docs/architecture/**`** (internal design truth). Every M-spec
  branch that touched flow-defining code produces a non-empty
  `git diff origin/main..HEAD -- docs/architecture/`. Else PR Session Notes
  documents why nothing architectural changed.
- **Docs project `~/Projects/docs/`** (user-facing, `docs.agentsfleet.net`). When
  the change alters documented user-facing behaviour (endpoints, Command-Line
  Interface (CLI) surface, flags, flows users see), the affected pages are
  revised per the CHORE(close) required
  outputs in `AGENTS.md` — a changelog `<Update>` alone is insufficient.
  Internal-only reshaping with no documented-behaviour change → no docs-project
  edit, and Session Notes say so.

Updating one home and not the other is the failure mode this section names: the
internal doc drifts from what users are told, or vice versa.
