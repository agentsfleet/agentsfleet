#!/usr/bin/env bash
# Assemble the pinned read-only toolbox; package operations use a disposable root.
set -euo pipefail

file_sha256() { sha256sum "$1" | cut -d' ' -f1; }

read_manifest() {
  local key values package
  while read -r key values; do
    case "$key" in
      ''|'#'*) ;;
      suite|snapshot|uuid) printf -v "$key" '%s' "$values" ;;
      erofs) read -r -a erofs <<<"$values" ;;
      packages) read -r -a packages <<<"$values" ;;
      purge) read -r -a purge <<<"$values" ;;
      archive) archives+=("$values") ;;
      vendor) vendors+=("$values") ;;
      *) echo "build.sh: unknown manifest key: $key" >&2; exit 2 ;;
    esac
  done <"$manifest"
  [ -n "$suite" ] && [ -n "$snapshot" ] && [ -n "$uuid" ]
  [ "${#archives[@]}" -gt 0 ] && [ "${#erofs[@]}" -gt 0 ]
  [ "${#packages[@]}" -gt 0 ] && [ "${#purge[@]}" -gt 0 ]
  for package in "${packages[@]}" "${purge[@]}"; do
    [[ "$package" =~ ^[a-z0-9][a-z0-9+.-]+$ ]] || { echo "invalid package: $package" >&2; exit 2; }
  done
  [ "${#purge[@]}" -eq "$(printf '%s\n' "${purge[@]}" | sort -u | wc -l)" ]
  [ "${#packages[@]}" -eq "$(printf '%s\n' "${packages[@]}" | sort -u | wc -l)" ]
  for package in apt dpkg passwd; do [[ " ${purge[*]} " == *" $package "* ]]; done
  for package in "${packages[@]}"; do [[ " ${purge[*]} " != *" $package "* ]]; done
}

fetch_vendors() {
  local line v_arch url sha256 members archive member name pair
  local pairs=()
  mkdir "$work/vendor-bin"
  : >"$work/vendored.tsv"
  for line in "${vendors[@]}"; do
    read -r v_arch url sha256 members <<<"$line"
    [ "$v_arch" = "$arch" ] || continue
    archive="$work/vendor-$(basename "$url")"
    curl --fail --silent --show-error --location --output "$archive" "$url"
    [ "$(file_sha256 "$archive")" = "$sha256" ]
    printf '%s\t%s\n' "$url" "$sha256" >>"$work/vendored.tsv"
    read -r -a pairs <<<"$members"
    for pair in "${pairs[@]}"; do
      member="${pair%%=*}" name="${pair#*=}"
      tar -xzf "$archive" -O "$member" >"$work/vendor-bin/$name"
      chmod 0755 "$work/vendor-bin/$name"
      hooks+=("--customize-hook=copy-in $work/vendor-bin/$name /usr/local/bin")
    done
  done
}

installed_packages() {
  dpkg-query --admindir="$1/var/lib/dpkg" -W -f '${Package}\t${Version}\t${Architecture}\t${Status}\n' |
    jq -Rn '[inputs|split("\t")|if .[3]=="install ok installed" then
      {package:.[0],version:.[1],architecture:.[2]} else error("package not installed") end]|sort_by(.package,.architecture)'
}

# Cached archives were authenticated by apt; hash the actual bytes it installed.
package_record() {
  local root="$1" deb="$2" package version architecture sha notice
  IFS=$'\t' read -r package version architecture < <(dpkg-deb -W --showformat='${Package}\t${Version}\t${Architecture}\n' "$deb")
  sha="$(file_sha256 "$deb")"
  notice="$(file_sha256 "$root/tmp/toolbox-audit/notices/$package")"
  jq -cn --arg package "$package" --arg version "$version" --arg architecture "$architecture" \
    --arg sha "$sha" --arg notice "$notice" \
    '{package:$package,version:$version,architecture:$architecture,sha256:$sha,copyright:{path:("/usr/share/doc/"+$package+"/copyright"),sha256:$notice}}'
}

