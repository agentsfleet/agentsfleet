#!/usr/bin/env bash
# deploy.sh — install the agentsfleet-runner binary and its toolbox, and restart
# the systemd service.
#
# Two modes:
#   Local:    deploy.sh runner <version> <binary-path> <toolbox-dir>
#             Installs from local files (CI copied the binary and the toolbox's
#             image, manifest and signature to the server).
#
#   Release:  deploy.sh runner <version>
#             Downloads the offline bundle from GitHub Releases (tagged release
#             deploys); it carries the binary and the toolbox together.
#
# The toolbox lands in the runner's incoming directory, and the runner admits it
# at boot: it verifies the manifest's signature against the key it is built
# with, stages the image and mounts it. A host without an admitted toolbox
# refuses every lease, so a deploy that lacks one stops before it writes
# anything to the host, as does every other refusal (see main).
#
# Environment:
#   DISCORD_WEBHOOK_URL — if set, sends deploy status to Discord
#   DEPLOY_HOSTNAME     — override hostname in notifications (default: $(hostname))
#   DRAIN_TIMEOUT       — seconds to wait for a graceful stop (default: 120)
#
# At most one deploy runs per host: main() takes a non-blocking flock and exits
# non-zero when another deploy already holds it. Sourcing this file runs no deploy
# — deploy_test.sh relies on that to exercise the functions directly.
#
# The runner holds zero datastore credentials, so an abrupt stop is safe: the
# control plane reclaims its in-flight lease (see drain_runner).

set -euo pipefail

# Sourcing this file must never deploy: deploy_test.sh sources it to reach the
# individual functions, so both the stdbuf re-exec below and `main` at the bottom
# stay behind this guard.
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  readonly DEPLOY_EXECUTED=1
else
  readonly DEPLOY_EXECUTED=0
fi

# Force line-buffered stdout/stderr so log output streams through SSH in real time.
if [[ "$DEPLOY_EXECUTED" == 1 && -z "${_DEPLOY_UNBUFFERED:-}" ]] && command -v stdbuf >/dev/null 2>&1; then
  export _DEPLOY_UNBUFFERED=1
  exec stdbuf -oL -eL "$0" "$@"
fi

# Load Discord webhook from the env file when not already in the environment.
# Reading the file here keeps the value out of sudo's argument list and therefore
# out of ps/cmdline output.
readonly _DISCORD_ENV_FILE="/opt/agentsfleet/.discord-env"
if [[ -z "${DISCORD_WEBHOOK_URL:-}" && -r "${_DISCORD_ENV_FILE}" ]]; then
  # shellcheck source=/dev/null
  source "${_DISCORD_ENV_FILE}"
fi

readonly REPO="agentsfleet/agentsfleet"
readonly INSTALL_DIR="/usr/local/bin"
readonly SYSTEMD_DIR="/etc/systemd/system"
readonly DEPLOY_DIR="/opt/agentsfleet/deploy"
readonly ENV_FILE="/opt/agentsfleet/.env"
readonly ENV_DEST="/etc/default/agentsfleet-runner"
# Staging the toolbox the runner admits at boot (`install_toolbox`) and
# restarting and health-checking the unit (`restart_services`,
# `verify_healthy`), beside this file on the host as in the repository.
# shellcheck source=./toolbox.sh
source "$(dirname "${BASH_SOURCE[0]}")/toolbox.sh"
# shellcheck source=./service.sh
source "$(dirname "${BASH_SOURCE[0]}")/service.sh"
readonly HOST="${DEPLOY_HOSTNAME:-$(hostname)}"

# The single deployable component. Kept as an explicit argument so the call site
# names what it deploys; the resolver rejects any other value (catches stale
# callers still passing a retired component name).
readonly COMPONENT_RUNNER="runner"
readonly BINARY_NAME="agentsfleet-runner"
readonly SERVICE_NAME="agentsfleet-runner.service"
# The release's offline bundle is arch-specific. CI's local mode skips this —
# it copies the right-arch binary and toolbox and passes their paths.
case "$(uname -m)" in
  x86_64 | amd64) _arch="amd64" ;;
  aarch64 | arm64) _arch="arm64" ;;
  *) _arch="$(uname -m)" ;;
esac
readonly RELEASE_BINARY="${BINARY_NAME}-linux-${_arch}"
readonly RELEASE_BUNDLE="${BINARY_NAME}-bundle-linux-${_arch}"

# Serializes install + `systemctl restart`, which is not atomic: a manual run and
# a cancel-orphaned CI run can otherwise interleave on the same host. flock beats a
# lock file — the kernel drops it when the holder dies, so a SIGKILLed deploy never
# strands later ones. Overridable only so deploy_test.sh can use a writable temp
# path (/var/lock is root-owned; the tests are not root). Production never sets it.
readonly DEPLOY_LOCK_PATH="${DEPLOY_LOCK_PATH:-/var/lock/agentsfleet-deploy.lock}"

