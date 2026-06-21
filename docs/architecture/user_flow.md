# User Flow — how a user uses the system

> Parent: [`README.md`](./README.md)

Read this when you want to know how a real human gets from "I want a Fleet" to "the Fleet is running on my repo." The §-numbered subsections are stable anchors that other specs reference; do not rename them without sweeping cross-references.

The initial user assumption is simple:

- the user is already working inside Claude (or Amp, Codex CLI, OpenCode — any coding fleet that can read SKILL.md)
- the user is already working on their own project or infrastructure
- the user wants operational work to continue without babysitting an endless terminal loop

The Claude session becomes the place where the user defines, imports, creates, updates, and supervises Fleets. The Fleet runtime becomes the place where long-lived operational outcomes continue after the chat session ends.

For the full end-to-end install + first-trigger walkthroughs (platform-managed and self-managed), see [`scenarios/`](./scenarios/).

## §8.0 The wedge surface: `/agentsfleet-install-platform-ops` skill

The MVP's user-facing wedge is not raw `agentsfleet install`. It is a host-neutral SKILL.md invoked as **`/agentsfleet-install-platform-ops`** — the same slash-command in every host (Claude Code, Amp, Codex CLI, OpenCode). One install procedure: drop the SKILL.md directory into the host's skills folder (`~/.claude/skills/agentsfleet-install-platform-ops/` or the host-equivalent path), or run the one-liner `curl -fsSL https://agentsfleet.dev | bash`, which installs `agentsfleet` and adds the skill into the host path in one step (§8.2.1). No plugin manifest, no per-host packaging fork. The brand is in the slash-command itself; future skills follow the same pattern (`/agentsfleet-steer`, `/agentsfleet-doctor`).

The skill is the install UX; `agentsfleet install --from <path>` is the substrate it drives.

What the skill does, in order:

1. **Detects the user's repo**: reads `.github/workflows/*.yml`, `fly.toml`, `Dockerfile`, `pyproject.toml`, `package.json` to infer CI provider, deploy target, and Slack channel. Bails clearly if no GitHub Actions workflow is detected (non-GH CI is post-MVP).
2. **Asks at most three or four gating questions** through the host-neutral `variables:` frontmatter (so the same SKILL.md works on Claude Code, Amp, Codex CLI, and OpenCode without `AskUserQuestion` lock-in). Slack channel, prod branch glob, and cron opt-in.
3. **Resolves credentials in order**: 1Password CLI (`op read`) → environment variables → interactive prompt. The skill never asks again for what `op` already has.
4. **Calls `agentsfleet doctor --json` first** (see §8.2) to verify auth + workspace binding before any write.
5. **Generates `.agentsfleet/platform-ops/SKILL.md` and `.agentsfleet/platform-ops/TRIGGER.md`** in the user's repo with substituted values, refusing to overwrite without `--force`. These files are committed by the user — they are the configuration, version-controlled by design.
6. **Drives `agentsfleet install --from .agentsfleet/platform-ops/`** then runs a batch `agentsfleet steer {id} "morning health check"` smoke test.

This matters architecturally for two reasons. First, the skill artifact is portable — it is a markdown file, not a Claude-specific binary. The same wedge installs from any fleet CLI that can read SKILL.md. Second, the skill is the only place where repo detection, secret resolution, and ≤4 question discipline are enforced. The runtime stays prompt-driven; the install UX is what makes the prompt-driven runtime tractable for a first-time user.

## §8.0.1 Deployment posture: hosted-only in v2

v2 ships **hosted-only** on `api.agentsfleet.net`. The skill detects no choice point: it defaults to the hosted endpoint, prompts Clerk OAuth via `agentsfleet login` if the CLI is not authenticated, and proceeds. There is no self-host runbook in v2 and no `--self-host` flag.

