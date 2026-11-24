# Capabilities (what the fleet has, what the platform guarantees)

> Parent: [`README.md`](./README.md). User-facing: [docs.agentsfleet.net/fleets/tools](https://docs.agentsfleet.net/fleets/tools) (the tool inventory) and [docs.agentsfleet.net/concepts/context-lifecycle](https://docs.agentsfleet.net/concepts/context-lifecycle) (§4 as a user reads it).

> **Scope:** the platform *guarantees* below are binding. The *mechanism* runs on the split. `agentsfleetd` (control plane) assigns work and resolves policy on `run`.

> The host-resident `agentsfleet-runner` enforces the hard layer in its supervisor and the run's sandbox ([`runner_execution.md`](./runner_execution.md#process-model)). See [`runner_fleet.md`](./runner_fleet.md) and [`data_flow.md`](./data_flow.md).

A Fleet's capabilities split into two layers. The soft layer is what the language model is told it can do, which the model can ignore or get wrong. The hard layer is what the platform enforces, which the model cannot escape from inside the sandbox.

Both matter; the second is what makes the first safe.

---

## Facts

Every row is extracted from the numbered sections below; the owner column names the section that carries the full story.

| Invariant | Value | Mechanism | Owner section |
|---|---|---|---|
| Capability layers | 2 | the fixed prompt text, the pinned memory and the conversation are advisory; the run's policy, built from the fleet row and its approved grants, is binding, enforced by the runner | §1 |
| A trigger's `kind` vs `event_type` | orthogonal fields | different homes and meanings (`fleet.triggers` vs `thread.turns`); some values overlap | §1.1 |
| Tool primitives | the runner's published catalog, e.g. `http_request` · the `file_*` tools · `git` · the four `memory_*` tools · `shell` (gated) | every run is offered every published tool and the seven sub-agent tools; `task_start` and `task_input` need a decision profile | [`runner_execution.md`](./runner_execution.md) §"Tool catalog" |
| Mintable integrations | short-lived tokens, minted per run | GitHub App: daemon-signed RS256 JSON Web Token (JWT) exchanged for a ≤ 1 h installation token; the App private key never leaves the daemon | §3 |
| Vault secret shapes | 2 | static (stores the value, resolved at run) vs mintable (stores a handle, never a token) | §3 |
| Context lifecycle | Shared conversation compaction; separate semantic memory | Compaction defaults to 90 percent; output headroom can trigger it earlier. Semantic memory pushes every five tool calls. | §4 |
| Continuation authority | guarded private checkpoint | Source and assignment checks guard saved working notes; valid compaction continues the same run with tool access. | §4 |
| Runaway bound | the payer's balance + run runtime deadline | no fleet has a budget; there is no enforced continuation-chain counter | §4 |
| Memory hygiene | `core` pinned into every prompt; `daily` expires | `memory_store` defaults to `note`; `daily` rows older than 72 h are deleted on the next capture push; re-storing a key is an upsert | §4 |
| Cap resolution | saved catalogue window, read per run | the fleet's `model_ref` selects the model; a fleet carries no context override | §4 |

## Traps

Each trap is enforced in its owner section; this list is the index. §5 (What the platform never does) is the binding never-list.

- The prompt is advisory and the run's policy is binding; never treat a message or a pinned entry as enforcement (§1).

- A trigger's `kind` and a turn's `event_type` have different meanings and homes; values such as `webhook` overlap (§1.1).

- A fleet's own schedules go through a runner verb onto `agentsfleetd`'s schedule plane. Upstash QStash, the hosted scheduler (`afd_cron`), still owns the clock, and the runner receives only the resulting event ([`runner_execution.md`](./runner_execution.md) §"Tool catalog").

- The runtime cannot force a chunk; do not design as if a mid-loop interrupt exists (§4).

- A fleet that pins everything as `core` crowds its own prompt. Once `core` alone fills the load budget, the oldest entries stay unloaded and out of the prompt (§4).

- The GitHub App private key never rides the run envelope, the `secrets_map`, or the runner (§3, §5).

## 1. What the model is told, and what the run holds

A fleet has no written job description. What its model reads and what its run may do come from different places, and they carry different weight:

| Part | What it carries | Enforced by |
|---|---|---|
| Fixed prompt text | `agentsfleet`'s own text, with the fleet's name filled in (below). | The language model reading its own prompt, which is soft enforcement only. The model can drift; the platform-level guarantees below contain the consequences. |
| Pinned memory | Every `core` entry the fleet stored, under one heading after the fixed text ([memory](./memory.md#6-what-reaches-the-model)). | The model, as above. A pinned entry grants nothing. |
| The conversation | The thread's turns and the run's own tool results. | The model, as above. |
| The run's policy | Built by `afd_gate` from the fleet row and its approved grants: every published tool, the approved credentials, the GitHub repository binding and the network the fleet's toggles allow. | Code-enforced by the runner's supervisor, outside the model's reach. The language model cannot escape these. |

The prompt builder (`afr_agent`'s `prompt` module) writes this fixed text, then the pinned memory:

```text
You are {name}, a fleet in agentsfleet: a long-lived agent a team talks to in one chat.
Remember standing guidance with memory_store under the category core, only when the person's newest message asks you to.
It is shown to you at the start of every run. Store anything else under another category.
Schedule yourself with the cron tools, add a webhook, GitHub or Slack trigger with trigger_add, and start work that must
keep going with task_start, each only when the person's newest message asks.
You hold only the secrets and integrations a person approved for you. When you need one, call request_access with its name
(and, for GitHub, the repositories and read or write); the person approves it once.
```

The split matters. The prompt is *advisory*: the model reads it and tries to comply. The run's policy is *binding*, and only a person's approval adds a credential to it.

A pinned entry stays in every later prompt until a person removes it. That holds for an entry a hostile page talked the fleet into, so the fleet page lists every pinned entry with remove.

Every run is offered every published tool and the seven sub-agent tools, and the runner refuses any call outside the policy ([`runner_execution.md`](./runner_execution.md#tool-catalog)). `notify`, `git` and `message` fail until their credential is approved. `task_start` and `task_input` are offered only when the runner is task-capable and the run carries a decision profile ([durable tasks](./durable_tasks.md#how-a-task-starts)).

Each tool that changes a fleet says in its description to call it only when the person's newest message asks. These are `memory_store` under `core`, `cron_add`, `trigger_add`, `request_access`, `task_start` and `task_input`. A refused call returns to the model as `[kind] sentence`, and the model may correct it in the same run.

### 1.1 A trigger's `kind` vs `event_type` (orthogonal fields, common confusion)

These are two different fields on two different tables. Mixing them up is the most common source of new-contributor confusion in this area:

| Field | Where it lives | Values | What it tags |
|---|---|---|---|
| `kind` | `fleet.triggers.kind`, one row per trigger the fleet added | `webhook` / `app_event` / `mention` | **How a Fleet gets woken**: the wiring the fleet added with `trigger_add`. |
| `event_type` | `thread.turns.kind` column (per turn), named `event_type` on the wire | `chat` / `webhook` / `cron` / `continuation` | **What an individual turn is**: the runtime label per input. |

A fleet needs no trigger to receive `event_type: chat` from the steer API.
A fleet with a `webhook` trigger receives `event_type: webhook`.
A schedule's fire arrives as `cron`, and no trigger row holds a schedule.

A person's answer to a question or an access card creates `event_type: continuation`, whatever woke the original turn (`afd_approval`'s inbox resolve).

Source of truth: `fleet.triggers.kind` for a trigger; `TurnKind` in `af_wire` for `event_type`.

---

## 2. The platform tools the fleet can call

The runner hosts every published tool, and every run is offered all of them (§1; [`runner_execution.md`](./runner_execution.md) §"Tool catalog"). Placeholders resolve at send time in two places only. They are the `Authorization` header's value and the URL's whole host (`${secrets.NAME.host}`), in `af_egress` admission.

---

## 3. Platform-level guarantees (the substrate that wraps every tool call)

| Capability | What it does | Primary owner |
|---|---|---|
| Work assignment + archive | `agentsfleetd` assigns the next turn as a run (atomic thread-row lock + monotonic fencing token; status, model and grants resolved fresh from Postgres per run). Archive and delete stop new runs and send Cancel to a running one ([`runner_fleet.md`](./runner_fleet.md) §"Steer, archive, delete"). | agentsfleetd control plane |
| Per-run policy | Each `Down::Run` frame's `RunPayload` carries an `ExecutionPolicy` (tools, secrets, allowed hosts, context knobs), built from the fleet row and its approved grants, that the runner's supervisor enforces outside the sandbox. The fleet's two toggles set its `NetworkPolicy` inside the runner's assigned posture (`allow_all`, `deny_all_egress` or `allow_list_egress`), which stays the operator's ceiling ([`runner_fleet.md`](./runner_fleet.md#egress-model-outbound-is-the-only-network-surface)). | Run ExecutionPolicy |
| Turns ledger + history | Every steer / webhook / cron input commits a queued `thread.turns` row with actor provenance. The turn is opened when a run first takes it and settled at completion. | Turn ingest + history path |
| Webhook ingest (GitHub App + manual) | Both receivers verify the signature, bind the delivery to the fleets whose `fleet.triggers` rows match inside the install's workspace, and refuse replays before queueing. The App records actor `github-app`; the manual receiver records `webhook:<source>` ([`connectors.md`](./connectors.md) §"GitHub App: platform setup to fleet runs"). | App ingress + webhook receiver |
| Secret vault (`vault.secrets`) | Stores opaque-JSON-object secrets. Each row is encrypted with its own data encryption key (DEK), which is sealed by the daemon's key encryption key (KEK), the `ENCRYPTION_MASTER_KEY` boot setting (`af_crypto`). Two shapes: a **static** secret stores the value itself (resolved at run, shipped inline); a **mintable integration** stores only a *handle* (e.g. `{integration:"github", installation_id}`), never a token, which the credential broker exchanges for a short-lived token on demand. The runner's supervisor substitutes it at send time. A run receives only the secrets a person approved for its fleet ([grants](./connectors.md#where-a-grant-comes-from)). | Vault + secret resolution |
| Integration credential broker | `agentsfleetd` mints a short-lived, workspace-scoped token per mintable integration on demand, reading the fleet's current grants; the App private key never leaves the daemon ([`connectors.md`](./connectors.md) §"Credential use remains separate from event receipt"). | Credential broker (daemon-side) |
| Provider config (self-managed) | A tenant chooses platform-managed inference or its own provider key. The api_key crosses vault → run `ExecutionPolicy` → runner's supervisor → outbound HTTPS and never reaches a user-facing surface ([`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §"8.2 The api_key visibility boundary"). | Provider resolution path |
| Questions and access cards | A run asks a person with `ask_user`, or asks for a credential with `request_access`. Either raises a card on the fleet; the run ends waiting and its turn settles waiting, so later turns are not held. A signed-in person answers in the dashboard, through the API, or with `agentsfleet fleet approvals approve`, and the answer writes a continuation turn that resumes the asking turn. No rule holds an event for a person. The card and the turn are Postgres rows, so both survive control-plane and runner restarts. | Approval workflow |
| Wallet check | A turn is refused `balance_exhausted` when the payer's balance is empty or the payer has no wallet row. No fleet has a budget of its own ([`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §"5. The balance gate: code path"). | Billing gate |
| Per-run context lifecycle | Shared compaction and guarded working notes let the same run continue. See §4. | Context lifecycle |

---

## 4. Context lifecycle — keeping a long incident reasoning past the model's working-memory limit

The runner compacts older conversation before a model request exhausts its working window, then continues the same run.
Semantic memory has its own save cadence for facts used by later runs.

### Context defaults

A fleet carries no context settings of its own. `afd_gate`'s context resolver reads the selected model's saved catalogue entry and applies these defaults, with no override:

- The physical window and maximum output come from the model's saved entry ([model origin](./user_flow.md#87-model-and-context-cap-origin-platform-vs-self-managed)).
- Compaction fires at 90 percent of the window, or earlier when output headroom needs it.
- The runner pushes already-stored semantic memory every five completed tool calls. The push does not ask the model to create a snapshot.

Whole tool call/result groups follow shared compaction rather than count-based eviction.

### How continuation works

```text
  Original history + fixed prompt + pinned memory + current request
                            |
                   Measure provider request
                            |
                    Fits working window?
                      /             \
                    yes              no
                     |                |
                Model turn      Summarize older groups
                     |                |
                Tool results    Save guarded checkpoint
                     |                |
                     +-------> measure again
```

Fleet runs use the shared [conversation compaction engine](context_lifecycle.md).
That page owns thresholds, summary validation, checkpoint ownership, source guards and native replay rules.

The runner keeps complete tool call/result groups together.
A summary disables tools and contributes real usage to the run's meter.
A valid checkpoint lets the current run continue with its goal, current request and tool access.

### Semantic memory remains separate

Every five completed tool calls, the runner pushes entries the model already stored.
An empty pending set writes nothing.
A failed push is logged; the final push before the report carries the entries again (`afr_agent`'s loop conversation).

Semantic memory supplies facts for later runs.
Conversation checkpoints retain working notes for the current run.
Neither summary text nor quoted prior history can replace the fixed prompt text or the pinned memory.

### Limits and defaults

The compaction threshold defaults to 90 percent; semantic memory pushes default to every five tool calls.
Output headroom can trigger earlier compaction, and the saved model still supplies physical headroom for the summary.

Compaction consumes model calls and output tokens.
The payer's balance, the run deadline, output limits and a person's Stop can still stop work.
A pinned request that cannot fit the physical window fails before provider access.

### Memory hygiene (what to store so it survives)

Durable memory is selected rather than searched. Loading takes the `core` category first (newest-first, within the byte budget), then fills the remaining budget with the newest other entries.

No entry count caps a fleet's memory. What does not fit the budget stays in Postgres and is not loaded (`afd_memory`'s `window.rs`; see [`runner_fleet.md`](./runner_fleet.md) §"Memory continuity").

Every loaded `core` entry of the fleet's own also goes into the run's prompt; everything else is recall-only ([what reaches the model](./memory.md#6-what-reaches-the-model)).

Four habits make that selection work for the fleet instead of against it:

- **Pin standing guidance as `core`, only when asked**. `memory_store` stores under `note` unless the call names a category, and only an explicit `core` is pinned. Owner, deploy target and standing constraints a person states belong in `core`.

  `note`, `daily`, `conversation` and any custom category are windowed by recency and are the first to age out of loading. `daily` additionally expires outright: rows older than the 72-hour retention window are deleted on the fleet's next capture push (`afd_memory`). Every other category stays until a store under its key replaces it; `memory_forget` hides a key for the current run only (`afr_memory`).

- **Reuse stable keys.** Re-storing a key is an upsert, so it refreshes the entry instead of duplicating it. `deploy_target` beats a dated `deploy_target_jun12`.

- **Replace what's stale**. A store under the same key replaces the stored copy, while `memory_forget` only hides a key for the rest of the run.

  A Fleet that pins everything as `core` fills its own prompt. When the `core` entries alone pass the budget, the oldest `core` entries stay unloaded and out of the prompt.

- **Keep entries small.** The loading budget is bytes rather than entries, so one bloated entry crowds out many small ones. Snapshot conclusions, never transcripts.

The selection is deterministic and documented, so "why does my fleet remember X but not Y?" has a checkable answer. Y was windowed (or oversized), and X was `core`.

---

## 5. What the platform never does

- Never logs raw secret bytes
- Never echoes secrets in the fleet's context
- Never saves secrets in `thread.turns`
- Never lets the GitHub App private key leave the daemon. The key is platform-side, and the run envelope, the `secrets_map`, and the runner never carry it
- Never issues a run a credential no person approved for its fleet
- Never sends a stored secret to any host but its own `host` field
- Never lets a private fleet's HTTP tools reach a host outside its approved secrets' hosts and, once approved, GitHub
- Never lets a fleet's network exceed the runner's assigned posture
- Never starts a run for a payer whose balance is empty or who has no wallet row
