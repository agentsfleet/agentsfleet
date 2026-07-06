# M115_001: Playbook — Register the agentsfleet Linear App (platform secrets)

**Milestone:** M115
**Workstream:** 001 (§2.3 deliverable)
**Updated:** Jul 06, 2026
**Prerequisite:** `op` CLI authenticated; the `agentsfleet-admin` tenant API key in vault (`operations/admin_bootstrap/001_playbook.md`). A Linear workspace where you can create an OAuth application.

Registers **one** multi-tenant Linear OAuth application and stores its **platform** credentials (`client_id`, `client_secret`) in the `agentsfleet-admin` workspace vault, resolved daemon-side via `crypto_store.load` — the same model as the Slack/GitHub platform app secrets. Linear is an **OAuth 2.0 + refresh** connector: the broker uses `linear-app`'s `client_id`/`client_secret` both for the initial code exchange and for every later refresh mint (`credentials/integration.zig`'s `selectLinear`).

> **Run once per environment.** Re-running is idempotent on the vault write (§4); the Linear app itself is edited in place at `linear.app/settings/api/applications`.

---

## Human vs Agent Split

| Step | Owner | What |
|------|-------|------|
| 0.0 | Agent | Resolve environment; load the admin API key from vault |
| 1.0 | Human | Create an OAuth application in Linear's workspace settings |
| 2.0 | Human | Copy `client_id` · `client_secret` from the application's details |
| 3.0 | Agent | Store the two as a platform secret `linear-app` in the `agentsfleet-admin` vault |
| 4.0 | Agent | Verify the daemon resolves `linear-app` and a live connect completes |

Steps 1–2 are browser-interactive; 3–4 run unattended.

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
export ADMIN_KEY=$(op read "op://$VAULT/agentsfleet-admin/api_key")
[[ "$ADMIN_KEY" =~ ^agt_t[0-9a-f]{64}$ ]] || { echo "missing admin key"; exit 1; }
curl -sf -o /dev/null "$API_BASE/healthz" || { echo "$API_BASE unreachable"; exit 1; }
```

### Acceptance

`$API_BASE/healthz` returns 200; `ADMIN_KEY` is a well-formed `agt_t` key.

---

## 1.0 Human: Create the Linear OAuth application

**Goal:** one OAuth application scoped to the read + offline-access shape the connector requests. At **linear.app** → workspace **Settings → API → OAuth applications → Create new**:

1. **Name** — `agentsfleet` (or `agentsfleet-<env>`); **Developer URL** `https://agentsfleet.net`.
2. **Callback URL(s)**:

```
https://api.agentsfleet.net/v1/connectors/linear/callback
```

(swap `api.agentsfleet.net` for `$API_BASE` in dev)

3. Leave the application **public/confidential** setting at its default (confidential) — the exchange runs server-side with `client_secret`, never in a browser.

### Acceptance

The application exists; the callback URL is saved exactly as above.

---

## 2.0 Human: Copy the client credentials

**Goal:** capture `client_id` and `client_secret` from the application's details page. Keep them off-screen; they go straight to vault in §3 (never paste into chat, a ticket, or a shell argument).

### Acceptance

Both values captured.

---

## 3.0 Agent: Store the platform secret in the admin vault

**Goal:** persist the two as one platform credential `linear-app` in the `agentsfleet-admin` workspace vault. The values flow through stdin, never argv (RULE VLT).

```bash
jq -n \
  --arg cid "$(read -rp  'client_id: '     cid; printf '%s' "$cid")" \
  --arg sec "$(read -rsp 'client_secret: ' sec; printf '%s' "$sec")" \
  '{client_id:$cid, client_secret:$sec}' |
  AGENTSFLEET_API_KEY="$ADMIN_KEY" agentsfleet secret add linear-app --data @-
```

> Prefer mirroring `op://$VAULT/linear-app/{client_id,client_secret}` into 1Password first, then piping `op read` → `jq` → `agentsfleet secret add` so the source of truth survives a vault rotation. Re-run with `--force` to overwrite an existing `linear-app`.

### Acceptance

`agentsfleet secret add` exits 0. No secret appears in shell history, process argv, or output.

---

## 4.0 Agent: Verify resolution end-to-end

**Goal:** the daemon resolves `linear-app`, and a live connect completes.

```bash
# Daemon can see the platform credential (metadata only — never the secret bytes):
AGENTSFLEET_API_KEY="$ADMIN_KEY" agentsfleet secret show linear-app --json | jq '{name,kind}'
# Live check: connect a test Linear workspace from the dashboard's Integrations page.
# Expect: the browser round-trip completes at linear.app/oauth/authorize and the
# workspace's fleet:linear vault handle is written with a refresh token.
```

### Acceptance

`secret show` returns the `linear-app` metadata with no secret material; a real Linear connect completes and `fleet:linear` is vaulted.

---

## Rollback

1. `agentsfleet secret delete linear-app` (admin key) to drop the platform secret.
2. At **linear.app** → workspace **Settings → API** → the application → regenerate the client secret (and re-run §3), or delete the application entirely.
3. Existing per-workspace installs (`fleet:linear` vault handles) are unaffected by a client-secret rotation; a full application delete breaks future refresh mints for every connected workspace — reconnect per workspace afterward.
