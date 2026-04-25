# Architecture — v2 MVP Operational Outcome Runner

Date: Apr 25, 2026
Status: Canonical reference for the v2 MVP problem, thesis, runtime model, agent/zombie interaction, capabilities, and context lifecycle. This document defines why the product exists, how a zombie runs, how operators and external systems talk to it, and what guarantees the platform enforces. All v2 specs in `docs/v2/pending/` (M40_001 onward) are grounded here.

UseZombie v2 is no longer framed around coding-agent orchestration or PR delivery. The v2 wedge is an operational outcome runner: a long-lived zombie defined in natural language through `SKILL.md` and `TRIGGER.md` that wakes on operational signals, reasons with an LLM, uses approved tools with injected secrets, preserves history, resumes from checkpoints, requests approval for risky actions, and stays on the outcome until it is resolved or explicitly blocked.

---

## 0. Product Thesis

### What we are

UseZombie v2 is a durable runtime for one operational outcome.

It is meant for work that:

- continues after the original human prompt is gone
- needs durable state across retries and failures
- must gather evidence from real systems
- may need approvals before acting
- benefits from natural-language reasoning instead of rigid typed branching

### What we are not

UseZombie v2 is not:

- a general-purpose personal assistant in the cloud
- a generic coding-agent orchestration product
- a broad "AI can automate anything" platform
- just a chat UI over tools

If a user can get the same value by opening Claude locally and asking "what should I do next?", then v2 has not earned its existence.

### The first problem we solve

The first problem v2 solves is deploy and production failure handling.

When a deploy fails, or production looks unhealthy, the operator should not have to manually bounce between CI, logs, infra dashboards, chat, and shell history while also remembering what they already tried. The zombie should take ownership of that outcome: gather evidence, explain what is wrong, preserve the timeline, request approval when necessary, and continue until resolved or blocked.

This is the flagship workflow: `platform-ops`. The wedge surface is **GitHub Actions CD-failure responder + manual operator steer** — a zombie that wakes on a failed deploy webhook, gathers evidence, posts a diagnosis to Slack, and is also reachable via `zombiectl steer` for manual investigation.

---

## 1. Problem Statement

Operational work falls into limbo.

When a deploy fails, production looks unhealthy, or a risky recovery action like a database teardown is needed, the operator has to manually gather logs, inspect dashboards, remember prior attempts, decide the next step, and keep the audit trail straight across terminals, chat, CI, and infrastructure consoles. The work is fragmented, state is lost between attempts, and dangerous actions are performed ad hoc.

Existing tooling captures pieces of the workflow but not the whole outcome:

- CI can tell you a deploy failed.
- Logging and observability can show symptoms.
- Chat can deliver alerts.
- Shell scripts and runbooks can perform actions.

What is missing is a long-lived runtime that owns the outcome end-to-end.

The v2 MVP solves that gap. A zombie receives an operational trigger, reasons over it in natural language, gathers evidence with the right tools, persists the full attempt history, resumes if interrupted, asks for approval when needed, and continues until the outcome is resolved or clearly blocked.

The MVP wedge is:

- operational outcome runner
- deploy and infra recovery (GH Actions CD-failure responder + manual steer)
- persistent history with actor provenance
- resumable execution with bounded context
- approvals where needed
- one flagship workflow

The flagship workflow is `platform-ops`: when a GitHub Actions CD pipeline fails, the zombie wakes from the webhook, gathers the right evidence from Fly.io, Upstash, Redis, and adjacent sources, explains what is wrong, and posts a remediation suggestion to Slack. The same zombie can also be steered manually for "morning health check" investigations and follow-up reasoning after the webhook diagnosis.

---

## 2. Why Now / Why Not the Alternatives

### Why now

Three shifts make this MVP plausible now:

1. LLMs can reason over messy operational evidence well enough to choose the next step when paired with constrained tools and clear instructions.
2. Operators already maintain personal assistants and prompt files locally, but those assistants are ephemeral and session-bound. They do not own outcomes across time, triggers, retries, approvals, and failures.
3. The integration surface already exists in the real workflow: deploy webhooks, cron checks, logs APIs, secret stores, approval policies, and shell-accessible recovery tasks. The missing piece is durable orchestration around them.

Stated plainly: many people now run a personal assistant in a TUI. v2 moves the useful part of that behavior into a cloud-resident, durable, auditable runtime that can wait for events, wake itself up, keep state, and continue working after the original human session is gone.

### Why not just use incident tooling

Incident tooling is adjacent but incomplete for this problem.

Incident products can:

- ingest alerts
- manage timelines
- route responders
- collect notes
- summarize incidents

They usually do not provide a natural-language-defined runtime that can continuously reason over the situation, use approved tools, preserve its own checkpointed execution state, and carry the operational task forward as an active worker.

The v2 claim is not "better incident chat." The claim is "a durable operational runtime owns the outcome."

### Why not just use typed automation and runbooks

Typed automation solves repetitive known paths well, but it is weak when:

- the evidence needed changes by incident
- the next step depends on interpretation, not a fixed branch
- the operator wants to iterate behavior by editing prompts and trigger docs instead of rewriting typed control flow

v2 should keep a minimal typed envelope for safety, durability, auth, approvals, and idempotency. The operational behavior itself should stay primarily natural-language-driven through `SKILL.md`, `TRIGGER.md`, and the consumer's own operating instructions.

### Why this is not automatically a good business

