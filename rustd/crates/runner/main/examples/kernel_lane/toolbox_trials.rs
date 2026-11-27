//! The toolbox's own trials, whichever engine runs on it: its build
//! reproduces, and its image fits its size cap.

use std::fs;
use std::path::Path;
use std::process::Command;

use libtest_mimic::Failed;

use crate::lane::{Lane, toolbox_source};
use crate::run::expect;
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

/// Every toolbox trial, by the name the lane reports it under.
pub(crate) const TRIALS: &[(&str, Body)] = &[
    ("test_toolbox_build_is_reproducible", reproducible),
    ("test_toolbox_image_fits_its_size_cap", fits_size_cap),
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
