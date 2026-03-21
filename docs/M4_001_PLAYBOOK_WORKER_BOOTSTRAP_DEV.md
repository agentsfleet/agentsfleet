# M4_001: Playbook — DEV Worker Bootstrap (`zombie-dev-worker-ant`)

**Milestone:** M4
**Workstream:** 001
**Updated:** Mar 21, 2026
**Prerequisite:** Vault items exist (`ZMB_CD_DEV`, `ZMB_CD_PROD`). Tailscale authkey in `ZMB_CD_PROD/tailscale/authkey`. 1Password service account token available as `OP_SERVICE_ACCOUNT_TOKEN`.

Bootstrap the DEV bare-metal worker node so CI can deploy the `zombied worker` process autonomously. After step 0 (human buys the server), every remaining step is agent-executable — no human interaction required.

**Current provider:** OVHCloud (Beauharnois CA). See `docs/spec/v2/M001_AUTOPROCURER_PROVIDER.md` for the multi-provider design that will replace step 0 entirely.

---

## Human vs Agent Split

| Step | Owner | What |
|------|-------|------|
| 0.0 | Human | Buy server from provider, get IP + initial root credentials |
| 1.0 | Agent | Generate deploy SSH key → store in vault → authorize on server |
| 2.0 | Agent | Verify KVM (`kvm-ok`) |
| 3.0 | Agent | Install Tailscale + join tailnet |
| 4.0 | Agent | Install Docker, verify GHCR pull |
| 5.0 | Agent | Install Firecracker |
| 6.0 | Agent | Bootstrap `/opt/zombie/` (deploy.sh + .env from vault) |
| 7.0 | Agent | Smoke test + activate CI |

After step 0 the agent runs steps 1–7 in sequence without human intervention.

---

## 0.0 Human: Buy Server

**Goal:** A bare-metal server is provisioned with a known IP and initial root credentials.

1. Log in to your provider console
2. Order a bare-metal node with KVM support (required for Firecracker, M4_008)
3. Install **Debian 12 (Bookworm)** via the provider reinstall wizard
4. Set a root password or upload your personal public key for first login
5. Record: `<server-ip>` and the initial root credential

Hand off to agent: `server_ip=<ip>` and either `root_password=<pass>` or `initial_ssh_key=<path>`.

### Acceptance

```bash
# Agent confirms KVM is available before proceeding
ssh root@<server-ip> "grep -c vmx /proc/cpuinfo || grep -c svm /proc/cpuinfo"
# Expected: integer > 0
```

---

## 1.0 Agent: Deploy SSH Key → Vault → Authorize on Server

**Goal:** A dedicated deploy key pair exists. Private key is in the vault. Root credentials are no longer needed after this step.

This replaces root access with a scoped, vaulted deploy key that CI and future agents use.

```bash
# Generate Ed25519 deploy key — no passphrase (CI needs unattended access)
ssh-keygen -t ed25519 -C "zombie-dev-worker-ant deploy key" \
  -f /tmp/zombie-dev-worker-ant -N ""

# Store private key in vault
op item edit "zombie-dev-worker-ant" --vault ZMB_CD_DEV \
  "ssh-private-key[concealed]=$(cat /tmp/zombie-dev-worker-ant)"

# Authorize the public key on the server using initial root credentials
# (use -i <initial_key> or sshpass -p <root_password> as appropriate)
ssh root@<server-ip> \
  "mkdir -p ~/.ssh && chmod 700 ~/.ssh && \
   echo '$(cat /tmp/zombie-dev-worker-ant.pub)' >> ~/.ssh/authorized_keys && \
   chmod 600 ~/.ssh/authorized_keys"

# Discard the temporary key files
rm /tmp/zombie-dev-worker-ant /tmp/zombie-dev-worker-ant.pub
```

### Acceptance

```bash
KEY=$(op read "op://ZMB_CD_DEV/zombie-dev-worker-ant/ssh-private-key")

# Vault key is present
echo "$KEY" | head -1
# Expected: -----BEGIN OPENSSH PRIVATE KEY-----

# Passwordless SSH works using the vault key
ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no root@<server-ip> "echo ok"
# Expected: ok
```

---

## 2.0 Agent: Verify KVM

**Goal:** KVM acceleration is available on the node. Fail early — all Firecracker work (M4_008) depends on this. Do not proceed if this fails.