This thesis can still fail.

Counterarguments:

- A deterministic deploy watcher plus logs fetcher may solve enough of the problem without an LLM.
- The hard part may be trust, approvals, and evidence quality, not reasoning.
- The market is adjacent to incident automation and AI SRE tooling, which already exist.
- If the zombie mostly summarizes logs but does not move the outcome forward, the product will feel ornamental.

That is why the MVP bar is strict: the zombie must make a real operational outcome faster and safer to complete, not merely easier to read about.

---

## 3. MVP Thesis

UseZombie v2 should be judged on one promise:

> Operational outcomes do not fall into limbo.

For the MVP, that means:

- a trigger arrives from webhook (GH Actions failure), cron, or operator steer
- the zombie gathers the right evidence
- the zombie reasons over the situation using LLM + constrained tools
- every step is persisted with actor provenance
- the zombie context is bounded (rolling window + memory checkpoints + stage chunking)
- risky steps are approval-gated
- the zombie can resume after interruption
- the operator resumes from state, not memory

If the product cannot prove that loop on a real GH Actions deploy failure and a real approval-gated destructive workflow, the thesis is not validated.

---

## 4. Initial Use Cases

### 4.1 Platform-Ops (wedge)

Primary job:

- investigate failed GitHub Actions CD deploys (webhook trigger)
- investigate unhealthy production state on a schedule (cron trigger) or on operator request (steer trigger)
- collect logs and health evidence from Fly.io, Upstash, Redis, GitHub Actions run logs, and adjacent systems
- summarize likely cause
- recommend or execute the next action depending on approval policy

Trigger modes:

- **webhook**: GitHub Actions `workflow_run.conclusion == failure` posted to `/v1/.../webhooks/github` with HMAC signature; lands as synthetic event with `actor=webhook:github`
- **cron**: periodic production health check via NullClaw-managed schedule; lands with `actor=cron:<schedule>`
- **steer**: direct operator instruction via `zombiectl steer {id} [<message>]` or the dashboard chat widget; lands with `actor=steer:<user>`

All three flow through the same reasoning loop. The zombie does not branch on actor type — its SKILL.md describes the general outcome and the same `http_request` tool calls fire regardless of trigger source.

---

## 5. Architecture Direction

The architecture optimizes for one generic operational runtime, not bespoke typed flows per use case.

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

---

## 6. Open Product Question

Many users already run a personal assistant locally in a TUI. The v2 bet is that the durable cloud runtime is more valuable than the local interactive loop for operational work.

That bet is valid only if the cloud zombie does something the local assistant does not:

- waits for events while the human is gone
- keeps state across retries and failures
- resumes after interruption
- owns approvals and audit
- acts on real infrastructure signals, not just conversational prompts

If those properties are not materially better than "open Claude and ask it what to do next," then the v2 thesis is too weak.

---

## 7. Why Not Just Use OpenClaw

This is the hardest product challenge and is stated plainly.

For many technical users, OpenClaw or a similar local assistant can already do a meaningful subset of the work:

- inspect deploy logs
- analyze production failures
- prepare a recovery plan
- use tools from a supervised terminal session
- ask for confirmation before risky actions

If that is enough, then UseZombie v2 should not exist as a separate product.

The v2 product is only justified if it provides something materially better than a local assistant session with a few scripts around it.

### What OpenClaw already does well

OpenClaw is strong for:

- operator-driven, one-shot tasks
- supervised tool use
- prompt-defined reasoning
- iterative editing of instructions while the human is present

That means v2 cannot claim novelty simply by saying:

- "it uses an LLM"
- "it can call tools"
- "it can follow natural-language instructions"
- "it can help with ops"

Those are table stakes.

### What UseZombie must do better

UseZombie v2 only earns its existence if it does at least some of these materially better than OpenClaw alone:

- waits for future events while the user is gone
- wakes on cron or webhook without a live interactive session
- keeps durable event history with actor provenance across attempts and failures
- resumes from checkpoint after interruption, restart, or approval wait
- owns approval state as part of the runtime
- stays attached to one operational outcome until resolved or blocked
- preserves an auditable record of what happened, why, and what was approved
- bounds context across long-running stages so it can keep reasoning past the model's working-memory limit

If those properties are weak, bolted on, or rarely needed, then users should just use OpenClaw.

### Pass / fail test for the product

The v2 thesis passes only if the answer to this question is clearly "yes":

> Is the zombie materially more useful than opening OpenClaw locally and asking what to do next?

For the answer to be "yes", the user must get real additional value from:

- durability
- triggers
- resumability
- approvals
- history
- outcome ownership
- bounded long-running reasoning

If the main value is still interactive reasoning inside a live human-supervised session, then the better answer is to improve the assistant experience, not build a separate runtime.

### Practical implication for the MVP

The MVP should avoid trying to beat OpenClaw at being a general assistant.

Instead, it should prove one thing OpenClaw does not naturally solve on its own:

> A long-lived operational outcome continues correctly after the original interactive session ends, AND a webhook can wake it without any human present.

That is why the flagship stays narrow:

- GitHub Actions CD-failure responder via `platform-ops`, plus manual operator steer on the same zombie

If that does not show clear advantage over OpenClaw plus light glue code, then the product thesis should be reconsidered.

---

## 8. How the User Uses the System

The initial user assumption is simple:

