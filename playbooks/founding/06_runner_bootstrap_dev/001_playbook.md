# DEV Runner Bootstrap (`zombie-dev-worker-ant`)

**Updated:** May 28, 2026
**Owner:** Agent (steps 1.0–5.0); Human (step 0.0 only)
**Prerequisite:** Vault items exist (`ZMB_CD_DEV`, `ZMB_CD_PROD`). Tailscale authkey in `ZMB_CD_PROD/tailscale/authkey`. 1Password service account token available as `OP_SERVICE_ACCOUNT_TOKEN`.

Bootstrap the DEV bare-metal worker node so CI can deploy the host-resident `agentsfleet-runner` daemon autonomously. After step 0 (human buys the server), every remaining step is agent-executable — no human interaction required. (Historical note: pre-M80 this host ran two services that the M80 cutover folded into the single `agentsfleet-runner` daemon.)

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
| 4.0 | Agent | Bootstrap `/opt/agentsfleet/` (deploy.sh + .env from vault) |
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

**Gate check:** `SECTIONS=1 ./playbooks/founding/06_runner_bootstrap_dev/00_gate.sh`

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

**Goal:** Packages required by `agentsfleet-runner` at runtime are installed.

| Package | Required by | Why |
|---------|------------|-----|
| `bubblewrap` | `agentsfleet-runner` | Sandbox isolation — `bwrap --unshare-all` for per-lease process namespacing; the egress handoff needs `--info-fd` + `--block-fd` (bwrap ≥ 0.8; prod boxes run 0.11) |
| `nftables` | `agentsfleet-runner` | **Egress allowlist (M84_004)** — the per-lease kernel egress rules on the veth |
| `iproute2` | `agentsfleet-runner` | **Egress allowlist (M84_004)** — veth pair + network-namespace plumbing |
| `git` | `agentsfleet-runner` | Clones repos into workspace for agent runs |
| `ca-certificates` | `agentsfleet-runner` | TLS connection to the agentsfleetd control plane |
| `openssl` | `agentsfleet-runner` | TLS runtime libraries |

Landlock and cgroups v2 require **no packages** — Landlock uses raw syscalls (kernel 5.13+, included in Debian 12), cgroups v2 is the default hierarchy on Debian 12. The egress boundary also needs the service to hold **CAP_NET_ADMIN** — granted by the systemd unit (`AmbientCapabilities`), not a package; the runner's own startup probe + `doctor` confirm it at runtime.

The installed versions of the three egress packages (`bubblewrap` + `--info-fd`/`--block-fd`, `nftables`, `iproute2`) are **asserted and recorded** by the shared probe `playbooks/lib/egress_host_deps.sh`, called from both `02_host_readiness.sh` (manual) and `03_deploy_readiness.sh` (run in CI by `deploy-dev.yml` → recorded in the job log). A box missing any of them fails readiness loud, before first lease.

```bash
KEY=$(op read "op://$VAULT_DEV/zombie-dev-worker-ant/ssh-private-key")

ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no zombie-dev-worker-ant << 'REMOTE'
set -euo pipefail
sudo apt-get update -qq
sudo apt-get install -y --no-install-recommends \
  bubblewrap \
  nftables \
  iproute2 \
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
  "export PATH=/usr/sbin:/sbin:\$PATH; bwrap --version && nft --version && ip -V && git --version && openssl version && test -f /sys/fs/cgroup/cgroup.controllers && echo 'all deps ok'"
# PATH prepends /usr/sbin so sbin-only nft/ip resolve over non-login SSH
# (Debian's default non-interactive PATH omits /usr/sbin — a present nftables
#  would otherwise read as MISSING, matching playbooks/lib/egress_host_deps.sh).
# Expected:
#   bubblewrap 0.11.0 (prod boxes; 0.8+ suffices — needs --info-fd/--block-fd)
#   nftables v1.x
#   iproute2-6.x
#   git version 2.x
#   OpenSSL 3.x
#   all deps ok
#
# The egress-specific trio (bubblewrap + flags, nftables, iproute2) is also
# asserted programmatically by playbooks/lib/egress_host_deps.sh via
# 02_host_readiness.sh and the CI-run 03_deploy_readiness.sh.
```

**Gate check:** `SECTIONS=2 ./playbooks/founding/06_runner_bootstrap_dev/00_gate.sh`

---

## 4.0 Agent: Bootstrap `/opt/agentsfleet/`

**Goal:** Server directory structure is created, repo deploy artifacts (`deploy.sh` + `agentsfleet-runner.service`) are copied via scp, and `/opt/agentsfleet/.env` is populated from vault with the three runner env vars the Option B daemon requires (`AGENTSFLEET_API_URL`, `AGENTSFLEET_RUNNER_TOKEN`, `RUNNER_HOST_ID`).