# `agentsfleet-runner --version` prints `agentsfleet-runner <version>`: clap's
# `version` flag over the workspace version (`rustd/crates/agentsfleet_runner/src/main.rs`).
# The version is whitespace-delimited field 2.
readonly VERSION_FIELD_INDEX=2

# ── Logging ──────────────────────────────────────────────────────────────────

log()  { echo "[deploy] $*"; }
die()  { log "FATAL: $*"; exit 1; }

# Exits quietly: a run refused because another deploy holds the
# lock is not a failure, and a "deploy FAILED" embed would page for a working deploy.
die_unnotified() { log "FATAL: $*"; exit 1; }

# ── Version check ────────────────────────────────────────────────────────────

# Exact equality, never a substring: `0.1.0-rc1` contains `0.1.0` and `0.10.2`
# contains `0.1`, so a glob match skips a real upgrade, leaves the old binary
# running, and reports success.
version_token_matches() {
  local version_output="$1"
  local target="${2#v}"

  local token
  token=$(printf '%s\n' "$version_output" \
    | awk -v field="$VERSION_FIELD_INDEX" 'NR == 1 && NF >= field { print $field }')

  [[ -n "$token" && "$token" == "$target" ]]
}

# `dest` is injectable so the test can point at a stub binary (production passes
# nothing). An unreadable or unexpected `--version` shape yields no token → reports
# "not installed"; a redundant reinstall is safe, a wrong skip is not.
is_already_installed() {
  local dest="${1:-${INSTALL_DIR}/${BINARY_NAME}}"
  [[ -x "$dest" ]] || return 1

  local current
  current=$("$dest" --version 2>/dev/null || true)
  version_token_matches "$current" "$VERSION" || return 1

  log "✓ ${BINARY_NAME} ${VERSION} already installed — ensuring service is up."
  systemctl enable "$SERVICE_NAME" || return 1
  systemctl is-active --quiet "$SERVICE_NAME" && return 0

  # Not active: start it AND verify it stays up. `systemctl start` exits zero once
  # systemd accepts the job, so a runner that starts then dies would otherwise skip
  # to "ok" over a dead service. Any failure → report not-installed so the caller
  # runs the full reinstall path, which surfaces the real fault.
  if ! systemctl start "$SERVICE_NAME" || ! verify_healthy; then
    log "✗ ${SERVICE_NAME} is installed but will not stay up — forcing a full redeploy."
    return 1
  fi
  return 0
}

# ── Deploy mutex ─────────────────────────────────────────────────────────────

# Holds the lock on a descriptor open for the life of the process, so it releases
# on any exit — normal, fatal, or killed. Non-blocking: an operator wants to hear
# "a deploy is already running", not queue silently behind one.
acquire_deploy_lock() {
  command -v flock >/dev/null 2>&1 \
    || die_unnotified "flock not found — install util-linux; refusing to deploy without a mutex."

  exec {DEPLOY_LOCK_FD}>"$DEPLOY_LOCK_PATH" \
    || die_unnotified "cannot open deploy lock $DEPLOY_LOCK_PATH"

  flock -n "$DEPLOY_LOCK_FD" \
    || die_unnotified "another deploy holds $DEPLOY_LOCK_PATH — refusing to run install+restart concurrently."
}

# ── Binary acquisition ───────────────────────────────────────────────────────

install_binary() {
  local src="$1"
  log "Installing ${BINARY_NAME} from $src"
  install -m 755 "$src" "${INSTALL_DIR}/${BINARY_NAME}"
}

# The bundle is built on a Continuous Integration (CI) runner, so every member,
# `./` included, carries that runner's uid. Unpacked as root with tar's
# defaults, `./` would hand the directory, and the binary in it, to whichever
# host account has that uid before root installs it; these keep root the owner
# and the directory's own 0700.
readonly BUNDLE_EXTRACT_FLAGS=(--no-same-owner --no-overwrite-dir)

# Downloads the release's offline bundle and unpacks it into RELEASE_DIR, a
# fresh directory removed on exit. Changes nothing else on the host.
fetch_release() {
  local url="https://github.com/${REPO}/releases/download/${VERSION}/${RELEASE_BUNDLE}.tar.gz"
  RELEASE_DIR=$(mktemp -d)
  # shellcheck disable=SC2064
  trap "rm -rf '$RELEASE_DIR'" EXIT

  log "Downloading ${RELEASE_BUNDLE} ${VERSION} ..."
  curl -fsSL -o "${RELEASE_DIR}/${RELEASE_BUNDLE}.tar.gz" "$url" \
    || die "Download failed. Check that release ${VERSION} includes ${RELEASE_BUNDLE}."
  tar xzf "${RELEASE_DIR}/${RELEASE_BUNDLE}.tar.gz" -C "$RELEASE_DIR" "${BUNDLE_EXTRACT_FLAGS[@]}"
}