- the user is already working inside Claude (or Amp, Codex CLI, OpenCode — any agent that can read SKILL.md)
- the user is already working on their own project or infrastructure
- the user wants operational work to continue without babysitting an endless terminal loop

The Claude session becomes the place where the user defines, installs, updates, and supervises zombies. The zombie runtime becomes the place where long-lived operational outcomes continue after the chat session ends.

### 8.1 Authoring the zombie

The user defines the zombie in project files:

- `SKILL.md` describes how the zombie should think, what its job is, what "good" looks like, what evidence to gather, and what actions require caution. Plain English. No framework syntax.
- `TRIGGER.md` (or merged frontmatter under `x-usezombie:` in a single SKILL.md — see §10) describes how the zombie wakes up: webhook, cron, operator steer, or a combination. Also declares `tools:`, `credentials:`, `network.allow:`, `budget:`, and `context:` knobs.

The user iterates those files from Claude in natural language:

- "tighten the deploy-failure diagnosis prompt"
- "add a periodic health check every 15 minutes"
- "require approval before teardown"
- "include Fly logs and Redis health in the first pass"

This keeps the operational logic editable by changing instructions, not by rewriting a typed workflow engine for every variation.

### 8.2 Installing the zombie

Once the files are ready, the user installs the zombie into the workspace.

Conceptually, the workflow is:

1. Claude (or another agent) helps author or refine `SKILL.md` and `TRIGGER.md`.
2. The user installs or updates the zombie through `zombiectl install --from <path>` or the API.
3. `zombiectl doctor` is called first to verify the user's CLI is authenticated and bound to a workspace before the install request fires.
4. The API stores the zombie config, linked credentials, approval policy, and trigger settings.
5. The worker runtime becomes responsible for future triggers — no worker restart required.

After install, the zombie is no longer tied to the interactive Claude session that created it.

### 8.3 Triggering the zombie

For the MVP, the zombie is triggerable in three ways:

- **Webhook input**: an external system (most importantly GitHub Actions on `workflow_run.conclusion == failure`) sends an event to `POST /v1/.../webhooks/github`. The receiver verifies the HMAC signature, normalizes the payload, and lands a synthetic event on `zombie:{id}:events` with `actor=webhook:github`.
- **Cron input**: NullClaw's `cron_add` tool persists a schedule. Each fire arrives as a synthetic event with `actor=cron:<schedule>`.
- **Operator steer**: the user, while in Claude, asks to run an operational task. Claude invokes `zombiectl steer {id} "<message>"` (or the dashboard chat widget), which writes to `zombie:{id}:steer` and is converted to an event with `actor=steer:<user>`.

The important point is that all of these enter the same runtime model. The zombie's reasoning loop does not branch on actor type — the same `http_request`-driven evidence gathering and Slack post happen regardless of how the work was triggered.

### 8.4 Working from Claude

The user experience inside Claude (or Amp / Codex CLI / OpenCode) feels like this:

1. The user is already in their project.
2. The user asks Claude to create or refine an operational zombie.
3. Claude edits `SKILL.md`, `TRIGGER.md`, and related project instructions.
4. Claude installs or updates the zombie.
5. Claude can also manually invoke the zombie via `zombiectl steer` for one-off operator-triggered tasks.
6. Later, the zombie wakes on webhook or cron without the user staying in the terminal.
7. When the user returns to Claude, they inspect what happened from durable history (`zombiectl events {id}` or the dashboard Events tab) instead of reconstructing it from memory.

This matters because the zombie is not replacing Claude. It extends Claude from an interactive assistant into a durable operational worker.

### 8.5 Example: Platform-Ops with GH Actions trigger

While working in Claude, the user defines a `platform-ops` zombie that:

- wakes on GitHub Actions deploy-failure webhooks (primary)
- wakes on a periodic production health cron (secondary)
- can also be steered manually by the operator

When a GH Actions deploy fails:

1. GitHub posts to `/v1/.../webhooks/github` with the failed `workflow_run` payload.
2. The webhook receiver verifies the HMAC signature against the workspace's stored GH webhook secret.
3. The receiver normalizes the payload into a synthetic event and `XADD`s to `zombie:{id}:events` with `actor=webhook:github`, `data={run_url, head_sha, conclusion, ref, repo, attempt}`.
4. The worker's per-zombie thread unblocks from `XREADGROUP`, processes the event:
   - INSERT `core.zombie_events` (status='received')
   - balance + approval gates pass
   - resolve credentials from the vault (GitHub PAT, Fly token, Slack bot token)
   - `executor.createExecution` opens a sandbox session with `secrets_map`, `network_policy`, `tools` list, and `context` knobs
   - `executor.startStage` invokes the NullClaw agent with the message
5. The zombie's NullClaw agent reasons over the message:
   - calls `http_request GET https://api.github.com/repos/{repo}/actions/runs/{run_id}/logs` with `${secrets.github.api_token}` substituted at the tool bridge
   - calls `http_request GET ${fly.host}/v1/apps/{app}/logs`
   - calls `http_request GET ${upstash.host}/v2/redis/stats/{db}`
   - correlates: was the failure a migration error vs OOM kill vs network timeout vs deploy-config drift
   - calls `http_request POST ${slack.host}/api/chat.postMessage` with the diagnosis
6. The zombie's response is UPDATEd into `core.zombie_events` (status='processed', response_text, tokens, wall_ms).
7. If the SKILL.md prose said the zombie may schedule a follow-up health check, it calls `cron_add "*/30 * * * *" "post-recovery health check"`.

