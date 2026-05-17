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
| [`office_hours.md`](./office_hours.md) | Historical — the product-design session that produced the v2 wedge. Persona context, demand-bucket honesty check, rejected approaches. Not enforceable canon. |
| [`plan_engg_review.md`](./plan_engg_review.md) | Historical — the engineering-review pass that produced the substrate-tier vs packaging-tier split. Test framing, critical paths, regression surface. Not enforceable canon. |
| [`../AUTH.md`](../AUTH.md) | The three principal types (CLI, UI, API key), the cookie-vs-Bearer reasoning, and the full auth-flow sequences. The canonical reference any time auth is in scope. |

---

## What we are, in one paragraph

usezombie v2 is a durable runtime for one operational outcome — work that continues after the human prompt is gone, needs durable state across retries, and benefits from natural-language reasoning instead of rigid typed branching. The flagship `platform-ops` zombie wakes on a GitHub Actions deploy failure, gathers evidence, and posts a diagnosis to Slack; the same zombie is also reachable via `zombiectl steer`. Three differentiation pillars: open source, self-managed provider key, markdown-defined behaviour. Self-host is deferred to v3.

For the long form — problem statement, why-now, why-not-the-alternatives, and the pass/fail test — read [`high_level.md`](./high_level.md). This paragraph is the on-ramp; that file is the canon.

---

## Glossary

One-line definitions for quick lookup. The canonical, full definition lives in the file linked at the end of each row — drift between this table and the canonical source is a bug.

| Term | Meaning |
|---|---|
| **Zombie** | A long-lived, durable runtime instance defined by `SKILL.md` + `TRIGGER.md`; owns one operational outcome. [(more)](./high_level.md#1-product-thesis) |
| **NullClaw** | The language-model agent loop that runs inside the executor sandbox — the "zombie's agent." [(more)](./capabilities.md#1-reasoning--tool-inventory-declared-in-the-zombies-own-files) |
| **User's agent** | The workstation tool the human types into (Claude Code / Amp / Codex CLI / OpenCode) — drives `zombiectl`; distinct from the zombie's agent. [(more)](./user_flow.md#§8.0-the-wedge-surface-usezombie-install-platform-ops-skill) |
| **Steer** | A human-initiated message via `zombiectl steer {id} "…"` or the dashboard chat composer; lands as `actor=steer:<user>`. [(more)](./user_flow.md#§8.3-triggering-the-zombie) |
| **Webhook trigger** | An external system POSTing to `/v1/webhooks/{zombie_id}/{source}`; lands as `actor=webhook:<source>`. [(more)](./user_flow.md#§8.3-triggering-the-zombie) |
| **Trigger panel** | The dashboard card on `/zombies/{id}` that renders the local `gh`/`curl` command to register the webhook on the provider — the platform never holds the user's provider PAT. [(more)](./user_flow.md#§8.4-working-from-claude-or-the-dashboard) |
| **Free-trial pricing** | Through `FREE_TRIAL_END_MS` (2026-08-01 00:00 UTC), `compute_stage_charge` returns 0 nanos regardless of posture. [(more)](./billing_and_provider_keys.md#23-promotional-windows-free-trial-mechanism) |
| **Cron trigger** | A NullClaw-managed schedule firing on time; lands as `actor=cron:<schedule>`. [(more)](./user_flow.md#§8.3-triggering-the-zombie) |
| **Stage** | One `runner.execute` call inside the executor — one language-model context window's worth of reasoning. Long incidents span multiple stages via continuation events. [(more)](./capabilities.md#4-context-lifecycle--keeping-a-long-incident-reasoning-past-the-models-working-memory-limit) |
| **Tool bridge** | The substitution layer inside the executor that replaces `${secrets.NAME.FIELD}` placeholders with real bytes after sandbox entry. [(more)](./capabilities.md#3-platform-level-guarantees-the-substrate-that-wraps-every-tool-call) |
| **Self-managed provider keys** | The posture where the user stores their own LLM provider credential in the vault and activates it via `zombiectl tenant provider set --credential <name>`. [(more)](./billing_and_provider_keys.md#1-the-two-postures) |
| **Bastion** | The post-launch framing where the same zombie owns both internal triage and customer-facing status communication. [(more)](./bastion.md) |
