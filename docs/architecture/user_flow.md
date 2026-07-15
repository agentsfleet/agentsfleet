# User Flow — how a user uses the system

> Parent: [`README.md`](./README.md)

Read this when you want to know how a real human gets from "I want a Fleet" to "the Fleet is running on my repo." The §-numbered subsections are stable anchors that other specs reference; do not rename them without sweeping cross-references.

The initial user assumption is simple:

- the user is already working inside Claude (or Amp, Codex CLI, OpenCode — any coding fleet that can read SKILL.md)
- the user is already working on their own project or infrastructure
- the user wants operational work to continue without babysitting an endless terminal loop

The Claude session becomes the place where the user defines, imports, creates, updates, and supervises Fleets. The Fleet runtime becomes the place where long-lived operational outcomes continue after the chat session ends.

For the full end-to-end install + first-trigger walkthroughs (platform-managed and self-managed), see [`scenarios/`](./scenarios/).

## §8.0 The wedge surface

The MVP's user-facing wedge is the **`agentsfleet` Command-Line Interface (CLI) plus the first-party Fleet library**. A user goes from cold machine to a running fleet through the CLI — no host-agent and no markdown-skill install step:

```bash
curl -fsSL https://agentsfleet.dev | bash   # installs the agentsfleet CLI
agentsfleet login                            # Clerk OAuth
agentsfleet library                          # browse the first-party Fleet library gallery (GET /v1/fleets/bundles)
agentsfleet install --library github-pr-reviewer
```

`agentsfleet install` installs one already-onboarded template (§8.2.2):

- **`--library <id>`** — a curated, ready-to-run Fleet Bundle from the workspace gallery (platform library entries plus any tenant library entries). The pinned `SKILL.md`/`TRIGGER.md` are read server-side from the onboarded library row; the user supplies only the secrets the library entry declares.
- **Local or GitHub-authored bundles** — onboard first as a tenant library entry through the dashboard or `POST /v1/workspaces/{ws}/fleet-libraries`, then install by that tenant library id. There is no direct local-file install path — `install` only accepts `--library <id>`. Existing Fleets can still be live-edited from disk with `agentsfleet fleet update <fleet_id> --from <path>`.

Configuration — Slack channel, production-branch glob, cron schedule — lives in the bundle's `TRIGGER.md`/`SKILL.md`, version-controlled by design. A library entry ships sensible defaults; customize by editing a local copy, onboarding it as a tenant library entry, or updating an installed Fleet with `agentsfleet fleet update`. There are no install-time gating questions: the markdown *is* the configuration.

This matters architecturally: the install surface is the CLI (deterministic, scriptable, host-neutral) and the bundle is portable markdown. The runtime stays prompt-driven; `agentsfleet install` plus the catalogue is what makes it tractable from a cold start.

## §8.0.1 Deployment posture: hosted-only in v2

v2 ships **hosted-only** on `api.agentsfleet.net`. The skill detects no choice point: it defaults to the hosted endpoint, prompts Clerk OAuth via `agentsfleet login` if the CLI is not authenticated, and proceeds. There is no self-host runbook in v2 and no `--self-host` flag.

