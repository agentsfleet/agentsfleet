---
type: explanation
audience: contributor
verified: 2026-11-25
product_version: 0.90.0
executable: false
---

# Durable tasks: one objective kept going across runs

> Read this when a fleet must keep one objective going across runs, and to learn who decides it is done. Parent: [`README.md`](./README.md).

## What it is

A durable task is one objective a fleet accepted, with the completion statements that decide when it is done.
The fleet keeps its chat, memory, grants and triggers; the task adds objective state on top of them.
Each attempt is an ordinary run of a continuation turn, and a separate decision model judges after each attempt whether the task is done.

| Fact | Value | Source |
|---|---|---|
| Who starts a task | The fleet's own model, with `task_start`, only from a person's message | `afd_fleet::durable_task::bindings` |
| Who judges it done | A decision model, from the task's statements | `af_providers::decision` |
| Tables | `fleet.durable_tasks`, `fleet.durable_task_items`, `fleet.durable_task_inputs`, `fleet.durable_task_effects` | `afd_db/schema` |
| Decision tables | `runner.decision_calls`, `runner.decision_payloads` | `afd_db/schema` |
| States | `ready`, `running`, `waiting`, `blocked`, `paused`, `done`, `cancelled`, `exhausted` | `af_wire::durable_task::state` |
| Runs per fleet at once | One; several tasks share the fleet's thread in turn | [`data_flow.md`](./data_flow.md) |
| Slice per run | At most 10 minutes | `durable_task/run.rs` |
| Example fleet | [`stubborn-mule`](../../tests/fixtures/fleets/stubborn-mule.json) | prime fixture set |

```text
Fleet: Linkwarden repair
  ordinary message ─► turn ─► run ─► answer
  task A: repair PR X        run A1 ─► wait for checks ─► run A2 ─► done
  task B: investigate Y      run B1 ─► blocked on access

Fleet thread over time: [A1][B1][idle][A2]   one active run at a time
```

## Why it exists

Some work outlives one run: a repair that must wait for CI, or an investigation that spans hours.
A plain chat turn ends when its run ends, and a lost worker loses its place.
A task holds the objective, its limits and its progress in Postgres, so any runner can take the next attempt.

A greeting, a status question or a one-request answer needs no task.

## How it behaves

### How a task starts

```text
Person: "Fix the null-cart error until PR 88's checks pass"
   │
queued turn ─► ordinary run ─► fleet's model calls
   task_start {title, source_turn_id, source_start, source_end, target, statements}
   │
POST L/durable_tasks/bindings   (one item)
   │
daemon checks source turn, span, target, statements, limits
   │
one transaction: the turn's plan + one task item
                 + task (ready) + staged continuation turn
   │
calling run stays ordinary and ends ─► continuation runs as the task's first attempt
```

`L` is the run route prefix `/v1/runners/me/runs/{run_id}`, and `F` is `/v1/workspaces/{workspace_id}/fleets/{fleet_id}`.
`task_start` and `task_input` are offered only when the runner is task-capable and the run carries a decision profile.
An ordinary run's payload lists the fleet's unfinished tasks, so the model can name one.

| Tool | Item it posts | What the daemon commits |
|---|---|---|
| `task_start` | A new-work item | The turn's `plan`, one item row, a `ready` task whose `questions` are the call's statements, and a staged continuation turn |
| `task_input` | A steer item for an unfinished task | One input under the task's current generation, which cancels the task's wait and queues its continuation |

The daemon refuses a binding, writing nothing, unless every check holds:

- The source turn is the run's head turn or one folded into it.
- The source turn is a person's message: a signed-in person's, or an allowed Slack person's mention. A webhook, schedule or API key turn is refused `UZ-DT-004`.
- The objective is a span inside that message.
- A new item's target is valid and holds at least one statement, each with a stable key and nonblank text.
- The task limits and the unfinished-task cap hold; past the cap the answer is `UZ-DT-005`.
- A steer names an unfinished task of this fleet; anything else is `UZ-DT-001`.

