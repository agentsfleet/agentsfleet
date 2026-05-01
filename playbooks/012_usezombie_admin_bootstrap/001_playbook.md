# M11_006: Playbook — `usezombie-admin` User Bootstrap (DEV + PROD)

**Milestone:** M11
**Workstream:** 006 (§5 deliverable)
**Updated:** Apr 21, 2026
**Prerequisite:** Vault items `ZMB_CD_DEV/usezombie-admin` and `ZMB_CD_PROD/usezombie-admin` exist with fields `username` (email), `credential` (password), and `fireworks_api_key` (platform default Fireworks key). Clerk Dashboard access for both dev and prod. `op` CLI authenticated. Environment `{dev|prod}` selected per run.

Provisions the one global admin user (`usezombie-admin`) in Clerk for a given environment, promotes the user from `operator` to `admin` via `publicMetadata`, mints a tenant API key via `POST /v1/api-keys`, writes the raw key to the environment's vault item, stores the platform Fireworks key in the admin workspace vault, and registers it as the active platform default via `/v1/admin/platform-keys`. Idempotent on step 1 (signup) — if the user already exists in Clerk, step 1 becomes a login check and the playbook resumes at step 2.

**This playbook is not run during the M11_006 merge.** Run it manually, per environment, when you are ready to exercise admin-only endpoints.

---

## Human vs Agent Split

| Step | Owner | What |
|------|-------|------|
| 0.0 | Agent | Resolve environment; load credentials from vault |
| 1.0 | Human | Sign up at the website with the admin email + password |
| 2.0 | Human | Flip `publicMetadata.role` from `operator` to `admin` in Clerk Dashboard |
| 3.0 | Agent | Verify the admin JWT carries `role=admin` by calling an admin-gated endpoint |
| 4.0 | Agent | Mint a `zmb_t_` tenant API key via `POST /v1/api-keys` |
| 5.0 | Agent | Write the raw key to `op://ZMB_CD_<env>/usezombie-admin` field `api_key` |
| 6.0 | Agent | Verify the stored key authenticates a protected endpoint |
| 7.0 | Agent | Store the platform Fireworks key in the admin workspace vault |
| 8.0 | Agent | Register Fireworks as the active platform default |

Steps 1–2 are the only human-interactive steps. Steps 3–8 run end-to-end without intervention.

---

## 0.0 Agent: Resolve environment and load credentials

**Goal:** pick dev or prod, read the admin's email + password + API base URL from vault.

```bash
# Pick one:
export ENV="dev"   # or: export ENV="prod"

case "$ENV" in
  dev)  export VAULT="ZMB_CD_DEV";  export API_BASE="https://api-dev.usezombie.com";  export WEB_BASE="https://dev.usezombie.com" ;;
  prod) export VAULT="ZMB_CD_PROD"; export API_BASE="https://api.usezombie.com";      export WEB_BASE="https://usezombie.com" ;;
  *)    echo "ENV must be 'dev' or 'prod'"; exit 1 ;;
esac

export ADMIN_EMAIL=$(op read "op://$VAULT/usezombie-admin/username")
export ADMIN_PASS=$(op read "op://$VAULT/usezombie-admin/credential")

test -n "$ADMIN_EMAIL" || { echo "missing admin email"; exit 1; }
test -n "$ADMIN_PASS"  || { echo "missing admin password"; exit 1; }
op read "op://$VAULT/usezombie-admin/fireworks_api_key" >/dev/null || { echo "missing Fireworks api key"; exit 1; }
echo "Resolved admin=$ADMIN_EMAIL against $API_BASE"
```

### Acceptance

Both variables non-empty. `$API_BASE/healthz` returns 200.

---

## 1.0 Human: Sign up via website

**Goal:** `usezombie-admin` has a Clerk user in this environment, with a `core.tenants` row provisioned by the signup webhook, and `publicMetadata` containing `tenant_id=<new uuid>` and `role="operator"`.

1. Open `$WEB_BASE` in a browser.
2. Click Sign Up.
3. Use `$ADMIN_EMAIL` (the vault-resolved email) and `$ADMIN_PASS` (the vault-resolved password).
4. Complete email OTP / verification flow.
5. Land on the dashboard. This confirms the signup webhook fired and the tenant was provisioned.

