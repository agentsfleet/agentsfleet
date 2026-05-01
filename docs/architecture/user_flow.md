# User Flow — how an operator uses the system

> Parent: [`README.md`](./README.md)

Read this when you want to know how a real human gets from "I want a zombie" to "the zombie is running on my repo." The §-numbered subsections are stable anchors that other specs reference; do not rename them without sweeping cross-references.

The initial user assumption is simple:

- the user is already working inside Claude (or Amp, Codex CLI, OpenCode — any agent that can read SKILL.md)
- the user is already working on their own project or infrastructure
- the user wants operational work to continue without babysitting an endless terminal loop

The Claude session becomes the place where the user defines, installs, updates, and supervises zombies. The zombie runtime becomes the place where long-lived operational outcomes continue after the chat session ends.

For the full end-to-end install + first-trigger walkthroughs (platform-managed and BYOK), see [`scenarios/`](./scenarios/).

## §8.0 The wedge surface: `/usezombie-install-platform-ops` skill

The MVP's user-facing wedge is not raw `zombiectl install`. It is a host-neutral SKILL.md invoked as **`/usezombie-install-platform-ops`** — the same slash-command in every host (Claude Code, Amp, Codex CLI, OpenCode). One install procedure: drop the SKILL.md directory into the host's skills folder (`~/.claude/skills/usezombie-install-platform-ops/` or the host-equivalent path), or fetch it from `https://usezombie.sh/skills.md`. No plugin manifest, no per-host packaging fork. The brand is in the slash-command itself; future skills follow the same pattern (`/usezombie-steer`, `/usezombie-doctor`).

The skill is the install UX; `zombiectl install --from <path>` is the substrate it drives.

What the skill does, in order:

1. **Detects the user's repo**: reads `.github/workflows/*.yml`, `fly.toml`, `Dockerfile`, `pyproject.toml`, `package.json` to infer CI provider, deploy target, and Slack channel. Bails clearly if no GitHub Actions workflow is detected (non-GH CI is post-MVP).
2. **Asks at most three or four gating questions** through the host-neutral `variables:` frontmatter (so the same SKILL.md works on Claude Code, Amp, Codex CLI, and OpenCode without `AskUserQuestion` lock-in). Slack channel, prod branch glob, and cron opt-in.
3. **Resolves credentials in order**: 1Password CLI (`op read`) → environment variables → interactive prompt. The skill never asks again for what `op` already has.
4. **Calls `zombiectl doctor --json` first** (see §8.2) to verify auth + workspace binding before any write.
5. **Generates `.usezombie/platform-ops/{SKILL,TRIGGER,README}.md`** in the user's repo with substituted values, refusing to overwrite without `--force`. These files are committed by the user — they are the configuration, version-controlled by design.
6. **Drives `zombiectl install --from .usezombie/platform-ops/`** then runs a batch `zombiectl steer {id} "morning health check"` smoke test.

This matters architecturally for two reasons. First, the skill artifact is portable — it is a markdown file, not a Claude-specific binary. The same wedge installs from any agent CLI that can read SKILL.md. Second, the skill is the only place where repo detection, secret resolution, and ≤4 question discipline are enforced. The runtime stays prompt-driven; the install UX is what makes the prompt-driven runtime tractable for a first-time operator.

## §8.0.1 Deployment posture: hosted-only in v2

v2 ships **hosted-only** on `api.usezombie.com`. The skill detects no choice point: it defaults to the hosted endpoint, prompts Clerk OAuth via `zombiectl auth login` if the CLI is not authenticated, and proceeds. There is no self-host runbook in v2 and no `--self-host` flag.

