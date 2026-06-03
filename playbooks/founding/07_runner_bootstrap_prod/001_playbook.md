# PROD Runner Bootstrap

**Updated:** May 28, 2026
**Owner:** Agent (steps 1.0–5.0); Human (step 0.0 only)
**Status:** Worker era retired — each host now runs the single `zombie-runner` daemon (M80 cutover). `zombie-prod-worker-ant` is provisioned; `zombie-prod-worker-bird` is a placeholder (provision a second server to activate). `PROD_WORKER_READY=false` until a real `zrn_` runner-token is admin-minted via the prod control plane and stored under `op://ZMB_CD_PROD/zombie-prod-worker-ant/runner-token` (see §4.2). The vault entry may hold a `zrn_FAKE_…` placeholder until then.
**Prerequisite:** Vault items exist (`ZMB_CD_PROD`). Tailscale authkey in `ZMB_CD_PROD/tailscale/authkey`. 1Password service account token available as `OP_SERVICE_ACCOUNT_TOKEN`.

Bootstrap one or more PROD bare-metal worker nodes so CI can deploy the host-resident `zombie-runner` daemon autonomously. After step 0 (human buys the servers), every remaining step is agent-executable — no human interaction required. (Historical note: pre-M80 each host ran two services that the M80 cutover folded into the single `zombie-runner` daemon.)

**Fleet config:** PROD worker nodes are defined in the GitHub repository variable `PROD_WORKER_HOSTS` as a JSON array:

```json
[
  { "name": "zombie-prod-worker-1", "host": "zombie-prod-worker-1", "vault_key": "zombie-prod-worker-1" },
  { "name": "zombie-prod-worker-2", "host": "zombie-prod-worker-2", "vault_key": "zombie-prod-worker-2" }
]
```

Run this playbook for **each node** before setting `PROD_WORKER_READY=true`.

Environment setup for all commands in this playbook:

```bash
export VAULT_PROD="${VAULT_PROD:-ZMB_CD_PROD}"
# Replace with the specific worker being bootstrapped
export WORKER_NAME="zombie-prod-worker-1"
```

---

## Human vs Agent Split

| Step | Owner | What | Status |
|------|-------|------|--------|
| 0.0 | Human | Buy server(s) from provider, get IP + initial root credentials | ✅ DONE |
| 1.0 | Agent | Verify deploy SSH key from vault works | ✅ DONE |
| 2.0 | Agent | Install Tailscale + join tailnet | ✅ DONE — `zombie-prod-worker-ant` at `100.127.12.111` |
| 3.0 | Agent | Install runtime dependencies (bubblewrap, git, openssl, ca-certificates) | ✅ DONE |
| 4.0 | Agent | Bootstrap `/opt/zombie/` (deploy.sh + .env from vault) | ✅ DONE |
| 5.0 | Agent | First deploy + activate CI | ⏭ DEFERRED — CI executes first deploy on `v0.3.0` tag; `PROD_WORKER_READY=true` set |

After step 0 the agent runs steps 1–5 in sequence without human intervention.
After step 5 and after all nodes pass, CI handles all future deploys automatically via `release.yml`.

---

## 0.0 Human: Buy Servers

**Goal:** One or more bare-metal servers provisioned with known IPs and initial root credentials.

For each server in the fleet:

1. Log in to your provider console
2. Order a bare-metal or VPS node (Debian 13 Trixie preferred; Debian 12 Bookworm acceptable)
3. Install **Debian 13 (Trixie)** via the provider reinstall wizard
4. Set a root password or upload your personal public key for first login
5. Generate a deploy SSH key and store server details in vault:

```bash
# Generate per-worker deploy key
ssh-keygen -t ed25519 -C "${WORKER_NAME} deploy key" \
  -f /tmp/${WORKER_NAME} -N ""

# Store in 1Password
op item create --vault "$VAULT_PROD" \
  --title "$WORKER_NAME" \
  --category "SSH Key" \
  "hostname=$(echo '<server-ip-or-dns>')" \
  "tailscale-hostname=${WORKER_NAME}" \
  "deploy-user=debian" \
  "ssh-private-key[concealed]=$(cat /tmp/${WORKER_NAME})"

rm /tmp/${WORKER_NAME} /tmp/${WORKER_NAME}.pub
```

6. Add the worker to the `PROD_WORKER_HOSTS` GitHub variable (JSON array)
7. Signal agent: "Servers ready: zombie-prod-worker-1, zombie-prod-worker-2"

