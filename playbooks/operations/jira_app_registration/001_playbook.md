# M115_001: Playbook — Register the agentsfleet Jira App (platform secrets)

**Milestone:** M115
**Workstream:** 001 (§2.2 deliverable)
**Updated:** Jul 06, 2026
**Prerequisite:** `op` CLI authenticated; the `agentsfleet-admin` tenant API key in vault (`operations/admin_bootstrap/001_playbook.md`). An Atlassian account where you can register an OAuth 2.0 (3LO) app.

Registers **one** multi-tenant Jira Cloud OAuth 2.0 (3LO — three-legged OAuth) app and stores its **platform** credentials (`client_id`, `client_secret`) in the `agentsfleet-admin` workspace vault, resolved daemon-side via `crypto_store.load` — the same model as the Slack/GitHub platform app secrets. Jira is an **OAuth 2.0 + refresh** connector: the broker uses `jira-app`'s `client_id`/`client_secret` both for the initial code exchange and for every later refresh mint (`credentials/integration.zig`'s `selectJira`).

> **Run once per environment.** Re-running is idempotent on the vault write (§4); the Jira app itself is edited in place at `developer.atlassian.com`.

---

## Human vs Agent Split

| Step | Owner | What |
|------|-------|------|
| 0.0 | Agent | Resolve environment; load the admin API key from vault |
| 1.0 | Human | Create an OAuth 2.0 (3LO) app in the Atlassian Developer Console |
| 2.0 | Human | Add the Jira platform permissions/scopes |
| 3.0 | Human | Copy `client_id` · `client_secret` from the app's Settings |
| 4.0 | Agent | Store the two as a platform secret `jira-app` in the `agentsfleet-admin` vault |
| 5.0 | Agent | Verify the daemon resolves `jira-app` and a live connect completes |

Steps 1–3 are browser-interactive; 4–5 run unattended.

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

## 1.0 Human: Create the Atlassian OAuth 2.0 (3LO) app

**Goal:** one app registered against the `api.atlassian.com` audience. At **developer.atlassian.com/console/myapps** → **Create → OAuth 2.0 integration**:

1. **App name** — `agentsfleet-dev` in development and `agentsfleet` in production.
2. Select **Resource-level grant** so each authorization is limited to the Jira site selected on the consent screen. The callback stores one resolved `cloud_id`; do not use an account-level grant that can return multiple sites.
3. Under **Authorization** → **OAuth 2.0 (3LO)**, set the **Callback URL**:

```
https://api.agentsfleet.net/v1/connectors/jira/callback
```

(swap `api.agentsfleet.net` for `$API_BASE` in dev)

### Acceptance

The app exists; the callback URL is saved exactly as above.

---

## 2.0 Human: Add scopes

**Goal:** the app can read and reply on both Jira issues and Jira Service Management customer requests. In **Permissions**, add both **Jira platform REST API** and **Jira Service Management API**, then select the classic (non-granular) scopes `read:jira-work`, `read:jira-user`, `write:jira-work`, `read:servicedesk-request`, and `write:servicedesk-request`. The connector requests `offline_access` dynamically for refresh tokens; it may not appear in the permissions selector.

Do not add administrative, project-management, or user-management scopes. The two write scopes permit issue/request replies and updates; expanding beyond them requires a separate reviewed product change.

> **Cloud id note:** you do **not** configure a Jira **cloud id** anywhere in this app registration. Each connecting workspace's Jira site is looked up automatically at callback time via Atlassian's accessible-resources endpoint (`https://api.atlassian.com/oauth/token/accessible-resources`) and persisted on the vaulted handle as `cloud_id`/`site_url` — there is no operator step for it.

### Acceptance

The app uses a resource-level grant. Its Permissions tab shows exactly `read:jira-work read:jira-user write:jira-work read:servicedesk-request write:servicedesk-request`; the callback URL is environment-correct.

---

## 2.1 Human: Enable production sharing

**Goal:** users outside the app owner's Atlassian account can authorize the production integration. In the production app's **Distribution** settings, enable sharing and complete Atlassian's required vendor, privacy-policy, and terms fields. A Marketplace listing is not required for the agentsfleet **Connect Jira** button.

Keep the development app private unless cross-site testing requires sharing. Sharing changes who may authorize the app; it does not expose the client secret.

### Acceptance

The production app reports sharing/distribution enabled and its consent flow allows a different Atlassian site to be selected under the resource-level grant.

---

## 3.0 Human: Copy the client credentials

**Goal:** capture `client_id` and `client_secret` from the app's **Settings** tab. Keep them off-screen; they go straight to vault in §4 (never paste into chat, a ticket, or a shell argument).

### Acceptance

Both values captured.

---

## 4.0 Agent: Store the platform secret in the admin vault

**Goal:** persist the two as one platform credential `jira-app` in the `agentsfleet-admin` workspace vault. The values flow through stdin, never argv (RULE VLT).

```bash
jq -n \
  --arg cid "$(read -rp  'client_id: '     cid; printf '%s' "$cid")" \
  --arg sec "$(read -rsp 'client_secret: ' sec; printf '%s' "$sec")" \
  '{client_id:$cid, client_secret:$sec}' |
  AGENTSFLEET_API_KEY="$ADMIN_KEY" agentsfleet secret create jira-app --data @-
```

> Prefer mirroring `op://$VAULT/jira-app/{client_id,client_secret}` into 1Password first, then piping `op read` → `jq` → `agentsfleet secret create` so the source of truth survives a vault rotation. Re-run with `--force` to overwrite an existing `jira-app`.

### Acceptance

`agentsfleet secret create` exits 0. No secret appears in shell history, process argv, or output.

---

## 5.0 Agent: Verify resolution end-to-end

**Goal:** the daemon resolves `jira-app`, and a live connect completes.

```bash
# Daemon can see the platform credential (metadata only — never the secret bytes):
AGENTSFLEET_API_KEY="$ADMIN_KEY" agentsfleet secret show jira-app --json | jq '{name,kind}'
# Live check: connect a test Jira Cloud site from the dashboard's Integrations page.
# Expect: the browser round-trip completes, auth.atlassian.com issues a code, and
# the workspace's fleet:jira vault handle is written with a resolved cloud_id/site_url.
```

### Acceptance

`secret show` returns the `jira-app` metadata with no secret material; a real Jira connect completes and `fleet:jira` is vaulted with a resolved `cloud_id`.

---

## Rollback

1. `agentsfleet secret delete jira-app` (admin key) to drop the platform secret.
2. In the Atlassian Developer Console → the app → **Settings** → regenerate the client secret (and re-run §4), or delete the app entirely.
3. Existing per-workspace installs (`fleet:jira` vault handles) are unaffected by a client-secret rotation; a full app delete breaks future refresh mints for every connected workspace — reconnect per workspace afterward.
