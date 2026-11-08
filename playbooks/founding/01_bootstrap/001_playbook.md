---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Bootstrap Accounts and Secret Stores

## What this does

The Human creates accounts and fills the secret stores.
The Agent syncs Vercel variables.
Run it once per clean organization, or after a full account reset.

Runtime infrastructure starts in [step 03](../03_priming_infra/001_playbook.md).

## Steps

### 1. Human creates the account boundary

Create or confirm, in provider consoles:

- GitHub organization `agentsfleet` and repository `agentsfleet/agentsfleet`
- 1Password vaults `ZMB_CD_DEV`, `ZMB_CD_PROD`, `ZMB_LOCAL_DEV` and `ops`
- a 1Password service account that reads both deployment vaults
- Fly.io billing (organizations are created in step 03)
- the Cloudflare zone for `agentsfleet.net`
- Vercel projects `agentsfleet-website`, `agentsfleet-app` and `agentsfleet-agents-dev`
- separate development and production Clerk, PlanetScale, PostHog and Grafana resources
- QStash, Tailscale, npm and gitleaks access

Record account and project names, never credentials.

### 2. Human populates 1Password

Create every bootstrap item and field listed in [step 02](../02_preflight/001_playbook.md#bootstrap-inputs).
Store values directly in 1Password.

Generate three independent values per environment with `openssl rand -hex 32`.
Development and production values must differ.

- `encryption-master-key/credential`
- `auth-session-code-pepper/credential`
- `approval-signing-secret/credential`

Create two dedicated Clerk acceptance identities per environment.
Store their emails as the `regular` and `admin` fields of `e2e-fixtures-email`.
No fixture password is stored; the suites create and remove users through Clerk.

### 3. Human configures GitHub

| Repository secret | Source |
|---|---|
| `OP_SERVICE_ACCOUNT_TOKEN` | 1Password deployment service account |
| `GITLEAKS_LICENSE` | gitleaks account |

Step 03 creates the repository variables.
After the first image push, make `ghcr.io/agentsfleet/agentsfleetd` public and grant this repository write access.

### 4. Agent syncs Vercel

After Human approval, write and then check the website and dashboard variables on both targets:

```bash
ALLOW_VAULT_READS=1 ALLOW_VERCEL_WRITES=1 \
  ./playbooks/founding/01_bootstrap/02_vercel_env.sh --apply
ALLOW_VAULT_READS=1 \
  ./playbooks/founding/01_bootstrap/02_vercel_env.sh --check
```

`--apply` ends with `PASS: Vercel environment values applied (<N> update(s))`.
`--check` ends with `PASS: Vercel environment values match 1Password`.
Any `DRIFT:` line makes `--check` exit 1.

### 5. Human redeploys Vercel without cache

Public variables are embedded at build time.
Start one cache-free deployment of each changed project, and record the URLs.

| Surface | Development | Production |
|---|---|---|
| API | `api-dev.agentsfleet.net` | `api.agentsfleet.net` |
| Dashboard | `app-dev.agentsfleet.net` | `app.agentsfleet.net` |
| Website | Vercel preview URL | `agentsfleet.net` |
| Installer | Vercel preview URL | `agentsfleet.dev` |

## Verify it works

The Human reports: `Accounts, deployment vaults, GitHub secrets, and Vercel projects are ready.`
The Agent then runs the `bootstrap` gate in [step 02](../02_preflight/001_playbook.md#steps).
Do not continue until it passes.
