# Architecture — v2 Operational Outcome Runner

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
| [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) | How users pay for what they run. The credit-pool model (Amp-style), the one-time starter grant, the two debit points (receive + stage), `compute_receive_charge` / `compute_stage_charge`, the self-managed credential shape, the api_key visibility boundary, NullClaw's provider routing, the model-caps endpoint with per-model token rates, and the read-only billing dashboard + CLI surface. **For live customer-facing rates, link to <https://usezombie.com/#pricing>** — do not paraphrase numbers in code or docs. |
| [`scenarios/`](./scenarios/) | Three concrete end-to-end walkthroughs: [`01_default_install.md`](./scenarios/01_default_install.md), [`02_self_managed.md`](./scenarios/02_self_managed.md), [`03_balance_gate.md`](./scenarios/03_balance_gate.md). |
| [`bastion.md`](./bastion.md) | Where the wedge points after launch. Not part of v2 scope; documented so spec authors avoid foreclosing it. |
| [`office_hours_v2.md`](./office_hours_v2.md) | The product-design session that produced the v2 wedge. Read for persona context, the demand-bucket honesty check, and the rejected approaches. |
| [`plan_engg_review_v2.md`](./plan_engg_review_v2.md) | Durable validation shape for the platform-ops wedge — surfaces under test, core happy path, edge cases, regression surface. Consumed by `/qa` and `/qa-only`. |
| [`../AUTH.md`](../AUTH.md) | The three principal types (CLI, UI, API key), the cookie-vs-Bearer reasoning, and the full auth-flow sequences. The canonical reference any time auth is in scope. |

---

## Glossary

| Term | Meaning |
|---|---|
| **Zombie** | A long-lived, durable runtime instance defined by a `SKILL.md` plus `TRIGGER.md`. Owns one operational outcome. |
| **NullClaw** | The language-model agent loop that runs inside the executor sandbox. The "zombie's agent." |
| **User's agent** | Claude Code, Amp, Codex CLI, OpenCode — the workstation tool the human types into and that drives `zombiectl`. Distinct from the zombie's agent. |
| **Steer** | A user-initiated message sent to a zombie via `zombiectl steer <zombie_id> <message>` or the dashboard chat widget. Lands as an event with `actor=steer:<user>`. |
| **Webhook trigger** | An external system POSTing to the zombie's webhook ingest URL (today `POST /v1/webhooks/{zombie_id}`). Lands as an event with `actor=webhook:<source>`. |
| **Cron trigger** | A NullClaw-managed schedule firing on time. Lands as an event with `actor=cron:<schedule>`. |
| **Stage** | One `runner.execute` call inside the executor — one language-model context window's worth of reasoning. Long incidents span multiple stages via continuation events. |
| **Tool bridge** | The substitution layer inside the executor that replaces `${secrets.NAME.FIELD}` placeholders with real bytes after sandbox entry. |
| **Self-managed provider keys** | The posture where the user stores their own language-model provider credential in the vault under a user-chosen name (e.g. `account-fireworks-key`), then activates it for the tenant via `zombiectl tenant provider set --credential <name>`. The tenant's `core.tenant_providers` row carries `credential_ref` pointing at the active credential. **Supported providers:** see the canonical table in [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §9. |
| **Bastion** | The post-launch framing where the same zombie owns both internal triage and customer-facing status communication. Documented in [`bastion.md`](./bastion.md). |
