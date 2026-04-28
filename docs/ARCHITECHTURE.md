# Architecture — v2 MVP Operational Outcome Runner

> **Filename note:** This file is `ARCHITECHTURE.md` (with the extra "CH"), not `ARCHITECTURE.md`. The misspelling is the de-facto canonical name in this repo — every M40-M51 spec, the README, `playbooks/ARCHITECHTURE.md`, and the `office_hours_v2.md` / `plan_engg_review_v2.md` docs reference it under this exact filename. Do not "correct" the spelling without sweeping all ~20 cross-references in the same change; the rename is more expensive than the embarrassment.

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

### How we differentiate

Three structural pillars carry v2:

- **OSS** — the runtime is open source. The operator can read the code that holds their credentials and runs against their infrastructure.
- **BYOK** — operators bring their own LLM provider key (Anthropic, OpenAI, Together, Groq). The executor treats it as another secret resolved at the tool bridge. No vendor lock-in on inference cost.
- **Markdown-defined** — operational behavior lives in `SKILL.md` + `TRIGGER.md`, not in a typed workflow engine. Iteration is editing prose, not redeploying code.

**Self-host is deferred to v3.** v2 ships hosted-only on `api.usezombie.com` via Clerk OAuth. The architecture admits self-host (the auth substrate, KMS adapter, and process orchestration are the only deployment-specific layers), but validating it on a clean non-Fly Linux host — the Clerk shim, the KMS adapter, and the executor's Landlock+cgroups+bwrap sandbox running on a vanilla VM — is a v3 workstream. Reading the codebase as OSS is supported in v2; running the codebase on your own infra is not.

The `/usezombie-install-platform-ops` skill (§8.0) is what makes the v2 pillars reachable from a cold start — repo detection, ≤4 gating questions, host-neutral so it works from Claude Code, Amp, Codex CLI, or OpenCode.

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

### 8.0 The wedge surface: `/usezombie-install-platform-ops` skill

The MVP's user-facing wedge is not raw `zombiectl install`. It is a host-neutral SKILL.md invoked as **`/usezombie-install-platform-ops`** — the same slash-command in every host (Claude Code, Amp, Codex CLI, OpenCode). One install procedure: drop the SKILL.md directory into the host's skills folder (`~/.claude/skills/usezombie-install-platform-ops/` or the host-equivalent path), or fetch it from `https://usezombie.sh/skills.md`. No plugin manifest, no per-host packaging fork. The brand is in the slash-command itself; future skills follow the same pattern (`/usezombie-steer`, `/usezombie-doctor`).

The skill is the install UX; `zombiectl install --from <path>` is the substrate it drives.

What the skill does, in order:

1. **Detects the user's repo**: reads `.github/workflows/*.yml`, `fly.toml`, `Dockerfile`, `pyproject.toml`, `package.json` to infer CI provider, deploy target, and Slack channel. Bails clearly if no GitHub Actions workflow is detected (non-GH CI is post-MVP).
2. **Asks at most three or four gating questions** through the host-neutral `variables:` frontmatter (so the same SKILL.md works on Claude Code, Amp, Codex CLI, and OpenCode without `AskUserQuestion` lock-in). Slack channel, prod branch glob, and cron opt-in.
3. **Resolves credentials in order**: 1Password CLI (`op read`) → environment variables → interactive prompt. The skill never asks again for what `op` already has.
4. **Calls `zombiectl doctor --json` first** (see §8.2) to verify auth + workspace binding before any write.
5. **Generates `.usezombie/platform-ops/{SKILL,TRIGGER,README}.md`** in the user's repo with substituted values, refusing to overwrite without `--force`. These files are committed by the user — they are the configuration, version-controlled by design.
6. **Drives `zombiectl install --from .usezombie/platform-ops/`** then opens an interactive `zombiectl steer {id}` session.

This matters architecturally for two reasons. First, the skill artifact is portable — it is a markdown file, not a Claude-specific binary. The same wedge installs from any agent CLI that can read SKILL.md. Second, the skill is the only place where repo detection, secret resolution, and ≤4 question discipline are enforced. The runtime stays prompt-driven; the install UX is what makes the prompt-driven runtime tractable for a first-time operator.

### 8.0.1 Deployment posture: hosted-only in v2

v2 ships **hosted-only** on `api.usezombie.com`. The skill detects no choice point: it defaults to the hosted endpoint, prompts Clerk OAuth via `zombiectl auth login` if the CLI is not authenticated, and proceeds. There is no self-host runbook in v2 and no `--self-host` flag.

