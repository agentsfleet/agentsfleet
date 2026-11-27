---
type: reference
audience: contributor
verified: 2026-11-27
product_version: 0.90.0
executable: false
---

# Architecture index

> [!TIP]
> **Using `agentsfleet`?** Start at **[docs.agentsfleet.net](https://docs.agentsfleet.net)** for login, your first fleet and runner installation. This folder explains the implementation for contributors.

This folder explains how `agentsfleet` is built. Each page owns one topic, and every other page links to it.
Find your question below, open that one section, and stop there.

Why the product exists, and for whom, is in the [product thesis](../product/thesis.md).
Reviews and evaluations live under `docs/reviews/` and define no architecture.

## Question index

Each row answers one question, and no two rows share a section.

| Question | Section |
|---|---|
| How do the pieces fit together? | [high_level.md#the-pieces](./high_level.md#the-pieces) |
| Which page owns a topic? | [high_level.md#where-each-topic-lives](./high_level.md#where-each-topic-lives) |
| What is a fleet made of? | [user_flow.md#what-a-fleet-is-made-of](./user_flow.md#what-a-fleet-is-made-of) |
| How does a first message create a fleet? | [user_flow.md#starting-a-fleet](./user_flow.md#starting-a-fleet) |
| How does a fleet get a secret or GitHub access? | [user_flow.md#access](./user_flow.md#access) |
| What can wake a fleet? | [user_flow.md#wakes-on](./user_flow.md#wakes-on) |
| How does a person or another fleet message a fleet? | [user_flow.md#talking-to-it](./user_flow.md#talking-to-it) |
| How can a create retry without making a second fleet? | [data_flow.md#create-and-recovery](./data_flow.md#create-and-recovery) |
| How does a message become a turn? | [data_flow.md#intake](./data_flow.md#intake) |
| Who fires a schedule? | [data_flow.md#schedules](./data_flow.md#schedules) |
| How is work assigned, and can two runners hold one fleet? | [data_flow.md#assignment-and-the-fence](./data_flow.md#assignment-and-the-fence) |
| What happens to a message sent during a live run? | [data_flow.md#reports-and-folded-messages](./data_flow.md#reports-and-folded-messages) |
| How does a viewer watch a fleet live? | [data_flow.md#watching-a-fleet](./data_flow.md#watching-a-fleet) |
| How does one pull request review run, end to end? | [run_flow.md#3-one-run-end-to-end](./run_flow.md#3-one-run-end-to-end) |
| What does every run's prompt carry? | [capabilities.md#the-fixed-prompt](./capabilities.md#the-fixed-prompt) |
| What guards every tool call? | [capabilities.md#guarantees-around-every-tool-call](./capabilities.md#guarantees-around-every-tool-call) |
| Which tools can a run call? | [runner_execution.md#tool-catalog](./runner_execution.md#tool-catalog) |
| Which files does a sandbox see? | [runner_execution.md#sandbox-filesystem](./runner_execution.md#sandbox-filesystem) |
| What can a sandbox reach on the network? | [runner_execution.md#network-egress](./runner_execution.md#network-egress) |
| Which engine builds a sandbox, and what walls one microVM off from another? | [runner_execution.md#sandbox-engines](./runner_execution.md#sandbox-engines) |
| Which memory reaches the model? | [memory.md#what-reaches-the-model](./memory.md#what-reaches-the-model) |
| How does a long conversation stay inside the model's window? | [context_lifecycle.md#deciding-to-compact](./context_lifecycle.md#deciding-to-compact) |
| How does a fleet keep one objective across runs? | [durable_tasks.md#how-a-task-starts](./durable_tasks.md#how-a-task-starts) |
| Who judges whether a durable task is done? | [durable_tasks.md#goal-evaluation](./durable_tasks.md#goal-evaluation) |
| How does a Slack mention reach a fleet? | [slack_mentions.md#how-a-mention-is-routed](./slack_mentions.md#how-a-mention-is-routed) |
| How does a GitHub App event reach a fleet? | [connectors.md#github-app-from-install-to-a-routed-event](./connectors.md#github-app-from-install-to-a-routed-event) |
| How does a custom webhook prove its signature? | [connectors.md#webhook-signing-recipes](./connectors.md#webhook-signing-recipes) |
| How is a new provider added? | [connectors.md#adding-a-provider](./connectors.md#adding-a-provider) |
| Which model does a run use? | [models.md#resolving-a-runs-model](./models.md#resolving-a-runs-model) |
| Where does a provider key live? | [models.md#where-the-provider-key-lives](./models.md#where-the-provider-key-lives) |
| How is a run charged? | [billing.md#how-a-run-is-charged](./billing.md#how-a-run-is-charged) |
| What happens when the balance runs out? | [billing.md#when-the-balance-runs-out](./billing.md#when-the-balance-runs-out) |
| Which route takes which body? | [api_resources.md#fleet-routes](./api_resources.md#fleet-routes) |
| Who may call a route? | [api_resources.md#authorization](./api_resources.md#authorization) |
| How does a runner register? | [runner_fleet.md#registering-a-runner](./runner_fleet.md#registering-a-runner) |
| What happens when a runner dies mid-turn? | [runner_fleet.md#failure-recovery-model](./runner_fleet.md#failure-recovery-model) |
| How does a long run keep its lease? | [runner_protocol.md#run-lease-and-renewal](./runner_protocol.md#run-lease-and-renewal) |
| What crosses the runner stream? | [runner_protocol.md#four-streams-on-one-connection](./runner_protocol.md#four-streams-on-one-connection) |
| How does a run resume on a replacement runner? | [checkpoints.md#restore-on-a-replacement-runner](./checkpoints.md#restore-on-a-replacement-runner) |
| What does each thread table hold? | [thread_model.md#how-it-behaves](./thread_model.md#how-it-behaves) |
| Which schema holds which table? | [postgres_schemas.md#layout](./postgres_schemas.md#layout) |
| What does deleting a fleet remove? | [postgres_schemas.md#deletion-matrix](./postgres_schemas.md#deletion-matrix) |
| Where is the work queue? | [datastore_scaling.md#the-turns-ledger-is-the-queue](./datastore_scaling.md#the-turns-ledger-is-the-queue) |
| What if a NATS signal is lost? | [datastore_scaling.md#when-a-signal-is-lost](./datastore_scaling.md#when-a-signal-is-lost) |
| Which rows lock, and in what order? | [concurrency.md#row-lock-order](./concurrency.md#row-lock-order) |
| What limits throughput first as load grows? | [scaling.md#what-binds-first](./scaling.md#what-binds-first) |
| Which setting tunes performance? | [config_reference.md#tuning-for-performance](./config_reference.md#tuning-for-performance) |
| Where does a log, metric or trace go? | [observability.md#signal-routing](./observability.md#signal-routing) |
| What may a product analytics event carry? | [product_analytics.md#rules-for-every-event](./product_analytics.md#rules-for-every-event) |
| How does the dashboard reach the API? | [web_app.md#how-the-browser-reaches-the-api](./web_app.md#how-the-browser-reaches-the-api) |
| How does the browser recover a lost stream or send? | [web_app.md#sending-and-reconnecting](./web_app.md#sending-and-reconnecting) |
| Which test lane proves a change? | [testing.md#lanes](./testing.md#lanes) |
| How is a release promoted? | [release_candidates.md#the-jobs](./release_candidates.md#the-jobs) |
| Which crate owns a mechanism? | [rust-ownership.md#how-it-behaves](./rust-ownership.md#how-it-behaves) |

## Pages

| Page | Owns |
|---|---|
| [`high_level.md`](./high_level.md) | The pieces, the design rules and where each topic lives |
| [`user_flow.md`](./user_flow.md) | What a fleet is, how it starts, what it may access and what wakes it |
| [`data_flow.md`](./data_flow.md) | A message's path from intake to answer, schedules and the live view |
| [`run_flow.md`](./run_flow.md) | One pull request review, end to end |
| [`capabilities.md`](./capabilities.md) | What a fleet is told and what its run enforces |
| [`memory.md`](./memory.md) | What a fleet remembers across runs |
| [`context_lifecycle.md`](./context_lifecycle.md) | Compacting a conversation near the model's window |
| [`durable_tasks.md`](./durable_tasks.md) | One objective kept going across runs |
| [`connectors.md`](./connectors.md) | Provider connections, the GitHub App and webhook signing |
| [`slack_mentions.md`](./slack_mentions.md) | Slack mentions, routing and answers |
| [`models.md`](./models.md) | Which model a fleet uses and whose key pays |
| [`billing.md`](./billing.md) | Charges, the funds check and the usage report |
| [`api_resources.md`](./api_resources.md) | Fleet routes, access and run verbs |
| [`runner_fleet.md`](./runner_fleet.md) | Registering runners, their state, placement and recovery |
| [`runner_protocol.md`](./runner_protocol.md) | The runner stream, leases and refusals |
| [`runner_execution.md`](./runner_execution.md) | The runner runtime, tools, sandbox engines and egress |
| [`checkpoints.md`](./checkpoints.md) | Saving and restoring a run |
| [`thread_model.md`](./thread_model.md) | The thread tables |
| [`postgres_schemas.md`](./postgres_schemas.md) | Schemas, rules and deletion |
| [`datastore_scaling.md`](./datastore_scaling.md) | Postgres facts and NATS signals |
| [`concurrency.md`](./concurrency.md) | Tasks, channels, locks and shutdown |
| [`scaling.md`](./scaling.md) | Limits, budgets, measurements and sizing |
| [`config_reference.md`](./config_reference.md) | Every `agentsfleetd` and `agentsfleet-runner` setting |
| [`observability.md`](./observability.md) | Logs, metrics and traces |
| [`product_analytics.md`](./product_analytics.md) | PostHog events and what they may carry |
| [`web_app.md`](./web_app.md) | How the dashboard is built |
| [`testing.md`](./testing.md) | Which lane proves which change |
| [`release_candidates.md`](./release_candidates.md) | Building once and promoting the same bytes |
| [`rust-ownership.md`](./rust-ownership.md) | Which crate owns which mechanism |

Authentication, tokens and scopes live in [`../AUTH.md`](../AUTH.md).

## Glossary

Each row is a one-line pointer; the linked section owns the definition.

| Term | Meaning |
|---|---|
| **`agentsfleetd`** | The daemon (`rustd/crates/daemon/main`): it serves the API, owns Postgres and its NATS connection, and hands work to runners. [(more)](./runner_fleet.md#why-it-exists) |
| **`agentsfleet-runner`** | The host binary (`rustd/crates/runner/main`) that runs each run with its admitted tools. [(more)](./runner_execution.md#process-model) |
| **`agentsfleet-guest`** | The guest agent (`rustd/crates/guest/main`): each microVM's first process, serving its run's processes and files to the runner over vsock; built only into the guest's initramfs. [(more)](./runner_execution.md#inside-a-microvm) |
| **Tenant** (`identity.tenants`) | The paying account; it owns workspaces and one wallet. [(more)](../AUTH.md#signup-what-usercreated-writes) |
| **Workspace** (`identity.workspaces`) | The space inside a tenant that fleets, secrets and connector installs belong to. [(more)](../AUTH.md#who-opens-a-workspace) |
| **Fleet** (`fleet.fleets`) | A long-lived agent in one workspace with a name, a model and one thread. [(more)](./user_flow.md#what-a-fleet-is-made-of) |
| **Pinned memory** | A fleet's `core` memory entries, carried in every run's prompt. [(more)](./memory.md#categories-and-the-load-window) |
| **Thread** (`thread.threads`) | One fleet's conversation, keyed by the fleet's ID; it holds the fence. [(more)](./thread_model.md#threadthreads) |
| **Turn** (`thread.turns`) | One input on a thread and its answer, unique per producer and key. [(more)](./thread_model.md#threadturns) |
| **Fencing token** | The thread's counter an assignment raises; a report with an older one is refused. [(more)](./data_flow.md#assignment-and-the-fence) |
| **Run** (`thread.runs`) | One runner's time-limited right to run one turn. [(more)](./thread_model.md#threadruns) |
| **Item** (`thread.items`) | One tool call a run made, with its arguments and output. [(more)](./thread_model.md#threaditems) |
| **Delivery** (`thread.deliveries`) | An answer the fleet owes to a destination such as a Slack thread. [(more)](./data_flow.md#outbound-deliveries) |
| **Wake** | A NATS hint that starts an assignment pass; losing one costs latency, never work. [(more)](./datastore_scaling.md#the-turns-ledger-is-the-queue) |
| **Supervisor** (`afr_supervisor`) | The trusted half of the runner, outside every sandbox; it holds model keys. [(more)](./runner_execution.md#crates) |
| **Sandbox** (`afr_engine`) | The confined environment where one run's tool calls run: a Firecracker microVM. [(more)](./runner_execution.md#sandbox-engines) |
| **Engine** (`afr_engine_*`) | What builds a runner's sandboxes, chosen by `agentsfleet-runner run --engine`. [(more)](./runner_execution.md#sandbox-engines) |
| **Egress guard** (`af_egress`) | The supervisor's outbound check, which also puts secrets in at send time. [(more)](./runner_execution.md#credentials) |
| **Grant** (`connector.integration_grants`) | One fleet's approved credential: a workspace secret or an integration. [(more)](./user_flow.md#access) |
| **Trigger** (`fleet.triggers`) | A row that lets a webhook, a GitHub App event or a Slack mention wake a fleet. [(more)](./user_flow.md#wakes-on) |
| **Schedule** (`fleet.schedules`) | A cron row the fleet or a person adds. [(more)](./data_flow.md#schedules) |
| **Durable task** (`fleet.durable_tasks`) | One bounded objective a fleet works on across runs. [(more)](./durable_tasks.md#what-it-is) |
| **Steer** | A person's or another fleet's message to a fleet. [(more)](./user_flow.md#talking-to-it) |
| **guffbolt** (`rustd/guffbolt`) | The agent harness the runner embeds. [(more)](./runner_execution.md#crates) |
