#!/usr/bin/env bash
# build.sh — build the runner's toolbox: a reproducible, compressed, read-only
# EROFS image, named by its SHA-256.
#
#     bash scripts/toolbox/build.sh <output-dir>
#
# Prints the image's path. Everything that could vary between two builds is
# pinned: the packages by a snapshot.debian.org timestamp, every file's time by
# SOURCE_DATE_EPOCH (derived from that same timestamp), and the file system's
# UUID by the manifest. Two builds of one manifest on one architecture are
# byte-identical, which the runner's kernel lane proves.
#
# Needs mmdebstrap and mkfs.erofs (Debian/Ubuntu: `mmdebstrap erofs-utils`).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly HERE
readonly MANIFEST="${TOOLBOX_MANIFEST:-$HERE/manifest.txt}"
readonly SNAPSHOT_MIRROR="http://snapshot.debian.org/archive/debian"
# Mount points the sandbox binds onto; the image is read-only, so they must
# already exist in it. `/run` is not one: mmdebstrap empties it, and the
# sandbox mounts its own.
readonly MOUNT_POINTS=(workspace opt/agentsfleet)
readonly ENTRY_POINT="opt/agentsfleet/agentsfleet-runner"

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <output-dir>" >&2
  exit 2
fi
readonly OUT="$1"

suite="" snapshot="" uuid="" packages=()
while read -r key values; do
  case "$key" in
    ''|'#'*) ;;
    suite) suite="$values" ;;
    snapshot) snapshot="$values" ;;
    uuid) uuid="$values" ;;
    packages) read -r -a packages <<<"$values" ;;
    *) echo "build.sh: unknown manifest key '$key' in $MANIFEST" >&2; exit 2 ;;
  esac
done <"$MANIFEST"
for required in suite snapshot uuid; do
  if [ -z "${!required}" ]; then
    echo "build.sh: $MANIFEST names no $required" >&2
    exit 2
  fi
done

# 20260901T000000Z -> 2026-09-01T00:00:00Z -> seconds since the epoch.
stamp="${snapshot:0:4}-${snapshot:4:2}-${snapshot:6:2}T${snapshot:9:2}:${snapshot:11:2}:${snapshot:13:2}Z"
SOURCE_DATE_EPOCH="$(date -u -d "$stamp" +%s)"
export SOURCE_DATE_EPOCH

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

hooks=()
for point in "${MOUNT_POINTS[@]}"; do
  hooks+=("--customize-hook=mkdir -p \"\$1/$point\"")
done
hooks+=("--customize-hook=touch \"\$1/$ENTRY_POINT\"")

include="$(IFS=,; echo "${packages[*]}")"
mmdebstrap \
  --quiet \
  --variant=minbase \
  --include="$include" \
  --aptopt='Acquire::Check-Valid-Until "false"' \
  "${hooks[@]}" \
  "$suite" "$work/root.tar" "$SNAPSHOT_MIRROR/$snapshot"

mkfs.erofs -zlz4hc -T"$SOURCE_DATE_EPOCH" -U"$uuid" --all-root --quiet \
  --tar=f "$work/toolbox.erofs" "$work/root.tar"

digest="$(sha256sum "$work/toolbox.erofs" | cut -d' ' -f1)"
mkdir -p "$OUT"
image="$OUT/toolbox-$digest.erofs"
mv "$work/toolbox.erofs" "$image"
echo "$image"
