# M4_001: Playbook — DEV Worker Bootstrap (`zombie-dev-worker-ant`)

**Milestone:** M4
**Workstream:** 001
**Updated:** Mar 27, 2026
**Prerequisite:** Vault items exist (`ZMB_CD_DEV`, `ZMB_CD_PROD`). Tailscale authkey in `ZMB_CD_PROD/tailscale/authkey`. 1Password service account token available as `OP_SERVICE_ACCOUNT_TOKEN`.

Bootstrap the DEV bare-metal worker node so CI can deploy the `zombied worker` + `zombied-executor` processes autonomously. After step 0 (human buys the server), every remaining step is agent-executable — no human interaction required.

**Current provider:** OVHCloud (Beauharnois CA). See `docs/spec/v2/M1_002_AUTOPROCURER_PROVIDER.md` for the multi-provider design that will replace step 0 entirely.

Environment setup for all commands in this playbook:

```bash
export VAULT_DEV="${VAULT_DEV:-ZMB_CD_DEV}"
export VAULT_PROD="${VAULT_PROD:-ZMB_CD_PROD}"
```

---

## Human vs Agent Split

| Step | Owner | What |
|------|-------|------|
| 0.0 | Human | Buy server from provider, get IP + initial root credentials |
| 1.0 | Agent | Verify deploy SSH key from vault works |
| 2.0 | Agent | Install Tailscale + join tailnet |
| 3.0 | Agent | Install runtime dependencies (bubblewrap, git, openssl, ca-certificates) |
| 4.0 | Agent | Bootstrap `/opt/zombie/` (deploy.sh + .env from vault) |
| 5.0 | Agent | First deploy + activate CI |

After step 0 the agent runs steps 1–5 in sequence without human intervention.
After step 5, CI handles all future deploys automatically (binaries, deploy.sh, systemd units via `deploy-dev.yml`).

---

## 0.0 Human: Buy Server

**Goal:** A bare-metal server is provisioned with a known IP and initial root credentials.

1. Log in to your provider console
2. Order a bare-metal or VPS node (KVM not required for v1 — bubblewrap sandboxing only)
3. Install **Debian 12 (Bookworm)** via the provider reinstall wizard
4. Set a root password or upload your personal public key for first login
5. Store server details in vault:

```
Vault: ZMB_CD_DEV
Item: zombie-dev-worker-ant
Fields:
  hostname → server IP or DNS name
  deploy-user → debian (or whichever user has sudo)
  ssh-private-key → deploy SSH key (generate if needed)
```

6. Signal agent: "Server ready"

### Acceptance

```bash
# Agent confirms SSH access works
ssh debian@<server-ip> "echo ok"
# Expected: ok
```

---

## 1.0 Agent: Verify Deploy SSH Key

**Goal:** The vault SSH key can reach the server. If the key was generated during provisioning, this is a connectivity check. If not, generate one and store it.

```bash
KEY=$(op read "op://$VAULT_DEV/zombie-dev-worker-ant/ssh-private-key")
HOST=$(op read "op://$VAULT_DEV/zombie-dev-worker-ant/hostname")
USER=$(op read "op://$VAULT_DEV/zombie-dev-worker-ant/deploy-user")

ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no -o ConnectTimeout=10 \
  "${USER}@${HOST}" "echo ok"
```

If this fails and no key exists yet, generate one:

```bash
ssh-keygen -t ed25519 -C "zombie-dev-worker-ant deploy key" \
  -f /tmp/zombie-dev-worker-ant -N ""

op item edit "zombie-dev-worker-ant" --vault "$VAULT_DEV" \
  "ssh-private-key[concealed]=$(cat /tmp/zombie-dev-worker-ant)"

# Authorize on server using initial root credentials
ssh root@<server-ip> \
  "mkdir -p /home/debian/.ssh && chmod 700 /home/debian/.ssh && \
   echo '$(cat /tmp/zombie-dev-worker-ant.pub)' >> /home/debian/.ssh/authorized_keys && \
   chmod 600 /home/debian/.ssh/authorized_keys && chown -R debian:debian /home/debian/.ssh"

rm /tmp/zombie-dev-worker-ant /tmp/zombie-dev-worker-ant.pub
```