When the user opens Claude later, they see the outcome trail in `core.zombie_events` keyed by actor — they can filter "show me all webhook:github events from the last 24h" or "show me what kishore steered last Tuesday." They never reconstruct from memory; the durable log is authoritative.

The same zombie also responds to manual `zombiectl steer {id} "morning health check"` — same reasoning loop, different `actor=steer:kishore`.

### 8.6 Why Claude is the starting point

Starting with Claude is the right constraint because it matches how technical users already work today.

They are already:

- iterating prompts
- editing project docs
- asking for automation help
- supervising tools from the terminal

The v2 product meets them there first.

Later, other entrypoints exist (the dashboard chat widget, direct API calls). But the MVP assumes:

- the user authors and supervises from Claude
- the zombie executes durably outside that transient chat session

---

## 9. How an Agent Chats with a Zombie (the Steer Flow)

Two distinct "agents" are in play. Keeping them straight is essential to understanding the architecture:

```
┌────────────────────────────────┐         ┌──────────────────────────────┐
│  USER'S AGENT (laptop)         │         │  ZOMBIE'S AGENT (cloud/host) │
│                                │         │                              │
│  Claude Code / Amp / Codex /   │         │  NullClaw running inside     │
│  OpenCode driving zombiectl    │         │  zombied-executor            │
│                                │         │  (sandboxed Landlock+cgroups │
│  This is what the human types  │         │   +bwrap, durable, persists  │
│  into. Ephemeral.              │         │   across user's laptop close)│
└────────────────────────────────┘         └──────────────────────────────┘
```

The user's agent is a workstation tool driving `zombiectl`. The zombie's agent is a long-lived NullClaw instance inside the executor sandbox. The user's agent never becomes the zombie's agent and never sees its tokens — they communicate only through the steer endpoint, the event stream, and the events history.

### Steer flow end-to-end

```
                "what's the deploy status?"
                          ↓
         User's Agent → zombiectl steer {id} "<msg>"
                          ↓

           ╔═══════════════════════════════════╗
           ║  zombied-api (HTTP)               ║
           ║  POST /v1/.../zombies/{id}/steer  ║
           ║  ───────────────────────────────  ║
           ║  SET zombie:{id}:steer "<msg>"    ║
           ║       EX 300                       ║
           ║  → 202 Accepted                    ║
           ╚═══════════════════════════════════╝
                          ↓
           ╔═══════════════════════════════════╗
           ║  zombied-worker (zombie thread)   ║
           ║  ───────────────────────────────  ║
           ║  Top of every loop iteration:      ║
           ║    GETDEL zombie:{id}:steer        ║
           ║  Found a steer message? →          ║
           ║    XADD zombie:{id}:events         ║
           ║         actor=steer:<user>         ║
           ║         data=<msg>                 ║
           ║  → XREADGROUP unblocks ────────────╫───┐
           ║                                    ║   │
           ║  processNext():                    ║   │
           ║   1. INSERT core.zombie_events     ║   │   See §10 Capabilities
           ║      (status='received')           ║   │   for which spec owns
           ║   2. balance gate, approval gate   ║   │   each layer.
           ║   3. resolve creds from vault      ║   │
           ║   4. executor.createExecution      ║   │
           ║         (workspace_path,           ║   │
           ║          {network_policy,          ║   │
           ║           tools, secrets_map,      ║   │
           ║           context})                ║   │
           ║   5. executor.startStage           ║   │
           ║         (execution_id,             ║   │
           ║          message=<msg>)            ║   │
           ╚═══════════════════════════════════╝   │
                          ↓                         │
           ╔═══════════════════════════════════╗   │
           ║  zombied-executor (RPC over Unix) ║   │
           ║  ───────────────────────────────  ║   │
           ║  handleStartStage(...)             ║   │
           ║  → runner.execute(NullClaw Agent)  ║   │
           ║                                    ║   │
           ║   NullClaw reasons over msg.       ║   │
           ║   Calls tools per its SKILL.md.    ║───┘
           ║   Each tool call → tool bridge     ║
           ║   substitutes ${secrets.NAME.x}    ║       This is the
           ║   at sandbox boundary, then        ║       "ZOMBIE'S AGENT".
           ║   HTTPS request fires.             ║       It's an LLM in a
           ║   Agent returns StageResult.       ║       sandbox; user's
           ║                                    ║       agent never
           ║  → {content, tokens, wall_s,       ║       becomes it,
           ║     exit_ok}                       ║       never sees its
           ╚═══════════════════════════════════╝       tokens or context.
                          ↓
           ╔═══════════════════════════════════╗
           ║  zombied-worker (zombie thread)   ║
           ║  ───────────────────────────────  ║
           ║   6. UPDATE core.zombie_events    ║
           ║      status='processed'            ║
           ║      response_text=<content>       ║
           ║      tokens=N                      ║
           ║      wall_ms=...                   ║
           ║   7. XACK zombie:{id}:events       ║
           ╚═══════════════════════════════════╝
                          ↓
   User's Agent's `zombiectl steer {id}` polls GET /events
   (or SSE-tails core.zombie_activities once live-watch lands)
                          ↓
       [claw] <the zombie's response, streamed>
                          ↓
                  User reads it.
```

### Same flow for webhook and cron triggers

The flow above is identical for webhook and cron triggers — only the entry into the event stream differs:

