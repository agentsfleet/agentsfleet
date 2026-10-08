#!/usr/bin/env bash
# toolbox.sh — stage the runner's toolbox where the runner admits it at boot.
#
# Sourced by deploy.sh, which runs as root on the host and sits beside this
# file in /opt/agentsfleet/deploy (the runner playbook copies both). Defines
# functions only; sourcing it changes nothing on the host.
#
# The runner verifies the manifest's signature against the key it is built
# with, stages the image and mounts it (`afr_sandbox::Toolboxes`). A host
# without an admitted toolbox refuses every lease, so a set that is incomplete
# is refused here, before the restart, where the cause is easy to read.

# Where the runner keeps its state unless RUNNER_STORAGE_HOME in the env file
# says otherwise (`afr_supervisor::config::DEFAULT_STORAGE_HOME`), and the
# directory under it the runner reads a staged toolbox from at boot.
readonly RUNNER_STORAGE_HOME_DEFAULT="/var/lib/agentsfleet-runner"
readonly TOOLBOX_INCOMING_SUBDIR="toolbox/incoming"
# The three files one toolbox is: the image named by its digest, the release
# manifest, and cosign's signature over the manifest's exact bytes.
readonly TOOLBOX_SUFFIXES=(erofs json json.sig)

# The runner's storage home as the env file sets it, else the runner's default.
# `env_file` is injectable so deploy_test.sh can read a fixture (production
# passes nothing).
storage_home() {
  local env_file="${1:-$ENV_FILE}"
  local configured
  configured=$(sed -n 's/^RUNNER_STORAGE_HOME=//p' "$env_file" 2>/dev/null | head -1)
  printf '%s\n' "${configured:-$RUNNER_STORAGE_HOME_DEFAULT}"
}

# Moves the one toolbox under `src` into the runner's incoming directory,
# replacing whatever set was there, so the runner admits exactly this release
# at its next boot. Refuses a directory with no image, several, or a file of
# the set missing: the runner would refuse the whole set, after the restart,
# where the cause is harder to read. `incoming` is injectable for the same
# reason `storage_home`'s file is.
install_toolbox() {
  local src="$1"
  local incoming="${2:-$(storage_home)/${TOOLBOX_INCOMING_SUBDIR}}"
  local images=("$src"/toolbox-*.erofs)
  [[ "${#images[@]}" -eq 1 && -f "${images[0]}" ]] \
    || die "expected exactly one toolbox-<sha256>.erofs under $src"
  local name digest
  name="$(basename "${images[0]}")"
  digest="${name#toolbox-}"
  digest="${digest%.erofs}"
  local suffix
  for suffix in "${TOOLBOX_SUFFIXES[@]}"; do
    [[ -f "$src/toolbox-$digest.$suffix" ]] \
      || die "toolbox file missing: $src/toolbox-$digest.$suffix"
  done

  install -d -m 755 "$incoming"
  find "$incoming" -maxdepth 1 -name 'toolbox-*' -type f -delete
  for suffix in "${TOOLBOX_SUFFIXES[@]}"; do
    install -m 644 "$src/toolbox-$digest.$suffix" "$incoming/toolbox-$digest.$suffix"
  done
  log "Staged toolbox ${digest} → ${incoming}"
}