### Acceptance

```bash
KEY=$(op read "op://$VAULT_DEV/zombie-dev-worker-ant/ssh-private-key")
HOST=$(op read "op://$VAULT_DEV/zombie-dev-worker-ant/hostname")
USER=$(op read "op://$VAULT_DEV/zombie-dev-worker-ant/deploy-user")

ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no "${USER}@${HOST}" "echo ok"
# Expected: ok
```

**Gate check:** `SECTIONS=1 ./playbooks/gates/m4_001/run.sh`

---

## 2.0 Agent: Install Tailscale + Join Tailnet

**Goal:** Node is reachable in the tailnet as `zombie-dev-worker-ant`. After this step, all subsequent SSH uses the Tailscale hostname — not the public IP.

```bash
KEY=$(op read "op://$VAULT_DEV/zombie-dev-worker-ant/ssh-private-key")
HOST=$(op read "op://$VAULT_DEV/zombie-dev-worker-ant/hostname")
USER=$(op read "op://$VAULT_DEV/zombie-dev-worker-ant/deploy-user")
TAILSCALE_AUTHKEY=$(op read "op://$VAULT_PROD/tailscale/authkey")

ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no "${USER}@${HOST}" << REMOTE
set -euo pipefail
curl -fsSL https://tailscale.com/install.sh | sh
sudo tailscale up --authkey "$TAILSCALE_AUTHKEY" --hostname zombie-dev-worker-ant
tailscale status
REMOTE
```

### Acceptance

```bash
KEY=$(op read "op://$VAULT_DEV/zombie-dev-worker-ant/ssh-private-key")

# SSH via Tailscale hostname (no more public IP needed)
ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no zombie-dev-worker-ant \
  "tailscale status | grep zombie-dev-worker-ant"
# Expected: zombie-dev-worker-ant  <tailscale-ip>  ...  active
```

All remaining steps use the Tailscale hostname `zombie-dev-worker-ant`.

---

## 3.0 Agent: Install Runtime Dependencies

**Goal:** Packages required by `zombied` and `zombied-executor` at runtime are installed.

| Package | Required by | Why |
|---------|------------|-----|
| `bubblewrap` | `zombied-executor` | Sandbox isolation — `bwrap --unshare-all` for agent process namespacing |
| `git` | `zombied-executor` | Clones repos into workspace for agent runs |
| `ca-certificates` | `zombied` (worker + executor) | TLS connections to Upstash, PlanetScale, GitHub |
| `openssl` | `zombied` (worker + executor) | TLS runtime libraries |

Landlock and cgroups v2 require **no packages** — Landlock uses raw syscalls (kernel 5.13+, included in Debian 12), cgroups v2 is the default hierarchy on Debian 12.

```bash
KEY=$(op read "op://$VAULT_DEV/zombie-dev-worker-ant/ssh-private-key")

ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no zombie-dev-worker-ant << 'REMOTE'
set -euo pipefail
sudo apt-get update -qq
sudo apt-get install -y --no-install-recommends \
  bubblewrap \
  ca-certificates \
  git \
  openssl

# Verify bubblewrap is available
bwrap --version
# Verify cgroups v2 is active
test -f /sys/fs/cgroup/cgroup.controllers && echo "cgroups v2: ok" || echo "cgroups v2: MISSING"
# Verify kernel supports Landlock (5.13+)
uname -r
REMOTE
```

### Acceptance

```bash
KEY=$(op read "op://$VAULT_DEV/zombie-dev-worker-ant/ssh-private-key")
ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no zombie-dev-worker-ant \
  "bwrap --version && git --version && openssl version && test -f /sys/fs/cgroup/cgroup.controllers && echo 'all deps ok'"
# Expected:
#   bubblewrap 0.8.0 (or similar)
#   git version 2.x
#   OpenSSL 3.x
#   all deps ok
```

**Gate check:** `SECTIONS=2 ./playbooks/gates/m4_001/run.sh`

---

## 4.0 Agent: Bootstrap `/opt/zombie/`

