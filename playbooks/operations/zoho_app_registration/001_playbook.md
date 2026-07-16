# M115_001: Playbook — Register the agentsfleet Zoho Desk App (platform secrets)

**Milestone:** M115
**Workstream:** 001 (§2.1 deliverable)
**Updated:** Jul 06, 2026
**Prerequisite:** `op` CLI authenticated; the `agentsfleet-admin` tenant API key in vault (`operations/admin_bootstrap/001_playbook.md`). A Zoho account where you can register a client in the Zoho API Console.

Registers **one** multi-tenant Zoho Desk OAuth client and stores its **platform** credentials (`client_id`, `client_secret`) in the `agentsfleet-admin` workspace vault, resolved daemon-side via `crypto_store.load` — the same model as the Slack/GitHub platform app secrets. Zoho Desk is an **OAuth 2.0 + refresh** connector: the broker uses `zoho-app`'s `client_id`/`client_secret` both for the initial code exchange and for every later refresh mint (`credentials/integration.zig`'s `selectZoho`).

> **Run once per environment.** Re-running is idempotent on the vault write (§4); the Zoho client itself is edited in place at `api-console.zoho.com`.

---

## Human vs Agent Split

| Step | Owner | What |
|------|-------|------|
| 0.0 | Agent | Resolve environment; load the admin API key from vault |
| 1.0 | Human | Create a Server-based Application client in the Zoho API Console |
| 2.0 | Human | Set the authorized redirect URI to the agentsfleet callback |
| 3.0 | Human | Copy `client_id` · `client_secret` from the client's details |
| 4.0 | Agent | Store the two as a platform secret `zoho-app` in the `agentsfleet-admin` vault |
| 5.0 | Agent | Verify the daemon resolves `zoho-app` and a live connect completes |

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

## 1.0 Human: Create the Zoho API Console client

**Goal:** one Server-based Application client, scoped to Zoho Desk read access. At **api-console.zoho.com** → **Add Client → Server-based Applications**:

1. **Client Name** — `agentsfleet` (or `agentsfleet-<env>`); **Homepage URL** `https://agentsfleet.net`.
2. Leave scopes unset here — Zoho Desk grants scopes at authorize time, not on the client itself. The platform requests `Desk.organization.READ,Desk.basic.READ` per connect (`connectors/zoho/spec.zig`).

### Acceptance

The client exists in the API Console's client list.

---

## 2.0 Human: Set the authorized redirect URI

**Goal:** register the single agentsfleet callback URL. **Zoho's authorize step always starts at the US accounts server** (`accounts.zoho.com`) regardless of which data center the connecting org actually lives in — so only **one** redirect URI is needed here, not one per region:

```
https://api.agentsfleet.net/v1/connectors/zoho/callback
```

(swap `api.agentsfleet.net` for `$API_BASE` in dev)

> **Multi-datacenter note:** the *region* (US/EU/India/Australia/China/Japan/Canada) is resolved automatically from the callback's `location` query parameter — the connector's post-auth hook picks the matching regional accounts server for the token exchange and persists it on the vaulted handle (`connectors/zoho/multi_dc.zig`) for every future refresh. You do not configure per-region redirect URIs or endpoints; this is transparent to both the operator and the connecting workspace.

### Acceptance

The redirect URI is saved and exactly matches the callback path (a mismatch fails the exchange with `invalid redirect_uri` at Zoho, not at agentsfleet).

---

## 3.0 Human: Copy the client credentials

**Goal:** capture `client_id` and `client_secret` from the client's **Client Secret** tab. Keep them off-screen; they go straight to vault in §4 (never paste into chat, a ticket, or a shell argument).

### Acceptance

Both values captured.

---

## 4.0 Agent: Store the platform secret in the admin vault

**Goal:** persist the two as one platform credential `zoho-app` in the `agentsfleet-admin` workspace vault. The values flow through stdin, never argv (RULE VLT).

```bash
jq -n \
  --arg cid "$(read -rp  'client_id: '     cid; printf '%s' "$cid")" \
  --arg sec "$(read -rsp 'client_secret: ' sec; printf '%s' "$sec")" \
  '{client_id:$cid, client_secret:$sec}' |
  AGENTSFLEET_API_KEY="$ADMIN_KEY" agentsfleet secret create zoho-app --data @-
```

> Prefer mirroring `op://$VAULT/zoho-app/{client_id,client_secret}` into 1Password first, then piping `op read` → `jq` → `agentsfleet secret create` so the source of truth survives a vault rotation. Re-run with `--force` to overwrite an existing `zoho-app`.

### Acceptance

`agentsfleet secret create` exits 0. No secret appears in shell history, process argv, or output.

---

## 5.0 Agent: Verify resolution end-to-end

**Goal:** the daemon resolves `zoho-app`, and a live connect completes.

```bash
# Daemon can see the platform credential (metadata only — never the secret bytes):
AGENTSFLEET_API_KEY="$ADMIN_KEY" agentsfleet secret show zoho-app --json | jq '{name,kind}'
# Live check: connect a test Zoho Desk org from the dashboard's Integrations page.
# Expect: the browser round-trip completes and the workspace's fleet:zoho vault
# handle is written with an accounts_base matching the org's actual region.
```

### Acceptance

`secret show` returns the `zoho-app` metadata with no secret material; a real Zoho Desk connect completes and `fleet:zoho` is vaulted with the org's data-center-correct `accounts_base`.

---

## Rollback

1. `agentsfleet secret delete zoho-app` (admin key) to drop the platform secret.
2. In the Zoho API Console → the client → regenerate the client secret (and re-run §4), or delete the client entirely.
3. Existing per-workspace installs (`fleet:zoho` vault handles) are unaffected by a client-secret rotation; a full client delete breaks future refresh mints for every connected workspace — reconnect per workspace afterward.
