# Architecture — v2 Operational Outcome Runner

Date: Apr 30, 2026
Status: Canonical reference for the v2 problem, thesis, runtime model, agent / zombie interaction, capabilities, and context lifecycle. All v2 specs in `docs/v2/` are grounded in the topic files in this directory.

---

## Why the doc is split this way

The architecture doc used to be a single ~1,500-line file. That was hard to read end-to-end and hard to land changes against (every PR touching architecture got into a fifteen-section diff). Each topic now lives in its own file in this directory; this README is the table of contents and a short on-ramp.

Read in this order if you've never seen the project:

1. [`high_level.md`](./high_level.md) — what the product is, what it isn't, why it exists, and why the obvious alternatives don't make it redundant.
2. [`user_flow.md`](./user_flow.md) — how a user gets from "I want a zombie" to "the zombie is running on my repo."
3. [`scenarios/`](./scenarios/) — three end-to-end walkthroughs following one persona (John Doe) across his journey: default cold install, switching to BYOK with Fireworks + Kimi 2.6, and the credit pool draining and tripping the gate.

After that, dip into whichever of these matches the change you're making:

| File | Topic |
|---|---|
| [`high_level.md`](./high_level.md) | Product thesis, problem statement, why-now, MVP thesis, initial use cases, why-not-OpenClaw. The "why this exists" reading for new contributors. |
| [`direction.md`](./direction.md) | The architectural constants. When a spec proposes something that conflicts with these, the spec gets amended — not the constants. |
| [`user_flow.md`](./user_flow.md) | How a user authors, installs, triggers, and supervises a zombie. Includes the install-skill walkthrough, deployment posture, and the model-cap origin story (§8.7). |
| [`data_flow.md`](./data_flow.md) | Where a webhook, a steer, or a cron fire ends up. Covers the two agents in play, the three durable stores, the Redis streams + pub/sub channel, the install / trigger / execute / watch / kill sequences, multi-tenancy boundary, install-failure recovery, and the load-bearing invariants. |
| [`capabilities.md`](./capabilities.md) | What the zombie has, what the platform enforces, and the context-lifecycle layers (memory checkpoint, rolling tool window, stage chunking) that keep long incidents reasoning past the model's context window. |
| [`billing_and_byok.md`](./billing_and_byok.md) | How users pay for what they run. The credit-pool model (Amp-style), the one-time $10 starter grant, the two debit points (receive + stage), `compute_receive_charge` / `compute_stage_charge`, the BYOK credential shape, the api_key visibility boundary, NullClaw's provider routing, the model-caps endpoint with per-model token rates, and the read-only billing dashboard + CLI surface. |
| [`scenarios/`](./scenarios/) | Three concrete end-to-end walkthroughs: [`01_default_install.md`](./scenarios/01_default_install.md), [`02_byok.md`](./scenarios/02_byok.md), [`03_balance_gate.md`](./scenarios/03_balance_gate.md). |
| [`bastion.md`](./bastion.md) | Where the wedge points after launch. Not part of v2 scope; documented so spec authors avoid foreclosing it. |
| [`ship_reflection.md`](./ship_reflection.md) | §14 post-launch reflection appendix. Pre-launch skeleton; populated with real evidence — launch date, first external install, deferral status — once v2 ships. |
| [`office_hours_v2.md`](./office_hours_v2.md) | The product-design session that produced the v2 wedge. Read for persona context, the demand-bucket honesty check, and the rejected approaches. |
| [`plan_engg_review_v2.md`](./plan_engg_review_v2.md) | The engineering-review pass that produced the substrate-tier vs packaging-tier split. Test plan, critical paths, regression surface. |
| [`../AUTH.md`](../AUTH.md) | The three principal types (CLI, UI, API key), the cookie-vs-Bearer reasoning, and the full auth-flow sequences. The canonical reference any time auth is in scope. |

---

## What we are, in one paragraph

UseZombie v2 is a durable runtime for one operational outcome. It targets work that continues after the original human prompt is gone, needs durable state across retries and failures, must gather evidence from real systems, may need approvals before acting, and benefits from natural-language reasoning instead of rigid typed branching. The flagship is `platform-ops`: a zombie that wakes on a GitHub Actions deploy failure, gathers evidence from Fly.io / Upstash / Redis / GitHub run logs, posts an evidenced diagnosis to Slack, and is also reachable via `zombiectl steer` for manual investigation. The same zombie handles all three trigger paths through the same reasoning loop. Three differentiation pillars carry the launch: open source, Bring Your Own Key, markdown-defined behaviour. Self-host is deferred to v3.

For everything else, follow the topic files above.

---

## Glossary

| Term | Meaning |
|---|---|
| **Zombie** | A long-lived, durable runtime instance defined by a `SKILL.md` plus `TRIGGER.md`. Owns one operational outcome. |
| **NullClaw** | The language-model agent loop that runs inside the executor sandbox. The "zombie's agent." |
| **User's agent** | Claude Code, Amp, Codex CLI, OpenCode — the workstation tool the human types into and that drives `zombiectl`. Distinct from the zombie's agent. |
| **Steer** | A user-initiated message sent to a zombie via `zombiectl steer {id} "<msg>"` or the dashboard chat widget. Lands as an event with `actor=steer:<user>`. |
| **Webhook trigger** | An external system POSTing to the zombie's webhook ingest URL (today `POST /v1/webhooks/{zombie_id}`). Lands as an event with `actor=webhook:<source>`. |
| **Cron trigger** | A NullClaw-managed schedule firing on time. Lands as an event with `actor=cron:<schedule>`. |
| **Stage** | One `runner.execute` call inside the executor — one language-model context window's worth of reasoning. Long incidents span multiple stages via continuation events. |
| **Tool bridge** | The substitution layer inside the executor that replaces `${secrets.NAME.FIELD}` placeholders with real bytes after sandbox entry. |
| **Bring Your Own Key (BYOK)** | The posture where the user stores their own language-model provider credential (Anthropic, OpenAI, Fireworks, Together, Groq, Moonshot, …) in the vault under a user-chosen name (e.g. `account-fireworks-byok`), then activates it for the tenant via `zombiectl tenant provider set --credential <name>`. The tenant's `core.tenant_providers` row carries `credential_ref` pointing at the active credential. See [`billing_and_byok.md`](./billing_and_byok.md). |
| **Bastion** | The post-launch framing where the same zombie owns both internal triage and customer-facing status communication. Documented in [`bastion.md`](./bastion.md). |
