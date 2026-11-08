---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Rotate a credential

## What this does

Replaces a credential that expired, leaked or is due for replacement, then proves one environment still works.
Indy changes values in provider consoles and 1Password; the agent propagates and verifies.

| Credential | 1Password item and field | Vault |
|---|---|---|
| Datastore cluster seed on Fly's private network | `dragonfly-<ENV>/api-url` | Environment's |
| PostHog key | `posthog-<ENV>/credential` | Environment's |
| Vercel deployment protection | `vercel-bypass-app`, `vercel-bypass-agents`, `vercel-bypass-website`, each `/credential` | Production, for both environments |
| Pipeline Fly token | `fly-api-token/credential` | Environment's |

`<ENV>` is `dev` or `prod`.
The datastore has one credential; a self-hosted cluster is torn down by deleting its Fly app.
Runners hold no datastore credential.

## Steps

Work on one environment at a time.

1. Indy pauses affected writes if the provider cannot keep the old and new values valid together.
2. Indy replaces the provider value and updates the existing 1Password field.
   Never paste an old or new value into chat, a shell argument or a log.
3. If a Vercel value changed, check the full environment matrix.

   ```bash
   ALLOW_VAULT_READS=1 ./playbooks/founding/01_bootstrap/02_vercel_env.sh --check
   ```

   ```text
   <varies>
   ```

4. After approval, apply it.

   ```bash
   ALLOW_VAULT_READS=1 ALLOW_VERCEL_WRITES=1 ./playbooks/founding/01_bootstrap/02_vercel_env.sh --apply
   ```

   ```text
   <varies>
   ```

5. Redeploy the API so Fly secrets reload from 1Password: run `deploy-dev.yml` for development.
   For production, rerun the authorized Release workflow for the current tag, or use the next authorized release.

## Rotate the pipeline Fly token

Deploy workflows read `fly-api-token/credential` for every `flyctl` call; the daemon never uses it.
The token is bound to its Fly organization: `agentsfleet-dev` for development, `agentsfleet-prod` for production.
Rotate it whenever an app changes organization; linked billing shares credits and nothing else.

1. Mint the token. `--org` is a flag; a positional organization silently mints against the default one.

   ```bash
   fly tokens create org --org agentsfleet-dev --name agentsfleet-dev-ci --expiry 8760h
   ```

   ```text
   <varies: the token, shown once>
   ```

   For production, use `--org agentsfleet-prod --name agentsfleet-prod-ci`.
2. Put the value straight into the 1Password field, and nowhere else.
3. Redeploy as in step 5 above.
4. Once both lanes are green, list tokens to find the superseded one.

   ```bash
   fly tokens list --org agentsfleet-dev
   ```

   ```text
   <varies>
   ```

5. Revoke the superseded token. Revoking it earlier leaves a rollback no way back in.

   ```bash
   fly tokens revoke <TOKEN_ID>
   ```

   ```text
   <varies>
   ```

   `<TOKEN_ID>` comes from the list in step 4.

A token for the wrong organization fails as `Could not find App "<name>"`, naming the app and never the token.
If `fly apps list` shows the app under your login but the pipeline cannot find it, the token's organization is wrong.

## Verify it works

```bash
ALLOW_VAULT_READS=1 ENV=dev ./playbooks/operations/credential_rotation/00_gate.sh
```

```text
PASS: dev vault fields are present
PASS: dev services use the rotated credentials
PASS: dev credential rotation verification
```

Use `ENV=prod` for production.

The gate checks every field in the table except the Fly token, then `/healthz`, `/readyz` and the dashboard sign-in page.
In development it sends `vercel-bypass-app` as a header, never as an argument.
Each `OK:`, `MISSING:` or `FAIL:` line names one check.
