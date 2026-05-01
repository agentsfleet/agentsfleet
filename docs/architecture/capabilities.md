# Capabilities — what the zombie has, what the platform guarantees

> Parent: [`README.md`](./README.md)

A zombie's capabilities split into two layers: what the language model is told it can do (a soft layer the model can ignore or get wrong), and what the platform actually enforces (a hard layer the model cannot escape from inside the sandbox). Both matter; the second is what makes the first safe.

---

## 1. Reasoning + tool inventory (declared in the zombie's own files)

| File | What it carries | Enforced by |
|---|---|---|
| `SKILL.md` | Natural-language reasoning prompt: how to think, what's safe, what to gather, when to ask for approval. Free-form prose. | The language model reading its own prompt — soft enforcement only. The model can drift; the platform-level guarantees below contain the consequences. |
| `TRIGGER.md` (or merged frontmatter under `x-usezombie:` in a single SKILL.md file) | The `tools:` list, `credentials:` list, `network.allow:` list, `budget:` caps, `trigger.type:` (`webhook` / `api` / `cron` / `chain`), and `context:` budget knobs | Code-enforced at the executor sandbox boundary — the language model cannot escape these |

> **`trigger.type` vs event type — they are different fields.** `trigger.type` is the static config that says *how* a zombie gets triggered: `webhook` (external sender posts to `/v1/webhooks/...`), `api` (operator/integration calls `/v1/.../zombies/{id}/messages` — the chat path), `cron` (scheduled), or `chain` (another zombie hands off). The per-event `event_type` field on `core.zombie_events` (`chat`, `continuation`, …) tags individual events on the stream. A `trigger.type: api` zombie typically receives `event_type: chat` events from the steer/chat API; the two are orthogonal and live in different tables. See `src/zombie/config_helpers.zig` (`parseZombieTrigger`) and `src/zombie/event_envelope.zig` (`EventType`).

The split matters. `SKILL.md` is *advisory* — the model reads it and tries to comply. `TRIGGER.md` is *binding* — the executor refuses tool calls that would violate it, regardless of what the model wants.

---

## 2. The platform tools the zombie can call

These are the tool primitives NullClaw exposes. The zombie's `tools:` allowlist gates which of them are reachable for a given zombie.

| Tool | Purpose | Visible to the zombie's agent |
|---|---|---|
| `http_request` | GET / POST to allow-listed hosts. Placeholders like `${secrets.NAME.FIELD}` are substituted at the tool bridge after sandbox entry. | The agent sees placeholders only; it never sees raw secret bytes. |
| `memory_store` / `memory_recall` | Durable scratchpad keyed by string. Survives stage boundaries and full restart. The "where I am" snapshot mechanism. | Yes — the agent reads and writes. |
| `cron_add` / `cron_list` / `cron_remove` | Self-schedule future invocations. Each fire arrives as a synthetic event with `actor=cron:<schedule>`. | Yes. |
| `shell` (gated) | Read-only commands like `docker ps`, `kubectl get`. Not part of the initial platform-ops surface. | Yes, when explicitly enabled. |

---

## 3. Platform-level guarantees (the substrate that wraps every tool call)

