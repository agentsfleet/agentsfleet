# Capabilities — what the fleet has, what the platform guarantees

> Parent: [`README.md`](./README.md) · User-facing: [docs.agentsfleet.net/fleets/tools](https://docs.agentsfleet.net/fleets/tools) (the tool inventory) and [docs.agentsfleet.net/concepts/context-lifecycle](https://docs.agentsfleet.net/concepts/context-lifecycle) (§4 as a user reads it).
>
> **Scope:** the platform *guarantees* below are binding and unchanged by the M80_002 cutover. The *mechanism* now runs on the split — `agentsfleetd` (control plane) assigns work + resolves policy on `lease`; the host-resident `agentsfleet-runner` enforces the hard layer in its supervisor and the lease's sandbox ([`runner_execution.md`](./runner_execution.md#process-model)). See [`runner_fleet.md`](./runner_fleet.md) and [`data_flow.md`](./data_flow.md).

A Fleet's capabilities split into two layers: what the language model is told it can do (a soft layer the model can ignore or get wrong), and what the platform actually enforces (a hard layer the model cannot escape from inside the sandbox). Both matter; the second is what makes the first safe.

---

## Facts

Every row is extracted from the numbered sections below; the owner column names the section that carries the full story.

| Invariant | Value | Mechanism | Owner section |
|---|---|---|---|
| Capability layers | 2 | the Definition's `instructions_md` is advisory; its reviewed `config` plus approved grants are binding, enforced by the runner | §1 |
| `trigger.type` vs `event_type` | orthogonal fields | different homes and meanings (the setup's `config` vs `fleet.events`); some values overlap | §1.1 |
| Tool primitives | the runner's published catalog, e.g. `http_request` · the `file_*` tools · `git` · the four `memory_*` tools · `shell` (gated) | reachable only through the fleet's `tools:` allowlist | [`runner_execution.md`](./runner_execution.md) §"Tool catalog" |
| Mintable integrations | short-lived tokens, minted per lease | GitHub App: daemon-signed RS256 JSON Web Token (JWT) exchanged for a ≤ 1 h installation token; the App private key never leaves the daemon | §3 |
| Vault secret shapes | 2 | static (stores the value, resolved at lease) vs mintable (stores a handle, never a token) | §3 |
| Context lifecycle | Shared conversation compaction; separate semantic memory | Compaction defaults to 90 percent; an authored operating threshold or output headroom can trigger earlier. Semantic memory pushes every five tool calls. `tool_window` remains accepted metadata. | §4 |
| Continuation authority | guarded private checkpoint | Source and assignment checks guard saved working notes; valid compaction continues the same run with tool access. | §4 |
| Runaway bound | `budget` caps + lease runtime deadline | there is no enforced continuation-chain counter | §4 |
| Memory hygiene | `core` pinned; `daily` expires | `daily` rows older than 72 h are deleted on the next capture push; re-storing a key is an upsert | §4 |
| Cap resolution | saved catalogue window, read per lease | the revision's `runtime_model_ref` selects the model; an absent or zero operating cap inherits its physical window | §4 |

## Traps

Each trap is enforced in its owner section; this list is the index — §5 (What the platform never does) is the binding never-list.

- Instructions are advisory and `config` is binding; never treat prose as enforcement (§1).
- `trigger.type` and `event_type` have different meanings and homes; values such as `webhook` overlap (§1.1).
- A fleet's own schedules go through a runner verb onto `agentsfleetd`'s schedule plane — Upstash QStash, the hosted scheduler, still owns the clock, and the runner receives only the resulting event ([`runner_execution.md`](./runner_execution.md) §"Tool catalog").
- The runtime cannot force a chunk; do not design as if a mid-loop interrupt exists (§4).
- A fleet that hoards everything as `core` defeats the pinning — an over-cap all-`core` set evicts the coldest `core` (§4).
- The GitHub App private key never rides the lease envelope, the `secrets_map`, or the runner (§3, §5).

## 1. Reasoning + tool inventory (declared in the fleet's own files)

A fleet's setup is one reviewed Definition (`rustd/crates/afd_fleet_runtime/src/definition/mod.rs`). Its parts carry different weight:

| Part | What it carries | Enforced by |
|---|---|---|
| `instructions_md` | Natural-language reasoning prompt: how to think, what's safe, what to gather, when to ask for approval. Free-form prose, required before Save. | The language model reading its own prompt, which is soft enforcement only. The model can drift; the platform-level guarantees below contain the consequences. |
| `config` | The policy document. Under `x-agentsfleet` it carries `triggers` (`webhook` / `cron` / `mention`), `tools`, `credentials`, `network`, `budget` caps, `gates`, `repositories`, `context`, and `model`. One parser validates it into `FleetConfig`. | Code-enforced by the runner, outside the model's reach. The language model cannot escape these. |
| `description` | One required line of display text saying what the fleet does. The fleet's name is `config.name`. | Nothing. Display text grants nothing and places nothing. |

The split matters. The instructions are *advisory*: the model reads them and tries to comply. The reviewed `config` and the grants a person approved are *binding*: the runner offers the model only the policy's tools and refuses any call outside them, regardless of what the model wants ([`runner_execution.md`](./runner_execution.md#tool-catalog)).

### 1.1 `trigger.type` vs `event_type` — orthogonal fields, common confusion

These are two different fields on two different tables. Mixing them up is the most common source of new-contributor confusion in this area:

| Field | Where it lives | Values | What it tags |
|---|---|---|---|
| `trigger.type` | the setup's `config` (static, reviewed) | `webhook` / `cron` / `mention` | **How a Fleet gets triggered**: the wiring fixed at Save or a later reviewed revision. |
| `event_type` | `fleet.events` column (per delivery) | `chat` / `webhook` / `cron` / `continuation` | **What an individual admitted event is** — the runtime label per delivery. |

A fleet needs no declared trigger to receive `event_type: chat` from the steer API.
A `trigger.type: webhook` fleet receives `event_type: webhook`.
The fields describe different things; values such as `webhook` overlap.

Resolving an approval gate creates `event_type: continuation`, regardless of the original trigger type (`rustd/crates/afd_approval/src/inbox/resolve/mod.rs`).

Source of truth: `rustd/crates/afd_fleet_runtime/src/config/trigger/mod.rs` (`Trigger`) for `trigger.type`; `rustd/crates/afd_wire/src/event/mod.rs` (`EventType`) for `event_type`.

---

## 2. The platform tools the fleet can call

The runner hosts every published tool; `tools:` selects which the model is offered ([`runner_execution.md`](./runner_execution.md) §"Tool catalog"). Placeholders resolve at send time in two places only: the `Authorization` header's value and the URL's whole host (`${secrets.NAME.host}`) (`rustd/crates/afr_egress/src/admission/mod.rs`).

---

## 3. Platform-level guarantees (the substrate that wraps every tool call)

| Capability | What it does | Primary owner |
|---|---|---|
| Work assignment + kill | `agentsfleetd` assigns the next event on `lease` (atomic affinity claim + monotonic fencing token; status/config resolved fresh from Postgres per lease). Kill stops new leases; a lease already running runs to its end or to its fleet's stored budget ceiling ([`runner_fleet.md`](./runner_fleet.md) §"Steer, kill, pause"). | agentsfleetd control plane |
| Per-lease policy | Each `lease` reply carries an `ExecutionPolicy` (tools, secrets, allowed hosts, context knobs) that the runner's supervisor enforces outside the sandbox. The sandbox's network posture is the runner's assigned `allow_all`, `deny_all_egress` or `allow_list_egress` ([`runner_fleet.md`](./runner_fleet.md) §"Egress model — outbound is the only network surface"). | Lease ExecutionPolicy |
| Admissions ledger + history | Every steer / webhook / cron event commits to `fleet.admissions` with actor provenance. `fleet.events` rows are opened at receive and closed at completion. | Event ingest + history path |
| Webhook ingest (GitHub App + manual) | Both receivers verify the signature, bind the delivery to subscribed fleets and refuse replays before queueing. The App records actor `github-app`; the manual receiver records `webhook:<source>` ([`connectors.md`](./connectors.md) §"GitHub App: platform setup to fleet execution"). | App ingress + webhook receiver |
| Secret vault (`vault.secrets`) | Stores opaque-JSON-object secrets. Each row is encrypted with its own data encryption key (DEK), which is sealed by the daemon's key encryption key (KEK), the `ENCRYPTION_MASTER_KEY` boot setting (`rustd/crates/afd_crypto/src/lib.rs`). Two shapes: a **static** secret stores the value itself (resolved at lease, shipped inline); a **mintable integration** stores only a *handle* (e.g. `{integration:"github", installation_id}`) — never a token — which the credential broker exchanges for a short-lived token on demand. The runner's supervisor substitutes it at send time. | Vault + secret resolution |
| Integration credential broker | `agentsfleetd` mints a short-lived, workspace-scoped token per mintable integration on demand; the App private key never leaves the daemon ([`connectors.md`](./connectors.md) §"Credential use remains separate from event receipt"). | Credential broker (daemon-side) |
| Provider config (self-managed) | A tenant chooses platform-managed inference or its own provider key. The api_key crosses vault → lease `ExecutionPolicy` → runner's supervisor → outbound HTTPS and never reaches a user-facing surface ([`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §"8.2 The api_key visibility boundary"). | Provider resolution path |
| Approval gating | A gated event parks at lease until a workspace member approves it in the dashboard, through the API, or with `agentsfleet approvals approve`; gates judge whole events, never individual tool calls, and nothing is sent to Slack today (`rustd/crates/afd_gate/src/gate/park.rs`, "No pending summary, no staged notification"). The state machine survives control-plane and runner restarts (it is durable in Postgres, gated at `lease`). | Approval workflow |
| Budget caps | Daily and monthly hard caps in integer nanos (`daily_nanos`, `monthly_nanos`), configured per fleet in the reviewed `config`, block further runs at the first trip ([`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §"5.1 The per-fleet budget gate — a second, independent ceiling"). | Billing gate |
| Per-run context lifecycle | Shared compaction and guarded working notes let the same run continue. See §4. | Context lifecycle |

---

## 4. Context lifecycle — keeping a long incident reasoning past the model's working-memory limit

The runner compacts older conversation before a model request exhausts its working window, then continues the same run.
Private setup and fleet runtime share this policy.
Semantic memory has its own save cadence for facts used by later runs.

### Context settings

```json
{
  "x-agentsfleet": {
    "model": "accounts/fireworks/models/kimi-k3",
    "context": {
      "tool_window": "auto",
      "memory_checkpoint_every": 5,
      "stage_chunk_threshold": 0.90,
      "context_cap_tokens": 200000
    }
  }
}
```

The block sits in the fleet's `config`:

- `model` is parsed as setup metadata. The revision's `runtime_model_ref` selects the execution model at lease time; authored model text cannot replace that identity.
- `tool_window` remains accepted setup metadata. Whole tool call/result groups now follow shared compaction instead of count-based eviction.
- `memory_checkpoint_every` pushes already-stored semantic memory every N tool calls. It does not ask the model to create a snapshot.
- `stage_chunk_threshold` selects an earlier compaction threshold; values above 0.90 use 0.90.
- `context_cap_tokens` sets the operating threshold. Zero or absence inherits the selected model's physical window.

### How continuation works

```text
  Original history + current instructions + current request
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

Private setup and runtime use the same [conversation compaction engine](context_lifecycle.md).
That page owns thresholds, summary validation, checkpoint ownership, source guards and native replay rules.

The runner keeps complete tool call/result groups together.
A summary disables tools and contributes real usage to the run's meter.
A valid checkpoint lets the current run continue with its goal, current request and tool access.

### Semantic memory remains separate

Every `memory_checkpoint_every` completed tool calls, the runner pushes entries the model already stored.
An empty pending set writes nothing.
A failed push is logged; the final push before the report carries the entries again (`afr_agent/src/loop/conversation.rs`).

Semantic memory supplies facts for later runs.
Conversation checkpoints retain working notes for the current run.
Neither summary text nor quoted prior history can replace current instructions.

### Limits and defaults

The compaction threshold defaults to 90 percent; semantic memory pushes default to every five tool calls.
An authored operating cap can trigger earlier compaction while the saved model still supplies physical headroom for the summary.

Compaction consumes model calls and output tokens.
Money, runtime, output and cancellation limits can still stop work.
A pinned request that cannot fit the physical window fails before provider access.

### Memory hygiene — what to store so it survives

Durable memory is selected, not searched: hydration pins the `core` category first (newest-first, within the byte budget), fills the remaining budget with the newest non-core entries, and cap eviction takes non-core rows before any `core` row (see [`runner_fleet.md`](./runner_fleet.md) §"Memory continuity — durable fleet memory rides the trusted plane"). Four habits make that selection work for the fleet instead of against it:

- **Store load-bearing facts as `core`.** Owner, deploy target, customer plan, standing constraints — anything the fleet must still know at entry 1001 belongs in `core`. `daily`, `conversation`, and any custom category are windowed by recency and are the first to age out of hydration. `daily` additionally expires outright: rows older than the 72-hour retention window are deleted on the fleet's next capture push (only `daily` — every other category persists until cap eviction; `memory_forget` hides a key for the current run only, `rustd/crates/afr_memory/src/hydrated.rs`).
- **Reuse stable keys.** Re-storing a key is an upsert — it refreshes the entry instead of duplicating it. `deploy_target` beats a dated `deploy_target_jun12`.
- **Replace what's stale.** A store under the same key replaces the stored copy; `memory_forget` only hides a key for the rest of the run. A Fleet that hoards everything as `core` defeats the pinning: an all-`core` set over the entry cap evicts the coldest `core` as last resort.
- **Keep entries small.** The hydration budget is bytes, not entries — one bloated entry crowds out many small ones. Snapshot conclusions, not transcripts.

The selection is deterministic and documented so a confused "why does my fleet remember X but not Y?" has a checkable answer: Y was windowed (or oversized), X was `core`.

---

## 5. What the platform never does

- Never logs raw secret bytes
- Never echoes secrets in the fleet's context
- Never persists secrets in `fleet.events`
- Never lets the GitHub App private key leave the daemon — it is platform-side; the lease envelope, the `secrets_map`, and the runner never carry it
- Never lets the fleet reach a host outside its `network.allow` list
- Never lets the fleet exceed its `budget` caps without trip-blocking
