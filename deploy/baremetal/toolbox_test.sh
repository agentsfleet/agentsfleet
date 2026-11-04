#!/usr/bin/env bash
# Self-tests for toolbox.sh: what one toolbox set is, and staging it.
#
#     bash deploy/baremetal/toolbox_test.sh
#
# toolbox.sh takes every path as an argument and only returns on a refusal, so
# this suite sources it once and calls it directly. Staging writes only under
# this suite's scratch directory.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./toolbox.sh
source "$SCRIPT_DIR/toolbox.sh"

passed=0
failed=0

ok()  { printf 'ok   %s\n' "$1"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n       %s\n' "$1" "$2" >&2; failed=$((failed + 1)); }

WORK_DIR="$(mktemp -d)"
readonly WORK_DIR
cleanup() { rm -rf "$WORK_DIR"; }
trap cleanup EXIT

# Digest-shaped names for the fixtures; staging reads them, nothing hashes them.
readonly FIXTURE_DIGEST="0000000000000000000000000000000000000000000000000000000000000001"
readonly OLDER_DIGEST="0000000000000000000000000000000000000000000000000000000000000002"
# The set the runner admits, spelled here rather than read from toolbox.sh, so
# a suffix dropped there fails these cases instead of shrinking their fixtures.
readonly TOOLBOX_PARTS=(erofs json json.sig)

# A directory holding the toolbox set named `digest`, without `skip` if given.
toolbox_set() {
  local dir="$1" digest="$2" skip="${3:-}" part
  mkdir -p "$dir"
  for part in "${TOOLBOX_PARTS[@]}"; do
    [[ "$part" == "$skip" ]] || printf '%s\n' "$part" >"$dir/toolbox-$digest.$part"
  done
}

# Stages `src` into `incoming`, quietly.
stage_toolbox() {
  install_toolbox "$1" "$2" >/dev/null 2>&1
}

# The set's paths, image first, so the playbook copies and the action bundles
# exactly what the host checks.
test_toolbox_set_files_names_the_three_files_image_first() {
  local name="test_toolbox_set_files_names_the_three_files_image_first"
  local dir="$WORK_DIR/named" got want part
  toolbox_set "$dir" "$FIXTURE_DIGEST"
  got="$(toolbox_set_files "$dir" | tr '\n' ' ')"
  want=""
  for part in "${TOOLBOX_PARTS[@]}"; do
    want+="$dir/toolbox-$FIXTURE_DIGEST.$part "
  done
  if [[ "$got" != "$want" ]]; then
    bad "$name" "toolbox_set_files printed [$got], want [$want]"
  elif toolbox_set_files "$WORK_DIR/no-such-dir" >/dev/null 2>&1; then
    bad "$name" "a directory with no image passed"
  else
    ok "$name"
  fi
}

# The staged set replaces the previous one whole, so the runner admits exactly
# the release this deploy brought.
test_toolbox_staging_replaces_the_previous_set() {
  local name="test_toolbox_staging_replaces_the_previous_set"
  local src="$WORK_DIR/stage-src" incoming="$WORK_DIR/stage-incoming" staged
  toolbox_set "$src" "$FIXTURE_DIGEST"
  toolbox_set "$incoming" "$OLDER_DIGEST"

  if ! stage_toolbox "$src" "$incoming"; then
    bad "$name" "a complete set was refused"
    return
  fi
  staged="$(cd "$incoming" && find . -type f | sort | tr '\n' ' ')"
  local want="./toolbox-$FIXTURE_DIGEST.erofs ./toolbox-$FIXTURE_DIGEST.json ./toolbox-$FIXTURE_DIGEST.json.sig "
  if [[ "$staged" != "$want" ]]; then
    bad "$name" "incoming holds [$staged], want [$want]"
  else
    ok "$name"
  fi
}

# A set short of any one file, or a directory with two images, is refused, and
# what was staged before stays as it was.
test_toolbox_staging_refuses_an_incomplete_set() {
  local name="test_toolbox_staging_refuses_an_incomplete_set"
  local incoming="$WORK_DIR/refuse-incoming" part n=0
  toolbox_set "$incoming" "$OLDER_DIGEST"
  for part in "${TOOLBOX_PARTS[@]}"; do
    n=$((n + 1))
    toolbox_set "$WORK_DIR/short-$n" "$FIXTURE_DIGEST" "$part"
    if stage_toolbox "$WORK_DIR/short-$n" "$incoming"; then
      bad "$name" "a set without its .$part was staged"
      return
    fi
  done
  toolbox_set "$WORK_DIR/two" "$FIXTURE_DIGEST"
  toolbox_set "$WORK_DIR/two" "$OLDER_DIGEST"
  if stage_toolbox "$WORK_DIR/two" "$incoming"; then
    bad "$name" "a directory with two images was staged"
  elif [[ ! -f "$incoming/toolbox-$OLDER_DIGEST.json.sig" ]]; then
    bad "$name" "a refused set disturbed the staged one"
  else
    ok "$name"
  fi
}

# The storage home is the env file's RUNNER_STORAGE_HOME when it sets one, and
# the runner's own default when it does not.
test_toolbox_storage_home_follows_the_env_file() {
  local name="test_toolbox_storage_home_follows_the_env_file"
  local configured="$WORK_DIR/configured.env" bare="$WORK_DIR/bare.env" set_home default_home
  printf 'AGENTSFLEET_API_URL=https://api.example.test\nRUNNER_STORAGE_HOME=/srv/runner\n' >"$configured"
  printf 'AGENTSFLEET_API_URL=https://api.example.test\n' >"$bare"
  set_home="$(storage_home "$configured")"
  default_home="$(storage_home "$bare")"
  if [[ "$set_home" != "/srv/runner" ]]; then
    bad "$name" "a configured home read as '$set_home'"
  elif [[ "$default_home" != "/var/lib/agentsfleet-runner" ]]; then
    bad "$name" "an unset home read as '$default_home'"
  else
    ok "$name"
  fi
}

test_toolbox_set_files_names_the_three_files_image_first
test_toolbox_staging_replaces_the_previous_set
test_toolbox_staging_refuses_an_incomplete_set
test_toolbox_storage_home_follows_the_env_file

# Exercise the public reconstruction boundary without mounting or modifying an image.
if python3 -I - "$SCRIPT_DIR/toolbox.sh" "$WORK_DIR" <<'PYTEST'
import base64, copy, gzip, hashlib, io, json, subprocess, sys, tarfile
from pathlib import Path
library, scratch = sys.argv[1], Path(sys.argv[2])
root = scratch / "inventory"
root.mkdir()
lower, upper = root / "lower", root / "upper"
(lower / "etc").mkdir(parents=True)
upper.mkdir()
image_bytes = b"synthetic read-only image fixture"
sha = hashlib.sha256(image_bytes).hexdigest()
image = root / f"toolbox-{sha}.erofs"
image.write_bytes(image_bytes)
packages, blocks, info = [], [], {}
for name in ("python3", "git"):
    packages.append({"package": name, "architecture": "amd64", "version": "1:2.3-4",
                     "sha256": "1" * 64,
                     "copyright": {"path": f"/usr/share/doc/{name}/copyright", "sha256": "2" * 64}})
    blocks.append(f"Package: {name}\nStatus: install ok installed\nArchitecture: amd64\nVersion: 1:2.3-4\nSource: {name} (2.3-4)\n")
    info[f"{name}.list"] = f"/.\n/usr/bin/{name}\n"
    info[f"{name}.md5sums"] = f"{'3' * 32}  usr/bin/{name}\n"
    info[f"{name}.postinst"] = "#!/bin/sh\nprintf 'scanner data only'\n"
metadata = {"status": "\n".join(blocks), "info": info}
canonical = lambda value: json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
def encode(value, package_set=packages):
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode="w") as archive:
        files = {"status": value["status"], "packages.json": canonical(package_set).decode(),
                 **{f"info/{name}": text for name, text in value["info"].items()}}
        for name, text in files.items():
            member = tarfile.TarInfo(name)
            data = text.encode()
            member.size = len(data)
            archive.addfile(member, io.BytesIO(data))
    return base64.b64encode(gzip.compress(output.getvalue(), mtime=0)).decode()
