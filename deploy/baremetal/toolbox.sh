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

# The image commits to its external records; decode nothing beyond the bounds.
toolbox_decode_scan_inventory() {
  python3 -I - "$1" "$2" <<'PY'
import base64, hashlib, io, json, re, sys, tarfile, zlib
from pathlib import Path
def unique(pairs):
    value = {}
    for key, item in pairs:
        assert key not in value, f"duplicate inventory key: {key}"
        value[key] = item
    return value
image, lower = map(Path, sys.argv[1:])
with image.with_suffix(".json").open("rb") as handle:
    raw = handle.read(1_048_577)
assert len(raw) <= 1_048_576, "release exceeds admission byte limit"
release = json.loads(raw.decode("utf-8"), object_pairs_hook=unique)
sha = release["sha256"]
assert isinstance(sha, str) and re.fullmatch("[0-9a-f]{64}", sha), "invalid image digest"
assert type(release["length"]) is int and image.stat().st_size == release["length"] > 0
assert image.name == f"toolbox-{sha}.erofs", "image filename differs from release"
with image.open("rb") as handle:
    assert hashlib.file_digest(handle, "sha256").hexdigest() == sha, "image digest mismatch"
payload = {key: release[key] for key in ("packages", "package_metadata")}
compressed = base64.b64decode(payload["package_metadata"], validate=True)
marker = lower / "etc/toolbox-inventory.sha256"
assert marker.is_file() and not marker.is_symlink(), "inventory commitment must be a regular file"
with marker.open("rb") as handle:
    assert handle.read(66) == hashlib.sha256(compressed).hexdigest().encode() + b"\n", "inventory commitment mismatch"
assert isinstance(payload["package_metadata"], str), "metadata must be base64 text"
decoder = zlib.decompressobj(31)
decoded = decoder.decompress(compressed, 4_194_305)
assert len(decoded) <= 4_194_304 and decoder.eof and not decoder.unused_data and not decoder.unconsumed_tail, "invalid or oversized metadata"
with tarfile.open(fileobj=io.BytesIO(decoded)) as archive:
    members = archive.getmembers()
    assert len(members) <= 1027 and len({member.name for member in members}) == len(members)
    assert all(member.isfile() or (member.isdir() and member.name == "info") for member in members)
    files = {member.name: archive.extractfile(member).read().decode("utf-8") for member in members if member.isfile()}
assert set(files) >= {"status", "packages.json"} and all(name in ("status", "packages.json") or name.startswith("info/") for name in files)
assert json.loads(files.pop("packages.json"), object_pairs_hook=unique) == payload["packages"], "package commitment mismatch"
metadata = {"status": files.pop("status"), "info": {name[5:]: value for name, value in files.items()}}
assert isinstance(payload["packages"], list) and 0 < len(payload["packages"]) <= 512
assert isinstance(metadata["status"], str) and metadata["status"].strip() and "\0" not in metadata["status"]
metadata["status"].encode("utf-8")
assert isinstance(metadata["info"], dict) and len(metadata["info"]) <= 1024
for contents in metadata["info"].values():
    assert isinstance(contents, str) and "\0" not in contents, "invalid file records"
    contents.encode("utf-8")
json.dump({"packages": payload["packages"], **metadata}, sys.stdout)
PY
}

# Validate every field before creating even the first scanner record.
toolbox_write_scan_inventory() {
  python3 -I /dev/fd/3 "$1" 3<<'PY'
import json, re, sys
from email.parser import Parser
from pathlib import Path
value = json.load(sys.stdin)
packages, status, info = (value[key] for key in ("packages", "status", "info"))
expected, installed, lists, entries = {}, {}, set(), set()
for package in packages:
    assert isinstance(package, dict) and set(package) == {"package", "version", "architecture", "sha256", "copyright"}
    name, version, arch = (package[key] for key in ("package", "version", "architecture"))
    assert re.fullmatch("[a-z0-9][a-z0-9+.-]+", name) and re.fullmatch("[a-z0-9][a-z0-9-]*", arch)
    assert re.fullmatch("[0-9][A-Za-z0-9.+:~\\-]*", version) and re.fullmatch("[0-9a-f]{64}", package["sha256"])
    notice = package["copyright"]
    assert set(notice) == {"path", "sha256"} and notice["path"] == f"/usr/share/doc/{name}/copyright"
    assert re.fullmatch("[0-9a-f]{64}", notice["sha256"]) and (name, arch) not in expected
    expected[name, arch] = version
for block in status.strip().split("\n\n"):
    if not block.strip():
        continue
    message = Parser().parsestr(block.lstrip("\n"), headersonly=True)
    record = {key.lower(): item.strip() for key, item in message.raw_items()}
    assert not message.defects and not message.get_payload() and len(record) == len(list(message.raw_items()))
    key = record["package"], record["architecture"]
    source = record.get("source", key[0])
    assert re.fullmatch(r"[a-z0-9][a-z0-9+.-]+(?: \([0-9][A-Za-z0-9.+:~\-]*\))?", source), "invalid source identity"
    assert record["status"] == "install ok installed" and key not in installed
    installed[key] = record["version"]
assert installed == expected, "package inventory and status disagree"
for name, contents in info.items():
    assert re.fullmatch(r"[a-z0-9][a-z0-9+.-]+(?::[a-z0-9][a-z0-9-]*)?\.(list|md5sums|conffiles|postinst|postrm|prerm|preinst|templates|triggers|config|shlibs|symbols)", name), "unsafe info name"
    assert isinstance(contents, str) and "\0" not in contents, "invalid file records"
    stem, suffix = name.rsplit(".", 1)
    package, _, arch = stem.partition(":")
    owners = [key for key in expected if key[0] == package and (not arch or key[1] == arch)]
    assert len(owners) == 1 and (owners[0], suffix) not in entries, "missing or duplicate info owner"
    entries.add((owners[0], suffix))
    if suffix == "list":
        lists.add(owners[0])
assert lists == set(expected), "missing file list"
upper = Path(sys.argv[1])
assert upper.is_dir() and not upper.is_symlink() and not any(upper.iterdir()), "upper must be empty"
database = upper / "var/lib/dpkg"
(database / "info").mkdir(parents=True)
for path, contents in [(database / "status", status), *((database / "info" / name, text) for name, text in info.items())]:
    with path.open("x", encoding="utf-8") as handle:
        handle.write(contents)
    path.chmod(0o644)
PY
}

toolbox_restore_scan_inventory() {
  toolbox_decode_scan_inventory "$1" "$2" | toolbox_write_scan_inventory "$3"
}