- **Webhook**: zombied-api's webhook receiver `XADD`s to `zombie:{id}:events` directly with `actor=webhook:<source>` and the normalized payload as `data`. No steer key, no GETDEL — the receiver writes straight to the stream.
- **Cron**: NullClaw's cron runtime fires on schedule and `XADD`s a synthetic event with `actor=cron:<schedule>`.

In all three cases the zombie's reasoning loop sees a single `message` and reasons over it. The actor field is metadata for audit and routing decisions in the SKILL.md prose ("if this is a webhook from GitHub Actions, fetch the run logs first; if this is a cron, do the standing health check").

### What the user's agent never does

- Never sees the zombie's LLM tokens or reasoning state
- Never holds the zombie's credentials in its own context
- Never executes the zombie's tool calls in its own session
- Never persists across the user's laptop being closed

### What the zombie's agent never does

- Never touches the user's laptop directly
- Never reads the user's local filesystem (it sees only what the SKILL.md and TRIGGER.md grant it)
- Never escapes the sandbox — Landlock + cgroups + bwrap enforce egress, fs, and process limits

---

## 10. What the Zombie Has (Capabilities)

Two layers — what the LLM is told it can do, and what the platform actually enforces.

### Reasoning + tool inventory (declared in the zombie's own files)

| File | What it carries | Enforced by |
|---|---|---|
| `SKILL.md` | Natural-language reasoning prompt: how to think, what's safe, what to gather, when to ask for approval. Free-form prose. | The LLM reading its own prompt — soft enforcement |
| `TRIGGER.md` (or merged frontmatter under `x-usezombie:` in a single SKILL.md file) | `tools:` list, `credentials:` list, `network.allow:` list, `budget:` caps, `trigger.type:` (webhook/chat/cron), `context:` budget knobs | Code-enforced at the executor sandbox boundary — the LLM cannot escape these |

### The platform tools the zombie can call (via NullClaw, gated by `tools:` allowlist)

| Tool | Purpose | Visible to agent |
|---|---|---|
| `http_request` | GET/POST to allowlisted hosts. `${secrets.NAME.FIELD}` substituted at the tool bridge after sandbox entry. | Agent sees placeholders; never raw bytes |
| `memory_store` / `memory_recall` | Durable scratchpad keyed by string. Survives stage boundaries and full restart. The "where I am" snapshot mechanism. | Yes — agent reads/writes |
| `cron_add` / `cron_list` / `cron_remove` | Self-schedule future invocations. Each fire arrives as a synthetic event with `actor=cron:<schedule>`. | Yes |
| `shell` (future, gated) | Read-only commands like `docker ps`, `kubectl get`. Not in v1 platform-ops. | Yes (when wired) |

### Platform-level guarantees (the substrate that wraps every tool call)

| Capability | What it does | Owner |
|---|---|---|
| Worker control stream | Watcher thread on `zombie:control` claims new zombies + spawns per-zombie threads + propagates kill within ms (not on the 5s XREADGROUP cycle) | Worker substrate (M40) |
| Per-execution policy | Each `executor.createExecution` carries `secrets_map`, `network_policy`, `tools` list, and `context` knobs. Tool bridge substitutes secrets at sandbox boundary. | Context Layering (M41) |
| Event stream + history | Every steer / webhook / cron event lands on `zombie:{id}:events` with actor provenance. `core.zombie_events` row INSERTed at receive, UPDATEd at completion. | Streaming substrate (M42) |
| Webhook ingest (GH Actions in v1) | HTTP receiver verifies HMAC signature, normalizes payload, writes synthetic event with `actor=webhook:github` | Webhook ingest spec (M43) |
| Credential vault | Stores structured `{host, api_token}` records, KMS-enveloped. Tool bridge substitutes at sandbox entry. | Vault spec (M45) |
| Approval gating | Risky actions block until human clicks Approve in dashboard or Slack DM. State machine survives worker restart. | Approval inbox (M47) |
| Budget caps | Daily $ + monthly $ hard caps; blocks further runs at first trip. Configured per-zombie in TRIGGER.md / `x-usezombie.budget`. | Already enforced via M37 sample shape |
| Per-stage context lifecycle | Rolling tool-result window + memory_store nudge + stage chunking + continuation events. See §11. | Context Layering (M41) — same spec |

### What the platform never does

- Never logs raw secret bytes
- Never echoes secrets in the agent's context
- Never persists secrets in `core.zombie_events`
- Never lets the agent reach a host outside its `network.allow` list
- Never lets the agent exceed its `budget` caps without trip-blocking

---

## 11. Context Lifecycle

Every zombie reasoning loop lives inside a single `runner.execute` call. As the agent makes tool calls, each result lands in the LLM's context. On a long-running incident (30+ tool calls), this can exhaust the model's context window. The platform layers three independent mechanisms — defense in depth, not override — to keep the zombie reasoning past the model's working-memory limit.

### The three knobs

```yaml
# In the zombie's SKILL.md frontmatter under x-usezombie:
x-usezombie:
  context:
    tool_window: auto              # rolling tool-result window size
    memory_checkpoint_every: 5     # call memory_store every N tool calls
    stage_chunk_threshold: 0.75    # % context fill that triggers chunking
```

### How the three layers compose (defense-in-depth, not override)

