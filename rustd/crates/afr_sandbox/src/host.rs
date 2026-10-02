//! The host programs the engine runs, and the one way it runs them.
//!
//! Three steps have no library a runner should link: launching bubblewrap,
//! formatting an ext4 image, and attaching an image to a loop device and
//! mounting it. The crates that attach loop devices generate their bindings
//! with `bindgen` at build time, which would put `libclang` on every Linux
//! build of the workspace; `mount -o loop` is what every distribution ships.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::error::{Result, program};

/// Where Debian and Ubuntu install `mke2fs`.
pub const MKE2FS_PATH: &str = "/usr/sbin/mke2fs";
/// Where Debian and Ubuntu install `mount`.
pub const MOUNT_PATH: &str = "/usr/bin/mount";

/// The name a failed `mke2fs` is reported under.
pub(crate) const MKE2FS: &str = "mke2fs";
/// The name a failed `mount` is reported under.
pub(crate) const MOUNT: &str = "mount";
/// How much of a failed program's error stream a report keeps.
const STDERR_TAIL_BYTES: usize = 2_048;

/// Where each host program lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostTools {
    /// The bubblewrap launcher.
    pub bwrap: PathBuf,
    /// The ext4 formatter.
    pub mke2fs: PathBuf,
    /// The mount command, which attaches loop devices.
    pub mount: PathBuf,
}

impl Default for HostTools {
    fn default() -> Self {
        Self {
            bwrap: crate::probe::BWRAP_PATH.into(),
            mke2fs: MKE2FS_PATH.into(),
            mount: MOUNT_PATH.into(),
        }
    }
}

/// Runs `path` with `args` to completion, refusing on a non-zero exit with the
/// tail of what it wrote to standard error.
pub(crate) async fn run<I, S>(name: &'static str, path: &Path, args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = tokio::process::Command::new(path)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await?;
    if output.status.success() {
        return Ok(());
    }
    Err(program(name, output.status, tail(&output.stderr)))
}

/// The last [`STDERR_TAIL_BYTES`] of `bytes`, as text, cut on a character
/// boundary.
pub(crate) fn tail(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim_end();
    let start = trimmed.len().saturating_sub(STDERR_TAIL_BYTES);
    let boundary = (start..=trimmed.len())
        .find(|&index| trimmed.is_char_boundary(index))
        .unwrap_or(trimmed.len());
    trimmed.get(boundary..).unwrap_or_default().to_owned()
}

#[cfg(test)]
mod tests;
