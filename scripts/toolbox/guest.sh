#!/usr/bin/env bash
# Build the pinned guest set a Firecracker microVM boots from: the monitor and
# its jailer, the guest kernel, and the initramfs whose /init is the in-VM
# agent, agentsfleet-guest.
#
#   guest.sh packages
#       The Debian packages the build needs, one per line.
#   guest.sh pinned <dir>
#       What guest.txt alone decides, into <dir>: `firecracker` and `jailer`
#       from the release archive, `vmlinux` built from kernel.org source, and
#       `pinned.json` recording each one's SHA-256 and inputs. Minutes, so
#       callers cache <dir>.
#   guest.sh assemble <pinned-dir> <guest-binary> <output-dir>
#       The whole set into <output-dir>: the three pinned files, checked
#       against their record, `guest.cpio` around <guest-binary>, and
#       `guest.json` recording all four, the binary as `init`, and its
#       version beside the runner's.
#
# Linux only, as any user, for the builder's own architecture.
set -euo pipefail

# What the kernel build, the archive checks and the initramfs run.
readonly BUILDER_PACKAGES=(gcc binutils make flex bison bc libelf-dev libssl-dev perl xz-utils cpio curl jq)

# Every line the final kernel configuration must carry, each one something the
# engine or the guest agent relies on (afr_engine_firecracker, afg_entry).
readonly REQUIRED_CONFIG=(
  # vda, the toolbox: EROFS with the compression and attributes its image uses.
  'CONFIG_EROFS_FS=y'
  'CONFIG_EROFS_FS_ZIP=y'
  'CONFIG_EROFS_FS_XATTR=y'
  'CONFIG_EROFS_FS_POSIX_ACL=y'
  'CONFIG_EROFS_FS_SECURITY=y'
  # The drives, the tap and the workspace's file system.
  'CONFIG_VIRTIO_MMIO=y'
  'CONFIG_VIRTIO_BLK=y'
  'CONFIG_VIRTIO_NET=y'
  'CONFIG_EXT4_FS=y'
  # The guest agent's only way in, which no guest process may dial.
  'CONFIG_VIRTIO_VSOCKETS=y'
  '# CONFIG_VSOCKETS_LOOPBACK is not set'
  # As the measured spike kernel had it.
  'CONFIG_VIRTIO_BALLOON=y'
  # What /init is started from and mounts, and the `ip=` it is booted with.
  'CONFIG_BLK_DEV_INITRD=y'
  'CONFIG_DEVTMPFS=y'
  'CONFIG_TMPFS=y'
  'CONFIG_PROC_FS=y'
  'CONFIG_SYSFS=y'
  'CONFIG_UNIX98_PTYS=y'
  'CONFIG_IP_PNP=y'
  # The tenant leaf: cgroup v2 with memory counted and processes capped.
  'CONFIG_CGROUPS=y'
  'CONFIG_MEMCG=y'
  'CONFIG_CGROUP_PIDS=y'
  # The inner environment and what each tenant process puts on itself.
  'CONFIG_PID_NS=y'
  'CONFIG_SECCOMP_FILTER=y'
  'CONFIG_SECURITY_LANDLOCK=y'
  # No capabilities in a new user namespace, and no code loaded into the kernel.
  '# CONFIG_USER_NS is not set'
  '# CONFIG_MODULES is not set'
)
readonly PINNED_FILES=(vmlinux firecracker jailer)
readonly PINNED_RECORD=pinned.json
readonly INITRAMFS=guest.cpio
readonly GUEST_RECORD=guest.json

die() { echo "guest.sh: $*" >&2; exit 1; }

usage() {
  echo "usage: $0 packages | pinned <dir> | assemble <pinned-dir> <guest-binary> <output-dir>" >&2
  exit 2
}

file_sha256() { sha256sum "$1" | cut -d' ' -f1; }

is_sha256() { [[ "$1" =~ ^[0-9a-f]{64}$ ]]; }

