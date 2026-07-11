# High-Level Thesis

> Parent: [`README.md`](./README.md)

The product, the problem, the wedge, the differentiation, and why the obvious alternatives don't make this product redundant. Read this first if you've never seen the project — every other file in `docs/architecture/` and every spec under `docs/v2/` assumes you've internalized this one.

---

## 1. Product Thesis

### What we are

agentsfleet v2 is a durable runtime for one operational outcome.

On the product surface, that runtime ships as a fleet of prebuilt AI teammates — each one wakes on an event (a pull request, a deploy, an incident), does the recurring engineering work, and holds at human approval before anything merges or ships. The marketing framing and this technical thesis describe the same thing from two ends: "AI teammates, ready to run" is what the operator buys; "a durable runtime for one operational outcome" is what we build.

It is meant for work that:

- continues after the original human prompt is gone
- needs durable state across retries and failures
- must gather evidence from real systems
- may need approvals before acting
- benefits from natural-language reasoning instead of rigid typed branching

### What we are not

agentsfleet v2 is not:

- a general-purpose personal assistant in the cloud
- a generic coding-fleet orchestration product
- a broad "AI can automate anything" platform
- just a chat UI over tools

If a user can get the same value by opening Claude locally and asking "what should I do next?", then v2 has not earned its existence.

### How we differentiate

Three structural pillars carry v2:

- **Open source.** The runtime is open source. The operator can read the code that holds their credentials and runs against their infrastructure.
- **Self-managed provider keys.** Operators bring their own large-language-model provider key. The control plane resolves it and the runner's NullClaw child uses it for the inference call only. No vendor lock-in on inference cost. Supported providers are listed in [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §9 (single source of truth).
- **Markdown-defined.** Operational behaviour lives in `SKILL.md`, optional `TRIGGER.md`, and optional support files carried by a Fleet Bundle, not in a typed workflow engine. Iteration is editing prose and policy, not redeploying code.

**Self-host is deferred to v3.** v2 ships hosted-only on `api.agentsfleet.net` via Clerk OAuth. The architecture admits self-host (the auth substrate, the key-management-service adapter, and process orchestration are the only deployment-specific layers), but validating it on a clean non-Fly Linux host is a v3 workstream.

The `agentsfleet` CLI plus the first-party Fleet library ([`user_flow.md`](./user_flow.md) §8.0) is what makes the v2 pillars reachable from a cold start.

### The first problem we solve

The first problem v2 solves is deploy and production failure handling. When a deploy fails, or production looks unhealthy, the operator should not have to manually bounce between CI, logs, infra dashboards, chat, and shell history while also remembering what they already tried. The fleet should take ownership of that outcome: gather evidence, explain what is wrong, preserve the timeline, request approval when necessary, and continue until resolved or blocked.

The flagship workflow: `platform-ops`. The wedge surface is **GitHub Actions deploy-failure responder + manual operator steer** — one fleet that wakes on a failed deploy webhook (continuous-deployment failure), gathers evidence, posts a diagnosis to Slack, and is also reachable via `agentsfleet steer` for manual investigation.

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

The v2 MVP solves that gap. A Fleet receives an operational trigger, reasons over it in natural language, gathers evidence with the right tools, persists the full attempt history, resumes if interrupted, asks for approval when needed, and continues until the outcome is resolved or clearly blocked.

The MVP wedge is:

- an operational-outcome runner
- deploy and infrastructure recovery (GitHub Actions deploy-failure responder + manual steer)
- persistent history that records who or what triggered each event
- resumable execution with a bounded reasoning context
- approvals where needed
- one flagship workflow

The flagship workflow is `platform-ops`: when a GitHub Actions CD pipeline fails, the fleet wakes from the webhook, gathers the right evidence from Fly.io, Upstash, Redis, and adjacent sources, explains what is wrong, and posts a remediation suggestion to Slack. The same fleet can also be steered manually for "morning health check" investigations and follow-up reasoning after the webhook diagnosis.

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
- If the fleet mostly summarizes logs but does not move the outcome forward, the product will feel ornamental.

That is why the MVP bar is strict: the fleet must make a real operational outcome faster and safer to complete, not merely easier to read about.

---

## 4. MVP Thesis

agentsfleet v2 should be judged on one promise:

> Operational outcomes do not fall into limbo.

For the MVP, that means:

- a trigger arrives from webhook (GH Actions failure), cron, or operator steer
- the fleet gathers the right evidence
- the fleet reasons over the situation using LLM + constrained tools
- every step is persisted with actor provenance
- the fleet context is bounded (rolling window + memory checkpoints + run chunking)
- risky steps are approval-gated
- the fleet can resume after interruption
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

- **Webhook.** A workspace connection is accepted only after the GitHub user-authorization callback proves access to the claimed installation; cross-workspace reassignment is refused. GitHub Actions then posts a completed failed `workflow_run` to the shared GitHub App ingress (`POST /v1/ingress/github`). The receiver verifies the platform App signature, maps the installation to a workspace, and wakes only fleets explicitly subscribed to that repository and event with an approved GitHub grant. The manual fleet-addressed route remains available for custom hooks. Both paths write `actor=webhook:github`.
- **Cron.** A periodic production health check, scheduled by NullClaw's `cron_add` tool; each fire arrives as a synthetic event with `actor=cron:<schedule>`.
- **Steer.** A direct operator instruction via `agentsfleet steer <fleet_id> <message>` or the dashboard chat widget; lands with `actor=steer:<user>`.

All three flow through the same reasoning loop. The fleet does not branch on actor type — its SKILL.md describes the general outcome and the same `http_request` tool calls fire regardless of trigger source.

### 5.2 Slack-resident channel bot — the on-ramp (M106)

The first surface that meets non-terminal users — support, ops — where they already work. A first-party multi-tenant `@agentsfleet` Slack app: one OAuth (Open Authorization) install per workspace, and in any channel it's invited to, an `@mention` is answered in-thread, read-only, learning that channel over time (the channel's memory namespace is a per-channel resident fleet, keyed by its `fleet_id`). It is **reactive** — it answers, never acts unattended — which is the deliberate boundary to the durable hired teammate (§5.1's platform-ops is the active form). This is acquisition, not a second product: the reactive bot's job is to convert to a durable teammate, so it does **not** make v2 "just a chat UI over tools" — agency, not memory, is the line. Specced in `docs/v2/done/M106_001_P1_API_DOCS_INFRA_UI_SLACK_RESIDENT_CHANNEL_BOT.md`; the hired-teammate Rung 1 is the follow-on.