```
┌─────────────────────────────────────────────────────────────┐
│  Inside one stage (one runner.execute call)                  │
│                                                              │
│  Tool call 1 → result added to context                       │
│  Tool call 2 → result added to context                       │
│  Tool call 3 → result added to context                       │
│  Tool call 4 → result added to context                       │
│  Tool call 5 → ━━━━ checkpoint! agent calls                 │ ← L1 fires here
│                     memory_store("findings_so_far")          │   (every N=5)
│  Tool call 6 → result added                                  │
│  ...                                                         │
│  Tool call 20 → ━━━━ tool_window cap!                        │ ← L2 fires here
│                       oldest results dropped from            │   (when result
│                       context (still in event log)           │    count > N)
│  Tool call 21 → context now bounded again                    │
│  ...                                                         │
│  Tool call 35 → context fill = 78% of model's limit          │
│  Tool call 36 → context fill = 81% ━━━━ chunk!               │ ← L3 fires here
│                  agent writes final snapshot,                │   (at threshold)
│                  returns {exit_ok:false, ...}                │
│  Stage ends. zombied-worker re-enqueues continuation.        │
│                                                              │
│  ──── Next stage starts fresh ────                           │
│  Stage opens with memory_recall("incident:X")                │
│  Continues from where the snapshot left off.                 │
└─────────────────────────────────────────────────────────────┘
```

### What each layer catches

