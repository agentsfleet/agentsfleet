//! The identities a save's pins keep, so the next pin reads only what changed.
//!
//! ```text
//!   walk(parent) ─► pin n, its snapshot pending
//!   release(n, Some(generation))   n's snapshot becomes the committed one,
//!                                  labelled by the generation it published
//!   release(n, None)               n's snapshot is dropped
//!   walk(parent) reads against the committed snapshot only when its label
//!   is `parent`; any other walk reads the whole workspace
//! ```
//!
//! The record is a hint a restore never trusts: it says which entries to
//! read again, never what a generation holds. One lost to a reconnect or a
//! reboot costs one walk that reads everything.

use std::collections::HashMap;
use std::path::PathBuf;

use cap_std::fs::Dir;

use super::CURRENT_DIRECTORY;
use super::walk::{Snapshot, walk};
use crate::error::{self, Result};
use crate::protocol::transfer_chunk_bytes;

/// Why a release was refused.
const DETAIL_NO_PIN: &str = "no pin with that number";

/// The committed pin's snapshot and every open pin's.
#[derive(Debug, Default)]
pub struct Pins {
    /// The last committed pin's snapshot, by the generation it published.
    pub(crate) committed: Option<(String, Snapshot)>,
    /// Each open pin's snapshot, by its number.
    pending: HashMap<u64, Snapshot>,
    next: u64,
}

/// One pin's walk: what its export carries and what its answer says.
#[derive(Debug)]
pub struct PinWalk {
    /// The number its release names it by.
    pub pin: u64,
    /// Whether `roots` and `removed` are only what changed since the parent.
    pub delta: bool,
    /// The paths to pack, each with what is below it: `.` for the whole.
    pub roots: Vec<PathBuf>,
    /// What the parent held that is gone, none below another.
    pub removed: Vec<String>,
    /// The entries whose identity the walk read.
    pub entries: u64,
    /// The bytes of the regular files the roots carry.
    pub changed_bytes: u64,
}

impl Pins {
    /// Walks `dir` for a new pin against `parent`, reading only what changed
    /// when the committed snapshot is `parent`'s; the answer travels in one
    /// frame of `frame_bytes`.
    ///
    /// # Errors
    /// The workspace could not be written back or read; no pin is kept.
    pub fn walk(&mut self, dir: &Dir, parent: Option<&str>, frame_bytes: usize) -> Result<PinWalk> {
        written_back(dir)?;
        let before = self
            .committed
            .as_ref()
            .filter(|(label, _)| Some(label.as_str()) == parent)
            .map(|(_, snapshot)| snapshot);
        let walked = walk(dir, before)?;
        let removed = before.and_then(|_| listed(&walked.removed, frame_bytes));
        let pin = self.next;
        self.next += 1;
        self.pending.insert(pin, walked.snapshot);
        let delta = removed.is_some();
        let changed_bytes = if delta {
            walked.changed_bytes
        } else {
            self.pending.get(&pin).map_or(0, Snapshot::file_bytes)
        };
        Ok(PinWalk {
            pin,
            delta,
            roots: if delta {
                walked.roots
            } else {
                vec![PathBuf::from(CURRENT_DIRECTORY)]
            },
            removed: removed.unwrap_or_default(),
            entries: walked.entries,
            changed_bytes,
        })
    }

    /// Ends `pin`: with `generation`, its snapshot is what a walk against
    /// `generation` reads against; with none, the committed one stays.
    ///
    /// # Errors
    /// No open pin has that number.
    pub fn release(&mut self, pin: u64, generation: Option<&str>) -> Result<()> {
        let snapshot = self
            .pending
            .remove(&pin)
            .ok_or_else(|| error::invalid_params(DETAIL_NO_PIN))?;
        if let Some(generation) = generation {
            self.committed = Some((generation.to_owned(), snapshot));
        }
        Ok(())
    }
}

impl Snapshot {
    /// The bytes of every regular file it saw.
    fn file_bytes(&self) -> u64 {
        self.seen
            .values()
            .filter(|seen| seen.kind == super::identity::Kind::File)
            .fold(0, |sum, seen| sum.saturating_add(seen.identity.size))
    }
}

/// Writes back the dirty pages of the filesystem `dir` lies on, before the
/// walk reads a stat field. A page a tenant wrote through a shared mapping
/// stays writable while it is dirty, so a second write to it takes no fault
/// and moves no file time, and the walk would trust the file. Writing it back
/// write-protects the page: the next write through the mapping faults and
/// moves the file's times, which the next walk reads.
#[cfg(target_os = "linux")]
fn written_back(dir: &Dir) -> std::io::Result<()> {
    Ok(rustix::fs::syncfs(dir)?)
}

/// The guest agent runs on Linux alone; elsewhere a walk serves tests, which
/// map no file.
#[cfg(not(target_os = "linux"))]
#[expect(
    clippy::unnecessary_wraps,
    reason = "the Linux build's write-back can fail"
)]
const fn written_back(_dir: &Dir) -> std::io::Result<()> {
    Ok(())
}

/// `removed` as the answer lists it, or none when it cannot: a name that is
/// not UTF-8 has no place in it, and the list must fit the chunk a frame of
/// `frame_bytes` carries beside its envelope. Either way the walk reads the
/// whole workspace instead.
fn listed(removed: &[PathBuf], frame_bytes: usize) -> Option<Vec<String>> {
    let listed = removed
        .iter()
        .map(|path| path.to_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()?;
    let encoded = serde_json::to_vec(&listed).ok()?;
    (encoded.len() <= transfer_chunk_bytes(frame_bytes)).then_some(listed)
}

#[cfg(test)]
#[path = "pins_tests.rs"]
mod tests;