**Goal:** Server directory structure is created, repo deploy artifacts (`deploy.sh`, systemd units) are copied via scp, and `.env` is populated from vault.

### 4.1 Create directory structure + copy deploy artifacts

```bash
KEY=$(op read "op://$VAULT_DEV/zombie-dev-worker-ant/ssh-private-key")
HOST=$(op read "op://$VAULT_DEV/zombie-dev-worker-ant/tailscale-hostname")
USER=$(op read "op://$VAULT_DEV/zombie-dev-worker-ant/deploy-user")
SSH_OPTS="-i <(printf '%s\n' \"\$KEY\") -o StrictHostKeyChecking=no"

# Create directory structure on server
ssh $SSH_OPTS "${USER}@zombie-dev-worker-ant" \
  "sudo mkdir -p /opt/zombie/{bin,deploy} && sudo chown -R ${USER}:${USER} /opt/zombie"

# Copy deploy script and systemd units from repo
scp $SSH_OPTS deploy/baremetal/deploy.sh     "${USER}@zombie-dev-worker-ant:/opt/zombie/deploy/deploy.sh"
scp $SSH_OPTS deploy/baremetal/*.service     "${USER}@zombie-dev-worker-ant:/opt/zombie/deploy/"
ssh $SSH_OPTS "${USER}@zombie-dev-worker-ant" "chmod +x /opt/zombie/deploy/deploy.sh"
```

### 4.2 Populate `.env` from vault

```bash
DB_URL=$(op read "op://$VAULT_DEV/planetscale-dev/worker-connection-string")
REDIS_URL=$(op read "op://$VAULT_DEV/upstash-dev/worker-url")
ENC_KEY=$(op read "op://$VAULT_DEV/encryption-master-key/credential")
GH_APP_ID=$(op read "op://$VAULT_DEV/github-app/app-id")
GH_APP_KEY=$(op read "op://$VAULT_DEV/github-app/private-key")

ssh $SSH_OPTS "${USER}@zombie-dev-worker-ant" << EOF
cat > /opt/zombie/.env << 'ENVFILE'
DATABASE_URL_WORKER=${DB_URL}
REDIS_URL_WORKER=${REDIS_URL}
ENCRYPTION_MASTER_KEY=${ENC_KEY}
GITHUB_APP_ID=${GH_APP_ID}
GITHUB_APP_PRIVATE_KEY=${GH_APP_KEY}
ENVIRONMENT=dev
ENVFILE
chmod 600 /opt/zombie/.env
EOF
```

### 4.3 Install systemd units

```bash
ssh $SSH_OPTS "${USER}@zombie-dev-worker-ant" << 'REMOTE'
sudo cp /opt/zombie/deploy/zombied-executor.service /etc/systemd/system/
sudo cp /opt/zombie/deploy/zombied-worker.service   /etc/systemd/system/
sudo systemctl daemon-reload
REMOTE
```

### Acceptance

```bash
ssh $SSH_OPTS "${USER}@zombie-dev-worker-ant" \
  "stat -c '%a %n' /opt/zombie/deploy/deploy.sh /opt/zombie/.env /opt/zombie/deploy/*.service"
# Expected:
#   755 /opt/zombie/deploy/deploy.sh
#   600 /opt/zombie/.env
#   644 /opt/zombie/deploy/zombied-executor.service
#   644 /opt/zombie/deploy/zombied-worker.service

ssh $SSH_OPTS "${USER}@zombie-dev-worker-ant" "ls /etc/systemd/system/zombied-*.service"
# Expected:
#   /etc/systemd/system/zombied-executor.service
#   /etc/systemd/system/zombied-worker.service
```

**Gate check:** `SECTIONS=3 ./playbooks/gates/m4_001/run.sh`

---

## 5.0 Agent: First Deploy + Activate CI

**Goal:** First deploy runs end-to-end. Services stay up. CI gate is lifted.

After this step, all future deploys happen automatically via `deploy-dev.yml` on every push to `main`. The CI job (`deploy-dev-worker`) scp's freshly compiled binaries, the latest `deploy.sh`, and systemd units to the server — then calls `deploy.sh` to install and restart. No manual intervention needed after bootstrap.