- **L1 — `memory_checkpoint_every`**: runs periodically as the agent works. Forces the agent to write a durable snapshot of "what I've learned so far" via `memory_store` every N tool calls. Cheap and always safe — even if subsequent layers drop context, the snapshot survives.
- **L2 — `tool_window`**: runs continuously. Bounds context growth by dropping the oldest tool results once the count exceeds the cap. Old results stay in `core.zombie_events`; they just leave the active LLM context.
- **L3 — `stage_chunk_threshold`**: the failsafe. When context fill exceeds the threshold (% of the active model's hard limit), the agent writes a final snapshot, returns `{exit_ok: false, content: "needs continuation", checkpoint_id: ...}`, and the worker re-enqueues the same incident as a synthetic event with `actor=continuation`. The next stage starts fresh and immediately calls `memory_recall` to load the snapshot.

The order is failure-mode escalation: L1 keeps your work safe, L2 keeps your context bounded, L3 saves the incident from collapse. They never conflict.

### Defaults — the user shouldn't have to do token math

NullClaw ships with model-tier-aware defaults that the user inherits without any config:

| Active model | `tool_window` (auto) | `memory_checkpoint_every` | `stage_chunk_threshold` |
|---|---|---|---|
| Claude 4.7 (1M) | 30 | 5 | 0.75 |
| Claude Sonnet 4.6 (~200k) | 20 | 5 | 0.75 |
| Claude Haiku 4.5 (~200k) | 20 | 5 | 0.75 |
| Kimi 2.6 (~256k) | 20 | 5 | 0.75 |
| GPT-5 (~256k) | 20 | 5 | 0.75 |

The model being used has a known context cap; NullClaw reads the active model's cap and picks defaults that fit. The user-overridable `auto` value bumps to 30 for ≥1M, 20 for 200-300k, 10 for ≤200k.

### When a user *does* want to override (rare)

| Goal | What to change | How to think about it |
|---|---|---|
| "My zombie loses important findings mid-incident" | `memory_checkpoint_every: 3` | Checkpoint MORE often. Cheap. Always safe. |
| "My zombie hits context limits and chunks too aggressively" | `tool_window: 10` | Drop OLD results sooner so newer stuff fits. May lose context recency. |
| "My zombie chunks too late and produces partial diagnoses" | `stage_chunk_threshold: 0.6` | Chunk EARLIER. More handoffs but less risk of cutoff mid-thought. |
| "I'm on Kimi 2.6 (256k) and incidents are big" | `tool_window: 8` + `memory_checkpoint_every: 3` | Smaller windows + more checkpoints. Standard tight-context discipline. |

### The 80/20 rule

80% of users use defaults forever and never see context errors. 20% who run very-deep incidents tweak `tool_window` once and forget. Almost nobody touches `stage_chunk_threshold`.

---

## 12. End-to-End Technical Sequence (the 11 steps)

This section restores the install → control stream → worker → events → executor sequence that previously lived in `docs/ARCHITECTURE_ZOMBIE_EVENT_FLOW.md` (now removed). The platform-ops zombie under the author's signup is the worked example; numbers tie back to spec implementation slices.

### Process and stream ownership

| Process | Role |
|---|---|
| **zombied-api** (`zombied serve`) | HTTP routes. Writes `core.zombies`, `vault.secrets`, `zombie:control` (produces), `zombie:{id}:steer` (produces). Reads `core.zombie_events` for history. Webhook receivers write directly to `zombie:{id}:events`. |
| **zombied-worker** (`zombied worker`) | Hosts one watcher thread (consumes `zombie:control`) + N zombie threads (each consumes one `zombie:{id}:events`). Owns per-zombie cancel flags. Never runs LLM code. |
| **zombied-executor** (sidecar; `zombied executor`) | Unix-socket RPC server. Hosts NullClaw agent inside Landlock + cgroups + bwrap. Credential substitution lives here. |

### Stream + DB ownership

| Target | Producer | Consumer |
|---|---|---|
| `zombie:control` | zombied-api on `innerCreateZombie`, status change, config PATCH | zombied-worker watcher thread |
| `zombie:{id}:events` | zombied-api on webhook (GH Actions, others). zombied-worker on steer inject. NullClaw cron-tool fires. | zombied-worker's per-zombie thread |
| `zombie:{id}:steer` (Redis key, transient) | zombied-api on `POST /steer` | zombied-worker zombie thread (polls + GETDEL at top of loop) |
| `core.zombie_events` | zombied-worker zombie thread (INSERT on receive, UPDATE on complete) | zombied-api `GET /events`, dashboard, `zombiectl events` |
| `core.zombies` | zombied-api only | zombied-worker at claim + watcher tick |
| `core.zombie_sessions` | zombied-worker (checkpoint + execution_id) | zombied-worker at claim + kill path |
| `vault.secrets` | zombied-api on `credential add` | zombied-worker resolves just-in-time before each `createExecution` |

### The 11 steps (platform-ops worked example)

| # | Action | zombied-api | `zombie:control` | `zombie:{id}:events` | zombied-worker | zombied-executor |
|---|---|---|---|---|---|---|
| 1 | Sign in via Clerk | OAuth callback → INSERT core.users/workspaces | — | — | idle | idle |
| 2-4 | `zombiectl credential add {fly,upstash,slack,github}` with structured `{host, api_token}` fields | `PUT /v1/.../credentials/{name}` → crypto_store.store (KMS envelope) → UPSERT vault.secrets | — | — | idle | idle |
| 5 | `zombiectl install --from .usezombie/platform-ops/` | `innerCreateZombie`: INSERT core.zombies (active). **Atomically + before 201**: `XGROUP CREATE zombie:{id}:events zombie_workers 0 MKSTREAM` + XADD `zombie:control` type=zombie_created. **Invariant 1**: stream + group exist before any producer/consumer can arrive. | +1 entry | stream+group created, empty | idle | idle |
| 6 | Watcher claims | — | — | — | Watcher XREADGROUP on `zombie:control` unblocks. SELECT core.zombies row. Allocates cancel_flag, spawns zombie thread. XACK. Zombie thread claims (loads config + checkpoint), blocks on XREADGROUP `zombie:{id}:events` BLOCK 5s. | idle |
| 7 | **Trigger arrives** — three paths land on the same stream | | | | | |
|  ↳ 7a (webhook) | GitHub Actions posts `workflow_run` failure to `/v1/.../webhooks/github`. Receiver verifies HMAC, normalizes payload. | XADD `zombie:{id}:events` actor=webhook:github data={run_url, head_sha, conclusion, ...} | — | +1 entry | (within ≤5s) zombie thread XREADGROUP returns it | idle |
|  ↳ 7b (cron) | — | — | (NullClaw cron runtime fires) +1 entry actor=cron:<schedule> | — | idle |
|  ↳ 7c (steer) | `innerSteer`: SET `zombie:{id}:steer "<msg>" EX 300`, return 202. | — | — | (within ≤5s) zombie thread's `pollSteerAndInject` GETDEL steer key → XADD `zombie:{id}:events` actor=steer:<user>. Next XREADGROUP returns it. | idle |
| 8a | processEvent starts | — | — | event consumed, in pending list | `processEvent`: INSERT `core.zombie_events` (status='received', actor=<from event>, request_json=msg). Balance gate + approval gate pass. Resolves credentials from vault just-in-time. `executor.createExecution(workspace_path, {network_policy, tools, secrets_map, context})` over Unix socket. `setExecutionActive`. `executor.startStage(execution_id, {agent_config, message, context})`. | `handleCreateExecution` creates session storing policy + context knobs. `handleStartStage` invokes `runner.execute` → NullClaw `Agent.runSingle`. **Wakes.** |
| 8b | Agent runs inside executor | — | — | — | waiting on Unix socket | NullClaw makes tool calls per the SKILL.md prose. The order the agent decides; example for GH Actions failure: `http_request GET https://api.github.com/repos/{repo}/actions/runs/{run_id}/logs` (tool-bridge substitutes `${secrets.github.api_token}` after sandbox entry, agent never sees raw bytes); `http_request GET ${fly.host}/v1/apps/{app}/logs`; `http_request GET ${upstash.host}/v2/redis/stats/{db}`; `http_request POST ${slack.host}/api/chat.postMessage` with diagnosis. **L1 (memory_checkpoint_every) fires every N tool calls; L2 (tool_window) bounds context growth; L3 (stage_chunk_threshold) escalates to continuation if threshold breached.** **Optional**: `cron_add "*/30 * * * *" "post-recovery health check"` if SKILL.md prose requests it. |
| 8c | Agent returns StageResult | — | — | — | Receives `{content, tokens, wall_s, exit_ok}` on Unix socket. updateSessionContext (in-memory). Defers destroyExecution + clearExecutionActive. | `runner.execute` returns; session destroyed on handler side; executor **sleeps** (no other work). |
| 8d | zombie thread finalizes | — | — | XACK | UPDATE core.zombie_events (status='processed', response_text, tokens, wall_ms, completed_at). checkpointState → UPSERT core.zombie_sessions. metering.recordZombieDelivery. XACK. CLI's `zombiectl steer` session (polling GET `/events` or tailing activity stream) picks up the new row and prints `[claw] <response_text>`. Back to XREADGROUP BLOCK. | idle |
| 9 | Read history | `GET /v1/.../zombies/{id}/events` reads core.zombie_events. CLI: `zombiectl events {id}`. | — | — | idle | idle |
| 10 | `zombiectl kill {id}` | UPDATE core.zombies SET status='killed'. XADD `zombie:control` type=zombie_status_changed status=killed. | +1 entry | — | Watcher reads control msg, sets `cancels[id].store(true)`, reads execution_id from zombie_sessions, calls `executor_client.cancelExecution(execution_id)`. Zombie thread's watchShutdown sees cancel_flag → running=false → exits loop → thread returns. | If mid-stage: `handleCancelExecution` flips session.cancelled=true; in-flight `runner.execute` breaks out with `.cancelled`. |
| 11 | Audit grep — verify no secret leak | manual `grep <token-bytes> logs/* db_dump.sql` | — | — | token bytes held only transiently during `createExecution` RPC | token bytes held only in session memory + emitted inline into HTTPS TCP to upstream — never logged, never written to disk |

### Notable invariants this sequence proves

- **No race on stream/group creation.** `innerCreateZombie` does INSERT + XGROUP CREATE + XADD `zombie:control` synchronously before returning 201. Any webhook arriving within microseconds of the 201 finds the stream already there.
- **All triggers funnel into one reasoning loop.** Webhook, cron, and steer are different *producers* into `zombie:{id}:events`; the worker's per-zombie thread doesn't branch on actor type.
- **Credentials never enter agent context.** Substitution happens at the tool bridge, inside the executor, after sandbox entry. Agent sees `${secrets.fly.api_token}`; HTTPS request headers get real bytes; responses never echo the token.
- **Kill is immediate for in-flight runs.** Control-stream XADD triggers `cancelExecution` RPC within milliseconds — not the 5s XREADGROUP cycle.
- **Long-running stages don't crash the model.** L1+L2+L3 (§11) keep context bounded; if a single incident exceeds budget the zombie chunks and continues in a new stage from a `memory_recall` snapshot.

---

## 13. Path to Bastion / Customer-Facing Statuspage

The MVP wedge ships an internal-only diagnosis posted to the operator's Slack. The longer-term play is the **bastion** — a single durable surface where:

- internal triage continues as today (Slack post, evidence trail, follow-up steers)
- external customer communication is automatically derived from the same incident state (statuspage updates, broadcast email/SMS, embedded `<iframe>` widgets)
- the same zombie owns both — the diagnosis and the customer-facing narrative come from one event log, not two

This is post-MVP. It's the shape that competes structurally with Atlassian Statuspage's manual-update model and with the AI-statuspage-automation tools (Dust, Relevance AI, PageCalm) that bolt LLMs onto an external statuspage product.

### What changes structurally to get from MVP to bastion

1. **Per-zombie "audience" routing**: TRIGGER.md / `x-usezombie:` adds `audiences: [internal_slack, customer_status, customer_email]`. The zombie's SKILL.md prose teaches it to draft different summaries per audience from the same evidence.
2. **Status-page rendering surface**: a hosted page at `status.<customer-domain>` renders the latest `processed` event's customer-facing summary. Updates as new events land.
3. **Broadcast channels**: the zombie's `tools:` list grows to include `email_send`, `sms_send` (gated, approval-required for first incident), `webhook_post` (for downstream Statuspage / PagerDuty / etc.).
4. **Approval gating per audience**: the SKILL.md prose can require human approval before posting to customer-facing audiences while letting internal Slack go through automatically. M47 approval inbox handles the mechanic.
5. **Retention + replay for compliance**: customer-facing comms have stricter retention requirements (SOX, GDPR). `core.zombie_events` retention policy becomes per-actor configurable.

### What does NOT change

- The runtime architecture (worker → session → streaming).
- The sandbox boundary (Landlock + cgroups + bwrap).
- The trigger model (webhook, cron, steer).
- The credential vault, network policy, budget caps, context lifecycle.

The bastion is a SKILL.md authoring pattern + a few new tool primitives + a new rendering surface. It is not a different product. The MVP's job is to earn enough trust on internal-only diagnoses that customers feel safe letting the same zombie talk to *their* customers.

---

## Appendix — Glossary

| Term | Meaning |
|---|---|
| **Zombie** | A long-lived, durable runtime instance defined by a SKILL.md + TRIGGER.md (or merged frontmatter). Owns one operational outcome. |
| **NullClaw** | The LLM agent loop that runs inside the executor sandbox. The "zombie's agent." |
| **User's agent** | Claude Code, Amp, Codex CLI, OpenCode, etc. — the workstation tool the human types into and that drives `zombiectl`. Distinct from the zombie's agent. |
| **Steer** | An operator-initiated message sent to a zombie via `zombiectl steer {id} "<msg>"` or the dashboard chat widget. Lands as an event with `actor=steer:<user>`. |
| **Webhook trigger** | An external system (GitHub Actions, etc.) POSTing to `/v1/.../webhooks/<source>`. Lands as an event with `actor=webhook:<source>`. |
| **Cron trigger** | A NullClaw-managed schedule firing on time. Lands as an event with `actor=cron:<schedule>`. |
| **Stage** | One `runner.execute` call inside the executor — one LLM context window's worth of reasoning. Long incidents span multiple stages via continuation events. |
| **Tool bridge** | The substitution layer inside the executor that replaces `${secrets.NAME.FIELD}` placeholders with real bytes after sandbox entry. |
| **Bastion** | The post-MVP framing where the same zombie owns both internal triage AND customer-facing status communication for an incident. |