This is a deliberate scope cut, not a gap in the architecture. The runtime is already structured so the auth substrate (Clerk OAuth), KMS adapter (cloud KMS), and process orchestration (Fly.io machines) are the only deployment-specific layers — the worker, executor, sandbox, event stream, and reasoning loop are all posture-agnostic. **Validating** that on a clean non-Fly Linux host (Clerk shim or local-token auth, a portable KMS adapter, executor's Landlock+cgroups+bwrap on a vanilla VM, systemd orchestration) is a v3 workstream once v2 has earned the trust to justify the integration burden.

Practically, this means:

- v2 launch claim is **OSS + BYOK + markdown-defined**. Not "self-hostable."
- The `/self-host` runbook page does not exist on `docs.usezombie.com` for v2.
- Operators who need self-host today are out of scope; the AI-infra / GPU-cloud / regulated mid-market personas in [`office_hours_v2.md`](./office_hours_v2.md) P1 are v3 customers, not v2.
- BYOK still ships in v2 — it sits on top of the hosted posture and removes the inference-cost lock-in independently of where the runtime runs. See [`capabilities.md`](./capabilities.md) and [`scenarios/02_byok.md`](./scenarios/02_byok.md).

## §8.1 Authoring the zombie

The user defines the zombie in project files:

- `SKILL.md` describes how the zombie should think, what its job is, what "good" looks like, what evidence to gather, and what actions require caution. Plain English. No framework syntax.
- `TRIGGER.md` (or merged frontmatter under `x-usezombie:` in a single SKILL.md) describes how the zombie wakes up: webhook, cron, operator steer, or a combination. Also declares `tools:`, `credentials:`, `network.allow:`, `budget:`, and `context:` knobs.

The user iterates those files from Claude in natural language:

- "tighten the deploy-failure diagnosis prompt"
- "add a periodic health check every 15 minutes"
- "require approval before teardown"
- "include Fly logs and Redis health in the first pass"

This keeps the operational logic editable by changing instructions, not by rewriting a typed workflow engine for every variation.

## §8.2 Installing the zombie

Once the files are ready, the user installs the zombie into the workspace.

Conceptually, the workflow is:

1. Claude (or another agent), typically driven by the `/usezombie-install-platform-ops` skill (§8.0), helps author or refine `SKILL.md` and `TRIGGER.md`.
2. **`zombiectl doctor --json` runs first** as the deterministic readiness gate. Doctor is auth-exempt, fast, and verifies four things: token validity, server reachability, an active workspace, and workspace binding for the current CLI. The skill (and any future caller) reads `doctor`'s JSON output verbatim and aborts on failure with the operator-facing message instead of letting `install` fail with a confusing 401. Doctor is the only sanctioned preflight surface — no parallel `preflight` command exists.
3. The user (or skill) installs or updates the zombie through `zombiectl install --from <path>` or the API. The CLI POSTs `{name, config_json, source_markdown}`; the API parses frontmatter, persists the zombie row, and synchronously creates the events stream + consumer group before returning 201. See [`data_flow.md`](./data_flow.md) for the install-to-worker claim sequence.
4. The API stores the zombie config, linked credentials reference, approval policy, and trigger settings.
5. The worker runtime becomes responsible for future triggers — no worker restart required (the watcher thread on `zombie:control` claims the new zombie within milliseconds).

After install, the zombie is no longer tied to the interactive Claude session that created it.

## §8.3 Triggering the zombie

For the MVP, the zombie is triggerable in three ways:

- **Webhook input**: an external system (most importantly GitHub Actions on `workflow_run.conclusion == failure`) sends an event to the zombie's webhook ingest URL, which on `main` is `POST /v1/webhooks/{zombie_id}` with an optional URL-secret suffix. The receiver verifies the HMAC signature against the workspace's stored credential (vault credential `github`, field `webhook_secret`), normalizes the payload, and lands a synthetic event on `zombie:{id}:events` with `actor=webhook:github`.
- **Cron input**: NullClaw's `cron_add` tool persists a schedule. Each fire arrives as a synthetic event with `actor=cron:<schedule>`.
- **Operator steer**: the user, while in Claude, asks to run an operational task. Claude invokes `zombiectl steer {id} "<message>"` (or the dashboard chat widget), which `XADD`s directly to `zombie:{id}:events` with `actor=steer:<user>` — the same single-ingress path webhook and cron use.

All three flow through the same runtime path. The zombie's reasoning loop does not branch on actor type — the same `http_request`-driven evidence gathering and Slack post happen regardless of how the work was triggered. The "morning health check" steer that ships as the install-time smoke test produces a real first-pass evidence sweep, not a canned response — the SKILL.md prose is what dictates behaviour, not the actor field.

## §8.4 Working from Claude

The user experience inside Claude (or Amp / Codex CLI / OpenCode) feels like this:

1. The user is already in their project.
2. The user asks Claude to create or refine an operational zombie.
3. Claude edits `SKILL.md`, `TRIGGER.md`, and related project instructions.
4. Claude installs or updates the zombie.
5. Claude can also manually invoke the zombie via `zombiectl steer` for one-off operator-triggered tasks.
6. Later, the zombie wakes on webhook or cron without the user staying in the terminal.
7. When the user returns to Claude, they inspect what happened from durable history (`zombiectl events {id}` or the dashboard Events tab) instead of reconstructing it from memory.

This matters because the zombie is not replacing Claude. It extends Claude from an interactive assistant into a durable operational worker.

## §8.5 Example: Platform-Ops with GH Actions trigger

While working in Claude, the user defines a `platform-ops` zombie that:

- wakes on GitHub Actions deploy-failure webhooks (primary)
- wakes on a periodic production health cron (secondary)
- can also be steered manually by the operator

When a GH Actions deploy fails:

1. GitHub posts to the zombie's webhook ingest URL, which on `main` is `POST /v1/webhooks/{zombie_id}` with the failed `workflow_run` payload.
2. The webhook receiver verifies the HMAC signature against the workspace's stored credential (vault credential `github`, field `webhook_secret`). The credential is workspace-scoped — every zombie in the workspace using `trigger.source: github` shares it; rotating it once rotates everywhere. Resolver: `vault.loadJson(workspace_id, name=trigger.source)`; an optional `x-usezombie.trigger.credential_name:` frontmatter override addresses the rare multi-org case.
3. The receiver normalizes the payload into a synthetic event and `XADD`s to `zombie:{id}:events` with `actor=webhook:github`, `type=webhook`, `workspace_id={ws}`, `request={run_url, head_sha, conclusion, ref, repo, attempt}`, `created_at=<epoch_ms>`.
4. The worker's per-zombie thread unblocks from `XREADGROUP`, processes the event:
   - INSERT `core.zombie_events` (status='received')
   - balance + approval gates pass
   - resolve credentials from the vault (GitHub PAT, Fly token, Slack bot token)
   - resolve provider config (`tenant_provider.resolveActiveProvider`) — platform-managed key OR BYOK key, depending on tenant posture
   - `executor.createExecution` opens a sandbox session with `secrets_map`, `network_policy`, `tools` list, `context` knobs, and provider config
   - `executor.startStage` invokes the NullClaw agent with the message
5. The zombie's NullClaw agent reasons over the message:
   - calls `http_request GET https://api.github.com/repos/{repo}/actions/runs/{run_id}/logs` with `${secrets.github.api_token}` substituted at the tool bridge
   - calls `http_request GET ${fly.host}/v1/apps/{app}/logs`
   - calls `http_request GET ${upstash.host}/v2/redis/stats/{db}`
   - correlates: was the failure a migration error vs OOM kill vs network timeout vs deploy-config drift
   - calls `http_request POST ${slack.host}/api/chat.postMessage` with the diagnosis
6. The zombie's response is UPDATEd into `core.zombie_events` (status='processed', response_text, tokens, wall_ms).
7. If the SKILL.md prose said the zombie may schedule a follow-up health check, it calls `cron_add "*/30 * * * *" "post-recovery health check"`.

When the user opens Claude later, they see the outcome trail in `core.zombie_events` keyed by actor — they can filter "show me all webhook:github events from the last 24h" or "show me what kishore steered last Tuesday." They never reconstruct from memory; the durable log is authoritative.

The same zombie also responds to manual `zombiectl steer {id} "morning health check"` — same reasoning loop, different `actor=steer:kishore`.

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
- the zombie executes durably outside that transient chat session

## §8.7 Model and context-cap origin (platform vs. BYOK target contract)

This section describes the intended M48 contract. Current `main` already ships the model-caps endpoint, but it still stores BYOK credentials through the workspace-scoped `PUT /v1/workspaces/{workspace_id}/credentials/llm` route and does not yet expose the tenant-scoped `tenant_providers` posture or `zombiectl provider set` flow described below.

Two things travel together: the **model** the executor invokes, and the **`context_cap_tokens`** L3 stage chunking uses. They originate from different places under platform-managed and BYOK postures, and the worker's overlay logic is what reconciles them at trigger time.

```
                     PLATFORM-MANAGED                          BYOK
                  ─────────────────────────              ──────────────────────
install-skill →   GET /_um/da5b6b3810543fe108d816ee972e4ff8/model-caps.json          (skill skips lookup —
                  → pin into frontmatter:                tenant_providers
                    model: claude-sonnet-4-6               already has cap)
                    context_cap_tokens: 200000          → frontmatter pinned to:
                                                          model: ""
                                                          context_cap_tokens: 0

provider set   → (nothing — defaults stay platform)   → zombiectl provider set
                                                          → API GETs
                                                            /_um/da5b6b3810543fe108d816ee972e4ff8/model-caps.json
                                                            for the llm.model
                                                          → write
                                                            tenant_providers.{
                                                              mode: byok,
                                                              model,
                                                              context_cap_tokens
                                                            }

trigger fires  → processEvent:                        → processEvent:
                   resolveActiveProvider()                resolveActiveProvider()
                     returns mode=platform                 returns mode=byok +
                   frontmatter has cap → use it.           tenant_providers
                                                           {model, cap}
                                                         frontmatter sentinels
                                                           (0 / "") → overlay
                                                           with tenant_providers
                                                           values

createExecution → context_cap_tokens=200000           → context_cap_tokens=256000
                  model=claude-sonnet-4-6               model=accounts/.../kimi-k2
                  provider_api_key=<platform>           provider_api_key=<fw_…>

L3 stage chunking
                → threshold = 0.75 × 200000           → threshold = 0.75 × 256000
```

Single source of truth: `https://api.usezombie.com/_um/da5b6b3810543fe108d816ee972e4ff8/model-caps.json`. Resolved at install time (platform path → pinned in frontmatter) or at `provider set` time (BYOK path → pinned in `tenant_providers`). **Never resolved at trigger time** — would add a network dependency to the hot path. See [`scenarios/02_byok.md`](./scenarios/02_byok.md) §5 for the endpoint shape.