release = {"sha256": sha, "length": len(image_bytes), "packages": packages,
           "package_metadata": encode(metadata)}
manifest = image.with_suffix(".json")
marker = lower / "etc/toolbox-inventory.sha256"
def write(value, commit=True):
    manifest.write_bytes(canonical(value))
    if commit:
        try:
            compressed = base64.b64decode(value["package_metadata"], validate=True)
        except ValueError:
            compressed = b""
        marker.write_text(hashlib.sha256(compressed).hexdigest() + "\n")
def run():
    return subprocess.run(["bash", "-euo", "pipefail", "-c",
                           'source "$1"; toolbox_restore_scan_inventory "$2" "$3" "$4"',
                           "--", library, str(image), str(lower), str(upper)], capture_output=True)
def refuse(label, value, commit=True):
    write(value, commit)
    before = {str(p.relative_to(upper)): p.read_bytes() for p in upper.rglob("*") if p.is_file()}
    result = run()
    after = {str(p.relative_to(upper)): p.read_bytes() for p in upper.rglob("*") if p.is_file()}
    assert result.returncode != 0 and result.stderr and before == after, label
    print(f"ok   inventory refuses {label}")
write(release)
result = run()
assert result.returncode == 0, result.stderr.decode()
assert (upper / "var/lib/dpkg/status").read_text() == metadata["status"]
assert {p.name: p.read_text() for p in (upper / "var/lib/dpkg/info").iterdir()} == info
assert all(p.stat().st_mode & 0o777 == 0o644 for p in (upper / "var/lib/dpkg/info").iterdir())
assert image.read_bytes() == image_bytes and not (lower / "var").exists()
import shutil
shutil.rmtree(upper / "var")
print("ok   inventory reconstructs exact records without changing the image")
for label, change in (
    ("coherent package omission", lambda value: value["packages"].pop()),
    ("altered source attribution", lambda value: value.update(package_metadata=encode({**metadata, "status": metadata["status"].replace("Source: git (2.3-4)", "Source: git (9.0)")}))),
    ("altered file evidence", lambda value: value.update(package_metadata=encode({**metadata, "info": {**info, "git.list": "/usr/bin/other\n"}}))),
):
    write(release)
    value = copy.deepcopy(release)
    change(value)
    if label == "coherent package omission":
        value["package_metadata"] = encode({"status": blocks[0], "info": {key: text for key, text in info.items() if key.startswith("python3.")}}, value["packages"])
    refuse(label, value, commit=False)