```bash
KEY=$(op read "op://ZMB_CD_DEV/zombie-dev-worker-ant/ssh-private-key")

ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no root@<server-ip> << 'REMOTE'
set -euo pipefail
apt-get install -y cpu-checker -qq
if ! kvm-ok 2>&1 | grep -q "KVM acceleration can be used"; then
  echo "ERROR: KVM not available — this host cannot run Firecracker microVMs." >&2
  echo "Abort bootstrap. Order a different server (bare-metal with hardware virt)." >&2
  exit 1
fi
echo "KVM OK"
REMOTE
```

### Acceptance

```
INFO: /dev/kvm exists
KVM acceleration can be used
KVM OK
```

Abort and reprovision if this fails — do not continue.

---

## 3.0 Agent: Install Tailscale + Join Tailnet

**Goal:** Node is reachable in the tailnet as `zombie-dev-worker-ant`. After this step, all subsequent SSH uses the Tailscale hostname — not the public IP.

```bash
KEY=$(op read "op://ZMB_CD_DEV/zombie-dev-worker-ant/ssh-private-key")
TAILSCALE_AUTHKEY=$(op read "op://ZMB_CD_PROD/tailscale/authkey")

ssh -i <(printf '%s\n' "$KEY") -o StrictHostKeyChecking=no root@<server-ip> << REMOTE
set -euo pipefail
curl -fsSL https://tailscale.com/install.sh | sh
tailscale up --authkey "$TAILSCALE_AUTHKEY" --hostname zombie-dev-worker-ant
tailscale status
REMOTE
```

### Acceptance

```bash
KEY=$(op read "op://ZMB_CD_DEV/zombie-dev-worker-ant/ssh-private-key")

# SSH via Tailscale hostname (no more public IP needed)
ssh -i <(printf '%s\n' "$KEY") zombie-dev-worker-ant \
  "tailscale status | grep zombie-dev-worker-ant"
# Expected: zombie-dev-worker-ant  <tailscale-ip>  ...  active
```

All remaining steps use the Tailscale hostname `zombie-dev-worker-ant`.

---

## 4.0 Agent: Install Docker

**Goal:** Docker daemon is running. Agent can pull GHCR public images.

```bash
KEY=$(op read "op://ZMB_CD_DEV/zombie-dev-worker-ant/ssh-private-key")

ssh -i <(printf '%s\n' "$KEY") zombie-dev-worker-ant << 'REMOTE'
set -euo pipefail
apt-get update -qq
apt-get install -y docker.io
systemctl enable docker
systemctl start docker
docker pull ghcr.io/usezombie/zombied:dev-latest
REMOTE
```

### Acceptance

```bash
KEY=$(op read "op://ZMB_CD_DEV/zombie-dev-worker-ant/ssh-private-key")
ssh -i <(printf '%s\n' "$KEY") zombie-dev-worker-ant \
  "docker images ghcr.io/usezombie/zombied | grep dev-latest"
# Expected: zombied  dev-latest  ...
```

---

## 5.0 Agent: Install Firecracker

**Goal:** Firecracker binary is installed. The `debian` user can access `/dev/kvm` without root (required for M4_008 sandbox execution — each spec run boots a Firecracker microVM).

```bash
KEY=$(op read "op://ZMB_CD_DEV/zombie-dev-worker-ant/ssh-private-key")

ssh -i <(printf '%s\n' "$KEY") zombie-dev-worker-ant << 'REMOTE'
set -euo pipefail
ARCH=$(uname -m)
FC_VERSION=v1.7.0
curl -fsSL \
  "https://github.com/firecracker-microvm/firecracker/releases/download/${FC_VERSION}/firecracker-${FC_VERSION}-${ARCH}.tgz" \
  | tar xz -C /usr/local/bin --strip-components=1
firecracker --version
usermod -aG kvm debian
REMOTE
```

### Acceptance

```bash
KEY=$(op read "op://ZMB_CD_DEV/zombie-dev-worker-ant/ssh-private-key")
ssh -i <(printf '%s\n' "$KEY") zombie-dev-worker-ant \
  "firecracker --version && ls -l /dev/kvm && groups debian | grep kvm"
# Expected:
#   Firecracker v1.7.0
#   crw-rw---- 1 root kvm ... /dev/kvm
#   debian : ... kvm
```

---

## 6.0 Agent: Bootstrap `/opt/zombie/`

**Goal:** `deploy.sh` and `.env` exist on the node, populated from vault. CI calls `deploy.sh` on every push.

