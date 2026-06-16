# Runner Onboarding (local dashboard + mint)

**Tier:** operations (on-demand runbook, no implied order)
**Updated:** Jun 05, 2026
**Owner:** Human (Clerk + mint) · Agent (host provision)
**Prerequisite:** `operations/admin_bootstrap/001_playbook.md` has run for the
target environment — the operator (`nkishore@megam.io`) is a Clerk user with
`publicMetadata.platform_admin = true`. `op` is authenticated.

Onboard a `agentsfleet-runner` end to end: stand up the dashboard (locally or via the
deployed dev app), mint a dedicated `agt_r` token from the platform-admin
"Add runner" surface, store it in 1Password, and provision it onto a host. The
host-bootstrap playbooks (`founding/06_runner_bootstrap_dev`,
`founding/07_runner_bootstrap_prod`) only *install* a `agt_r`; **this** playbook is
where one is *minted*. The mint requires a platform-admin Clerk **session** — a
tenant `agt_t` key is rejected (`403 UZ-AUTH-021`).

---

## Local env contract

Three env files, one per service — never share them; different processes read
different files. All `.env*` are gitignored (`.env` + `.env.*` in the root
`.gitignore`); there are **no committed `.env` templates** — this section is the
contract.

| File | Read by | Notes |
|------|---------|-------|
| `ui/packages/app/.env.local` | Next.js dashboard (local) | API base + dev Clerk keys |
| `.env.agentsfleetd.local` | `agentsfleetd` container (`docker-compose.yml`, `make up`) | optional override; inline compose defaults already satisfy a from-scratch `make up` |
| `.env.runner.local` | local `agentsfleet-runner` (Linux container) | local/fake control-plane + token |

### `ui/packages/app/.env.local`

```
NEXT_PUBLIC_API_URL=https://api-dev.agentsfleet.net
NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY=<op://ZMB_CD_DEV/clerk-dev/publishable-key>
CLERK_SECRET_KEY=<op://ZMB_CD_DEV/clerk-dev/secret-key>
```

The Clerk keys MUST be the **dev** instance (`pk_test_…`/`sk_test_…`) — `api-dev`
only trusts dev-Clerk JSON Web Tokens (JWTs). Pulling them:

```bash
cd ui/packages/app
{ echo "NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY=$(op read 'op://ZMB_CD_DEV/clerk-dev/publishable-key')"
  echo "CLERK_SECRET_KEY=$(op read 'op://ZMB_CD_DEV/clerk-dev/secret-key')"; } >> .env.local
```

### `.env.runner.local` (only when running a runner locally)

```
AGENTSFLEET_API_URL=http://agentsfleetd:3000        # compose service name; http://localhost:3000 if on host
AGENTSFLEET_RUNNER_TOKEN=agt_r…                  # mint via §3 below; a fake agt_r verifies structure only
RUNNER_HOST_ID=local-dev-runner
RUNNER_SANDBOX_TIER=dev_none               # local default; landlock_full needs a hardened Linux container
```

The runner binary is Linux-only (bubblewrap + Landlock), so a local runner runs
inside a Linux container joined to the compose network — hence the `agentsfleetd:3000`
service-name endpoint.

---

## Human vs Agent split

| Step | Owner | What |
|------|-------|------|
| 0.0 | — | Prereq: admin_bootstrap ran; operator has `platform_admin=true` |
| 1.0 | Human | Set up `ui/packages/app/.env.local` (API base + dev Clerk keys) |
| 2.0 | Human | Run the dashboard (or use the deployed one) and sign in as the platform admin |
| 3.0 | Human | Mint a `agt_r` at `/admin/runners` → revealed once → copy |
| 4.0 | Agent | Store the `agt_r` in vault and provision the target host |

---

## 1.0 Human: dashboard env

Populate `ui/packages/app/.env.local` per the contract above. Without the Clerk
keys the dashboard cannot boot its auth middleware, so you never reach the mint
surface.

---

## 2.0 Human: run the dashboard + sign in

- **Local:** `cd ui/packages/app && bun run dev` → http://localhost:3000
- **Deployed (zero local setup):** `https://agentsfleet-app.vercel.app` — already
  built against `api-dev` (may sit behind a Vercel bypass).

Sign in as `nkishore@megam.io`. If **Configuration → Runners** is **absent**, the
`platform_admin` claim is not set on your user — return to
`operations/admin_bootstrap/001_playbook.md` §2 and set it.

---

## 3.0 Human: mint a runner

**Configuration → Runners → Add runner**:

| Field | Value |
|-------|-------|
| `host_id` | the runner's stable id — dev bare-metal: `zombie-dev-worker-ant`; local: `local-dev-runner` |
| `sandbox_tier` | `landlock_full` (bare-metal Linux) · `dev_none` (local) |
| `labels` | `dev` |

`host_id` must equal the `RUNNER_HOST_ID` the daemon will report (keeps the host
logs and the fleet list in agreement). The `agt_r` is revealed **once** — copy it
immediately (dismissal locked during reveal; the raw value is dropped on close).

### Acceptance

The new runner appears in the list with liveness `registered` (a freshly minted
runner is never a fake `online`).

---

## 4.0 Agent: store the token + provision the host

```bash
# Bare-metal dev host:
op item edit "zombie-dev-worker-ant" --vault ZMB_CD_DEV "runner-token=<agt_r…>"
```

Then hand off:

- **Bare-metal host** → `playbooks/founding/06_runner_bootstrap_dev/04_provision_runner_env.sh`
  (writes `/opt/agentsfleet/.env`, syncs `/etc/default/agentsfleet-runner`, restarts, verifies active).
- **Local container** → drop the `agt_r` into `.env.runner.local`, restart the runner.

### Acceptance

List the fleet with a platform-admin Clerk **session** JWT (mint one via Clerk's
Backend API exactly as `admin_bootstrap/001_playbook.md` §3 does — a tenant `agt_t`
key is rejected here):

```bash
API_BASE="https://api-dev.agentsfleet.net"
ADMIN_EMAIL="nkishore@megam.io"
CLERK_SECRET=$(op read 'op://ZMB_CD_DEV/clerk-dev/secret-key')
USER_ID=$(curl -s -H "Authorization: Bearer $CLERK_SECRET" \
  "https://api.clerk.com/v1/users?email_address=$ADMIN_EMAIL" | jq -r '.[0].id')
SESSION_ID=$(curl -s -X POST -H "Authorization: Bearer $CLERK_SECRET" \
  "https://api.clerk.com/v1/sessions" -d "user_id=$USER_ID" | jq -r '.id')
ADMIN_JWT=$(curl -s -X POST -H "Authorization: Bearer $CLERK_SECRET" \
  "https://api.clerk.com/v1/sessions/$SESSION_ID/tokens" | jq -r '.jwt')

curl -s -H "Authorization: Bearer $ADMIN_JWT" "$API_BASE/v1/fleet/runners" | jq .
# The runner is listed; liveness registered → online after first heartbeat.
```

A tenant `agt_t` key or a non-platform-admin JSON Web Token returns
`403 UZ-AUTH-021`.
