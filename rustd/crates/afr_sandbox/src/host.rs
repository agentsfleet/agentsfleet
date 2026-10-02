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

use crate::error::{Error, Result, program as failed};

/// Where Debian and Ubuntu install `mke2fs`.
pub const MKE2FS_PATH: &str = "/usr/sbin/mke2fs";
/// Where Debian and Ubuntu install `mount`.
pub const MOUNT_PATH: &str = "/usr/bin/mount";

/// The name a failed `mke2fs` is reported under.
pub(crate) const MKE2FS: &str = "mke2fs";
/// The name a failed `mount` is reported under.
pub(crate) const MOUNT: &str = "mount";
/// The event a host program's start is logged under.
const EVENT_PROGRAM_STARTED: &str = "sandbox_host_program_started";
/// The event a host program that succeeded is logged under.
const EVENT_PROGRAM_COMPLETED: &str = "sandbox_host_program_completed";
/// The event a host program that failed is logged under.
const EVENT_PROGRAM_FAILED: &str = "sandbox_host_program_failed";
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
///
/// The program is killed if the caller stops waiting: a cancelled lease start
/// leaves no `mount` or `mke2fs` running behind it.
pub(crate) async fn run<I, S>(name: &'static str, path: &Path, args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let program = name;
    let event = EVENT_PROGRAM_STARTED;
    tracing::debug!(program, event);
    let ran = tokio::process::Command::new(path)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(Error::from)
        .and_then(|output| {
            if output.status.success() {
                Ok(())
            } else {
                Err(failed(name, output.status, tail(&output.stderr)))
            }
        });
    match &ran {
        Ok(()) => {
            let event = EVENT_PROGRAM_COMPLETED;
            tracing::debug!(program, event);
        }
        Err(error) => {
            let error_code = error.code().as_str();
            let reason = error.to_string();
            let event = EVENT_PROGRAM_FAILED;
            tracing::warn!(program, error_code, reason, event);
        }
    }
    ran
}

/// The last [`STDERR_TAIL_BYTES`] of `bytes`, as text, cut on a character
/// boundary.
pub(crate) fn tail(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim_end();
    let start = trimmed.ceil_char_boundary(trimmed.len().saturating_sub(STDERR_TAIL_BYTES));
    trimmed.get(start..).unwrap_or_default().to_owned()
}

#[cfg(test)]
mod tests;
