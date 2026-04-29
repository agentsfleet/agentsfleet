# High-Level Thesis

> Parent: [`ARCHITECHTURE.md`](../ARCHITECHTURE.md)

The product, the problem, the wedge, the differentiation, and why the obvious alternatives don't make this product redundant. Read this first if you've never seen the project — every other file in `docs/architecture/` and every spec under `docs/v2/` assumes you've internalized this one.

---

## 1. Product Thesis

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

- **Open source.** The runtime is open source. The operator can read the code that holds their credentials and runs against their infrastructure.
- **Bring Your Own Key (BYOK).** Operators bring their own large-language-model provider key — Anthropic, OpenAI, Fireworks (which serves Kimi K2), Together AI, Groq, and others. The executor treats it as another secret resolved at the tool bridge. No vendor lock-in on inference cost. See [`billing_and_byok.md`](./billing_and_byok.md).
- **Markdown-defined.** Operational behaviour lives in `SKILL.md` + `TRIGGER.md`, not in a typed workflow engine. Iteration is editing prose, not redeploying code.

**Self-host is deferred to v3.** v2 ships hosted-only on `api.usezombie.com` via Clerk OAuth. The architecture admits self-host (the auth substrate, the key-management-service adapter, and process orchestration are the only deployment-specific layers), but validating it on a clean non-Fly Linux host is a v3 workstream.

The `/usezombie-install-platform-ops` skill ([`user_flow.md`](./user_flow.md) §8.0) is what makes the v2 pillars reachable from a cold start.

### The first problem we solve

The first problem v2 solves is deploy and production failure handling. When a deploy fails, or production looks unhealthy, the operator should not have to manually bounce between CI, logs, infra dashboards, chat, and shell history while also remembering what they already tried. The zombie should take ownership of that outcome: gather evidence, explain what is wrong, preserve the timeline, request approval when necessary, and continue until resolved or blocked.

The flagship workflow: `platform-ops`. The wedge surface is **GitHub Actions deploy-failure responder + manual operator steer** — one zombie that wakes on a failed deploy webhook (continuous-deployment failure), gathers evidence, posts a diagnosis to Slack, and is also reachable via `zombiectl steer` for manual investigation.

---

## 2. Problem Statement

Operational work falls into limbo.

When a deploy fails, production looks unhealthy, or a risky recovery action like a database teardown is needed, the operator has to manually gather logs, inspect dashboards, remember prior attempts, decide the next step, and keep the audit trail straight across terminals, chat, CI, and infrastructure consoles. The work is fragmented, state is lost between attempts, and dangerous actions are performed ad hoc.

Existing tooling captures pieces of the workflow but not the whole outcome:

- A continuous-integration system can tell you a deploy failed.
- Logging and observability can show symptoms.
- Chat can deliver alerts.
- Shell scripts and runbooks can perform actions.

What is missing is a long-lived runtime that owns the outcome end-to-end.

The v2 MVP solves that gap. A zombie receives an operational trigger, reasons over it in natural language, gathers evidence with the right tools, persists the full attempt history, resumes if interrupted, asks for approval when needed, and continues until the outcome is resolved or clearly blocked.

The MVP wedge is:

- an operational-outcome runner
- deploy and infrastructure recovery (GitHub Actions deploy-failure responder + manual steer)
- persistent history that records who or what triggered each event
- resumable execution with a bounded reasoning context
- approvals where needed
- one flagship workflow

The flagship workflow is `platform-ops`: when a GitHub Actions CD pipeline fails, the zombie wakes from the webhook, gathers the right evidence from Fly.io, Upstash, Redis, and adjacent sources, explains what is wrong, and posts a remediation suggestion to Slack. The same zombie can also be steered manually for "morning health check" investigations and follow-up reasoning after the webhook diagnosis.

---

## 3. Why Now / Why Not the Alternatives

### Why now

Three shifts make this MVP plausible now:

1. LLMs can reason over messy operational evidence well enough to choose the next step when paired with constrained tools and clear instructions.
2. Operators already maintain personal assistants and prompt files locally, but those assistants are ephemeral and session-bound. They do not own outcomes across time, triggers, retries, approvals, and failures.
3. The integration surface already exists in the real workflow: deploy webhooks, cron checks, logs APIs, secret stores, approval policies, and shell-accessible recovery tasks. The missing piece is durable orchestration around them.

Stated plainly: many people now run a personal assistant in a terminal. v2 moves the useful part of that behaviour into a cloud-resident, durable, auditable runtime that can wait for events, wake itself up, keep state, and continue working after the original human session is gone.

### Why not just use incident tooling

Incident tooling is adjacent but incomplete for this problem.

Incident products can: ingest alerts; manage timelines; route responders; collect notes; summarize incidents.

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

## 4. MVP Thesis

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

## 5. Initial Use Cases

### 5.1 Platform-Ops (wedge)

Primary job:

- investigate failed GitHub Actions deploys (webhook trigger)
- investigate unhealthy production state on a schedule (cron trigger) or on operator request (steer trigger)
- collect logs and health evidence from Fly.io, Upstash, Redis, GitHub Actions run logs, and adjacent systems
- summarize the likely cause
- recommend or execute the next action depending on approval policy

Trigger modes:

- **Webhook.** GitHub Actions posts `workflow_run.conclusion == failure` to `/v1/.../webhooks/github` with a hash-based-message-authentication signature; the receiver writes a synthetic event with `actor=webhook:github`.
- **Cron.** A periodic production health check, scheduled by NullClaw's `cron_add` tool; each fire arrives as a synthetic event with `actor=cron:<schedule>`.
- **Steer.** A direct operator instruction via `zombiectl steer {id} [<message>]` or the dashboard chat widget; lands with `actor=steer:<user>`.

All three flow through the same reasoning loop. The zombie does not branch on actor type — its SKILL.md describes the general outcome and the same `http_request` tool calls fire regardless of trigger source.

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

For many technical users, OpenClaw or a similar local assistant can already do a meaningful subset of the work: inspect deploy logs, analyze production failures, prepare a recovery plan, use tools from a supervised terminal session, ask for confirmation before risky actions.

If that is enough, then UseZombie v2 should not exist as a separate product.

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

For the answer to be "yes", the user must get real additional value from durability, triggers, resumability, approvals, history, outcome ownership, and bounded long-running reasoning.

If the main value is still interactive reasoning inside a live human-supervised session, then the better answer is to improve the assistant experience, not build a separate runtime.

### Practical implication for the MVP

The MVP should avoid trying to beat OpenClaw at being a general assistant. Instead, it should prove one thing OpenClaw does not naturally solve on its own:

> A long-lived operational outcome continues correctly after the original interactive session ends, AND a webhook can wake it without any human present.

That is why the flagship stays narrow: GitHub Actions CD-failure responder via `platform-ops`, plus manual operator steer on the same zombie.

If that does not show clear advantage over OpenClaw plus light glue code, then the product thesis should be reconsidered.
