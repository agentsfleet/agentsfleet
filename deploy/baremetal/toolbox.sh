#!/usr/bin/env bash
# toolbox.sh — what one runner toolbox is, and staging it where the runner
# admits it at boot.
#
# Sourced by deploy.sh on the host, by the runner playbook before it copies a
# set to a host (playbooks/lib/runner/deploy.sh), and by the build-toolbox
# action when it names and bundles one (.github/actions/build-toolbox), so the
# three agree on what a set is. Defines constants and functions only; sourcing
# it changes nothing. Every function takes its paths as arguments, writes a
# refusal to stderr and returns 1, and leaves the caller to decide what a
# refusal ends.
#
# The runner verifies the manifest's signature against the key it is built
# with, stages the image and mounts it (`afr_sandbox::Toolboxes`). A host
# without an admitted toolbox refuses every lease, so a set that is incomplete
# is refused before the restart, where the cause is easy to read.

# Where the runner keeps its state unless RUNNER_STORAGE_HOME in the env file
# says otherwise (`afr_supervisor::config::DEFAULT_STORAGE_HOME`), and the
# directory under it the runner reads a staged toolbox from at boot.
readonly RUNNER_STORAGE_HOME_DEFAULT="/var/lib/agentsfleet-runner"
# shellcheck disable=SC2034  # deploy.sh's stage_toolbox joins it to storage_home
readonly TOOLBOX_INCOMING_SUBDIR="toolbox/incoming"
# The three files one toolbox is: the image named by its digest, the release
# manifest, and cosign's signature over the manifest's exact bytes.
readonly TOOLBOX_SUFFIXES=(erofs json json.sig)

# The runner's storage home as `env_file` sets it, else the runner's default.
storage_home() {
  local env_file="$1"
  local configured
  configured=$(sed -n 's/^RUNNER_STORAGE_HOME=//p' "$env_file" 2>/dev/null | head -1)
  printf '%s\n' "${configured:-$RUNNER_STORAGE_HOME_DEFAULT}"
}

# The digest a toolbox file is named by: toolbox-<digest>.erofs.
toolbox_digest_of() {
  local name
  name="$(basename "$1")"
  name="${name#toolbox-}"
  printf '%s\n' "${name%.erofs}"
}

# Prints the paths of the one toolbox set under `dir`, image first, one per
# line. Refuses, printing no path, unless `dir` holds exactly one
# toolbox-<sha256>.erofs and every other file of its set: the runner would
# refuse the whole set after the restart, where the cause is harder to read.
toolbox_set_files() {
  local dir="$1"
  local images=("$dir"/toolbox-*.erofs)
  if [[ "${#images[@]}" -ne 1 || ! -f "${images[0]}" ]]; then
    printf 'expected exactly one toolbox-<sha256>.erofs under %s\n' "$dir" >&2
    return 1
  fi
  local digest suffix file files=()
  digest="$(toolbox_digest_of "${images[0]}")"
  for suffix in "${TOOLBOX_SUFFIXES[@]}"; do
    file="$dir/toolbox-$digest.$suffix"
    if [[ ! -f "$file" ]]; then
      printf 'toolbox file missing: %s\n' "$file" >&2
      return 1
    fi
    files+=("$file")
  done
  printf '%s\n' "${files[@]}"
}

# Moves the one toolbox under `src` into `incoming`, replacing whatever set was
# there, so the runner admits exactly this release at its next boot. A set
# toolbox_set_files refuses is never staged, and what was staged before stays.
install_toolbox() {
  local src="$1" incoming="$2"
  local files file
  files="$(toolbox_set_files "$src")" || return 1
  install -d -m 755 "$incoming" || return 1
  find "$incoming" -maxdepth 1 -name 'toolbox-*' -type f -delete || return 1
  while IFS= read -r file; do
    install -m 644 "$file" "$incoming/$(basename "$file")" || return 1
  done <<<"$files"
}
