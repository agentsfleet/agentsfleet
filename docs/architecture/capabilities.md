# Capabilities — what the agent has, what the platform guarantees

> Parent: [`README.md`](./README.md)
>
> **Scope:** the platform *guarantees* below are binding and unchanged by the M80_002 cutover. The *mechanism* now runs on the split — `agentsfleetd` (control plane) assigns work + resolves policy on `lease`; the host-resident `agentsfleet-runner`'s sandboxed child enforces the hard layer. See [`runner_fleet.md`](./runner_fleet.md) and [`data_flow.md`](./data_flow.md).

An agent's capabilities split into two layers: what the language model is told it can do (a soft layer the model can ignore or get wrong), and what the platform actually enforces (a hard layer the model cannot escape from inside the sandbox). Both matter; the second is what makes the first safe.

---

## 1. Reasoning + tool inventory (declared in the agent's own files)

| File | What it carries | Enforced by |
|---|---|---|
| `SKILL.md` | Natural-language reasoning prompt: how to think, what's safe, what to gather, when to ask for approval. Free-form prose. Required for every local install and every Fleet Bundle. | The language model reading its own prompt — soft enforcement only. The model can drift; the platform-level guarantees below contain the consequences. |
| `TRIGGER.md` | Optional policy file. When present, carries the `tools:` list, `credentials:` list, `network.allow:` list, `budget:` caps, `trigger.type:` (`webhook` / `api` / `cron` / `chain`), and `context:` budget knobs. When absent, install creates the default manual/API trigger with no tools, no credentials, and no network. | Code-enforced inside the runner's sandboxed child — the language model cannot escape these |
| Fleet Bundle support files | Optional files such as `SOUL.md`, provider playbooks, scripts, examples, and assets. These are materialized into the sandbox workspace for the installed bundle. | File access only. They do not grant tools, network, approvals, or secrets unless `TRIGGER.md` / install metadata declares those grants. |

The split matters. `SKILL.md` and bundle support files are *advisory* — the model reads them and tries to comply. `TRIGGER.md` and install-derived policy are *binding* — the runner's sandboxed child refuses tool calls that would violate them, regardless of what the model wants.

### 1.1 `trigger.type` vs `event_type` — orthogonal fields, common confusion

These are two different fields on two different tables. Mixing them up is the most common source of new-contributor confusion in this area:

| Field | Where it lives | Values | What it tags |
|---|---|---|---|
| `trigger.type` | `TRIGGER.md` frontmatter (static config) | `webhook` / `api` / `cron` / `chain` | **How an agent gets triggered** — the wiring at install time. |
| `event_type` | `core.agent_events` column (per delivery) | `chat` / `webhook` / `cron` / `continuation` | **What an individual event on the stream is** — the runtime label per delivery. |

A `trigger.type: api` agent typically receives `event_type: chat` events from the steer/chat API. A `trigger.type: webhook` agent receives `event_type: webhook`. A continuation `agentsfleetd` enqueues under context chunking produces `event_type: continuation` regardless of the original `trigger.type`. The two fields are orthogonal — never the same value, never the same table.

Source of truth: `src/agent/config_helpers.zig` (`parseAgentTrigger`) for `trigger.type`; `src/lib/contract/event_envelope.zig` (`EventType`) for `event_type`.

---

## 2. The platform tools the agent can call

These are the tool primitives NullClaw exposes. The agent's `tools:` allowlist gates which of them are reachable for a given agent.

| Tool | Purpose | Visible to the agent (host) |
|---|---|---|
| `http_request` | GET / POST to allow-listed hosts. Placeholders like `${secrets.NAME.FIELD}` are substituted at the tool bridge after sandbox entry. | The agent sees placeholders only; it never sees raw secret bytes. |
| `memory_store` / `memory_recall` | Durable scratchpad keyed by string. Survives run boundaries and full restart. The "where I am" snapshot mechanism. | Yes — the agent reads and writes. |
| `cron_add` / `cron_list` / `cron_remove` | Self-schedule future invocations. Each fire arrives as a synthetic event with `actor=cron:<schedule>`. | Yes. |
| `shell` (gated) | Read-only commands like `docker ps`, `kubectl get`. Not part of the initial platform-ops surface. | Yes, when explicitly enabled. |

---

## 3. Platform-level guarantees (the substrate that wraps every tool call)

