//! What one edit changed: each file's path and the lines it added and
//! removed, as the runner's file tools counted them. The trace, the live
//! completion frame and the call's full record carry the same list, so no
//! reader works the counts out from the call's arguments.

use std::borrow::Cow;

use garde::Validate;
use serde::{Deserialize, Serialize};

use super::free_of_nul;

/// One file an edit changed.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct FileChange<'a> {
    /// The file, as the call named it relative to its working directory,
    /// secret values masked; never empty.
    #[serde(borrow)]
    #[garde(length(bytes, min = 1))]
    pub path: Cow<'a, str>,
    /// Lines the call added to the file.
    #[garde(skip)]
    pub added: u64,
    /// Lines the call removed from it.
    #[garde(skip)]
    pub removed: u64,
}

impl FileChange<'_> {
    /// The same change, owning its path.
    #[must_use]
    pub fn into_owned(self) -> FileChange<'static> {
        FileChange {
            path: Cow::Owned(self.path.into_owned()),
            added: self.added,
            removed: self.removed,
        }
    }
}

/// Whether every path `changes` names is free of NUL, which no stored string
/// may hold.
#[must_use]
pub fn changes_free_of_nul(changes: &[FileChange<'_>]) -> bool {
    changes.iter().all(|change| free_of_nul(&change.path))
}

/// Whether `changes` are what a trace, a frame and a record hold: each path
/// named and free of NUL. The runner keeps a call's changes only when they
/// are; the frame bounds how many.
#[must_use]
pub fn changes_fit(changes: &[FileChange<'_>]) -> bool {
    changes_free_of_nul(changes) && changes.iter().all(|change| change.validate().is_ok())
}

#[cfg(test)]
#[path = "change_tests.rs"]
mod tests;
