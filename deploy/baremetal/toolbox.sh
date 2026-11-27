#!/usr/bin/env bash
# toolbox.sh — what one runner toolbox is, and staging it where the runner
# admits it at boot; and what one guest set is, and installing it where the
# Firecracker engine boots each microVM from.
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
# with, stages the image and mounts it (`afr_image::Toolboxes`). A host
# without an admitted toolbox refuses every run, so a set that is incomplete
# is refused before the restart, where the cause is easy to read.
#
# The guest set travels in the same directory as the toolbox set: the monitor
# and its jailer, the guest kernel, the initramfs whose /init is this
# release's runner, and the record naming each one's SHA-256
# (scripts/toolbox/guest.sh). A host whose files differ from the record, or
# that lacks one, cannot boot a microVM, so that set is refused the same way.

# Where the runner keeps its state unless RUNNER_STORAGE_HOME in the env file
# says otherwise (`afr_supervisor::config::DEFAULT_STORAGE_HOME`), and the
# directory under it the runner reads a staged toolbox from at boot.
readonly RUNNER_STORAGE_HOME_DEFAULT="/var/lib/agentsfleet-runner"
# shellcheck disable=SC2034  # deploy.sh's stage_toolbox joins it to storage_home
readonly TOOLBOX_INCOMING_SUBDIR="toolbox/incoming"
# The three files one toolbox is: the image named by its digest, the release
# manifest, and cosign's signature over the manifest's exact bytes.
readonly TOOLBOX_SUFFIXES=(erofs json json.sig)
# Where the Firecracker engine looks for what it boots, unless the runner's
# file names another place (`afr_engine_firecracker::config`): the monitor and
# jailer among the host's programs, the kernel and initramfs beside them.
readonly GUEST_PROGRAM_DIR_DEFAULT="/usr/local/bin"
readonly GUEST_IMAGE_DIR_DEFAULT="/usr/local/lib/agentsfleet"
readonly GUEST_PROGRAMS=(firecracker jailer)
readonly GUEST_IMAGES=(vmlinux guest.cpio)
# The record of the four files above. The release pipeline signs it and checks
# that signature against the runner's key; a host checks each file against it.
readonly GUEST_RECORD="guest.json"

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

# Prints the paths of the guest set under `dir`, programs first, one per line.
# Refuses, printing no path, unless `dir` holds the record and every file it
# names, each with the SHA-256 recorded for it: a kernel, initramfs or monitor
# from another release boots a guest this runner cannot talk to, or none at
# all.
guest_set_files() {
  local dir="$1"
  local record="$dir/$GUEST_RECORD" name file recorded files=()
  if [[ ! -f "$record" ]]; then
    printf 'guest file missing: %s\n' "$record" >&2
    return 1
  fi
  for name in "${GUEST_PROGRAMS[@]}" "${GUEST_IMAGES[@]}"; do
    file="$dir/$name"
    if [[ ! -f "$file" ]]; then
      printf 'guest file missing: %s\n' "$file" >&2
      return 1
    fi
    recorded="$(jq -er --arg name "$name" '.artifacts[$name].sha256' "$record" 2>/dev/null)" || {
      printf '%s records no SHA-256 for %s\n' "$record" "$name" >&2
      return 1
    }
    if [[ "$(sha256sum "$file" | cut -d' ' -f1)" != "$recorded" ]]; then
      printf 'guest file is not the one %s records: %s\n' "$record" "$file" >&2
      return 1
    fi
    files+=("$file")
  done
  printf '%s\n' "${files[@]}" "$record"
}

# Installs the guest set under `src` where the engine boots from: the monitor
# and jailer into `program_dir`, the kernel, the initramfs and their record
# into `image_dir`, every file root's. A set guest_set_files refuses is never
# installed, and what was installed before stays.
#
# The runner copies the kernel and initramfs into each jail as root and gives
# the copies to that jail's user, and the jailer copies the monitor the same
# way, so nothing here need be readable by a jail user; the monitor's copy
# keeps this mode, and its jail user must be able to execute it.
install_guest() {
  local src="$1" program_dir="${2:-$GUEST_PROGRAM_DIR_DEFAULT}" image_dir="${3:-$GUEST_IMAGE_DIR_DEFAULT}"
  local name
  guest_set_files "$src" >/dev/null || return 1
  install -d -m 755 -o 0 -g 0 "$image_dir" || return 1
  for name in "${GUEST_IMAGES[@]}" "$GUEST_RECORD"; do
    install -m 644 -o 0 -g 0 "$src/$name" "$image_dir/$name" || return 1
  done
  for name in "${GUEST_PROGRAMS[@]}"; do
    install -m 755 -o 0 -g 0 "$src/$name" "$program_dir/$name" || return 1
  done
}
