# Architecture — v2 Operational Outcome Runner

> [!TIP]
> **Using `agentsfleet`?** Start at **[docs.agentsfleet.net](https://docs.agentsfleet.net)** for login, your first fleet, and runner installation. This directory explains the implementation for contributors.

This directory is the canonical reference for the v2 problem, thesis, runtime model, fleet and runner interaction, capabilities and context lifecycle. Every v2 spec in `docs/v2/` builds on these topic files.

Product thesis: [`high_level.md`](./high_level.md).

## How the system fits together

`agentsfleetd` replicas serve the API and push work to runners. Each runner host holds one stream to one replica.

Postgres holds every durable fact: threads, turns, runs, items, outbound deliveries and usage. Core NATS carries disposable signals only, such as live items and thread wakes.

A lost signal costs latency, never work, because every reader can rebuild its view from Postgres. [`high_level.md`](./high_level.md#how-the-runtime-fits-together) draws the picture, and [`datastore_scaling.md`](./datastore_scaling.md) owns the details.

---

## Why the doc is split this way

Each file holds one topic. A change then lands in one small diff, and a reader
(human or agent) loads only the file that answers their question.

This README is the lookup page. Find your topic in the table below, open that
one file and search inside it. Do not read the whole directory to answer one
question.

## Shared topic ownership

The browser and the terminal create and talk to a fleet through the same topic owners below. [`user_flow.md`](./user_flow.md#81-what-a-fleet-is) owns what a fleet is, and [`data_flow.md`](./data_flow.md#fleet-creation-and-recovery) owns the create transaction and its recovery. [`api_resources.md`](./api_resources.md) owns exact wire shapes.

The runner protocol, `runner_execution`, billing, concurrency and web app pages keep their existing responsibilities. Review files live under `docs/reviews/` and define no other architecture.

## Question → anchor index

Start here. Find the question, then jump to the one §-section that answers it.

The larger topic files open with a Facts table, so the answer is usually on the first screen. Short files, such as `direction.md`, fit on one screen already.

| Question | Where |
|---|---|
| What is a fleet made of? | [`user_flow.md`](./user_flow.md#81-what-a-fleet-is) §"§8.1 What a fleet is" |
| How does a person's first message create a fleet? | [`user_flow.md`](./user_flow.md#822-per-fleet-create-flow) §"§8.2.2 Per-Fleet create flow" |
| How can a create retry without a duplicate fleet, and what survives a crash? | [`data_flow.md`](./data_flow.md#fleet-creation-and-recovery) §"Fleet creation and recovery" |
| What does every run's prompt carry, and what does the run actually hold? | [`capabilities.md`](./capabilities.md#1-what-the-model-is-told-and-what-the-run-holds) §1 |
| Which memory is in every prompt? | [`memory.md`](./memory.md#6-what-reaches-the-model) §"6. What reaches the model" |
| How does a fleet get a secret or an integration such as GitHub? | [`connectors.md`](./connectors.md#where-a-grant-comes-from) §"Where a grant comes from" |
| How does a fleet add a webhook, GitHub or Slack trigger? | [`connectors.md`](./connectors.md#fleet-triggers) §"Fleet triggers" |
| How does a custom webhook prove its signature? | [`connectors.md`](./connectors.md#webhook-signing-recipes) §"Webhook signing recipes" |
| How do several unfinished jobs share one fleet? | [Durable Tasks](durable_tasks.md) |
| How does a fleet start a durable task? | [`durable_tasks.md`](./durable_tasks.md#how-a-task-starts) §"How a task starts" |
| Who judges whether a durable task is done? | [Goal evaluation](goal_evaluation.md#goal-evaluation) |
| Which settings does `agentsfleetd` read from its file, and what are their defaults? | [`config_reference.md`](./config_reference.md) |
| How long can a fleet run before its run expires? | [`runner_fleet.md`](./runner_fleet.md) §"Per-run renewal: how a long fleet keeps its run" |
| What happens when a runner dies mid-turn? | [`runner_fleet.md`](./runner_fleet.md) §"Failure recovery model" |
| Where are checkpoints saved, and how do many failed fleets resume? | [Platform runner checkpoint recovery](runner_checkpoint_recovery.md) |
| How is a stale runner's late report rejected? | [`runner_fleet.md`](./runner_fleet.md) §"System guarantees (read this first)" |
| How does work get assigned, run, and reported? | [`data_flow.md`](./data_flow.md) §"C. EXECUTE  (run → runner → report)" |
| Can two runners hold the same fleet at once? | [`data_flow.md`](./data_flow.md) §"One active run per fleet: the ownership model" |
| How does a new runner get enrolled? | [`runner_fleet.md`](./runner_fleet.md) §"Registering a runner" |
| What are the runner protocol verbs? | [API reference](https://docs.agentsfleet.net/api-reference/introduction) › Runner plane |
| What replaces runner polling, and how do HTTP/3 and HTTP/2 coexist? | [Runner connection and recovery](runner_protocol.md) |
| What sandbox does a held turn run in? | [`runner_fleet.md`](./runner_fleet.md) §"Running one event" |
| How does the Rust runner run a run, and what survives to the next one? | [`runner_execution.md`](./runner_execution.md) §"Process model", §"Workspace between runs" |
| How should we evaluate SPACE and test a recoverable coding session? | [Coding sandbox evaluation](sandbox_evaluation.md) |
| How do three assigned repairs survive later CI and worker replacement, then verify a Grafana/Slack outcome? | [Durable repair scenario](scenarios/durable-repair.md) |
| What network can a sandboxed fleet reach? | [`runner_fleet.md`](./runner_fleet.md) §"Egress model: outbound is the only network surface" |
| Which sandbox tier may run whose work? | [`runner_fleet.md`](./runner_fleet.md) §"Sandbox tiers" |
| How do steer, archive, and delete propagate? | [`runner_fleet.md`](./runner_fleet.md) §"Steer, archive, delete" |
| Where does a webhook / steer / cron fire end up? | [`data_flow.md`](./data_flow.md) §"B. TRIGGER  (steer / webhook / cron — three callers, ONE ingress)" |
| What does one turn write, in what order? | [`data_flow.md`](./data_flow.md) §"Steer flow end-to-end" |
| Which table answers "what did this fleet do"? | [`data_flow.md`](./data_flow.md) §"The five durable stores: who owns what" |
| How does the live tail work — and can it lose frames? | [`data_flow.md`](./data_flow.md) §"D. WATCH  (user-side: how the live tail surfaces)" |
| What happens if the datastore blips during a create? | [`data_flow.md`](./data_flow.md#the-create-failure-scenario-visually) §"The create failure scenario, visually" |
| Why was my webhook rejected, and what do I fix? | [error codes](https://docs.agentsfleet.net/api-reference/error-codes#UZ-WH-020) |
| Who owns cron scheduling? | [`data_flow.md`](./data_flow.md) §"QStash owns the clock" |
| What is memory keyed by, and what survives? | [`memory.md`](./memory.md) §"1. Scope: written by one fleet, shared with the workspace only by grant" |
| How does memory travel between runs? | [`runner_fleet.md`](./runner_fleet.md) §"Memory continuity: durable fleet memory rides the trusted plane" |
| What should a fleet store so it survives a new run? | [`capabilities.md`](./capabilities.md) §"Memory hygiene — what to store so it survives" |
| How does a long incident outlive one context window? | [Conversation compaction](./context_lifecycle.md) |
| When and how is a tenant charged? | [`billing_and_provider_keys.md`](./billing_and_provider_keys.md#3-one-debit-point-usage-charged-as-it-is-written) §"3. One debit point: usage, charged as it is written" |
| What does one turn cost, by shape? | [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §"4.2 What an event costs: by shape, not by number" |
| What happens when credits run out? | [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §"6. The credit-exhausted user experience" |
| What free usage does a new tenant get? | [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §"2.3 Free usage is a balance, never a window" |
| Where may the provider `api_key` exist? | [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §"8.2 The api_key visibility boundary" |
| Where do model rates and context caps come from? | [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §"10. The model library: catalogue table + authenticated read" |
| How does a cold machine reach a running fleet? | [`user_flow.md`](./user_flow.md) §"§8.0 The wedge surface", §"§8.2.1 Cold-machine bootstrap (run once per machine)" |
| Where do model and cap originate, per posture? | [`user_flow.md`](./user_flow.md) §"§8.7 Model and context-cap origin (platform vs. self-managed)" |
| What can wake a fleet besides a message? | [`user_flow.md`](./user_flow.md#83-triggering-the-fleet) §"§8.3 Triggering the Fleet" |
| What does the Slack bot do — and never do? | [`user_flow.md`](./user_flow.md) §"§8.8 Slack mentions and configured fleets" |
| How many datastore connections does a deployment need? | [`scaling.md`](./scaling.md) §"Connection budget after the cutover" |
| What is the datastore target, and what has moved so far? | [`datastore_scaling.md`](./datastore_scaling.md) |
| How will a message reach a runner at 20,000 fleets, and in which milestones? | [`push_scheduler.md`](./push_scheduler.md) |
| What drives idle request volume? | [`scaling.md`](./scaling.md) §"Idle request volume" |
| Which knob do I turn, and when? | [`scaling.md`](./scaling.md) §"Tuneup knobs and when to turn them" |
| Where is the next bottleneck? | [`scaling.md`](./scaling.md) §"Where the next ceiling actually lives" |
| Where does a signal go, and who owns it? | [`observability.md`](./observability.md) §"The three signal paths" |
| What does metric family X mean? | [`observability.md`](./observability.md) §"Metric family census — what to watch, and what it means" |
| Which locks exist, and what does each protect? | [`concurrency.md`](./concurrency.md) §"Lock-invariant registry" |
| What order does shutdown happen in? | [`concurrency.md`](./concurrency.md) §"Shutdown choreography" |
| How do I add a connector provider? | [`connectors.md`](./connectors.md) §"Adding a provider (the recipe)" |
| How does a GitHub event find its fleet? | [`connectors.md`](./connectors.md) §"GitHub App: platform setup to fleet runs" |
| What does one run write, end to end, and when is its sandbox destroyed? | [`run_flow.md`](./run_flow.md) — one run walked end to end against the Rust source, with its open gaps |
| How does a first message become a running fleet, product-wide? | [`high_level.md`](./high_level.md#fleet-creation-and-runtime) §"Fleet creation and runtime" |
| What is the detailed first-message-to-result ASCII flow? | [`data_flow.md`](./data_flow.md#fleet-creation-and-runtime-sequence) §"Fleet creation and runtime sequence" |
| Which routes, run verbs and wire types does a fleet use? | [API resources](api_resources.md) |
| How does the fleet chat stream, and what proves it works? | [Chat implementation and verification](shared_chat_verification.md) |
| Which test root owns my component? | [`testing.md`](./testing.md) §"Component ownership" |
| When should Rust code use a shared owner or a crate? | [`rust-ownership.md`](./rust-ownership.md) §"Review rubric" |
| What rules govern a client analytics event? | [`product_analytics.md`](./product_analytics.md) §"Client event rules" |
| Who may call what, with which token? | [`../AUTH.md`](../AUTH.md) |
| Where is the user-facing version of this page? | the `User-facing:` pointer in each file's header block |

Read in this order if you've never seen the project:

1. [`high_level.md`](./high_level.md): what the product is, what it isn't, and why it exists.
2. [`user_flow.md`](./user_flow.md): how a user gets from "I want a Fleet" to "the Fleet is running on my repo."
3. [`scenarios/slack-incident-responder.md`](./scenarios/slack-incident-responder.md): follow one Slack mention to a fleet's answer in the thread.

> [!IMPORTANT]
> Active and shipped specs cite `user_flow.md` and `scenarios/` by `§`-anchor in acceptance criteria, and so do sibling architecture pages. Both stay here as **contributor-canonical** pages; docs.agentsfleet.net carries its own user coverage. Before moving any user-facing page, `git grep` the spec corpus for references to the file and its anchors.

After that, dip into whichever of these matches the change you're making:

| File | Topic |
|---|---|
| 🧭 [`high_level.md`](./high_level.md) | Product thesis, problem, and why it exists. |
| 📐 [`direction.md`](./direction.md) | Design constants every spec must fit. |
| 🧑‍💻 [`user_flow.md`](./user_flow.md) | What a fleet is, and how a person creates, wakes and supervises one. |
| 🔄 [`data_flow.md`](./data_flow.md) | Where a first message, webhook, steer or cron fire lands. |
| 🧶 [`thread_model.md`](./thread_model.md) | The thread, turn, run, item and delivery tables. |
| 🏃 [`runner_fleet.md`](./runner_fleet.md) | Control plane and runner: running, fencing, recovery. |
| ⚙️ [`runner_execution.md`](./runner_execution.md) | How the runner runs one run. |
| 🎟️ [`run_flow.md`](./run_flow.md) | One run, from install to sandbox teardown. |
| 🧰 [`capabilities.md`](./capabilities.md) | What a fleet can do, and context lifecycle. |
| 🧠 [`memory.md`](./memory.md) | Memory scope, isolation and durability. |
| 📈 [`observability.md`](./observability.md) | Where each signal goes, and who owns it. |
| 📊 [`product_analytics.md`](./product_analytics.md) | PostHog product events and their rules. |
| 🧵 [`concurrency.md`](./concurrency.md) | Tasks, locks, channels and shutdown order. |
| 📏 [`scaling.md`](./scaling.md) | Sizing the runtime and its tuning knobs. |
| 🗄️ [`datastore_scaling.md`](./datastore_scaling.md) | Where state lives: Postgres facts, core NATS signals. |
| 🗃️ [`postgres_schemas.md`](./postgres_schemas.md) | Which Postgres schema owns each table, and the naming rules. |
| 🧬 [`schema_convergence.md`](./schema_convergence.md) | The ruled folds of the chat and durable-task tables into threads, turns, runs and items. |
| 🖥️ [`web_app.md`](./web_app.md) | The dashboard's server-client rules, the fleet composer and the fleet page. |
| 💬 [`shared_chat_verification.md`](./shared_chat_verification.md) | The fleet chat's provider reader, browser lifecycle, tracing, limits and verification evidence. |
| 🎯 [`durable_tasks.md`](./durable_tasks.md) | How a fleet starts, runs and finishes a durable task. |
| ⚖️ [`goal_evaluation.md`](./goal_evaluation.md) | How the decision model judges a task's completion statements. |
| 🧪 [`testing.md`](./testing.md) | Test ownership, lanes and the coverage bar. |
| 🦀 [`rust-ownership.md`](./rust-ownership.md) | When Rust code shares an owner or crate. |
| 🔌 [`connectors.md`](./connectors.md) | Provider connections, App ingress, access grants and fleet triggers. |
| 💳 [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) | Credits, debit points, provider keys, model library. |
| 🚨 [`scenarios/slack-incident-responder.md`](./scenarios/slack-incident-responder.md) | A fleet answering one Slack channel. |
| 🔐 [`../AUTH.md`](../AUTH.md) | Principals, tokens and bearer routing. |

---

## Decision records (published Claude pages)

Long-form decision write-ups live as published Claude pages outside this
repository. Link to them instead of pasting them in. Each row names the
decision it carries.

| Page | Decision |
|---|---|
| [Index audit — slots 033 & 034](https://claude.ai/code/artifact/16b3fe3e-6a0f-47cf-a80f-03f34681ec85) | Which Postgres indexes earn their slots. |
| [Error registry — inventory & curation](https://claude.ai/code/artifact/f5dd342f-633e-4a32-a7ee-579cd2db2427) | The `UZ-*` error-code inventory review. |
| [M120_002 — Admin Model Library, as built](https://claude.ai/code/artifact/3add99c7-6ce3-4617-8656-bf371b658490) | The admin catalogue's final shape. |
| [Fleet library: why your gallery was empty](https://claude.ai/code/artifact/a6b8c064-8643-444b-a43b-2fb2e7e82434) | Root cause of the empty-gallery incident. |
| [Model configuration journeys](https://claude.ai/code/artifact/e0621bf7-7b01-4492-8862-38a43d6f46b3) | How users reach a working model configuration. |

---

## Glossary

One-line definitions for quick lookup. The full, canonical definition lives in the file linked at the end of each row. Drift between this table and that file is a bug.

| Term | Meaning |
|---|---|
| **`agentsfleetd`** (control plane) | The daemon (`rustd/crates/daemon/main`) that serves the API, owns Postgres and its NATS connection, and hands work to runners. [(more)](./runner_fleet.md#the-split-two-binaries-no-sidecar) |
| **`agentsfleet-runner`** | The host binary (`rustd/crates/runner/main`) that receives work from `agentsfleetd` and runs each run with its admitted tools. [(more)](./runner_execution.md#process-model) |
| **Tenant** (`identity.tenants`) | The paying account: it owns one or more workspaces and one wallet (`billing.wallets`). [(more)](../AUTH.md#signup-what-usercreated-writes) |
| **Workspace** (`identity.workspaces`) | The space inside a tenant that fleets, secrets and connector installs belong to. [(more)](../AUTH.md#who-opens-a-workspace) |
| **Fleet** (`fleet.fleets`, `fleet_id`) | A long-lived agent in one workspace: a name, a model, one thread, its pinned memory, its grants, its triggers and schedules, and two network toggles. A person's first message creates it. [(more)](./user_flow.md#81-what-a-fleet-is) |
| **Pinned memory** | A fleet's own `memory.entries` rows in category `core`, which every run's prompt carries under one heading. The fleet pins one with `memory_store` only when a person asks. [(more)](./memory.md#6-what-reaches-the-model) |
| **Producer** (`afd_turns::Producer`) | The ingress path that asked for work (`steer`, `webhook`, `webhook_app`, `schedule_fire`, `question_answer`, `durable_task_continuation` or `slack_mention`), stored on the turn with its dedup key. [(more)](./run_flow.md#intake-commits-the-queue-row) |
| **Thread** (`thread.threads`) | One fleet's conversation, keyed by the fleet's id. It holds the fence, the sticky runner, the warm hold and the resume bookmark; one runner at a time can be assigned it, and an assignment raises `fencing_token` and holds until `run_expires_at`. [(more)](./thread_model.md#threadthreads) |
| **Turn** (`thread.turns`) | One input on a thread and its answer. A fleet turn is unique per `(producer, producer_key)` so a retried delivery is not run twice, written `queued`, opened `received` when a run takes it, and settled by the report. [(more)](./run_flow.md#intake-commits-the-queue-row) |
| **Turns queue** (`thread.turns`) | Unsettled turns ordered by `sequence` within each thread. Gateway assignment passes take the oldest head under the thread and turn fences. [(more)](./datastore_scaling.md#the-turns-ledger-is-the-queue) |
| **Wake notification** (`af.thread.<thread_id>.wake`) | A thread hint on its own NATS subject, which each replica reads through one `af.thread.*.wake` wildcard. It starts a gateway assignment pass. A detected bus gap starts a recovery pass; a lost notification with no gap waits for the next wake or freed runner capacity. Idle runners send no run requests. [(more)](runner_protocol.md#recovery-and-availability) |
| **Fencing token** | The thread's `fencing_token` a won assignment returns, copied onto the run's `fencing_token`; a report carrying an older one is refused. [(more)](./runner_fleet.md#system-guarantees-read-this-first) |
| **Run** (`thread.runs`, `afd_fleet::run::Runs`) | One runner's time-limited right to run one turn, renewed while the run lasts. [(more)](./data_flow.md#one-active-run-per-fleet-the-ownership-model) |
| **Item** (`thread.items`) | One tool call a run made, with its full arguments and output; a page reads it only through "show all". [(more)](./thread_model.md#threaditems) |
| **Report** (`af_wire::protocol::Up::Report`) | The runner's terminal result for one run (`af_wire::report::ReportRequest`), carrying its run and fencing token. [(more)](./data_flow.md#c-execute--run--runner--report) |
| **Delivery** (`thread.deliveries`) | An answer the fleet owes to a reply destination such as a Slack thread, written with the report and delivered by `afd_outbound`. [(more)](./run_flow.md#4-how-agent-bob-01-can-reply) |
| **Items subject** (`af.thread.<thread_id>.items`) | The thread's NATS subject for live-tail frames: at most once, with no history. [(more)](./data_flow.md#two-streams--one-pubsub-channel-and-the-one-that-retired) |
| **Subscription hub** (`afd_bus::nats`) | The one NATS connection in each `agentsfleetd` process; it subscribes each subject once and copies frames to every local reader, wake listeners included. [(more)](./data_flow.md#two-streams--one-pubsub-channel-and-the-one-that-retired) |
| **guffbolt** (`rustd/guffbolt`) | The agent harness: the order one loop's steps run in, the window and the model call each step makes, and the transcript it keeps, over the embedder's conversation and the extension a run of one kind adds, the tools a loop offers, and the children a run starts. The runner embeds it. [(more)](./runner_execution.md#crates) |
| **guffbolt-ai** (`rustd/guffbolt-ai`) | The harness's provider side: the `Model` trait and the request and chunk types guffbolt re-exports, and behind feature `wire`, every provider wire over rig. guffbolt depends on it, never the reverse. [(more)](./runner_execution.md#crates) |
| **Providers** (`af_providers`) | The runner's adapter over `guffbolt-ai`: it builds a run's route from saved model settings, sends through the egress guard and names failures with registry codes. [(more)](./runner_execution.md#crates) |
| **Supervisor** (`afr_supervisor`) | The trusted half of `agentsfleet-runner`, outside every sandbox: it speaks to `agentsfleetd`, holds the model keys and runs the agent loop. [(more)](./runner_execution.md#process-model) |
| **Agent loop** (`afr_agent`) | The supervisor's model loop, outside the sandbox: `guffbolt` embedded over the run, with its durable task, recovery, window, frames and tools. [(more)](./runner_execution.md#crates) |
| **Sandbox** (`afr_sandbox`) | The confined Linux environment, built per run by `BubblewrapEngine`, where that run's tool calls run with no credentials. [(more)](./runner_execution.md#sandbox-engines) |
| **Egress guard** (`af_egress`) | The supervisor's outbound guard: it checks each request against the run's egress policy and puts secrets in at send time. [(more)](./runner_execution.md#credentials) |
| **Vault** (`vault.secrets`) | Encrypted workspace secrets, one per `key_name`; `afd_vault` writes and lists them, and a run opens only the ones a person approved for its fleet. [(more)](./capabilities.md#3-platform-level-guarantees-the-substrate-that-wraps-every-tool-call) |
| **Grant** (`connector.integration_grants`) | A row giving one fleet one credential, a workspace secret or an integration such as GitHub, unique per `(fleet_id, service)`. The fleet asks with `request_access`, and a person approves it once on a card in the chat. [(more)](./connectors.md#where-a-grant-comes-from) |
| **Trigger** (`fleet.triggers`) | A row that lets something outside wake a fleet: a webhook, a GitHub App event or a Slack mention. The fleet adds one with `trigger_add`; a person can list and remove it. [(more)](./connectors.md#fleet-triggers) |
| **Durable task** (`fleet.durable_tasks`) | One accepted, bounded objective a fleet works on across several runs, judged done against its completion statements. The fleet's model starts one with `task_start` under a person's message. [(more)](./durable_tasks.md#how-a-task-starts) |
| **Connector install** (`connector.installs`) | A workspace's link to one provider account, such as a GitHub App installation, unique per `(provider, external_account_id)`. [(more)](./connectors.md#terminology-binding) |
| **Coding fleet** | The workstation tool a human types into. [(more)](./user_flow.md#80-the-wedge-surface) |
| **Steer** | A human message; lands as `actor=steer:<user>`. [(more)](./user_flow.md#83-triggering-the-fleet) |
| **Fleet steer** (M252) | One fleet's message to another fleet in its workspace, admitted as a steer with `actor=fleet:<fleet_id>`. [(more)](./scenarios/chief-of-staff.md) |
| **App webhook trigger** | A provider App delivery to the fleets whose `app_event` trigger names it; GitHub's lands as `actor=github-app`. [(more)](./connectors.md#github-app-platform-setup-to-fleet-runs) |
| **Manual webhook trigger** | A post to `/v1/webhooks/{fleet_id}/{source}`, the URL the fleet page shows for a `webhook` trigger, signed with the workspace secret the trigger names. [(more)](./connectors.md#webhook-signing-recipes) |
| **Wakes on** | The list on a fleet's page (`/w/{workspaceId}/fleets/{id}`) of its schedules and triggers, with remove and each webhook's URL. [(more)](./user_flow.md#84-working-from-claude-or-the-dashboard) |
| **Free usage** | The starter credit a new tenant gets: a balance that drains. [(more)](./billing_and_provider_keys.md#23-free-usage-is-a-balance-never-a-window) |
| **Schedule** (`fleet.schedules`) | A cron row the fleet adds with `cron_add`, or a person adds through the schedule routes; Upstash QStash owns the clock. [(more)](./user_flow.md#83-triggering-the-fleet) |
| **Run** | One agent-loop pass over one run. [(more)](./capabilities.md#4-context-lifecycle--keeping-a-long-incident-reasoning-past-the-models-working-memory-limit) |
| **Self-managed provider keys** | The tenant stores and activates its own provider key. [(more)](./billing_and_provider_keys.md#1-the-two-postures) |
| **Bastion** | Post-launch: one fleet for triage and customer comms. [(more)](./high_level.md#61-bastion--one-surface-for-internal-triage-and-customer-comms) |