for label, change in (
    ("duplicate package", lambda value: value["packages"].append(value["packages"][0])),
    ("invalid package identity", lambda value: value["packages"][0].update(package="../escape")),
    ("wrong version", lambda value: value["packages"][0].update(version="9.0")),
    ("invalid archive digest", lambda value: value["packages"][0].update(sha256="bad")),
    ("unsafe copyright", lambda value: value["packages"][0]["copyright"].update(path="../../escape")),
    ("wrong image length", lambda value: value.update(length=len(image_bytes) + 1)),
    ("invalid base64", lambda value: value.update(package_metadata="!")),
    ("concatenated compressed records", lambda value: value.update(package_metadata=base64.b64encode(base64.b64decode(value["package_metadata"]) * 2).decode())),
    ("expanded metadata above limit", lambda value: value.update(package_metadata=base64.b64encode(gzip.compress(b"x" * 4_194_305)).decode())),
):
    value = copy.deepcopy(release)
    change(value)
    if value["packages"] != release["packages"]:
        value["package_metadata"] = encode(metadata, value["packages"])
    refuse(label, value)
for label, value in (
    ("path traversal", {**metadata, "info": {**info, "../escape.list": "x"}}),
    ("missing file list", {**metadata, "info": {name: text for name, text in info.items() if name != "git.list"}}),
    ("duplicate installed record", {**metadata, "status": metadata["status"] + "\n" + blocks[0]}),
    ("wrong install status", {**metadata, "status": metadata["status"].replace("install ok installed", "deinstall ok config-files")}),
    ("unsafe source", {**metadata, "status": metadata["status"].replace("Source: git (2.3-4)", "Source: ../escape")}),
):
    refuse(label, {**release, "package_metadata": encode(value)})
write(release)
marker.unlink()
marker.symlink_to(manifest)
assert run().returncode != 0 and not any(upper.iterdir())
marker.unlink()
write(release)
manifest.write_text('{"sha256":"a","sha256":"b"}')
assert run().returncode != 0 and not any(upper.iterdir())
manifest.write_bytes(b" " * 1_048_577)
assert run().returncode != 0 and not any(upper.iterdir())
write(release)
image.write_bytes(image_bytes + b"changed")
assert run().returncode != 0 and not any(upper.iterdir())
image.write_bytes(image_bytes)
(upper / "sentinel").write_text("keep")
refuse("nonempty upper", release)
print("ok   inventory refuses linked commitments, duplicate JSON, oversized releases and changed images")
PYTEST
then
  ok "test_toolbox_scan_inventory_boundary"
else
  bad "test_toolbox_scan_inventory_boundary" "reconstruction or refusal changed records"
fi

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[[ "$failed" -eq 0 ]]
