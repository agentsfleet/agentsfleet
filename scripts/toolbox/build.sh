#!/usr/bin/env bash
# build.sh — build the runner's toolbox: a reproducible, compressed, read-only
# EROFS image, named by its SHA-256, and the release manifest beside it.
#
#     bash scripts/toolbox/build.sh <output-dir>
#
# Prints the image's path; the manifest is the same path with `.json` for
# `.erofs`. Everything the manifest can pin is pinned: the packages by one
# snapshot.debian.org timestamp across main, updates and security, every
# file's time by SOURCE_DATE_EPOCH (derived from that same timestamp), the
# file system's UUID, compressor and block size, the archive key the packages
# are checked against, and every binary Debian does not ship by URL and
# SHA-256.
#
# What reproducible means here, exactly: two builds of one manifest on one
# architecture, by the same versions of mmdebstrap, apt, dpkg and erofs-utils,
# are byte-identical — the runner's kernel lane proves it. A different host
# tool version may lay the image out differently, so the release names the
# digest it shipped and the runner refuses any other.
#
# The release manifest is unsigned here. The runner admits an image only on a
# signature over the manifest's exact bytes, made by the release process (or,
# in the kernel lane, by its fixture key).
#
# Runs as root, or as a user with subordinate ID ranges in /etc/subuid and
# /etc/subgid, onto which mmdebstrap's unshare mode maps the root's users.
# Needs mmdebstrap, mkfs.erofs, dump.erofs, curl, python3 and Debian's archive
# keyring (Debian/Ubuntu: `mmdebstrap erofs-utils curl python3
# debian-archive-keyring`, and `uidmap` to build unprivileged).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly HERE
readonly MANIFEST="${TOOLBOX_MANIFEST:-$HERE/manifest.txt}"
readonly SNAPSHOT_ROOT="http://snapshot.debian.org/archive"
# The key every package is verified against, named rather than left to
# whatever the host's apt happens to trust.
readonly KEYRING="${TOOLBOX_KEYRING:-/usr/share/keyrings/debian-archive-keyring.gpg}"
# The runner versions this image serves; the runner refuses an image that
# does not name its own. Defaults to the repository's version.
readonly RUNNER_VERSIONS="${TOOLBOX_RUNNER_VERSIONS:-$(tr -d '[:space:]' <"$HERE/../../VERSION")}"
# Mount points the sandbox binds onto; the image is read-only, so they must
# already exist in it. `/run` is not one: mmdebstrap empties it, and the
# sandbox mounts its own.
readonly MOUNT_POINTS=(workspace opt/agentsfleet)
readonly ENTRY_POINT="opt/agentsfleet/agentsfleet-runner"
# Where vendored binaries are installed inside the image.
readonly VENDOR_BIN="usr/local/bin"
# Where the package list is written inside the root before it is carried out.
readonly PACKAGES_INSIDE="tmp/toolbox-packages.txt"
# mmdebstrap copies the builder's resolver configuration and hostname into the
# root, so two builders would ship two images. Both files get fixed content
# rather than being removed: the per-lease network allowlist binds rendered
# resolver files in (docs/architecture/runner_execution.md §Sandbox engines),
# and a bind onto this read-only image needs a target that already exists.
readonly RESOLVER_FILE="etc/resolv.conf"
readonly HOSTNAME_FILE="etc/hostname"
# The image's own /etc/hosts maps this name, so reading it sends no query.
readonly IMAGE_HOSTNAME="localhost"

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <output-dir>" >&2
  exit 2
fi
readonly OUT="$1"

arch="$(dpkg --print-architecture)"
suite="" snapshot="" uuid="" erofs=() packages=() archives=() vendors=()
while read -r key values; do
  case "$key" in
    ''|'#'*) ;;
    suite) suite="$values" ;;
    snapshot) snapshot="$values" ;;
    uuid) uuid="$values" ;;
    erofs) read -r -a erofs <<<"$values" ;;
    packages) read -r -a packages <<<"$values" ;;
    archive) archives+=("$values") ;;
    vendor) vendors+=("$values") ;;
    *) echo "build.sh: unknown manifest key '$key' in $MANIFEST" >&2; exit 2 ;;
  esac
done <"$MANIFEST"
for required in suite snapshot uuid; do
  if [ -z "${!required}" ]; then
    echo "build.sh: $MANIFEST names no $required" >&2
    exit 2
  fi
done
if [ "${#archives[@]}" -eq 0 ] || [ "${#erofs[@]}" -eq 0 ]; then
  echo "build.sh: $MANIFEST names no archive or no erofs options" >&2
  exit 2
fi

