//! The toolbox's own trials: its build reproduces, its image fits its size
//! cap, and a run sees its licence notices kept and its docs dropped.

use std::fs;
use std::path::Path;
use std::process::Command;

use libtest_mimic::Failed;

use crate::lane::{Lane, toolbox_source};
use crate::release::JSON;
use crate::run::expect;
use crate::tools::{answers, shell, shell_calls};
use crate::trials::Body;

/// The most bytes the image may be: under 160 MB, decimal, which is Indy's
/// number (Indy, Nov 27, 2026: "bring the toolbox cap to 160mb"), ruled once
/// Node.js left the toolbox. CI's build refuses past the same number
/// (`.github/actions/build-toolbox/action.yml`).
const IMAGE_CAP_BYTES: u64 = 159_999_999;
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
assert os.statvfs('/usr').f_flag & os.ST_RDONLY
try:
    Path('/usr/toolbox-write').write_text('must fail')
except OSError as error:
    assert error.errno in (errno.EROFS, errno.EACCES, errno.EPERM), error
else:
    raise AssertionError('toolbox write succeeded')
workspace = Path('/workspace/toolbox-write')
workspace.write_text('writable workspace')
assert workspace.read_text() == 'writable workspace'
workspace.unlink()
print(f'{len(packages)} verified package notices; read-only toolbox; writable workspace')
PY
";

/// Every toolbox trial, by the name the lane reports it under.
pub(crate) const TRIALS: &[(&str, Body)] = &[
    ("test_toolbox_build_is_reproducible", reproducible),
    ("test_toolbox_image_fits_its_size_cap", fits_size_cap),
    (
        "test_toolbox_keeps_copyright_and_drops_docs",
        keeps_copyright_and_drops_docs,
    ),
];

fn reproducible(lane: &Lane) -> Result<(), Failed> {
    let out = tempfile::tempdir()?;
    let script = toolbox_source()
        .ok_or("the lane names no toolbox source")?
        .join("build.sh");
    let built = Command::new("bash").arg(script).arg(out.path()).output()?;
    // A failed build prints no path, and hashing the empty path reported only
    // "No such file or directory", which hid that the download had failed.
    expect(
        built.status.success(),
        format!(
            "the second build failed ({}): {}",
            built.status,
            String::from_utf8_lossy(&built.stderr).trim()
        ),
    )?;
    let path = String::from_utf8_lossy(&built.stdout).trim().to_owned();
    let second = crate::release::sha256_file(Path::new(&path))?;
    expect(
        second == lane.image.digest(),
        format!("{second} != {}", lane.image.digest()),
    )
}

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
        serde_json::from_slice(&fs::read(lane.image.path().with_extension(JSON))?)?;
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
                .contains("verified package notices; read-only toolbox; writable workspace"),
        format!("all retained notices, stripped docs and a read-only toolbox, got {licences:?}"),
    )
}
