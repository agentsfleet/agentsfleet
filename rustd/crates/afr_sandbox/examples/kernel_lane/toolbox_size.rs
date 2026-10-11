//! Image bytes and licence notices, checked against the toolbox a lease sees.

use std::fs;

use libtest_mimic::Failed;

use crate::lane::Lane;
use crate::run::expect;
use crate::tools::{answers, shell, shell_calls};
use crate::trials::Body;

/// Eighty percent of the pinned baseline build on each architecture, rounded down.
/// ARM64: 244,531,200 bytes; AMD64: 248,217,600 bytes (baseline deploy build).
#[cfg(target_arch = "aarch64")]
const IMAGE_CAP_BYTES: u64 = 195_624_960;
#[cfg(target_arch = "x86_64")]
const IMAGE_CAP_BYTES: u64 = 198_574_080;
/// An architecture without a measured baseline cannot pass this proof.
#[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
const IMAGE_CAP_BYTES: u64 = 0;
/// Copyright files and aliases in the baseline; shared doc directories are not followed.
const COPYRIGHT_FILES: &str = "156";
/// The installed tree must retain licences while excluding the other payloads.
const LICENCES: &str = r"
python3 - <<'PY'
from pathlib import Path
import subprocess
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
packages = subprocess.check_output(['dpkg-query', '-W', '-f', '${binary:Package}\n'], text=True)
for package in packages.splitlines():
    notice = roots[0] / package.split(':')[0] / 'copyright'
    assert notice.is_file() and notice.stat().st_size > 0, f'missing licence: {package}'
print(len(copyrights))
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
    let calls = [shell(LICENCES, None)];
    let output = shell_calls(lane, "toolbox-licences", &calls)?;
    let [licences] = answers(&output)?;
    expect(
        licences.exit_code == Some(0)
            && licences.error_code.is_none()
            && licences.text.trim() == COPYRIGHT_FILES,
        format!("{COPYRIGHT_FILES} licences and no other docs, got {licences:?}"),
    )
}
