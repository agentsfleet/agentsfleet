# M4_002: Playbook — PROD Worker Bootstrap

**Milestone:** M4
**Workstream:** 002
**Updated:** Apr 02, 2026
**Status:** ✅ DONE — `zombie-prod-worker-ant` fully bootstrapped Apr 02, 2026; `PROD_WORKER_READY=true` set; CI will execute first deploy on `v0.3.0` tag. `zombie-prod-worker-bird` is a placeholder (same hostname as ant — provision a second server to activate).
**Prerequisite:** Vault items exist (`ZMB_CD_PROD`). Tailscale authkey in `ZMB_CD_PROD/tailscale/authkey`. 1Password service account token available as `OP_SERVICE_ACCOUNT_TOKEN`.

Bootstrap one or more PROD bare-metal worker nodes so CI can deploy the `zombied worker` + `zombied-executor` processes autonomously. After step 0 (human buys the servers), every remaining step is agent-executable — no human interaction required.

**Current provider:** OVHCloud (Beauharnois CA). See `docs/spec/v2/M1_002_AUTOPROCURER_PROVIDER.md` for the multi-provider design.

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

**Goal:** Packages required by `zombied` and `zombied-executor` at runtime are installed.

| Package | Required by | Why |
|---------|------------|-----|
| `bubblewrap` | `zombied-executor` | Sandbox isolation — `bwrap --unshare-all` for agent process namespacing |
| `git` | `zombied-executor` | Clones repos into workspace for agent runs |
| `ca-certificates` | `zombied` (worker + executor) | TLS connections to Upstash, PlanetScale, GitHub |
| `openssl` | `zombied` (worker + executor) | TLS runtime libraries |

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

**Goal:** Server directory structure is created, deploy artifacts are copied, and `.env` is populated from vault.

### 4.1 Create directory structure + copy deploy artifacts

```bash
KEY=$(op read "op://$VAULT_PROD/${WORKER_NAME}/ssh-private-key")
USER=$(op read "op://$VAULT_PROD/${WORKER_NAME}/deploy-user")

ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no "${USER}@${WORKER_NAME}" \
  "sudo mkdir -p /opt/zombie/{bin,deploy} && sudo chown -R ${USER}:${USER} /opt/zombie"

scp -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no \
  deploy/baremetal/deploy.sh     "${USER}@${WORKER_NAME}:/opt/zombie/deploy/deploy.sh"
scp -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no \
  deploy/baremetal/*.service     "${USER}@${WORKER_NAME}:/opt/zombie/deploy/"

ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no "${USER}@${WORKER_NAME}" \
  "chmod +x /opt/zombie/deploy/deploy.sh"
```

### 4.2 Populate `.env` from vault

```bash
KEY=$(op read "op://$VAULT_PROD/${WORKER_NAME}/ssh-private-key")
USER=$(op read "op://$VAULT_PROD/${WORKER_NAME}/deploy-user")
DB_URL=$(op read "op://$VAULT_PROD/planetscale-prod/worker-connection-string")
REDIS_URL=$(op read "op://$VAULT_PROD/upstash-prod/worker-url")
ENC_KEY=$(op read "op://$VAULT_PROD/encryption-master-key/credential")
GH_APP_ID=$(op read "op://$VAULT_PROD/github-app/app-id")
GH_APP_KEY=$(op read "op://$VAULT_PROD/github-app/private-key")

ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no "${USER}@${WORKER_NAME}" << EOF
cat > /opt/zombie/.env << 'ENVFILE'
DATABASE_URL_WORKER=${DB_URL}
REDIS_URL_WORKER=${REDIS_URL}
ENCRYPTION_MASTER_KEY=${ENC_KEY}
GITHUB_APP_ID=${GH_APP_ID}
GITHUB_APP_PRIVATE_KEY=${GH_APP_KEY}
ENVIRONMENT=prod
ENVFILE
chmod 600 /opt/zombie/.env
EOF
```

### 4.3 Install systemd units

```bash
KEY=$(op read "op://$VAULT_PROD/${WORKER_NAME}/ssh-private-key")
USER=$(op read "op://$VAULT_PROD/${WORKER_NAME}/deploy-user")

ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no "${USER}@${WORKER_NAME}" << 'REMOTE'
sudo cp /opt/zombie/deploy/zombied-executor.service /etc/systemd/system/
sudo cp /opt/zombie/deploy/zombied-worker.service   /etc/systemd/system/
sudo systemctl daemon-reload
REMOTE
```

### Acceptance

