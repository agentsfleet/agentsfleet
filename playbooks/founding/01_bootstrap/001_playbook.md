# Bootstrap Accounts and Secret Stores

**Owner:** Human
**Executors:** Human for external consoles; Agent for repeatable commands
**Frequency:** once for a clean organization; repeat only after a full account
reset

This step establishes identities and secret stores. It does not create runtime
infrastructure; that begins in `03_priming_infra`.

## Execution and verification

| Order | Executor | Action | Verifier | Required evidence |
|---|---|---|---|---|
| 1 | Human | Create accounts and approve billing. | Human | Account and project names recorded without credentials. |
| 2 | Human | Populate both deployment vaults. | Agent | Bootstrap credential gate passes for development and production. |
| 3 | Human | Configure GitHub secrets and access. | Pipeline | A workflow can authenticate to 1Password without printing values. |
| 4 | Agent | Synchronize Vercel settings after Human approval. | `02_vercel_env.sh --check` | Every expected variable exists on its required targets. |
| 5 | Human | Start cache-free Vercel deployments. | Agent | Development and production deployment URLs recorded. |

## 1. Human creates the account boundary

Create or confirm:

- GitHub organization `agentsfleet` and repository `agentsfleet/agentsfleet`
- 1Password vaults `ZMB_CD_DEV`, `ZMB_CD_PROD`, `ZMB_LOCAL_DEV`, and `ops`
- a 1Password service account with read access to both deployment vaults
- Fly.io organization and billing
- Cloudflare zone for `agentsfleet.net`
- Vercel projects `agentsfleet-website`, `agentsfleet-app`, and
  `agentsfleet-agents-dev`
- separate Clerk, PlanetScale, Upstash, PostHog, and Grafana resources for
  development and production
- QStash, Tailscale, npm, and gitleaks access

Use the provider consoles for account creation and payment approval. Do not
paste raw provider credentials into chat.

## 2. Human populates 1Password

Create every item and field listed in
[`02_preflight/001_playbook.md`](../02_preflight/001_playbook.md). Store values
directly in 1Password.

Generate four independent 32-byte values per environment:

- `encryption-master-key/credential`
- `auth-session-code-pepper/credential`
- `audit-log-pepper/credential`
- `approval-signing-secret/credential`

Use `openssl rand -hex 32`; development and production values must differ.

Create two dedicated Clerk acceptance identities per environment. Store their
email addresses as the `regular` and `admin` fields on the single
`e2e-fixtures-email` item. The suites provision and remove users through Clerk;
no fixture password is stored.

## 3. Human configures GitHub

Repository secrets:

| Name | Source |
|---|---|
| `OP_SERVICE_ACCOUNT_TOKEN` | 1Password deployment service account |
| `GITLEAKS_LICENSE` | gitleaks account |

Repository variables are created during `03_priming_infra`; they name vaults,
Fly apps, and runner-readiness state.

After the first container push, the Human makes the
`ghcr.io/agentsfleet/agentsfleetd` package public and grants this repository
write access. The image contains compiled code only; runtime credentials come
from 1Password.

## 4. Agent synchronizes Vercel

The sync covers both Vercel targets and the complete repository-owned variable
set for the website and dashboard:

```bash
ALLOW_VAULT_READS=1 \
ALLOW_VERCEL_WRITES=1 \
  ./playbooks/founding/01_bootstrap/02_vercel_env.sh --apply

ALLOW_VAULT_READS=1 \
  ./playbooks/founding/01_bootstrap/02_vercel_env.sh --check
```

The Human then starts one cache-free deployment of each changed project because
public variables are embedded during the build.

The target domains are:

| Surface | Development | Production |
|---|---|---|
| API | `api-dev.agentsfleet.net` | `api.agentsfleet.net` |
| Dashboard | `app-dev.agentsfleet.net` | `app.agentsfleet.net` |
| Website | Vercel preview URL | `agentsfleet.net` |
| Installer | Vercel preview URL | `agentsfleet.dev` |

These are desired endpoints, not proof that DNS or deployments are already
ready.

## 5. Handoff

The Human tells the Agent:

> Accounts, deployment vaults, GitHub secrets, and Vercel projects are ready.

The Agent runs the pre-priming gate:

```bash
ENV=all STAGE=bootstrap \
  ./playbooks/founding/02_preflight/00_gate.sh
```

The successful gate output is the handoff evidence. Do not continue until it
passes. Runtime-generated admin and runner values are created only after the
first control-plane deployment.
