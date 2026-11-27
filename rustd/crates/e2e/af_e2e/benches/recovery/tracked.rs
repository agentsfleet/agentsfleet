//! The guest's watch over the pause bench's workspace: what a microVM's
//! guest agent reads a save's changes from, started here on the file system
//! the bench's scratch directory lies on.

use std::path::Path;

use af_guest_wire::tree::Tracker;

use crate::Checked;

/// Starts recording what is touched in `workspace`, on the file system it
/// lies on.
///
/// # Errors
/// The kernel refused the watch, which user 0 alone may start.
#[cfg(target_os = "linux")]
pub(crate) fn watch(workspace: &Path) -> Checked<Box<dyn Tracker>> {
    use std::os::unix::fs::MetadataExt as _;

    let device = std::fs::metadata(workspace)?.dev();
    // The file system's mount: the highest directory above still on it.
    let disk = workspace
        .ancestors()
        .take_while(|above| std::fs::metadata(above).is_ok_and(|found| found.dev() == device))
        .last()
        .ok_or("a workspace lies on a file system")?;
    Ok(Box::new(afg_entry::track::Watch::over(
        disk, workspace, None,
    )?))
}

/// Only Linux records what is touched on a file system.
#[cfg(not(target_os = "linux"))]
pub(crate) fn watch(_workspace: &Path) -> Checked<Box<dyn Tracker>> {
    Err("recording what is touched on a file system needs Linux's fanotify".into())
}