### Acceptance

```bash
# Agent confirms SSH access works for each node
KEY=$(op read "op://$VAULT_PROD/${WORKER_NAME}/ssh-private-key")
HOST=$(op read "op://$VAULT_PROD/${WORKER_NAME}/hostname")
USER=$(op read "op://$VAULT_PROD/${WORKER_NAME}/deploy-user")

ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no -o ConnectTimeout=10 \
  "${USER}@${HOST}" "echo ok"
# Expected: ok
```

---

## 1.0 Agent: Verify Deploy SSH Key

**Goal:** The vault SSH key can reach the server.

```bash
KEY=$(op read "op://$VAULT_PROD/${WORKER_NAME}/ssh-private-key")
HOST=$(op read "op://$VAULT_PROD/${WORKER_NAME}/hostname")
USER=$(op read "op://$VAULT_PROD/${WORKER_NAME}/deploy-user")

ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no -o ConnectTimeout=10 \
  "${USER}@${HOST}" "echo ok"
```

If the key isn't authorized on the server yet, authorize it using initial root credentials:

```bash
ssh root@<server-ip> \
  "mkdir -p /home/debian/.ssh && chmod 700 /home/debian/.ssh && \
   echo '$(op read "op://$VAULT_PROD/${WORKER_NAME}/ssh-public-key")' \
     >> /home/debian/.ssh/authorized_keys && \
   chmod 600 /home/debian/.ssh/authorized_keys && \
   chown -R debian:debian /home/debian/.ssh"
```

### Acceptance

```bash
ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no "${USER}@${HOST}" "echo ok"
# Expected: ok
```

---

## 2.0 Agent: Install Tailscale + Join Tailnet

**Goal:** Node is reachable in the tailnet as `${WORKER_NAME}`. After this step, all SSH uses the Tailscale hostname.

```bash
KEY=$(op read "op://$VAULT_PROD/${WORKER_NAME}/ssh-private-key")
HOST=$(op read "op://$VAULT_PROD/${WORKER_NAME}/hostname")
USER=$(op read "op://$VAULT_PROD/${WORKER_NAME}/deploy-user")
TAILSCALE_AUTHKEY=$(op read "op://$VAULT_PROD/tailscale/authkey")

ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no "${USER}@${HOST}" << REMOTE
set -euo pipefail
curl -fsSL https://tailscale.com/install.sh | sh
sudo tailscale up --authkey "$TAILSCALE_AUTHKEY" --hostname "${WORKER_NAME}"
tailscale status
REMOTE
```

### Acceptance

```bash
KEY=$(op read "op://$VAULT_PROD/${WORKER_NAME}/ssh-private-key")
ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no "${WORKER_NAME}" \
  "tailscale status | grep ${WORKER_NAME}"
# Expected: zombie-prod-worker-N  <tailscale-ip>  ...  active
```

All remaining steps use the Tailscale hostname `${WORKER_NAME}`.

---

## 3.0 Agent: Install Runtime Dependencies

**Goal:** Packages required by `zombie-runner` at runtime are installed.

| Package | Required by | Why |
|---------|------------|-----|
| `bubblewrap` | `zombie-runner` | Sandbox isolation — `bwrap --unshare-all` for per-lease process namespacing |
| `git` | `zombie-runner` | Clones repos into workspace for agent runs |
| `ca-certificates` | `zombie-runner` | TLS connection to the zombied control plane |
| `openssl` | `zombie-runner` | TLS runtime libraries |

```bash
KEY=$(op read "op://$VAULT_PROD/${WORKER_NAME}/ssh-private-key")

ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no "${WORKER_NAME}" << 'REMOTE'
set -euo pipefail
sudo apt-get update -qq
sudo apt-get install -y --no-install-recommends \
  bubblewrap \
  ca-certificates \
  git \
  openssl

bwrap --version
test -f /sys/fs/cgroup/cgroup.controllers && echo "cgroups v2: ok" || echo "cgroups v2: MISSING"
uname -r
REMOTE
```

### Acceptance

```bash
KEY=$(op read "op://$VAULT_PROD/${WORKER_NAME}/ssh-private-key")
ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no "${WORKER_NAME}" \
  "bwrap --version && git --version && openssl version && test -f /sys/fs/cgroup/cgroup.controllers && echo 'all deps ok'"
# Expected:
#   bubblewrap 0.9.0 (or similar — Trixie ships a newer version than Bookworm)
#   git version 2.x
#   OpenSSL 3.x
#   all deps ok
```

