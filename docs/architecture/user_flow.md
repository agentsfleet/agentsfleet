# User Flow — how a user uses the system

> Parent: [`README.md`](./README.md) · User-facing: [docs.agentsfleet.net/quickstart](https://docs.agentsfleet.net/quickstart) walks the same path in five minutes; [docs.agentsfleet.net/fleets/overview](https://docs.agentsfleet.net/fleets/overview) is its reference.

Read this to learn how a real human gets from "I want a Fleet" to a Fleet running on their repo. The §-numbered subsections are stable anchors that other specs reference. Do not rename them without sweeping cross-references.

The initial user assumption is simple:

- the user is already working inside Claude (or Amp, Codex command-line interface (CLI), OpenCode), or in the `agentsfleet` dashboard
- the user is already working on their own project or infrastructure
- the user wants operational work to continue without babysitting an endless terminal loop

A fleet's chat, in the browser or the terminal, is where a person starts a fleet, talks to it and supervises it. The Fleet runtime is where long-lived operational outcomes continue after the chat ends.

The [fixture fleets](../../tests/fixtures/fleets/README.md) are the worked examples.
Each file says what one fleet is, what a person types to it and what that proves.

## Facts

Every row is extracted from the §-numbered sections below; the owner column names the section that carries the full story.

| Invariant | Value | Mechanism | Owner section |
|---|---|---|---|
| The wedge surface | `agentsfleet` CLI and dashboard, both over a fleet's chat | `curl -fsSL https://agentsfleet.dev \| bash` → `login` → `chat` | §8.0 |
| List output shape | name, identifier, domain columns, `AGO` | one helper owns the order for every entity list the CLI prints, and appends the age itself, so a call site has neither a position to choose nor a column to forget; a catalogue with no age of its own declares that rather than rendering an empty column | §8.4 |
| Create input | a person's first message | `POST W/fleets {message, operation_id, model_ref?, name?}` writes the fleet, its thread and its first turn in one transaction | §8.2.2 |
| Deployment posture | hosted-only in v2 | no `--self-host` flag, no self-host runbook; validation on non-Fly hosts is v3 | §8.0.1 |
| What a fleet is | a name, a model, a thread, pinned memory, grants, triggers, schedules and two network toggles | nothing about the job is written before the first message; the fleet adds the rest by tool, and a person approves its access | §8.1 |
| Preflight | `agentsfleet doctor --json`, health only | `server_reachable` · `workspace_selected` · `workspace_binding_valid`; the only sanctioned preflight surface | §8.2.2 |
| Access | asked for by the fleet, approved once by a person | `request_access` raises a card; a run holds only the credentials a person approved for its fleet | §8.1 |
| Triggers | rows the fleet adds by tool | `trigger_add` writes `fleet.triggers`: one webhook per source, one GitHub App subscription, one Slack channel; refused when the workspace has not connected the provider | §8.3 |
| Cron authority | QStash (Upstash's hosted scheduler) owns the clock | the fleet's `cron_add` or a person writes a `fleet.schedules` row; `afd_cron` mirrors it to QStash; the runner owns no timer | §8.3 |
| Steer | always accepted | same single ingress as webhook and cron: a committed queued turn in `thread.turns` with `actor=steer:<user>`; the answer names a disposition (`started`, `folded`, `queued`) | §8.3 |
| Model identity | stored on the fleet | `fleet.fleets.model_ref`: the create body's, else the workspace's selected model; one `PATCH F` changes it | §8.7 |
| Context limits | the selected model's saved window | each run reads the saved model window and maximum output, without a provider lookup; a fleet carries no context override | §8.7 |
| Conversation compaction | Defaults to 90 percent of the selected physical window | Output headroom can trigger it earlier; valid checkpoints let the same run continue with tools. | §8.7 |
| Slack surface | Subscribed fleet routing | A fleet's `mention` trigger row subscribes it to one channel; an unsubscribed channel returns `channel_not_configured`. | §8.8 |

## Traps

Each trap is enforced in its owner section; this list is the index.

- The §-numbered subsections are stable anchors; never rename them without sweeping cross-references (preamble).
- Doctor carries no provider or billing posture; read `tenant provider show` and `billing show` separately once health passes (§8.2.2).
- A GitHub App trigger names its repositories; a delivery for any other repository reaches no fleet (§8.3).
- The fleet's in-run loop never branches on actor; the conversation and the pinned memory dictate behaviour (§8.3).
- Never resolve context caps at trigger time (§8.7).
- The dashboard never asks for or stores a user's provider Personal Access Token (§8.2.2, §8.4).
- The browser never repeats a failed workspace POST on its own (§8.4).
- A fleet starts `active` with no credential; a missing one is a card the fleet raises when needed, never a refused create (§8.2.2).
- Messages and pinned memory are prose and grant nothing; capability comes only from the run's policy (§8.1).
- A Slack mention creates no default fleet and grants no unattended work authority (§8.8).

## Topology

The dual-posture model/cap resolution diagram lives in §8.7; it is that section's proof and stays there.

## Decisions

| Decision | Reason | Where |
|---|---|---|
| Hosted-only v2 | launch claim is "open-source software (OSS) + self-managed + chat-authored", not "self-hostable"; integration burden earns v3 | §8.0.1 |
| A first message creates the fleet | one call writes the fleet and its first turn, so the chat a person started is the fleet's chat; browser and terminal share that route | §8.2.2 |
| The fleet asks for access, and a person approves it once | the fleet learns what it needs while it works; the card names the credential and a person decides | §8.1 |
| Context limits come from the selected model | a fleet carries no context override, so changing the model changes the window | §8.7 |
| `/credentials` page removed outright, not redirected | provider keys live on `/settings/models`; custom secrets on `/secrets` | §8.7 |

---

## Detail

Everything below is the full reference. Headings are stable because specs cite them by §-number; insert new sections, never rename or renumber existing ones.

## §8.0 The wedge surface

The minimum viable product (MVP)'s user-facing wedge is the **`agentsfleet` Command-Line Interface (CLI) and dashboard, both over a fleet's chat**. A user goes from a cold machine to a running fleet by saying what the fleet should do:

```bash
curl -fsSL https://agentsfleet.dev | bash   # installs the agentsfleet CLI
agentsfleet login                            # Clerk OAuth
agentsfleet fleet create "<MESSAGE>"         # the first message creates the fleet
agentsfleet fleet steer @<NAME>              # keep talking to it
```

`agentsfleet fleet steer <FLEET>` with no message is an interactive chat with an existing fleet, named by its id or as `@<NAME>`; each line is one turn.

`agentsfleet fleet create "<MESSAGE>" [--name <NAME>] [--model <MODEL_REF>]` creates a fleet from one message. With `--json` it prints `{fleet_id, turn_id}`.

`<MESSAGE>` is the first thing the fleet should do. Choose `<MODEL_REF>` from the `ref` values `GET /v1/workspaces/{ws}/models` returns; without it, the fleet takes the workspace's selected model (§8.7). The dashboard's New fleet composer calls the same route ([`api_resources.md`](./api_resources.md)).

A person asks for a Slack channel, a webhook or a cron schedule in the chat, and the fleet adds it by tool. The fleet page lists what the fleet added, and a person can remove any of it (§8.4).

## §8.0.1 Deployment posture: hosted-only in v2

v2 ships **hosted-only** on `api.agentsfleet.net`. The skill detects no choice point: it defaults to the hosted endpoint, prompts Clerk OAuth via `agentsfleet login` if the CLI is not authenticated, and proceeds. There is no self-host runbook in v2 and no `--self-host` flag.

This is a deliberate scope cut. The auth substrate (Clerk OAuth), the key management service (KMS) adapter, and process control (Fly.io machines) are the only deployment-specific layers. The worker, runner, sandbox, event stream, and reasoning loop are all posture-agnostic.

Validating that on a clean non-Fly Linux host is a v3 workstream. It waits until v2 has earned the trust to justify the integration burden.

That validation covers a Clerk shim or local-token auth, a portable KMS adapter and systemd process control. The validation also covers the runner's Landlock+cgroups+bwrap on a vanilla VM.

Practically, this means:

- v2 launch claim is **OSS + self-managed + chat-authored**. Not "self-hostable."
- The `/self-host` runbook page does not exist on `docs.agentsfleet.net` for v2.
- Users who need self-host today are out of scope. The AI-infra / GPU-cloud / regulated mid-market P1 personas are v3 customers, not v2.
- self-managed still ships in v2. It sits on top of the hosted posture and removes the inference-cost lock-in independently of where the runtime runs. See [`capabilities.md`](./capabilities.md) and [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §1.

## §8.1 What a fleet is

A fleet is a name, a model, a thread, its pinned memory, its grants, its triggers, its schedules and two network toggles. Nothing about its job is written down before a person's first message. Nothing is private either: every workspace member reads a new fleet at once.

| Part | Where it lives | Who sets it |
|---|---|---|
| Name | `fleet.fleets.name`, unique in the workspace | the create body, else `fleet-` and the last 12 hex digits of the fleet's id; a person renames it with `PATCH F` ([name rule](api_resources.md#fleet-and-common-types)) |
| Model | `fleet.fleets.model_ref` | the create body, else the workspace's selected model; a person changes it with `PATCH F` (§8.7) |
| Thread | `thread.threads`, whose id is the fleet's | written with the fleet; every message, trigger delivery and schedule fire is a turn on it ([thread model](thread_model.md)) |
| Pinned memory | `memory.entries` under category `core` | the fleet, with `memory_store`, when a person asks; every later run's prompt carries it ([memory](memory.md#6-what-reaches-the-model)) |
| Grants | `connector.integration_grants` | the fleet asks with `request_access`; a person approves the card once ([grants](connectors.md#where-a-grant-comes-from)) |
| Triggers | `fleet.triggers` | the fleet, with `trigger_add`; a person can remove them (§8.3) |
| Schedules | `fleet.schedules` | the fleet, with `cron_add`, or a person (§8.3) |
| Network | `fleet.fleets.http_egress` and `package_domains` | off at birth; only a signed-in person flips "Allow all" and "Packages" ([egress model](runner_fleet.md#egress-model-outbound-is-the-only-network-surface)) |

The person shapes the fleet in natural language, and the fleet calls the matching tool:

- "always link the evidence" pins standing guidance with `memory_store` under `core`
- "check it every 15 minutes" adds a schedule with `cron_add`
- "wake on Sentry alerts" adds a webhook trigger with `trigger_add`, and the fleet page shows its URL
- "fix the null-cart error until PR 88 is green" starts a [durable task](durable_tasks.md#how-a-task-starts) with `task_start`

Each tool that changes the fleet says in its description to call it only when the person's newest message asks. A refused call returns to the model as `[kind] sentence`, and the model may correct it in the same run.

Messages and pinned memory grant no access. A run holds the published tool catalog, the credentials a person approved for its fleet, and the network its toggles allow ([capabilities](capabilities.md#1-what-the-model-is-told-and-what-the-run-holds)).

When the fleet needs a secret or an integration, it calls `request_access` with its name. A card appears in the chat, a signed-in person clicks Approve once, and the waiting turn resumes holding it.

The New fleet suggestion buttons load editable message text from `ui/packages/app/public/fleet-starters.json` into the composer. The starters check a deploy, watch a pull request, or set up a Chief of Staff that reads fleet activity and delegates requests. The person edits and sends that text as an ordinary first message; Rust supplies no canned reply and no default completion statement.

## §8.2 Creating the Fleet

A person's first message creates the Fleet in the workspace.

### §8.2.1 Cold-machine bootstrap (run once per machine)

The canonical entry is the one-liner served from `https://agentsfleet.dev`, which installs the `agentsfleet` CLI:

```bash
curl -fsSL https://agentsfleet.dev | bash   # installs the agentsfleet CLI (npm under the hood)
```

Or run the chain explicitly (skip any step already in place):

```bash
npm install -g @agentsfleet/cli   # CLI binary
agentsfleet login                  # Clerk OAuth → credential in ~/.config/agentsfleet/credentials.json
agentsfleet whoami                 # who that credential belongs to
agentsfleet connections show github --json
```

Login proves the new credential against `GET /v1/users/me`, which needs no capability scope ([API reference](https://docs.agentsfleet.net/api-reference/introduction) › Users).

`agentsfleet doctor --json` is the readiness gate (§8.2.2 step 1): on any miss it prints the explicit fix commands and stops. The commands are deliberately separate so a user with most of the chain already in place skips what they already have.

### §8.2.2 Per-Fleet create flow

1. **`agentsfleet doctor --json` runs first** as the deterministic readiness gate after login. Doctor is fast and verifies connectivity and workspace health only: `server_reachable`, `workspace_selected`, and `workspace_binding_valid`. It is the only sanctioned preflight surface for health; no parallel `preflight` command exists.
   - Doctor does **not** carry provider posture or balance. Those live in `agentsfleet models current --json` (mode/provider/model/context cap) and `agentsfleet usage show` (balance and exhaustion state), read separately once health passes.
   - The CLI (and any caller) reads `doctor`'s JSON output verbatim and aborts on failure with the user-facing message.
2. **First message.** The person sends it with `agentsfleet fleet create` or Send on New fleet. Each calls `POST /v1/workspaces/{ws}/fleets` with `{message, operation_id, model_ref?, name?}` under scope `fleet:write`.
3. **One transaction.** `afd_setup`'s create writes the fleet `active`, its thread, whose id is the fleet's, and turn #1 `queued` with actor `steer:<person>`. After the commit it publishes the wake and the per-tenant fleet notice.
   - A retry with the same `operation_id` answers the same fleet. [`data_flow.md`](./data_flow.md#fleet-creation-and-recovery) owns the retry key and recovery after a crash.
   - A refused create writes nothing: no usable model is `409 UZ-SETUP-008`, and a taken name is `409 UZ-AGT-006`. [`api_resources.md`](./api_resources.md) lists every code.
4. **The first run.** The turn is assigned like any other turn. Its run holds the tool catalog and no credential, because nothing is approved yet. The fleet asks for one with `request_access` when it needs it (§8.1).
5. **Provider wiring follows the trigger.** A GitHub App or Slack trigger needs the workspace connection first, and the fleet's `trigger_add` is refused without it. A custom webhook trigger gets a per-fleet URL, which the fleet page shows. The platform holds its own App secrets, never a user's Personal Access Token (PAT) ([`connectors.md`](./connectors.md) §"GitHub App: platform setup to fleet runs").
6. Future triggers are served with no restart and no watcher thread. Each trigger commits a queued turn to `thread.turns`. The control plane assigns that turn to an eligible `agentsfleet-runner` as a run, pushed as `Down::Run` on the runner's connection.
   - [`data_flow.md`](./data_flow.md) §"Steer flow end-to-end" carries the full path.

The chat that created the Fleet is the Fleet's own chat; later messages land on the same thread.

### §8.2.3 First scenarios

Two first scenarios anchor the product flow:

- **GitHub Pull Request reviewer.** Wakes on GitHub pull request events and reads diff/context through `api.github.com`. Posting review comments needs a GitHub grant whose scope a person approved for write.
- **Zoho Recruit outreach.** Reads Zoho Recruit data and optionally sends mail through a separate secret; its provider-specific guidance lives in its pinned memory.

## §8.3 Triggering the Fleet

A Fleet wakes on the messages sent to it, on its triggers and on its schedules. A trigger is a `fleet.triggers` row the fleet adds with `trigger_add` when a person's newest message asks ([fleet triggers](connectors.md#fleet-triggers)). A fleet may add any trigger the workspace has connected, with no other limit; a fleet with none is woken only by messages.

Each trigger row is one of:

- **GitHub App trigger.** Kind `app_event`, source `github`, with its `repositories` and `events`. Any other repository's delivery, or another workspace's, reaches no fleet ([`connectors.md`](./connectors.md) §"GitHub App: platform setup to fleet runs").
- **Webhook trigger.** Kind `webhook`, with a lowercase-slug `source` other than `approval` or `github`, and a `secret_name` naming a workspace secret. Its signature follows one of the [webhook signing recipes](connectors.md#webhook-signing-recipes).
  - Each webhook row has one URL, `POST /v1/webhooks/{fleet_id}/{source}`, which the fleet page shows. The operator registers the URL and the signing secret with the provider, and a delivery lands as `actor=webhook:<source>`.
- **Slack mention trigger.** Kind `mention`, one Slack channel per row (§8.8).

A schedule is a `fleet.schedules` row, added by the fleet's `cron_add` or by a person through the schedule routes. Each row stands alone; `afd_cron` mirrors it to QStash under a stable identifier.

- QStash owns the clock and sends each signed fire to `agentsfleetd`, which writes one queued turn to `thread.turns` with `actor=cron:<schedule_id>`. The runner owns no timer.

In addition to its triggers and schedules, every Fleet always accepts:

- **User steer.** The user asks Claude to run an operational task. Claude invokes `agentsfleet fleet steer {id} "<message>"` or uses the dashboard's composer on `/fleets/{id}`, which POSTs to `/v1/workspaces/{ws}/fleets/{id}/messages`.

  `afd_turns::Steer::append` commits a queued turn with `actor=steer:<user>` and sends a best-effort wake notification. Webhook and cron write the same `thread.turns` rows.

  The 202 answer names a disposition. A message sent while the fleet's run is live is `folded` into that run, which takes it at its next model call. Otherwise the message has `started` a run of its own, or is `queued` behind the work ahead of it.

  The dashboard names every send with an `operation_id`, a version 7 universally unique identifier (UUID v7). It keeps unresolved sends in a per-user, per-fleet ledger mirrored to `localStorage` until the 202 response arrives. **Try again** carries the same identifier, including after reload.

  Retrying with that identifier returns the first turn ([API reference](https://docs.agentsfleet.net/api-reference/introduction) › Fleet messages). A send unanswered after 30 seconds shows “Sending was interrupted.” beside **Try again**.

  Another tab's send stays hidden while that tab holds its Web Lock, then shows **Try again** once the tab is gone. Every tab keeps a dismissal tombstone for one day, so a late write cannot restore a dismissed send. A final `UZ-AGT-016` refusal offers **Send as new** under a fresh identifier.

  A replayed answer for a turn the page never loaded reads that turn's detail and settles its row. A turn already loaded settles from its own frames. A running turn unheard from for 45 seconds is read once, so a lost completion still ends its row.

- **Fleet steer.** Any fleet may message any fleet in its workspace, as often as the payer's balance allows. The turn lands with `actor=fleet:<fleet_id>`.

All actors flow through the same runtime path. The Fleet's in-run loop does not branch on actor type. The same `http_request`-driven evidence gathering and Slack post happen however the work was triggered.

A "morning health check" steer produces a real first-pass evidence sweep; the conversation and the fleet's pinned memory dictate behaviour, never the actor field.

A fleet with no trigger is valid: messages reach every fleet.

### §8.3.1 Reaching a fleet by name and fleet to fleet steering

A person reaches any fleet by its name, and a fleet hands work to any fleet in its workspace and hears back.
Nobody copies a UUID, and nobody relays between fleets.
The [`queen-bee`](../../tests/fixtures/fleets/queen-bee.json) and [`busy-bee`](../../tests/fixtures/fleets/busy-bee.json) fixtures are the prime set's pair for this path.

```text
command line   agentsfleet fleet steer @busy-bee "What is today's word?"
dashboard      in any fleet's chat:   @busy-bee What is today's word?
                                      the browser opens busy-bee's chat
Slack          @agentsfleet-dev busy-bee: What is today's word?
```

The name is the fleet's name, the one the sidebar shows.
The command line and the dashboard look it up with `GET …/fleets?name=`, then send an ordinary message.
Slack has one bot for every fleet, so there the first word after the bot names the fleet.

In the dashboard, type `@` to see active fleets in the current workspace.
Use the arrow keys and Enter, or tap a fleet; the composer keeps the rest of the message.
Fleet conversations are stacked in the sidebar.

In an interactive named CLI conversation, the prompt shows the recipient, for example `@busy-bee> `.

```text
person, in queen-bee's chat:  "Ask busy-bee for today's word and tell me what it said."

queen-bee   fleet_activity()                        find busy-bee's id
            fleet_steer(<busy-bee id>, "what is today's word?")
            answers at once: "asked busy-bee"       it does not wait

busy-bee    runs under its own grants
            its message begins: [from fleet queen-bee, fleet_id <id>; …]
            writes its answer, then
            fleet_steer(<queen-bee id>, "today's word is …")

queen-bee   wakes for a second run:  "busy-bee said …"
```

On the receiving fleet, the incoming message shows `@queen-bee`.
Back in the sender's conversation, the report shows the other fleet's name and the sender's summary.
Expand the **Worked** row to see **Sent fleet message** or **Read fleet activity**, then the tool's recorded input and output.

The [interactive design reference](../ux.html#chief) illustrates the exchange.
A digest is the same fleet reading without steering.
`fleet_activity()` lists every fleet with its status, and one more read per fleet gives its latest result.

These rules bound it:

- A name grants nothing. A person's message passes the message route's own guard, or a channel's `mention` trigger row.
- An unknown `@name` is refused. It never falls back to the fleet whose chat was open.
- A fleet's target is another fleet in its own workspace, and active.
- Any fleet may steer any fleet in its workspace, as often as the payer's balance allows. Nothing lists targets, limits the chain or caps the count.
- A steer carries text, never access: the target runs under its own grants. A fleet with no access can still ask a fleet that holds access to act.
- A fleet's steer runs as its own run. A resent steer admits once. A runner that lost its run writes none.
- Prompt injection is accepted, and stated. A fleet that reads untrusted text can be told to steer any fleet in its workspace. Each target's own grants and the payer's balance bound the damage.

Fleet steering has no wait primitive: the receiver steers back, or the sender reads with `fleet_activity`.
Slack has no mention per fleet, and a fleet cannot reach a fleet in another workspace.

## §8.4 Working from Claude or the dashboard

The user experience inside Claude (or Amp / Codex CLI / OpenCode) feels like this:

1. The user is already in their project.
2. The user asks Claude to create or refine an operational fleet.
3. Claude drives `agentsfleet fleet create` to send the first message, then `agentsfleet fleet steer` to keep talking to the fleet.
4. For a GitHub App trigger, the user names the repositories and events, and the fleet adds the row. The workspace's existing App installation supplies the event source. For a custom webhook, `agentsfleet fleet triggers list` and the fleet page show the URL to register.
5. Claude can also manually invoke the fleet via `agentsfleet fleet steer` for one-off user-triggered tasks.
6. Later, the fleet wakes on webhook or a QStash fire without the user staying in the terminal or any `agentsfleet` cron daemon running.
7. When the user returns to Claude, they inspect what happened from durable history (`agentsfleet fleet turns {id}` or the dashboard Events tab) instead of reconstructing it from memory.

The dashboard equivalent surface on `/fleets/{id}` matches the CLI path:

- The **Wakes on** list beside the chat shows the fleet's schedules and triggers, each with remove, and a webhook's copyable URL. The dashboard never asks for or stores a user's provider PAT.
- The **Can access** list shows the fleet's approved grants, each with Remove. The **Remembers** list shows its pinned memory, each entry with remove ([`web_app.md`](./web_app.md)).
- An **access card** in the chat carries Approve and Deny. The model picker and the "Allow all" and "Packages" toggles each send one `PATCH F`.
- The **chat surface** shows trigger turns as system chips and each reply as its thinking, its tool calls and its answer. It reads the live frames while the fleet works and the turn's saved trace after a reload. A send the server refuses leaves the thread, and its text returns to the composer with **Try again**.
- The **workspace switcher** requires a name while the server owns workspace identity. The browser never repeats a failed POST on its own. It reports the failure and walks the tenant workspace endpoint's stable cursor pages to refresh the complete list.
- The CLI handles an uncertain response or registered name conflict with one encoded exact-name GET. It then selects the authoritative match without replaying the POST. A deliberate same-name retry cannot create another row because the tenant/name unique index returns a registered `409 Conflict`.
- For login tokens, both create and list resolve the database subject mapping before a stale signed tenant claim and return that tenant identifier. Tenant API keys instead stay bound to the tenant recorded on the key, even if the creator's user mapping later changes. These rules keep local state from mixing tenants without letting an API key cross its issuing boundary.
- The **authenticated frame** is shell-first ([`web_app.md`](./web_app.md) §"The five statements").

The fleet extends Claude from an interactive assistant into a durable operational worker. The dashboard mirrors the same primitives, so a user who lives in the browser sees an equivalent surface.

## §8.5 Example: Production deploy repair

The current `platform-ops` flow wakes when GitHub Actions reports a failed production deployment. The fleet reads deployment evidence and posts a diagnosis.

The approved target adds a bounded code fix and a draft Pull Request (PR). A human still reviews and merges the PR.

The existing deployment pipeline deploys the merge. A later production-health verifier stays a target, not implemented runtime behavior.


## §8.6 Why Claude is the starting point

Starting with Claude is the right constraint because it matches how technical users already work today.

They are already:

- iterating prompts
- editing project docs
- asking for automation help
- supervising tools from the terminal

The v2 product meets them there first.

Later, other entrypoints exist (the dashboard chat widget, direct API calls). But the MVP assumes:

- the user authors and supervises from Claude
- the fleet runs durably outside that transient chat session

## §8.7 Model and context-cap origin (platform vs. self-managed)

The fleet stores its model reference in `fleet.fleets.model_ref`. A create takes the body's `model_ref`, else the workspace's selected model, resolved and stored at create; with neither, the create is refused `409 UZ-SETUP-008`. Both platform-managed and self-managed runs resolve that reference at run time.

The selected row in `model.platform_models` supplies the model identity, endpoint, API format, physical context window and maximum output length.

A person changes the model with one `PATCH F {model_ref}`, which writes one `model_changed` turn. An ineligible reference is refused with the same code and changes nothing. A run paused on a question resumes under the new model.

The create flow starts with `agentsfleet doctor --json` for connectivity and workspace health (§8.2.2 step 1). Model selection uses the workspace's available-model list: owner registrations with credentials and the enabled platform default. A catalogue row without saved inference settings or a matching credential is not an eligible model.

`agentsfleet models current --json` describes the account's provider posture; it does not override a fleet's model.

```text
Fleet row: model_ref
                  |
          Resolve the exact reference
                  |
        +---------+------------------------+
        |                                  |
Owner model registration           Enabled platform default
model.tenant_models          model.platform_defaults
        |                                  |
Owner workspace vault              Admin workspace vault
        |                                  |
        +----------------+-----------------+
                         |
              Selected model.platform_models row
              model + endpoint + API format
              physical window + maximum output
                         |
                  ExecutionPolicy
                         |
              Saved model_context_tokens
              physical request limit
                         |
        +----------------+-----------------+
        |                                  |
Final output bounded by            Compaction triggers at 90 percent
remaining physical headroom        of the window, or earlier when
and saved maximum output           output headroom needs it
```

**Model identity and context limits have separate jobs.** `afd_credential`'s `Providers::resolve_reference` loads the exact owner registration or active platform default. It also loads the saved model metadata and the vault credential.

`afd_gate` carries that metadata into `ExecutionPolicy`; its context resolver always uses the selected model, with no override from the fleet. Credentials stay in the daemon and runner supervisor, outside the sandbox and browser.

A fleet carries no context cap of its own, so every run uses the saved physical window for [conversation compaction](context_lifecycle.md).

A valid checkpoint lets the same run continue with tools. Output stays bounded by the saved maximum and remaining physical headroom, planned by `af_context`.
The context defaults live in [`capabilities.md`](./capabilities.md) §4.

The run reads saved catalogue metadata instead of querying the inference provider for limits. Updating provider configuration is an explicit settings change; it does not require a deployment or a JSON routing file ([`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §10).

**Dashboard equivalent: the Models page (`/w/{workspace_id}/settings/models`).** A browser user can use the platform default or connect a provider through the owner registry.

The fleet page's model picker lists eligible configured models, and a change sends one `PATCH F`. The platform administrator manages the shared catalogue and default; users manage their own registrations and credentials.

Saved credentials are never returned to the browser. The former `/credentials` page stays removed; custom secrets live on `/secrets`.

## §8.8 Slack mentions and configured fleets

A workspace administrator connects Slack. A fleet subscribes to a channel when a person asks, by adding a `mention` trigger row with `trigger_add`.

Only an addressed fleet or the sole subscriber receives a mention turn.
An unsubscribed channel returns `channel_not_configured` without a model call or fleet creation.

[Slack mention routing](connectors.md#routing-a-mention) owns name matching, notice-only channel bindings and current workspace checks.
The fleet's approved grants and network toggles determine its permitted work.
Connecting Slack creates no fallback fleet and no fallback model route.