capture_provenance() {
  local root="$1" deb package
  local audit="$1/tmp/toolbox-audit"
  mkdir -p "$audit/notices"
  installed_packages "$root" >"$audit/installed.json"
  jq -r '.[].package' "$audit/installed.json" >"$audit/installed.names"
  while IFS= read -r package; do
    cp -Lp "$root/usr/share/doc/$package/copyright" "$audit/notices/$package"
  done <"$audit/installed.names"
  for deb in "$root/var/cache/apt/archives/"*.deb; do
    package_record "$root" "$deb"
  done | jq -s 'sort_by(.package,.architecture)' >"$audit/all-packages.json"
  jq -e --slurpfile installed "$audit/installed.json" \
    'map({package,version,architecture})==$installed[0]' "$audit/all-packages.json" >/dev/null
}

# GNU tar compares file content, permissions and link targets independently of dpkg.
capture_payload() {
  local root="$1" audit="$1/tmp/toolbox-audit" path
  grep -Fvx -f "$root/tmp/toolbox-purge" "$audit/installed.names" >"$audit/retained"
  xargs -a "$audit/retained" dpkg-query --admindir="$root/var/lib/dpkg" -L >"$audit/owned"
  sort -u "$audit/owned" -o "$audit/owned"
  while IFS= read -r path; do
    case "$path" in /usr/share/doc/*) continue ;; esac
    if [ -f "$root$path" ] || [ -L "$root$path" ]; then printf '%s\0' "${path#/}"; fi
  done <"$audit/owned" >"$audit/payload.paths"
  printf '%s\0' etc/passwd etc/group etc/shadow etc/gshadow usr/local/bin/uv usr/local/bin/uvx >>"$audit/payload.paths"
  find "$root/etc/alternatives" -type l -printf 'etc/alternatives/%f\0' >>"$audit/payload.paths"
  tar -C "$root" --no-recursion --null -T "$audit/payload.paths" -cf "$audit/payload.tar"
}

purge_packages() {
  local root="$1" package notice parent
  grep -vx dpkg "$root/tmp/toolbox-purge" | xargs dpkg --root="$root" --purge --force-depends --force-remove-essential >&2
  dpkg --root="$root" --purge --force-depends --force-remove-essential dpkg >&2
  while IFS= read -r package; do
    notice="$root/usr/share/doc/$package/copyright"
    if [ ! -f "$notice" ]; then
      parent="$(dirname "$notice")"
      if [ -L "$parent" ]; then rm "$parent"; fi
      mkdir -p "$parent"
      rm -f "$notice"
      cp -p "$root/tmp/toolbox-audit/notices/$package" "$notice"
    fi
    cmp "$root/tmp/toolbox-audit/notices/$package" "$notice"
  done <"$root/tmp/toolbox-audit/retained"
}

capture_inventory() {
  local root="$1" audit="$1/tmp/toolbox-audit"
  installed_packages "$root" >"$audit/final.json"
  jq --slurpfile final "$audit/final.json" --rawfile purge "$root/tmp/toolbox-purge" --rawfile required "$root/tmp/toolbox-required" \
    '($purge|split("\n")|map(select(length>0))) as $remove |
     ($required|split("\n")|map(select(length>0))) as $keep |
     if ($remove-map(.package)|length)>0 then error("missing removal package") else . end |
     map(select(.package as $p|$remove|index($p)|not)) |
     if map({package,version,architecture})!=$final[0] or ($keep-map(.package)|length)>0 or
        any(.package|test("^(perl|libperl)|-perl$")) then error("final package set differs") else . end' \
    "$audit/all-packages.json" >"$root/var/lib/dpkg/packages.json"
  tar -C "$root/var/lib/dpkg" --exclude=info/format --sort=name --mtime="@$SOURCE_DATE_EPOCH" \
    --owner=0 --group=0 --numeric-owner -cf "$audit/metadata.tar" status info packages.json
  [ "$(stat -c %s "$audit/metadata.tar")" -le 4194304 ]
  gzip -n -9 "$audit/metadata.tar"
  file_sha256 "$audit/metadata.tar.gz" >"$root/etc/toolbox-inventory.sha256"
  chmod 0644 "$root/etc/toolbox-inventory.sha256"
}

check_runtime() {
  local root="$1" tool
  for tool in git python3 node rg jq curl uv uvx; do chroot "$root" "$tool" --version >&2; done
  chroot "$root" sh -eu -c 'for tool in perl apt apt-get dpkg dpkg-query passwd useradd chpasswd; do if command -v "$tool"; then exit 1; fi; done'
}

finalize_root() {
  local root="$1"
  [ "$(stat -c %d:%i "$root")" != "$(stat -c %d:%i /)" ]
  capture_provenance "$root"
  capture_payload "$root"
  purge_packages "$root"
  capture_inventory "$root"
  tar -C "$root" --compare -f "$root/tmp/toolbox-audit/payload.tar"
  rm -rf "$root/var/lib/dpkg" "$root/var/lib/apt" "$root/var/cache/apt" "$root/var/lib/perl"
  tar -C "$root" --compare -f "$root/tmp/toolbox-audit/payload.tar"
  check_runtime "$root"
}

configure_root() {
  hooks+=("--customize-hook=mkdir -p \"\$1/workspace\" \"\$1/opt/agentsfleet\"")
  hooks+=("--customize-hook=touch \"\$1/opt/agentsfleet/agentsfleet-runner\"")
  hooks+=("--customize-hook=: >\"\$1/etc/resolv.conf\" && chmod 0644 \"\$1/etc/resolv.conf\"")
  hooks+=("--customize-hook=echo localhost >\"\$1/etc/hostname\" && chmod 0644 \"\$1/etc/hostname\"")
  hooks+=('--customize-hook=chroot "$1" find /usr/share/doc -type l ! -name copyright ! -exec test -f {}/copyright \; -delete')
  hooks+=('--customize-hook=chroot "$1" find /usr/share/man /usr/share/locale /usr/share/info -type l -delete')
  hooks+=('--customize-hook=rm -f "$1/usr/share/info/dir"')
  hooks+=("--customize-hook=printf '%s\n' ${purge[*]} >\"\$1/tmp/toolbox-purge\"")
  hooks+=("--customize-hook=printf '%s\n' ${packages[*]} >\"\$1/tmp/toolbox-required\"")
  hooks+=("--customize-hook=copy-in $here/build.sh /tmp")
  hooks+=('--customize-hook=bash -euo pipefail -c '\''source "$1/tmp/build.sh"; finalize_root "$1"'\'' -- "$1"')
  hooks+=("--customize-hook=download /tmp/toolbox-audit/metadata.tar.gz $work/inventory.tar.gz")
  hooks+=('--customize-hook=rm -rf "$1/tmp/toolbox-audit" "$1/tmp/toolbox-purge" "$1/tmp/toolbox-required" "$1/tmp/build.sh"')
}

build_root() {
  local entry archive_name archive_suite include mode=root sources=()
  [ "$(id -u)" -eq 0 ] || mode=unshare
  include="$(IFS=,; echo "${packages[*]}")"
  for entry in "${archives[@]}"; do
    read -r archive_name archive_suite <<<"$entry"
    sources+=("deb http://snapshot.debian.org/archive/$archive_name/$snapshot $archive_suite main")
  done
  # Keep archives for provenance; finalization owns cleanup so apt cannot recreate its state.
  SOURCE_DATE_EPOCH="$epoch" mmdebstrap --quiet --mode="$mode" --variant=minbase --skip=essential/unlink --skip=cleanup/apt \
    --include="$include" --aptopt='Acquire::Check-Valid-Until "false"' \
    --dpkgopt='path-exclude=/usr/share/doc/*' --dpkgopt='path-include=/usr/share/doc/*/copyright' \
    --dpkgopt='path-exclude=/usr/share/man/*' --dpkgopt='path-exclude=/usr/share/locale/*' \
    --dpkgopt='path-exclude=/usr/share/info/*' --keyring="$keyring" \
    "${hooks[@]}" "$suite" "$work/root.tar" "${sources[@]}"
}

assemble_image() {
  SOURCE_DATE_EPOCH="$epoch" mkfs.erofs "${erofs[@]}" -T"$epoch" -U"$uuid" --all-root --quiet \
    --tar=f "$work/toolbox.erofs" "$work/root.tar"
  digest="$(file_sha256 "$work/toolbox.erofs")"
  length="$(stat -c %s "$work/toolbox.erofs")"
  features="$(dump.erofs -s "$work/toolbox.erofs" | sed -n 's/^Filesystem features:[[:space:]]*//p')"
}

