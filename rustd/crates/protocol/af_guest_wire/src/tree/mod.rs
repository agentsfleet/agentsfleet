//! A tree as a tar stream: packed from paths inside a directory handle, or
//! checked whole and unpacked under one, every entry through the handle.
//!
//! The guest agent's `import` and `export` serve these over its workspace, and
//! the supervisor runs the same two over its own staging directory on the
//! host, so a stream is held to one set of checks on both ends of the link.
//!
//! An unpack reads its stream from a spool, a file it can read twice, so the
//! stream is checked whole before its first entry is written: one entry
//! leaving the directory refuses all of it. Ownership in the stream is
//! ignored, so what lands is the unpacking user's; the mode's permission bits
//! are kept and its set-id bits are not.
//!
//! A stream carries each byte its tree holds on disk once: a file with
//! several names is packed under the first and as a link to it under each
//! other, and a hole in a file is left out of the stream and is a hole again
//! where it lands.

use std::io;
use std::path::Path;

use cap_std::fs::{Dir, OpenOptions, OpenOptionsExt as _};
use rustix::fs::OFlags;
use rustix::io::Errno;

use crate::error::{self, Error, Result};

mod pack;
mod unpack;

pub use self::pack::pack;
pub use self::unpack::unpack;

/// A directory named relative to itself.
pub const CURRENT_DIRECTORY: &str = ".";
/// The permission bits an entry keeps: never set-user-id, set-group-id or
/// sticky, which a stream from elsewhere has no business granting.
const PERMISSION_BITS: u32 = 0o777;
/// Open without waiting: a pipe with no writer must not hold the call.
const NONBLOCK: i32 = OFlags::NONBLOCK.bits().cast_signed();

/// Opens `inside` without blocking and refuses anything but a regular file.
///
/// With [`Parents::Make`], an open that fails because a directory is missing
/// makes the directories and tries once more. The open is always tried
/// first, so an escape is refused as one before anything is made.
///
/// # Errors
/// The path leaves the directory, is not a regular file, or will not open.
pub fn regular(
    dir: &Dir,
    inside: &Path,
    options: &mut OpenOptions,
    parents: Parents,
) -> Result<cap_std::fs::File> {
    options.custom_flags(NONBLOCK);
    let file = match (dir.open_with(inside, options), parents) {
        (Err(missing), Parents::Make) if missing.kind() == io::ErrorKind::NotFound => {
            // `inside` is never empty (the directory itself reads as `.`),
            // so it always has a parent to make.
            inside
                .parent()
                .map_or(Ok(()), |parent| dir.create_dir_all(parent))
                .map_err(confined)?;
            dir.open_with(inside, options)
        }
        (opened, _either) => opened,
    }
    .map_err(confined)?;
    if file.metadata()?.is_file() {
        Ok(file)
    } else {
        Err(error::not_a_file())
    }
}

/// Whether opening a file may make its missing parent directories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parents {
    /// Only what exists is opened: a read.
    Keep,
    /// Missing directories are made: a write.
    Make,
}

/// Sorts a failed open.
///
/// `cap_std` reports an escape as a permission refusal
/// that no system call produced, which is how it is told from a real `EACCES`;
/// a pipe with no reader refuses a non-blocking open for writing with
/// `ENXIO`, and that is a file that is not a regular one; a name that is not
/// there is the caller's, and gets the code a handler names to the model.
#[must_use]
pub fn confined(failure: io::Error) -> Error {
    match failure.raw_os_error() {
        None if failure.kind() == io::ErrorKind::PermissionDenied => error::path_refused(),
        Some(code) if code == Errno::NXIO.raw_os_error() => error::not_a_file(),
        _other if failure.kind() == io::ErrorKind::NotFound => error::not_found(failure),
        _other => failure.into(),
    }
}