| Capability | What it does | Primary owner |
|---|---|---|
| Worker control stream | A watcher thread on `zombie:control` claims new zombies, spawns per-zombie threads, and propagates kill within milliseconds (not on the 5-second `XREADGROUP` cycle). | Worker control plane |
| Per-execution policy | Each `executor.createExecution` carries `secrets_map`, `network_policy`, `tools` list, and `context` knobs. The tool bridge substitutes secrets at the sandbox boundary. | Executor session policy |
| Event stream + history | Every steer / webhook / cron event lands on `zombie:{id}:events` with actor provenance. `core.zombie_events` rows are opened at receive and closed at completion. | Event ingest + history path |
| Webhook ingest (GitHub Actions in v1) | The HTTP receiver verifies the hash-based-message-authentication signature, normalises the payload, and writes a synthetic event with `actor=webhook:github`. | Webhook receiver |
| Credential vault | Stores opaque-JSON-object credentials, encrypted with a tenant-scoped data key sealed by the cloud key-management-service. The tool bridge substitutes at sandbox entry. | Vault + secret resolution |
| Provider config (BYOK) | Per-tenant posture choice between platform-managed inference and Bring Your Own Key. Tenant-scoped `core.tenant_providers` row carries `mode / provider / model / context_cap_tokens / credential_ref`; the operator-named credential pointed to by `credential_ref` carries `{provider, api_key, model}`. The api_key crosses one boundary cleanly (vault → resolver → executor → outbound HTTPS) and never appears in any user-facing surface. See [`billing_and_byok.md`](./billing_and_byok.md) §7. | Provider resolution path |
| Approval gating | Risky actions block until a human clicks Approve in the dashboard or a Slack DM. The state machine survives worker restarts. | Approval workflow |
| Budget caps | Daily and monthly dollar hard caps; further runs are blocked at the first trip. Configured per-zombie in `TRIGGER.md` / `x-usezombie.budget`. | Billing gate |
| Per-stage context lifecycle | Rolling tool-result window, memory-store nudge, stage chunking, and continuation events. See §4. | Context lifecycle |

---

## 4. Context lifecycle — keeping a long incident reasoning past the model's working-memory limit

Every zombie reasoning loop lives inside a single `runner.execute` call. As the agent makes tool calls, each result lands in the language model's context window. On a long-running incident (thirty-plus tool calls), this can exhaust the window. The platform layers three independent mechanisms — defence in depth, not override — to keep the zombie reasoning past the limit.

### The three knobs

```yaml
# In the zombie's SKILL.md frontmatter under x-usezombie:
x-usezombie:
  context:
    tool_window: auto              # rolling tool-result window size
    memory_checkpoint_every: 5     # call memory_store every N tool calls
    stage_chunk_threshold: 0.75    # % context fill that triggers chunking
    context_cap_tokens: 200000     # the active model's context window
                                    # (resolved at install time from the
                                    #  model-caps endpoint — see user_flow.md
                                    #  §8.7 and billing_and_byok.md)
```

### How the three layers compose (defence-in-depth, not override)

```mermaid
flowchart TD
    Start([Stage opens — runner.execute]) --> Tool1[tool call 1<br/>result added to context]
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
    L3 -->|yes| Chunk[L3 fires:<br/>agent writes final snapshot<br/>returns exit_ok=false<br/>worker re-enqueues]
    L3 -->|no| Tool4[tool call N+1]
    Tool4 --> L1
    Chunk --> NextStage([Next stage:<br/>memory_recall<br/>incident:X])
    NextStage --> Tool1
```

### What each layer catches

