//! One walk of a workspace: every entry's identity, and what changed since a
//! snapshot an earlier walk kept.
//!
//! ```text
//!   absent from the snapshot, or another kind    changed (a directory: one root)
//!   a directory of the same kind                 unchanged; its entries walked
//!   a file or link, same identity, changed time  unchanged
//!     before the snapshot's tick
//!   anything else                                changed: read again
//! ```
//!
//! The tick rule is git's racy-clean rule. A write inside the clock tick the
//! snapshot was taken on can leave every stat field as the snapshot saw it,
//! so an entry whose change time is at or after that tick is read again
//! rather than trusted.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cap_std::fs::Dir;

use super::identity::{self, Kind, Seen, Tick};
use crate::error::Result;

/// Every entry one walk found, and the tick it began on.
#[derive(Debug, Clone)]
pub(crate) struct Snapshot {
    pub(crate) tick: Tick,
    pub(crate) seen: BTreeMap<PathBuf, Seen>,
}

/// What a walk found against an earlier snapshot.
#[derive(Debug)]
pub(crate) struct Walked {
    /// This walk's snapshot, for a later walk to read against.
    pub(crate) snapshot: Snapshot,
    /// What changed, none below another: each is read whole.
    pub(crate) roots: Vec<PathBuf>,
    /// What the earlier snapshot held that is gone, none below another.
    pub(crate) removed: Vec<PathBuf>,
    /// The entries whose identity the walk read.
    pub(crate) entries: u64,
    /// The bytes of the changed regular files, below a changed root too.
    pub(crate) changed_bytes: u64,
}

/// One directory still to walk: its handle, its path, and whether it lies
/// inside a root already read whole.
type Pending = (Dir, PathBuf, bool);

/// Walks `dir`, judging each entry against `before`; with none, everything
/// is changed.
///
/// # Errors
/// A directory would not open or list, or an entry would not stat.
pub(crate) fn walk(dir: &Dir, before: Option<&Snapshot>) -> Result<Walked> {
    let mut walked = Walked {
        snapshot: Snapshot {
            tick: identity::tick(),
            seen: BTreeMap::new(),
        },
        roots: Vec::new(),
        removed: Vec::new(),
        entries: 0,
        changed_bytes: 0,
    };
    let mut pending: Vec<Pending> = vec![(dir.try_clone()?, PathBuf::new(), false)];
    while let Some((directory, at, inside_root)) = pending.pop() {
        for listed in directory.entries()? {
            let name = listed?.file_name();
            walked.entries += 1;
            let Some(seen) = identity::seen_at(&directory, &name)? else {
                continue;
            };
            let path = at.join(&name);
            let changed = inside_root || changed(before, &path, &seen);
            if changed && !inside_root {
                walked.roots.push(path.clone());
            }
            if changed && seen.kind == Kind::File {
                walked.changed_bytes = walked.changed_bytes.saturating_add(seen.identity.size);
            }
            if seen.kind == Kind::Directory {
                let below = identity::directory_at(&directory, &name)?;
                pending.push((below, path.clone(), changed));
            }
            walked.snapshot.seen.insert(path, seen);
        }
    }
    if let Some(before) = before {
        walked.removed = removed(before, &walked.snapshot);
    }
    Ok(walked)
}

/// Whether `path`, found as `seen`, changed since `before`.
fn changed(before: Option<&Snapshot>, path: &Path, seen: &Seen) -> bool {
    let Some(before) = before else {
        return true;
    };
    match before.seen.get(path) {
        None => true,
        Some(was) if was.kind != seen.kind => true,
        Some(_) if seen.kind == Kind::Directory => false,
        Some(was) => was.identity != seen.identity || seen.identity.changed >= before.tick,
    }
}

/// What `before` held that `now` does not, none below another: an entry
/// whose directory is gone too goes with it.
fn removed(before: &Snapshot, now: &Snapshot) -> Vec<PathBuf> {
    before
        .seen
        .keys()
        .filter(|path| !now.seen.contains_key(*path))
        .filter(|path| {
            path.parent()
                .is_none_or(|parent| parent.as_os_str().is_empty() || now.seen.contains_key(parent))
        })
        .cloned()
        .collect()
}

#[cfg(test)]
#[path = "walk_tests.rs"]
mod tests;
