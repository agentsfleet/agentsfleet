//! Image bytes and licence notices, checked against the toolbox a lease sees.

use std::fs;

use libtest_mimic::Failed;

use crate::lane::Lane;
use crate::run::expect;
use crate::tools::{answers, shell, shell_calls};
use crate::trials::Body;

/// ARM64 retains its previously qualified limit until this change is measured there.
/// AMD64 is bounded by the fresh 162,004,992-byte package-manager baseline.
#[cfg(target_arch = "aarch64")]
const IMAGE_CAP_BYTES: u64 = 195_624_960;
#[cfg(target_arch = "x86_64")]
const IMAGE_CAP_BYTES: u64 = 162_004_991;
/// An architecture without a measured baseline cannot pass this proof.
#[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
const IMAGE_CAP_BYTES: u64 = 0;
/// The installed tree must retain licences while excluding the other payloads.
const LICENCES: &str = r"
python3 -I - EXPECTED_PACKAGES <<'PY'
from pathlib import Path
import errno, hashlib, json, os, shutil, sys
roots = [Path('/usr/share') / name for name in ('doc', 'man', 'locale', 'info')]
copyrights = []
for root in roots:
    if not root.exists():
        continue
    for path in root.rglob('*'):
        if path.is_symlink():
            assert path.exists(), f'broken licence or directory link: {path}'
        if path.is_file():
            assert root.name == 'doc' and path.name == 'copyright', f'unstripped: {path}'
            assert path.stat().st_size > 0, f'empty licence: {path}'
            copyrights.append(path)
packages = json.loads(sys.argv[1])
assert packages and copyrights
for package in packages:
    notice = Path(package['copyright']['path'])
    assert notice == roots[0] / package['package'] / 'copyright'
    assert notice.is_file() and notice.stat().st_size > 0, f'missing licence: {notice}'
    assert hashlib.sha256(notice.read_bytes()).hexdigest() == package['copyright']['sha256'], notice
for state in ('/var/lib/dpkg', '/var/lib/apt', '/var/cache/apt', '/var/lib/perl'):
    assert not Path(state).exists(), f'removed package state remains: {state}'
assert os.statvfs('/').f_flag & os.ST_RDONLY
try:
    Path('/toolbox-root-write').write_text('must fail')
except OSError as error:
    assert error.errno in (errno.EROFS, errno.EACCES, errno.EPERM), error
else:
    raise AssertionError('root write succeeded')
workspace = Path('/workspace/toolbox-write')
workspace.write_text('writable workspace')
assert workspace.read_text() == 'writable workspace'
workspace.unlink()
print(f'{len(packages)} verified package notices; read-only root; writable workspace')
PY
";

pub(crate) const TRIALS: &[(&str, Body)] = &[
    ("test_toolbox_image_fits_its_size_cap", fits_size_cap),
    (
        "test_toolbox_keeps_copyright_and_drops_docs",
        keeps_copyright_and_drops_docs,
    ),
];

fn fits_size_cap(lane: &Lane) -> Result<(), Failed> {
    let length = fs::metadata(lane.image.path())?.len();
    eprintln!("toolbox image: {length} bytes; cap: {IMAGE_CAP_BYTES} bytes");
    expect(
        length <= IMAGE_CAP_BYTES,
        format!("toolbox image is {length} bytes; cap is {IMAGE_CAP_BYTES}"),
    )
}

fn keeps_copyright_and_drops_docs(lane: &Lane) -> Result<(), Failed> {
    let release: serde_json::Value =
        serde_json::from_slice(&fs::read(lane.image.path().with_extension("json"))?)?;
    let packages = release.get("packages").ok_or("release has no packages")?;
    let expected = serde_json::to_string(packages)?.replace('\'', "'\\''");
    let command = LICENCES.replace("EXPECTED_PACKAGES", &format!("'{expected}'"));
    let calls = [shell(&command, None)];
    let output = shell_calls(lane, "toolbox-licences", &calls)?;
    let [licences] = answers(&output)?;
    expect(
        licences.exit_code == Some(0)
            && licences.error_code.is_none()
            && licences
                .text
                .contains("verified package notices; read-only root; writable workspace"),
        format!("all retained notices, stripped docs and read-only root, got {licences:?}"),
    )
}
