# Credential Rotation

**Owners:** 🤠 Indy rotates values in provider consoles and 1Password; 🦉 Orly
propagates them and verifies the selected environment.
**Trigger:** a credential expires, is exposed, or is deliberately replaced.

## 1. Select and quiesce

Choose exactly one environment:

```bash
export ENV=dev
# or: export ENV=prod
```

🤠 Indy pauses affected writes when the provider cannot keep old and new values
valid at the same time.

## 2. Rotate and store

🤠 Indy replaces the provider value, then updates its existing 1Password field.
For Upstash, keep both fields current:

- `upstash-{env}/url` — root connection, reserved for destructive teardown
- `upstash-{env}/api-url` — restricted `agentsfleetd` runtime connection

There is no runner Redis credential.

For a PostHog key, update `posthog-{env}/credential`. For Vercel deployment
protection, update the matching `vercel-bypass-*` item in `ZMB_CD_PROD`.
Never paste an old or new value into chat, a shell argument, or a log.

## 3. Propagate

If a Vercel value changed, inspect and apply the complete environment matrix:

```bash
ALLOW_VAULT_READS=1 \
  ./playbooks/founding/01_bootstrap/02_vercel_env.sh --check

ALLOW_VAULT_READS=1 \
ALLOW_VERCEL_WRITES=1 \
  ./playbooks/founding/01_bootstrap/02_vercel_env.sh --apply
```

Redeploy the selected API so its Fly secret values are refreshed from
1Password:

- development: run `deploy-dev.yml`
- production: rerun the authorized Release workflow for the current tag, or
  use the next authorized release

## 4. Verify

```bash
ALLOW_VAULT_READS=1 \
ENV="$ENV" \
  ./playbooks/operations/credential_rotation/00_gate.sh
```

The gate verifies the current vault fields, `/healthz`, `/readyz`, and the
selected dashboard without exposing credential values in process arguments.
