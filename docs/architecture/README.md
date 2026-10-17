# Architecture — v2 Operational Outcome Runner

> [!TIP]
> **Trying to USE agentsfleet?** This directory is the contributor-facing architecture set. If you want to install a Fleet on your own infra, go to **[docs.agentsfleet.net](https://docs.agentsfleet.net)** instead — that surface walks you through `agentsfleet install` end-to-end and never asks you to read a system-topology file. Stay here only if you are contributing to the runtime, the Command-Line Interface (CLI), the dashboard, the website, or the design system.

Canonical reference for the v2 problem, thesis, runtime model, Fleet / runner interaction, capabilities, and context lifecycle. All v2 specs in `docs/v2/` are grounded in the topic files in this directory.

Product thesis: [`high_level.md`](./high_level.md).

---

## Why the doc is split this way

One topic per file, so a change lands in one small diff and a reader (human or
agent) loads only the file that answers their question. This README is the
lookup surface: find your topic in the table below, open that one file, grep
inside it. Do not read the whole directory to answer one question.

## Shared topic ownership

Browser and terminal authoring, saved revisions and runtime setup use the same topic owners below. Definition and revisions belong to fleet_definitions; activation and recovery to data_flow; runner protocol/execution, billing, concurrency and web_app retain their existing responsibilities. API resources owns exact wire shapes. Review artifacts live under docs/reviews and do not define another architecture.

## Question → anchor index

Start here: find the question, jump to the one §-section that answers it. The larger topic files front-load a Facts table, so the answer is usually in the first screen; the short files (direction, testing, product_analytics) are one screen already.

| Question | Where |
|---|---|
| How long can a fleet run before its lease expires? | [`runner_fleet.md`](./runner_fleet.md) §"Per-lease renewal — how a long fleet keeps its lease" |
| What happens when a runner dies mid-event? | [`runner_fleet.md`](./runner_fleet.md) §"Failure recovery model" |
| How is a stale runner's late report rejected? | [`runner_fleet.md`](./runner_fleet.md) §"System guarantees (read this first)" |
| How does work get assigned, executed, and reported? | [`data_flow.md`](./data_flow.md) §"C. EXECUTE  (lease → runner → report)" |
| Can two runners hold the same fleet at once? | [`data_flow.md`](./data_flow.md) §"One active lease per fleet — the ownership model" |
| How does a new runner get enrolled? | [`runner_fleet.md`](./runner_fleet.md) §"Registering a runner" |
| What are the runner protocol verbs? | [API reference](https://docs.agentsfleet.net/api-reference/introduction) › Runner plane |
| What sandbox does a leased event run in? | [`runner_fleet.md`](./runner_fleet.md) §"Running one event" |
| How does the Rust runner run a lease, and what survives to the next one? | [`runner_execution.md`](./runner_execution.md) §"Process model", §"Workspace between leases" |
| What network can a sandboxed fleet reach? | [`runner_fleet.md`](./runner_fleet.md) §"Egress model — outbound is the only network surface" |
| Which sandbox tier may run whose work? | [`runner_fleet.md`](./runner_fleet.md) §"Sandbox tiers" |
| How do steer, kill, and pause propagate? | [`runner_fleet.md`](./runner_fleet.md) §"Steer, kill, pause" |
| Where does a webhook / steer / cron fire end up? | [`data_flow.md`](./data_flow.md) §"B. TRIGGER  (steer / webhook / cron — three callers, ONE ingress)" |
| What does one event write, in what order? | [`data_flow.md`](./data_flow.md) §"Steer flow end-to-end" |
| Which table answers "what did this fleet do"? | [`data_flow.md`](./data_flow.md) §"The five durable stores: who owns what" |
| How does the live tail work — and can it lose frames? | [`data_flow.md`](./data_flow.md) §"D. WATCH  (user-side: how the live tail surfaces)" |
| What happens if the datastore blips during install? | [`data_flow.md`](./data_flow.md) §"The install failure scenario, visually" |
| Why was my webhook rejected, and what do I fix? | [error codes](https://docs.agentsfleet.net/api-reference/error-codes#UZ-WH-020) |
| Who owns cron scheduling? | [`data_flow.md`](./data_flow.md) §"QStash owns the clock" |
| What is memory keyed by, and what survives? | [`memory.md`](./memory.md) §"1. Scope — written by one fleet, shared with the workspace only by grant" |
| How does memory travel between runs? | [`runner_fleet.md`](./runner_fleet.md) §"Memory continuity — durable fleet memory rides the trusted plane" |
| What should a fleet store so it survives a new run? | [`capabilities.md`](./capabilities.md) §"Memory hygiene — what to store so it survives" |
| How does a long incident outlive one context window? | [`capabilities.md`](./capabilities.md) §"The three knobs" |
| What can a fleet do, and what is merely advisory? | [`capabilities.md`](./capabilities.md) §"1. Reasoning + tool inventory (declared in the fleet's own files)" |
| When and how is a tenant charged? | [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §"3. The two debit points" |
| What does one event cost, by shape? | [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §"4.3 What an event costs — by shape, not by number" |
| What happens when credits run out? | [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §"6. The credit-exhausted user experience" |
| What free usage does a new tenant get? | [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §"2.3 Free usage is a balance, never a window" |
| Where may the provider `api_key` exist? | [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §"8.2 The api_key visibility boundary" |
| Why does provider activation lock three tables, and what deletes that? | [`tenant_provider_v2.md`](./tenant_provider_v2.md) |
| How does a per-fleet budget cap work? | [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §"5.1 The per-fleet budget gate — a second, independent ceiling" |
| Where do model rates and context caps come from? | [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §"10. The model library — catalogue table + authenticated read" |
| How does a cold machine reach a running fleet? | [`user_flow.md`](./user_flow.md) §"§8.0 The wedge surface", §"§8.2.1 Cold-machine bootstrap (run once per machine)" |
| Where do model and cap originate, per posture? | [`user_flow.md`](./user_flow.md) §"§8.7 Model and context-cap origin (platform vs. self-managed)" |
| What triggers can a fleet's setup declare? | [`user_flow.md`](./user_flow.md) §"§8.3 Triggering the Fleet" |
| What does the Slack bot do — and never do? | [`user_flow.md`](./user_flow.md) §"§8.8 Slack as a resident surface (Rung 0) — M106" |
| How many datastore connections does a deployment need? | [`scaling.md`](./scaling.md) §"Connection budget after the cutover" |
| What is the datastore target, and what has moved so far? | [`datastore_scaling.md`](./datastore_scaling.md) |
| What drives idle request volume? | [`scaling.md`](./scaling.md) §"Per-request volume (now a machine, not a bill)" |
| Which knob do I turn, and when? | [`scaling.md`](./scaling.md) §"Tuneup knobs and when to turn them" |
| Where is the next bottleneck? | [`scaling.md`](./scaling.md) §"Where the next ceiling actually lives" |
| Where does a signal go, and who owns it? | [`observability.md`](./observability.md) §"The three signal paths" |
| What does metric family X mean? | [`observability.md`](./observability.md) §"Metric family census — what to watch, and what it means" |
| Which locks exist, and what does each protect? | [`concurrency.md`](./concurrency.md) §"Lock-invariant registry" |
| What order does shutdown happen in? | [`concurrency.md`](./concurrency.md) §"Shutdown choreography" |
| How do I add a connector provider? | [`connectors.md`](./connectors.md) §"Adding a provider (the recipe)" |
| How does a GitHub event find its fleet? | [`connectors.md`](./connectors.md) §"GitHub App: platform setup to fleet execution" |
| How do drafts, Library revisions and installed fleets relate? | [Fleet definitions and revisions](fleet_definitions.md#drafts-library-and-fleets) |
| What does one lease write, end to end, and when is its sandbox destroyed? | [`lease_flow.md`](./lease_flow.md) — one lease walked end to end against the Rust source, with its open gaps |
| How does a definition become reusable in Library? | [Explicit publication](fleet_definitions.md#explicit-library-publication) |
| What is the proposed chat-to-fleet journey? | [`docs/architecture/high_level.md#fleet-authoring-and-runtime`](high_level.md#fleet-authoring-and-runtime) |
| Where is the complete workspace chat review packet? | [`docs/reviews/workspace-setup/review-packet.md`](../reviews/workspace-setup/review-packet.md) |
| Which documents changed in the chat cutover rewrite? | [Rewrite report](../reviews/workspace-setup/rewrite-report.md) |
| What is the detailed signup-to-result ASCII flow? | [`docs/architecture/data_flow.md#fleet-setup-and-runtime-sequence`](data_flow.md#fleet-setup-and-runtime-sequence) |
| What are all the proposed workspace screens and options? | [`workspace-chat-options.md`](../designs/workspace-chat-options.md) |
| What setup format do both chat clients save? | [Validated Definition](fleet_definitions.md#validated-definition) |
| How does this design compare with the lease-pickup runtime? | [`docs/reviews/workspace-setup/runtime-baseline.md`](../reviews/workspace-setup/runtime-baseline.md) |
| Which resources support fleet setup and runtime? | [API resources](api_resources.md) |
| What are the exact new API inputs, outputs, and missing pieces? | [`docs/architecture/api_resources.md`](api_resources.md) |
| How can creation retry without duplicate fleets or lost schedule fires? | [`docs/architecture/data_flow.md#fleet-creation-and-setup-recovery`](data_flow.md#fleet-creation-and-setup-recovery) |
| How would a runner refine a private draft? | [`docs/architecture/runner_fleet.md#authoring-leases`](runner_fleet.md#authoring-leases) |
| What should Claude Agent Aiwa challenge before agreement? | [`docs/reviews/workspace-setup/claude-review-prompt.md`](../reviews/workspace-setup/claude-review-prompt.md) |
| What prompt should ChatGPT use as CTO AIWA? | [`docs/reviews/workspace-setup/chatgpt-review-prompt.md`](../reviews/workspace-setup/chatgpt-review-prompt.md) |
| Which owner decisions and source answers govern the cutover? | [Decision ledger](../reviews/workspace-setup/decisions.md) and [source findings](../reviews/workspace-setup/source-findings.md) |
| What specs make this one product PR? | [M227_001](../v2/done/M227_001_P0_API_CLI_DOCS_UI_WORKSPACE_CHAT_CUTOVER.md) and [folded M227_002](../v2/done/M227_002_P1_CLI_DOCS_UI_WEBSITE_BRANDING_BATCH.md) |
| Which public changes need a documentation branch? | [Public handoff](../reviews/workspace-setup/public-changes.md) |
| Which test root owns my component? | [`testing.md`](./testing.md) §"Component ownership" |
| When should Rust code use a shared owner or a crate? | [`rust-ownership.md`](./rust-ownership.md) §"Review rubric" |
| What rules govern a client analytics event? | [`product_analytics.md`](./product_analytics.md) §"Client event rules" |
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
| 📦 [`fleet_definitions.md`](./fleet_definitions.md) | Native Definition, immutable revisions and explicit Library publication. |
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
| [`docs/architecture/high_level.md#fleet-authoring-and-runtime`](high_level.md#fleet-authoring-and-runtime) | Common validated fleet setup and reviewed lifecycle. |
| 🧪 [`testing.md`](./testing.md) | Test ownership, lanes and the coverage bar. |
| 🦀 [`rust-ownership.md`](./rust-ownership.md) | When Rust code shares an owner or crate. |
| 🔌 [`connectors.md`](./connectors.md) | Provider connections, App ingress and fleet subscriptions. |
| 💳 [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) | Credits, debit points, provider keys, model library. |
| 🔑 [`tenant_provider_v2.md`](./tenant_provider_v2.md) | Provider activation: today's cost and the plan. |
| 🐙 [`scenarios/github-pr-reviewer.md`](./scenarios/github-pr-reviewer.md) | Install the reviewer; follow one Pull Request (PR). |
| 🚑 [`scenarios/production-deploy-repair.md`](./scenarios/production-deploy-repair.md) | Diagnose a failed deploy toward a draft PR. |
| 💬 [`scenarios/slack-channel-resident.md`](./scenarios/slack-channel-resident.md) | Rung-0 Slack bot: a per-channel resident fleet. |
| 🚨 [`scenarios/slack-incident-responder.md`](./scenarios/slack-incident-responder.md) | An installed fleet answering one Slack channel. |
| 🔀 [`scenarios/two-fleets-github-slack.md`](./scenarios/two-fleets-github-slack.md) | Two fleets end to end, one picture per step. |
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
| **`agentsfleetd`** (control plane) | The daemon (`rustd/crates/agentsfleetd`) that serves the API, owns Postgres and Dragonfly, and hands work to runners. [(more)](./runner_fleet.md#the-split--two-binaries-no-sidecar) |
| **`agentsfleet-runner`** | The host binary (`rustd/crates/agentsfleet_runner`) that asks `agentsfleetd` for leases and runs each one in a sandbox. [(more)](./runner_execution.md#process-model) |
| **Tenant** (`core.tenants`) | The paying account: it owns one or more workspaces and one wallet (`billing.tenant_wallet`). [(more)](../AUTH.md#signup--what-usercreated-writes) |
| **Workspace** (`core.workspaces`) | The space inside a tenant that fleets, secrets and connector installs belong to. [(more)](../AUTH.md#who-opens-a-workspace) |
| **Fleet** (`core.fleets`, `fleet_id`) | A durable AI agent defined by a reviewed Definition, saved into one workspace. [(more)](./high_level.md#1-product-thesis) |
| **Definition** | A validated description, instructions and fleet configuration stored in immutable revisions. [(more)](./fleet_definitions.md#validated-definition) |
| **Producer** (`afd_admission::Producer`) | The ingress path that asked for work (`steer`, `webhook`, `webhook_app`, `schedule_fire`, `gate_continuation` or `slack_mention`), stored on the admission with its dedup key. [(more)](./lease_flow.md#admission-the-row-is-the-acceptance-the-stream-entry-is-a-receipt) |
| **Admission** (`core.fleet_admissions`) | The ledger row that accepts one piece of work for a fleet, unique per `(producer, producer_key)` so a retried delivery is not run twice. [(more)](./lease_flow.md#admission-the-row-is-the-acceptance-the-stream-entry-is-a-receipt) |
| **Fleet stream** (`fleet:{id}:events`) | The fleet's durable Dragonfly stream: each new admission appends one entry, and `agentsfleetd` reads it through consumer group `fleet_lease` when a runner asks for work. [(more)](./data_flow.md#two-streams--one-pubsub-channel--and-the-one-that-retired) |
| **Readiness index** (`fleet:ready:{p}`) | Sixteen Dragonfly hashes marking which fleets have work waiting; a hint the lease poll reads before it touches Postgres, never the record. [(more)](./runner_fleet.md#datastore-topology) |
| **Readiness doorbell** (`fleet:ready:{p}:wake`) | One pub/sub channel per readiness partition; a new mark rings it so the next lease poll checks that partition first. [(more)](./runner_fleet.md#datastore-topology) |
| **Affinity slot** (`fleet.runner_affinity`) | One row per fleet that one runner at a time can claim; a won claim raises `fencing_seq` and holds until `leased_until`. [(more)](./lease_flow.md#the-poll-ready-index--claim--event--row) |
| **Fencing token** | The `fencing_seq` a won claim returns, copied onto the lease as `fencing_token`; a report carrying an older one is refused. [(more)](./runner_fleet.md#system-guarantees-read-this-first) |
| **Lease** (`fleet.runner_leases`, `afd_fleet::lease::Leases`) | One runner's time-limited right to run one event, renewed while the run lasts. [(more)](./data_flow.md#one-active-lease-per-fleet--the-ownership-model) |
| **Event** (`core.fleet_events`) | The history row for one piece of work, written `received` when a lease picks it up and closed by the report. [(more)](./data_flow.md#the-five-durable-stores-who-owns-what) |
| **Report** (`POST /v1/runners/me/reports`) | The runner's terminal result for one run (`afd_wire::report::ReportRequest`), carrying its lease and fencing token. [(more)](./data_flow.md#c-execute--lease--runner--report) |
| **Obligation** (`core.fleet_obligations`) | An answer the fleet owes to a reply destination such as a Slack thread, written with the report and delivered by `afd_outbound`. [(more)](./lease_flow.md#4-how-agent-bob-01-can-reply) |
| **Activity channel** (`fleet:{id}:activity`) | The fleet's pub/sub channel for live-tail frames: best effort, with no history. [(more)](./data_flow.md#two-streams--one-pubsub-channel--and-the-one-that-retired) |
| **Subscription hub** (`afd_dragonfly::SubscriptionHub`) | The one pub/sub connection in each `agentsfleetd` process; it subscribes each channel once and copies frames to every viewer and to the readiness doorbell. [(more)](./data_flow.md#two-streams--one-pubsub-channel--and-the-one-that-retired) |
| **Supervisor** (`afr_supervisor`) | The trusted half of `agentsfleet-runner`, outside every sandbox: it speaks to `agentsfleetd`, holds the model keys and runs the agent loop. [(more)](./runner_execution.md#process-model) |
| **Agent loop** (`afr_agent`) | The supervisor's model loop, outside the sandbox. [(more)](./runner_execution.md#process-model) |
| **Sandbox** (`afr_sandbox`) | The confined Linux environment, built per lease by `BubblewrapEngine`, where that lease's tool calls run with no credentials. [(more)](./runner_execution.md#sandbox-engines) |
| **Egress guard** (`afr_egress`) | The supervisor's outbound guard: it admits each request against the lease's policy and puts secrets in at send time. [(more)](./runner_execution.md#credentials) |
| **Vault** (`vault.secrets`) | Encrypted workspace secrets, one per `key_name`; `afd_vault` writes and lists them, and a lease opens only the ones its fleet declares. [(more)](./capabilities.md#3-platform-level-guarantees-the-substrate-that-wraps-every-tool-call) |
| **Grant** (`core.integration_grants`) | A fleet's permission to use one service's credential, one row per `(fleet_id, service)` with a status. [(more)](./connectors.md#where-a-grant-comes-from) |
| **Connector install** (`core.connector_installs`) | A workspace's link to one provider account, such as a GitHub App installation, unique per `(provider, external_account_id)`. [(more)](./connectors.md#terminology-binding) |
| **Coding fleet** | The workstation tool a human types into. [(more)](./user_flow.md#80-the-wedge-surface) |
| **Steer** | A human message; lands as `actor=steer:<user>`. [(more)](./user_flow.md#83-triggering-the-fleet) |
| **App webhook trigger** | A provider App delivery; GitHub's lands as `actor=github-app`. [(more)](./connectors.md#github-app-platform-setup-to-fleet-execution) |
| **Manual webhook trigger** | A post to `/v1/webhooks/{fleet_id}/{source}`, the URL Save returns for that trigger, signed with the fleet's own webhook secret. [(more)](./user_flow.md#83-triggering-the-fleet) |
| **Trigger panel** | The card on a fleet's page (`/w/{workspaceId}/fleets/{id}`, `TriggerPanel`) showing trigger setup. [(more)](./user_flow.md#84-working-from-claude-or-the-dashboard) |
| **Free usage** | The starter credit a new tenant gets: a balance that drains. [(more)](./billing_and_provider_keys.md#23-free-usage-is-a-balance-never-a-window) |
| **Cron trigger** | An Upstash QStash schedule; QStash owns the clock. [(more)](./user_flow.md#83-triggering-the-fleet) |
| **Run** | One agent-loop pass over one lease. [(more)](./capabilities.md#4-context-lifecycle--keeping-a-long-incident-reasoning-past-the-models-working-memory-limit) |
| **Self-managed provider keys** | The tenant stores and activates its own provider key. [(more)](./billing_and_provider_keys.md#1-the-two-postures) |
| **Bastion** | Post-launch: one fleet for triage and customer comms. [(more)](./high_level.md#61-bastion--one-surface-for-internal-triage-and-customer-comms) |
