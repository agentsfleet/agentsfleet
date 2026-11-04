#!/usr/bin/env bash
# Build a pinned, reproducible EROFS toolbox and its unsigned release manifest.
# Package operations happen only in mmdebstrap's disposable root.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly HERE
readonly MANIFEST="${TOOLBOX_MANIFEST:-$HERE/manifest.txt}"
readonly SNAPSHOT_ROOT="http://snapshot.debian.org/archive"
readonly KEYRING="${TOOLBOX_KEYRING:-/usr/share/keyrings/debian-archive-keyring.gpg}"
readonly RUNNER_VERSIONS="${TOOLBOX_RUNNER_VERSIONS:-$(tr -d '[:space:]' <"$HERE/../../VERSION")}"
# Mount points the sandbox binds onto; the image is read-only, so they must
# already exist in it. `/run` is not one: mmdebstrap empties it, and the
# sandbox mounts its own.
readonly MOUNT_POINTS=(workspace opt/agentsfleet)
readonly ENTRY_POINT="opt/agentsfleet/agentsfleet-runner"
readonly VENDOR_BIN="usr/local/bin"
readonly PACKAGES_INSIDE="tmp/toolbox-packages.txt"
readonly INVENTORY_INSIDE="tmp/toolbox-inventory.json"
# mmdebstrap copies the builder's resolver configuration and hostname into the
# root, so two builders would ship two images. Both files get fixed content
# rather than being removed: the per-lease network allowlist binds rendered
# resolver files in (docs/architecture/runner_execution.md §Sandbox engines),
# and a bind onto this read-only image needs a target that already exists.
readonly RESOLVER_FILE="etc/resolv.conf"
readonly HOSTNAME_FILE="etc/hostname"
readonly IMAGE_HOSTNAME="localhost"
readonly BUILDER_PACKAGES=(mmdebstrap apt dpkg erofs-utils liblz4-1)

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <output-dir>" >&2
  exit 2
fi
readonly OUT="$1"

arch="$(dpkg --print-architecture)"
suite="" snapshot="" uuid="" erofs=() packages=() purge=() archives=() vendors=()
while read -r key values; do
  case "$key" in
    ''|'#'*) ;;
    suite) suite="$values" ;;
    snapshot) snapshot="$values" ;;
    uuid) uuid="$values" ;;
    erofs) read -r -a erofs <<<"$values" ;;
    packages) read -r -a packages <<<"$values" ;;
    purge) read -r -a purge <<<"$values" ;;
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

if [ "${#packages[@]}" -eq 0 ] || [ "${#purge[@]}" -eq 0 ]; then
  echo "build.sh: packages and purge inputs are required" >&2
  exit 2
fi
for package in "${packages[@]}" "${purge[@]}"; do
  [[ "$package" =~ ^[a-z0-9][a-z0-9+.-]+$ ]] || { echo "invalid package: $package" >&2; exit 2; }
done

stamp="${snapshot:0:4}-${snapshot:4:2}-${snapshot:6:2}T${snapshot:9:2}:${snapshot:11:2}:${snapshot:13:2}Z"
SOURCE_DATE_EPOCH="$(date -u -d "$stamp" +%s)"
export SOURCE_DATE_EPOCH

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# The temporary program runs with the root's pinned Python, then leaves the image.
cat >"$work/finalize.py" <<'PY'
import base64, gzip, hashlib, json, os, re, shutil, stat, subprocess, sys
from email.parser import Parser
from pathlib import Path

def rows(text):
    result = {}
    for block in text.strip().split("\n\n"):
        if not block.strip():
            continue
        message = Parser().parsestr(block.lstrip("\n"), headersonly=True)
        fields = {key.lower(): value.strip() for key, value in message.raw_items()}
        assert not message.defects and not message.get_payload(), "malformed package record"
        assert len(fields) == len(list(message.raw_items())), "duplicate package field"
        key = fields["package"], fields["architecture"]
        assert key not in result, f"duplicate package: {key}"
        result[key] = fields
    return result

def digest(path):
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()

def fingerprint(path):
    if not path.exists() and not path.is_symlink():
        return None
    mode = path.lstat().st_mode
    if path.is_symlink():
        return "link", stat.S_IMODE(mode), os.readlink(path), digest(path) if path.is_file() else None
    if path.is_file():
        return "file", stat.S_IMODE(mode), digest(path)
    return "directory", stat.S_IMODE(mode)

def reconcile(expected, permitted):
    for path, before in {**expected, **permitted}.items():
        assert fingerprint(path) == before, f"retained payload changed: {path}"

