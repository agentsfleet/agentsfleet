# Architecture — v2 Operational Outcome Runner

> **Trying to USE agentsfleet?** This directory is the contributor-facing architecture set. If you want to install a Fleet on your own infra, go to **[docs.agentsfleet.net](https://docs.agentsfleet.net)** instead — that surface walks you through `agentsfleet install` end-to-end and never asks you to read a system-topology file. Stay here only if you are contributing to the runtime, the Command-Line Interface (CLI), the dashboard, or the Software Development Kit (SDK) packages.

Date: Jul 11, 2026
Status: Canonical reference for the v2 problem, thesis, runtime model, Fleet / runner interaction, capabilities, and context lifecycle. All v2 specs in `docs/v2/` are grounded in the topic files in this directory.

---

## Why the doc is split this way

One topic per file, so a change lands in one small diff and a reader (human or
agent) loads only the file that answers their question. This README is the
lookup surface: find your topic in the table below, open that one file, grep
inside it. Do not read the whole directory to answer one question.

Read in this order if you've never seen the project:

1. [`high_level.md`](./high_level.md) — what the product is, what it isn't, and why it exists.
2. [`user_flow.md`](./user_flow.md) — how a user gets from "I want a Fleet" to "the Fleet is running on my repo."
3. [`scenarios/github-pr-reviewer.md`](./scenarios/github-pr-reviewer.md) — install `github-pr-reviewer` and watch it review a Pull Request.
4. [`scenarios/production-deploy-repair.md`](./scenarios/production-deploy-repair.md) — trace a failed deployment from evidence to a human-reviewed fix.

> `user_flow.md` and `scenarios/` are **contributor-canonical** — cited by `§`-anchor in active and shipped spec acceptance criteria and in sibling arch docs. They are *not* user-facing docs to relocate to docs.agentsfleet.net (which carries its own independent user coverage). Before "moving user-facing docs," `git grep` the spec corpus for the file/anchor references first.

After that, dip into whichever of these matches the change you're making:

| File | Topic |
|---|---|
| [`high_level.md`](./high_level.md) | Product thesis, problem statement, why-now, MVP thesis, initial use cases. The "why this exists" reading for new contributors. |
| [`direction.md`](./direction.md) | The architectural constants. When a spec proposes something that conflicts with these, the spec gets amended — not the constants. |
| [`user_flow.md`](./user_flow.md) | How a user authors, imports, installs, triggers, and supervises a Fleet. Includes Fleet Bundle entrypoints, the CLI + template-catalogue install walkthrough, deployment posture, and the model-cap origin story (§8.7). |
| [`data_flow.md`](./data_flow.md) | Where a webhook, a steer, or a cron fire ends up. Covers the two fleets in play, the three durable stores, the Redis streams + pub/sub channel, the install / trigger / execute / watch / kill sequences, multi-tenancy boundary, install-failure recovery, and the load-bearing invariants. |
| [`fleet_bundles.md`](./fleet_bundles.md) | The bundle/fleet split: how a GitHub source is fetched, re-packed into agentsfleet's own canonical tar, and stored across R2 + Postgres; what is immutable vs `PATCH`-editable; the runtime read path; and the current support-file storage redundancy. |
| [`runner_fleet.md`](./runner_fleet.md) | **The runtime split (implemented at the M80_002 cutover).** `agentsfleetd` control plane + host-resident `agentsfleet-runner` execution plane: System Guarantees + Failure Recovery Model first, then the `/v1/runners` control protocol, event-leasing + sticky routing + fencing/reclaim, secret-delivery trust modes, sandbox tiers, the scaling inversion, and the M80 roadmap. Sibling of `data_flow.md` (the same runtime, traced per event). |
| [`capabilities.md`](./capabilities.md) | What the fleet has, what the platform enforces, and the context-lifecycle layers (memory checkpoint, rolling tool window, run chunking) that keep long incidents reasoning past the model's context window. |
| [`memory.md`](./memory.md) | Fleet memory — the canonical scope/isolation/durability facts: keyed by `fleet_id` (never workspace), `memory_runtime` role isolation, survives workspace destruction, and why ephemeral-fleet-per-event loses continuity. Hydrate/capture transport lives in `runner_fleet.md` §Memory continuity; in-run tools + categories in `capabilities.md` §4. |
| [`observability.md`](./observability.md) | Where a signal goes and who owns it: `agentsfleetd` is the observability plane (Prometheus pull `/metrics`, live OTLP logs+traces direct to Grafana Cloud with no collector, PostHog, Postgres execution telemetry); the runner is deliberately bare (logfmt + liveness/result reports only). The M61 `OTEL_EXPORT_REMOVAL` naming trap and the shared `src/lib/logging/` module. |
| [`concurrency.md`](./concurrency.md) | The thread/lock/channel/shutdown model of both planes: every spawned thread and its stop path (thread map), the SPSC channel inventory with payload ownership, the lock-invariant registry, and the stop→join→deinit shutdown choreography. Grounds the `C1–C5` concurrency rules; the doc `name_architecture` consults before naming a thread, channel, or lock. |
| [`web_app.md`](./web_app.md) | The dashboard's five statements (server fetches / client-leaf boundary / shell-first / optimistic mutations / no useEffect loading), the server-client bar, and the grep-measured migration scoreboard. Consulted when a milestone touches `ui/packages/app`. |
| [`testing.md`](./testing.md) | Zig component test ownership, unit and integration roots, merged line coverage, and memory verification. |
| [`connectors.md`](./connectors.md) | The registry-driven connector platform: connect/callback/status, provider ownership proof, App-level inbound routing, platform App secrets, workspace installation handles, repository-bound fleet subscriptions, and the provider impact across GitHub, Slack, Jira, and Linear. This is the full platform-admin → workspace → fleet → event → short-lived-token walkthrough. |
| [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) | How users pay for what they run. The credit-pool model (Amp-style), the one-time starter grant, the two debit points (receive + run), `compute_receive_charge` / `compute_stage_charge`, the free-trial window through 2026-08-01 00:00 UTC, the self-managed secret shape, the api_key visibility boundary, NullClaw's provider routing, the model library (authenticated GET /v1/models) with per-model token rates, and the read-only billing dashboard + CLI surface. **Current dollar amounts live on [agentsfleet.net/#pricing](https://agentsfleet.net/#pricing)** — this doc covers shape and behaviour. |
| [`scenarios/github-pr-reviewer.md`](./scenarios/github-pr-reviewer.md) | Install `github-pr-reviewer`, connect GitHub, and receive review comments. |
| [`scenarios/production-deploy-repair.md`](./scenarios/production-deploy-repair.md) | Diagnose a failed deployment and show the unproven steps needed for a draft Pull Request (PR). |
| [`roadmap.md`](./roadmap.md) | Deferred / forward-looking direction: v2.1 scope-based auth, the fleet-keys first-class revamp, the bastion post-MVP shape, open-fleet (mode C). Direction, not commitment. |
| [`../AUTH.md`](../AUTH.md) | The principal model (CLI, UI, tenant api key, fleet key, and the `agt_r` runner machine principal), the bearer-routing middleware, and the per-flow detail. The canonical reference any time auth is in scope. |

---

## Decision records (Claude artifacts)

Long-form decision write-ups live as published artifacts, not in this repo —
link out instead of pasting them in. Each line names the decision it carries.

| Artifact | Decision |
|---|---|
| [Outbound bounding — before & after M139](https://claude.ai/code/artifact/de681e67-024d-4c08-bc04-4fa96aa58d48) | One process-wide deadline scheduler (generation-armed, monotonic clock) replaced per-caller watchdog threads on raw file descriptors. Postgres stays outside it; the pool's own timeouts bound it. |
| [Greptile P1 — OTLP unbounded export](https://claude.ai/code/artifact/aee9e003-6c91-40a1-9d7e-0feacdb1d810) | The in-flight OTLP export stays unbounded for now: `fetch` is not cancel-safe, and the cancel-safe socket-shutdown bound needs a boot reorder. Deferred with the fix scoped, not forgotten. |
| [PR #549 review — deadline scheduler](https://claude.ai/code/artifact/9bb6fc29-9ff4-4838-bc9d-bdf1bdcd5290) | The pre-merge review of the scheduler change above. |
| [Index audit — slots 033 & 034](https://claude.ai/code/artifact/16b3fe3e-6a0f-47cf-a80f-03f34681ec85) | Which Postgres indexes earn their slots. |
| [Error registry — inventory & curation](https://claude.ai/code/artifact/f5dd342f-633e-4a32-a7ee-579cd2db2427) | The `UZ-*` error-code inventory review. |
| [M120_002 — Admin Model Library, as built](https://claude.ai/code/artifact/3add99c7-6ce3-4617-8656-bf371b658490) | The admin catalogue's final shape. |
| [Fleet library: why your gallery was empty](https://claude.ai/code/artifact/a6b8c064-8643-444b-a43b-2fb2e7e82434) | Root cause of the empty-gallery incident. |
| [Model configuration journeys](https://claude.ai/code/artifact/e0621bf7-7b01-4492-8862-38a43d6f46b3) | How users reach a working model configuration. |

## What we are, in one paragraph

agentsfleet v2 is a durable runtime for one operational outcome — work that continues after the human prompt is gone, needs durable state across retries, and benefits from natural-language reasoning instead of rigid typed branching. The flagship `platform-ops` fleet wakes on a GitHub Actions deploy failure, gathers evidence, and posts a diagnosis to Slack; the same fleet is also reachable via `agentsfleet steer`. Three differentiation pillars: open source, self-managed provider key, markdown-defined behaviour. Self-host is deferred to v3.

For the long form — problem statement, why-now, why-not-the-alternatives, and the pass/fail test — read [`high_level.md`](./high_level.md). This paragraph is the on-ramp; that file is the canon.

---

## Glossary

One-line definitions for quick lookup. The canonical, full definition lives in the file linked at the end of each row — drift between this table and the canonical source is a bug.

| Term | Meaning |
|---|---|
| **Fleet** | The customer-created runtime instance: a durable AI teammate defined by `SKILL.md` plus optional `TRIGGER.md` and source metadata; owns one operational outcome. `/fleets`, `core.fleets`, and `fleet_id` are canonical. [(more)](./high_level.md#1-product-thesis) |
| **Fleet Bundle** | A validated template or imported folder/archive that contains required `SKILL.md` plus optional support files; creating from it still creates a runtime Fleet. [(more)](./user_flow.md#81-authoring-the-fleet) |
| **NullClaw** | The language-model fleet loop that runs inside the runner's sandboxed child — this is "the fleet" (host) at runtime. [(more)](./capabilities.md#1-reasoning-tool-inventory-declared-in-the-fleets-own-files) |
| **`agentsfleetd` (control plane)** | Owns Postgres, Redis, the Vault API, the HTTP API, and work assignment / fencing / reclaim. Host runners reach it only over the `/v1/runners` protocol. Implemented at the M80_002 cutover. [(more)](./runner_fleet.md) |
| **agentsfleet-runner** | The host-resident binary (the parent control loop + NullClaw execution linked in — no separate sandbox sidecar) that registers to `agentsfleetd` and pulls work; holds no datastore credentials. Implemented at the M80_002 cutover. [(more)](./runner_fleet.md) |
| **Coding fleet** | The workstation tool the human types into (Claude Code / Amp / Codex CLI / OpenCode) — drives `agentsfleet`; distinct from "the fleet" (host) it operates. [(more)](./user_flow.md#80-the-wedge-surface) |
| **Steer** | A human-initiated message via `agentsfleet steer {id} "…"` or the dashboard chat composer; lands as `actor=steer:<user>`. [(more)](./user_flow.md#83-triggering-the-fleet) |
| **App webhook trigger** | A provider App posts to `/v1/ingress/{provider}`; GitHub routes an installation to one workspace, then an explicit repository/event subscription to one or more fleets. It lands as `actor=webhook:<source>`. [(more)](./connectors.md#github-app-platform-setup-to-fleet-execution) |
| **Manual webhook trigger** | An operator-managed integration posts directly to `/v1/webhooks/{fleet_id}` (`…/{fleet_id}/github` for GitHub); the URL identifies the fleet and the workspace secret authenticates the sender. [(more)](./user_flow.md#83-triggering-the-fleet) |
| **Trigger panel** | The dashboard card on `/fleets/{id}` that shows App connection/subscription state or a manual `curl` registration path. The platform holds its own GitHub App key, never the user's Personal Access Token (PAT). [(more)](./user_flow.md#84-working-from-claude-or-the-dashboard) |
| **Free-trial pricing** | Through `FREE_TRIAL_END_MS` (2026-08-01 00:00 UTC), `compute_stage_charge` returns 0 nanos regardless of posture. [(more)](./billing_and_provider_keys.md#23-promotional-windows-free-trial-mechanism) |
| **Cron trigger** | An `agentsfleet` schedule registered with Upstash QStash; QStash owns the clock and signed fires land as `actor=cron:<schedule_id>`. [(more)](./user_flow.md#83-triggering-the-fleet) |
| **Run** | One `runner.execute` call inside the runner's sandboxed child — one language-model context window's worth of reasoning. Long incidents span multiple runs via continuation events. [(more)](./capabilities.md#4-context-lifecycle-keeping-a-long-incident-reasoning-past-the-models-working-memory-limit) |
| **Tool bridge** | The substitution layer inside the runner's sandboxed child that replaces `${secrets.NAME.FIELD}` placeholders with real bytes after sandbox entry. [(more)](./capabilities.md#3-platform-level-guarantees-the-substrate-that-wraps-every-tool-call) |
| **Self-managed provider keys** | The posture where the user stores their own large language model provider secret in the vault and activates it via `agentsfleet tenant provider create --secret <name>`. [(more)](./billing_and_provider_keys.md#1-the-two-postures) |
| **Bastion** | The post-launch framing where the same fleet owns both internal triage and customer-facing status communication. [(more)](./roadmap.md#bastion--post-mvp-shape) |