write_release() {
  tar -xOzf "$work/inventory.tar.gz" packages.json >"$work/packages.json"
  base64 -w0 "$work/inventory.tar.gz" >"$work/metadata.base64"
  dpkg-query -W -f='${Package}\t${Version}\n' mmdebstrap apt dpkg erofs-utils liblz4-1 jq \
    | jq -Rn '[inputs|split("\t")|{key:.[0],value:.[1]}]|from_entries' >"$work/builder.json"
  jq -Rn '[inputs|split("\t")|{url:.[0],sha256:.[1]}]' <"$work/vendored.tsv" >"$work/vendored.json"
  printf '%s\n' "${archives[@]}" >"$work/archives.txt"
  jq -n --arg arch "$arch" --argjson length "$length" --arg sha256 "$digest" \
    --arg features "$features" --arg versions "$runner_versions" --arg snapshot "$snapshot" \
    --slurpfile packages "$work/packages.json" --rawfile metadata "$work/metadata.base64" \
    --slurpfile vendored "$work/vendored.json" --slurpfile builder "$work/builder.json" --rawfile archives "$work/archives.txt" \
    '{arch:$arch,length:$length,sha256:$sha256,erofs_features:($features|split(" ")|map(select(length>0))),
      runner_versions:($versions|split(",")),packages:$packages[0],package_metadata:$metadata,
      vendored:$vendored[0],snapshot:$snapshot,archives:($archives|split("\n")|map(select(length>0))),builder:$builder[0]}' >"$work/release.json"
  [ "$(stat -c %s "$work/release.json")" -le 1048576 ]
}