# ── Systemd sync ─────────────────────────────────────────────────────────────

sync_systemd_unit() {
  local src="${DEPLOY_DIR}/${SERVICE_NAME}"
  [[ -f "$src" ]] || return 0
  cp "$src" "${SYSTEMD_DIR}/${SERVICE_NAME}"
  systemctl daemon-reload
  log "Synced ${SERVICE_NAME} → systemd."
}

# Refuses the env file sync_env would install. `env_file` is injectable so
# deploy_test.sh can read a fixture (production passes nothing).
check_env_file() {
  local env_file="${1:-$ENV_FILE}"
  [[ -f "$env_file" ]] \
    || die "missing $env_file — deploy through playbooks/lib/runner/deploy.sh"

  # Fail loud when any required runner env var is absent. The runner's own boot
  # check (`afr_supervisor::Config::from_env`) would catch this too, but a
  # 1/FAILURE systemd loop with `run_failed` is a confusing surface for an
  # operator — die here with the specific missing keys instead.
  local required=(AGENTSFLEET_API_URL AGENTSFLEET_RUNNER_TOKEN)
  local missing=()
  local k
  for k in "${required[@]}"; do
    grep -qE "^${k}=" "$env_file" || missing+=("$k")
  done
  if [[ ${#missing[@]} -gt 0 ]]; then
    die "missing required runner env vars in $env_file: ${missing[*]}"
  fi

  # Reject the documented placeholder shape (`agt_rFAKE_…`). The daemon's prefix
  # check only enforces `agt_r*`, which a placeholder satisfies — that would
  # loop on 401s. Better to fail at deploy time with a clear cause.
  if grep -qE '^AGENTSFLEET_RUNNER_TOKEN=agt_rFAKE' "$env_file"; then
    die "AGENTSFLEET_RUNNER_TOKEN in $env_file is the placeholder; mint a real agt_r via POST /v1/runners and update 1Password before re-running"
  fi
}

sync_env() {
  cp "$ENV_FILE" "$ENV_DEST"
  log "Synced .env → ${ENV_DEST}"
}

# Every check that can refuse this deploy, run before its first write to the
# host. A binary replaced ahead of a refusal boots at the next restart against
# the toolbox the previous release staged, and the runner refuses a manifest
# that does not name its own version, so the host would stop serving long
# after the deploy that broke it.
check_deploy_inputs() {
  local binary="$1" toolbox_dir="$2" env_file="${3:-$ENV_FILE}"
  [[ -f "$binary" ]] || die "runner binary not found: $binary"
  check_toolbox_set "$toolbox_dir"
  check_env_file "$env_file"
}

# ── Main ─────────────────────────────────────────────────────────────────────

main() {
  if [[ $# -lt 2 || $# -gt 4 || $# -eq 3 ]]; then
    echo "Usage: deploy.sh runner <version> [binary-path toolbox-dir]"
    echo "  version:     GitHub release tag (e.g. v0.1.0) or dev SHA (e.g. dev-abc1234)"
    echo "  binary-path: local path to the pre-staged binary; with it, toolbox-dir is required"
    echo "  toolbox-dir: local directory holding toolbox-<sha256>.{erofs,json,json.sig}"
    echo "  Both omitted: the release's offline bundle is downloaded from GitHub Releases."
    exit 1
  fi

  COMPONENT="$1"
  VERSION="$2"
  LOCAL_BINARY="${3:-}"
  LOCAL_TOOLBOX="${4:-}"

  [[ "$COMPONENT" == "$COMPONENT_RUNNER" ]] \
    || die "Unknown component '$COMPONENT'. The only deployable component is '${COMPONENT_RUNNER}'."

  # After argument validation, before anything that touches the host: a usage or
  # bad-component error needs no lock, and must not fail on an unwritable /var/lock.
  acquire_deploy_lock

  # Skip version check when CI provides a local binary — always do a full
  # install+restart cycle. The shortcut is only for release-download mode.
  if [[ -z "$LOCAL_BINARY" ]] && is_already_installed; then
    return 0
  fi

  local binary="$LOCAL_BINARY" toolbox_dir="$LOCAL_TOOLBOX"
  if [[ -z "$LOCAL_BINARY" ]]; then
    fetch_release
    binary="${RELEASE_DIR}/${RELEASE_BINARY}"
    toolbox_dir="$RELEASE_DIR"
  fi

  check_deploy_inputs "$binary" "$toolbox_dir"
  install_binary "$binary"
  install_toolbox "$toolbox_dir"
  sync_systemd_unit
  sync_env
  restart_services

  if verify_healthy; then
    log "Deploy complete: ${BINARY_NAME} ${VERSION}"
  else
    exit 1
  fi
}

if [[ "$DEPLOY_EXECUTED" == 1 ]]; then
  main "$@"
fi
