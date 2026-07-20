# M106_001: Playbook — Register the `@agentsfleet` Slack App (platform secrets)

**Milestone:** M106
**Workstream:** 001 (§6.1 deliverable)
**Updated:** Jun 30, 2026
**Prerequisite:** The M106 ingress (`POST /v1/connectors/slack/events`, `GET /v1/connectors/slack/callback`) is **deployed and reachable** at `$API_BASE` — Slack verifies the events Request URL with a live `url_verification` challenge, so the handler must answer before this playbook can complete. `op` CLI authenticated; the `agentsfleet-admin` tenant API key and agentsfleet-owned `approval-signing-secret/credential` in the environment vault (see `operations/admin_bootstrap/001_playbook.md`). The deployment must load that value as `APPROVAL_SIGNING_SECRET` before any connector callback can be minted or verified. Slack workspace where you can create apps.

Registers **one** multi-tenant Slack app and stores its **platform** credentials (`client_id`, `client_secret`, `signing_secret`) in the `agentsfleet-admin` workspace vault, resolved daemon-side via `crypto_store.load` — the same model as the platform LLM key (admin_bootstrap §7) and the GitHub App private key (`github_app_registration`). This is the Stage-0 one-time setup; the per-customer bot token (`xoxb`) is minted later at OAuth-install time and stored in **the customer's** workspace vault under the `fleet:slack` handle — it is **not** handled here.

> **Run once per environment**, when M106 has shipped. Re-running is idempotent on the vault write (§6); the Slack app itself is edited in place at `api.slack.com/apps`.

---

## Human vs Agent Split

| Step | Owner | What |
|------|-------|------|
| 0.0 | Agent | Resolve environment; load the admin API key from vault |
| 1.0 | Human | Create the Slack app from the manifest at `api.slack.com/apps` |
| 2.0 | Human | Confirm bot scopes + event subscription (carried by the manifest) |
| 3.0 | Human | Verify the events Request URL (Slack pings the live handler) |
| 4.0 | Human | Copy `client_id` · `client_secret` · `signing_secret` from Basic Information |
| 5.0 | Agent | Store the three as a platform secret `slack-app` in the `agentsfleet-admin` vault |
| 6.0 | Agent | Verify the daemon resolves `slack-app` and the events URL is `verified` |

Steps 1–4 are browser-interactive; 5–6 run unattended.

---

## 0.0 Agent: Resolve environment and load the admin key

**Goal:** pick dev or prod and read the `agentsfleet-admin` API key + base URL from vault.

```bash
export ENV="prod"   # or: export ENV="dev"
case "$ENV" in
  dev)  export VAULT="ZMB_CD_DEV";  export API_BASE="https://api-dev.agentsfleet.net" ;;
  prod) export VAULT="ZMB_CD_PROD"; export API_BASE="https://api.agentsfleet.net" ;;
  *)    echo "ENV must be dev|prod"; exit 1 ;;
esac
export ADMIN_KEY=$(op read "op://$VAULT/agentsfleet-admin/api-key")
[[ "$ADMIN_KEY" =~ ^agt_t[0-9a-f]{64}$ ]] || { echo "missing admin key"; exit 1; }
curl -sf -o /dev/null "$API_BASE/healthz" || { echo "$API_BASE unreachable"; exit 1; }
```

### Acceptance

`$API_BASE/healthz` returns 200; `ADMIN_KEY` is a well-formed `agt_t` key.

---

## 1.0 Human: Create the app from a manifest

**Goal:** one app, scoped exactly to the Rung-0 reactive surface. At `api.slack.com/apps` → **Create New App → From a manifest**, pick your workspace, and paste (swap `api.agentsfleet.net` for `$API_BASE` in dev):

```yaml
display_information:
  name: agentsfleet
features:
  bot_user:
    display_name: agentsfleet
    always_online: true
oauth_config:
  redirect_urls:
    - https://api.agentsfleet.net/v1/connectors/slack/callback
  scopes:
    bot:
      - app_mentions:read   # hear @agentsfleet
      - chat:write          # reply in-thread
      - chat:write.public   # reply in channels it isn't a member of
      - channels:history    # read the mentioned thread's recent context
      - users:read          # resolve the mentioning user
settings:
  event_subscriptions:
    request_url: https://api.agentsfleet.net/v1/connectors/slack/events
    bot_events:
      - app_mention
  org_deploy_enabled: false
  socket_mode_enabled: false
```

Use `agentsfleet-dev` for both `display_information.name` and `features.bot_user.display_name` in development. Reserve `agentsfleet` and `@agentsfleet` for production so installing both apps in one Slack workspace cannot create ambiguous mentions or duplicate handling.

