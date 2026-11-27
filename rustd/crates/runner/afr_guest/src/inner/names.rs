//! The hosts file every process inside resolves names through, replaced
//! whole while they run.
//!
//! ```text
//!   staging/.hosts…  a new file, written whole, user 0's, 0644
//!   hosts            a bind of it mounted beneath the one there now,
//!                    then the one above unmounted
//!   staging/.hosts…  unlinked; the mount keeps the file
//! ```
//!
//! A lookup opens either the old file or the new one, never a part of
//! either, and exactly one mount stays on the hosts file. The guest's mounts
//! are shared and the inner environment's mount namespace sits downstream
//! of them, receiving each, so the one swap in the executor's namespace reaches every
//! process inside, and the next environment is built over the new file.

use std::fs::Permissions;
use std::io::{self, Write as _};
use std::os::fd::AsFd as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

use rustix::fs::CWD;
use rustix::mount::{MoveMountFlags, OpenTreeFlags, UnmountFlags};

/// The staged file's mode: every process may read it, only user 0 write it.
const MODE: u32 = 0o644;
/// What each staged file's name starts with.
const PREFIX: &str = ".hosts";

/// Puts `file` in place of the hosts file at `hosts`, staging it in
/// `staging`, a directory on the same namespace's file systems that no
/// tenant process writes. Acts with the calling thread's file identity,
/// which must be user 0's, so the tenant cannot write what it reads.
///
/// # Errors
/// The file could not be staged, or the mounts would not swap; the old file
/// stays in place.
pub(super) fn replace(staging: &Path, hosts: &Path, file: &str) -> io::Result<()> {
    let mut staged = tempfile::Builder::new()
        .prefix(PREFIX)
        .permissions(Permissions::from_mode(MODE))
        .tempfile_in(staging)?;
    staged.write_all(file.as_bytes())?;
    let tree = rustix::mount::open_tree(
        CWD,
        staged.path(),
        OpenTreeFlags::OPEN_TREE_CLONE | OpenTreeFlags::OPEN_TREE_CLOEXEC,
    )?;
    rustix::mount::move_mount(
        tree.as_fd(),
        c"",
        CWD,
        hosts,
        MoveMountFlags::MOVE_MOUNT_F_EMPTY_PATH | MoveMountFlags::MOVE_MOUNT_BENEATH,
    )?;
    rustix::mount::unmount(hosts, UnmountFlags::DETACH)?;
    Ok(())
}