read_manifest() {
  local key values line_arch version url sha256 members
  while read -r key values; do
    case "$key" in
      ''|'#'*) ;;
      epoch) epoch="$values" ;;
      kernel) read -r kernel_version kernel_url kernel_sha256 <<<"$values" ;;
      config)
        read -r line_arch url sha256 <<<"$values"
        if [ "$line_arch" = "$arch" ]; then config_url="$url" config_sha256="$sha256"; fi
        ;;
      monitor)
        read -r line_arch version url sha256 members <<<"$values"
        if [ "$line_arch" = "$arch" ]; then
          monitor_version="$version" monitor_url="$url" monitor_sha256="$sha256"
          read -r -a monitor_members <<<"$members"
        fi
        ;;
      *) die "unknown manifest key: $key" ;;
    esac
  done <"$manifest"
  [[ "$epoch" =~ ^[0-9]+$ ]] || die "the manifest names no epoch"
  [[ "$kernel_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "the manifest names no kernel version"
  [ -n "$config_url" ] || die "the manifest names no kernel configuration for $arch"
  [ -n "$monitor_url" ] || die "the manifest names no monitor for $arch"
  [ "${#monitor_members[@]}" -eq 2 ] || die "the manifest's monitor for $arch must name firecracker and jailer"
  for sha256 in "$kernel_sha256" "$config_sha256" "$monitor_sha256"; do
    is_sha256 "$sha256" || die "the manifest carries a malformed SHA-256: $sha256"
  done
}

# How the kernel names this architecture, the image Firecracker boots on it
# (the uncompressed `vmlinux` on x86_64, the `Image` file on arm64), and how
# readelf names its machine. Either image is installed as `vmlinux`.
kernel_target() {
  case "$arch" in
    amd64) kernel_arch=x86_64 kernel_goal=vmlinux kernel_image=vmlinux machine='Advanced Micro Devices X86-64' ;;
    arm64) kernel_arch=arm64 kernel_goal=Image kernel_image=arch/arm64/boot/Image machine=AArch64 ;;
    *) die "no guest kernel is pinned for $arch" ;;
  esac
}

# Downloads `url` to `dest` and refuses it unless its SHA-256 is the pinned one.
fetch() {
  local url="$1" sha256="$2" dest="$3" got
  curl --fail --silent --show-error --location --retry 4 --retry-all-errors --output "$dest" "$url"
  got="$(file_sha256 "$dest")"
  [ "$got" = "$sha256" ] || die "$url has SHA-256 $got, not the pinned $sha256"
}

fetch_monitor() {
  local out="$1" archive="$work/monitor.tgz" pair member name
  fetch "$monitor_url" "$monitor_sha256" "$archive"
  for pair in "${monitor_members[@]}"; do
    member="${pair%%=*}" name="${pair#*=}"
    case "$name" in
      firecracker|jailer) ;;
      *) die "the manifest's monitor installs an unknown file: $name" ;;
    esac
    tar -xzf "$archive" -O "$member" >"$work/$name"
    install -m 0755 "$work/$name" "$out/$name"
  done
  for name in firecracker jailer; do
    [ -s "$out/$name" ] || die "the monitor archive carries no $name"
  done
}

# Refuses a final configuration that lacks any required line, naming each.
assert_config() {
  local config="$1" line failed=0
  for line in "${REQUIRED_CONFIG[@]}"; do
    if ! grep -Fxq -- "$line" "$config"; then
      echo "guest.sh: the kernel configuration lacks: $line" >&2
      failed=1
    fi
  done
  if ! grep -Eq '^CONFIG_LSM="([^"]*,)?landlock(,[^"]*)?"$' "$config"; then
    echo "guest.sh: the kernel configuration lacks: landlock in CONFIG_LSM" >&2
    failed=1
  fi
  [ "$failed" -eq 0 ] || exit 1
}

# The banner's timestamp, user, host and build number are fixed, and the
# release carries no local suffix, so the image depends on the source, the
# configuration and the compiler alone.
kernel_make() {
  local stamp
  stamp="$(LC_ALL=C date -u -d "@$epoch" '+%a %b %e %H:%M:%S UTC %Y')"
  make -C "$work/linux-$kernel_version" ARCH="$kernel_arch" LOCALVERSION= \
    KBUILD_BUILD_TIMESTAMP="$stamp" KBUILD_BUILD_USER=agentsfleet \
    KBUILD_BUILD_HOST=agentsfleet KBUILD_BUILD_VERSION=1 "$@" >&2
}

