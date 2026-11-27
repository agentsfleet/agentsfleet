//! Whom the executor's file calls act as, and every tree a tenant writes
//! emptied for the next environment.
//!
//! The executor keeps user 0 and its capabilities, which building namespaces
//! needs, but acts on files as the guest user: what it writes the tenant
//! owns, and what the tenant could not open it cannot either. The kernel
//! drops the file capabilities while the file identity is not 0. Only the
//! renewal's emptying acts as 0, on its own thread, for as long as it runs.

use std::fs;
use std::io;
use std::path::Path;

use rustix::fs::{Gid, Uid};

use crate::error::{Result, unconfined};

/// Why a file identity that did not take is refused.
const DETAIL_UNCHANGED: &str = "the executor's file identity did not change";

/// The calling thread's file identity, set for a scope.
pub(super) struct FilesAs {
    /// The identity put back when the scope ends.
    back: (Uid, Gid),
}

impl FilesAs {
    /// Has the calling thread act on files as `user` from now on, and every
    /// thread it makes after.
    ///
    /// # Errors
    /// The identity did not change.
    pub(super) fn user(user: (Uid, Gid)) -> Result<()> {
        if set(user) == user {
            Ok(())
        } else {
            Err(unconfined(DETAIL_UNCHANGED))
        }
    }

    /// Runs `step` acting on files as user 0, then as `user` again, even
    /// when `step` panics.
    pub(super) fn root<T>(user: (Uid, Gid), step: impl FnOnce() -> T) -> T {
        let _back = Self::scoped((Uid::ROOT, Gid::ROOT), user);
        step()
    }

    fn scoped(now: (Uid, Gid), back: (Uid, Gid)) -> Self {
        set(now);
        Self { back }
    }
}

impl Drop for FilesAs {
    fn drop(&mut self) {
        set(self.back);
    }
}

/// Sets the calling thread's file identity to `user` and answers the one in
/// force after: user 0 always may.
fn set((uid, gid): (Uid, Gid)) -> (Uid, Gid) {
    // `-1` names no identity, so asking for it changes nothing and answers
    // the one in force.
    const ASK: u32 = u32::MAX;
    // SAFETY: changes only this thread's file-system group; no memory.
    unsafe { libc::setfsgid(gid.as_raw()) };
    // SAFETY: changes only this thread's file-system user; no memory.
    unsafe { libc::setfsuid(uid.as_raw()) };
    // SAFETY: reads this thread's file-system group; no memory.
    let gid = unsafe { libc::setfsgid(ASK) };
    // SAFETY: reads this thread's file-system user; no memory.
    let uid = unsafe { libc::setfsuid(ASK) };
    let raw = |id: libc::c_int| u32::try_from(id).unwrap_or(ASK);
    (Uid::from_raw(raw(uid)), Gid::from_raw(raw(gid)))
}

/// Removes everything each of `trees` holds and keeps the directories
/// themselves, which are mounts: a link is removed, never followed.
///
/// # Errors
/// An entry could not be read or removed.
pub(super) fn clear<'a>(trees: impl IntoIterator<Item = &'a Path>) -> io::Result<()> {
    for tree in trees {
        for entry in fs::read_dir(tree)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                fs::remove_dir_all(entry.path())?;
            } else {
                fs::remove_file(entry.path())?;
            }
        }
    }
    Ok(())
}
