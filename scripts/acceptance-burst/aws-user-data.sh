#!/bin/bash
# EC2 user data for one burst acceptance run on Debian 13 (amd64), as root.
# acceptance-burst.sh prepends the BURST_* assignments. The instance reports
# only through its S3 prefix: run.log every minute, then results.tar.gz and
# exit-code. It shuts down, which terminates it, on success, failure or deadline.
set -uo pipefail

: "${BURST_BUCKET:?}" "${BURST_PREFIX:?}" "${BURST_REF:?}" "${BURST_SUITE:?}" "${BURST_REGION:?}"
: "${BURST_VAULT:?}" "${BURST_OP_PARAMETER:?}" "${BURST_DEADLINE_MINUTES:?}" "${BURST_BUN_VERSION:?}"

shutdown -h +"$BURST_DEADLINE_MINUTES"
readonly LOG=/var/log/acceptance-burst.log WORK=/work/agentsfleet
exec >"$LOG" 2>&1

put() { aws s3 cp "$1" "s3://$BURST_BUCKET/$BURST_PREFIX/$2" --region "$BURST_REGION" --only-show-errors; }

finish() {
  local status=$?
  put "$LOG" run.log
  if [ -d "$WORK/ui/packages/app" ]; then
    tar -C "$WORK/ui/packages/app" -czf /tmp/results.tar.gz \
      playwright-acceptance-results playwright-acceptance-report 2>/dev/null && put /tmp/results.tar.gz results.tar.gz
  fi
  printf '%s\n' "$status" >/tmp/exit-code
  put /tmp/exit-code exit-code
  shutdown -h now
}
trap finish EXIT

(while sleep 60; do put "$LOG" run.log; done) &

set -e
export DEBIAN_FRONTEND=noninteractive HOME=/root
echo "== packages"
apt-get update -qq
apt-get install -y -qq --no-install-recommends ca-certificates curl git gnupg jq unzip \
  docker.io libnss3-tools cmake clang build-essential pkg-config libssl-dev python3 openssl awscli
systemctl start docker

echo "== kernel probe"
modprobe erofs
grep -qw erofs /proc/filesystems
grep -qw securityfs /proc/filesystems
grep -q landlock /sys/kernel/security/lsm
test -e /dev/loop-control
docker run --rm --privileged mirror.gcr.io/library/debian:trixie-slim true

echo "== 1Password command-line tool"
curl -fsSL https://downloads.1password.com/linux/keys/1password.asc | gpg --dearmor -o /usr/share/keyrings/1password-archive-keyring.gpg
echo "deb [signed-by=/usr/share/keyrings/1password-archive-keyring.gpg] https://downloads.1password.com/linux/debian/amd64 stable main" \
  >/etc/apt/sources.list.d/1password.list
apt-get update -qq && apt-get install -y -qq 1password-cli

echo "== toolchains"
curl -fsSL https://bun.sh/install | bash -s "bun-v$BURST_BUN_VERSION"
curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal
export PATH="$HOME/.bun/bin:$HOME/.cargo/bin:$PATH"

echo "== checkout $BURST_REF"
aws s3 cp "s3://$BURST_BUCKET/$BURST_PREFIX/repo.bundle" /tmp/repo.bundle --region "$BURST_REGION" --only-show-errors
git clone --quiet --branch "$BURST_REF" /tmp/repo.bundle "$WORK"
aws s3 cp --recursive "s3://$BURST_BUCKET/$BURST_PREFIX/toolbox" /opt/toolbox --region "$BURST_REGION" --only-show-errors
cd "$WORK"
(cd rustd && rustup show >/dev/null)

echo "== dependencies (before any secret)"
bun install --frozen-lockfile
(cd cli && bun install --frozen-lockfile && bun run build)
for package in ui/packages/*/; do (cd "$package" && bun install --frozen-lockfile); done
(cd ui/packages/app && bunx playwright install --with-deps chromium)

echo "== credentials"
OP_SERVICE_ACCOUNT_TOKEN="$(aws ssm get-parameter --name "$BURST_OP_PARAMETER" --with-decryption \
  --query Parameter.Value --output text --region "$BURST_REGION")"
export OP_SERVICE_ACCOUNT_TOKEN
CLERK_SECRET_KEY="$(op read "op://$BURST_VAULT/clerk-dev/secret-key")"
CLERK_PUBLISHABLE_KEY="$(op read "op://$BURST_VAULT/clerk-dev/publishable-key")"
CLERK_WEBHOOK_SECRET="$(op read "op://$BURST_VAULT/clerk-dev/webhook-secret")"
OIDC_ISSUER="$(op read "op://$BURST_VAULT/clerk-dev/issuer")"
AUTH_SESSION_CODE_PEPPER="$(op read "op://$BURST_VAULT/auth-session-code-pepper/credential")"
FIREWORKS_API_KEY="$(op read "op://$BURST_VAULT/agentsfleet-fireworks-api-token/credential")"
export CLERK_SECRET_KEY CLERK_PUBLISHABLE_KEY CLERK_WEBHOOK_SECRET OIDC_ISSUER AUTH_SESSION_CODE_PEPPER FIREWORKS_API_KEY
while IFS= read -r assignment; do export "${assignment?}"; done < <(scripts/acceptance-burst/machine-env.sh /run/acceptance-env)

echo "== acceptance ($BURST_SUITE)"
ACCEPTANCE_LOCAL_TOOLBOX_DIR=/opt/toolbox ACCEPTANCE_LOCAL_CONNECTOR_VAULT="$BURST_VAULT" \
  make acceptance-codex SUITE="$BURST_SUITE"
