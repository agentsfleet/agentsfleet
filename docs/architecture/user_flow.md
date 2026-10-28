# User Flow — how a user uses the system

> Parent: [`README.md`](./README.md) · User-facing: [docs.agentsfleet.net/quickstart](https://docs.agentsfleet.net/quickstart) walks the same path in five minutes; [docs.agentsfleet.net/fleets/overview](https://docs.agentsfleet.net/fleets/overview) is its reference.

Read this when you want to know how a real human gets from "I want a Fleet" to "the Fleet is running on my repo." The §-numbered subsections are stable anchors that other specs reference; do not rename them without sweeping cross-references.

The initial user assumption is simple:

- the user is already working inside Claude (or Amp, Codex command-line interface (CLI), OpenCode), or in the `agentsfleet` dashboard
- the user is already working on their own project or infrastructure
- the user wants operational work to continue without babysitting an endless terminal loop

Workspace chat, in the browser or the terminal, is where the user describes, reviews, saves, improves, and supervises Fleets. The Fleet runtime is where long-lived operational outcomes continue after the chat ends.

For the full end-to-end setup + first-trigger walkthroughs (platform-managed and self-managed), see [`scenarios/`](./scenarios/).

## Facts

Every row is extracted from the §-numbered sections below; the owner column names the section that carries the full story.

| Invariant | Value | Mechanism | Owner section |
|---|---|---|---|
| The wedge surface | `agentsfleet` CLI and dashboard, both over workspace chat | `curl -fsSL https://agentsfleet.dev \| bash` → `login` → `chat` | §8.0 |
| List output shape | name, identifier, domain columns, `AGO` | one helper owns the order for every entity list the CLI prints, and appends the age itself, so a call site has neither a position to choose nor a column to forget; a catalogue with no age of its own declares that rather than rendering an empty column | §8.4 |
| Save input | a reviewed draft candidate or a pinned Library revision | C1 `POST W/fleets` names one source and the review digest; changes later arrive as reviewed setup revisions (N11) | §8.2.2 |
| Deployment posture | hosted-only in v2 | no `--self-host` flag, no self-host runbook; validation on non-Fly hosts is v3 | §8.0.1 |
| Fleet definition | one typed JSON Definition | a `description`, `instructions_md`, and the `config` policy document; one parser validates every source | §8.1 |
| Preflight | `agentsfleet doctor --json`, health only | `server_reachable` · `workspace_selected` · `workspace_binding_valid`; the only sanctioned preflight surface | §8.2.2 |
| Activation guarantee | readiness checks and committed activation before `active` | run-now activation commits the initial admission in the same transaction | §8.2.2 |
| Trigger declarations | 0–8 entries, unique on `(type, source)`; none is a fleet woken only by messages | GitHub App triggers require explicit `repositories` (fail-closed); at most one declarative cron | §8.3 |
| Cron authority | QStash (Upstash's hosted scheduler) owns the clock | Save stores the schedule intent in the same transaction; `afd_cron` mirrors it to QStash; the runner owns no timer | §8.3 |
| Steer | always accepted | same single ingress as webhook and cron: a committed `fleet.admissions` row with `actor=steer:<user>` | §8.3 |
| Model identity | selected at lease time | the revision's `runtime_model_ref` resolves to a credentialed saved model; authored model text cannot replace it | §8.7 |
| Context limits | saved physical window and separate operating cap | each lease reads the saved model window; an absent or zero operating cap inherits it, without a provider lookup | §8.7 |
| Conversation compaction | Defaults to 90 percent of the selected physical window | An authored operating threshold or output headroom can trigger earlier; valid checkpoints let the same run continue with tools. | §8.7 |
| Slack surface | Rung 0, reactive only | per-channel resident fleet is the memory namespace; answers on `@mention`, never unattended; shipped by M206_002 | §8.8 |

## Traps

Each trap is enforced in its owner section; this list is the index.

- The §-numbered subsections are stable anchors — never rename them without sweeping cross-references (preamble).
- Doctor carries no provider or billing posture; read `tenant provider show` and `billing show` separately once health passes (§8.2.2).
- Omitting `repositories` on a GitHub App trigger is fail-closed — no delivery, never every repository (§8.3).
- The fleet's in-run loop never branches on actor; its instructions dictate behaviour (§8.3).
- Never resolve context caps at trigger time (§8.7).
- The dashboard never asks for or stores a user's provider Personal Access Token (§8.2.2, §8.4).
- The browser never automatically repeats a failed workspace POST (§8.4).
- Readiness never blocks Save; a fleet short of a credential, grant, or funds waits in `installing` (§8.2.2).
- Instructions are prose and grant nothing; capability comes only from the reviewed `config` and approved grants (§8.1).
- The Rung 0 Slack bot never acts unattended; unattended action is Rung 1, a separate spec (§8.8).

## Topology

The dual-posture model/cap resolution diagram lives in §8.7 — it is that section's proof and stays there.

## Decisions

| Decision | Reason | Where / artifact |
|---|---|---|
| Hosted-only v2 | launch claim is "open-source software (OSS) + self-managed + chat-authored", not "self-hostable"; integration burden earns v3 | §8.0.1 |
| Browser and terminal share one draft API | one validator, one review digest, one Save; neither client has a private path | §8.1 |
| Private drafts are the authoring surface | Native Definition JSON can initialize a draft; retired bundle and GitHub archive imports remain unsupported | §8.1 |
| Zero or absent operating cap inherits the saved window | model identity stays bound to the reviewed selection while setup can select an earlier compaction threshold | §8.7 |
| `/credentials` page removed outright, not redirected | provider keys live on `/settings/models`; custom secrets on `/secrets` | §8.7 |

---

## Detail

Everything below is the full reference. Headings are stable — specs cite them by §-number; insert new sections, never rename or renumber existing ones.

## §8.0 The wedge surface

The minimum viable product (MVP)'s user-facing wedge is the **`agentsfleet` Command-Line Interface (CLI) and dashboard, both over workspace chat**. A user goes from cold machine to a running fleet by describing the job, reviewing the setup, and saving it:

```bash
curl -fsSL https://agentsfleet.dev | bash   # installs the agentsfleet CLI
agentsfleet login                            # Clerk OAuth
agentsfleet chat --model <MODEL_REF>          # describe the job, review it, then Save
agentsfleet library                          # list reusable Library entries (GET /v1/workspaces/{ws}/fleet-libraries)
```

Choose `<MODEL_REF>` from the `ref` values returned by `GET /v1/workspaces/{ws}/models?purpose=authoring`. New drafts require this authoring model; resumed drafts retain their existing model. A draft starts from four sources (`cli/src/commands/chat/commands.ts`):

- **Nothing.** `agentsfleet chat --model <MODEL_REF>` creates a private draft when the first message is sent.
- **A Library revision.** `agentsfleet chat --library <LIBRARY_ID> --revision <REVISION_ID> --model <MODEL_REF>` starts from one immutable Library revision.
- **A saved fleet.** `agentsfleet chat --fleet <FLEET_ID> --setup-revision <N> --model <MODEL_REF>` drafts an improvement to that exact setup revision.
- **Native JSON.** `agentsfleet chat --definition <PATH> --model <MODEL_REF>` reads one native Definition from a local file, bounded at 448 KiB.

Library IDs and revision IDs come from Library reads; fleet IDs and setup revisions come from fleet reads. `<PATH>` names your native JSON file.

`--review` shows the full setup, access, readiness, and review digest; `--save` saves the reviewed setup as an independent fleet, and `--publish` creates a Library entry from it. The dashboard does the same through the same routes ([`api_resources.md`](./api_resources.md) N1–N17, C1–C7).

Configuration such as a Slack channel, a production-branch glob, or a cron schedule lives in the reviewed setup. The conversation changes it; the review shows exactly what Save will store.

## §8.0.1 Deployment posture: hosted-only in v2

v2 ships **hosted-only** on `api.agentsfleet.net`. The skill detects no choice point: it defaults to the hosted endpoint, prompts Clerk OAuth via `agentsfleet login` if the CLI is not authenticated, and proceeds. There is no self-host runbook in v2 and no `--self-host` flag.

This is a deliberate scope cut, not a gap in the architecture. The runtime is already structured so the auth substrate (Clerk OAuth), key management service (KMS) adapter (cloud KMS), and process orchestration (Fly.io machines) are the only deployment-specific layers — the worker, runner, sandbox, event stream, and reasoning loop are all posture-agnostic. **Validating** that on a clean non-Fly Linux host (Clerk shim or local-token auth, a portable KMS adapter, the runner's Landlock+cgroups+bwrap on a vanilla VM, systemd orchestration) is a v3 workstream once v2 has earned the trust to justify the integration burden.

Practically, this means:

- v2 launch claim is **OSS + self-managed + chat-authored**. Not "self-hostable."
- The `/self-host` runbook page does not exist on `docs.agentsfleet.net` for v2.
- Users who need self-host today are out of scope; the AI-infra / GPU-cloud / regulated mid-market P1 personas are v3 customers, not v2.
- self-managed still ships in v2 — it sits on top of the hosted posture and removes the inference-cost lock-in independently of where the runtime runs. See [`capabilities.md`](./capabilities.md) and [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §1.

## §8.1 Authoring the Fleet

The user describes the Fleet in a private draft. Browser and terminal chat share the draft API, N1–N10 in [`api_resources.md`](./api_resources.md). The draft is visible only to its creator.

The conversation produces one typed JSON Definition (`rustd/crates/afd_fleet_runtime/src/definition/mod.rs`):

- `description` is required display text saying what the Fleet does; line breaks are allowed and line endings are normalized. The Fleet's name is `config.name`. Any top-level key besides `schema_version`, `description`, `instructions_md` and `config` is refused.
- `instructions_md` describes how the Fleet should think, what its job is, what "good" looks like, what evidence to gather, and what actions require caution. Plain English. The run receives it as its instructions.
- `config` is the policy document. Under `x-agentsfleet` it declares `triggers`, `tools`, `credentials`, `network`, `budget` (`daily_nanos` and `monthly_nanos`, decimal strings), `gates`, `repositories`, `context`, and `model`.

The user iterates in natural language:

- "tighten the deploy-failure diagnosis prompt"
- "add a periodic health check every 15 minutes"
- "require approval before teardown"
- "include Fly logs and Dragonfly health in the first pass"

This keeps the operational logic editable by changing instructions, not by rewriting a typed workflow engine for every variation.

Instructions grant no access. Runs use only reviewed `config` and approved grants ([capabilities](capabilities.md#1-reasoning--tool-inventory-declared-in-the-fleets-own-files)).

Native JSON may initialize a private draft. GitHub/template-bundle import and Markdown export remain unsupported. Library publication explicitly selects a draft candidate.

## §8.2 Creating the Fleet

Once the candidate is reviewed, the user saves the Fleet in the workspace.

### §8.2.1 Cold-machine bootstrap (run once per machine)

The canonical entry is the one-liner served from `https://agentsfleet.dev` — it installs the `agentsfleet` CLI:

```bash
curl -fsSL https://agentsfleet.dev | bash   # installs the agentsfleet CLI (npm under the hood)
```

Or run the chain explicitly (skip any step already in place):

```bash
npm install -g @agentsfleet/cli   # CLI binary
agentsfleet login                  # Clerk OAuth → credential in ~/.config/agentsfleet/credentials.json
agentsfleet whoami                 # who that credential belongs to
agentsfleet connector show github --json
```

Login proves the new credential against `GET /v1/users/me`, which needs no capability scope ([API reference](https://docs.agentsfleet.net/api-reference/introduction) › Users).

`agentsfleet doctor --json` is the readiness gate (§8.2.2 step 2): on any miss it prints the explicit fix commands and stops. The commands are deliberately separate so a user with most of the chain already in place skips what they already have.

### §8.2.2 Per-Fleet create flow

1. The user drafts a setup in chat (§8.1), optionally starting from a pinned Library revision.
2. **`agentsfleet doctor --json` runs first** as the deterministic readiness gate after login. Doctor is fast and verifies connectivity + workspace health only — `server_reachable`, `workspace_selected`, and `workspace_binding_valid`. It does **not** carry provider posture or balance; those live in `agentsfleet tenant provider show --json` (mode/provider/model/context cap) and `agentsfleet billing show` (balance and exhaustion state), read separately once health passes. The CLI (and any caller) reads `doctor`'s JSON output verbatim and aborts on failure with the user-facing message. Doctor is the only sanctioned preflight surface for health; no parallel `preflight` command exists.
3. **Review.** `GET` on the draft returns the effective setup, the access it needs, live readiness, and a `review_digest` (N4). The user picks the instance name and runtime model on the draft before reviewing.
4. **Save.** `POST /v1/workspaces/{ws}/fleets` (C1) names the reviewed draft candidate, or a pinned Library revision, plus the review digest. Save writes the fleet row, its first immutable revision in `fleet.revisions`, and any inherited schedule in one transaction (`rustd/crates/afd_authoring/src/creation/reserve.rs`). `fleet.fleets.head_revision_id` selects that revision, and ingress reads the setup from it; the fleet row keeps no config copy.
5. **Readiness.** The saved fleet starts in `installing` with one check per need: definition, model, wallet, credential, connection, and grant (`rustd/crates/afd_authoring/src/types/review/readiness.rs`). A missing credential or unapproved grant is a pending check, never a refused Save.

   Repeating Save with the same idempotency key retries the same fleet. The fleet turns `active` once every check passes and activation commits; run-now activation commits its initial admission in the same transaction (`rustd/crates/afd_authoring/src/creation/activate.rs`). See [`data_flow.md`](./data_flow.md#fleet-creation-and-setup-recovery).
6. **Provider wiring follows the trigger surface.** A GitHub App trigger needs a one-time workspace connection, and a custom webhook keeps its per-fleet URL, which the saved fleet's response lists in `webhook_urls`. The platform holds its own App secrets, never a user's Personal Access Token (PAT) ([`connectors.md`](./connectors.md) §"GitHub App: platform setup to fleet execution").
7. Future triggers are served with no restart and no watcher thread. Each trigger commits a row to the admission ledger (`fleet.admissions`). The control plane assigns that event to an eligible `agentsfleet-runner` through the ledger claim (`POST /v1/runners/me/leases`).

   [`data_flow.md`](./data_flow.md) §"Steer flow end-to-end" carries the full path.
8. **Improve.** A change to a saved fleet is a new draft from `--fleet <id> --setup-revision <n>`, reviewed, then applied as the next immutable revision (N11). Saving never publishes, and publishing never changes a saved fleet.

After Save, the Fleet is no longer tied to the conversation that created it.

### §8.2.3 Library reuse

The **Library** holds reusable setup. An entry is created only when a person explicitly publishes a chat-created candidate (C6 `POST W/fleet-libraries`), and every publication is an immutable revision in `authoring.library_revisions`. Saving from a Library revision pins that exact revision; a later revision never changes a fleet already saved from an earlier one.

Two first scenarios anchor the product flow:

- **GitHub Pull Request reviewer.** Wakes on GitHub pull request events and reads diff/context through `api.github.com`. Posting review comments remains blocked by repository egress policy; see the [reviewer scenario](scenarios/github-pr-reviewer.md).
- **Zoho Recruit outreach.** Reads Zoho Recruit data and optionally sends mail through a separate secret; its provider-specific guidance lives in its instructions.

## §8.3 Triggering the Fleet

A Fleet's setup declares `triggers: [...]` under `config.x-agentsfleet`: an array of up to 8 trigger entries, unique on the `(type, source)` tuple. It may be empty or omitted: a fleet with no trigger is woken only by the messages sent to it. Each entry is one of:

- **GitHub App trigger.** Type `webhook`, `source: github`, explicit `repositories` and `events`; omitting `repositories` is fail-closed for App traffic ([`connectors.md`](./connectors.md) §"GitHub App: platform setup to fleet execution").
- **Manual/custom webhook trigger.** Type `webhook` with a lowercase-slug `source` (never `approval`, whose segment is the approval route). Registry sources `github`, `linear` and `slack` use their fixed signing recipes and refuse overrides.

  Other sources declare `verifier.scheme` using one of the five [webhook recipes](fleet_definitions.md#webhook-declarations). Each trigger's `credential_name` selects a workspace JSON credential with a `webhook_secret` string; the name defaults to its source.

  Save returns `POST /v1/webhooks/{fleet_id}/{source}` for every webhook trigger, including Svix. The operator registers the URL and signing secret with the provider. This path does not infer a fleet from an App installation and does not require `repositories`.
- **Cron trigger.** Type `cron`, `schedule` as a 5-field cron expression, plus `timezone` and `message`. Save copies it into the fleet's inherited schedule in the same transaction (`rustd/crates/afd_authoring/src/creation/schedules.rs`), and `afd_cron` mirrors that intent to QStash under a stable identifier. QStash owns the clock and sends each signed fire to `agentsfleetd`, which admits one event through the admission ledger with `actor=cron:<schedule_id>` (`rustd/crates/afd_cron/src/fire/mod.rs`). The runner owns no timer. A setup allows at most one cron trigger; the schedule API can manage additional explicit schedules within the per-Fleet limit.

In addition to the declared triggers, every Fleet always accepts:

- **User steer.** The user asks Claude to run an operational task. Claude invokes `agentsfleet steer {id} "<message>"` or uses the dashboard's composer on `/fleets/{id}`, which POSTs to `/v1/workspaces/{ws}/fleets/{id}/messages`.

  `afd_events::Steer::append` commits a `fleet.admissions` row with `actor=steer:<user>` and sends a best-effort wake notification. Webhook and cron use the same admission ledger.

  The dashboard names every send with an `operation_id`, a version 7 universally unique identifier (UUID v7). It keeps unresolved sends in a per-user, per-fleet ledger mirrored to `localStorage` until the 202 response arrives. Retry, Resend, and Resend after reload all carry the same identifier.

  Resending with that identifier returns the first event ([API reference](https://docs.agentsfleet.net/api-reference/introduction) › Fleet messages). A send unanswered after 30 seconds reads "Couldn't confirm" with Resend.

  Another tab's send stays hidden while that tab holds its Web Lock, then shows Resend once the tab is gone. Every tab keeps a dismissal tombstone for one day, preventing late writes from restoring a dismissed send. A final `UZ-AGT-016` refusal offers Send as new under a fresh identifier, without Resend.

  A replayed answer for an event the page never loaded reads that event's detail and settles its row. An event already loaded settles from its own frames. A running event unheard from for 45 seconds is read once, so a lost completion still ends its row.

All actors flow through the same runtime path. The Fleet's in-run fleet loop does not branch on actor type — the same `http_request`-driven evidence gathering and Slack post happen regardless of how the work was triggered. A "morning health check" steer produces a real first-pass evidence sweep; the fleet's instructions dictate behaviour, not the actor field.

A fleet that declares no trigger is valid: steers reach every fleet, whatever it declares.

## §8.4 Working from Claude or the dashboard

The user experience inside Claude (or Amp / Codex CLI / OpenCode) feels like this:

1. The user is already in their project.
2. The user asks Claude to create or refine an operational fleet.
3. Claude drives `agentsfleet chat`: it sends the description, reads the review, and saves only after the user agrees.
4. For GitHub App triggers, the setup carries explicit `repositories` and `events`; the workspace's existing App installation supplies the event source. For a custom webhook, the saved fleet's response carries `webhook_urls` for operator registration.
5. Claude can also manually invoke the fleet via `agentsfleet steer` for one-off user-triggered tasks.
6. Later, the fleet wakes on webhook or a QStash fire without the user staying in the terminal or any `agentsfleet` cron daemon running.
7. When the user returns to Claude, they inspect what happened from durable history (`agentsfleet events {id}` or the dashboard Events tab) instead of reconstructing it from memory.

The dashboard equivalent surface on `/fleets/{id}` matches the CLI path:

- The **Trigger panel** renders one card per declared trigger. A GitHub App card shows connector state plus repository/event subscriptions and routes a disconnected workspace to Connect GitHub. Custom providers retain registration guidance and copyable per-fleet URLs. The dashboard never asks for or stores a user's provider PAT.
- The **chat surface** shows trigger events as system chips and each reply as its thinking, its tool calls and its answer, read from the live frames while the fleet works and from the event's saved trace after a reload. A send the server refuses leaves the thread, and its text returns to the composer with Resend.
- The **workspace switcher** requires a name while the server owns workspace identity. The browser never automatically repeats a failed POST. It reports the failure and walks the tenant workspace endpoint's stable cursor pages to refresh the complete list. The CLI handles an uncertain response or registered name conflict with one encoded exact-name GET, then selects the authoritative match without replaying the POST. For login tokens, both create and list resolve the database subject mapping before a stale signed tenant claim and return that tenant identifier. Tenant API keys instead remain bound to the tenant recorded on the key, even if the creator's user mapping later changes. These rules prevent local state from mixing tenants without letting an API key cross its issuing boundary. A deliberate same-name retry cannot create another row because the tenant/name unique index returns a registered `409 Conflict`.
- The **authenticated frame** is shell-first ([`web_app.md`](./web_app.md) §"The five statements").

This matters because the fleet is not replacing Claude. It extends Claude from an interactive assistant into a durable operational worker — and the dashboard mirrors the same primitives so a user who lives in the browser sees an equivalent surface.

## §8.5 Example: Production deploy repair

The current `platform-ops` flow wakes when GitHub Actions reports a failed production deployment. The fleet reads deployment evidence and posts a diagnosis.

The approved target adds a bounded code fix and a draft Pull Request (PR). A human still reviews and merges the PR.

The existing deployment pipeline deploys the merge. A later production-health verifier remains a target, not implemented runtime behavior; see the [repair scenario](scenarios/production-deploy-repair.md).

The full sequence and its proof boundary live in [`scenarios/production-deploy-repair.md`](./scenarios/production-deploy-repair.md).

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
- the fleet executes durably outside that transient chat session

## §8.7 Model and context-cap origin (platform vs. self-managed)

The draft selects a credentialed model, and Save pins its reference on the immutable revision as `runtime_model_ref`. Both platform-managed and self-managed execution resolve that reference at lease time. The selected row in `model.platform_models` supplies the model identity, endpoint, API format, physical context window and maximum output length. Authored `config` model text cannot replace that selection.

The setup flow starts with `agentsfleet doctor --json` for connectivity and workspace health (§8.2.2 step 2). Model selection uses the workspace's available-model list: owner registrations with credentials and the enabled platform default. A catalogue row without saved inference settings or a matching credential is not an eligible model. `agentsfleet tenant provider show --json` describes the account's provider posture; it does not override a revision's selected model.

The diagram's wrap-up branch records earlier behavior. Current behavior uses shared compaction at 90 percent by default, as described below.

```text
Reviewed revision: runtime_model_ref
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
        +----------------+-----------------+
        |                                  |
Saved model_context_tokens         context.context_cap_tokens
Physical request limit             Authored nonzero operating cap,
                                   otherwise saved physical window
        |                                  |
Final output bounded by            Wrap-up threshold defaults to
remaining physical headroom        0.75 x operating cap
and saved maximum output
```

**Model identity and context limits have separate jobs.** `afd_credential::Providers::resolve_reference` loads the exact owner registration or active platform default, its saved model metadata, and its vault credential. `afd_gate` carries that metadata into `ExecutionPolicy`; its context resolver always uses the selected model (`rustd/crates/afd_credential/src/provider/{runtime/resolve,registered/sql}.rs`, `rustd/crates/afd_gate/src/policy/{build/mod,context}.rs`). Credentials remain in the daemon and runner supervisor, outside the sandbox and browser.

An absent or zero `context_cap_tokens` inherits the saved physical window. A nonzero authored cap can trigger earlier [conversation compaction](context_lifecycle.md); it leaves `ExecutionPolicy.model_context_tokens` unchanged.

A valid checkpoint lets the same run continue with tools. Output remains bounded by the saved maximum and remaining physical headroom (`rustd/crates/afd_context/src/window.rs`).
The other `x-agentsfleet.context.*` settings retain their authored values or use the defaults in [`capabilities.md`](./capabilities.md) §4.

The lease reads saved catalogue metadata instead of querying the inference provider for limits. Updating provider configuration is an explicit settings change; it does not require a deployment or a JSON routing file ([`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §10).

**Dashboard equivalent — the Models page (`/w/{workspace_id}/settings/models`).** A browser user can use the platform default or connect a provider through the owner registry. The draft's model picker lists eligible configured models and selects a default. The platform administrator manages the shared catalogue and default; users manage their own registrations and credentials. Saved credentials are never returned to the browser. The former `/credentials` page remains removed; custom secrets live on `/secrets`.

## §8.8 Slack as a resident surface (Rung 0) — M106

> **Status.** Shipped in `agentsfleetd` by M206_002, which also adds fleets that subscribe to one channel by its ID; [`scenarios/slack-incident-responder.md`](./scenarios/slack-incident-responder.md) walks the zero-, one- and several-fleet cases.

After a workspace admin connects Slack once, an `@mention` lands as one `slack_mention` admission and routes to a fleet subscribed to that channel, or else to the channel's resident fleet ([`scenarios/slack-incident-responder.md`](./scenarios/slack-incident-responder.md) §"4. Routing a mention"; channel memory: [`memory.md`](./memory.md) §"4. The M106 channel pattern").
The bot answers on mention and never acts unattended; acting unattended with approval is Rung 1 ([`high_level.md`](./high_level.md) §"5.2 Slack-resident channel bot — the on-ramp (M106)").
