# M106_001: Playbook — Register the agentsfleet GitHub App (platform private key)

**Milestone:** M106
**Workstream:** 001 (§6.2 deliverable — documents the GitHub App used by M102_001 agent-identity proxy)
**Updated:** Jul 11, 2026
**Prerequisite:** `op` Command-Line Interface (CLI) authenticated; the `agentsfleet-admin` tenant API key in vault (`operations/admin_bootstrap/001_playbook.md`). A GitHub user or organisation where you may create a GitHub App.

Registers **one** GitHub App and stores its platform identity and inbound secret in the `agentsfleet-admin` workspace vault as `github-app` `{app_id, private_key_pem, app_slug, webhook_secret, client_id, client_secret}`. The private key signs App JSON Web Tokens (JWTs) for outbound installation-token minting; the distinct webhook secret verifies inbound App deliveries at `/v1/ingress/github`; the client credentials let the callback prove that the returning GitHub user may access the claimed installation. None of the secret fields leaves the daemon. Only `app_id`, `app_slug`, and `client_id` are public identifiers.

> **Run once per environment.** This is the Stage-0 platform setup behind the GitHub mintable integration; the per-customer **installation** of the App on the customer's repos is a separate, customer-driven step.

> **Verification status:** the callback and App-ingress suites pass against local Postgres and Redis, but `github-pr-reviewer` is not considered fixed until §4 passes against a real repository. Local datastore proof does not replace repository-level acceptance.

---

## Human vs Agent Split

| Step | Owner | What |
|------|-------|------|
| 0.0 | Agent | Resolve environment; load the admin API key |
| 1.0 | Human | Create the GitHub App (permissions, callback, webhook, events) |
| 2.0 | Human | Generate the private key and webhook secret; note public identifiers |
| 3.0 | Agent | Store the six-field platform bag as `github-app` |
| 4.0 | Agent | Verify callback mapping, event ingress, and token minting |

---

## 0.0 Agent: Resolve environment and load the admin key

```bash
export ENV="prod"   # or: export ENV="dev"
case "$ENV" in
  dev)  export VAULT="ZMB_CD_DEV";  export API_BASE="https://api-dev.agentsfleet.net" ;;
  prod) export VAULT="ZMB_CD_PROD"; export API_BASE="https://api.agentsfleet.net" ;;
  *)    echo "ENV must be dev|prod"; exit 1 ;;
esac
export ADMIN_KEY=$(op read "op://$VAULT/agentsfleet-admin/api-key")
[[ "$ADMIN_KEY" =~ ^agt_t[0-9a-f]{64}$ ]] || { echo "missing admin key"; exit 1; }
```

### Acceptance

`ADMIN_KEY` well-formed; `$API_BASE/healthz` returns 200.

---

## 1.0 Human: Create the GitHub App

**Goal:** one multi-tenant App scoped to what fleets actually do. At **github.com/settings/apps/new** (or `https://github.com/organizations/<org>/settings/apps/new` for an organisation-owned App):

1. **GitHub App name** — `agentsfleet` (or `agentsfleet-<env>`); **Homepage URL** `https://agentsfleet.net`.
2. **Callback URL** — `https://api.agentsfleet.net/v1/connectors/github/callback` (the connector callback; swap `$API_BASE` in dev).
3. **Request user authorization during installation** — enable it. GitHub then returns a one-time authorization `code` alongside `installation_id` and signed `state`; `agentsfleetd` exchanges the code and verifies the user can access that installation before accepting it.
4. **Webhook URL** — `https://api.agentsfleet.net/v1/ingress/github` (swap `$API_BASE` in development), set a generated high-entropy secret, and keep **Active** checked.
5. **Repository permissions** — scope minimally to the fleets you ship:
   - **Metadata:** Read-only (mandatory).
   - **Contents:** Read & write (Pull Request (PR)-opening fleets) or Read-only (review-only).
   - **Pull requests:** Read & write (review/PR fleets).
   - add Issues / Checks only if a shipped fleet needs them.
6. **Subscribe to events** — Pull request and Workflow run.
7. **Where can this App be installed?** — Any account (multi-tenant) for the hosted product.

### Acceptance

App created; user authorization requested during installation; webhook active; callback and ingress URLs are distinct; permissions and event subscriptions match the shipped fleets.

---

## 2.0 Human: Generate the private key and note the identifiers

**Goal:** capture the App identity, a fresh signing key, and the webhook verification secret. On the App's page:

1. Note the **App Identifier (ID)** (integer) — shown at the top of the App's settings page.
2. Note the **App's public slug** — the handle in the App's own public page URL, `github.com/apps/{app_slug}`. This is what the connect flow uses to build the install URL; it is not secret.
3. Note the **Client ID** and generate a **client secret**. Store the secret in 1Password immediately; it proves the server exchanging the one-time authorization code is this GitHub App.
4. **Generate a private key** — downloads a `.pem` (Privacy-Enhanced Mail) Rivest-Shamir-Adleman (RSA) key. This file goes to vault in §3 and is then shredded.
5. Retain the webhook secret through 1Password; never pass or print it in shell history.

### Acceptance

App ID, public slug, Client ID, downloaded `.pem`, client-secret vault reference, and webhook-secret vault reference in hand.

---

## 3.0 Agent: Store the platform secret in the admin vault

**Goal:** persist `{app_id, private_key_pem, app_slug, webhook_secret, client_id, client_secret}` as `github-app`. Secret values flow through files or 1Password reads into standard input, never command arguments or output. Then destroy the local key file.

```bash
PEM_PATH="${1:?path to the downloaded .pem}"
jq -n \
  --arg app_id "$(read -rp 'App ID: ' v; echo "$v")" \
  --arg app_slug "$(read -rp 'App public slug: ' v; echo "$v")" \
  --arg client_id "$(read -rp 'Client ID: ' v; echo "$v")" \
  --arg client_secret "$(op read "op://$VAULT/github-app/client_secret")" \
  --arg private_key_pem "$(cat "$PEM_PATH")" \
  --arg webhook_secret "$(op read "op://$VAULT/github-app/webhook_secret")" \
  '{app_id:$app_id, private_key_pem:$private_key_pem, app_slug:$app_slug, webhook_secret:$webhook_secret, client_id:$client_id, client_secret:$client_secret}' |
  AGENTSFLEET_API_KEY="$ADMIN_KEY" agentsfleet secret create github-app --data @-

# Destroy the local key copy — vault is now the only source of truth.
command -v shred >/dev/null && shred -u "$PEM_PATH" || rm -P "$PEM_PATH" 2>/dev/null || trash "$PEM_PATH"
```

> Prefer mirroring the PEM + identifiers into `op://$VAULT/github-app/*` first (so a vault rotation can re-issue), then pipe `op read` → `jq` → `agentsfleet secret create`. Re-run with `--force` to overwrite an existing `github-app`.

### Acceptance

`agentsfleet secret create` exits 0; the `.pem` no longer exists on disk; no key bytes in shell history, argv, or output.

---

## 4.0 Agent: Verify the broker can use it

**Goal:** prove both directions: App event routing into one repository-bound fleet and short-lived token minting out to GitHub.

```bash
# Metadata only — never the private key:
AGENTSFLEET_API_KEY="$ADMIN_KEY" agentsfleet secret show github-app --json | jq '{name,kind}'
# Connect a test workspace and install the App on one test repository. Confirm
# the callback exchanges the one-time code, verifies the GitHub user may access
# that installation, then creates the encrypted handle and connector-install
# route. A claimed installation already owned by another workspace must return
# 403 without changing either workspace. Create
# a github-pr-reviewer fleet whose TRIGGER.md names that repository and
# pull_request. Confirm `agentsfleet connector status github` reports connected.
# Open a test pull request; confirm exactly that fleet receives one event. Then
# let the fleet post its review through ${secrets.github.api_token}; the broker
# mints a short-lived installation token. Replay the same GitHub delivery and
# confirm no second event or review is created.
```

### Acceptance

`secret show` returns metadata with no key material; `agentsfleet connector status github` reports `connected`; a repository-bound Pull Request reaches exactly one expected `github-pr-reviewer` fleet; the fleet posts its review with a minted installation token; replay creates no duplicate event or review. Record the repository, Pull Request URL, fleet identifier, delivery identifier, and resulting event identifier without recording credentials.

If any check is skipped or fails, leave `github-pr-reviewer` marked unproven and keep this playbook's verification punch list open.

---

## Rollback

1. `agentsfleet secret delete github-app` (admin key) to drop the platform secret.
2. GitHub App page → **Generate a new private key** (rotation) and re-run §3, **or** Advanced → **Delete GitHub App** (invalidates every installation; re-install per customer afterward).
3. Customer installations of the App are independent of the stored private key; a key rotation keeps installs valid, a full delete revokes them.