---

## 4.0 Agent: Bootstrap `/opt/zombie/`

**Goal:** Server directory structure is created, deploy artifacts (`deploy.sh` + `zombie-runner.service`) are copied via scp, and `/opt/zombie/.env` is populated from vault with the three runner env vars the Option B daemon requires (`ZOMBIE_API_URL`, `ZOMBIE_RUNNER_TOKEN`, `RUNNER_HOST_ID`).

### 4.1 Create directory structure + copy deploy artifacts

```bash
KEY=$(op read "op://$VAULT_PROD/${WORKER_NAME}/ssh-private-key")
USER=$(op read "op://$VAULT_PROD/${WORKER_NAME}/deploy-user")

ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no "${USER}@${WORKER_NAME}" \
  "sudo mkdir -p /opt/zombie/{bin,deploy} && sudo chown -R ${USER}:${USER} /opt/zombie"

scp -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no \
  deploy/baremetal/deploy.sh             "${USER}@${WORKER_NAME}:/opt/zombie/deploy/deploy.sh"
scp -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no \
  deploy/baremetal/zombie-runner.service "${USER}@${WORKER_NAME}:/opt/zombie/deploy/"

ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no "${USER}@${WORKER_NAME}" \
  "chmod +x /opt/zombie/deploy/deploy.sh"
```

### 4.2 Populate `/opt/zombie/.env` from vault

```bash
# The runner daemon needs exactly three env vars (Option B contract):
#   - ZOMBIE_API_URL       — control-plane base, prod: https://api.usezombie.com
#   - ZOMBIE_RUNNER_TOKEN  — pre-minted zrn_ token (vault field: runner-token)
#   - RUNNER_HOST_ID       — stable machine identifier (reuse vault: hostname or
#                            ${WORKER_NAME} if no hostname field exists yet)
#
# A real prod `zrn_` requires the platform-admin enrollment gate on the live
# prod control plane. Store a placeholder (`zrn_FAKE_…`) until that's ready;
# PROD_WORKER_READY must stay `false` until every node in PROD_WORKER_HOSTS
# has a real admin-minted token in vault and the runner is verified active.

KEY=$(op read "op://$VAULT_PROD/${WORKER_NAME}/ssh-private-key")
USER=$(op read "op://$VAULT_PROD/${WORKER_NAME}/deploy-user")
RUNNER_TOKEN=$(op read "op://$VAULT_PROD/${WORKER_NAME}/runner-token")
HOST_ID="${WORKER_NAME}"
API_URL="https://api.usezombie.com"

ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no "${USER}@${WORKER_NAME}" << EOF
cat > /opt/zombie/.env << 'ENVFILE'
ZOMBIE_API_URL=${API_URL}
ZOMBIE_RUNNER_TOKEN=${RUNNER_TOKEN}
RUNNER_HOST_ID=${HOST_ID}
ENVFILE
chmod 600 /opt/zombie/.env
EOF
```

### 4.3 Install the systemd unit

```bash
KEY=$(op read "op://$VAULT_PROD/${WORKER_NAME}/ssh-private-key")
USER=$(op read "op://$VAULT_PROD/${WORKER_NAME}/deploy-user")

ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no "${USER}@${WORKER_NAME}" << 'REMOTE'
sudo cp /opt/zombie/deploy/zombie-runner.service /etc/systemd/system/
sudo systemctl daemon-reload
REMOTE
```

### Acceptance

```bash
KEY=$(op read "op://$VAULT_PROD/${WORKER_NAME}/ssh-private-key")
USER=$(op read "op://$VAULT_PROD/${WORKER_NAME}/deploy-user")

ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no "${USER}@${WORKER_NAME}" \
  "stat -c '%a %n' /opt/zombie/deploy/deploy.sh /opt/zombie/.env /opt/zombie/deploy/zombie-runner.service"
# Expected:
#   755 /opt/zombie/deploy/deploy.sh
#   600 /opt/zombie/.env
#   644 /opt/zombie/deploy/zombie-runner.service

ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no "${USER}@${WORKER_NAME}" \
  "ls /etc/systemd/system/zombie-runner.service"
# Expected:
#   /etc/systemd/system/zombie-runner.service
```

---

## 5.0 Agent: First Deploy + Activate CI

