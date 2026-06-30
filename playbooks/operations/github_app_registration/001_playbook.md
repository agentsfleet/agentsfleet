# M106_001: Playbook — Register the agentsfleet GitHub App (platform private key)

**Milestone:** M106
**Workstream:** 001 (§6.2 deliverable — documents the GitHub App used by M102_001 agent-identity proxy)
**Updated:** Jun 30, 2026
**Prerequisite:** `op` CLI authenticated; the `agentsfleet-admin` tenant API key in vault (`operations/admin_bootstrap/001_playbook.md`). A GitHub user or org where you may create a GitHub App.

Registers **one** GitHub App and stores its **platform** credentials — App ID, the RS256 (RSA Signature with SHA-256) private key, `client_id`, `client_secret` — in the `agentsfleet-admin` workspace vault, resolved daemon-side via `crypto_store.load`. The daemon signs an App JSON Web Token (JWT) with the private key and exchanges it at GitHub for a short-lived installation access token at the credential broker (`POST /v1/runners/me/credentials/mint`); **the App private key never leaves the daemon** — not the lease envelope, not the `secrets_map`, not the sandbox child (see `docs/architecture/capabilities.md` §3, broker row). Only the `ENCRYPTION_MASTER_KEY` Key-Encryption Key (KEK) lives in env; the App private key is real bytes in `vault.secrets` under the admin workspace.

> **Run once per environment.** This is the Stage-0 platform setup behind the GitHub mintable integration; the per-customer **installation** of the App on the customer's repos is a separate, customer-driven step.

---

## Human vs Agent Split

| Step | Owner | What |
|------|-------|------|
| 0.0 | Agent | Resolve environment; load the admin API key |
| 1.0 | Human | Create the GitHub App (permissions, callback, webhook posture) |
| 2.0 | Human | Generate the private key; note App ID / client_id / client_secret |
| 3.0 | Agent | Store App ID + private key + client creds as platform secret `github-app` |
| 4.0 | Agent | Verify the broker can mint an installation token (or resolve the secret) |

---

## 0.0 Agent: Resolve environment and load the admin key

```bash
export ENV="prod"   # or: export ENV="dev"
case "$ENV" in
  dev)  export VAULT="ZMB_CD_DEV";  export API_BASE="https://api-dev.agentsfleet.net" ;;
  prod) export VAULT="ZMB_CD_PROD"; export API_BASE="https://api.agentsfleet.net" ;;
  *)    echo "ENV must be dev|prod"; exit 1 ;;
esac
export ADMIN_KEY=$(op read "op://$VAULT/agentsfleet-admin/api_key")
[[ "$ADMIN_KEY" =~ ^agt_t[0-9a-f]{64}$ ]] || { echo "missing admin key"; exit 1; }
```

### Acceptance

`ADMIN_KEY` well-formed; `$API_BASE/healthz` returns 200.

---

## 1.0 Human: Create the GitHub App

**Goal:** an App scoped to what the fleets actually do, with no broker-irrelevant callback. At **github.com/settings/apps/new** (or `https://github.com/organizations/<org>/settings/apps/new` for an org-owned App):

1. **GitHub App name** — `agentsfleet` (or `agentsfleet-<env>`); **Homepage URL** `https://agentsfleet.net`.
2. **Callback URL** — `https://api.agentsfleet.net/v1/integrations/github/oauth/callback` (the connector callback; swap `$API_BASE` in dev).
3. **Webhook** — **uncheck Active.** The broker is outbound-only (it mints tokens for the fleet's own calls); the GitHub Actions deploy trigger is a **separate per-fleet webhook the customer registers** (`docs/architecture/user_flow.md` §8.5), not this App-level webhook.
4. **Repository permissions** — scope minimally to the fleets you ship:
   - **Metadata:** Read-only (mandatory).
   - **Contents:** Read & write (PR-opening fleets) or Read-only (review-only).
   - **Pull requests:** Read & write (review/PR fleets).
   - add Issues / Checks only if a shipped fleet needs them.
5. **Where can this App be installed?** — Any account (multi-tenant) for the hosted product.

### Acceptance

App created; webhook **inactive**; permissions match the shipped fleet set.

---

## 2.0 Human: Generate the private key and note the IDs

**Goal:** capture the App identity + a fresh signing key. On the App's page:

1. Note the **App ID** (integer) and **Client ID**.
2. **Generate a new client secret** — note `client_secret`.
3. **Generate a private key** — downloads a `.pem` (Privacy-Enhanced Mail) RSA key. This file is the only copy; it goes to vault in §3 and is then shredded.

### Acceptance

App ID, Client ID, `client_secret`, and a downloaded `.pem` in hand.

---

## 3.0 Agent: Store the platform secret in the admin vault

**Goal:** persist the App identity + private key as one platform credential `github-app` in the `agentsfleet-admin` workspace vault. The multi-line PEM flows via a file → `jq` → stdin, never argv (RULE VLT). Then destroy the local key file.

```bash
PEM_PATH="${1:?path to the downloaded .pem}"
jq -n \
  --arg app_id "$(read -rp 'App ID: ' v; echo "$v")" \
  --arg client_id "$(read -rp 'Client ID: ' v; echo "$v")" \
  --arg client_secret "$(read -rsp 'client_secret: ' v; echo "$v")" \
  --arg private_key "$(cat "$PEM_PATH")" \
  '{app_id:$app_id, client_id:$client_id, client_secret:$client_secret, private_key:$private_key}' |
  AGENTSFLEET_API_KEY="$ADMIN_KEY" agentsfleet credential add github-app --data @-

# Destroy the local key copy — vault is now the only source of truth.
command -v shred >/dev/null && shred -u "$PEM_PATH" || rm -P "$PEM_PATH" 2>/dev/null || trash "$PEM_PATH"
```

> Prefer mirroring the PEM + IDs into `op://$VAULT/github-app/*` first (so a vault rotation can re-issue), then pipe `op read` → `jq` → `agentsfleet credential add` (admin_bootstrap §7's `credential set` is the older verb for the same `--data @-` stdin pattern). Re-run with `--force` to overwrite an existing `github-app`.

### Acceptance

`agentsfleet credential set` exits 0; the `.pem` no longer exists on disk; no key bytes in shell history, argv, or output.

---

## 4.0 Agent: Verify the broker can use it

**Goal:** the daemon resolves `github-app` and (on a real install) mints an installation token.

```bash
# Metadata only — never the private key:
AGENTSFLEET_API_KEY="$ADMIN_KEY" agentsfleet credential show github-app --json | jq '{name,kind}'
# End-to-end: install the App on a test repo, run a fleet whose http_request hits
# api.github.com with ${secrets.github.api_token}; the tool bridge mints a ≤1h
# installation token at POST /v1/runners/me/credentials/mint. A 200 from GitHub
# confirms the App JWT signed and exchanged correctly.
```

### Acceptance

`credential show` returns `github-app` metadata with no key material; a fleet's GitHub call succeeds against a test installation.

---

## Rollback

1. `agentsfleet credential delete github-app` (admin key) to drop the platform secret.
2. GitHub App page → **Generate a new private key** (rotation) and re-run §3, **or** Advanced → **Delete GitHub App** (invalidates every installation; re-install per customer afterward).
3. Customer installations of the App are independent of the stored private key; a key rotation keeps installs valid, a full delete revokes them.