```bash
KEY=$(op read "op://$VAULT_DEV/zombie-dev-worker-ant/ssh-private-key")
USER=$(op read "op://$VAULT_DEV/zombie-dev-worker-ant/deploy-user")
SSH_OPTS="-i <(printf '%s\n' \"\$KEY\") -o StrictHostKeyChecking=no"

# Build binaries for linux/amd64
zig build -Doptimize=ReleaseSafe -Dtarget=x86_64-linux

# scp binaries to server
scp $SSH_OPTS zig-out/bin/zombied          "${USER}@zombie-dev-worker-ant:/opt/zombie/bin/zombied"
scp $SSH_OPTS zig-out/bin/zombied-executor "${USER}@zombie-dev-worker-ant:/opt/zombie/bin/zombied-executor"
ssh $SSH_OPTS "${USER}@zombie-dev-worker-ant" "chmod +x /opt/zombie/bin/*"

# Deploy executor first (worker Requires= it), then worker
VERSION="bootstrap-$(date +%Y%m%d)"
ssh $SSH_OPTS "${USER}@zombie-dev-worker-ant" \
  "sudo /opt/zombie/deploy/deploy.sh executor $VERSION /opt/zombie/bin/zombied-executor"
ssh $SSH_OPTS "${USER}@zombie-dev-worker-ant" \
  "sudo /opt/zombie/deploy/deploy.sh worker   $VERSION /opt/zombie/bin/zombied"

# Verify systemd services
ssh $SSH_OPTS "${USER}@zombie-dev-worker-ant" << 'REMOTE'
sleep 3
systemctl is-active zombied-executor
systemctl is-active zombied-worker
ls -l /run/zombie/executor.sock
journalctl -u zombied-executor --no-pager -n 5
journalctl -u zombied-worker   --no-pager -n 5
REMOTE

# Activate CI — set GitHub variable so deploy-dev-worker job runs on every push
gh variable set DEV_WORKER_READY --body "true" --repo usezombie/usezombie
echo "CI activated. Next push to main will deploy to zombie-dev-worker-ant."
```

### Acceptance

```
active                          <- zombied-executor running
active                          <- zombied-worker running
/run/zombie/executor.sock       <- executor socket exists
<executor startup log lines>    <- no crash or panic
<worker startup log lines>      <- connected to executor
DEV_WORKER_READY set            <- CI guard lifted
```

**Full gate check:** `./playbooks/gates/m4_001/run.sh` — runs all sections; must pass before CI activation.

---

## What CI does after bootstrap

Once `DEV_WORKER_READY=true` is set, every push to `main` triggers the `deploy-dev-worker` job in `deploy-dev.yml`. It automatically:

1. Downloads freshly compiled `zombied` + `zombied-executor` binaries (from the `build-dev` job)
2. Joins the Tailscale network
3. Runs `m4_001` gate section 3 (deploy readiness check)
4. scp's binaries + `deploy/baremetal/deploy.sh` + `*.service` to the server
5. Calls `deploy.sh executor` then `deploy.sh worker` with the local binary path
6. Sends Discord notification on success/failure

No manual steps after bootstrap — the server is fully CI-managed.

---

## Sequence Summary

```
0.0  Human: Buy server, store IP + deploy creds in vault
1.0  Agent: Verify SSH key from vault reaches server
2.0  Agent: Install Tailscale + join tailnet (switch to hostname, drop public IP)
3.0  Agent: Install runtime deps (bubblewrap, git, openssl, ca-certificates)
4.0  Agent: scp deploy/baremetal/* -> /opt/zombie/deploy/, populate .env from vault, install systemd units
5.0  Agent: Build + scp binaries, run deploy.sh (executor then worker), gh variable set DEV_WORKER_READY=true
--- CI-automated after this point ---
```

**PROD workers** follow the same sequence. Replace `ZMB_CD_DEV` -> `ZMB_CD_PROD`, `ENVIRONMENT=dev` -> `ENVIRONMENT=prod`, hostnames accordingly.