**Idempotency:** if the user already exists (repeat run), log in instead. The playbook resumes from step 2.

### Acceptance

```bash
# Clerk Dashboard: Users list shows $ADMIN_EMAIL with status "active".
# Optional sanity query (requires CLERK_SECRET_KEY):
curl -s -H "Authorization: Bearer $(op read op://$VAULT/clerk/secret-key)" \
  "https://api.clerk.com/v1/users?email_address=$ADMIN_EMAIL" | jq '.[0] | {id,public_metadata}'
# Expect: public_metadata = { "tenant_id": "...", "role": "operator" }
```

---

## 2.0 Human: Promote to admin in Clerk Dashboard

**Goal:** flip `publicMetadata.role` from `"operator"` to `"admin"` for the one global admin user. No other user in the environment receives this change.

1. Open https://dashboard.clerk.com
2. Pick the application for this environment (dev or prod).
3. Users → search for `$ADMIN_EMAIL` → open the user.
4. Metadata tab → Public metadata → edit:
   ```json
   { "tenant_id": "...<leave as-is>...", "role": "admin" }
   ```
5. Save.
6. **Do NOT touch any other user's metadata.** This is the only account that gets `role=admin`.

### Acceptance

```bash
curl -s -H "Authorization: Bearer $(op read op://$VAULT/clerk/secret-key)" \
  "https://api.clerk.com/v1/users?email_address=$ADMIN_EMAIL" | jq '.[0].public_metadata.role'
# Expect: "admin"
```

---

## 3.0 Agent: Verify admin JWT

**Goal:** a fresh session token for `$ADMIN_EMAIL` carries `role=admin` and is accepted by an admin-gated endpoint.

Use Clerk's sign-in API to obtain a session token, then call `/v1/admin/platform-keys` (any admin-gated route) to confirm 200.

```bash
# Implementation note: session acquisition uses Clerk's /v1/client/sign_ins flow.
# For playbook simplicity the agent can use a short-lived JWT minted via Clerk's
# Backend API (create_session_token) instead:
CLERK_SECRET=$(op read "op://$VAULT/clerk/secret-key")
USER_ID=$(curl -s -H "Authorization: Bearer $CLERK_SECRET" \
  "https://api.clerk.com/v1/users?email_address=$ADMIN_EMAIL" | jq -r '.[0].id')
SESSION_ID=$(curl -s -X POST -H "Authorization: Bearer $CLERK_SECRET" \
  "https://api.clerk.com/v1/sessions" -d "user_id=$USER_ID" | jq -r '.id')
ADMIN_JWT=$(curl -s -X POST -H "Authorization: Bearer $CLERK_SECRET" \
  "https://api.clerk.com/v1/sessions/$SESSION_ID/tokens" | jq -r '.jwt')

curl -s -o /dev/null -w "%{http_code}\n" \
  -H "Authorization: Bearer $ADMIN_JWT" \
  "$API_BASE/v1/admin/platform-keys"
# Expect: 200
```

### Acceptance

Admin-gated endpoint returns 200. If 403, the Clerk JWT template is not embedding `publicMetadata.role` — fix the template in Clerk Dashboard → JWT Templates and re-run.

---

## 4.0 Agent: Mint tenant API key

**Goal:** create one `zmb_t_` admin-CLI key via `POST /v1/api-keys` (M28_002 endpoint).

```bash
MINT_RESPONSE=$(curl -s -X POST \
  -H "Authorization: Bearer $ADMIN_JWT" \
  -H "Content-Type: application/json" \
  -d '{"key_name":"admin-cli","description":"Programmatic admin access bootstrapped by playbook 012"}' \
  "$API_BASE/v1/api-keys")

echo "$MINT_RESPONSE" | jq .

RAW_KEY=$(echo "$MINT_RESPONSE" | jq -r '.key')
KEY_ID=$(echo "$MINT_RESPONSE" | jq -r '.id')

[[ "$RAW_KEY" =~ ^zmb_t_[0-9a-f]{64}$ ]] || { echo "bad key shape"; exit 1; }
echo "Minted key_id=$KEY_ID (raw key held in RAW_KEY)"
```

