//! Separately scoped platform curation retains native immutable revisions.

mod curate;
pub(super) mod read;
mod remove;

use super::{Entry, PLATFORM, PLATFORM_DRAFT, Revision};
use crate::types::PlatformLibrary;

fn project(entry: &Entry, head: &Revision) -> PlatformLibrary {
    PlatformLibrary {
        card: entry.card(head),
        published: entry.visibility == PLATFORM,
        updated_at: entry.updated_at,
    }
}

fn require_tag(entry: &Entry, expected: &str) -> crate::Result<()> {
    if expected != entry.etag() {
        return Err(crate::error::ErrorKind::Stale { etag: entry.etag() }.into());
    }
    Ok(())
}