### 4.1 Create directory structure + copy deploy artifacts

```bash
KEY=$(op read "op://$VAULT_DEV/zombie-dev-worker-ant/ssh-private-key")
HOST=$(op read "op://$VAULT_DEV/zombie-dev-worker-ant/tailscale-hostname")
USER=$(op read "op://$VAULT_DEV/zombie-dev-worker-ant/deploy-user")
SSH_OPTS="-i <(printf '%s\n' \"\$KEY\") -o StrictHostKeyChecking=no"

# Create directory structure on server
ssh $SSH_OPTS "${USER}@zombie-dev-worker-ant" \
  "sudo mkdir -p /opt/agentsfleet/{bin,deploy} && sudo chown -R ${USER}:${USER} /opt/agentsfleet"

# Copy deploy script + the single runner systemd unit from repo
scp $SSH_OPTS deploy/baremetal/deploy.sh             "${USER}@zombie-dev-worker-ant:/opt/agentsfleet/deploy/deploy.sh"
scp $SSH_OPTS deploy/baremetal/agentsfleet-runner.service "${USER}@zombie-dev-worker-ant:/opt/agentsfleet/deploy/"
ssh $SSH_OPTS "${USER}@zombie-dev-worker-ant" "chmod +x /opt/agentsfleet/deploy/deploy.sh"
```

### 4.2 Populate `/opt/agentsfleet/.env` from vault

```bash
# The runner daemon needs exactly three env vars (Option B contract):
#   - AGENTSFLEET_API_URL       — control-plane base, dev: https://api-dev.agentsfleet.net
#   - AGENTSFLEET_RUNNER_TOKEN  — pre-minted agt_r token (vault field: runner-token)
#   - RUNNER_HOST_ID       — stable machine identifier (reuse vault: hostname)
#
# A real agt_r requires the platform-admin enrollment gate (M80_005) served by
# a live dev control plane. Until that's wired, store a placeholder
# (`agt_rFAKE_REPLACE_BEFORE_DEV_WORKER_READY_TRUE`) in the vault field so the
# bootstrap structure verifies end-to-end — DEV_WORKER_READY must stay `false`
# until the placeholder is swapped for a real admin-minted token.

RUNNER_TOKEN=$(op read "op://$VAULT_DEV/zombie-dev-worker-ant/runner-token")
HOST_ID=$(op read "op://$VAULT_DEV/zombie-dev-worker-ant/hostname")
API_URL="https://api-dev.agentsfleet.net"

ssh $SSH_OPTS "${USER}@zombie-dev-worker-ant" << EOF
cat > /opt/agentsfleet/.env << 'ENVFILE'
AGENTSFLEET_API_URL=${API_URL}
AGENTSFLEET_RUNNER_TOKEN=${RUNNER_TOKEN}
RUNNER_HOST_ID=${HOST_ID}
ENVFILE
chmod 600 /opt/agentsfleet/.env
EOF
```

The end-to-end provisioning above is also packaged as
[`04_provision_runner_env.sh`](./04_provision_runner_env.sh) — run that for
an idempotent one-shot (it also restarts `agentsfleet-runner.service` and verifies
it stays active).

### 4.3 Install the systemd unit

```bash
ssh $SSH_OPTS "${USER}@zombie-dev-worker-ant" << 'REMOTE'
sudo cp /opt/agentsfleet/deploy/agentsfleet-runner.service /etc/systemd/system/
sudo systemctl daemon-reload
REMOTE
```

### Acceptance

```bash
ssh $SSH_OPTS "${USER}@zombie-dev-worker-ant" \
  "stat -c '%a %n' /opt/agentsfleet/deploy/deploy.sh /opt/agentsfleet/.env /opt/agentsfleet/deploy/agentsfleet-runner.service"
# Expected:
#   755 /opt/agentsfleet/deploy/deploy.sh
#   600 /opt/agentsfleet/.env
#   644 /opt/agentsfleet/deploy/agentsfleet-runner.service

ssh $SSH_OPTS "${USER}@zombie-dev-worker-ant" "ls /etc/systemd/system/agentsfleet-runner.service"
# Expected:
#   /etc/systemd/system/agentsfleet-runner.service
```

**Gate check:** run `./04_provision_runner_env.sh` — it verifies vault refs, writes the env file, restarts the service, and confirms `is-active`.

---

## 5.0 Agent: First Deploy + Activate CI

**Goal:** First deploy runs end-to-end. `agentsfleet-runner.service` stays active. CI gate is lifted.