This is a deliberate scope cut, not a gap in the architecture. The runtime is already structured so the auth substrate (Clerk OAuth), KMS adapter (cloud KMS), and process orchestration (Fly.io machines) are the only deployment-specific layers — the worker, executor, sandbox, event stream, and reasoning loop are all posture-agnostic. **Validating** that on a clean non-Fly Linux host (Clerk shim or local-token auth, a portable KMS adapter, executor's Landlock+cgroups+bwrap on a vanilla VM, systemd orchestration) is a v3 workstream once v2 has earned the trust to justify the integration burden.

Practically, this means:

- v2 launch claim is **OSS + BYOK + markdown-defined** (§0). Not "self-hostable."
- The `/self-host` runbook page does not exist on `docs.usezombie.com` for v2.
- Operators who need self-host today are out of scope; the AI-infra / GPU-cloud / regulated mid-market personas in `office_hours_v2.md` P1 are v3 customers, not v2.
- BYOK still ships in v2 — it sits on top of the hosted posture and removes the inference-cost lock-in independently of where the runtime runs. See §10.

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

1. Claude (or another agent), typically driven by the `/usezombie-install-platform-ops` skill (§8.0), helps author or refine `SKILL.md` and `TRIGGER.md`.
2. **`zombiectl doctor --json` runs first** as the deterministic readiness gate. Doctor is auth-exempt, fast, and verifies four things: token validity, server reachability, an active workspace, and workspace binding for the current CLI. The skill (and any future caller) reads `doctor`'s JSON output verbatim and aborts on failure with the operator-facing message instead of letting `install` fail with a confusing 401. Doctor is the only sanctioned preflight surface — no parallel `preflight` command exists.
3. The user (or skill) installs or updates the zombie through `zombiectl install --from <path>` or the API. The CLI POSTs `{name, config_json, source_markdown}`; the API parses frontmatter, persists the zombie row, and synchronously creates the events stream + consumer group before returning 201 (see §12, Invariant 1).
4. The API stores the zombie config, linked credentials reference, approval policy, and trigger settings.
5. The worker runtime becomes responsible for future triggers — no worker restart required (the watcher thread on `zombie:control` claims the new zombie within milliseconds).

After install, the zombie is no longer tied to the interactive Claude session that created it.

### 8.3 Triggering the zombie

For the MVP, the zombie is triggerable in three ways:

- **Webhook input**: an external system (most importantly GitHub Actions on `workflow_run.conclusion == failure`) sends an event to `POST /v1/.../webhooks/github`. The receiver verifies the HMAC signature, normalizes the payload, and lands a synthetic event on `zombie:{id}:events` with `actor=webhook:github`.
- **Cron input**: NullClaw's `cron_add` tool persists a schedule. Each fire arrives as a synthetic event with `actor=cron:<schedule>`.
- **Operator steer**: the user, while in Claude, asks to run an operational task. Claude invokes `zombiectl steer {id} "<message>"` (or the dashboard chat widget), which `XADD`s directly to `zombie:{id}:events` with `actor=steer:<user>` — the same single-ingress path webhook and cron use.

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
3. The receiver normalizes the payload into a synthetic event and `XADD`s to `zombie:{id}:events` with `actor=webhook:github`, `type=webhook`, `workspace_id={ws}`, `request={run_url, head_sha, conclusion, ref, repo, attempt}`, `created_at=<epoch_ms>`.
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
           ║  XADD zombie:{id}:events *         ║   ← single ingress.
           ║       actor=steer:<user>           ║     Webhook + cron use
           ║       type=chat                    ║     the same XADD.
           ║       workspace_id=<uuid>          ║     No SET/GETDEL key.
           ║       request=<msg-json>           ║
           ║       created_at=<epoch_ms>        ║
           ║  → 202 { event_id }                ║
           ╚═══════════════════════════════════╝
                          ↓
           ╔═══════════════════════════════════╗
           ║  zombied-worker (zombie thread)   ║
           ║  ───────────────────────────────  ║
           ║  XREADGROUP unblocks ──────────────╫───┐
           ║                                    ║   │
           ║  processEvent():                   ║   │
           ║   1. INSERT core.zombie_events     ║   │   ← narrative log
           ║      (status='received',           ║   │     opens (mutable)
           ║       actor, request_json)         ║   │
           ║   2. PUBLISH zombie:{id}:activity  ║   │   ← live: pub/sub
           ║      {kind:"event_received"}       ║   │     channel (ephemeral,
           ║                                    ║   │     no buffer, no ACK)
           ║   3. balance gate, approval gate   ║   │   See §10 Capabilities
           ║   4. resolve creds from vault      ║   │   for which spec owns
           ║   5. UPSERT core.zombie_sessions   ║   │   each layer.
           ║      SET execution_id, started_at  ║   │   ← resume cursor:
           ║      (one row per zombie, mutable) ║   │     marks zombie busy
           ║   6. executor.createExecution      ║   │
           ║         (workspace_path,           ║   │
           ║          {network_policy, tools,   ║   │
           ║           secrets_map, context})   ║   │
           ║   7. executor.startStage           ║   │
           ║         (execution_id, message)    ║   │
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
           ║                                    ║       sandbox; user's
           ║   For each progress event,         ║       agent never
           ║   the worker (NOT the executor)    ║       becomes it,
           ║   PUBLISHes zombie:{id}:activity:  ║       never sees its
           ║     - tool_call_started            ║       tokens or context.
           ║     - agent_response_chunk         ║
           ║     - tool_call_completed          ║
           ║                                    ║
           ║   Agent returns StageResult.       ║
           ║  → {content, tokens, ttft_ms,      ║
           ║     wall_ms, exit_ok}              ║
           ╚═══════════════════════════════════╝
                          ↓
           ╔═══════════════════════════════════╗
           ║  zombied-worker (zombie thread)   ║
           ║  ───────────────────────────────  ║
           ║   8. UPDATE core.zombie_events     ║   ← narrative log
           ║      status='processed'            ║     closes (same row)
           ║      response_text=<content>       ║
           ║      completed_at=now()            ║
           ║   9. INSERT zombie_execution_      ║   ← billing/latency
           ║      telemetry                     ║     audit (immutable,
           ║      (event_id UNIQUE, tokens,     ║     UNIQUE event_id)
           ║       ttft_ms, wall_seconds,       ║
           ║       plan_tier, credit_cents)     ║
           ║  10. UPSERT core.zombie_sessions   ║   ← resume cursor:
           ║      SET context_json={last_       ║     clears execution
           ║         event_id, last_response},  ║     handle, advances
           ║      execution_id=NULL,            ║     bookmark
           ║      checkpoint_at=now()           ║
           ║  11. PUBLISH zombie:{id}:activity  ║   ← live: terminal
           ║      {kind:"event_complete",       ║     SSE frame
           ║       status:"processed"}          ║
           ║  12. XACK zombie:{id}:events       ║
           ╚═══════════════════════════════════╝
                          ↓
   User's Agent's `zombiectl steer {id}` polls GET /events
   (or SSE-tails GET /events/stream which SUBSCRIBEs
    zombie:{id}:activity)
                          ↓
       [claw] <the zombie's response, streamed>
                          ↓
                  User reads it.
```

### The three durable stores: who owns what

The flow above writes to three Postgres tables. They are **not** redundant — each answers a distinct operator question, has a different cardinality, mutability, and retention contract. Use the right one for the right question.

| Table | Cardinality | Mutability | Answers |
|---|---|---|---|
| `core.zombie_sessions` | **One row per zombie** | UPSERT — mutated on every event boundary | "Where is this zombie *right now*? Is it idle or executing? What was its last successful response?" — the worker's resume bookmark + active-execution handle. Read at claim, written at start + end of each event. |
| `core.zombie_events` | **One row per delivery** | INSERT (status=`received`) → UPDATE (status=`processed` \| `agent_error` \| `gate_blocked`) | "What did this zombie do for event X? Who triggered it, what did they ask, what did it answer, did the gates pass?" — the operator's narrative log. The single source of truth for the Events tab and `zombiectl events`. |
| `zombie_execution_telemetry` | **One row per delivery** (UNIQUE `event_id`) | INSERT once at end, immutable | "How much did event X cost? How fast was it? Which plan tier was charged?" — billing + latency audit. Joinable to `zombie_events` via `event_id`. Aggregated for p95 latency, token-spend rollups, credit deductions. |

Why two per-delivery tables (`events` + `telemetry`) instead of one? They have different write authorities and retention contracts:

- `zombie_events` holds operator-readable strings (`request_json`, `response_text`) — large, mutable mid-lifecycle, deletable on tenant offboarding.
- `zombie_execution_telemetry` holds numeric audit columns — small, immutable once written, retained for billing reconciliation independent of whether the conversation row is purged.

### Concrete platform-ops example

A GitHub Actions deploy fails on `usezombie/usezombie@c0a151bd`. The webhook lands as `event_id=1729874000000-0`, `actor=webhook:github`. Here is exactly what each row holds at each stage.

**Before the event** — `zombie_sessions` shows the zombie idle since the previous event:

```
core.zombie_sessions  (one row, the zombie itself)
─────────────────────────────────────────────────
zombie_id            f4e3c2b1-...
context_json         {"last_event_id": "1729873200000-0",
                      "last_response":  "All apps healthy at 07:30Z."}
checkpoint_at        1729873208000
execution_id         NULL          ← idle
execution_started_at NULL
```

**Step 1 — INSERT `zombie_events`** (status=`received`):

```
core.zombie_events  (new row, narrative-log opens)
──────────────────────────────────────────────────
zombie_id      f4e3c2b1-...
event_id       1729874000000-0
workspace_id   8d2e1c9f-...
actor          webhook:github
event_type     webhook
status         received
request_json   {
  "message":  "GH Actions workflow_run failure on
               usezombie/usezombie deploy.yml run 9876",
  "metadata": {"run_id": 9876, "head_sha": "c0a151bd",
               "conclusion": "failure", "ref": "main",
               "repo": "usezombie/usezombie", "attempt": 1}
}
response_text  NULL
created_at     2026-04-25T08:00:00Z
completed_at   NULL
```

**Step 5 — UPSERT `zombie_sessions`** (mark busy, do *not* touch `zombie_events`):

```
core.zombie_sessions  (same row, mutated)
─────────────────────────────────────────
execution_id         exec-7af3c2b1-...   ← now busy
execution_started_at 1729874001000
(other fields unchanged from "before")
```

NullClaw runs in the executor: fetches GH run logs via `${secrets.github.api_token}`, fetches Fly app logs, fetches Upstash Redis stats, posts a remediation message to Slack. Returns `StageResult{content, tokens=1840, wall_ms=8210, ttft_ms=320, exit_ok=true}`.

**Step 7 — UPDATE `zombie_events`** (close the same row):

```
core.zombie_events  (same row, narrative-log closes)
────────────────────────────────────────────────────
status         processed
response_text  "Deploy failed: Fly.io OOM kill on machine i-01abc,
                app over 4GB resident. Last successful migration at
                c0a151bc. Posted to #platform-ops with rollback-to-
                c0a151bc remediation."
completed_at   2026-04-25T08:00:08Z
```

**Step 8 — INSERT `zombie_execution_telemetry`** (immutable audit row, joinable on `event_id`):

```
zombie_execution_telemetry  (new row, write-once)
─────────────────────────────────────────────────
id                       tel-1729874000000-0
zombie_id                f4e3c2b1-...
workspace_id             8d2e1c9f-...
event_id                 1729874000000-0   ← UNIQUE; joins to zombie_events
token_count              1840
time_to_first_token_ms   320
wall_seconds             8
epoch_wall_time_ms       1729874000000
plan_tier                free
credit_deducted_cents    4
recorded_at              1729874008210
```

**Step 9 — UPSERT `zombie_sessions`** (advance bookmark, clear execution handle):

```
core.zombie_sessions  (same row, mutated)
─────────────────────────────────────────
context_json         {"last_event_id": "1729874000000-0",
                      "last_response":  "Deploy failed: Fly.io OOM kill..."}
checkpoint_at        1729874008210
execution_id         NULL          ← idle again
execution_started_at NULL
```

### Reading the three tables

- `zombiectl status {id}` reads **`zombie_sessions`** — answers "is the zombie executing right now, and where did it leave off?"
- `zombiectl events {id} [--actor=…]` reads **`zombie_events`** — answers "what has this zombie done, what was asked, what did it reply, did any gate block it?"
- Billing rollups + p95 dashboards read **`zombie_execution_telemetry`** — answers "how many tokens this month, what's the latency tail?"

If only **one** table existed, every operator query would either pay full-table-scan cost (one row per delivery for "is it busy now?") or lose immutability guarantees on billing audit (mutable narrative columns alongside immutable spend columns). Three tables, three contracts, one join key (`event_id`).


  Two streams + one pub/sub channel — three surfaces, three jobs

  ┌──────────────────────┬─────────────────┬──────────────────────────────────────────────────────────────────────────────────────────────────────────┬───────────────┬────────────────────────────────┐
  │  Redis surface       │     Type        │                                                  Purpose                                                  │  Cardinality  │            Volume              │
  ├──────────────────────┼─────────────────┼──────────────────────────────────────────────────────────────────────────────────────────────────────────┼───────────────┼────────────────────────────────┤
  │ zombie:control       │ Stream + group  │ Lifecycle signals (created / status_changed / config_changed / drain_request) — tells the watcher to       │ ONE,          │ Low — only on                  │
  │                      │ zombie_workers  │ spawn/cancel/reconfig per-zombie threads                                                                   │ fleet-wide    │ install/kill/patch             │
  ├──────────────────────┼─────────────────┼──────────────────────────────────────────────────────────────────────────────────────────────────────────┼───────────────┼────────────────────────────────┤
  │ zombie:{id}:events   │ Stream + group  │ Single event ingress — steer / webhook / cron / continuation all XADD here. At-least-once delivery via    │ ONE PER       │ High — every event the zombie  │
  │                      │ zombie_workers  │ XREADGROUP, XACKed at end of processEvent. Idempotent on replay via INSERT ON CONFLICT.                    │ ZOMBIE        │ handles                        │
  ├──────────────────────┼─────────────────┼──────────────────────────────────────────────────────────────────────────────────────────────────────────┼───────────────┼────────────────────────────────┤
  │ zombie:{id}:activity │ Pub/sub channel │ Best-effort live tail — worker PUBLISHes one frame per event_received, tool_call_started,                  │ ONE PER       │ High during execution, zero    │
  │                      │ (no group, no   │ agent_response_chunk, tool_call_progress (~2s heartbeat during long tool calls), tool_call_completed,      │ ZOMBIE        │ when idle. Subscribers get     │
  │                      │ persistence)    │ event_complete. SSE handler SUBSCRIBEs and forwards. No buffer, no ACK, no resume. If a frame drops, fall   │               │ messages only while connected. │
  │                      │                 │ back to GET /events for the durable record.                                                                │               │                                │
  └──────────────────────┴─────────────────┴──────────────────────────────────────────────────────────────────────────────────────────────────────────┴───────────────┴────────────────────────────────┘

  The two streams are durable (events appended, XACKed entries pruned) and back the at-least-once delivery contract. The pub/sub channel is ephemeral and exists only to power live operator UIs — its
  loss never affects correctness, only what the operator sees in real time. Durable activity history lives in core.zombie_events; the pub/sub channel is the eyeballs surface, not the audit surface.

  Why a single zombie:control instead of per-tenant control

  Considered alternatives:
  - Per-tenant control (zombie:control:{workspace_id}): would force every worker to XREADGROUP on N streams. Discovery problem (which tenants exist?). High-cardinality BLOCK polling with no traffic.
  - Per-zombie control: collapses control plane into data plane — no longer a control plane.
  - Single zombie:control ✓: one XREADGROUP per worker, exactly-once delivery via consumer group, payload carries workspace_id + zombie_id. Multi-tenancy is encoded in the message body, not the stream
  key. Tenant boundary is enforced at the PG layer (RLS on core.zombies) — Redis stays fleet-wide.

  End-to-end sequence

  A. INSTALL  (zombiectl install --from <path>)
  ────────────────────────────────────────────

     user / install-skill
      │  POST /v1/workspaces/{ws}/zombies
      │  body: { name, config_json, source_markdown }
      ▼
    zombied-api (innerCreateZombie)
      │
      ├─► [PG]    INSERT core.zombies          (RLS: tenant boundary)
      ├─► [PG]    INSERT core.zombie_sessions  (idle row: execution_id=NULL,
      │                                         context_json={}, checkpoint_at=now)
      ├─► [Redis] XGROUP CREATE MKSTREAM zombie:{id}:events zombie_workers 0
      ├─► [Redis] XADD zombie:control * type=zombie_created
      │                                  zombie_id={id} workspace_id={ws}
      └─► 201 to user  (invariant 1: data stream + group exist before 201)

    zombied-worker:watcher  (any replica, exactly-once via zombie_workers)
      │  XREADGROUP zombie_workers <consumer> COUNT 16 BLOCK 5000
      │             STREAMS zombie:control >
      │
      ├─► SELECT core.zombies + core.zombie_sessions  (config + resume cursor)
      ├─► spawn per-zombie thread on this worker
      │    └─► thread XREADGROUPs zombie:{id}:events
      │        with consumer name worker-{pid}:zombie-{id}
      └─► XACK zombie:control

     ≤1s end-to-end from 201 to thread-ready. No worker restart.

     At rest:
       PG:    core.zombies row, core.zombie_sessions idle row.
              No core.zombie_events. No zombie_execution_telemetry.
       Redis: stream zombie:{id}:events with group zombie_workers (empty).
              Channel zombie:{id}:activity does not yet exist (implicit on
              first PUBLISH).
       Worker: one thread per replica blocked on XREADGROUP.

  B. TRIGGER  (steer / webhook / cron — three callers, ONE ingress)
  ─────────────────────────────────────────────────────────────────

     Common envelope (every XADD on zombie:{id}:events carries these
     five fields; the stream entry id IS the canonical event_id —
     never carry a separate id in the payload):

         actor         steer:<user> | webhook:<source> | cron:<schedule>
                       | continuation:<original_actor>
         type          chat | webhook | cron | continuation
         workspace_id  <uuid>
         request       <opaque JSON — the message + metadata>
         created_at    <epoch milliseconds; project bigint convention>

     STEER     zombiectl steer {id} "morning health check"
                 → POST /v1/.../zombies/{id}/steer
                 → XADD zombie:{id}:events *
                        actor=steer:kishore  type=chat
                        workspace_id=<ws>    request=<msg>
                        created_at=<ms>
                 → 202 { event_id }                ← CLI uses event_id
                                                     to filter SSE frames

     WEBHOOK   GH Actions posts workflow_run failure
                 → POST /v1/.../webhooks/github   (HMAC-SHA256 verified)
                 → XADD zombie:{id}:events *
                        actor=webhook:github  type=webhook
                        workspace_id=<ws>     request=<normalized-json>
                        created_at=<ms>
                 → 202

     CRON      NullClaw cron-tool fires on schedule (in-executor)
                 → XADD zombie:{id}:events *
                        actor=cron:0_*/30_*_*_*  type=cron
                        workspace_id=<ws>        request=<msg>
                        created_at=<ms>

     CONTINUATION  worker re-enqueue (chunk-continuation OR M47
                   gate-resolved fulfillment)
                 → XADD zombie:{id}:events *
                        actor=continuation:<original_actor>
                        type=continuation
                        workspace_id=<ws>  request=<continuation-msg>
                        created_at=<ms>
                   The new event's row carries
                   resumes_event_id=<immediate_parent_event_id>.
                   Continuation actor is FLAT — never re-nests
                   `continuation:` (a steer that chunks 3 times produces
                   `actor=continuation:steer:kishore` on every continuation,
                   not `continuation:continuation:continuation:...`).

     All four producers land the same envelope on the same stream. The
     reasoning loop never branches on actor — actor is metadata for the
     SKILL.md prose and the operator's history filter.

  C. EXECUTE  (worker → executor → tables → activity → XACK)
  ──────────────────────────────────────────────────────────

     per-zombie thread (XREADGROUP-blocked on zombie:{id}:events)
      │  unblocks with new entry
      ▼
     processEvent(envelope):

       1. INSERT core.zombie_events                  ← narrative log opens
            (zombie_id, event_id, workspace_id, actor, event_type,
             status='received', request_json, created_at)
            ON CONFLICT (zombie_id, event_id) DO NOTHING   (idempotent
                                                            on XAUTOCLAIM)

       2. PUBLISH zombie:{id}:activity               ← live: ephemeral
            { kind:"event_received", event_id, actor }     pub/sub, no buffer

       3. Gates:  balance, approval.
            Blocked → UPDATE core.zombie_events SET status='gate_blocked',
                                                    failure_label=<gate>,
                                                    updated_at=now_ms
                      → PUBLISH zombie:{id}:activity
                          { kind:"event_complete", event_id,
                            status:"gate_blocked" }
                      → XACK zombie:{id}:events       ← row-terminal:
                        gate_blocked rows are NEVER reopened. When the
                        gate resolves (M47 Approval Inbox), a fresh
                        XADD lands with actor=continuation:<original>,
                        type=continuation, resumes_event_id=<blocked>,
                        producing a NEW zombie_events row whose
                        lifecycle is independent. The original blocked
                        row stays as the historical record.

                        Until M47 ships: workspace-admin-gated fallback
                        endpoint POST /v1/.../zombies/{id}/events/{event_id}/admin-resume
                        synthesises the continuation XADD on the
                        operator's behalf, audit-logged, idempotent
                        (409 on already-resumed). Removed when M47 lands.

       4. resolveSecretsMap from vault (per-zombie credentials).

       5. UPSERT core.zombie_sessions                ← worker marks busy
            SET execution_id, execution_started_at = now()

       6. executor.createExecution(workspace_path, {
            network_policy, tools, secrets_map, context })
              (RPC over Unix socket to zombied-executor)
            → returns execution_id

       7. executor.startStage(execution_id, message)
            │
            │  Executor RPC speaks rpc_version: 2 (HELLO handshake on
            │  socket connect; mismatch → executor.rpc_version_mismatch
            │  fast-fail, no v1 compat shim pre-v2.0.0).
            │
            │  Reply for StartStage is multiplexed over the same Unix
            │  socket: zero-or-more JSON-RPC Progress notifications
            │  followed by exactly ONE terminal result frame, all
            │  sharing the StartStage request id. The worker dispatches
            │  each progress frame to its on_progress handler before
            │  the next read; the handler PUBLISHes to the activity
            │  channel.
            │
            │  args_redacted is built INSIDE the executor before the
            │  frame leaves the RPC boundary: any byte range that came
            │  from a secrets_map[NAME][FIELD] substitution is replaced
            │  with ${secrets.NAME.FIELD} placeholder. Resolved secret
            │  bytes never appear on this RPC channel and therefore
            │  never reach the activity pub/sub.
            ▼
            on tool_call_started   → PUBLISH zombie:{id}:activity
                                       { kind:"tool_call_started",
                                         name, args_redacted }
            on agent_response_chunk → PUBLISH zombie:{id}:activity
                                       { kind:"chunk", text }
            on tool_call_progress  → PUBLISH zombie:{id}:activity
                                       { kind:"tool_call_progress",
                                         name, elapsed_ms }
                                     (~2s heartbeat for any tool call
                                      still in flight; absence past
                                      ~5s renders as "stuck" in the UI)
            on tool_call_completed → PUBLISH zombie:{id}:activity
                                       { kind:"tool_call_completed",
                                         name, ms }
            │
            └─ terminal: StageResult{ content, tokens, ttft_ms,
                                      wall_ms, exit_ok }

       8. UPDATE core.zombie_events                  ← narrative log closes
            SET status = exit_ok ? 'processed' : 'agent_error',
                response_text, completed_at = now()

       9. INSERT zombie_execution_telemetry          ← billing/latency,
            (event_id UNIQUE, token_count,             immutable, write-once
             time_to_first_token_ms, wall_seconds,
             plan_tier, credit_deducted_cents)

      10. UPSERT core.zombie_sessions                ← idle bookmark
            SET context_json = { last_event_id, last_response },
                execution_id = NULL, checkpoint_at = now()

      11. PUBLISH zombie:{id}:activity               ← live: terminal frame
            { kind:"event_complete", event_id, status }

      12. XACK zombie:{id}:events                    ← consumer group
                                                       cursor advances

     Crash mid-event → worker restarts. IF an XAUTOCLAIM sweep is wired
     (currently a v2 followup; see line 936 — v2.0 launches single-replica
     and does not yet ship the sweep), the pending entry is handed to a
     new consumer name (the same worker process post-restart, with a
     new pid → new consumer name worker-{newpid}:zombie-{id}) inside
     zombie_workers. Step 1's ON CONFLICT (zombie_id, event_id) DO NOTHING
     and the UNIQUE event_id on zombie_execution_telemetry guarantee
     the replay is safe — exactly one zombie_events row, exactly one
     telemetry row — regardless of how many redelivery attempts occur.
     M42 makes the WRITE PATH replay-safe; the RECLAIM mechanism itself
     is M40 / v2 followup territory.

  D. WATCH  (operator-side: how the live tail surfaces)
  ─────────────────────────────────────────────────────

     CLI       zombiectl steer {id}        (interactive REPL)
                 → opens GET /v1/.../zombies/{id}/events/stream (SSE)
                 → server SUBSCRIBE zombie:{id}:activity on a dedicated
                   Redis connection held outside the request-handler pool
                   (SUBSCRIBE blocks the conn).
                 → forward each PUBLISH as an SSE frame, one per line:
                     id:<seq>\nevent:<kind>\ndata:<json>\n\n
                 → on disconnect: UNSUBSCRIBE, close.

     UI        Dashboard /zombies/{id}/live
                 → same GET /events/stream SSE consumer.
                 → on page load also fetches GET /events?limit=20 for
                   recent history context.

     SSE auth (dual-accept, strict no-fallthrough). The endpoint accepts
     EITHER a session cookie (browser EventSource path; cookie sent
     automatically) OR Authorization: Bearer <api_key> (CLI path; Node
     fetch can set custom headers). Resolution order:
       if request has Cookie header → validate cookie → 401 on failure
                                       (do NOT also try Authorization).
       elif request has Authorization → validate Bearer → 401 on failure.
       else → 401.
     A stale or leaked cookie does not silently fall through to a valid
     Bearer; the request is 401'd. No query-param tokens (avoids leaking
     long-lived API keys via URL / referrer / access logs).

     Reconnect / sequence id. The id:<seq> line on each SSE frame is a
     per-connection in-memory monotonic counter that resets to 0 on each
     new SUBSCRIBE. The server IGNORES the Last-Event-ID request header —
     sequence ids are not durable and have no cross-connection meaning.
     Clients backfill via GET /events?cursor=<last_seen_event_id>&limit=20
     after reconnect; the new SSE then resumes from sequence 0.

     HISTORY   zombiectl events {id} [--actor=…] [--since=2h]
               Dashboard /zombies/{id}/events
                 → reads core.zombie_events (cursor-paginated).

     STATUS    zombiectl status {id}
                 → reads core.zombie_sessions
                   ("busy or idle, last response").

     If a live frame drops (slow consumer, network blip), the operator pulls
     the gap from GET /events. Live tail is best-effort by design; the
     durable record is core.zombie_events.

  KILL
  ─────────
     user
      │  POST /v1/.../zombies/{id}/kill
      ▼
    zombied-api
      ├─► UPDATE core.zombies SET status='killed' (PG)
      ├─► XADD zombie:control * type=zombie_status_changed
      │                              zombie_id={id} status=killed
      └─► 202 to user

    zombied-worker:watcher
      ├─► XREADGROUP picks up the control message
      ├─► cancel_flag_map[zombie_id].store(true, .release)
      ├─► executor_client.cancelExecution(execution_id)  [if mid-tool-call]
      └─► XACK

    per-zombie thread (top of loop)
      ├─► cancel_flag.load(.acquire) → true
      ├─► WorkerState.endEvent() if mid-event
      └─► break, thread exits

     ≤200ms end-to-end from 202 to thread-exit

  Multi-tenancy boundary

  ┌───────────────────────────────────────┬───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
  │                 Layer                 │                                                                Tenant isolation mechanism                                                                 │
  ├───────────────────────────────────────┼───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ PG (core.zombies, core.zombie_events, │ Row-Level Security by workspace_id. API enforces via app.workspace_id session var; worker uses service role with explicit WHERE filtering.                │
  │  etc.)                                │                                                                                                                                                           │
  ├───────────────────────────────────────┼───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ Redis data plane (zombie:{id}:events) │ Key namespaced by zombie UUID (globally unique); no cross-tenant collision possible. No RLS in Redis — protected by zombie_id being unguessable + API     │
  │                                       │ gatekeeping.                                                                                                                                              │
  ├───────────────────────────────────────┼───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ Redis control plane (zombie:control)  │ Fleet-wide, not tenant-scoped. Workers are tenant-blind by design (one fleet serves all tenants). Message payload carries workspace_id for logging +      │
  │                                       │ downstream PG lookups; routing uses zombie_id.                                                                                                            │
  ├───────────────────────────────────────┼───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ Worker process                        │ Per-zombie thread maintains its own consumer name worker-{pid}:zombie-{id} on the data stream. Different zombies' events never cross threads.             │
  ├───────────────────────────────────────┼───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ Executor (M41 territory)              │ Per-execution session — secrets resolved at createExecution boundary, never flow as raw strings into agent context.                                       │
  └───────────────────────────────────────┴───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘

  Why one worker = all events for that zombie

  Concern: if multiple workers are members of zombie_workers, won't zombie:{id}:events round-robin events across workers and break per-zombie state continuity?

  No — consumer groups distribute messages across consumers that are actively reading the stream. Only the worker that won the control message spawns the per-zombie thread; only that thread reads
  zombie:{id}:events. Other workers never XREADGROUP that stream → no round-robin → all events flow to the right thread.

  Failure mode (out of scope for M40, flagging for v2 followup): if the worker hosting zombie X crashes, no other worker is reading zombie:{id}:events. Recovery needs a heartbeat or XAUTOCLAIM sweep.
  v2.0 launches single-replica; multi-replica HA is a known v3 concern.

  ---

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

### The install failure scenario, visually

The API server (not the worker) is the side that writes to Redis during install. The worker only reads from `zombie:control`. So a Redis blip during install hits the API → Redis hop, not worker → Redis. The API has two layers of defence, and the watcher's reconcile sweep is the third.

**Defence-in-depth, in order of how the system tries to keep `core.zombies` and Redis consistent:**

1. **Inline retry (API).** `publishInstallSignals` retries `XGROUP CREATE` + `XADD zombie:control` on a fixed backoff `[100ms, 500ms, 1500ms]` — four attempts, ~2.1s total wall budget. Most blips never escape this loop.
2. **PG rollback (API).** If retries exhaust, the handler `DELETE`s the freshly-inserted `core.zombies` row and returns 500 with `hint=rolling_back_pg_row` so the caller can retry cleanly. No orphan.
3. **Reconcile sweep (worker watcher).** If both publish AND rollback fail (rare double-failure, logged with `hint=row_orphaned_reconcile_will_heal`), the watcher's reconcile loop runs every ~30s (6 ticks × 5s), walks `core.zombies` for `status='active'` rows, and calls `spawnZombieThread` for each. Idempotent: `ensureZombieEventsGroup` treats `BUSYGROUP` as success, and `spawnZombieThread` no-ops if the zombie is already mapped.

Worker process restart is the same machinery — boot calls `listActiveZombieIds(pool)` and runs the same `spawnZombieThread` per id — but the periodic sweep means orphans don't have to wait for a restart.

**Timeline of the rare double-failure path (publish exhausts retries AND rollback also fails):**

```
TIME ──►

t=0   USER ──── zombiectl install ────►  zombiectl/src/commands/zombie.js
                                          │
t=1   zombiectl ──── POST /v1/.../zombies ────►  API server
                                                  │
                                                  ▼
t=2   API ──── INSERT INTO core.zombies (id=Z, status='active') ────►  PG
                                                                       ✓ committed
                                                  │
                                                  ▼
t=3   API ──── publishInstallSignals (XGROUP CREATE + XADD) ────►  Redis
                 4 attempts over ~2.1s (100ms, 500ms, 1500ms backoffs)
                                                                       ╳ 💥
                                                                       all retries fail
                                                  │
                                                  ▼
t=4   API ──── DELETE core.zombies WHERE id=Z ────►  PG
                                                     ╳ 💥 rare second failure
                                                  │
                                                  ▼
t=5   API ──── 500 ────►  zombiectl ────►  USER (sees error)
        log: zombie.create_publish_failed err=... hint=rolling_back_pg_row
        log: zombie.create_rollback_failed  err=... hint=row_orphaned_reconcile_will_heal

   ─────────  STATE DURING THE ORPHAN WINDOW (≤ ~30s typical)  ─────────

   PG (core.zombies)        :  ████ row Z exists, status='active'
   Redis zombie:Z:events    :  ░░░░ does NOT exist
   Redis zombie:control     :  ░░░░ never received zombie_created for Z
   Worker watcher           :  ░░░░ doesn't know Z exists yet
   Webhooks arriving for Z  :  XADD zombie:Z:events creates the stream
                                with NO consumer group → events accumulate
                                untread (Redis-side memory only, bounded
                                by retention; no executor work)

   Worker keeps running OTHER zombies normally — no impact on the rest of
   the fleet. Z is the only one stranded, and only until the next reconcile
   tick.


   ─── ≤ ~30s passes (one reconcile cadence) ───


t=N   Watcher reconcile tick (also runs at worker boot — same code path)
                                                  │
                                                  ▼
t=N+1 watcher ──── worker_zombie.listActiveZombieIds(pool) ────►  PG
                                                                  returns [Z, ...]
                                                  │
                                                  ▼
t=N+2 for each id: watcher.spawnZombieThread(id)
                                                                          │
                       ┌──────────────────────────────────────────────────┘
                       ▼
       spawnZombieThread(Z):
         │
         ├─►  control_stream.ensureZombieEventsGroup(redis, Z)
         │       └─►  XGROUP CREATE zombie:Z:events zombie_workers 0
         │            ✓ creates the missing stream + group
         │            (BUSYGROUP-as-success on the lucky path where
         │             webhook traffic had already created the stream)
         │
         ├─►  install per-zombie ZombieRuntime (cancel + exited atomics)
         │
         └─►  std.Thread.spawn(zombie worker loop, Z)
                  │
                  ▼
              zombie thread:
                  XREADGROUP zombie_workers worker-{pid}:zombie-Z
                             ... STREAMS zombie:Z:events >
                  ✓ blocked, ready
                  ↓
              if webhooks accumulated during the orphan window,
              XREADGROUP returns them with id `0-...` (group started
              at 0) and the thread processes them in arrival order.

   ─────────  STATE AFTER RECONCILE  ─────────

   Z is fully healthy. Indistinguishable from a zombie that installed
   cleanly. Any backlog webhooks get processed in order.
```

**Variant: `XADD zombie:control` fails after `XGROUP CREATE` succeeded.**

Same picture, different broken hop inside `publishInstallSignals`'s retry loop:

```
t=3a  API ──── XGROUP CREATE zombie:Z:events ✓ (Redis briefly OK)
t=3b  API ──── XADD zombie:control * type=zombie_created Z ────►  Redis
                                                                  ╳ 💥
```

The retry loop covers both calls, so transient failures here are usually absorbed at layer 1. If retries exhaust and rollback also fails, the orphan picture is identical: `zombie:Z:events` + group both exist; only `zombie:control` missed the signal. The reconcile sweep finds Z in PG, `ensureZombieEventsGroup` is a no-op (BUSYGROUP-as-success), thread spawns, healthy.

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
| BYOK provider | Operator-supplied LLM key (Anthropic, OpenAI, Together, Groq) injected into the executor as just another secret resolved at the tool bridge. The reasoning loop is provider-agnostic; the executor reads `provider:` + the operator's stored key and routes accordingly. Soft-blocks the OSS/self-host positioning claim until shipped. | BYOK provider (M48) |

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
| **zombied-api** (`zombied serve`) | HTTP routes. Writes `core.zombies`, `vault.secrets`, `zombie:control` (produces). Steer / webhook / cron / continuation handlers all `XADD zombie:{id}:events` directly — single-ingress, no transient `zombie:{id}:steer` key. Reads `core.zombie_events` for history (per-zombie + workspace-aggregate). |
| **zombied-worker** (`zombied worker`) | Hosts one watcher thread (consumes `zombie:control`) + N zombie threads (each consumes one `zombie:{id}:events`). Owns per-zombie cancel flags. Worker is the sole publisher on `zombie:{id}:activity`. Never runs LLM code. |
| **zombied-executor** (sidecar; `zombied executor`) | Unix-socket RPC server speaking rpc_version: 2 (HELLO handshake on connect; mismatch fast-fails). Reply for `StartStage` is multiplexed: zero-or-more JSON-RPC Progress notifications (`tool_call_started` / `agent_response_chunk` / `tool_call_progress` / `tool_call_completed`) followed by exactly one terminal result frame, all sharing the request id. Hosts NullClaw agent inside Landlock + cgroups + bwrap. Credential substitution lives here; `args_redacted` is rebuilt before any progress frame leaves the RPC boundary. |

### Control-stream messages + worker loops

The watcher is the only `zombie:control` consumer per worker process; it dispatches every message to one of four handlers, then runs a periodic reconcile sweep against `core.zombies` so any signal that was never published (or never delivered) heals without a worker restart.

**Message catalog** (`zombie:control` consumer group `zombie_workers`, BLOCK 5s, COUNT 16):

| Type | Payload | Producer | Watcher action |
|---|---|---|---|
| `zombie_created` | `{zombie_id, workspace_id}` | `innerCreateZombie` after PG INSERT + `XGROUP CREATE MKSTREAM` | `spawnZombieThread(zombie_id)` — idempotent: a stale entry whose wrapper already exited is reaped before the fresh spawn (see "Per-zombie runtime lifecycle" below). |
| `zombie_status_changed` | `{zombie_id, status: active\|killed\|paused}` | `innerKillZombie` (kill verb) and pause/resume mutators | For `killed`/`paused`: hold `map_lock`, `runtime.cancel.store(true, .release)` if the runtime is live (`!exited`), release lock, then `executor.cancelExecution(execution_id)` outside the lock (Redis I/O). The per-zombie thread's `watchShutdown` polls cancel every 100ms and breaks the event loop ≤200ms after the XADD lands. For `active`: log + no-op (status flips back via PATCH; spawn already happened on `zombie_created`). |
| `zombie_config_changed` | `{zombie_id, config_revision}` | `innerPatchZombie` after `core.zombies.config_json` UPDATE | Logged today; full hot-reload deferred to M41. The revision number lets the per-zombie thread snapshot config-at-claim and apply the new config to the next event. |
| `worker_drain_request` | `{reason?}` | Operator-initiated (CLI, control plane) or `SIGTERM` handler in `zombied worker` | `WorkerState.startDrain()` — flips the global drain phase. `beginEventIfActive` returns `WorkerError.ShutdownRequested` for new event claims; in-flight events run to `endEvent` then exit. `awaitDrained(30s)`; on overrun, force-cancel + dirty exit. |

**Watcher loop** (`worker_watcher.zig`):

```
while shouldKeepRunning():       # shutdown_requested OR worker_state.isAcceptingWork
  pollOnce()                     # XREADGROUP zombie:control ... > BLOCK 5s
    for each entry:
      processEntry → dispatch → xack
  if (++ticks_since_reconcile >= 6):    # ≈30s (6 × 5s BLOCK windows)
    reconcileSpawnActive()       # SELECT id WHERE status='active'
                                 #   → spawnZombieThread(id) for each
    reconcileCancelNonActive()   # SELECT id WHERE status != 'active'
                                 #   → cancelZombie(id) for each (no-op
                                 #     if not in local runtimes map)
    ticks_since_reconcile = 0
```

`reconcileTick` is the bidirectional safety net for three failure modes: (a) `zombie_created` XADD that never landed (`publishInstallSignals` retries 3× with backoff; on exhaust, the install path rolls back the PG row, but a process death between INSERT and rollback leaves an orphan that the spawn-active branch heals); (b) `zombie_status_changed` XADD that failed for a kill or pause — PG row is `killed`/`paused` but the worker's per-zombie thread is still running, healed by the cancel-non-active branch within ≈30s without a worker restart; (c) any cross-process drift between PG state and the watcher's in-memory map.

**Per-zombie runtime lifecycle** (`worker_watcher_runtime.zig`):

Each spawn allocates a `ZombieRuntime { cancel, exited }` (two atomics) and runs `worker_zombie.zombieWorkerLoop` inside a watcher-owned `zombieRuntimeWrapper`. The wrapper does not touch the watcher maps — it only flips `runtime.exited.store(true, .release)` when the loop returns. Map ownership stays with the watcher.

The next `spawnZombieThread` invocation (driven either by another `zombie_created` for the same id or by the periodic reconcile sweep) calls `sweepExitedLocked` first. The sweep walks the `runtimes` map, collects every entry whose `exited` is set, removes it from both `runtimes` and `threads`, frees the duped key, destroys the runtime, and `Thread.detach()`s the (already-exited) handle so `deinit`'s join no longer races. After the sweep, `runtimes.contains(zombie_id)` is the authoritative "live thread for this zombie" predicate.

This lazy-sweep design closes a class of stuck-zombie bugs that the alternative ("wrapper self-cleans under map_lock") would have introduced: with one lock and no nested acquisitions, deadlock is structurally impossible — every concurrent `cancelZombie` / `spawnZombieThread` / wrapper-exit just serialises on `map_lock`. The trade-off is that an exited entry stays in the map until the next spawn attempt, which is harmless because nothing reads `cancel` once `exited` is set.

**Per-zombie loop** (`worker_zombie.zig`):

```
zombieWorkerLoop:                        # entered via wrapper
  redis = connectRedis() orelse return   # early-exit cases below
  session = claimZombie(pg) orelse return
  exec = cfg.executor orelse return
  spawn watchShutdown(shutdown, cancel, drain, &running)
  defer running.store(false); watcher.join()
  runEventLoop(...)                      # XREADGROUP zombie:{id}:events ... > BLOCK 5s
                                         #   processEvent → executor → XACK
                                         # top-of-loop: cancel.load + drain + steer-poll
                                         # eventLoop returns on running=false
                                         #   (set by watchShutdown when any signal fires)
```

Three early-exit paths return BEFORE the event loop starts: Redis connect failure, PG `claimZombie` failure, missing executor. Historically these left the watcher's map entry pointing at a dead thread (no one re-spawned because `contains == true`); now the wrapper still flips `runtime.exited` on return and the next spawn reaps the entry, so a transient Redis/PG blip self-heals on the next `zombie_created` retry or reconcile tick.

### Stream + DB ownership

| Target | Producer | Consumer |
|---|---|---|
| `zombie:control` | zombied-api on `innerCreateZombie`, status change, config PATCH | zombied-worker watcher thread |
| `zombie:{id}:events` | zombied-api on `POST /steer` (direct XADD, no transient key), zombied-api on webhook (GH Actions, others), NullClaw cron-tool fires, zombied-worker on continuation re-enqueue (chunk-continuation, gate-resolved fulfillment) | zombied-worker's per-zombie thread |
| `zombie:{id}:activity` | zombied-worker (sole publisher: `event_received`, `tool_call_started`, `agent_response_chunk`, `tool_call_progress`, `tool_call_completed`, `event_complete`) | SSE handler in zombied-api on dedicated Redis connection (SUBSCRIBE blocks the conn — outside the request-handler pool). Zero-or-N subscribers per zombie. |
| `core.zombie_events` | zombied-worker zombie thread (INSERT received, UPDATE terminal). `resumes_event_id` column links continuation rows back to their immediate parent (chunk-continuation OR gate-resolved fulfillment); recursive CTE on `zombie_events_resumes_idx` walks the chain to origin. | zombied-api `GET /v1/.../zombies/{id}/events` + `GET /v1/workspaces/{ws}/events` (workspace-aggregate, RLS-protected, replaces deleted `workspaces/activity.zig`), dashboard, `zombiectl events` |
| `core.zombies` | zombied-api only | zombied-worker at claim + watcher tick |
| `core.zombie_sessions` | zombied-worker (checkpoint + execution_id) | zombied-worker at claim + kill path |
| `vault.secrets` | zombied-api on `credential add` | zombied-worker resolves just-in-time before each `createExecution` |

### The 11 steps (platform-ops worked example)

| # | Action | zombied-api | `zombie:control` | `zombie:{id}:events` | zombied-worker | zombied-executor |
|---|---|---|---|---|---|---|
| 1 | Sign in via Clerk | OAuth callback → INSERT core.users/workspaces | — | — | idle | idle |
| 2-4 | `zombiectl credential add {fly,upstash,slack,github}` with structured `{host, api_token}` fields | `PUT /v1/.../credentials/{name}` → crypto_store.store (KMS envelope) → UPSERT vault.secrets | — | — | idle | idle |
| 5 | `zombiectl install --from .usezombie/platform-ops/` | `innerCreateZombie`: INSERT core.zombies (active). **Atomically + before 201**: `XGROUP CREATE zombie:{id}:events zombie_workers 0 MKSTREAM` + XADD `zombie:control` type=zombie_created. **Invariant 1**: stream + group exist before any producer/consumer can arrive. | +1 entry | stream+group created, empty | idle | idle |
| 6 | Watcher claims | — | — | — | Watcher `XREADGROUP zombie:control` unblocks. `spawnZombieThread`: under `map_lock`, sweep stale-exited entries; idempotent `XGROUP CREATE` for `zombie:{id}:events` (BUSYGROUP-as-success); allocate `ZombieRuntime { cancel, exited }`; spawn `zombieRuntimeWrapper`; publish to `runtimes` + `threads` maps. XACK. Wrapper invokes `worker_zombie.zombieWorkerLoop` → claims (loads config + checkpoint), spawns `watchShutdown` poller, blocks on `XREADGROUP zombie:{id}:events` BLOCK 5s. | idle |
| 7 | **Trigger arrives** — three paths land on the same stream | | | | | |
|  ↳ 7a (webhook) | GitHub Actions posts `workflow_run` failure to `/v1/.../webhooks/github`. Receiver verifies HMAC, normalizes payload. | XADD `zombie:{id}:events * actor=webhook:github type=webhook workspace_id={ws} request={run_url, head_sha, conclusion, ...} created_at=<ms>` | — | +1 entry | (within ≤5s) zombie thread XREADGROUP returns it | idle |
|  ↳ 7b (cron) | — | — | (NullClaw cron runtime fires) +1 entry actor=cron:<schedule> | — | idle |
|  ↳ 7c (steer) | `innerSteer`: directly `XADD zombie:{id}:events * actor=steer:<user> type=chat workspace_id={ws} request=<msg> created_at=<ms>`. Returns `202 { event_id }` so CLI can correlate. No transient `zombie:{id}:steer` key, no top-of-loop GETDEL — single ingress. | — | +1 entry | (within ≤5s) zombie thread XREADGROUP returns it | idle |
| 8a | processEvent starts | — | — | event consumed, in pending list | `processEvent`: INSERT `core.zombie_events` (status='received', actor=<from event>, request_json=msg). Balance gate + approval gate pass. Resolves credentials from vault just-in-time. `executor.createExecution(workspace_path, {network_policy, tools, secrets_map, context})` over Unix socket. `setExecutionActive`. `executor.startStage(execution_id, {agent_config, message, context})`. | `handleCreateExecution` creates session storing policy + context knobs. `handleStartStage` invokes `runner.execute` → NullClaw `Agent.runSingle`. **Wakes.** |
| 8b | Agent runs inside executor | — | — | — | waiting on Unix socket | NullClaw makes tool calls per the SKILL.md prose. The order the agent decides; example for GH Actions failure: `http_request GET https://api.github.com/repos/{repo}/actions/runs/{run_id}/logs` (tool-bridge substitutes `${secrets.github.api_token}` after sandbox entry, agent never sees raw bytes); `http_request GET ${fly.host}/v1/apps/{app}/logs`; `http_request GET ${upstash.host}/v2/redis/stats/{db}`; `http_request POST ${slack.host}/api/chat.postMessage` with diagnosis. **L1 (memory_checkpoint_every) fires every N tool calls; L2 (tool_window) bounds context growth; L3 (stage_chunk_threshold) escalates to continuation if threshold breached.** **Optional**: `cron_add "*/30 * * * *" "post-recovery health check"` if SKILL.md prose requests it. |
| 8c | Agent returns StageResult | — | — | — | Receives `{content, tokens, wall_s, exit_ok}` on Unix socket. updateSessionContext (in-memory). Defers destroyExecution + clearExecutionActive. | `runner.execute` returns; session destroyed on handler side; executor **sleeps** (no other work). |
| 8d | zombie thread finalizes | — | — | XACK | UPDATE core.zombie_events (status='processed', response_text, tokens, wall_ms, completed_at). checkpointState → UPSERT core.zombie_sessions. metering.recordZombieDelivery. XACK. CLI's `zombiectl steer` session (polling GET `/events` or tailing activity stream) picks up the new row and prints `[claw] <response_text>`. Back to XREADGROUP BLOCK. | idle |
| 9 | Read history | `GET /v1/.../zombies/{id}/events` reads core.zombie_events. CLI: `zombiectl events {id}`. | — | — | idle | idle |
| 10 | `zombiectl kill {id}` | UPDATE core.zombies SET status='killed'. XADD `zombie:control` type=zombie_status_changed status=killed. | +1 entry | — | Watcher reads control msg, dispatches to `cancelZombie(id)`: under `map_lock` checks `!runtime.exited` then `runtime.cancel.store(true, .release)`, releases lock, calls `executor_client.cancelExecution(execution_id)` outside the lock (Redis I/O). Zombie thread's `watchShutdown` polls `cancel.load(.acquire)` every 100ms → flips `running=false` → event loop exits → wrapper sets `runtime.exited=true` → next `spawnZombieThread` (or `reconcileTick`) reaps the entry via `sweepExitedLocked`. | If mid-stage: `handleCancelExecution` flips session.cancelled=true; in-flight `runner.execute` breaks out with `.cancelled`. |
| 11 | Audit grep — verify no secret leak | manual `grep <token-bytes> logs/* db_dump.sql` | — | — | token bytes held only transiently during `createExecution` RPC | token bytes held only in session memory + emitted inline into HTTPS TCP to upstream — never logged, never written to disk |

### Notable invariants this sequence proves

- **No race on stream/group creation.** `innerCreateZombie` does INSERT + XGROUP CREATE + XADD `zombie:control` synchronously before returning 201. Any webhook arriving within microseconds of the 201 finds the stream already there.
- **All triggers funnel into one reasoning loop.** Webhook, cron, and steer are different *producers* into `zombie:{id}:events`; the worker's per-zombie thread doesn't branch on actor type.
- **Credentials never enter agent context.** Substitution happens at the tool bridge, inside the executor, after sandbox entry. Agent sees `${secrets.fly.api_token}`; HTTPS request headers get real bytes; responses never echo the token.
- **Kill is immediate for in-flight runs.** Control-stream XADD triggers `cancelExecution` RPC within milliseconds — not the 5s XREADGROUP cycle.
- **One lock, no deadlock surface.** The watcher takes exactly one mutex (`map_lock`), with no nested acquisitions. `cancelZombie` holds it across the cancel.store to close the UAF window the lazy-sweep wrapper opens; the wrapper itself never touches the lock. Concurrent `cancelZombie` + `spawnZombieThread` + wrapper-exit serialise on the single lock — there is no second-lock ordering to get wrong.
- **Stuck zombies self-heal.** A per-zombie thread that exits early (Redis connect, PG claim, no executor) flips `runtime.exited` via the watcher-owned wrapper; the next `spawnZombieThread` (driven by `zombie_created` retry or by `reconcileTick`'s ≈30s sweep) reaps the entry and re-spawns. Pre-M40 hardening this was a permanent stuck state for the worker process's life.
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