```bash
KEY=$(op read "op://$VAULT_PROD/${WORKER_NAME}/ssh-private-key")
USER=$(op read "op://$VAULT_PROD/${WORKER_NAME}/deploy-user")

ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no "${USER}@${WORKER_NAME}" \
  "stat -c '%a %n' /opt/zombie/deploy/deploy.sh /opt/zombie/.env /opt/zombie/deploy/*.service"
# Expected:
#   755 /opt/zombie/deploy/deploy.sh
#   600 /opt/zombie/.env
#   644 /opt/zombie/deploy/zombied-executor.service
#   644 /opt/zombie/deploy/zombied-worker.service

ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no "${USER}@${WORKER_NAME}" \
  "ls /etc/systemd/system/zombied-*.service"
# Expected:
#   /etc/systemd/system/zombied-executor.service
#   /etc/systemd/system/zombied-worker.service
```

---

## 5.0 Agent: First Deploy + Activate CI

**Goal:** First deploy runs end-to-end on every prod node. After all nodes pass, CI gate is lifted.

```bash
KEY=$(op read "op://$VAULT_PROD/${WORKER_NAME}/ssh-private-key")
USER=$(op read "op://$VAULT_PROD/${WORKER_NAME}/deploy-user")
DISCORD=$(op read "op://$VAULT_PROD/discord-ci-webhook/credential")

# Build binaries for linux/amd64
zig build -Doptimize=ReleaseSafe -Dtarget=x86_64-linux

# scp binaries to server
scp -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no \
  zig-out/bin/zombied          "${USER}@${WORKER_NAME}:/opt/zombie/bin/zombied"
scp -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no \
  zig-out/bin/zombied-executor "${USER}@${WORKER_NAME}:/opt/zombie/bin/zombied-executor"
ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no "${USER}@${WORKER_NAME}" \
  "chmod +x /opt/zombie/bin/*"

# Deploy executor first (worker Requires= it), then worker
VERSION="bootstrap-$(date +%Y%m%d)"
ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no "${USER}@${WORKER_NAME}" \
  "sudo DISCORD_WEBHOOK_URL='$DISCORD' /opt/zombie/deploy/deploy.sh executor $VERSION /opt/zombie/bin/zombied-executor"
ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no "${USER}@${WORKER_NAME}" \
  "sudo DISCORD_WEBHOOK_URL='$DISCORD' /opt/zombie/deploy/deploy.sh worker $VERSION /opt/zombie/bin/zombied"

# Verify systemd services
ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no "${USER}@${WORKER_NAME}" << 'REMOTE'
sleep 3
systemctl is-active zombied-executor
systemctl is-active zombied-worker
ls -l /run/zombie/executor.sock
journalctl -u zombied-executor --no-pager -n 5
journalctl -u zombied-worker   --no-pager -n 5
REMOTE
```

### Activate CI (after ALL nodes are bootstrapped)

Only set `PROD_WORKER_READY=true` once **every node** in `PROD_WORKER_HOSTS` has passed step 5.

```bash
gh variable set PROD_WORKER_READY --body "true" --repo usezombie/usezombie
echo "CI activated. Next release tag will deploy to all prod workers."
```

### Acceptance

```
active                          <- zombied-executor running
active                          <- zombied-worker running
/run/zombie/executor.sock       <- executor socket exists
<executor startup log lines>    <- no crash or panic
<worker startup log lines>      <- connected to executor
PROD_WORKER_READY set           <- CI guard lifted
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

1. Downloads compiled `zombied` + `zombied-executor` binaries from the GitHub Release assets
2. Joins the Tailscale network
3. SSH-deploys executor then worker to the canary node first
4. Waits for human approval (GitHub environment: `production-fleet`)
5. Deploys remaining fleet sequentially
6. Sends Discord notification on success/failure per node

No manual steps after bootstrap — the fleet is fully CI-managed.

---

## Sequence Summary

```
0.0  Human: Buy server(s), store IP + deploy creds in ZMB_CD_PROD vault
1.0  Agent: Verify SSH key from vault reaches each server
2.0  Agent: Install Tailscale + join tailnet (switch to hostname, drop public IP)
3.0  Agent: Install runtime deps (bubblewrap, git, openssl, ca-certificates)
4.0  Agent: scp deploy/baremetal/* -> /opt/zombie/deploy/, populate .env from vault, install systemd units
5.0  Agent: Build + scp binaries, run deploy.sh (executor then worker), verify services active
--- After ALL nodes pass step 5 ---
5.1  Agent: gh variable set PROD_WORKER_READY=true
--- CI-automated after this point ---
```

**DEV worker** bootstrap follows the same pattern. See `playbooks/M4_001_WORKER_BOOTSTRAP_DEV.md`.