# 20260901T000000Z -> 2026-09-01T00:00:00Z -> seconds since the epoch.
stamp="${snapshot:0:4}-${snapshot:4:2}-${snapshot:6:2}T${snapshot:9:2}:${snapshot:11:2}:${snapshot:13:2}Z"
SOURCE_DATE_EPOCH="$(date -u -d "$stamp" +%s)"
export SOURCE_DATE_EPOCH

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# Every vendored binary for this architecture, fetched, checked and unpacked
# before the root is built, so a wrong download never reaches an image. Files
# cross into and out of the root only through mmdebstrap's `copy-in` and
# `download` hooks, which run as this user: in unshare mode the namespace's
# root cannot read this user's files.
installs=()
mkdir "$work/vendor-bin"
: >"$work/vendored.tsv"
for line in "${vendors[@]}"; do
  read -r v_arch url sha256 members <<<"$line"
  [ "$v_arch" = "$arch" ] || continue
  archive="$work/vendor-$(basename "$url")"
  curl --fail --silent --show-error --location --output "$archive" "$url"
  if [ "$(sha256sum "$archive" | cut -d' ' -f1)" != "$sha256" ]; then
    echo "build.sh: $url does not hash to $sha256" >&2
    exit 1
  fi
  printf '%s\t%s\n' "$url" "$sha256" >>"$work/vendored.tsv"
  read -r -a pairs <<<"$members"
  for pair in "${pairs[@]}"; do
    member="${pair%%=*}" name="${pair#*=}"
    tar -xzf "$archive" -O "$member" >"$work/vendor-bin/$name"
    chmod 0755 "$work/vendor-bin/$name"
    installs+=("--customize-hook=copy-in $work/vendor-bin/$name /$VENDOR_BIN")
  done
done

hooks=()
for point in "${MOUNT_POINTS[@]}"; do
  hooks+=("--customize-hook=mkdir -p \"\$1/$point\"")
done
hooks+=("--customize-hook=touch \"\$1/$ENTRY_POINT\"")
# The mode is set too: mmdebstrap copies it from the builder's own file.
hooks+=("--customize-hook=: >\"\$1/$RESOLVER_FILE\" && chmod 0644 \"\$1/$RESOLVER_FILE\"")
hooks+=("--customize-hook=echo $IMAGE_HOSTNAME >\"\$1/$HOSTNAME_FILE\" && chmod 0644 \"\$1/$HOSTNAME_FILE\"")
hooks+=("${installs[@]}")
# Every installed package, its version and the SHA-256 apt verified it by,
# read while the archive indexes are still in the root.
hooks+=("--customize-hook=chroot \"\$1\" sh -c 'dpkg-query -W -f \"\\\${binary:Package}=\\\${Version}\\n\" | xargs apt-cache show --no-all-versions >/$PACKAGES_INSIDE'")
hooks+=("--customize-hook=download /$PACKAGES_INSIDE $work/packages.txt")
hooks+=("--customize-hook=rm \"\$1/$PACKAGES_INSIDE\"")

sources=()
for entry in "${archives[@]}"; do
  read -r archive_name archive_suite <<<"$entry"
  sources+=("deb $SNAPSHOT_ROOT/$archive_name/$snapshot $archive_suite main")
done

# mmdebstrap left to choose falls back to fakechroot or chrootless when it
# cannot unshare, and those lay the root out differently. Naming the mode
# makes a builder without subordinate IDs fail here instead.
if [ "$(id -u)" -eq 0 ]; then
  mode=root
else
  mode=unshare
fi

include="$(IFS=,; echo "${packages[*]}")"
mmdebstrap \
  --quiet \
  --mode="$mode" \
  --variant=minbase \
  --include="$include" \
  --aptopt='Acquire::Check-Valid-Until "false"' \
  --keyring="$KEYRING" \
  "${hooks[@]}" \
  "$suite" "$work/root.tar" "${sources[@]}"

mkfs.erofs "${erofs[@]}" -T"$SOURCE_DATE_EPOCH" -U"$uuid" --all-root --quiet \
  --tar=f "$work/toolbox.erofs" "$work/root.tar"

digest="$(sha256sum "$work/toolbox.erofs" | cut -d' ' -f1)"
length="$(stat -c %s "$work/toolbox.erofs")"
features="$(dump.erofs -s "$work/toolbox.erofs" | sed -n 's/^Filesystem features:[[:space:]]*//p')"

python3 - "$work/release.json" "$arch" "$length" "$digest" "$features" "$RUNNER_VERSIONS" \
  "$work/packages.txt" "$work/vendored.tsv" <<'PY'
import json, sys

out, arch, length, digest, features, versions, packages_path, vendored_path = sys.argv[1:]
packages, record = [], {}
for line in open(packages_path, encoding="utf-8").read().splitlines() + [""]:
    if not line.strip():
        if record:
            packages.append(record)
        record = {}
        continue
    key, _, value = line.partition(":")
    if key in ("Package", "Version", "Architecture", "SHA256"):
        record[key.lower()] = value.strip()
vendored = []
for line in open(vendored_path, encoding="utf-8").read().splitlines():
    url, sha256 = line.split("\t")
    vendored.append({"url": url, "sha256": sha256})
manifest = {
    "arch": arch,
    "length": int(length),
    "sha256": digest,
    "erofs_features": features.split(),
    "runner_versions": versions.split(","),
    "packages": sorted(packages, key=lambda p: (p["package"], p.get("architecture", ""))),
    "vendored": vendored,
}
with open(out, "w", encoding="utf-8") as handle:
    json.dump(manifest, handle, indent=2, sort_keys=True)
    handle.write("\n")
PY

mkdir -p "$OUT"
image="$OUT/toolbox-$digest.erofs"
mv "$work/toolbox.erofs" "$image"
mv "$work/release.json" "${image%.erofs}.json"
echo "$image"
