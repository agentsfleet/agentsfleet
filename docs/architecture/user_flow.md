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
| Activation guarantee | stream + consumer group before `active` | Save records the stream as ready, and activation refuses a fleet without it | §8.2.2 |
| Trigger declarations | 0–8 entries, unique on `(type, source)`; none is a fleet woken only by messages | GitHub App triggers require explicit `repositories` (fail-closed); at most one declarative cron | §8.3 |
| Cron authority | QStash (Upstash's hosted scheduler) owns the clock | Save stores the schedule intent in the same transaction; `afd_cron` mirrors it to QStash; the runner owns no timer | §8.3 |
| Steer | always accepted | same single ingress as webhook and cron: a `fleet.admissions` row, then an `XADD` with `actor=steer:<user>` | §8.3 |
| Model/cap overlay | per-field, at lease time | `config` `model: ""` / `context_cap_tokens: 0` / absent ⇒ overlay from the provider the revision's `runtime_model_ref` resolves to | §8.7 |
| Cap resolution time | provider-set time | the lease reads the stored cap; it never calls out at trigger time | §8.7 |
| Run-chunk threshold | 0.75 × `context_cap_tokens` | L3 chunking; continuation resumes in a fresh window | §8.7 |
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
| Chat is the only authoring input | file, GitHub, and template import and Markdown export were removed by owner decision ([`decisions.md`](../reviews/workspace-setup/decisions.md) §"Chat-only authoring amendment") | §8.1 |
| Visible sentinels (`""`, `0`) in the setup's `config` | a reviewer can spot "inherits from the selected model" at a glance; an absent key is the safety net | §8.7 |
| `/credentials` page removed outright, not redirected | provider keys live on `/settings/models`; custom secrets on `/secrets` | §8.7 |

---

## Detail

Everything below is the full reference. Headings are stable — specs cite them by §-number; insert new sections, never rename or renumber existing ones.

## §8.0 The wedge surface

The minimum viable product (MVP)'s user-facing wedge is the **`agentsfleet` Command-Line Interface (CLI) and dashboard, both over workspace chat**. A user goes from cold machine to a running fleet by describing the job, reviewing the setup, and saving it:

```bash
curl -fsSL https://agentsfleet.dev | bash   # installs the agentsfleet CLI
agentsfleet login                            # Clerk OAuth
agentsfleet chat                             # describe the job in a private draft, review it, then Save
agentsfleet library                          # list reusable Library entries (GET /v1/workspaces/{ws}/fleet-libraries)
```

A draft starts from one of three sources (`cli/src/commands/chat/commands.ts`):

- **Nothing.** `agentsfleet chat` opens a private draft; the conversation produces the setup.
- **A Library revision.** `agentsfleet chat --library <id> --revision <revision_id>` starts from one immutable Library revision.
- **A saved fleet.** `agentsfleet chat --fleet <id> --setup-revision <n>` drafts an improvement to that exact setup revision.

`--review` shows the full setup, access, readiness, and review digest; `--save` saves the reviewed setup as an independent fleet, and `--publish` creates a Library entry from it. The dashboard does the same through the same routes ([`api_resources.md`](./api_resources.md) N1–N17, C1–C7).

Configuration such as a Slack channel, a production-branch glob, or a cron schedule lives in the reviewed setup. The conversation changes it; the review shows exactly what Save will store.

## §8.0.1 Deployment posture## §8.0.1 Deployment posture: hosted-only in v2

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

- `description` is one required line of display text saying what the Fleet does. The Fleet's name is `config.name`. Any top-level key besides `schema_version`, `description`, `instructions_md` and `config` is refused.
- `instructions_md` describes how the Fleet should think, what its job is, what "good" looks like, what evidence to gather, and what actions require caution. Plain English. The run receives it as its instructions.
- `config` is the policy document. Under `x-agentsfleet` it declares `triggers`, `tools`, `credentials`, `network`, `budget` (`daily_nanos` and `monthly_nanos`, decimal strings), `gates`, `repositories`, `context`, and `model`.

The user iterates in natural language:

- "tighten the deploy-failure diagnosis prompt"
- "add a periodic health check every 15 minutes"
- "require approval before teardown"
- "include Fly logs and Dragonfly health in the first pass"

This keeps the operational logic editable by changing instructions, not by rewriting a typed workflow engine for every variation.

Instructions are prose and grant nothing. What a run may do comes only from `config` and the grants a person approved ([`capabilities.md`](./capabilities.md) §1). Chat is the only authoring input: there is no file, GitHub, or template import, and no Markdown export. A Library entry is created only by explicitly publishing a chat-created candidate (C6).

## §8.2 Creating the Fleet

Once the files are ready, the user creates the Fleet in the workspace.

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
5. **Readiness.** The saved fleet starts in `installing` with one check per need: definition, model, wallet, credential, connection, and grant (`rustd/crates/afd_authoring/src/types/review/readiness.rs`). A missing credential or unapproved grant is a pending check, never a refused Save. Repeating Save with the same idempotency key retries the same fleet. The fleet turns `active` once every check passes and its events stream and consumer group exist. See [`data_flow.md`](./data_flow.md#fleet-creation-and-setup-recovery).
6. **Provider wiring follows the trigger surface.** A GitHub App trigger needs a one-time workspace connection, and a custom webhook keeps its per-fleet URL, which the saved fleet's response lists in `webhook_urls`. The platform holds its own App secrets, never a user's Personal Access Token (PAT) ([`connectors.md`](./connectors.md) §"GitHub App: platform setup to fleet execution").
7. Future triggers are served with no restart and no watcher thread. Each trigger is first recorded as a row in the admission ledger (`fleet.admissions`), then `XADD`ed to the canonical stream name `fleet:{id}:events`, and the control plane hands that event to whichever `agentsfleet-runner` leases next (`POST /v1/runners/me/leases`). [`data_flow.md`](./data_flow.md) §"Steer flow end-to-end" carries the full path.
8. **Improve.** A change to a saved fleet is a new draft from `--fleet <id> --setup-revision <n>`, reviewed, then applied as the next immutable revision (N11). Saving never publishes, and publishing never changes a saved fleet.

After Save, the Fleet is no longer tied to the conversation that created it.

### §8.2.3 Library reuse

The **Library** holds reusable setup. An entry is created only when a person explicitly publishes a chat-created candidate (C6 `POST W/fleet-libraries`), and every publication is an immutable revision in `authoring.library_revisions`. Saving from a Library revision pins that exact revision; a later revision never changes a fleet already saved from an earlier one.

Two first scenarios anchor the product flow:

- **GitHub Pull Request reviewer.** Wakes on GitHub pull request events, reads diff/context through `api.github.com`, and posts review comments using the workspace `github` secret.
- **Zoho Recruit outreach.** Reads Zoho Recruit data and optionally sends mail through a separate secret; its provider-specific guidance lives in its instructions.

## §8.3 Triggering the Fleet

A Fleet's setup declares `triggers: [...]` under `config.x-agentsfleet`: an array of up to 8 trigger entries, unique on the `(type, source)` tuple. It may be empty or omitted: a fleet with no trigger is woken only by the messages sent to it. Each entry is one of:

- **GitHub App trigger.** Type `webhook`, `source: github`, explicit `repositories` and `events`; omitting `repositories` is fail-closed for App traffic ([`connectors.md`](./connectors.md) §"GitHub App: platform setup to fleet execution").
- **Manual/custom webhook trigger.** Type `webhook` with a lowercase-slug `source` (never `approval`, whose segment is the approval route). The source must be one a route can verify: `github`, `linear` or `slack`, whose signature schemes the ingress ships, or any other slug that declares a Svix `signature: {secret_ref}`. Save refuses anything else. Save returns one URL per webhook trigger: `POST /v1/webhooks/{fleet_id}/{source}` for a shipped scheme (GitHub's own route for `source: github`), and `POST /v1/webhooks/svix/{fleet_id}` for a Svix-signed source. The operator registers those URLs and a workspace webhook secret with the provider. This path does not infer a fleet from an App installation and does not require `repositories`.
- **Cron trigger.** Type `cron`, `schedule` as a 5-field cron expression, plus `timezone` and `message`. Save copies it into the fleet's inherited schedule in the same transaction (`rustd/crates/afd_authoring/src/creation/schedules.rs`), and `afd_cron` mirrors that intent to QStash under a stable identifier. QStash owns the clock and sends each signed fire to `agentsfleetd`, which admits one event through the admission ledger with `actor=cron:<schedule_id>` (`rustd/crates/afd_cron/src/fire/mod.rs`). The runner owns no timer. A setup allows at most one cron trigger; the schedule API can manage additional explicit schedules within the per-Fleet limit.

In addition to the declared triggers, every Fleet always accepts:

- **User steer.** The user, while in Claude, asks to run an operational task. Claude invokes `agentsfleet steer {id} "<message>"` or types into the dashboard's chat composer on `/fleets/{id}`, which POSTs to `/v1/workspaces/{ws}/fleets/{id}/messages`. `afd_events::Steer::append` admits it as a `fleet.admissions` row and then `XADD`s it to `fleet:{id}:events` with `actor=steer:<user>`: the same admission ledger webhook and cron use. The dashboard names every send with an `operation_id` (a version 7 universally unique identifier, UUID v7) and keeps it in a per-user, per-fleet ledger of unresolved sends, mirrored to `localStorage`, until the 202 arrives; a retry, a Resend, or a Resend after reload carries the same id. A resend under the same id returns the first event ([API reference](https://docs.agentsfleet.net/api-reference/introduction) › Fleet messages). Every send ends visibly. One unanswered after 30 seconds reads "Couldn't confirm" with Resend. Another tab's send stays hidden while that tab holds its Web Lock, and shows Resend once the tab is gone. A dismissal is a tombstone every tab keeps for a day, so no late write brings the send back. A `UZ-AGT-016` refusal is final: its notice offers Send as new, under a fresh id, and never Resend. A replayed answer whose event the page never loaded reads the event's detail and settles its row; one the page already holds settles from its own frames. A running event the page has not heard from in 45 seconds is read once, so a lost completion still ends its row.

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

The existing deployment pipeline deploys the merge. A later run checks production health and records the result.

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

Two things travel together: the **model** the runner's fleet invokes, and the **`context_cap_tokens`** L3 run chunking uses. They originate from different places under platform-managed and self-managed postures, and the control plane's overlay logic is what reconciles them at lease time.

The setup flow is the same shape in both postures: **run `agentsfleet doctor --json` for connectivity + workspace health, then read the active provider posture from `agentsfleet tenant provider show --json`. The draft selects a runtime model, and Save pins it on the revision as `runtime_model_ref`; the setup's `config` carries resolved-or-sentinel model/cap values.** Doctor's checks and what it leaves out are in §8.2.2 step 2. If a health check fails (or the CLI is not authenticated) the CLI prints the `agentsfleet login` hint and stops; `tenant provider show` is only meaningful once health passes. The CLI never reads the model library directly — `tenant provider show` always carries resolved values (synth-default for tenants with no row, real values for tenants with an explicit row).

```
                     PLATFORM-MANAGED (John Doe)                self-managed (John Doe, post-flip)
                  ─────────────────────────────────       ─────────────────────────────────
setup flow     →   doctor --json (health)                  doctor --json (health)
                    server_reachable: true  ✓              server_reachable: true  ✓
                    workspace_selected: true ✓             workspace_selected: true ✓
                    workspace_binding_valid: ✓             workspace_binding_valid: ✓
                  ─ if any health check fails: print      ─ same health-fail short-circuit ─
                    `agentsfleet login` and STOP. ─
                  tenant provider show --json:                tenant provider show --json:
                    {mode=platform,                           {mode=self_managed,
                     model=accounts/fireworks/models/kimi-k3, provider=fireworks,
                     context_cap_tokens=1048576}              model=accounts/.../kimi-k3,
                  (billing show → balance state)                context_cap_tokens=1048576}
                  branch on mode → write config               branch on mode → write config
                  pin into config (resolved):                 pin into config (sentinels):
                    model: accounts/fireworks/models/kimi-k3  model: ""
                    context_cap_tokens: 1048576               context_cap_tokens: 0

tenant provider → (nothing — synth-default                → agentsfleet tenant provider create
                   stays in place)                            --secret account-fireworks-key
                                                              → API loads vault row
                                                              → API reads model.platform_models
                                                              → upsert model.tenant_selections row
                                                                {mode=self_managed, provider, model,
                                                                 context_cap_tokens, secret_ref}

trigger fires  → lease resolve:                            → lease resolve:
                   Providers::resolve()                       Providers::resolve()
                     no row → synth-default                    follows secret_ref to vault
                   config has resolved cap →                   returns mode=self_managed + cap + key
                   use it directly.                          config sentinels overlay:
                                                               model "" or absent → overlay
                                                               cap 0   or absent → overlay

lease render    → context_cap_tokens=1048576                  → context_cap_tokens=1048576
                  model=accounts/fireworks/models/kimi-k3       model=accounts/.../kimi-k3
                  api_key=<from admin workspace vault>          api_key=<fw_LIVE_…>

L3 run chunking
                → threshold = 0.75 × 1048576                  → threshold = 0.75 × 1048576
```

**Overlay rule (per-field, independent, applied at lease time):** `config` `model: ""` OR the `model` key absent ⇒ overlay from the provider the revision's `runtime_model_ref` resolves to (`rustd/crates/afd_fleet/src/lease/pull/mod.rs`, `rustd/crates/afd_gate/src/policy/context.rs`). Same rule for `context_cap_tokens: 0` OR absent. Non-empty / non-zero values are respected as-is. The setup carries the *visible* sentinels (`""`, `0`) under self-managed posture so a reviewer can spot at a glance that "this fleet inherits from the selected model"; an absent key is the safety net.

`x-agentsfleet.model` and `x-agentsfleet.context.*` are first-class fields on `FleetConfig`, carried on the lease as `ExecutionPolicy` / `ContextBudget` (`rustd/crates/afd_wire/src/policy/mod.rs`) *before* sentinel defaults are substituted, so values the setup pins win against runtime defaults.

Caps are resolved at provider-set time, never at trigger time ([`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §"10. The model library — catalogue table + authenticated read").

**Dashboard equivalent — the Models page (`/settings/models`).** A browser user manages the same self-managed posture there instead of the CLI. The **active-model row** shows the resolved `provider` · `model` with a LIVE/DEFAULT pill (the dashboard read of `tenant provider show`). The secret-driven **switch-list** flips the active provider in one click. It calls the same self-managed provider-set as `tenant provider create`, keyed off the server-projected secret `kind` (see [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §8.3). The row's **Edit** replaces the credential whole via PUT (§8.3), prefilled from the projected row — provider, base URL, and model are editable, and the key is re-entered because a stored secret is never readable. The `/credentials` page was removed outright (not redirected) — provider keys live here; custom (non-provider) secrets moved to the standalone Secrets & ENVs page (`/secrets`).

## §8.8 Slack as a resident surface (Rung 0) — M106

> **Status.** Shipped in `agentsfleetd` by M206_002, which also adds fleets that subscribe to one channel by its ID; [`scenarios/slack-incident-responder.md`](./scenarios/slack-incident-responder.md) walks the zero-, one- and several-fleet cases.

After a workspace admin connects Slack once, an `@mention` lands as one `slack_mention` admission and routes to a fleet subscribed to that channel, or else to the channel's resident fleet ([`scenarios/slack-incident-responder.md`](./scenarios/slack-incident-responder.md) §"4. Routing a mention"; channel memory: [`memory.md`](./memory.md) §"4. The M106 channel pattern").
The bot answers on mention and never acts unattended; acting unattended with approval is Rung 1 ([`high_level.md`](./high_level.md) §"5.2 Slack-resident channel bot — the on-ramp (M106)").
