# Architecture — v2 Operational Outcome Runner

> **Trying to USE usezombie?** This directory is the contributor-facing architecture set. If you want to install a zombie on your own infra, go to **[docs.usezombie.com](https://docs.usezombie.com)** instead — that surface walks you through `zombiectl install` end-to-end and never asks you to read a system-topology file. Stay here only if you are contributing to the runtime, the CLI, the dashboard, or the SDK packages.

Date: Apr 30, 2026
Status: Canonical reference for the v2 problem, thesis, runtime model, agent / zombie interaction, capabilities, and context lifecycle. All v2 specs in `docs/v2/` are grounded in the topic files in this directory.

---

## Why the doc is split this way

The architecture doc used to be a single ~1,500-line file. That was hard to read end-to-end and hard to land changes against (every PR touching architecture got into a fifteen-section diff). Each topic now lives in its own file in this directory; this README is the table of contents and a short on-ramp.

Read in this order if you've never seen the project:

1. [`high_level.md`](./high_level.md) — what the product is, what it isn't, why it exists, and why the obvious alternatives don't make it redundant.
2. [`user_flow.md`](./user_flow.md) — how a user gets from "I want a zombie" to "the zombie is running on my repo."
3. [`scenarios/`](./scenarios/) — three end-to-end walkthroughs following one persona (John Doe) across his journey: default cold install, switching to self-managed with Fireworks + Kimi 2.6, and the credit pool draining and tripping the gate.

After that, dip into whichever of these matches the change you're making:

| File | Topic |
|---|---|
| [`high_level.md`](./high_level.md) | Product thesis, problem statement, why-now, MVP thesis, initial use cases, why-not-OpenClaw. The "why this exists" reading for new contributors. |
| [`direction.md`](./direction.md) | The architectural constants. When a spec proposes something that conflicts with these, the spec gets amended — not the constants. |
| [`user_flow.md`](./user_flow.md) | How a user authors, installs, triggers, and supervises a zombie. Includes the install-skill walkthrough, deployment posture, and the model-cap origin story (§8.7). |
| [`data_flow.md`](./data_flow.md) | Where a webhook, a steer, or a cron fire ends up. Covers the two agents in play, the three durable stores, the Redis streams + pub/sub channel, the install / trigger / execute / watch / kill sequences, multi-tenancy boundary, install-failure recovery, and the load-bearing invariants. |
| [`capabilities.md`](./capabilities.md) | What the zombie has, what the platform enforces, and the context-lifecycle layers (memory checkpoint, rolling tool window, stage chunking) that keep long incidents reasoning past the model's context window. |
| [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) | How users pay for what they run. The credit-pool model (Amp-style), the one-time starter grant, the two debit points (receive + stage), `compute_receive_charge` / `compute_stage_charge`, the free-trial window through 2026-07-31, the self-managed credential shape, the api_key visibility boundary, NullClaw's provider routing, the model-caps endpoint with per-model token rates, and the read-only billing dashboard + CLI surface. **Current dollar amounts live on [usezombie.com/#pricing](https://usezombie.com/#pricing)** — this doc covers shape and behaviour. |
| [`scenarios/`](./scenarios/) | Three concrete end-to-end walkthroughs: [`01_default_install.md`](./scenarios/01_default_install.md), [`02_self_managed.md`](./scenarios/02_self_managed.md), [`03_balance_gate.md`](./scenarios/03_balance_gate.md). |
| [`bastion.md`](./bastion.md) | Where the wedge points after launch. Not part of v2 scope; documented so spec authors avoid foreclosing it. |
| [`ship_reflection.md`](./ship_reflection.md) | §14 post-launch reflection appendix. Pre-launch skeleton; populated with real evidence — launch date, first external install, deferral status — once v2 ships. |
| [`office_hours_v2.md`](./office_hours_v2.md) | The product-design session that produced the v2 wedge. Read for persona context, the demand-bucket honesty check, and the rejected approaches. |
| [`plan_engg_review_v2.md`](./plan_engg_review_v2.md) | The engineering-review pass that produced the substrate-tier vs packaging-tier split. Test plan, critical paths, regression surface. |
| [`../AUTH.md`](../AUTH.md) | The three principal types (CLI, UI, API key), the cookie-vs-Bearer reasoning, and the full auth-flow sequences. The canonical reference any time auth is in scope. |

---

## What we are, in one paragraph

usezombie v2 is a durable runtime for one operational outcome. It targets work that continues after the original human prompt is gone, needs durable state across retries and failures, must gather evidence from real systems, may need approvals before acting, and benefits from natural-language reasoning instead of rigid typed branching. The flagship is `platform-ops`: a zombie that wakes on a GitHub Actions deploy failure, gathers evidence from Fly.io / Upstash / Redis / GitHub run logs, posts an evidenced diagnosis to Slack, and is also reachable via `zombiectl steer` for manual investigation. The same zombie handles all three trigger paths through the same reasoning loop. Three differentiation pillars carry the launch: open source, self-managed provider key, markdown-defined behaviour. Self-host is deferred to v3.

For everything else, follow the topic files above.

---

## Glossary

| Term | Meaning |
|---|---|
| **Zombie** | A long-lived, durable runtime instance defined by a `SKILL.md` plus `TRIGGER.md`. Owns one operational outcome. |
| **NullClaw** | The language-model agent loop that runs inside the executor sandbox. The "zombie's agent." |
| **User's agent** | Claude Code, Amp, Codex CLI, OpenCode — the workstation tool the human types into and that drives `zombiectl`. Distinct from the zombie's agent. |
| **Steer** | A user-initiated message sent to a zombie via `zombiectl steer {id} "<msg>"` or the dashboard chat widget. Lands as an event with `actor=steer:<user>`. |
| **Webhook trigger** | An external system POSTing to the zombie's webhook ingest URL (`POST /v1/webhooks/{zombie_id}/{source}`). Lands as an event with `actor=webhook:<source>`. A zombie's `TRIGGER.md` declares `triggers: [...]` (array, 1–8 entries) so the same zombie can wake on multiple sources and on a cron schedule simultaneously; each webhook entry carries `events: [...]` listing the provider-specific events it subscribes to (e.g. `["workflow_run"]` for GitHub). |
| **Trigger panel** | The dashboard surface on `/zombies/{id}` that renders one card per declared trigger. For known providers (GitHub, Linear, Jira, Grafana, Slack, agentmail, Clerk) it pre-renders the exact terminal command the user runs locally to register the webhook on the provider (e.g. `gh api repos/.../hooks ...`). For unknown sources it falls back to a webhook-URL copy block. The dashboard never holds the user's provider credentials — registration runs from the user's own machine via their own `gh` / `curl` auth. |
| **Free-trial pricing** | Through `FREE_TRIAL_END_MS` (2026-08-01 00:00 UTC), `compute_stage_charge` returns 0 nanos regardless of posture. `EVENT_NANOS` is already 0 outside the trial. After the cutoff, both functions fall through to the existing `STAGE_PLATFORM_NANOS` / `STAGE_SELF_MANAGED_NANOS` rates. `zombiectl doctor --json` and the dashboard billing panel surface `free_trial: { active, ends_at_ms }` so users see the cutoff before charges begin. See [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §2.x. |
| **Cron trigger** | A NullClaw-managed schedule firing on time. Lands as an event with `actor=cron:<schedule>`. |
| **Stage** | One `runner.execute` call inside the executor — one language-model context window's worth of reasoning. Long incidents span multiple stages via continuation events. |
| **Tool bridge** | The substitution layer inside the executor that replaces `${secrets.NAME.FIELD}` placeholders with real bytes after sandbox entry. |
| **Self-managed provider keys** | The posture where the user stores their own language-model provider credential (Anthropic, OpenAI, Fireworks, Together, Groq, Moonshot, …) in the vault under a user-chosen name (e.g. `account-fireworks-key`), then activates it for the tenant via `zombiectl tenant provider set --credential <name>`. The tenant's `core.tenant_providers` row carries `credential_ref` pointing at the active credential. See [`billing_and_provider_keys.md`](./billing_and_provider_keys.md). |
| **Bastion** | The post-launch framing where the same zombie owns both internal triage and customer-facing status communication. Documented in [`bastion.md`](./bastion.md). |