build_kernel() {
  local out="$1" source="$work/linux-$kernel_version" release
  fetch "$kernel_url" "$kernel_sha256" "$work/linux.tar.xz"
  fetch "$config_url" "$config_sha256" "$work/base.config"
  tar -xJf "$work/linux.tar.xz" -C "$work"
  rm "$work/linux.tar.xz"
  ARCH="$kernel_arch" "$source/scripts/kconfig/merge_config.sh" -m -O "$source" "$work/base.config" "$fragment" >&2
  kernel_make olddefconfig
  assert_config "$source/.config"
  kernel_make -j"$(nproc)" "$kernel_goal"
  release="$(make -s -C "$source" ARCH="$kernel_arch" LOCALVERSION= kernelrelease)"
  [ "$release" = "$kernel_version" ] || die "the kernel calls itself $release, not $kernel_version"
  install -m 0644 "$source/$kernel_image" "$out/vmlinux"
  cp "$source/.config" "$work/final.config"
  rm -rf "$source"
}

artifact_json() {
  jq -cn --arg name "$(basename "$1")" --arg sha256 "$(file_sha256 "$1")" --argjson length "$(stat -c %s "$1")" \
    '{key:$name,value:{sha256:$sha256,length:$length}}'
}

write_pinned() {
  local out="$1" name
  for name in "${PINNED_FILES[@]}"; do artifact_json "$out/$name"; done | jq -s 'from_entries' >"$work/artifacts.json"
  dpkg-query -W -f='${Package}\t${Version}\n' "${BUILDER_PACKAGES[@]}" \
    | jq -Rn '[inputs|split("\t")|{key:.[0],value:.[1]}]|from_entries' >"$work/builder.json"
  jq -n --arg arch "$arch" --argjson epoch "$epoch" \
    --arg kernel_version "$kernel_version" --arg kernel_url "$kernel_url" --arg kernel_sha256 "$kernel_sha256" \
    --arg config_url "$config_url" --arg config_sha256 "$config_sha256" \
    --arg fragment_sha256 "$(file_sha256 "$fragment")" --arg final_sha256 "$(file_sha256 "$work/final.config")" \
    --arg compiler "$(gcc --version | sed -n 1p)" \
    --arg monitor_version "$monitor_version" --arg monitor_url "$monitor_url" --arg monitor_sha256 "$monitor_sha256" \
    --slurpfile artifacts "$work/artifacts.json" --slurpfile builder "$work/builder.json" \
    '{arch:$arch,epoch:$epoch,artifacts:$artifacts[0],
      kernel:{version:$kernel_version,source:{url:$kernel_url,sha256:$kernel_sha256},
        config:{url:$config_url,sha256:$config_sha256},fragment_sha256:$fragment_sha256,
        config_sha256:$final_sha256,compiler:$compiler},
      monitor:{version:$monitor_version,url:$monitor_url,sha256:$monitor_sha256},
      builder:$builder[0]}' >"$out/$PINNED_RECORD"
}

build_pinned() {
  local out="$1"
  mkdir -p "$out"
  fetch_monitor "$out"
  build_kernel "$out"
  write_pinned "$out"
  echo "$out/$PINNED_RECORD"
}

# Refuses a pinned directory built from another manifest, fragment or
# architecture, or whose files are not the ones its record names: a restored
# cache is checked as a fresh build is.
check_pinned() {
  local dir="$1" record="$1/$PINNED_RECORD" name
  [ -f "$record" ] || die "$dir holds no $PINNED_RECORD; run: $0 pinned $dir"
  jq -e --arg arch "$arch" --argjson epoch "$epoch" --arg kernel "$kernel_sha256" --arg config "$config_sha256" \
    --arg fragment "$(file_sha256 "$fragment")" --arg monitor "$monitor_sha256" \
    '.arch==$arch and .epoch==$epoch and .kernel.source.sha256==$kernel and .kernel.config.sha256==$config
     and .kernel.fragment_sha256==$fragment and .monitor.sha256==$monitor' "$record" >/dev/null \
    || die "$dir was built from another manifest, fragment or architecture"
  for name in "${PINNED_FILES[@]}"; do
    [ "$(file_sha256 "$dir/$name")" = "$(jq -er --arg name "$name" '.artifacts[$name].sha256' "$record")" ] \
      || die "$dir/$name is not the file $PINNED_RECORD records"
  done
}

