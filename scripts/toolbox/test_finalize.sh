#!/usr/bin/env bash
# Real package removal, read-only payload checks, and builder publication failures.
set -euo pipefail

if [ "$(uname -s)" != Linux ] || [ "$(id -u)" -ne 0 ]; then
  echo "test_finalize.sh: needs Linux root and the toolbox builder" >&2
  exit 1
fi
readonly HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly REPOSITORY="$(cd "$HERE/../.." && pwd)"
readonly FINALIZATION_TEST_ENGINE="$(command -v mmdebstrap)"
work="$(mktemp -d /var/tmp/toolbox-finalize-tests.XXXXXX)"
cleanup() {
  local point
  for point in "$work/mount/workspace" "$work/mount"; do
    if mountpoint -q "$point" && ! umount "$point"; then
      echo "test_finalize.sh: unmount failed; retained $work" >&2
      return 1
    fi
  done
  rm -rf "$work"
}
trap cleanup EXIT
readonly FINALIZATION_TEST_WORK="$work"
export FINALIZATION_TEST_ENGINE FINALIZATION_TEST_WORK
mkdir "$work/bin" "$work/mount" "$work/workspace"

# The wrapper changes only the external dpkg boundary in a disposable root.
cat >"$work/fault-dpkg" <<'SH'
#!/bin/sh
set -eu
if [ "$1" != --purge ]; then exec /usr/bin/.finalization-real-dpkg "$@"; fi
mode=$(cat /tmp/finalization-fault-case)
if [ "$mode" = failure ]; then
  echo "fixture-fault: failure" >&2
  exit 73
fi
/usr/bin/.finalization-real-dpkg "$@"
if [ ! -e /tmp/finalization-fault-fired ]; then
  touch /tmp/finalization-fault-fired
  case "$mode" in
    python) rm /usr/bin/python3 ;;
    config) rm /etc/apt/apt.conf.d/70debconf ;;
    notice) rm /usr/share/common-licenses/GPL-2 ;;
    source)
      python3 - <<'PY'
from pathlib import Path
path = Path('/var/lib/dpkg/status')
before = path.read_text()
after = before.replace('Source: perl\n', 'Source: fixture-source\n')
after = after.replace('Source: perl (', 'Source: fixture-source (')
assert after != before, 'source fault did not reach a retained Perl record'
path.write_text(after)
PY
      ;;
    *) echo "unknown fault case" >&2; exit 2 ;;
  esac
  echo "fixture-fault: $mode" >&2
fi
SH
chmod 0755 "$work/fault-dpkg"
cat >"$work/bin/mmdebstrap" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
args=()
for argument in "$@"; do
  case "$argument" in
    --customize-hook=*toolbox-finalizer/finalize.py*)
      args+=("--customize-hook=cp \"\$1/usr/bin/dpkg\" \"\$1/usr/bin/.finalization-real-dpkg\"")
      args+=("--customize-hook=copy-in $FINALIZATION_TEST_WORK/fault-dpkg /usr/bin")
      args+=("--customize-hook=mv \"\$1/usr/bin/fault-dpkg\" \"\$1/usr/bin/dpkg\"")
      args+=("--customize-hook=copy-in $FINALIZATION_TEST_WORK/finalization-fault-case /tmp")
      ;;
  esac
  args+=("$argument")
done
exec "$FINALIZATION_TEST_ENGINE" "${args[@]}"
SH
chmod 0755 "$work/bin/mmdebstrap"

verify="$(cat <<'PY'
import hashlib, json, os, shutil
from pathlib import Path
import sys
release = json.load(sys.stdin)
names = {package['package'] for package in release['packages']}
removed = set(sys.argv[1].split())
assert not names & removed, names & removed
assert {'python3', 'git', 'perl-base', 'perl-modules-5.40'} <= names
for command in ('apt', 'apt-get', 'dpkg', 'dpkg-query', 'passwd', 'useradd', 'chpasswd'):
    assert shutil.which(command) is None, command
for name in ('/var/lib/dpkg', '/var/lib/apt', '/var/cache/apt'):
    assert not Path(name).exists(), name
for package in release['packages']:
    notice = package['copyright']
    contents = Path(notice['path']).read_bytes()
    assert contents and hashlib.sha256(contents).hexdigest() == notice['sha256'], notice
for name in ('/etc/passwd', '/etc/group', '/etc/shadow', '/etc/gshadow',
             '/etc/apt/apt.conf.d/70debconf', '/etc/dpkg/origins/debian'):
    assert Path(name).is_file() and Path(name).stat().st_size, name
assert os.statvfs('/').f_flag & os.ST_RDONLY, 'root is not read-only'
try:
    Path('/image-write').write_text('refuse')
except OSError:
    pass
else:
    raise AssertionError('root write succeeded')
Path('/workspace/proof').write_text('workspace is writable')
assert Path('/workspace/proof').read_text() == 'workspace is writable'
import hashlib, json, pathlib, ssl
print(f"retained packages and notice hashes: {len(names)}")
PY
)"

make -C "$REPOSITORY" toolbox-image TOOLBOX_DIR="$work/positive" >"$work/positive.log" 2>&1 || {
  cat "$work/positive.log" >&2
  exit 1
}
images=("$work/positive"/toolbox-*.erofs)
[ "${#images[@]}" = 1 ] && [ -f "${images[0]}" ]
image="${images[0]}"
mount -t erofs -o loop,ro,nosuid,nodev -- "$image" "$work/mount"
mount --bind "$work/workspace" "$work/mount/workspace"
purge_names="$(PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$HERE" python3 -c 'from finalize import APPROVED_PURGE; print(" ".join(sorted(APPROVED_PURGE)))')"
chroot "$work/mount" python3 -c "$verify" "$purge_names" <"${image%.erofs}.json"
chroot "$work/mount" perl -MGit -MJSON::PP -MIPC::Open2 -e 'print "Git Perl dependencies work\n"'
printf 'ok - test_finalization_preserves_runtime_and_removes_management\n'
printf 'ok - test_finalization_preserves_shared_licences_and_owned_files\n'
umount "$work/mount/workspace"
umount "$work/mount"

for mode in failure python config notice source; do
  printf '%s\n' "$mode" >"$work/finalization-fault-case"
  out="$work/$mode"
  mkdir "$out"
  if PATH="$work/bin:$PATH" make -C "$REPOSITORY" toolbox-image TOOLBOX_DIR="$out" >"$work/$mode.log" 2>&1; then
    echo "test_finalize.sh: $mode unexpectedly published an image" >&2
    exit 1
  fi
  grep -Fq "fixture-fault: $mode" "$work/$mode.log" || { cat "$work/$mode.log" >&2; exit 1; }
  case "$mode" in
    failure) diagnostic='returned non-zero exit status 73' ;;
    python) diagnostic='retained payload changed: /usr/bin/python3' ;;
    config) diagnostic='retained payload changed: /etc/apt/apt.conf.d/70debconf' ;;
    notice) diagnostic='retained payload changed: /usr/share/common-licenses/GPL-2' ;;
    source) diagnostic='unexpected final installed set or source versions' ;;
  esac
  grep -Fq "$diagnostic" "$work/$mode.log" || { cat "$work/$mode.log" >&2; exit 1; }
  python3 - "$out" <<'PY'
from pathlib import Path
import sys
output = Path(sys.argv[1])
assert output.is_dir(), output
assert not list(output.glob('toolbox-*')), f'failed finalization published: {list(output.iterdir())}'
PY
  printf 'ok - test_finalization_refuses_%s_before_publication\n' "$mode"
done