**Idempotency:** if `admin-cli` key already exists for this tenant, the server returns 409 `ERR_APIKEY_NAME_TAKEN`. In that case, rotate: `PATCH /v1/api-keys/{id}` with `{"active":false}`, then `DELETE`, then re-run this step.

### Acceptance

`RAW_KEY` matches `^zmb_t_[0-9a-f]{64}$`. Response shape: `{id, key_name, key, created_at}`.

---

## 5.0 Agent: Write raw key to vault

**Goal:** persist the raw key at `op://$VAULT/usezombie-admin` field `api_key`. This is the only place it will ever exist after this step — the server stores only the SHA-256 hash.

```bash
op item edit "usezombie-admin" --vault "$VAULT" "api_key=$RAW_KEY"
unset RAW_KEY

# Verify:
STORED=$(op read "op://$VAULT/usezombie-admin/api_key")
[[ "$STORED" =~ ^zmb_t_[0-9a-f]{64}$ ]] || { echo "vault write verification failed"; exit 1; }
echo "api_key stored at op://$VAULT/usezombie-admin/api_key"
unset STORED
```

### Acceptance

`op read` returns a well-formed `zmb_t_` key. Shell history contains no raw key (we unset the variables).

---

## 6.0 Agent: Verify stored key authenticates

**Goal:** a request bearing the vault-stored key hits an admin-gated endpoint and gets 200.

```bash
KEY=$(op read "op://$VAULT/usezombie-admin/api_key")
curl -s -o /dev/null -w "%{http_code}\n" \
  -H "Authorization: Bearer $KEY" \
  "$API_BASE/v1/admin/platform-keys"
# Expect: 200
unset KEY
```

### Acceptance

200 response. A 401 indicates the key write was corrupted or the tenant's admin-role mapping isn't active; re-check step 4 and step 2.

---

## 7.0 Agent: Store platform Fireworks key in admin workspace vault

**Goal:** write the Fireworks API key into the admin tenant's normal credential vault under the provider name `fireworks`. This is the key platform-managed tenants use through the `core.platform_llm_keys` pointer. The raw key must flow from 1Password to `jq` to `zombiectl` through stdin; do not pass it as a shell argument.

```bash
op read "op://$VAULT/usezombie-admin/fireworks_api_key" |
  jq -Rn '{provider:"fireworks", api_key: input, model:"accounts/fireworks/models/kimi-k2.6"}' |
  zombiectl credential set fireworks --data @-
```

### Acceptance

`zombiectl credential set` exits 0. The raw Fireworks key does not appear in shell history, process argv, or playbook output.

---

## 8.0 Agent: Register Fireworks as platform default

**Goal:** create or update the active `core.platform_llm_keys` pointer so platform-managed users resolve Fireworks from the admin workspace vault at runtime. No key material is stored in `core.platform_llm_keys`.

```bash
KEY=$(op read "op://$VAULT/usezombie-admin/api_key")
curl -s -X PUT \
  -H "Authorization: Bearer $KEY" \
  -H "Content-Type: application/json" \
  -d '{"provider":"fireworks","credential_name":"fireworks","model":"accounts/fireworks/models/kimi-k2.6","context_cap_tokens":256000}' \
  "$API_BASE/v1/admin/platform-keys" | jq .
unset KEY
```

### Acceptance

`GET /v1/admin/platform-keys` returns one active Fireworks row pointing at the admin workspace credential named `fireworks`. `zombiectl doctor --json` for a fresh non-admin tenant reports `tenant_provider.mode="platform"`, provider `fireworks`, model `accounts/fireworks/models/kimi-k2.6`, and `context_cap_tokens=256000` without exposing the Fireworks API key.

---

## Rollback

If the admin user was misconfigured mid-playbook:

1. `DELETE /v1/api-keys/{KEY_ID}` (after PATCHing `active:false`) to revoke the minted key.
2. Clerk Dashboard → user → Metadata → set `"role": "operator"` to demote.
3. Clear `op://$VAULT/usezombie-admin/api_key`.
4. Deactivate the Fireworks platform-default row through `/v1/admin/platform-keys`.
5. Restart the playbook from step 1.