# The guest's /init must run with nothing beside it: the initramfs holds one
# file, and the toolbox's own loader and libraries are bound over /usr only
# after /init has started. So a binary that needs a loader is refused.
check_init() {
  local binary="$1" header segments dynamic
  header="$(readelf -h "$binary")" || die "$binary is not an ELF binary"
  segments="$(readelf -l "$binary")"
  dynamic="$(readelf -d "$binary")"
  [[ "$header" == *"Machine:"*"$machine"* ]] || die "$binary is not built for $arch"
  [[ "$segments" != *INTERP* ]] || die "$binary needs a loader; the guest's /init must be the static musl build"
  [[ "$dynamic" != *'(NEEDED)'* ]] || die "$binary needs shared libraries; the guest's /init must be the static musl build"
}

# One file, `init`, owned by root, its time the manifest's epoch, its inode
# and device numbers zeroed: the archive depends on the binary alone.
build_initramfs() {
  local guest="$1" out="$2" stage="$work/initramfs" listed
  mkdir "$stage"
  install -m 0755 "$guest" "$stage/init"
  touch -d "@$epoch" "$stage/init"
  (cd "$stage" && printf 'init\0' | cpio --null --create --format=newc --reproducible --owner=0:0 --quiet) >"$work/$INITRAMFS"
  listed="$(cpio --list --quiet <"$work/$INITRAMFS")"
  [ "$listed" = init ] || die "the initramfs lists '$listed', not init alone"
  install -m 0644 "$work/$INITRAMFS" "$out/$INITRAMFS"
}

# The version the guest binary reports of itself: clap answers --version before
# the binary checks that it is a microVM's first process, so it runs here.
guest_version_of() {
  local binary="$1" said
  said="$("$binary" --version)" || die "$binary does not answer --version"
  [[ "$said" =~ ^agentsfleet-guest\ ([^[:space:]]+)$ ]] || die "$binary is not agentsfleet-guest: it says '$said'"
  printf '%s\n' "${BASH_REMATCH[1]}"
}

assemble() {
  local pinned="$1" guest="$2" out="$3" name guest_version
  [ -f "$guest" ] || die "guest binary not found: $guest"
  check_pinned "$pinned"
  check_init "$guest"
  guest_version="$(guest_version_of "$guest")"
  [ "$guest_version" = "$runner_version" ] \
    || die "$guest is agentsfleet-guest $guest_version, not this release's $runner_version"
  mkdir -p "$out"
  install -m 0644 "$pinned/vmlinux" "$out/vmlinux"
  for name in firecracker jailer; do install -m 0755 "$pinned/$name" "$out/$name"; done
  build_initramfs "$guest" "$out"
  jq --arg version "$runner_version" --arg guest_version "$guest_version" \
    --argjson initramfs "$(artifact_json "$out/$INITRAMFS")" \
    --arg init_sha256 "$(file_sha256 "$guest")" --argjson init_length "$(stat -c %s "$guest")" \
    '.runner_version=$version | .guest_version=$guest_version
     | .artifacts[$initramfs.key]=$initramfs.value
     | .init={sha256:$init_sha256,length:$init_length}' "$pinned/$PINNED_RECORD" >"$work/$GUEST_RECORD"
  install -m 0644 "$work/$GUEST_RECORD" "$out/$GUEST_RECORD"
  echo "$out/$GUEST_RECORD"
}

main() (
  local here manifest fragment runner_version arch work
  local epoch="" kernel_version="" kernel_url="" kernel_sha256="" config_url="" config_sha256=""
  local monitor_version="" monitor_url="" monitor_sha256="" monitor_members=()
  local kernel_arch kernel_goal kernel_image machine
  if [ "${1:-}" = packages ] && [ "$#" -eq 1 ]; then
    printf '%s\n' "${BUILDER_PACKAGES[@]}"
    return 0
  fi
  here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  manifest="${GUEST_MANIFEST:-$here/guest.txt}"
  fragment="$here/guest.config"
  runner_version="$(tr -d '[:space:]' <"$here/../../VERSION")"
  arch="$(dpkg --print-architecture)"
  read_manifest
  kernel_target
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' EXIT
  case "${1:-}" in
    pinned) [ "$#" -eq 2 ] || usage; build_pinned "$2" ;;
    assemble) [ "$#" -eq 4 ] || usage; assemble "$2" "$3" "$4" ;;
    *) usage ;;
  esac
)
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then main "$@"; fi