This is a deliberate scope cut, not a gap in the architecture. The runtime is already structured so the auth substrate (Clerk OAuth), KMS adapter (cloud KMS), and process orchestration (Fly.io machines) are the only deployment-specific layers — the worker, runner, sandbox, event stream, and reasoning loop are all posture-agnostic. **Validating** that on a clean non-Fly Linux host (Clerk shim or local-token auth, a portable KMS adapter, the runner's Landlock+cgroups+bwrap on a vanilla VM, systemd orchestration) is a v3 workstream once v2 has earned the trust to justify the integration burden.

Practically, this means:

- v2 launch claim is **OSS + self-managed + markdown-defined**. Not "self-hostable."
- The `/self-host` runbook page does not exist on `docs.agentsfleet.net` for v2.
- Users who need self-host today are out of scope; the AI-infra / GPU-cloud / regulated mid-market P1 personas are v3 customers, not v2.
- self-managed still ships in v2 — it sits on top of the hosted posture and removes the inference-cost lock-in independently of where the runtime runs. See [`capabilities.md`](./capabilities.md) and [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §1.

## §8.1 Authoring the Fleet

The user defines the Fleet in project files:

- `SKILL.md` describes how the Fleet's in-run fleet should think, what its job is, what "good" looks like, what evidence to gather, and what actions require caution. Plain English. No framework syntax. Required.
- `TRIGGER.md` describes how the Fleet wakes up: webhook, cron, user steer, or a combination. Also declares `tools:`, `credentials:`, `network.allow:`, `budget:`, and `context:` knobs. Optional for Fleet Bundles; a missing trigger creates the default manual/API trigger at install time.
- Optional support files such as `SOUL.md`, provider playbooks, examples, scripts, and assets can ship with a Fleet Bundle. They are files the Fleet's in-run fleet may read inside the sandbox workspace; they are not capability grants.

The user iterates those files from Claude in natural language:

- "tighten the deploy-failure diagnosis prompt"
- "add a periodic health check every 15 minutes"
- "require approval before teardown"
- "include Fly logs and Redis health in the first pass"

This keeps the operational logic editable by changing instructions, not by rewriting a typed workflow engine for every variation.

A **Fleet Bundle** is the import/template form of those files. It may come from:

- a first-party Fleet library card,
- an uploaded folder/archive *(DEFERRED 2026-06-20, Indy-acked — not in the shipping picker)*, or
- a public GitHub repository/path.

Import validates and snapshots the bundle before create. The snapshot is immutable: searchable metadata and parsed requirements live in Postgres, while the source archive and assets live in object storage. Creating from a bundle still creates a runtime Fleet. `/fleets`, `core.fleets`, and `fleet_id` are the canonical runtime API/schema names.

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
agentsfleet login                  # Clerk OAuth → token in ~/.config/agentsfleet/credentials.json
agentsfleet connector status github --json
```

`agentsfleet doctor --json` is the readiness gate (§8.2.2 step 2): on any miss it prints the explicit fix commands and stops. The commands are deliberately separate so a user with most of the chain already in place skips what they already have.

### §8.2.2 Per-Fleet create flow

1. The user picks a catalogue library entry (`agentsfleet install --library <id>`) or authors `SKILL.md` and `TRIGGER.md` for a local bundle (§8.0) — optionally with a coding agent (Claude Code, Amp, Codex CLI, OpenCode) helping draft the markdown.
2. **`agentsfleet doctor --json` runs first** as the deterministic readiness gate after login. Doctor is fast and verifies connectivity + workspace health only — `server_reachable`, `workspace_selected`, and `workspace_binding_valid`. It does **not** carry provider or trial posture; that lives in `agentsfleet tenant provider show --json` (mode/provider/model/context cap) and `agentsfleet billing show` (free-trial state), read separately once health passes. The CLI (and any caller) reads `doctor`'s JSON output verbatim and aborts on failure with the user-facing message instead of letting `install` fail with a confusing 401. Doctor is the only sanctioned preflight surface for health — no parallel `preflight` command exists.
3. The user (or coding agent) creates the Fleet from an onboarded library entry:
   - **Platform library entry** — `POST /v1/workspaces/{ws}/fleets` with `{platform_library_id, name?}`.
   - **Tenant library entry** — `POST /v1/workspaces/{ws}/fleets` with `{tenant_library_id, name?}` after the local/GitHub source has been onboarded through `POST /v1/workspaces/{ws}/fleet-libraries` or the dashboard.
   - **Existing Fleet edit** — `agentsfleet fleet update <fleet_id> --from <path>` PATCHes `source_markdown` / `trigger_markdown` in place; it is not a create path.
4. The API reads the library entry's `SKILL.md` / `TRIGGER.md`, parses frontmatter, derives `name` + `config_json`, persists the Fleet row, and synchronously creates the events stream + consumer group before returning 201. When a library entry lacks `TRIGGER.md`, the API generates a default manual/API trigger with no tools, no secrets, and no network. The 201 response carries `fleet_id` and `webhook_urls: { <source>: <url> }` — one entry per webhook trigger declared by `TRIGGER.md` or the library entry's metadata. See [`data_flow.md`](./data_flow.md) for the create-to-lease sequence.
5. The API stores the Fleet config, linked secret reference, approval policy, trigger declarations (`triggers: [...]` array), and optional bundle snapshot reference.
6. **Provider wiring follows the trigger surface.** For the GitHub App path, a workspace administrator connects GitHub once. `agentsfleet` signs state bound to the selected workspace; GitHub returns `installation_id` plus a one-time user-authorization code; `agentsfleetd` exchanges that code and verifies the user can access the installation before storing its workspace route. An installation already owned by another workspace is rejected without reassignment. The administrator selects the maximum repositories during App installation, and each fleet declares its smaller `repositories` + `events` subscription; GitHub then delivers automatically to `/v1/ingress/github`. Other custom webhooks retain the printed per-fleet URL and operator-run provider registration. The platform holds its own App identity and secrets, never a user's Personal Access Token (PAT).
7. Future triggers are served with no restart and no watcher thread: creation made the Fleet's events stream + consumer group up front (step 4), so each later trigger `XADD`s to the canonical stream name `fleet:{id}:events` and the control plane hands that event to whichever `agentsfleet-runner` leases next (`POST /v1/runners/me/leases`).

After creation, the Fleet is no longer tied to the interactive Claude session that created it.

### §8.2.3 Fleet Bundle dashboard flow

The dashboard create screen is source-first, not paste-first:

1. **Start from Fleet library** lists first-party Fleet Bundles such as GitHub Pull Request reviewer and Zoho Recruit outreach.
2. **Upload bundle** accepts a local folder/archive snapshot. *(DEFERRED 2026-06-20, Indy-acked — not in the shipping picker; ship library + GitHub + paste first.)*
3. **Import from GitHub** accepts a public repository/path and snapshots the resolved content.
4. **Manual paste** remains available for power users and existing tests.

Import validation is server-side: required `SKILL.md`, safe paths, size caps, frontmatter parsing, and no resolved secrets. Parsed preview shows required secrets, required tools, network hosts, trigger mode, and whether `TRIGGER.md` is present. Missing workspace secrets block Fleet creation with a clear list and a create action that routes to Secrets & ENVs, not tenant model-provider setup.

Two first scenarios anchor the product flow:

- **GitHub Pull Request reviewer.** Wakes on GitHub pull request events, reads diff/context through `api.github.com`, and posts review comments using the workspace `github` secret.
- **Zoho Recruit outreach.** Reads Zoho Recruit data, optionally sends mail through a separate secret, and uses support files such as `ZOHO.md` for provider-specific instructions.

## §8.3 Triggering the Fleet

A Fleet's `TRIGGER.md` declares `triggers: [...]` — an array of 1–8 trigger entries (unique on `(type, source)` tuple). Each entry is one of:

- **GitHub App trigger.** Type `webhook`, `source: github`, explicit `repositories: [owner/repo, …]`, and `events: [...]`. GitHub posts once to `POST /v1/ingress/github`; the signed delivery's installation resolves the workspace, then repository + event + approved GitHub grant select the fleet. Omitting `repositories` is fail-closed for App traffic.
- **Manual/custom webhook trigger.** The existing fleet-addressed routes remain available: `POST /v1/webhooks/{fleet_id}` and the GitHub-specific `POST /v1/webhooks/{fleet_id}/github`. The operator registers those URLs and a workspace webhook secret with the provider. This path does not infer a fleet from an App installation and does not require `repositories`.
- **Cron trigger.** Type `cron`, `schedule` as a 5-field cron expression, plus `timezone` and `message`. Installing the Fleet stores one desired schedule and synchronously registers the same stable schedule identifier with QStash. QStash owns the clock and sends each signed fire to `agentsfleetd`, which appends one synthetic event with `actor=cron:<schedule_id>`. The runner and its disposable NullClaw child own no timer. `TRIGGER.md` allows at most one declarative cron entry per Fleet; the schedule API can manage additional explicit schedules within the per-Fleet limit.

In addition to the declared triggers, every Fleet always accepts:

- **User steer.** The user, while in Claude, asks to run an operational task. Claude invokes `agentsfleet steer {id} "<message>"` or types into the dashboard's chat composer on `/fleets/{id}`, which POSTs to `/v1/workspaces/{ws}/fleets/{id}/messages` and `XADD`s directly to `fleet:{id}:events` with `actor=steer:<user>` — the same single-ingress path webhook and cron use.

All actors flow through the same runtime path. The Fleet's in-run fleet loop does not branch on actor type — the same `http_request`-driven evidence gathering and Slack post happen regardless of how the work was triggered. The "morning health check" steer that ships as the create-time smoke test produces a real first-pass evidence sweep, not a canned response — the SKILL.md prose is what dictates behaviour, not the actor field.

`type: api` (catch-all JSON ingress at `POST /v1/fleets/{id}/events`) is reserved by the architecture but **not accepted in `TRIGGER.md` in v1** — admission lands with the workspace-API-tokens spec that builds the `/v1/auth/tokens` surface. Webhook and cron cover the wedge.

Beyond the three trigger ingresses, the runtime emits its own `system:*` events on the activity channel when state changes apply (`config_updated` after a PATCH reload; more kinds to follow). These are not triggers — they are the runtime telling the user "what I just had to apply got applied" — see [`data_flow.md`](./data_flow.md). They surface in the same activity tail and in `agentsfleet events {id} --actor=system`, so the user sees them alongside the work the fleet does.

## §8.4 Working from Claude or the dashboard

The user experience inside Claude (or Amp / Codex CLI / OpenCode) feels like this:

1. The user is already in their project.
2. The user asks Claude to create or refine an operational fleet.
3. Claude edits `SKILL.md`, `TRIGGER.md`, and related project instructions.
4. Claude installs or updates the fleet through the CLI. For GitHub App triggers, `TRIGGER.md` carries explicit `repositories` and `events`; the workspace's existing App installation supplies the event source. For a custom webhook, the install response still carries `webhook_urls` for operator registration.
5. Claude can also manually invoke the fleet via `agentsfleet steer` for one-off user-triggered tasks.
6. Later, the fleet wakes on webhook or a QStash fire without the user staying in the terminal or any `agentsfleet` cron daemon running.
7. When the user returns to Claude, they inspect what happened from durable history (`agentsfleet events {id}` or the dashboard Events tab) instead of reconstructing it from memory.

The dashboard equivalent surface on `/fleets/{id}` matches the CLI path:

- The **Trigger panel** renders one card per declared trigger. A GitHub App card shows connector state plus repository/event subscriptions and routes a disconnected workspace to Connect GitHub. Custom providers retain registration guidance and copyable per-fleet URLs. The dashboard never asks for or stores a user's provider PAT.
- The **chat surface** (composed via `@assistant-ui/react`) shows webhook / cron / continuation events as system chips, fleet reasoning as streaming assistant bubbles, and the steer composer at the bottom turns user input into an event on the fleet's stream.

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

The install flow is the same shape in both postures: **run `agentsfleet doctor --json` for connectivity + workspace health, then read the active provider posture from `agentsfleet tenant provider show --json`, branch on `mode`; the bundle's frontmatter carries resolved-or-sentinel model/cap values.** Doctor is the sanctioned health check — it verifies `server_reachable`, `workspace_selected`, and `workspace_binding_valid`; it does **not** carry provider or trial posture. If a health check fails (or the CLI is not authenticated) the CLI prints the `agentsfleet login` hint and stops; `tenant provider show` is only meaningful once health passes. Free-trial state comes from `agentsfleet billing show`. The CLI never reads the model library directly — `tenant provider show` always carries resolved values (synth-default for tenants with no row, real values for tenants with an explicit row).

```
                     PLATFORM-MANAGED (John Doe)                self-managed (John Doe, post-flip)
                  ─────────────────────────────────       ─────────────────────────────────
install flow   →   doctor --json (health)                  doctor --json (health)
                    server_reachable: true  ✓              server_reachable: true  ✓
                    workspace_selected: true ✓             workspace_selected: true ✓
                    workspace_binding_valid: ✓             workspace_binding_valid: ✓
                  ─ if any health check fails: print      ─ same health-fail short-circuit ─
                    `agentsfleet login` and STOP. ─
                  tenant provider show --json:            tenant provider show --json:
                    {mode=platform,                        {mode=self_managed,
                     model=accounts/fireworks/models/kimi-k2.6,                provider=fireworks,
                     context_cap_tokens=256000}              model=accounts/.../kimi-k2.6,
                  (billing show → free-trial state)         context_cap_tokens=256000}
                  branch on mode → write frontmatter      branch on mode → write frontmatter
                  pin into frontmatter (resolved):        pin into frontmatter (sentinels):
                    model: accounts/fireworks/models/kimi-k2.6                model: ""
                    context_cap_tokens: 256000              context_cap_tokens: 0

tenant provider → (nothing — synth-default                → agentsfleet tenant provider create
                   stays in place)                            --secret account-fireworks-key
                                                              → API loads vault row
                                                              → API reads core.model_library
                                                              → upsert tenant_model_selection row
                                                                {mode=self_managed, provider, model,
                                                                 context_cap_tokens, secret_ref}

trigger fires  → lease resolve:                            → lease resolve:
                   resolveActiveProvider()                    resolveActiveProvider()
                     no row → synth-default                    follows secret_ref to vault
                   frontmatter has resolved cap →              returns mode=self_managed + cap + key
                   use it directly.                          frontmatter sentinels overlay:
                                                               model "" or absent → overlay
                                                               cap 0   or absent → overlay

createExecution → context_cap_tokens=256000               → context_cap_tokens=256000
                  model=accounts/fireworks/models/kimi-k2.6                   model=accounts/.../kimi-k2.6
                  api_key=<from admin workspace vault>                   api_key=<fw_LIVE_…>

L3 run chunking
                → threshold = 0.75 × 200000               → threshold = 0.75 × 256000
```

**Overlay rule (per-field, independent, applied at lease time):** frontmatter `model: ""` OR `model:` key absent ⇒ overlay from `tenant_model_selection.model` (or synth-default if no row). Same rule for `context_cap_tokens: 0` OR absent. Non-empty / non-zero values respected as-is. The bundle's frontmatter carries the *visible* sentinels (`""`, `0`) under self-managed posture so a human reading it can spot at a glance that "this fleet inherits from tenant config"; absent-key is the safety net for hand-edits.

The parser-side companion to this rule landed with M49: `x-agentsfleet.model` and `x-agentsfleet.context.*` are now first-class fields on `FleetConfig`, carried on the lease as `ExecutionPolicy` / `ContextBudget` (`execution_policy.zig`) *before* auto-sentinel defaults are substituted. Frontmatter overrides therefore win against runtime defaults (the doc previously described this shape but the parser dropped the fields silently — now closed).

Single source of truth for caps: the `core.model_library` table (tenant read: bearer-authed `GET /v1/models`; the former public cap.json route is retired). Resolved server-side at `tenant provider create` time (self-managed path) or hardcoded as a server-side synth-default constant (platform path). **Never resolved at trigger time** — would add a network dependency to the hot path. See [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §10 for the library shape and §1 for the full self-managed posture.

**Dashboard equivalent — the Models page (`/settings/models`).** A browser user manages the same self-managed posture there instead of the CLI. The **active-model row** shows the resolved `provider` · `model` with a LIVE/DEFAULT pill — the dashboard read of `tenant provider show` — and the secret-driven **switch-list** flips the active provider in one click, calling the same self-managed provider-set as `tenant provider create`, keyed off the server-projected secret `kind` (see [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §8.3). The row's **Replace key** rotates a provider key in place via PATCH (§8.3) without re-entering model or endpoint. The `/credentials` page was removed outright (not redirected) — provider keys live here; custom (non-provider) secrets moved to the standalone Secrets & ENVs page (`/secrets`).

## §8.8 Slack as a resident surface (Rung 0) — M106

A second front door, alongside Claude / CLI / dashboard, for users who live in Slack and never author markdown. After a workspace admin connects Slack once in the dashboard (OAuth — Open Authorization; the install is a `fleet:slack` vault handle plus a generic `core.connector_installs` row mapping `team_id → workspace`), `@agentsfleet` lives in any channel it's invited to:

1. A user `@mentions` it; the signed events ingress (`POST /v1/connectors/slack/events`) resolves `(team_id, channel_id)` and lands a `slack:<user>` event via the webhook-producer XADD shape (signature-authed, no principal).
2. The first mention in a channel materializes a **durable per-channel resident fleet** by calling the existing fleet-create path with a default channel-bot skill.md (a `core.fleets` row with a code-set reactive config — read-only, no triggers, no cron), bound in the generic `core.connector_channels`.
3. The run hydrates and captures that channel's memory via the existing `/v1/runners/me/memory/{fleet_id}` loop ([`runner_fleet.md`](./runner_fleet.md) §Memory continuity) — so the bot **learns the channel**, and memory persists thread→thread because the resident fleet (not the thread) is the namespace. The answer posts back in-thread.

The bot is **reactive** by design — it answers on mention, never acts unattended. Converting a recurring need into a durable teammate that wakes on a real source and acts with approval is **Rung 1** (the follow-on; out of scope of M106). Canonical spec: `docs/v2/done/M106_001_P1_API_DOCS_INFRA_UI_SLACK_RESIDENT_CHANNEL_BOT.md`.