```bash
KEY=$(op read "op://ZMB_CD_DEV/zombie-dev-worker-ant/ssh-private-key")

# Read secrets from vault (on the agent machine)
DB_URL=$(op read "op://ZMB_CD_DEV/planetscale-dev/connection-string")
REDIS_URL=$(op read "op://ZMB_CD_DEV/upstash-dev/url")
ENC_KEY=$(op read "op://ZMB_CD_DEV/zombied-local-config/encryption-master-key")
GH_APP_ID=$(op read "op://ZMB_CD_DEV/github-app/app-id")
GH_APP_KEY=$(op read "op://ZMB_CD_DEV/github-app/private-key")

ssh -i <(printf '%s\n' "$KEY") zombie-dev-worker-ant << REMOTE
set -euo pipefail

mkdir -p /opt/zombie
chown debian:debian /opt/zombie

cat > /opt/zombie/deploy.sh << 'EOF'
#!/usr/bin/env bash
set -euo pipefail
IMAGE="ghcr.io/usezombie/zombied:dev-latest"
echo "Pulling \$IMAGE..."
docker pull "\$IMAGE"
docker stop zombied-worker 2>/dev/null || true
docker rm   zombied-worker 2>/dev/null || true
docker run -d \
  --name zombied-worker \
  --restart unless-stopped \
  --device /dev/kvm \
  --env-file /opt/zombie/.env \
  "\$IMAGE" \
  zombied worker
echo "Done."
docker ps --filter name=zombied-worker
EOF
chmod +x /opt/zombie/deploy.sh

cat > /opt/zombie/.env << EOF
DATABASE_URL_WORKER=$DB_URL
REDIS_URL_WORKER=$REDIS_URL
ENCRYPTION_MASTER_KEY=$ENC_KEY
GITHUB_APP_ID=$GH_APP_ID
GITHUB_APP_PRIVATE_KEY=$GH_APP_KEY
ENVIRONMENT=dev
EOF
chmod 600 /opt/zombie/.env
REMOTE
```

### Acceptance

```bash
KEY=$(op read "op://ZMB_CD_DEV/zombie-dev-worker-ant/ssh-private-key")
ssh -i <(printf '%s\n' "$KEY") zombie-dev-worker-ant \
  "stat -c '%a %n' /opt/zombie/deploy.sh /opt/zombie/.env"
# Expected:
#   755 /opt/zombie/deploy.sh
#   600 /opt/zombie/.env
```

---

## 7.0 Agent: Smoke Test + Activate CI

**Goal:** `deploy.sh` runs end-to-end. Container stays up. CI is activated.

```bash
KEY=$(op read "op://ZMB_CD_DEV/zombie-dev-worker-ant/ssh-private-key")

# Run deploy.sh
ssh -i <(printf '%s\n' "$KEY") zombie-dev-worker-ant \
  "cd /opt/zombie && ./deploy.sh"

# Verify container health
ssh -i <(printf '%s\n' "$KEY") zombie-dev-worker-ant << 'REMOTE'
sleep 5
docker ps --filter name=zombied-worker --format "{{.Status}}" | grep -i "^Up"
docker inspect zombied-worker --format '{{range .HostConfig.Devices}}{{.PathOnHost}}{{end}}' | grep kvm
docker logs zombied-worker 2>&1 | tail -5
REMOTE

# Activate CI — set GitHub variable so deploy-dev-worker job runs
gh variable set DEV_WORKER_READY --body "true" --repo usezombie/usezombie
echo "CI activated. Next push to main will deploy to zombie-dev-worker-ant."
```

### Acceptance

```
Up X seconds          ← container running
/dev/kvm              ← KVM device passed in
<worker log lines>    ← no crash or panic
DEV_WORKER_READY set  ← CI guard lifted
```

---

## Sequence Summary

```
0.0  Human: Buy server → get IP + root creds
1.0  Agent: Deploy key → vault → authorized_keys (root no longer needed)
2.0  Agent: kvm-ok (abort if fails — no bare-metal without KVM)
3.0  Agent: Tailscale install + join (switch to hostname, drop public IP)
4.0  Agent: Docker install + GHCR pull verify
5.0  Agent: Firecracker install + kvm group
6.0  Agent: /opt/zombie/ bootstrap from vault
7.0  Agent: Smoke test + gh variable set DEV_WORKER_READY=true
```

**PROD workers** follow the same sequence. Replace `ZMB_CD_DEV` → `ZMB_CD_PROD`, image tag `dev-latest` → `latest`, `ENVIRONMENT=dev` → `ENVIRONMENT=prod`, hostnames accordingly.