root_id, provenance, output, purge, required = sys.argv[1:]
root = Path("/").stat()
assert os.geteuid() == 0 and root_id == f"{root.st_dev}:{root.st_ino}", "wrong disposable root"
purge, required = purge.split(), required.split()
assert len(purge) == len(set(purge)) and len(required) == len(set(required)), "duplicate package input"
assert purge and required and not set(purge) & set(required), "invalid removal or required set"
assert {"apt", "dpkg", "passwd"} <= set(purge), "package-manager removal is required"
database = Path("/var/lib/dpkg")
before = rows((database / "status").read_text())
archives = rows(Path(provenance).read_text())
assert set(before) == set(archives), "archive inventory differs from installed set"
for key, record in before.items():
    assert record["status"] == "install ok installed", f"package is not installed: {key}"
    assert record["version"] == archives[key]["version"], f"archive version mismatch: {key}"
    assert re.fullmatch("[0-9a-f]{64}", archives[key]["sha256"]), f"archive hash missing: {key}"
names = {key[0] for key in before}
assert set(purge) <= names and set(required) <= names, "missing removal or required package"
retained = {key: record for key, record in before.items() if key[0] not in purge}
kept, removed = set(), set()
for listing in (database / "info").glob("*.list"):
    name = listing.name[:-5].split(":")[0]
    target = removed if name in purge else kept
    for line in listing.read_text().splitlines():
        path = Path(line)
        assert path.is_absolute() and ".." not in path.parts, f"unsafe installed path: {path}"
        target.add(path)
special = {Path("/etc") / name for name in ("passwd", "group", "shadow", "gshadow")}
special |= {Path("/usr/local/bin/uv"), Path("/usr/local/bin/uvx")}
special |= {path for path in Path("/etc/alternatives").iterdir() if path.is_symlink()}
assert all(path.exists() for path in special), "missing identity, vendor or alternative"
expected = {path: fingerprint(path) for path in kept | special if path.is_file() or path.is_symlink()}
unwanted = {path for path in removed - kept if path.is_file() or path.is_symlink()}
notices = {}
for name, _ in retained:
    path = Path("/usr/share/doc") / name / "copyright"
    assert path.is_file() and path.stat().st_size, f"missing copyright: {path}"
    notices[path] = path.read_bytes(), stat.S_IMODE(path.stat().st_mode), digest(path)
for group in (sorted(set(purge) - {"dpkg"}), ["dpkg"]):
    subprocess.run(["dpkg", "--purge", "--force-depends", "--force-remove-essential", *group],
                   check=True, stdout=sys.stderr)
permitted = {}
for path, (contents, mode, sha256) in notices.items():
    if not path.is_file():
        if path.parent.is_symlink():
            path.parent.unlink()
        if not path.parent.exists():
            path.parent.mkdir()
            path.parent.chmod(0o755)
            permitted[path.parent] = "directory", 0o755
        if path.is_symlink():
            path.unlink()
        path.write_bytes(contents)
        path.chmod(mode)
        permitted[path] = "file", mode, sha256
    assert digest(path) == sha256, f"copyright changed: {path}"
status = (database / "status").read_text()
after = rows(status)
identity = lambda value: {key: (row["version"], row.get("source", key[0])) for key, row in value.items()}
assert identity(after) == identity(retained), "final package or source versions differ"
assert all(row["status"] == "install ok installed" for row in after.values())
assert not any(name.startswith(("perl", "libperl")) or name.endswith("-perl") for name, _ in after)
info = {path.name: path.read_text() for path in (database / "info").iterdir()
        if path.is_file() and path.suffix}
reconcile(expected, permitted)
for name in ("/var/lib/dpkg", "/var/lib/apt", "/var/cache/apt", "/var/lib/perl"):
    path = Path(name)
    if path.exists():
        shutil.rmtree(path)
reconcile(expected, permitted)
assert not any(path.exists() or path.is_symlink() for path in unwanted), "removed payload remains"
for tool in ("git", "python3", "node", "rg", "jq", "curl", "uv", "uvx"):
    subprocess.run([tool, "--version"], check=True, stdout=sys.stderr)
for tool in ("perl", "apt", "apt-get", "dpkg", "dpkg-query", "passwd", "useradd", "chpasswd"):
    assert shutil.which(tool) is None, f"removed tool remains: {tool}"
subprocess.run(["python3", "-c", "import hashlib, json, pathlib, ssl"], check=True)
packages = []
for key, row in sorted(retained.items()):
    notice = Path("/usr/share/doc") / key[0] / "copyright"
    packages.append({field: archives[key][field] for field in ("package", "version", "architecture", "sha256")})
    packages[-1]["copyright"] = {"path": str(notice), "sha256": notices[notice][2]}