| Capability | What it does | Primary owner |
|---|---|---|
| Work assignment + kill | `agentsfleetd` assigns the next event on `lease` (atomic affinity claim + monotonic fencing token; status/config resolved fresh from Postgres per lease), and propagates kill via heartbeat-carried lease revocation. | agentsfleetd control plane |
| Per-lease policy | Each `lease` reply carries an `ExecutionPolicy` — `secrets_map`, `network_policy`, `tools` list, and `context` knobs. The tool bridge substitutes secrets inside the runner's sandboxed child. `network_policy` is `deny_all` (no egress) or network-enabled, where egress is constrained to an operator-declared host allowlist (see [`runner_fleet.md` §Egress model](./runner_fleet.md)). | Lease ExecutionPolicy |
| Event stream + history | Every steer / webhook / cron event lands on `agent:{id}:events` with actor provenance. `core.agent_events` rows are opened at receive and closed at completion. | Event ingest + history path |
| Webhook ingest (GitHub Actions in v1) | The HTTP receiver verifies the hash-based-message-authentication signature, normalises the payload, and writes a synthetic event with `actor=webhook:github`. | Webhook receiver |
| Credential vault | Stores opaque-JSON-object credentials, encrypted with a tenant-scoped data key sealed by the cloud key-management-service. The tool bridge substitutes at sandbox entry. | Vault + secret resolution |
| Provider config (self-managed) | Per-tenant posture choice between platform-managed inference and self-managed provider key. Tenant-scoped `core.tenant_providers` row carries `mode / provider / model / context_cap_tokens / credential_ref`; the user-named credential pointed to by `credential_ref` carries `{provider, api_key, model}`. The api_key crosses one boundary cleanly (vault → resolver → lease `ExecutionPolicy` → runner's sandboxed child → outbound HTTPS) and never appears in any user-facing surface. See [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §8.2. | Provider resolution path |
| Approval gating | Risky actions block until a human clicks Approve in the dashboard or a Slack DM. The state machine survives control-plane and runner restarts (it is durable in Postgres, gated at `lease`). | Approval workflow |
| Budget caps | Daily and monthly dollar hard caps; further runs are blocked at the first trip. Configured per-agent in `TRIGGER.md`. | Billing gate |
| Per-run context lifecycle | Rolling tool-result window, memory-store nudge, run chunking, and continuation events. See §4. | Context lifecycle |

---

## 4. Context lifecycle — keeping a long incident reasoning past the model's working-memory limit

Every agent reasoning loop lives inside a single `runner.execute` call. As the agent makes tool calls, each result lands in the language model's context window. On a long-running incident (thirty-plus tool calls), this can exhaust the window. The platform layers three independent mechanisms — defence in depth, not override — to keep the agent reasoning past the limit.

### The three knobs

```yaml
# In the agent's TRIGGER.md frontmatter under x-agentsfleet:
x-agentsfleet:
  model: accounts/fireworks/models/kimi-k2.6   # opaque pass-through; the control
                                                # plane forwards it into the lease
                                                # ExecutionPolicy (the engine's
                                                # ContextBudget.model). Empty
                                                # string ("") = self-managed overlay
                                                # sentinel: agentsfleetd resolves
                                                # the real model from
                                                # core.tenant_providers at
                                                # lease time.
  context:
    tool_window: auto              # rolling tool-result window size; "auto"
                                    # (bare YAML string) and 0 are equivalent
                                    # — both mean "let the runtime pick".
    memory_checkpoint_every: 5     # call memory_store every N tool calls
    stage_chunk_threshold: 0.75    # % context fill that triggers chunking
    context_cap_tokens: 200000     # the active model's context window
                                    # (resolved at install time from the
                                    #  model-caps endpoint — see user_flow.md
                                    #  §8.7 and billing_and_provider_keys.md). 0 is the
                                    # self-managed overlay sentinel — same shape as
                                    # the empty `model` value above.
```

**Wire-shape parser status.** Both `x-agentsfleet.model` and the four
`x-agentsfleet.context.*` knobs are parsed into `AgentConfig`, carried in the
lease `ExecutionPolicy`, and applied by the engine's `ContextBudget` on every
run. Operator overrides take effect; absent or zero fields fall through to
the runtime defaults below via `applyContextDefaults`.

### How the three layers compose (defence-in-depth, not override)

```mermaid
flowchart TD
    Start([Run opens — runner.execute]) --> Tool1[tool call 1<br/>result added to context]
    Tool1 --> Tool2[tool calls 2-4]
    Tool2 --> L1{N tool calls?}
    L1 -->|yes| Checkpoint[L1 fires:<br/>agent calls memory_store<br/>'findings_so_far']
    Checkpoint --> Tool3[more tool calls]
    L1 -->|no| Tool3
    Tool3 --> L2{result count<br/>over tool_window?}
    L2 -->|yes| Drop[L2 fires:<br/>oldest results dropped<br/>from context<br/>still in event log]
    L2 -->|no| Fill
    Drop --> Fill[continue tool calls]
    Fill --> L3{context fill<br/>over stage_chunk_threshold?}
    L3 -->|yes| Chunk[L3 fires observability:<br/>runtime logs chunk_threshold_breached<br/>SKILL.md prose tells the agent to<br/>snapshot via memory_store + wrap up]
    L3 -->|no| Tool4[tool call N+1]
    Tool4 --> L1
    Chunk --> NextStage([Next trigger:<br/>agent recalls snapshot<br/>via memory hydration])
    NextStage --> Tool1
```

### What each layer catches

- **Layer 1 — `memory_checkpoint_every`.** Runs periodically as the agent works. Forces the agent to write a durable snapshot of "what I've learned so far" via `memory_store` every N tool calls. Cheap and always safe — even if subsequent layers drop context, the snapshot survives.
- **Layer 2 — `tool_window`.** Runs continuously. Bounds context growth by dropping the oldest tool results once the count exceeds the cap. Old results stay in `core.agent_events`; they just leave the active language-model context.
- **Layer 3 — `stage_chunk_threshold`.** The failsafe. After every LLM round-trip the runtime computes context fill (prompt tokens / cap); when it crosses the threshold it emits a `chunk_threshold_breached` observability log. It does **not** force a chunk — NullClaw exposes no mid-loop interrupt, so the runtime can see the fill but cannot halt the loop. The agent owns the decision: its `SKILL.md` prose tells it to write a durable snapshot (`memory_store`) and wrap up the run as its context fills. A subsequent run then picks up where it left off: the runner parent hydrates that agent's prior memory over the `agt_r` plane — category-pinned, so `core` entries hydrate before any recency windowing (see [*Memory continuity*](./runner_fleet.md); the agent itself holds no datastore credential) — and the agent recalls the snapshot from its in-run store, possibly on a different runner.

The order is failure-mode escalation: Layer 1 keeps your work safe, Layer 2 keeps your context bounded, Layer 3 saves the chain from collapse. They never conflict.

### Bounding a runaway run

Because the runtime cannot force a chunk (above), there is no enforced continuation-chain counter. A single run that will not wrap up is instead bounded by the guards that *are* enforced: the agent's `budget` caps (`daily_dollars` / `monthly_dollars`) and the lease's runtime kill-deadline. Those stop a runaway run regardless of whether the agent chooses to snapshot.

`chunk_threshold_breached` is observability only — it tells an operator a run is getting large, nothing more. Resume a wrapped-up incident by steering a fresh message that calls `memory_recall` against whatever snapshot key the agent's own `SKILL.md` prose chose (the runtime never invents the key shape).

### Defaults — the user shouldn't have to do token math

The runtime ships with model-tier-aware defaults the user inherits without any configuration:

- `memory_checkpoint_every: 5` and `stage_chunk_threshold: 0.75` for every model — checkpoint cadence and chunk-trigger fraction don't meaningfully change with context size.
- `tool_window: auto` resolves at install time based on the active model's context cap: **30** when the cap is at least one million tokens, **20** for caps between two-hundred-thousand and three-hundred-thousand tokens, **10** for caps at or below two-hundred-thousand tokens.

The model's context cap is **not** baked into the runtime. It's resolved at install time (platform-managed posture) or at provider-set time (self-managed provider key posture) from the model-caps endpoint. See [`user_flow.md`](./user_flow.md) §8.7 and [`billing_and_provider_keys.md`](./billing_and_provider_keys.md).

### When a user does want to override (rare)

| Goal | What to change | How to think about it |
|---|---|---|
| "My agent loses important findings mid-incident" | `memory_checkpoint_every: 3` | Checkpoint more often. Cheap. Always safe. |
| "My agent hits context limits and chunks too aggressively" | `tool_window: 10` | Drop old results sooner so newer stuff fits. Loses older tool results earlier. |
| "My agent chunks too late and produces partial diagnoses" | `stage_chunk_threshold: 0.6` | Chunk earlier. More handoffs but less risk of being cut off mid-thought. |
| "I'm on Kimi K2.6 (256k) and incidents are big" | `tool_window: 8` + `memory_checkpoint_every: 3` | Smaller windows + more checkpoints. Standard tight-context discipline. |

### The 80/20 rule

Eighty percent of users use the defaults forever and never see context errors. Twenty percent who run very deep incidents tweak `tool_window` once and forget. Almost nobody touches `stage_chunk_threshold`.

### Memory hygiene — what to store so it survives

Durable memory is selected, not searched: hydration pins the `core` category first (newest-first, within the byte budget), fills the remaining budget with the newest non-core entries, and cap eviction takes non-core rows before any `core` row (see [*Memory continuity*](./runner_fleet.md) §Selection policy). Four habits make that selection work for the agent instead of against it:

- **Store load-bearing facts as `core`.** Owner, deploy target, customer plan, standing constraints — anything the agent must still know at entry 1001 belongs in `core`. `daily`, `conversation`, and any custom category are windowed by recency and are the first to age out of hydration. `daily` additionally expires outright: rows older than the 72-hour retention window are deleted on the agent's next capture push (only `daily` — every other category persists until cap eviction or an explicit `memory_forget`).
- **Reuse stable keys.** Re-storing a key is an upsert — it refreshes the entry instead of duplicating it. `deploy_target` beats a dated `deploy_target_jun12`.
- **Forget what's stale.** `memory_forget` is cheaper than letting cap eviction pick the victim. An agent that hoards everything as `core` defeats the pinning: an all-`core` set over the entry cap evicts the coldest `core` as last resort.
- **Keep entries small.** The hydration budget is bytes, not entries — one bloated entry crowds out many small ones. Snapshot conclusions, not transcripts.

The selection is deterministic and documented so a confused "why does my agent remember X but not Y?" has a checkable answer: Y was windowed (or oversized), X was `core`.

---

## 5. What the platform never does

- Never logs raw secret bytes
- Never echoes secrets in the agent's context
- Never persists secrets in `core.agent_events`
- Never lets the agent reach a host outside its `network.allow` list
- Never lets the agent exceed its `budget` caps without trip-blocking