> **Scope discipline (RULE PRI / privacy):** do **not** add `message.channels` or any channel-wide read. The bot learns from interaction, not surveillance. Interactivity, slash commands, and DM scopes (`im:write`) are **Rung-1** and deliberately absent.

### Acceptance

The app exists; the manifest applied with no scope warnings.

---

## 1.1 Human: Enable production distribution

**Goal:** customer workspaces can authorize the production app without receiving platform credentials. In the production Slack app, open **Manage Distribution**, complete Slack's required app details and OAuth checks, and activate public distribution. Marketplace listing is not required for the agentsfleet **Connect Slack** button; public distribution is.

Keep the development app restricted to the agentsfleet test workspace unless a cross-workspace development proof explicitly requires distribution.

### Acceptance

The production app can be installed into a different authorized Slack workspace through OAuth; the development app remains visibly named `agentsfleet-dev`.

---

## 2.0 Human: Confirm scopes + event subscription

**Goal:** the manifest carried the scopes and the single `app_mention` subscription. Open **OAuth & Permissions** (bot scopes match the manifest) and **Event Subscriptions** (one bot event: `app_mention`).

### Acceptance

Bot Token Scopes list exactly the five above; Subscribe-to-bot-events lists only `app_mention`.

---

## 3.0 Human: Verify the events Request URL

**Goal:** Slack confirms the live handler answers its `url_verification` challenge.

In **Event Subscriptions**, the Request URL shows **Verified** once the M106 handler echoes the `challenge`. If it shows **Failed**, the ingress is not deployed/reachable — fix `$API_BASE` reachability before continuing (this is the prerequisite).

### Acceptance

Event Subscriptions Request URL state is **Verified** (green).

---

## 4.0 Human: Copy the platform credentials

**Goal:** capture the three app-level secrets. From **Basic Information → App Credentials**: `client_id`, `client_secret`, and `signing_secret`. Keep them off-screen; they go straight to vault in §5 (never paste into chat, a ticket, or a shell argument).

### Acceptance

All three values captured.

---

## 5.0 Agent: Store the platform secret in the admin vault

**Goal:** persist the three as one platform credential `slack-app` in the `agentsfleet-admin` workspace vault — the daemon resolves it via `crypto_store.load` for signature verification and the OAuth exchange. The values flow through stdin, never argv (RULE VLT). Paste the values at the prompts of a JSON-shaping helper so they never touch shell history:

```bash
jq -n \
  --arg cid "$(cid=$(op read "op://$VAULT/slack-app/client_id" 2>/dev/null) || read -rsp 'client_id: ' cid >&2; printf '%s' "$cid")" \
  --arg sec "$(read -rsp 'client_secret: ' sec >&2; printf '%s' "$sec")" \
  --arg sig "$(read -rsp 'signing_secret: ' sig >&2; printf '%s' "$sig")" \
  '{client_id:$cid, client_secret:$sec, signing_secret:$sig}' |
  AGENTSFLEET_API_KEY="$ADMIN_KEY" agentsfleet secret create slack-app --force --data @-
```

> Prefer mirroring `op://$VAULT/slack-app/{client_id,client_secret,signing_secret}` into 1Password first, then piping `op read` → `jq` → `agentsfleet secret create --force` so the source of truth survives a vault rotation.

### Acceptance

`agentsfleet secret create` exits 0. No secret appears in shell history, process argv, or output.

---

## 6.0 Agent: Verify resolution end-to-end

**Goal:** the daemon resolves `slack-app`, and a signed `app_mention` is accepted.

```bash
# Daemon can see the platform credential (metadata only — never the secret bytes):
AGENTSFLEET_API_KEY="$ADMIN_KEY" agentsfleet secret show slack-app --json | jq '{name,kind}'
# Live check: post a real @agentsfleet in a test channel the bot was invited to.
# Expect: an in-thread reply within a few seconds; the event lands in core.fleet_events.
```

### Acceptance

`secret show` returns the `slack-app` metadata with no secret material; a real `@agentsfleet` mention is answered in-thread.

---

## Rollback

1. `agentsfleet secret delete slack-app` (admin key) to drop the platform secret.
2. At `api.slack.com/apps` → the app → **Basic Information → Delete App** (or rotate `client_secret`/`signing_secret` and re-run §5).
3. Existing per-customer installs (`core.connector_installs` rows + the per-workspace `fleet:slack` vault handles) are unaffected by an app-secret rotation; a full app delete invalidates them — re-install per customer afterward.