canonical = lambda value: json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode()
metadata = canonical({"status": status, "info": info})
assert len(metadata) <= 4_194_304 and len(packages) <= 512 and len(info) <= 1024, "inventory exceeds bounds"
inventory = {"packages": packages, "package_metadata": base64.b64encode(gzip.compress(metadata, mtime=0)).decode()}
Path("/etc/toolbox-inventory.sha256").write_text(hashlib.sha256(canonical(inventory)).hexdigest() + "\n")
Path("/etc/toolbox-inventory.sha256").chmod(0o644)
Path(output).write_bytes(canonical(inventory))
Path(output).chmod(0o644)
PY


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
# dpkg's path filters retain symlinks, including aliases for excluded files.
# Prune those aliases and install-info's regenerated index inside the root;
# copyright files and shared licence directories stay. Regular documentation
# was excluded during installation.
hooks+=("--customize-hook=chroot \"\$1\" python3 -c 'from pathlib import Path
doc = Path(\"/usr/share/doc\")
info = Path(\"/usr/share/info\")
for root in (doc, Path(\"/usr/share/man\"), Path(\"/usr/share/locale\"), info):
    for path in root.rglob(\"*\"):
        if path.is_symlink() and not (root == doc and (path.name == \"copyright\" or (path / \"copyright\").is_file())):
            path.unlink()
(info / \"dir\").unlink(missing_ok=True)
'")

# Root identity guards the only destructive operation; dpkg is removed last.
hooks+=("--customize-hook=copy-in $work/finalize.py /tmp")
purge_names="${purge[*]}" required_names="${packages[*]}"
hooks+=("--customize-hook=root_id=\$(stat -c %d:%i \"\$1\") && test \"\$root_id\" != \"\$(stat -c %d:%i /)\" && chroot \"\$1\" python3 -I /tmp/finalize.py \"\$root_id\" /$PACKAGES_INSIDE /$INVENTORY_INSIDE '$purge_names' '$required_names'")
hooks+=("--customize-hook=download /$INVENTORY_INSIDE $work/inventory.json")
hooks+=("--customize-hook=rm \"\$1/$PACKAGES_INSIDE\" \"\$1/$INVENTORY_INSIDE\" \"\$1/tmp/finalize.py\"")

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
# The finalizer owns apt cleanup; host apt would recreate removed state.
mmdebstrap \
  --quiet \
  --mode="$mode" \
  --variant=minbase \
  --skip=cleanup/apt \
  --include="$include" \
  --aptopt='Acquire::Check-Valid-Until "false"' \
  --dpkgopt='path-exclude=/usr/share/doc/*' \
  --dpkgopt='path-include=/usr/share/doc/*/copyright' \
  --dpkgopt='path-exclude=/usr/share/man/*' \
  --dpkgopt='path-exclude=/usr/share/locale/*' \
  --dpkgopt='path-exclude=/usr/share/info/*' \
  --keyring="$KEYRING" \
  "${hooks[@]}" \
  "$suite" "$work/root.tar" "${sources[@]}"

mkfs.erofs "${erofs[@]}" -T"$SOURCE_DATE_EPOCH" -U"$uuid" --all-root --quiet \
  --tar=f "$work/toolbox.erofs" "$work/root.tar"

digest="$(sha256sum "$work/toolbox.erofs" | cut -d' ' -f1)"
length="$(stat -c %s "$work/toolbox.erofs")"
features="$(dump.erofs -s "$work/toolbox.erofs" | sed -n 's/^Filesystem features:[[:space:]]*//p')"
dpkg-query -W -f="\${Package}\t\${Version}\n" "${BUILDER_PACKAGES[@]}" >"$work/builder.tsv"
printf '%s\n' "${archives[@]}" >"$work/archives.txt"

python3 - "$work/release.json" "$arch" "$length" "$digest" "$features" "$RUNNER_VERSIONS" \
  "$work/inventory.json" "$work/vendored.tsv" "$snapshot" "$work/archives.txt" \
  "$work/builder.tsv" <<'PY'
import json, sys

(out, arch, length, digest, features, versions, inventory_path, vendored_path, snapshot,
 archives_path, builder_path) = sys.argv[1:]
with open(inventory_path, encoding="utf-8") as handle:
    inventory = json.load(handle)
vendored = []
for line in open(vendored_path, encoding="utf-8").read().splitlines():
    url, sha256 = line.split("\t")
    vendored.append({"url": url, "sha256": sha256})
builder = {}
for line in open(builder_path, encoding="utf-8").read().splitlines():
    package, version = line.split("\t")
    builder[package] = version
manifest = {
    "arch": arch,
    "length": int(length),
    "sha256": digest,
    "erofs_features": features.split(),
    "runner_versions": versions.split(","),
    "packages": inventory["packages"],
    "package_metadata": inventory["package_metadata"],
    "vendored": vendored,
    "snapshot": snapshot,
    "archives": open(archives_path, encoding="utf-8").read().splitlines(),
    "builder": builder,
}
encoded = (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode("utf-8")
if len(encoded) > 1_048_576:
    raise SystemExit("build.sh: release exceeds admission byte limit")
with open(out, "wb") as handle:
    handle.write(encoded)
PY

mkdir -p "$OUT"
image="$OUT/toolbox-$digest.erofs"
mv "$work/toolbox.erofs" "$image"
mv "$work/release.json" "${image%.erofs}.json"
echo "$image"
