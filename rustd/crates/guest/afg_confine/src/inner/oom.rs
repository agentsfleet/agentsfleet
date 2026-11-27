//! Whom the guest kernel's out-of-memory killer may pick: a tenant process,
//! never the guest agent.
//!
//! ```text
//!   build  ─► the guest agent: oom_score_adj -1000, never picked
//!          ─► init inherits it: ending init would end every tenant process,
//!             and it holds next to nothing
//!   entry  ─► child, still user 0: oom_score_adj 0 ─► the guest user ─► exec
//! ```
//!
//! A child inherits its parent's standing, so each tenant process writes the
//! kernel's default back between fork and exec. It writes while it still
//! holds `CAP_SYS_RESOURCE`, which also raises the floor it may later lower
//! its own standing to: once it is the guest user it cannot make itself
//! unpickable (`Documentation/filesystems/proc.rst`, `oom_score_adj`).

use std::io;

use rustix::fs::{Mode, OFlags};
use rustix::path::Arg;

/// The calling process's standing with the out-of-memory killer.
pub(super) const OOM_SCORE_ADJ: &std::ffi::CStr = c"/proc/self/oom_score_adj";
/// `OOM_SCORE_ADJ_MIN`: the kernel never picks a process standing here.
const NEVER_PICKED: &[u8] = b"-1000";
/// The standing every process starts with.
const DEFAULT: &[u8] = b"0";

/// Has the process whose standing is the file `standing` never picked.
///
/// # Errors
/// The file would not open or take the value: the caller lacks
/// `CAP_SYS_RESOURCE`, which lowering a standing needs.
pub(super) fn spare(standing: impl Arg) -> io::Result<()> {
    stand(standing, NEVER_PICKED)
}

/// Puts the calling process's standing back to the kernel's default. Runs in
/// a child between fork and exec: an open, a write and a close, nothing
/// allocated.
///
/// # Errors
/// The file would not open or take the value.
pub(super) fn killable() -> io::Result<()> {
    stand(OOM_SCORE_ADJ, DEFAULT)
}

fn stand(standing: impl Arg, value: &[u8]) -> io::Result<()> {
    let file = rustix::fs::open(standing, OFlags::WRONLY | OFlags::CLOEXEC, Mode::empty())?;
    rustix::io::write(&file, value)?;
    Ok(())
}

#[cfg(test)]
#[path = "oom_tests.rs"]
mod tests;