main() (
  [ "$#" -eq 1 ] || { echo "usage: $0 <output-dir>" >&2; exit 2; }
  local here manifest keyring runner_versions arch epoch work digest length features stamp
  local suite="" snapshot="" uuid="" erofs=() packages=() purge=() archives=() vendors=() hooks=()
  here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  manifest="${TOOLBOX_MANIFEST:-$here/manifest.txt}"
  keyring="${TOOLBOX_KEYRING:-/usr/share/keyrings/debian-archive-keyring.gpg}"
  runner_versions="${TOOLBOX_RUNNER_VERSIONS:-$(tr -d '[:space:]' <"$here/../../VERSION")}"
  command -v jq >/dev/null
  arch="$(dpkg --print-architecture)"
  read_manifest
  stamp="${snapshot:0:4}-${snapshot:4:2}-${snapshot:6:2}T${snapshot:9:2}:${snapshot:11:2}:${snapshot:13:2}Z"
  epoch="$(date -u -d "$stamp" +%s)"
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' EXIT
  fetch_vendors
  configure_root
  build_root
  assemble_image
  write_release
  mkdir -p "$1"
  mv "$work/toolbox.erofs" "$1/toolbox-$digest.erofs"
  mv "$work/release.json" "$1/toolbox-$digest.json"
  echo "$1/toolbox-$digest.erofs"
)
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then main "$@"; fi