- **Layer 1 — `memory_checkpoint_every`.** Runs periodically as the agent works. Forces the agent to write a durable snapshot of "what I've learned so far" via `memory_store` every N tool calls. Cheap and always safe — even if subsequent layers drop context, the snapshot survives.
- **Layer 2 — `tool_window`.** Runs continuously. Bounds context growth by dropping the oldest tool results once the count exceeds the cap. Old results stay in `core.zombie_events`; they just leave the active language-model context.
- **Layer 3 — `stage_chunk_threshold`.** The failsafe. When context fill exceeds the threshold (a percentage of the active model's context cap), the agent writes a final snapshot, returns `{exit_ok: false, content: "needs continuation", checkpoint_id: ...}`, and the worker re-enqueues the same event chain as a synthetic event with `actor=continuation:<original_actor>`. The next stage starts fresh and immediately calls `memory_recall` to load the snapshot.

The order is failure-mode escalation: Layer 1 keeps your work safe, Layer 2 keeps your context bounded, Layer 3 saves the chain from collapse. They never conflict.

### The chain-cap escape hatch — `chunk_chain_escalate_human`

A pathological agent can in principle chunk forever — each stage hits the L3 threshold, snapshots, and the worker dutifully re-enqueues. To bound this, the runtime caps each chain at **10 continuations**. On the 11th attempt:

- **No XADD.** The worker stops re-enqueueing this chain.
- **`failure_label = chunk_chain_escalate_human`** is written to the originating event row. The label appears in `zombiectl events {id}`, the dashboard Events tab, and the activity stream's terminal `event_complete` frame.
- **The zombie itself stays alive.** Only this one chain is forfeit; future webhooks, cron fires, and operator steers land on the events stream as fresh chains with their own 10-chunk budgets.
- **Idempotency on replay** is preserved by the `(zombie_id, event_id)` PRIMARY KEY — a duplicate XADD of an already-processed event is a no-op.

**Notification today is silent.** The label is observability — the operator sees it only by looking (`zombiectl events`, dashboard, SSE tail). There is no automatic Slack post, email, or page when `chunk_chain_escalate_human` fires. Active notification could later come from the approval surface, from SKILL prose that posts a Slack handoff message preemptively on the agent's 10th continuation, or from an optional `escalation_webhook_url` on the zombie config that the worker POSTs to whenever this label fires.

**Resuming a forfeited chain manually.** There is no special "resume chunk-chain" endpoint. The cleanest path is `zombiectl steer {id} "continue from <snapshot-key> — pick up where you left off"`, where `<snapshot-key>` is whatever the zombie's own SKILL.md prose taught the agent to use as the memory key for that work unit (e.g. `incident:<id>:findings` for platform-ops; `lead:<id>:scoring` for a lead-scorer; `applicant:<id>:assessment` for a screener). This XADDs a fresh chat event; the SKILL prose tells the agent to `memory_recall` the snapshot. The new event's chain starts at zero — the operator gets another 10-chunk budget without the runtime needing a dedicated resume verb. The runtime never invents the key shape — it's whatever the zombie's own prose chose.

### Defaults — the user shouldn't have to do token math

The runtime ships with model-tier-aware defaults the user inherits without any configuration:

- `memory_checkpoint_every: 5` and `stage_chunk_threshold: 0.75` for every model — checkpoint cadence and chunk-trigger fraction don't meaningfully change with context size.
- `tool_window: auto` resolves at install time based on the active model's context cap: **30** when the cap is at least one million tokens, **20** for caps between two-hundred-thousand and three-hundred-thousand tokens, **10** for caps at or below two-hundred-thousand tokens.

The model's context cap is **not** baked into the runtime. It's resolved at install time (platform-managed posture) or at provider-set time (Bring Your Own Key posture) from the model-caps endpoint. See [`user_flow.md`](./user_flow.md) §8.7 and [`billing_and_byok.md`](./billing_and_byok.md).

### When a user does want to override (rare)

| Goal | What to change | How to think about it |
|---|---|---|
| "My zombie loses important findings mid-incident" | `memory_checkpoint_every: 3` | Checkpoint more often. Cheap. Always safe. |
| "My zombie hits context limits and chunks too aggressively" | `tool_window: 10` | Drop old results sooner so newer stuff fits. May lose context recency. |
| "My zombie chunks too late and produces partial diagnoses" | `stage_chunk_threshold: 0.6` | Chunk earlier. More handoffs but less risk of being cut off mid-thought. |
| "I'm on Kimi 2.6 (256k) and incidents are big" | `tool_window: 8` + `memory_checkpoint_every: 3` | Smaller windows + more checkpoints. Standard tight-context discipline. |

### The 80/20 rule

Eighty percent of users use the defaults forever and never see context errors. Twenty percent who run very deep incidents tweak `tool_window` once and forget. Almost nobody touches `stage_chunk_threshold`.

---

## 5. What the platform never does

- Never logs raw secret bytes
- Never echoes secrets in the agent's context
- Never persists secrets in `core.zombie_events`
- Never lets the agent reach a host outside its `network.allow` list
- Never lets the agent exceed its `budget` caps without trip-blocking
