# Scenario 01 — Default install, platform-managed key

**Persona — John Doe.** First-time user. Has a GitHub repo with a CD pipeline. Wants a Fleet that wakes on deploy failures and posts diagnoses to Slack. No own LLM key. Brand-new tenant — running on the one-time starter credit grant. Tenant carries no `core.tenant_providers` row — the resolver synthesises the platform default for him.

> **Rate snapshot.** Through 2026-07-31 UTC every event and every run execution is free (`FREE_TRIAL_STAGE_NANOS = 0`); the gate and telemetry rows still run but `credit_deducted_nanos = 0`. After the cutoff, the rates in `src/agentsfleetd/state/tenant_billing.zig` apply. Cent-and-token arithmetic in steps 4–8 below was authored against an earlier rate table — the *flow* is unchanged, but every deduction is 0 during the trial. **For the live, customer-facing rate table, always consult [`https://agentsfleet.net/#pricing`](https://agentsfleet.net/#pricing).** The architecture description here covers shape and behaviour; numbers change. Code-level pin: [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) §2.3.

> **Important framing.** There is no separate "Free tier" in v2.0. Every tenant has the same credit-pool billing model and the same cost functions; new tenants just start with a one-time grant. John in this scenario, John in Scenario 02 (after he flips to self-managed), and any future tenant who tops up via support all run through identical code paths and identical billing math. "Free" is a marketing word for "starting credits not yet exhausted," not a code-path concept. See [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) §2.

**Outcome under test:** From cold start (`agentsfleet` not installed) to the first webhook-driven Slack diagnosis in under 10 minutes, with zero manual JSON-editing.

This scenario is the wedge demo. If this path doesn't work end-to-end, nothing else matters.

```mermaid
sequenceDiagram
    autonumber
    participant Op as User (laptop)
    participant CLI as agentsfleet
    participant API as agentsfleetd-api
    participant Runner as agentsfleet-runner
    participant GH as GitHub Actions
    participant Slack

    Op->>CLI: login (Clerk OAuth) + doctor --json (health)
    CLI->>API: GET /v1/me + workspace binding
    API-->>CLI: { server_reachable ✓, workspace_selected ✓, workspace_binding_valid ✓ }
    Op->>CLI: tenant provider show --json
    CLI->>API: GET /v1/.../tenant-provider
    API-->>CLI: { mode, provider, model, context_cap_tokens }
    Note over Op,CLI: doctor returns health only. Posture comes from<br/>tenant provider show (synth-default for John);<br/>billing show carries free-trial state.
    Op->>CLI: credential add (fly, slack, github, upstash)
    CLI->>API: PUT /credentials
    Op->>CLI: install --from .agentsfleet/platform-ops/  (or --template <id>)
    CLI->>API: POST /fleets<br/>{trigger_markdown, source_markdown}
    API->>API: XGROUP CREATE fleet:{id}:events (+ consumer group)
    API-->>CLI: { id, webhook_urls: { github: "..." } }
    Op->>GH: gh api repos/owner/repo/hooks<br/>(events[]=workflow_run, config.url, secret)
    GH-->>Op: { id, active: true }
    Op->>CLI: steer {id} "morning health check"
    CLI->>API: POST /steer
    API->>API: XADD fleet:{id}:events
    Runner->>API: lease (POST /v1/runners/me/leases)
    Runner->>Slack: posts first-pass health summary
    Note over Op,Slack: Hours later, real CD failure...
    GH->>API: POST /v1/webhooks/{fleet_id}/github (HMAC verified)
    API->>API: XADD fleet:{id}:events
    Runner->>API: lease (POST /v1/runners/me/leases)
    Runner->>Slack: posts evidenced diagnosis
```

---

## 1. Cold install (user's laptop)

John installs the CLI and provisions the fleet from his shell — no host-agent and no markdown-skill step. He authors the `platform-ops` bundle locally (a `SKILL.md` + `TRIGGER.md` for the deploy-failure → Slack use case, often drafted with a coding agent's help), or starts from a catalogue template and customizes it.

### 1.1 Install steps

1. **Bootstrap + auth (once per machine).** `curl -fsSL https://agentsfleet.dev | bash` installs the CLI; `agentsfleet login` does the Clerk OAuth. `agentsfleet doctor --json` is the readiness gate — `server_reachable`, `workspace_selected`, `workspace_binding_valid`; on any miss it prints the exact fix (`npm install -g @agentsfleet/cli`, `agentsfleet login`, or `gh auth login -s admin:repo_hook`) and stops. Doctor is the only sanctioned health check.
2. **Author the bundle.** John writes `.agentsfleet/platform-ops/SKILL.md` (operational behaviour in plain English) and `.agentsfleet/platform-ops/TRIGGER.md` (the config below). The Slack channel, production-branch glob, and cron schedule are values *in* the markdown — version-controlled, not install-time prompts. A coding agent (Claude Code, Amp, Codex CLI, OpenCode) can draft these; or `agentsfleet install --template <id>` pulls a curated catalogue bundle and skips authoring entirely (browse with `agentsfleet templates`). Either way, the markdown *is* the configuration.
   ```yaml
   ---
   name: platform-ops
   x-agentsfleet:
     triggers:
       - type: webhook
         source: github
         events: ["workflow_run"]
         signature:
           secret_ref: github
           header: x-hub-signature-256
           prefix: "sha256="
       - type: cron                 # omit the whole entry to skip the periodic sweep
         schedule: "*/30 * * * *"
     model: accounts/fireworks/models/kimi-k2.6
     context:
       context_cap_tokens: 256000   # ← from /_um/da5b6b3810543fe108d816ee972e4ff8/cap.json
       tool_window: auto
       memory_checkpoint_every: 5
       stage_chunk_threshold: 0.75
     credentials: [fly, slack, github, upstash]
     network:
       allow:
         - api.github.com
         - api.fly.io
         - "*.upstash.io"
         - slack.com
     budget:
       daily_dollars: 5
       monthly_dollars: 100
   ---
   <SKILL.md prose body — operational behaviour in plain English>
   ```
   The `model` / `context_cap_tokens` come from `agentsfleet tenant provider show --json` (synth-default for John: `accounts/fireworks/models/kimi-k2.6`, `256000`, `provider: fireworks`); under self-managed posture the bundle carries the `""` / `0` overlay sentinels instead (Scenario 02). `tenant provider show` always carries resolved values, so neither the CLI nor the author calls the model-caps endpoint (`/_um/da5b6b3810543fe108d816ee972e4ff8/cap.json`) directly — that endpoint is consumed by the platform-side resolver and by `agentsfleet tenant provider add`. See [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) §9.
3. **Add credentials.** For each of `fly`, `slack`, `github`, optional `upstash`: `agentsfleet credential add <name> --data @-`, JSON piped on stdin so secret bytes never reach shell history or argv (upsert; skip-if-exists per M45). The `github` body is `{ "api_token": "<PAT>", "webhook_secret": "<base64 32 bytes>" }` — generate the secret with `openssl rand -base64 32` (one per workspace; all GitHub-sourced fleets share it, rotation rotates everywhere). Install rejects a bundle whose declared credentials are absent (`UZ-BUNDLE-003`), so add them first.
4. **Install.** `agentsfleet install --from .agentsfleet/platform-ops/ --json` (or `--template <id>`). The CLI POSTs `{trigger_markdown, source_markdown}`; the API parses frontmatter server-side, derives `name` + `config_json`, persists the row, and atomically `XGROUP CREATE`s the `fleet:{id}:events` stream + consumer group before returning. No restart and no watcher thread (the `fleet:control` watcher was retired at the cutover): a later trigger `XADD`s to `fleet:{id}:events`, and the control plane hands that event to whichever `agentsfleet-runner` leases next. The 201 response carries `{ fleet_id, name, status, webhook_urls: { github: "https://api.agentsfleet.net/v1/webhooks/{id}/github" } }`. The dashboard install form exercises the same wire shape.
5. **Register the webhook on GitHub** — from John's own machine, for each webhook URL the install printed:
   ```bash
   gh api -X POST "repos/${GH_REPO}/hooks" \
     --field name=web --field active=true \
     --field 'events[]=workflow_run' \
     --field "config[url]=https://api.agentsfleet.net/v1/webhooks/{id}/github" \
     --field 'config[content_type]=json' \
     --field "config[secret]=${WEBHOOK_SECRET}"
   ```
   The user's `gh auth` does the work — the platform never holds the user's PAT. Failure modes: `403`/`401` → `gh auth refresh -s admin:repo_hook`; `404` → repo or token wrong; `422 Hook already exists` → idempotent (match on `config.url`). For dashboard creates, the Trigger panel on `/fleets/{id}` renders this exact command pre-filled with the webhook URL and event list.
6. **First steer (smoke test).** `agentsfleet steer {id} "morning health check"` runs the stored playbook against a manual trigger and streams the response inline — the install-time proof that creds, network, sandbox, and Slack are all wired. (Optionally self-verify the webhook first by curling the receiver with an HMAC-SHA256-signed synthetic payload + `X-GitHub-Event: workflow_run`; a 202 confirms the stored `webhook_secret` matches before the first real fire.)

### 1.2 What the first steer actually returns

The "morning health check" is **not** a canned ack. It enters the same reasoning loop as any other event — actor `steer:<user>`, type `chat`, into `fleet:{id}:events`. The SKILL.md prose body teaches the fleet to handle this input by:

- fetching the latest GH Actions runs on `prod_branch_glob`
- fetching Fly app status / last deploy
- fetching Upstash Redis ping if configured
- posting a one-line "all healthy at HH:MM Z" or a real diagnosis to Slack

So the user sees a **real first-pass evidence sweep**, not a "hello world." This is the install-time proof that everything (creds, network, sandbox, slack) is wired correctly. If any of the four `http_request` calls fails, the user sees the failure inline and can fix it before any real production webhook arrives.

The webhook-driven path (next section) and this steer path are the **same reasoning loop**. The asymmetry is purely in the input: the webhook brings a `workflow_run` payload; the steer brings the user's text. The SKILL.md prose decides what to do with whichever input arrives. There is no "install-time mode" vs. "production mode" branch — the runtime never sees that distinction.

---

## 2. First production webhook fires

A few hours later, the user pushes a commit. CD fails on a Fly OOM. GitHub Actions fires `workflow_run.conclusion=failure`. The webhook receiver:

1. Verifies HMAC-SHA256 against the workspace credential `github.webhook_secret` stored during install.
2. Normalises payload → synthetic event envelope (actor=`webhook:github`, type=`webhook`).
3. `XADD fleet:{id}:events *` with the envelope.
4. Returns 202 to GitHub.

A `agentsfleet-runner` leases the event within ≤5s. The lease path (in `agentsfleetd`) walks the credit-pool gate path (the same code path that scenario 03 walks more deeply):

1. INSERT `core.fleet_events` (`status='received'`, `actor='webhook:github'`, `request_json=<normalised payload>`).
2. PUBLISH `fleet:{id}:activity` (`event_received`).
3. **Resolve provider posture.** `tenant_provider.resolveActiveProvider(tenant_id)` returns the synth-default for John (no row): `{mode: "platform", provider: "fireworks", api_key: <fetched from admin workspace vault via platform_llm_keys pointer>, model: "accounts/fireworks/models/kimi-k2.6", context_cap_tokens: 256000}`.
4. **Balance gate.** Estimate = `compute_receive_charge(.platform)` (1¢) + worst-case `compute_stage_charge(.platform, accounts/fireworks/models/kimi-k2.6, ESTIMATE_FLOOR, ESTIMATE_FLOOR)` (~2¢) = ~3¢. John has $10 starter (`balance_nanos=1000`); 1000 ≥ 3 → pass. (See [`./03_balance_gate.md`](./03_balance_gate.md) for the gate-trip case.)
5. **Receive deduct.** UPDATE `tenant_billing` SET `balance_nanos = 1000 - 1 = 999`. INSERT `fleet_execution_telemetry` (`event_id`, `posture='platform'`, `model='accounts/fireworks/models/kimi-k2.6'`, `charge_type='receive'`, `credit_deducted_nanos=1`). One transaction.
6. Approval gate (no destructive tools wired in this fleet) → pass.
7. Resolve `secrets_map` from vault for `fly`, `slack`, `github`, `upstash`. The platform api_key is **not** in `secrets_map`; `resolveActiveProvider`'s resolved provider+key ride the lease on `ExecutionPolicy.provider` + `ExecutionPolicy.api_key` (delivered fresh on every lease, including reclaim), separate from `secrets_map`, and the runner injects them into the NullClaw child for the inference call only.
8. **Run deduct (conservative estimate).** UPDATE `tenant_billing` SET `balance_nanos = 999 - 2 = 997`. INSERT `fleet_execution_telemetry` (`event_id`, `posture='platform'`, `model='accounts/fireworks/models/kimi-k2.6'`, `charge_type='stage'`, `credit_deducted_nanos=2`, `token_count_input=NULL`, `token_count_output=NULL`). Same transaction shape.
9. `agentsfleetd` issues the lease with `policy = ExecutionPolicy{network_policy, tools, secrets_map, provider: "fireworks", api_key: <platform key>, context: {context_cap_tokens: 256000, tool_window: auto, memory_checkpoint_every: 5, stage_chunk_threshold: 0.75, model: "accounts/fireworks/models/kimi-k2.6"}}`. The platform provider key (fetched from the admin workspace vault via the `platform_llm_keys` pointer) is resolved by `agentsfleetd`, delivered on the lease policy, and injected by the runner's NullClaw child for the inference call only — not carried in `secrets_map`. The lease **also carries `instructions`** — the installed fleet's stored `SKILL.md` body, extracted server-side by `FleetSession` — so the runner composes the NullClaw turn from the installed instructions **plus** the event. This is what makes the GitHub deploy-failure path (and the install smoke-test steer below) run the stored playbook on every trigger instead of a generic chat; it is delivered on **each** lease, fresh and reclaim alike (M84_008).
10. The runner forks a sandboxed NullClaw child and runs the event (the webhook payload as the message).

NullClaw runs the SKILL.md prose against the webhook payload. The fleet makes its calls — `http_request GET .../actions/runs/{run_id}/logs`, `http_request GET ${fly.host}/v1/apps/{app}/logs`, etc. — credentials substituted at the tool bridge after sandbox entry. Posts a remediation diagnosis to Slack.

`StageResult{content, token_count_input=820, token_count_output=1040, wall_ms=8210, ttft_ms=320, exit_ok=true}` returns over the Unix socket.

Worker:
- UPDATE `core.fleet_events` (`status='processed'`, `response_text`, `completed_at`).
- UPDATE `fleet_execution_telemetry` run row (the one INSERTed at step 8) SET `token_count_input=820`, `token_count_output=1040`, `wall_ms=8210`. The `credit_deducted_nanos` column does NOT change — the conservative estimate at step 8 is the charge (v3 may add refund-on-actual; see [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) §3).
- UPSERT `core.fleet_sessions` (advance bookmark, clear execution handle).
- PUBLISH `event_complete`.
- XACK.

After this event: `balance_nanos = 997`. Two telemetry rows (`charge_type='receive'` + `charge_type='stage'`), both with `posture='platform'`. The user reads the diagnosis in Slack; later opens `agentsfleet events {id}` (or the dashboard) to see the full evidence trail and the per-charge-type breakdown.

---

## 3. Terminal transcript — what John Doe sees

This is the verbatim end-to-end CLI experience — the commands John types from a cold machine to first steer. His only inputs are the bundle markdown (authored by hand or with a coding agent, or skipped via `--template`), the credential secrets, and the webhook registration.

### 3.1 Cold install through to first steer

```text
$ curl -fsSL https://agentsfleet.dev | bash
  ✓ installed agentsfleet → ~/.agentsfleet/bin/agentsfleet

$ agentsfleet login
  → opened browser for Clerk approval; enter the 6-digit code: ••••••
  ✓ logged in; active workspace ws_01HX… (auto-provisioned at signup)

$ agentsfleet doctor
  server_reachable        ✓
  workspace_selected      ✓
  workspace_binding_valid ✓

$ agentsfleet tenant provider show
  mode: platform   provider: fireworks
  model: accounts/fireworks/models/kimi-k2.6   context_cap_tokens: 256000

$ agentsfleet billing show
  free_trial: active, ends 2026-07-31 (UTC) → runs charged 0 nanos

# Authored .agentsfleet/platform-ops/{SKILL.md,TRIGGER.md} — slack #platform-ops,
# prod branch main, no cron — by hand or with a coding agent's help. (Or skip
# authoring with: agentsfleet install --template <id>.) github webhook_secret
# generated once with: openssl rand -base64 32

$ agentsfleet credential add github --data @- <<'JSON'
{ "api_token": "ghp_…", "webhook_secret": "…" }
JSON
  ✓ credential `github` stored   (also added: fly, slack)

$ agentsfleet install --from .agentsfleet/platform-ops/
  ✓ platform-ops is live.
    fleet_id     = agt_a01HX9N3K…
    webhook_urls = { github: https://api.agentsfleet.net/v1/webhooks/agt_a01HX9N3K…/github }

$ gh api -X POST repos/john-doe/widgetly/hooks \
    --field name=web --field active=true --field 'events[]=workflow_run' \
    --field "config[url]=https://api.agentsfleet.net/v1/webhooks/agt_a01HX9N3K…/github" \
    --field 'config[content_type]=json' --field "config[secret]=$WEBHOOK_SECRET"
  ✓ hook 482389123 registered, active=true

$ agentsfleet steer agt_a01HX9N3K… "morning health check"
  GH Actions runs on main: 12 in last 24h, all green
  Fly app widgetly-prod: healthy, last deploy 6h ago, 2 instances
  Posted to #platform-ops at 09:14 UTC.

# Webhook ready. Next failed workflow_run on john-doe/widgetly wakes the fleet.
```

### 3.2 First production webhook fires (a few hours later)

```text
$ agentsfleet events agt_a01HX9N3K…
EVENT_ID                 ACTOR             STATUS     STARTED              TOKENS  CREDIT
evt_01HX9P7M…           webhook:github    processed  2026-05-01T13:42:01  1840    4¢
evt_01HX9N4P…           steer:john        processed  2026-05-01T09:14:22  1610    4¢
```

John clicks into `evt_01HX9P7M…` in the dashboard and sees the fleet's evidence trail — the `http_request` calls to GitHub run logs and Fly app status, the diagnosis posted to Slack. The credential names appear (`github`, `fly`, `slack`); their secret bytes do not.

### 3.3 Provider posture confirmed by `tenant provider show`

```text
$ agentsfleet tenant provider show
Mode:                platform   (synthesised default — no explicit row)
Provider:            fireworks
Model:               accounts/fireworks/models/kimi-k2.6
Context cap tokens:  256000

ⓘ This is the platform default. To bring your own LLM key:
   op read 'op://<vault>/<item>/api_key' |
     jq -Rn '{provider:"fireworks", api_key: input, model:"accounts/fireworks/models/kimi-k2.6"}' |
     agentsfleet credential add <name> --data @-
   agentsfleet tenant provider add --credential <name>
```

No `core.tenant_providers` row exists for John's tenant; `tenant provider show` reads through the resolver and surfaces the synthesised default, plus an inline pointer at the self-managed setup commands.

---

## 4. What this scenario proves

- Install is CLI-driven; repo, Slack-channel, and branch configuration live in the authored bundle (or a catalogue template), not in install-time prompts. The runtime stays prompt-driven.
- The model→cap lookup is **one external GET per install**, pinned into frontmatter. Adding a new model never requires an agentsfleet release.
- The first steer and the first production webhook hit the **same reasoning loop**. Asymmetry would mean a code-path the SKILL.md author can't reason about — the architecture forbids it.
- Credit deduction goes through the same `fleet_execution_telemetry` insert path under both postures. There is no plan-tier branching — same code path for John (synth-default platform) and any future user on Stripe-purchased credits.

---

## 5. What is NOT in this scenario

- No self-managed. See `scenarios/02_self_managed.md`.
- No balance trip. See `scenarios/03_balance_gate.md`.
- No customer-facing statuspage / external comms. That's the bastion direction documented in [`../roadmap.md`](../roadmap.md#bastion--post-mvp-shape).
- No GitHub App for auto-webhook config. Manual step in v2.
