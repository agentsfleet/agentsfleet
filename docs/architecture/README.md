# Architecture — v2 Operational Outcome Runner

> [!TIP]
> **Trying to USE agentsfleet?** This directory is the contributor-facing architecture set. If you want to install a Fleet on your own infra, go to **[docs.agentsfleet.net](https://docs.agentsfleet.net)** instead — that surface walks you through `agentsfleet install` end-to-end and never asks you to read a system-topology file. Stay here only if you are contributing to the runtime, the Command-Line Interface (CLI), the dashboard, or the Software Development Kit (SDK) packages.

Canonical reference for the v2 problem, thesis, runtime model, Fleet / runner interaction, capabilities, and context lifecycle. All v2 specs in `docs/v2/` are grounded in the topic files in this directory.

Product thesis: [`high_level.md`](./high_level.md).

---

## Why the doc is split this way

One topic per file, so a change lands in one small diff and a reader (human or
agent) loads only the file that answers their question. This README is the
lookup surface: find your topic in the table below, open that one file, grep
inside it. Do not read the whole directory to answer one question.

## Question → anchor index

Start here: find the question, jump to the one §-section that answers it. The larger topic files front-load a Facts table, so the answer is usually in the first screen; the short files (direction, testing, product_analytics) are one screen already.

| Question | Where |
|---|---|
| How long can a fleet run before its lease expires? | [`runner_fleet.md`](./runner_fleet.md) §Per-lease renewal |
| What happens when a runner dies mid-event? | [`runner_fleet.md`](./runner_fleet.md) §Failure recovery model |
| How is a stale runner's late report rejected? | [`runner_fleet.md`](./runner_fleet.md) §System guarantees |
| How does work get assigned, executed, and reported? | [`data_flow.md`](./data_flow.md) §C. EXECUTE |
| Can two runners hold the same fleet at once? | [`data_flow.md`](./data_flow.md) §One active lease per fleet |
| How does a new runner get enrolled? | [`runner_fleet.md`](./runner_fleet.md) §Registering a runner |
| What are the runner protocol verbs? | [API reference](https://docs.agentsfleet.net/api-reference/introduction) › Runner plane |
| What sandbox does a leased event run in? | [`runner_fleet.md`](./runner_fleet.md) §Running one event |
| How does the Rust runner run a lease, and what survives to the next one? | [`runner_execution.md`](./runner_execution.md) §Process model, §Workspace between leases |
| What network can a sandboxed fleet reach? | [`runner_fleet.md`](./runner_fleet.md) §Egress model |
| Which sandbox tier may run whose work? | [`runner_fleet.md`](./runner_fleet.md) §Sandbox tiers |
| How do steer, kill, and pause propagate? | [`runner_fleet.md`](./runner_fleet.md) §Steer, kill, pause |
| Where does a webhook / steer / cron fire end up? | [`data_flow.md`](./data_flow.md) §"B. TRIGGER" |
| What does one event write, in what order? | [`data_flow.md`](./data_flow.md) §Steer flow end-to-end |
| Which table answers "what did this fleet do"? | [`data_flow.md`](./data_flow.md) §The five durable stores |
| How does the live tail work — and can it lose frames? | [`data_flow.md`](./data_flow.md) §D. WATCH |
| What happens if the datastore blips during install? | [`data_flow.md`](./data_flow.md) §The install failure scenario, visually |
| Why was my webhook rejected, and what do I fix? | [error codes](https://docs.agentsfleet.net/api-reference/error-codes#UZ-WH-020) |
| Who owns cron scheduling? | [`data_flow.md`](./data_flow.md) §"QStash owns the clock" |
| What is memory keyed by, and what survives? | [`memory.md`](./memory.md) §1 |
| How does memory travel between runs? | [`runner_fleet.md`](./runner_fleet.md) §Memory continuity |
| What should a fleet store so it survives a new run? | [`capabilities.md`](./capabilities.md) §4 — Memory hygiene |
| How does a long incident outlive one context window? | [`capabilities.md`](./capabilities.md) §4 — The three knobs |
| What can a fleet do, and what is merely advisory? | [`capabilities.md`](./capabilities.md) §1 |
| When and how is a tenant charged? | [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §3 |
| What does one event cost, by shape? | [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §4.3 |
| What happens when credits run out? | [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §6 |
| What free usage does a new tenant get? | [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §2.3 |
| Where may the provider `api_key` exist? | [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §8.2 |
| Why does provider activation lock three tables, and what deletes that? | [`tenant_provider_v2.md`](./tenant_provider_v2.md) |
| How does a per-fleet budget cap work? | [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §5.1 |
| Where do model rates and context caps come from? | [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §10 |
| How does a cold machine reach a running fleet? | [`user_flow.md`](./user_flow.md) §8.0 + §8.2.1 |
| Where do model and cap originate, per posture? | [`user_flow.md`](./user_flow.md) §8.7 |
| What triggers can `TRIGGER.md` declare? | [`user_flow.md`](./user_flow.md) §8.3 |
| What does the Slack bot do — and never do? | [`user_flow.md`](./user_flow.md) §8.8 |
| How many datastore connections does a deployment need? | [`scaling.md`](./scaling.md) §Connection budget after the cutover |
| What is the datastore target, and what has moved so far? | [`datastore_scaling.md`](./datastore_scaling.md) |
| What drives idle request volume? | [`scaling.md`](./scaling.md) §Per-request volume |
| Which knob do I turn, and when? | [`scaling.md`](./scaling.md) §Tuneup knobs |
| Where is the next bottleneck? | [`scaling.md`](./scaling.md) §Where the next ceiling actually lives |
| Where does a signal go, and who owns it? | [`observability.md`](./observability.md) §The three signal paths |
| What does metric family X mean? | [`observability.md`](./observability.md) §Metric family census |
| Which locks exist, and what does each protect? | [`concurrency.md`](./concurrency.md) §Lock-invariant registry |
| What order does shutdown happen in? | [`concurrency.md`](./concurrency.md) §Shutdown choreography |
| How do I add a connector provider? | [`connectors.md`](./connectors.md) §Adding a provider |
| How does a GitHub event find its fleet? | [`connectors.md`](./connectors.md) §GitHub App |
| What is immutable in a bundle vs editable in a fleet? | [`fleet_bundles.md`](./fleet_bundles.md) §Two layers |
| What does one lease write, end to end, and when is its sandbox destroyed? | [`lease_flow.md`](./lease_flow.md) — one lease walked end to end against the Rust source, with its open gaps |
| How does a platform fleet become installable? | [`fleet_bundles.md`](./fleet_bundles.md) §The publish gate |
| Which test root owns my component? | [`testing.md`](./testing.md) §Component ownership |
| When should Rust code use a shared owner or a crate? | [`rust-ownership.md`](./rust-ownership.md) §Review rubric |
| What rules govern a client analytics event? | [`product_analytics.md`](./product_analytics.md) §Client event rules |
| Who may call what, with which token? | [`../AUTH.md`](../AUTH.md) |
| Where is the user-facing version of this page? | the `User-facing:` pointer in each file's header block |

Read in this order if you've never seen the project:

1. [`high_level.md`](./high_level.md) — what the product is, what it isn't, and why it exists.
2. [`user_flow.md`](./user_flow.md) — how a user gets from "I want a Fleet" to "the Fleet is running on my repo."
3. [`scenarios/github-pr-reviewer.md`](./scenarios/github-pr-reviewer.md) — install `github-pr-reviewer` and watch it review a Pull Request.
4. [`scenarios/production-deploy-repair.md`](./scenarios/production-deploy-repair.md) — trace a failed deployment from evidence to a human-reviewed fix.

> [!IMPORTANT]
> `user_flow.md` and `scenarios/` are **contributor-canonical** — cited by `§`-anchor in active and shipped spec acceptance criteria and in sibling arch docs. They are *not* user-facing docs to relocate to docs.agentsfleet.net (which carries its own independent user coverage). Before "moving user-facing docs," `git grep` the spec corpus for the file/anchor references first.

After that, dip into whichever of these matches the change you're making:

| File | Topic |
|---|---|
| 🧭 [`high_level.md`](./high_level.md) | Product thesis, problem, and why it exists. |
| 📐 [`direction.md`](./direction.md) | Design constants every spec must fit. |
| 🧑‍💻 [`user_flow.md`](./user_flow.md) | How a user installs, triggers and supervises Fleets. |
| 🔄 [`data_flow.md`](./data_flow.md) | Where a webhook, steer or cron fire lands. |
| 📦 [`fleet_bundles.md`](./fleet_bundles.md) | Immutable bundle versus editable fleet; onboarding. |
| 🏃 [`runner_fleet.md`](./runner_fleet.md) | Control plane and runner: leasing, fencing, recovery. |
| ⚙️ [`runner_execution.md`](./runner_execution.md) | How the runner executes one lease. |
| 🎟️ [`lease_flow.md`](./lease_flow.md) | One lease, from install to sandbox teardown. |
| 🧰 [`capabilities.md`](./capabilities.md) | What a fleet can do, and context lifecycle. |
| 🧠 [`memory.md`](./memory.md) | Memory scope, isolation and durability. |
| 📈 [`observability.md`](./observability.md) | Where each signal goes, and who owns it. |
| 📊 [`product_analytics.md`](./product_analytics.md) | PostHog product events and their rules. |
| 🧵 [`concurrency.md`](./concurrency.md) | Tasks, locks, channels and shutdown order. |
| 📏 [`scaling.md`](./scaling.md) | Sizing the runtime and its tuning knobs. |
| 🗄️ [`datastore_scaling.md`](./datastore_scaling.md) | The Dragonfly cluster target and its status. |
| 🖥️ [`web_app.md`](./web_app.md) | The dashboard's server-client rules. |
| 🧪 [`testing.md`](./testing.md) | Test ownership, lanes and the coverage bar. |
| 🦀 [`rust-ownership.md`](./rust-ownership.md) | When Rust code shares an owner or crate. |
| 🔌 [`connectors.md`](./connectors.md) | Provider connections, App ingress and fleet subscriptions. |
| 💳 [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) | Credits, debit points, provider keys, model library. |
| 🔑 [`tenant_provider_v2.md`](./tenant_provider_v2.md) | Provider activation: today's cost and the plan. |
| 🐙 [`scenarios/github-pr-reviewer.md`](./scenarios/github-pr-reviewer.md) | Install the reviewer; follow one Pull Request (PR). |
| 🚑 [`scenarios/production-deploy-repair.md`](./scenarios/production-deploy-repair.md) | Diagnose a failed deploy toward a draft PR. |
| 💬 [`scenarios/slack-channel-resident.md`](./scenarios/slack-channel-resident.md) | Rung-0 Slack bot: a per-channel resident fleet. |
| 🚨 [`scenarios/slack-incident-responder.md`](./scenarios/slack-incident-responder.md) | An installed fleet answering one Slack channel. |
| 🔐 [`../AUTH.md`](../AUTH.md) | Principals, tokens and bearer routing. |

---

## Decision records (Claude artifacts)

Long-form decision write-ups live as published artifacts, not in this repo —
link out instead of pasting them in. Each line names the decision it carries.

| Artifact | Decision |
|---|---|
| [Index audit — slots 033 & 034](https://claude.ai/code/artifact/16b3fe3e-6a0f-47cf-a80f-03f34681ec85) | Which Postgres indexes earn their slots. |
| [Error registry — inventory & curation](https://claude.ai/code/artifact/f5dd342f-633e-4a32-a7ee-579cd2db2427) | The `UZ-*` error-code inventory review. |
| [M120_002 — Admin Model Library, as built](https://claude.ai/code/artifact/3add99c7-6ce3-4617-8656-bf371b658490) | The admin catalogue's final shape. |
| [Fleet library: why your gallery was empty](https://claude.ai/code/artifact/a6b8c064-8643-444b-a43b-2fb2e7e82434) | Root cause of the empty-gallery incident. |
| [Model configuration journeys](https://claude.ai/code/artifact/e0621bf7-7b01-4492-8862-38a43d6f46b3) | How users reach a working model configuration. |

---

## Glossary

One-line definitions for quick lookup. The canonical, full definition lives in the file linked at the end of each row — drift between this table and the canonical source is a bug.

| Term | Meaning |
|---|---|
| **Fleet** | A durable AI agent defined by `SKILL.md`. [(more)](./high_level.md#1-product-thesis) |
| **Fleet Bundle** | A validated template or import holding `SKILL.md`. [(more)](./user_flow.md#81-authoring-the-fleet) |
| **Agent loop** | The runner supervisor's model loop, outside the sandbox. [(more)](./runner_execution.md#process-model) |
| **`agentsfleetd` (control plane)** | Owns datastores, the API and work assignment. [(more)](./runner_fleet.md) |
| **agentsfleet-runner** | Host binary that leases work and runs sandboxes. [(more)](./runner_execution.md) |
| **Coding fleet** | The workstation tool a human types into. [(more)](./user_flow.md#80-the-wedge-surface) |
| **Steer** | A human message; lands as `actor=steer:<user>`. [(more)](./user_flow.md#83-triggering-the-fleet) |
| **App webhook trigger** | A provider App delivery; GitHub's lands as `actor=github-app`. [(more)](./connectors.md#github-app-platform-setup-to-fleet-execution) |
| **Manual webhook trigger** | A post to `/v1/webhooks/{fleet_id}` with the workspace secret. [(more)](./user_flow.md#83-triggering-the-fleet) |
| **Trigger panel** | The `/fleets/{id}` card showing trigger setup. [(more)](./user_flow.md#84-working-from-claude-or-the-dashboard) |
| **Free usage** | The starter grant: a balance that drains. [(more)](./billing_and_provider_keys.md#23-free-usage-is-a-balance-never-a-window) |
| **Cron trigger** | An Upstash QStash schedule; QStash owns the clock. [(more)](./user_flow.md#83-triggering-the-fleet) |
| **Run** | One agent-loop pass over one lease. [(more)](./capabilities.md#4-context-lifecycle-keeping-a-long-incident-reasoning-past-the-models-working-memory-limit) |
| **Egress guard** | Supervisor guard that admits requests and injects secrets. [(more)](./runner_execution.md#credentials) |
| **Self-managed provider keys** | The tenant stores and activates its own provider key. [(more)](./billing_and_provider_keys.md#1-the-two-postures) |
| **Bastion** | Post-launch: one fleet for triage and customer comms. [(more)](./high_level.md#61-bastion--one-surface-for-internal-triage-and-customer-comms) |