After this step, all future deploys happen automatically via `deploy-dev.yml` on every push to `main`. The CI job (`deploy-worker-dev`) scp's the freshly compiled `agentsfleet-runner` binary, the latest `deploy.sh`, and `agentsfleet-runner.service` to the server — then calls `deploy.sh runner` to install and restart. No manual intervention needed after bootstrap.

```bash
KEY=$(op read "op://$VAULT_DEV/zombie-dev-worker-ant/ssh-private-key")
USER=$(op read "op://$VAULT_DEV/zombie-dev-worker-ant/deploy-user")
SSH_OPTS="-i <(printf '%s\n' \"\$KEY\") -o StrictHostKeyChecking=no"

# Build the runner binary for linux/amd64
zig build -Doptimize=ReleaseSafe -Dtarget=x86_64-linux

# scp binary to server
scp $SSH_OPTS zig-out/bin/agentsfleet-runner "${USER}@zombie-dev-worker-ant:/opt/agentsfleet/bin/agentsfleet-runner"
ssh $SSH_OPTS "${USER}@zombie-dev-worker-ant" "chmod +x /opt/agentsfleet/bin/agentsfleet-runner"

# Deploy (single runner component)
VERSION="bootstrap-$(date +%Y%m%d)"
ssh $SSH_OPTS "${USER}@zombie-dev-worker-ant" \
  "sudo /opt/agentsfleet/deploy/deploy.sh runner $VERSION /opt/agentsfleet/bin/agentsfleet-runner"

# Verify
ssh $SSH_OPTS "${USER}@zombie-dev-worker-ant" << 'REMOTE'
sleep 3
systemctl is-active agentsfleet-runner.service
journalctl -u agentsfleet-runner.service --no-pager -n 10
REMOTE

# Activate CI — flip the gate ONLY after the runner is verified active with a
# real (non-placeholder) agt_r runner-token in the vault. The placeholder shape
# (agt_rFAKE_…) is rejected by deploy.sh and by the daemon's startup prefix check;
# leaving the gate true with a placeholder would just produce a red deploy.
gh variable set DEV_WORKER_READY --body "true" --repo agentsfleet/agentsfleet
echo "CI activated. Next push to main will deploy to zombie-dev-worker-ant."
```

### Acceptance

```
active                            <- agentsfleet-runner.service running
<runner startup log lines>        <- no MissingEnvVar / InvalidRunnerToken
DEV_WORKER_READY set              <- CI guard lifted
```

**Full gate check:** `./04_provision_runner_env.sh` — provisions the env file, restarts the service, confirms it stays active.

---

## What CI does after bootstrap

Once `DEV_WORKER_READY=true` is set, every push to `main` triggers the `deploy-worker-dev` job in `deploy-dev.yml`. It automatically:

1. Downloads the freshly compiled `agentsfleet-runner` binary (from the `compile-dev` job)
2. Joins the Tailscale network
3. Verifies worker host readiness (`03_deploy_readiness.sh`)
4. scp's the binary + `deploy/baremetal/deploy.sh` + `agentsfleet-runner.service` to the server
5. Calls `sudo deploy.sh runner $VERSION /opt/agentsfleet/bin/agentsfleet-runner` with the local binary path
6. Sends Discord notification on success/failure

No manual steps after bootstrap — the server is fully CI-managed. The env file (`/opt/agentsfleet/.env`) is **not** rewritten by CI; it's host-resident state, provisioned once via section 4.0 and rotated only via the credential-rotation playbook.

---

## Sequence Summary

```
0.0  Human: Buy server, store IP + deploy creds in vault
1.0  Agent: Verify SSH key from vault reaches server
2.0  Agent: Install Tailscale + join tailnet (switch to hostname, drop public IP)
3.0  Agent: Install runtime deps (bubblewrap, git, openssl, ca-certificates)
4.0  Agent: scp deploy/baremetal/{deploy.sh,agentsfleet-runner.service} -> /opt/agentsfleet/deploy/, provision /opt/agentsfleet/.env (AGENTSFLEET_API_URL + AGENTSFLEET_RUNNER_TOKEN + RUNNER_HOST_ID), install systemd unit
5.0  Agent: Build + scp the agentsfleet-runner binary, run deploy.sh runner, gh variable set DEV_WORKER_READY=true (only with a real agt_r in vault)
--- CI-automated after this point ---
```

**PROD workers** follow the same sequence. Replace `ZMB_CD_DEV` -> `ZMB_CD_PROD` and the hostnames accordingly. The runner-token vault field on the prod host entry must be admin-minted from the prod control plane; placeholders are dev-only.