**Goal:** First deploy runs end-to-end on every prod node. After all nodes pass with a real (non-placeholder) `zrn_` runner-token in vault, CI gate is lifted.

```bash
KEY=$(op read "op://$VAULT_PROD/${WORKER_NAME}/ssh-private-key")
USER=$(op read "op://$VAULT_PROD/${WORKER_NAME}/deploy-user")
DISCORD=$(op read "op://$VAULT_PROD/discord-ci-webhook/credential")

# Build the runner binary for linux/amd64
zig build -Doptimize=ReleaseSafe -Dtarget=x86_64-linux

# scp binary to server
scp -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no \
  zig-out/bin/zombie-runner "${USER}@${WORKER_NAME}:/opt/zombie/bin/zombie-runner"
ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no "${USER}@${WORKER_NAME}" \
  "chmod +x /opt/zombie/bin/zombie-runner"

# Deploy (single runner component)
VERSION="bootstrap-$(date +%Y%m%d)"
ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no "${USER}@${WORKER_NAME}" \
  "sudo DISCORD_WEBHOOK_URL='$DISCORD' /opt/zombie/deploy/deploy.sh runner $VERSION /opt/zombie/bin/zombie-runner"

# Verify
ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no "${USER}@${WORKER_NAME}" << 'REMOTE'
sleep 3
systemctl is-active zombie-runner.service
journalctl -u zombie-runner.service --no-pager -n 10
REMOTE
```

### Activate CI (after ALL nodes are bootstrapped)

Only set `PROD_WORKER_READY=true` once **every node** in `PROD_WORKER_HOSTS` has passed step 5 with a real admin-minted `zrn_` runner-token in vault (placeholder `zrn_FAKE_…` values are rejected by `deploy.sh` and by the daemon's startup prefix check).

```bash
gh variable set PROD_WORKER_READY --body "true" --repo usezombie/usezombie
echo "CI activated. Next release tag will deploy to all prod workers."
```

### Acceptance

```
active                            <- zombie-runner.service running on each node
<runner startup log lines>        <- no MissingEnvVar / InvalidRunnerToken
PROD_WORKER_READY set             <- CI guard lifted
```

---

## Repeating for Additional Nodes

Set `WORKER_NAME` to each node name and run steps 1–5 again:

```bash
export WORKER_NAME="zombie-prod-worker-2"
# repeat steps 1.0 – 5.0 (skip the final gh variable set until all nodes done)
```

Do not set `PROD_WORKER_READY=true` until every node in `PROD_WORKER_HOSTS` passes step 5.

---

## What CI does after bootstrap

Once `PROD_WORKER_READY=true` is set, every version tag (`v*`) triggers the `deploy-prod-canary` and `deploy-prod-fleet` jobs in `release.yml`. CI automatically:

1. Downloads the compiled `zombie-runner` binary from the GitHub Release assets
2. Joins the Tailscale network
3. SSH-deploys to the canary node first
4. Waits for human approval (GitHub environment: `production-fleet`)
5. Deploys remaining fleet sequentially
6. Sends Discord notification on success/failure per node

No manual steps after bootstrap — the fleet is fully CI-managed. The env file (`/opt/zombie/.env`) is **not** rewritten by CI; it's host-resident state, provisioned once via section 4.0 and rotated only via the credential-rotation playbook.

---

## Sequence Summary

```
0.0  Human: Buy server(s), store IP + deploy creds in ZMB_CD_PROD vault
1.0  Agent: Verify SSH key from vault reaches each server
2.0  Agent: Install Tailscale + join tailnet (switch to hostname, drop public IP)
3.0  Agent: Install runtime deps (bubblewrap, git, openssl, ca-certificates)
4.0  Agent: scp deploy/baremetal/{deploy.sh,zombie-runner.service} -> /opt/zombie/deploy/, provision /opt/zombie/.env (ZOMBIE_API_URL + ZOMBIE_RUNNER_TOKEN + RUNNER_HOST_ID), install systemd unit
5.0  Agent: Build + scp the zombie-runner binary, run deploy.sh runner, verify zombie-runner.service is active
--- After ALL nodes pass step 5 with a real admin-minted zrn_ in vault ---
5.1  Agent: gh variable set PROD_WORKER_READY=true
--- CI-automated after this point ---
```

**DEV worker** bootstrap follows the same pattern. See `playbooks/founding/06_runner_bootstrap_dev/001_playbook.md`.