This is a deliberate scope cut, not a gap in the architecture. The runtime is already structured so the auth substrate (Clerk OAuth), KMS adapter (cloud KMS), and process orchestration (Fly.io machines) are the only deployment-specific layers — the worker, runner, sandbox, event stream, and reasoning loop are all posture-agnostic. **Validating** that on a clean non-Fly Linux host (Clerk shim or local-token auth, a portable KMS adapter, the runner's Landlock+cgroups+bwrap on a vanilla VM, systemd orchestration) is a v3 workstream once v2 has earned the trust to justify the integration burden.

Practically, this means:

- v2 launch claim is **OSS + self-managed + markdown-defined**. Not "self-hostable."
- The `/self-host` runbook page does not exist on `docs.agentsfleet.net` for v2.
- Users who need self-host today are out of scope; the AI-infra / GPU-cloud / regulated mid-market P1 personas are v3 customers, not v2.
- self-managed still ships in v2 — it sits on top of the hosted posture and removes the inference-cost lock-in independently of where the runtime runs. See [`capabilities.md`](./capabilities.md) and [`scenarios/02_self_managed.md`](./scenarios/02_self_managed.md).

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

- a first-party template card,
- an uploaded folder/archive *(DEFERRED 2026-06-20, Indy-acked — not in the shipping picker)*, or
- a public GitHub repository/path.

Import validates and snapshots the bundle before create. The snapshot is immutable: searchable metadata and parsed requirements live in Postgres, while the source archive and assets live in object storage. Creating from a bundle still creates a runtime Fleet. `/fleets`, `core.fleets`, and `fleet_id` are the canonical runtime API/schema names.

## §8.2 Creating the Fleet

Once the files are ready, the user creates the Fleet in the workspace.

### §8.2.1 Cold-machine bootstrap (run once per machine)

The canonical entry is the one-liner served from `https://agentsfleet.dev` — it wraps the first two steps below (install `agentsfleet`, add the skill):

```bash
curl -fsSL https://agentsfleet.dev | bash   # installs agentsfleet, then npx skills add agentsfleet/skills
```

Or run the chain explicitly (skip any step already in place):

```bash
npm install -g @agentsfleet/cli     # CLI binary only (templates ship from github.com/agentsfleet/skills, not bundled)
npx skills add agentsfleet/skills         # symlinks /agentsfleet-* into host skill paths (skills now ship from github.com/agentsfleet/skills)
agentsfleet login                          # Clerk OAuth → token in ~/.config/agentsfleet/credentials.json
gh auth login -s admin:repo_hook         # one-time; lets the install-skill register webhooks
```

The install-skill's first action (§8.2.2 step 1) is a `which agentsfleet && which gh && agentsfleet doctor --json` precondition check; on any miss it prints the explicit four-command block above and stops. The commands are deliberately separate so a user with most of the chain already in place skips what they already have.

### §8.2.2 Per-Fleet create flow

1. Claude (or another fleet), typically driven by the `/agentsfleet-install-platform-ops` skill (§8.0), helps author or refine `SKILL.md` and `TRIGGER.md`.
2. **`agentsfleet doctor --json` runs first** as the deterministic readiness gate after login. Doctor is fast and verifies connectivity + workspace health only — `server_reachable`, `workspace_selected`, and `workspace_binding_valid`. It does **not** carry provider or trial posture; that lives in `agentsfleet tenant provider show --json` (mode/provider/model/context cap) and `agentsfleet billing show` (free-trial state), read separately once health passes. The skill (and any future caller) reads `doctor`'s JSON output verbatim and aborts on failure with the user-facing message instead of letting `install` fail with a confusing 401. Doctor is the only sanctioned preflight surface for health — no parallel `preflight` command exists.
3. The user (or skill) creates or updates the Fleet through one of two source paths:
   - **Direct Markdown create** — `agentsfleet install --from <path>` or manual dashboard paste POSTs `{trigger_markdown, source_markdown}` to `POST /v1/workspaces/{ws}/fleets`.
   - **Fleet Bundle create** — the dashboard imports/uploads/selects a validated bundle snapshot, previews required credentials/tools/network, then POSTs `bundle_id` to the same Fleet creation handler. There is no bundle-specific install route.
4. The API parses frontmatter, derives `name` + `config_json`, persists the Fleet row, and synchronously creates the events stream + consumer group before returning 201. When a bundle lacks `TRIGGER.md`, the API generates a default manual/API trigger with no tools, no credentials, and no network. The 201 response carries `fleet_id` and `webhook_urls: { <source>: <url> }` — one entry per webhook trigger declared by `TRIGGER.md` or the imported bundle metadata. See [`data_flow.md`](./data_flow.md) for the create-to-lease sequence.
5. The API stores the Fleet config, linked credentials reference, approval policy, trigger declarations (`triggers: [...]` array), and optional bundle snapshot reference.
6. **Webhook registration on the upstream provider runs from the user's own machine** — the install-skill loops over webhook entries in the rendered TRIGGER.md and shells out to `gh api repos/.../hooks` (for GitHub) or the equivalent provider command, using the user's existing `gh` auth or stored API token. The platform never holds the user's Personal Access Token (PAT) for this step; the registration is logged on the provider side by the user. For dashboard-only creates, the Trigger panel on `/fleets/{id}` renders the exact terminal command pre-filled with the webhook URL and event list, ready to copy.
7. Future triggers are served with no restart and no watcher thread: creation made the Fleet's events stream + consumer group up front (step 4), so each later trigger `XADD`s to the canonical stream name `fleet:{id}:events` and the control plane hands that event to whichever `agentsfleet-runner` leases next (`POST /v1/runners/me/leases`).

After creation, the Fleet is no longer tied to the interactive Claude session that created it.

### §8.2.3 Fleet Bundle dashboard flow

The dashboard create screen is source-first, not paste-first:

1. **Start from template** lists first-party Fleet Bundles such as GitHub Pull Request reviewer and Zoho Recruit outreach.
2. **Upload bundle** accepts a local folder/archive snapshot. *(DEFERRED 2026-06-20, Indy-acked — not in the shipping picker; ship template + GitHub + paste first.)*
3. **Import from GitHub** accepts a public repository/path and snapshots the resolved content.
4. **Manual paste** remains available for power users and existing tests.

Import validation is server-side: required `SKILL.md`, safe paths, size caps, frontmatter parsing, and no resolved secrets. Parsed preview shows required credentials, required tools, network hosts, trigger mode, and whether `TRIGGER.md` is present. Missing workspace credentials block Fleet creation with a clear list and a create action that routes to workspace credentials, not tenant model-provider setup.

Two first scenarios anchor the product flow:

- **GitHub Pull Request reviewer.** Wakes on GitHub pull request events, reads diff/context through `api.github.com`, and posts review comments using the workspace `github` credential.
- **Zoho Recruit outreach.** Reads Zoho Recruit data, optionally sends mail through a separate credential, and uses support files such as `ZOHO.md` for provider-specific instructions.

## §8.3 Triggering the Fleet

A Fleet's `TRIGGER.md` declares `triggers: [...]` — an array of 1–8 trigger entries (unique on `(type, source)` tuple). Each entry is one of:

- **Webhook trigger.** Type `webhook`, `source` from M28's `PROVIDER_REGISTRY` (`github`, `linear`, `jira`, `grafana`, `slack`, `agentmail`, `clerk`), and `events: [...]` listing the provider-specific subscriptions. An external system POSTs to `POST /v1/webhooks/{fleet_id}/{source}`; `fleet_id` is the canonical Fleet identifier. The receiver verifies the HMAC signature via M28's middleware (per provider), normalises the payload, and lands a synthetic event on `fleet:{id}:events` with `actor=webhook:<source>`.
- **Cron trigger.** Type `cron`, `schedule` as a 5-field cron expression. NullClaw's in-runner cron tool fires on time. Each fire arrives as a synthetic event with `actor=cron:<schedule>`. At most one cron entry per Fleet.

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
4. Claude installs or updates the fleet. The skill captures `webhook_urls` from the install response, parses the rendered `TRIGGER.md` for `triggers[].events`, and shells out to `gh api repos/.../hooks` per webhook trigger — registration happens without leaving the terminal.
5. Claude can also manually invoke the fleet via `agentsfleet steer` for one-off user-triggered tasks.
6. Later, the fleet wakes on webhook or cron without the user staying in the terminal.
7. When the user returns to Claude, they inspect what happened from durable history (`agentsfleet events {id}` or the dashboard Events tab) instead of reconstructing it from memory.

The dashboard equivalent surface on `/fleets/{id}` matches the CLI path:

- The **Trigger panel** renders one card per declared trigger. Known providers get a pre-rendered terminal command (e.g. `gh api repos/.../hooks ...` for GitHub, `curl https://api.linear.app/graphql ...` for Linear) the user copies and runs locally. The card shows the registered hook id and last delivery once a real event arrives. The dashboard never holds the user's provider PAT.
- The **chat surface** (composed via `@assistant-ui/react`) shows webhook / cron / continuation events as system chips, fleet reasoning as streaming assistant bubbles, and the steer composer at the bottom turns user input into an event on the fleet's stream.

This matters because the fleet is not replacing Claude. It extends Claude from an interactive assistant into a durable operational worker — and the dashboard mirrors the same primitives so a user who lives in the browser sees an equivalent surface.

## §8.5 Example: Platform-Ops with GH Actions trigger

While working in Claude, the user defines a `platform-ops` fleet that:

- wakes on GitHub Actions deploy-failure webhooks (primary)
- wakes on a periodic production health cron (secondary; declared in `triggers[]` or added by NullClaw's `cron_add` tool at runtime)
- can also be steered manually by the user

When a GH Actions deploy fails:

1. GitHub posts to the fleet's webhook ingest URL `POST /v1/webhooks/{fleet_id}/github` with the failed `workflow_run` payload. The URL was registered earlier by the install-skill running `gh api repos/{repo}/hooks` from the user's machine; the platform never held the user's PAT for that step.
2. The webhook receiver verifies the HMAC signature against the workspace's stored credential (vault credential `github`, field `webhook_secret`). The credential is workspace-scoped — every fleet in the workspace whose `triggers[]` contains a `source: github` entry shares it by default; rotating it once rotates everywhere. Resolver: `vault.loadJson(workspace_id, name=trigger.source)` (where `trigger` is the matching `triggers[]` entry); an optional `x-agentsfleet.triggers[].credential_name:` frontmatter override scopes a distinct vault row per fleet for the per-fleet credential-isolation case (multi-org GitHub, multi-app Slack, multi-tenant B2B-on-agentsfleet).
3. The receiver normalizes the payload into a synthetic event and `XADD`s to `fleet:{id}:events` with `actor=webhook:github`, `type=webhook`, `workspace_id={ws}`, `request={run_url, head_sha, conclusion, ref, repo, attempt}`, `created_at=<epoch_ms>`.
4. A `agentsfleet-runner` long-polls `POST /v1/runners/me/leases`; on the lease path `agentsfleetd`:
   - INSERTs `core.fleet_events` (status='received')
   - passes the balance + approval gates
   - resolves credentials from the vault (GitHub PAT, Fly token, Slack bot token)
   - resolves provider config (`tenant_provider.resolveActiveProvider`) — platform-managed key OR self-managed key, depending on tenant posture
   - returns the lease carrying `secrets_map`, `network_policy`, `tools` list, `context` knobs, and provider config

   The runner then forks a sandboxed child that runs the NullClaw fleet on the leased event.
5. The fleet's NullClaw fleet reasons over the message:
   - calls `http_request GET https://api.github.com/repos/{repo}/actions/runs/{run_id}/logs` with `${secrets.github.api_token}` substituted at the tool bridge
   - calls `http_request GET ${fly.host}/v1/apps/{app}/logs`
   - calls `http_request GET ${upstash.host}/v2/redis/stats/{db}`
   - correlates: was the failure a migration error vs OOM kill vs network timeout vs deploy-config drift
   - calls `http_request POST ${slack.host}/api/chat.postMessage` with the diagnosis
6. The fleet's response is UPDATEd into `core.fleet_events` (status='processed', response_text, tokens, wall_ms).
7. If the SKILL.md prose said the fleet may schedule a follow-up health check, it calls `cron_add "*/30 * * * *" "post-recovery health check"`.

When the user opens Claude later, they see the outcome trail in `core.fleet_events` keyed by actor — they can filter "show me all webhook:github events from the last 24h" or "show me what kishore steered last Tuesday." They never reconstruct from memory; the durable log is authoritative.

The same fleet also responds to manual `agentsfleet steer {id} "morning health check"` — same reasoning loop, different `actor=steer:kishore`.

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

The install-skill's job in both postures is the same shape: **run `agentsfleet doctor --json` for connectivity + workspace health, then read the active provider posture from `agentsfleet tenant provider show --json`, branch on `mode`, write resolved-or-sentinel into frontmatter.** Doctor is the sanctioned health check — it verifies `server_reachable`, `workspace_selected`, and `workspace_binding_valid`; it does **not** carry provider or trial posture. If a health check fails (or the CLI is not authenticated) the skill prints the `agentsfleet login` hint and stops; `tenant provider show` is only meaningful once health passes. Free-trial state comes from `agentsfleet billing show`. The skill never calls the model-caps endpoint directly — `tenant provider show` always carries resolved values (synth-default for tenants with no row, real values for tenants with an explicit row).

```
                     PLATFORM-MANAGED (John Doe)                self-managed (John Doe, post-flip)
                  ─────────────────────────────────       ─────────────────────────────────
install-skill →   doctor --json (health)                  doctor --json (health)
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

tenant provider → (nothing — synth-default                → agentsfleet tenant provider add
add                stays in place)                            --credential account-fireworks-key
                                                              → API loads vault row
                                                              → API GETs /_um/.../cap.json
                                                              → upsert tenant_providers row
                                                                {mode=self_managed, provider, model,
                                                                 context_cap_tokens, credential_ref}

trigger fires  → lease resolve:                            → lease resolve:
                   resolveActiveProvider()                    resolveActiveProvider()
                     no row → synth-default                    follows credential_ref to vault
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

**Overlay rule (per-field, independent, applied at lease time):** frontmatter `model: ""` OR `model:` key absent ⇒ overlay from `tenant_providers.model` (or synth-default if no row). Same rule for `context_cap_tokens: 0` OR absent. Non-empty / non-zero values respected as-is. The install-skill emits the *visible* sentinels (`""`, `0`) under self-managed posture so a human reading the frontmatter can spot at a glance that "this fleet inherits from tenant config"; absent-key is the safety net for hand-edits.

The parser-side companion to this rule landed with M49: `x-agentsfleet.model` and `x-agentsfleet.context.*` are now first-class fields on `FleetConfig`, carried on the lease as `ExecutionPolicy` / `ContextBudget` (`src/lib/contract/execution_policy.zig`) *before* auto-sentinel defaults are substituted. Frontmatter overrides therefore win against runtime defaults (the doc previously described this shape but the parser dropped the fields silently — now closed).

Single source of truth for caps: `https://api.agentsfleet.net/_um/da5b6b3810543fe108d816ee972e4ff8/cap.json`. Resolved at `tenant provider add` time (self-managed path) or hardcoded as a server-side synth-default constant (platform path). **Never resolved at trigger time** — would add a network dependency to the hot path. See [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §9 for the endpoint shape and [`scenarios/02_self_managed.md`](./scenarios/02_self_managed.md) for the full self-managed walkthrough.
