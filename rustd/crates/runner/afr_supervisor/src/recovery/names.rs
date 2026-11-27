//! How the scan treats a second name of one file.
//!
//! A file with several hard-linked names, as a package store writes them, is
//! read and hashed once per name today, and a restore writes each name as a
//! file of its own. The scan asks one question per file here so that a later
//! implementation can answer it by inode without the scan changing shape.

use super::files::Identity;

/// What the scan does with one file name it found.
///
/// One variant today. A later implementation that recognises a second name of
/// a file adds an alias variant carrying the path the scan already read for
/// that inode; the catalog then lists the name without reading its bytes, and
/// a restore links it to that path. Until that variant exists the case cannot
/// be constructed, and the scan's `let` on this type stops compiling the day
/// it can.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Naming {
    /// Read, hashed and saved as a file of its own.
    OwnFile,
}

/// Decides, once per file the scan finds, how that name is saved.
pub(super) trait SecondNames {
    /// Asked once for each file name, in the inventory's path order, with the
    /// identity the scan pinned for it: its device and inode among it.
    fn naming(&mut self, path: &str, identity: &Identity) -> Naming;
}

/// Every name is a file of its own, whatever inode it shares.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct EveryNameItsOwnFile;

impl SecondNames for EveryNameItsOwnFile {
    fn naming(&mut self, _path: &str, _identity: &Identity) -> Naming {
        Naming::OwnFile
    }
}
