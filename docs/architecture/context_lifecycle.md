---
type: explanation
audience: contributor
verified: 2026-11-25
product_version: 0.90.0
executable: false
---

# Context lifecycle: compacting a conversation near the model's window

> Read this when a fleet's run approaches the selected model's context window. Parent: [`README.md`](./README.md) · User-facing: [docs.agentsfleet.net/concepts/context-lifecycle](https://docs.agentsfleet.net/concepts/context-lifecycle).

## What it is

Compaction replaces the older part of a long conversation with a model-written summary, so the same run keeps going.
The runner does it before a model request would overflow the model's context window, the most input the model accepts.
The `af_context` crate plans it; the runner makes the model call; the daemon only stores the result.

| Fact | Value | Source |
|---|---|---|
| Compaction fires at | 90 percent of the window, or earlier when input plus reserved output passes 95 percent | `af_context::window` |
| History read in pages of | 64 complete groups | `af_context::plan` |
| Recent groups kept verbatim | Up to 2 | `af_context::plan` |
| Summary output | At most 2,048 tokens; at most 32,768 bytes after validation | `af_context::summary` |
| Saved summaries | One `thread.compactions` row per thread | `20261108123655_thread_compactions.sql` |
| Who saves one | A fleet chat's root run, through the `context_load` and `context_save` stream verbs | `afr_agent` loop context |

A group is one complete exchange: an assistant message with its tool results, or one authored message.

## Why it exists

A fleet chat can run for weeks, and no model window holds all of it.
Cutting old messages by count loses decisions and breaks tool call and result pairs.
A checked summary keeps the goal, the decisions and the open work, and the run continues with its tools.

## How it behaves

### Deciding to compact

```text
history + fixed prompt + pinned memory + current request
                        │
            measure the provider request
                        │
                fits the window? ──yes──► send the model call
                        │ no
                        ▼
    summarize an older prefix of groups (one model call, tools off)
                        │
       validate ──► save under the run's fence (fleet chat only)
                        │
                        └──► measure again, then send
```

The planner checks every physical request: ordinary turns, tool continuations and summary calls.
After a summary is published, it measures the shorter request again before sending it.

### Producing a summary

The planner reads history oldest first, in pages of 64 groups, and never splits a group.
At the end of history, it keeps up to two recent groups verbatim and summarizes an older prefix.
At least one group must go into the summary.

Selection follows time order; nothing ranks messages by keyword or importance.

If the summary request itself is too large, the planner halves the prefix until it fits.
Groups left out stay in the original history for the next compaction.
If no complete group fits, preparation returns a window error and sends nothing.

The selected conversation model gets one request with tools turned off and fixed instructions from `af_context`'s summary module.
The input quotes the previous summary and the selected groups as JSON.
The instructions ask for the goal, constraints, decisions, completed work, remaining work and exact references, with open failures kept.

The reply must hold only the expected JSON fields and pass validation.
A partial reply, a tool call, a malformed summary or missing usage publishes nothing.
Validation proves shape and source identity, never that every relevant fact survived.

### Measuring tokens

`guffbolt_ai::wire::input_tokens` counts tokens with `tiktoken-rs` for models whose encoding it knows.
Any other model gets the `o200k_base` count plus a quarter, rounded up (`PROXY_HEADROOM_PERCENT`).
The count covers the serialized request, including tools and replay data, plus 32 tokens per history message.

Near the limit, the run can ask the provider's own count endpoint, through the Responses, Messages or Gemini API.
That check starts at 80 percent of input plus reserved output and has a ten-second deadline.
A failed count stops that request; it never becomes a zero count or a paid retry.

The estimate only decides fit; provider-reported usage is what the payer is charged.
The `model_input_calibration` trace records each call's estimate beside the provider's reported input.

### What the model sees after compaction

The summary goes in as untrusted conversation data, before the retained groups and the current request.
The fixed prompt text, the pinned memory and the current request are never summarized.
Image blocks in retained tool results stay; native replay recorded before the new prefix is dropped.

### Where a summary is saved

Only a fleet chat's root run saves: a `chat` turn not bound to a durable task or a recovery.
It loads its thread's row with `context_load` before its first model call and skips the turns that row covers.
Each later compaction that covers earlier turns whole saves the next row with `context_save`.

Every other run keeps its summary in its own memory and stores none.
A loaded row is used only when its fleet, model and prompt identity match, so a model change starts from the turns.
A failed load keeps the run's summaries in memory and saves nothing.

The save is a compare-and-swap on the row's `resource_version`, written only while the run holds the fleet's thread at its fence.
A superseded run's save is refused with `RUN_STALE_FENCING_TOKEN`, which fails that run; any other failed save is logged.
The checkpoint store lives in `afd_setup`'s `context` module, and the row is deleted with its thread.

### What a person sees

The runner sends `context_progress` on the fleet's activity stream, with a generation and one of three states.

| State | Label in the browser |
|---|---|
| `summarizing` | Compacting context · This can take a few minutes |
| `resuming` | Context compacted · Continuing… |
| `interrupted` | Context compaction interrupted. |

The browser takes notices only for a known running message and ignores older generations.
Reconnect, cancellation or completion clears the label, and reply text replaces it.
Summary text never reaches the browser.

### Traces

`af_context::Trace` opens a `context.compact` span around each summary and its save.
The events are `context_compaction_started`, `context_compaction_completed` and `context_compaction_failed`; completion is recorded only after the save commits.
They carry the operation identifier, generations, group count, reserved output, elapsed time and error code, never prompts or summary text.

## Limits

- Compaction spends model calls and output tokens, metered like any other call; a refused save keeps its charge.
- The payer's balance, output limits and a person's Stop still end work.
- A request that cannot fit even after compaction fails before any provider call.
- The `o200k_base` estimate is not an upper bound; a model whose tokenizer makes a third more tokens can pass its window first.
- Saved summaries are ordinary database JSON, not encrypted per document.
- Debug output omits summary contents, but ordinary strings holding conversation text are not wiped on release.
- A summary cannot select a pull request, authorize a write or prove a remote action finished.
- Compaction does not depend on the sandbox engine.

## Related

- [`memory.md`](./memory.md): saved facts across runs, separate from summaries.
- [`runner_execution.md`](./runner_execution.md): the chat history each run carries.
- [`runner_fleet.md`](./runner_fleet.md): steering, and how a message folds into a live run.
- [`durable_tasks.md`](./durable_tasks.md): how a task's run resumes from a checkpoint.
- [`../reviews/pr784-tokenizer-evaluation.md`](../reviews/pr784-tokenizer-evaluation.md): the token-counting comparison.
