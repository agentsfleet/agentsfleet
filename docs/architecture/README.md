# Architecture — v2 Operational Outcome Runner

> **Trying to USE agentsfleet?** This directory is the contributor-facing architecture set. If you want to install an agent on your own infra, go to **[docs.agentsfleet.net](https://docs.agentsfleet.net)** instead — that surface walks you through `agentsfleet install` end-to-end and never asks you to read a system-topology file. Stay here only if you are contributing to the runtime, the CLI, the dashboard, or the SDK packages.

Date: Apr 30, 2026
Status: Canonical reference for the v2 problem, thesis, runtime model, agent / agent interaction, capabilities, and context lifecycle. All v2 specs in `docs/v2/` are grounded in the topic files in this directory.

---

## Why the doc is split this way

The architecture doc used to be a single ~1,500-line file. That was hard to read end-to-end and hard to land changes against (every PR touching architecture got into a fifteen-section diff). Each topic now lives in its own file in this directory; this README is the table of contents and a short on-ramp.

Read in this order if you've never seen the project:

1. [`high_level.md`](./high_level.md) — what the product is, what it isn't, and why it exists.
2. [`user_flow.md`](./user_flow.md) — how a user gets from "I want an agent" to "the agent is running on my repo."
3. [`scenarios/`](./scenarios/) — three end-to-end walkthroughs following one persona (John Doe) across his journey: default cold install, switching to self-managed with Fireworks + Kimi 2.6, and the credit pool draining and tripping the gate.

> `user_flow.md` and `scenarios/` are **contributor-canonical** — cited by `§`-anchor in active and shipped spec acceptance criteria and in sibling arch docs. They are *not* user-facing docs to relocate to docs.agentsfleet.net (which carries its own independent user coverage). Before "moving user-facing docs," `git grep` the spec corpus for the file/anchor references first.

After that, dip into whichever of these matches the change you're making:

| File | Topic |
|---|---|
| [`high_level.md`](./high_level.md) | Product thesis, problem statement, why-now, MVP thesis, initial use cases. The "why this exists" reading for new contributors. |
| [`direction.md`](./direction.md) | The architectural constants. When a spec proposes something that conflicts with these, the spec gets amended — not the constants. |
| [`user_flow.md`](./user_flow.md) | How a user authors, installs, triggers, and supervises an agent. Includes the install-skill walkthrough, deployment posture, and the model-cap origin story (§8.7). |
| [`data_flow.md`](./data_flow.md) | Where a webhook, a steer, or a cron fire ends up. Covers the two agents in play, the three durable stores, the Redis streams + pub/sub channel, the install / trigger / execute / watch / kill sequences, multi-tenancy boundary, install-failure recovery, and the load-bearing invariants. |
| [`runner_fleet.md`](./runner_fleet.md) | **The runtime split (implemented at the M80_002 cutover).** `agentsfleetd` control plane + host-resident `agentsfleet-runner` execution plane: System Guarantees + Failure Recovery Model first, then the `/v1/runners` control protocol, event-leasing + sticky routing + fencing/reclaim, secret-delivery trust modes, sandbox tiers, the scaling inversion, and the M80 roadmap. Sibling of `data_flow.md` (the same runtime, traced per event). |
| [`capabilities.md`](./capabilities.md) | What the agent has, what the platform enforces, and the context-lifecycle layers (memory checkpoint, rolling tool window, run chunking) that keep long incidents reasoning past the model's context window. |
| [`observability.md`](./observability.md) | Where a signal goes and who owns it: `agentsfleetd` is the observability plane (Prometheus pull `/metrics`, live OTLP logs+traces direct to Grafana Cloud with no collector, PostHog, Postgres execution telemetry); the runner is deliberately bare (logfmt + liveness/result reports only). The M61 `OTEL_EXPORT_REMOVAL` naming trap and the shared `src/lib/logging/` module. |
| [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) | How users pay for what they run. The credit-pool model (Amp-style), the one-time starter grant, the two debit points (receive + run), `compute_receive_charge` / `compute_stage_charge`, the free-trial window through 2026-07-31, the self-managed credential shape, the api_key visibility boundary, NullClaw's provider routing, the model-caps endpoint with per-model token rates, and the read-only billing dashboard + CLI surface. **Current dollar amounts live on [agentsfleet.net/#pricing](https://agentsfleet.net/#pricing)** — this doc covers shape and behaviour. |
| [`scenarios/`](./scenarios/) | Three concrete end-to-end walkthroughs: [`01_default_install.md`](./scenarios/01_default_install.md), [`02_self_managed.md`](./scenarios/02_self_managed.md), [`03_balance_gate.md`](./scenarios/03_balance_gate.md). |
| [`roadmap.md`](./roadmap.md) | Deferred / forward-looking direction: v2.1 scope-based auth, the agent-keys first-class revamp, the bastion post-MVP shape, open-fleet (mode C). Direction, not commitment. |
| [`archive/`](./archive/) | Historical, non-canon records: `office_hours.md` (the product-design session behind the v2 wedge), `office_hours_support_wedge_jun2026.md` (the support-engineer wedge exploration), and `plan_engg_review.md` (the pre-launch engineering review). Context for past decisions; not the truth about today. |
| [`../AUTH.md`](../AUTH.md) | The principal model (CLI, UI, tenant api key, agent key, and the `zrn_` runner machine principal), the bearer-routing middleware, and the per-flow detail. The canonical reference any time auth is in scope. |

---

## What we are, in one paragraph

agentsfleet v2 is a durable runtime for one operational outcome — work that continues after the human prompt is gone, needs durable state across retries, and benefits from natural-language reasoning instead of rigid typed branching. The flagship `platform-ops` agent wakes on a GitHub Actions deploy failure, gathers evidence, and posts a diagnosis to Slack; the same agent is also reachable via `agentsfleet steer`. Three differentiation pillars: open source, self-managed provider key, markdown-defined behaviour. Self-host is deferred to v3.

For the long form — problem statement, why-now, why-not-the-alternatives, and the pass/fail test — read [`high_level.md`](./high_level.md). This paragraph is the on-ramp; that file is the canon.

---

## Glossary

One-line definitions for quick lookup. The canonical, full definition lives in the file linked at the end of each row — drift between this table and the canonical source is a bug.

| Term | Meaning |
|---|---|
| **Agent** | A long-lived, durable runtime instance defined by `SKILL.md` + `TRIGGER.md`; owns one operational outcome. [(more)](./high_level.md#1-product-thesis) |
| **NullClaw** | The language-model agent loop that runs inside the runner's sandboxed child — this is "the agent" (host) at runtime. [(more)](./capabilities.md#1-reasoning--tool-inventory-declared-in-the-agents-own-files) |
| **`agentsfleetd` (control plane)** | Owns Postgres, Redis, the Vault API, the HTTP API, and work assignment / fencing / reclaim. Host runners reach it only over the `/v1/runners` protocol. Implemented at the M80_002 cutover. [(more)](./runner_fleet.md) |
| **agentsfleet-runner** | The host-resident binary (the parent control loop + NullClaw execution linked in — no separate sandbox sidecar) that registers to `agentsfleetd` and pulls work; holds no datastore credentials. Implemented at the M80_002 cutover. [(more)](./runner_fleet.md) |
| **Coding agent** | The workstation tool the human types into (Claude Code / Amp / Codex CLI / OpenCode) — drives `agentsfleet`; distinct from "the agent" (host) it operates. [(more)](./user_flow.md#§8.0-the-wedge-surface-agentsfleet-install-platform-ops-skill) |
| **Steer** | A human-initiated message via `agentsfleet steer {id} "…"` or the dashboard chat composer; lands as `actor=steer:<user>`. [(more)](./user_flow.md#§8.3-triggering-the-agent) |
| **Webhook trigger** | An external system POSTing to `/v1/webhooks/{agent_id}/{source}`; lands as `actor=webhook:<source>`. [(more)](./user_flow.md#§8.3-triggering-the-agent) |
| **Trigger panel** | The dashboard card on `/agents/{id}` that renders the local `gh`/`curl` command to register the webhook on the provider — the platform never holds the user's provider PAT. [(more)](./user_flow.md#§8.4-working-from-claude-or-the-dashboard) |
| **Free-trial pricing** | Through `FREE_TRIAL_END_MS` (2026-08-01 00:00 UTC), `compute_stage_charge` returns 0 nanos regardless of posture. [(more)](./billing_and_provider_keys.md#23-promotional-windows-free-trial-mechanism) |
| **Cron trigger** | A NullClaw-managed schedule firing on time; lands as `actor=cron:<schedule>`. [(more)](./user_flow.md#§8.3-triggering-the-agent) |
| **Run** | One `runner.execute` call inside the runner's sandboxed child — one language-model context window's worth of reasoning. Long incidents span multiple runs via continuation events. [(more)](./capabilities.md#4-context-lifecycle--keeping-a-long-incident-reasoning-past-the-models-working-memory-limit) |
| **Tool bridge** | The substitution layer inside the runner's sandboxed child that replaces `${secrets.NAME.FIELD}` placeholders with real bytes after sandbox entry. [(more)](./capabilities.md#3-platform-level-guarantees-the-substrate-that-wraps-every-tool-call) |
| **Self-managed provider keys** | The posture where the user stores their own LLM provider credential in the vault and activates it via `agentsfleet tenant provider set --credential <name>`. [(more)](./billing_and_provider_keys.md#1-the-two-postures) |
| **Bastion** | The post-launch framing where the same agent owns both internal triage and customer-facing status communication. [(more)](./roadmap.md#bastion--post-mvp-shape) |
