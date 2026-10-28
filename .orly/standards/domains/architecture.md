# Architecture

This standard covers the names, cardinality and flows that a repository's `docs/architecture/` owns, and how a decision about them lands. It applies to code, specs, design conversations and answers about how data moves between components.

## Grounding

### ARCH-GROUND — Every architectural name, cardinality claim and flow description matches the `docs/architecture/` topic file that owns it.
- **Applies to:** names for streams, channels, key-value store namespaces, consumer groups, queues, remote procedure call (RPC) methods, Postgres schemas and tables
- **Checked by:** review
- **Bad:** `A spec names a new stream run_events when the topic file already calls it project_runs.`
- **Good:** `The spec uses project_runs and cites the topic file's section.`
- **Why:** Names and flows recalled from memory drift away from the system that ships.

Claims such as one consumer per stream, flow descriptions, answers about data flow and proposals to change any of them are covered too.
Each topic is checked against its own file, and a read for one topic does not cover the next.

### ARCH-WINS — Where a spec or code disagrees with `docs/architecture/`, the doc wins until the owner reconciles the two.
- **Applies to:** specs, implementation and reviews that touch an owned name, cardinality or flow
- **Checked by:** review
- **Bad:** `Code follows a spec's new cardinality while the topic file still says one consumer per stream.`
- **Good:** `The conflict is raised with the section it contradicts, and the change waits for the owner.`
- **Why:** Specs are instances, and the architecture doc is the constant they all follow.

## Landing

### ARCH-LAND — An architecture decision reaches `docs/architecture/` in a doc-only commit before its code, or in the same commit, and never after it.
- **Applies to:** new names, cardinality, flows and invariants, including greenfield designs and decisions the doc was silent on
- **Checked by:** review
- **Bad:** `A punch-list task to capture the architecture after the slice lands.`
- **Good:** `A doc-only commit on the branch now, or the doc edit inside the code commit.`
- **Why:** A doc that trails its code misleads every reader in between.

### ARCH-CHAT — A design settled in conversation that names a channel, asserts cardinality, traces a flow or defines an invariant is an architecture decision under ARCH-LAND.
- **Applies to:** design discussions in chat, before any code exists
- **Checked by:** review
- **Bad:** `A chat design kept only in a task note for after the code ships.`
- **Good:** `A doc-only commit before the next agent action, or an owner-approved deferral into the code commit.`
- **Why:** A design held only in chat is lost to the next agent.

A picked-up task that defers the capture until after its code is rewritten to land the doc first or with the code.

## Both homes

### ARCH-HOMES — A branch that changes flow-defining code changes `docs/architecture/`, or its pull request notes say why nothing architectural changed.
- **Applies to:** branches that touch flow-defining code
- **Checked by:** review
- **Bad:** `A branch that adds a queue and leaves docs/architecture/ untouched, with no note.`
- **Good:** `The branch diff carries the queue's topic-file edit.`
- **Why:** Internal design truth drifts silently when code moves without it.

### ARCH-PUBLISHED — A change to documented user-facing behavior revises the affected published pages, and a changelog entry alone does not count.
- **Applies to:** changes to endpoints, the command-line interface (CLI), flags and flows users see
- **Checked by:** gate: docs.updated
- **Bad:** `A new flag ships with a changelog entry and no reference-page edit.`
- **Good:** `The flag's reference page changes in the same release, in the repository that owns it.`
- **Why:** When internal docs and published pages diverge, users and maintainers believe different systems.

The gate sees only this repository's declared docs paths, so review covers a docs repository kept elsewhere.
A docs repository outside this checkout is edited on its own branch, with its owner's approval.
Internal-only reshaping needs no published edit, and the pull request notes say so.
