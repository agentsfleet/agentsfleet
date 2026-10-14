//! The host programs the engine runs, and the one way it runs them.
//!
//! Three steps have no library a runner should link: launching bubblewrap,
//! formatting an ext4 image, and attaching an image to a loop device and
//! mounting it. The crates that attach loop devices generate their bindings
//! with `bindgen` at build time, which would put `libclang` on every Linux
//! build of the workspace; `mount -o loop` is what every distribution ships.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use afd_core::error_code::{Coded as _, Logged};

use crate::error::{Result, program as failed};

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
/// The file system every workspace disk is formatted with.
pub(crate) const EXT4: &str = "ext4";
/// The flag both `mke2fs` and `mount` take a file-system type after.
const TYPE_FLAG: &str = "-t";

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

impl HostTools {
    /// Formats `image` as a journal-free ext4 whose root `owner` may write.
    pub(crate) async fn format_ext4(&self, image: &Path, owner: (u32, u32)) -> Result<()> {
        run(MKE2FS, &self.mke2fs, format_arguments(image, owner)).await
    }

    /// Mounts `source` at `target` as `fstype` with `options`.
    pub(crate) async fn mount(
        &self,
        fstype: &str,
        options: &str,
        source: &Path,
        target: &Path,
    ) -> Result<()> {
        run(
            MOUNT,
            &self.mount,
            mount_arguments(fstype, options, source, target),
        )
        .await
    }
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
    let output = tokio::process::Command::new(path)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output()
        .await;
    let ran = match output {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => Err(failed(name, output.status, tail(&output.stderr))),
        Err(error) => Err(error.into()),
    };
    match &ran {
        Ok(()) => {
            let event = EVENT_PROGRAM_COMPLETED;
            tracing::debug!(program, event);
        }
        Err(error) => {
            let Logged { error_code, reason } = error.logged();
            let event = EVENT_PROGRAM_FAILED;
            tracing::warn!(program, error_code, reason, event);
        }
    }
    ran
}

/// `mke2fs` for a quiet, journal-free ext4 whose root `owner` may write.
///
/// No journal: the disk outlives no crash, so a journal is writes for nothing.
pub(crate) fn format_arguments(image: &Path, owner: (u32, u32)) -> Vec<OsString> {
    let (user, group) = owner;
    [
        "-q",
        "-F",
        TYPE_FLAG,
        EXT4,
        "-m",
        "0",
        "-O",
        "^has_journal",
        "-E",
    ]
    .into_iter()
    .map(OsString::from)
    .chain([
        format!("root_owner={user}:{group}").into(),
        image.as_os_str().to_owned(),
    ])
    .collect()
}

/// `mount -t fstype -o options source target`.
pub(crate) fn mount_arguments(
    fstype: &str,
    options: &str,
    source: &Path,
    target: &Path,
) -> Vec<OsString> {
    [TYPE_FLAG, fstype, "-o", options]
        .into_iter()
        .map(OsString::from)
        .chain([source.as_os_str().to_owned(), target.as_os_str().to_owned()])
        .collect()
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