One source turn takes one plan, written once (`trg_turns_refuse_plan_change`).
The same call again returns the first task; a different call naming the same turn is refused `UZ-DT-003`.
A refusal returns to the model, which may correct its call ([capabilities](capabilities.md#tools-that-change-the-fleet)).

### Completion statements

The statements are the task's own `questions`, stored on `fleet.durable_tasks` beside their `goal_digest`.
The fleet's model writes them in its `task_start` call, from the outcome the person described.
Each holds a stable key, a yes/no question, and what counts as `true` and as `false`.

A key is 1 to 64 ASCII letters, digits, `_`, `.` or `-`, unique within the task.
Question and criterion text is nonblank, holds no NUL byte and fits the text ceiling.
An empty list, a duplicate key or a missing criterion is refused, never filled in.

A person edits the statements on the task panel, which sends `PATCH F/durable_tasks/{id}` with `{questions, expected_generation}` and `If-Match`.
The edit writes new `questions` and `goal_digest` and raises the generation by one, so a decision prepared earlier is stale.
A stale tag answers 412 `UZ-DT-013`, a missing tag 428 `UZ-SETUP-004`, and an API key 403.

### How a task runs

Each attempt is a run of a continuation turn: `TurnKind::Continuation`, producer `DurableTaskContinuation`, actor `durable_task:<id>`.
The run gets a slice of at most ten minutes, further limited by the task's remaining running time.
The runner stops starting new work with three minutes left: one minute for the goal check, two for checkpoint and report.

At the end of each attempt, the decision model answers the task's statements and picks the next action.
The daemon then settles the task.

| Next action | What the daemon does |
|---|---|
| `CONTINUE` | Queues another continuation turn |
| `WAIT` | Arms a wait on the bound pull request's current head and frees the thread |
| `DONE` | Ends the task, once every statement passes and native checks agree |
| `BLOCKED` | Holds the task for a named prerequisite, permission or piece of evidence |

Waiting frees the fleet's thread, so other turns and tasks run meanwhile.
A task holds one wait, stored on its own row (`fleet.durable_tasks.wait_*`) with the head and check results read when it was armed.
A `task_input` from a person cancels the wait and queues the next attempt.

Each attempt holds the grants approved for the fleet when that run is issued.
A grant approved while the task waits reaches its next run, and a revoked one is gone from it.
`done`, `cancelled` and `exhausted` are final; a greeting or late event cannot revive them.

### Goal evaluation

The decision model only judges whether a task is done.
It never starts a task, links a message to one or widens access, and a plain message makes no decision call.
Its answers are typed probabilities, so the daemon compares numbers against thresholds instead of parsing prose.

| Primitive | Meaning | Use in the goal check |
|---|---|---|
| Noul | Probability, 0 to 1, that a yes/no statement is true | One per completion statement |
| Choice | The picked option with per-option probabilities and a confidence | The next action: `CONTINUE`, `WAIT`, `DONE` or `BLOCKED` |

A Noul near 0.5 means uncertain, not missing.
The daemon compiles each request from the task's statements, one fixed `next_action` question and the task state (`durable_task/decision/compile.rs`).
The objective, a span of the person's message, gives the business context.

The runner sends the completed action group whole first.
If the daemon refuses it as larger than the profile's `input_max_bytes` (`UZ-REQ-002`, before any reservation), the runner sends one summary of it instead.
A summary never satisfies a native publication or exact-head check.

The daemon accepts a report only for its recorded goal call and that call's profile thresholds (`decision/authorize.rs`).
The picked action must meet its probability and confidence thresholds, or the report is refused as ambiguous (`UZ-DT-007`).
`DONE` also needs every statement's Noul at or above `statement_min_bps`, and current-head evidence a model's confidence cannot replace.

Thresholds live in the profile's `GoalDecisionPolicy`, as integer basis points under question revision `dt-goal-v1`.
Each profile carries its own numbers; nothing in code sets a default.

### Decision profiles

A decision profile names the provider, model, endpoint, vault reference, rates, thresholds and input limit.
Profiles are reviewed revisions in the model catalogue (`model.decision_revisions`).
An operator selects one per runner with `decision_model_id` on the runner's assigned policy.

```text
operator PATCH /v1/admin/runners/{id} {assigned_policy.decision_model_id}
        ─► heartbeat ─► run issue freezes the profile in thread.runs.decision_snapshot
        ─► each goal call uses that snapshot
```

An edit reaches later runs; a call in flight keeps its recorded profile.
A run with no profile, or one the runner cannot serve, is offered no task tools.
A replacement runner may use another profile without changing the task's objective, history or limits.

One `DecisionEvaluator` trait in `af_providers` has one implementation, `DecisionClient`, with four wire adapters: TypeSafe, OpenAI, Perplexity and Cloudflare.
The daemon resolves the provider key from the platform-admin workspace's vault, chosen by `PLATFORM_ADMIN_WORKSPACE_ID`.
There is no tenant-key or runner-environment fallback, and the key never reaches the sandbox, the conversation or a checkpoint.

### Paying for a decision call

A goal call bills under the issuing run's work identity, as one usage line of kind `decision` with its own rates.
Before issuing a call, the daemon checks the payer's funds for USD 0.02 (`CALL_NANOS`) and reserves nothing.
Each physical call is billed once, on its receipt, whichever of the receipt, renewal or report delivers it.

| Bound | Value | Source |
|---|---|---|
| Connect timeout | 3 seconds | `af_providers` decision client |
| One physical request | 12 seconds | `af_providers` decision client |
| One logical decision | 30 seconds | `durable_task/decision/mod.rs` |
| Retries | One, after a 429 or 529 | `durable_task/decision/attempt.rs` |
| Reply size | The 32 MiB message ceiling | `af_providers::transport` |

An unknown network outcome is never retried, and missing usage is never treated as free.
Timeouts, unsupported replies and provider failures never default to success.

### Human input while a task runs

The daemon records each message before acknowledging it.
Between complete action groups, the runner reads pending input through the current run (`GET L/durable_tasks/inputs`).
The fleet's model links a message to a task with `task_input`, or starts new work with `task_start`.

Accepting an input saves its text and task link in the task's history before the turn leaves the queue.
A later queued run cannot repeat it, and a resumed run reloads accepted inputs past its checkpoint.
An input changes the method inside the accepted objective; it grants no new repository access and no new statements.

New work found during a task becomes another task.
An input for a different task of the same fleet is queued for that task and grants the current run nothing.

### Publishing to a pull request

The runner proposes a change; a publisher in the daemon writes it.
The publisher calls GitHub's `createCommitOnBranch` with `expectedHeadOid`, so a head a person advanced refuses the write.
The model never holds the credential that advances the branch.

| Mode | When | What the publisher may do |
|---|---|---|
| Existing pull request | The fleet's GitHub grant covers an open same-repository pull request | Append commits to that pull request's branch |
| Turn-owned branch | Otherwise | One branch and one draft pull request |

The publisher refuses fork heads, default or protected target branches, workflow edits, force pushes, merges and deployments.
Changes support ordinary `100644` files; symlinks, submodule changes and other modes block publication.
Before sending, the daemon rebuilds the candidate tree from its base and compares it with the tree the runner tested.

The daemon saves each effect, its expected head and request digest before the remote call.
Only one unresolved publication may be outstanding per task.
After a lost acknowledgement, the effect is `unknown` (`UZ-DT-010`) until a read of the branch settles it; nothing resends blindly.

### Recovery after worker loss

A task's run saves a checkpoint of its files, Git state and covered conversation before yielding.
A replacement run restores the newest compatible checkpoint, reconciles recorded effects, then continues under a new fence.
A failed save keeps the previous checkpoint and reports a blocked handoff.

[`checkpoints.md`](./checkpoints.md) owns checkpoint storage, encryption and restore.

### Reading and controlling a task

| Route | Who | What it does |
|---|---|---|
| `GET F/durable_tasks` | `FleetRead` | Lists unfinished tasks: title, state, version, next dependency |
| `GET F/durable_tasks/{id}` | `FleetRead` | Returns progress, statements, generation, evidence and an ETag |
| `PATCH F/durable_tasks/{id}` | `FleetWrite`, a signed-in person | Pauses, resumes or cancels, or edits the statements, under `If-Match` |

The runner-side routes live under `L/durable_tasks/`: bindings, inputs, input receipts, decisions, effects and checkpoints.
[`api_resources.md`](./api_resources.md) and `public/openapi.json` hold their request and response shapes.

The browser's `DurableTaskPanel` shows each task's statements under "What counts as done?" and lets a person edit them.
Changes publish `durable_task_changed` on the fleet's stream; the browser reads the task again, since frames carry no authority.

## Limits

Each task saves its own allowances when it starts.
Smaller run limits and the payer's balance always win, and no fleet has a budget of its own.

| Allowance per task | Default | Source |
|---|---|---|
| Paid-model spend | USD 5 | `DurableTaskLimits::default()` |
| Wall deadline, waits included | 7 days | same |
| Running time | 2 hours | same |
| Runs | 12 | same |
| Publications | 8 | same |
| Decision calls, retries included | 128 | same |

`[fleet.durable_tasks]` caps the rest, and the code default for each is no cap.
Production sets 16 unfinished tasks per fleet, 4 inputs per read, 4 actions per message and 6 statements per task.

Gates compare spent against the maximum and reserve nothing, so concurrent decision calls can pass the model allowance by one call's cost.
A decision call counts against its allowance once, on its first report.

What is not built:

- Nothing wakes a wait when CI finishes; a waiting task stays waiting until a person sends a `task_input` or its deadline passes.
- No timer pass runs for tasks: nothing reclaims abandoned input claims, reconciles unknown effects or collects checkpoint objects.
- An expired task is simply no longer assigned.
- A message cannot name a task directly; only the fleet's model links it with `task_input`.

| Code | HTTP | Meaning |
|---|---|---|
| `UZ-DT-001` | 404 | No such task in this workspace and fleet |
| `UZ-DT-002` | 409 | The task changed or no longer allows this transition |
| `UZ-DT-003` | 409 | A replay with different content |
| `UZ-DT-004` | 409 | No valid target, or the source turn is not a person's |
| `UZ-DT-005` | 429 | An allowance or the unfinished-task cap is spent |
| `UZ-DT-006` | 503 | No reviewed profile, confirmed funds or platform key |
| `UZ-DT-007` | 409 | An ambiguous answer or association |
| `UZ-DT-008` | 409 | The checkpoint cannot restore this task generation |
| `UZ-DT-009` | 503 | Checkpoint storage is unavailable |
| `UZ-DT-010` | 409 | A publication may have happened; reconcile first |
| `UZ-DT-013` | 412 | A stale resource tag |

## Related

- [`checkpoints.md`](./checkpoints.md): task checkpoints and restore.
- [`api_resources.md`](./api_resources.md): the task routes.
- [`billing.md`](./billing.md): how usage lines debit a wallet.
- [`observability.md`](./observability.md): `dt_transition`, `dt_decision_recorded`, `dt_decision_funding_refused` and `dt_effect_uncertain`.
- [`web_app.md`](./web_app.md): the task panel in the fleet chat.
- [`connectors.md`](./connectors.md): the GitHub grant that selects a publication mode.
