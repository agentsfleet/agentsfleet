//! `/v1/workspaces/{workspace_id}/library-entries` — what a workspace owns.
//!
//! # A second collection, not a filter on the gallery
//!
//! [`super::workspace_library`] answers what this workspace can INSTALL: the
//! published platform catalogue unioned with its own rows, which is the list
//! `install --library` resolves against. This answers what it OWNS, which is
//! the tenant half alone. Two questions, two shapes, two cursors — the split
//! the models domain settled first, where `/v1/models` is the catalogue and
//! `/v1/tenants/me/models` is the registry a tenant administers, and no query
//! parameter selects between them.
//!
//! Its own module rather than two more verbs next door: that file serves a
//! different collection and stands at 287 lines against a 350 cap, with no
//! room left for the tests these verbs need.
//!
//! # Nothing here decides who may act
//!
//! [`WorkspaceContext`] is the ownership boundary and it runs before either
//! verb, so a caller reaching this file already owns the workspace the path
//! names. The store's predicates are the second half — every statement carries
//! that workspace — and neither layer alone is what keeps one tenant's entries
//! out of another's.
//!
//! # Removing something that is not there is not an error
//!
//! An identifier already removed and one belonging to another workspace both
//! answer `204`. They are indistinguishable on purpose: every statement is
//! scoped by workspace, so separating them would need a second unscoped read
//! whose only effect is to confirm that the identifier exists somewhere. §2 of
//! `docs/REST_API_DESIGN_GUIDELINES.md` asks for the same answer on a replay,
//! and `delete_tenant_model_entry` documents the same behaviour.

use afd_observability::metrics::label::library::Surface;
use serde::{Deserialize, Serialize};

use afd_core::paging::struct_cursor::StructCursor;

pub(crate) mod list;
pub(crate) mod remove;

pub(crate) use self::list::list;
#[cfg(test)]
use self::list::{rendered, resume_from};
pub(crate) use self::remove::remove;

/// The surface both verbs report under, distinct from the gallery's.
///
/// Sharing `FleetSummary` would average two reads whose row counts move for
/// different reasons — a gallery grows with the platform catalogue, this only
/// with what a workspace onboarded — and hide whichever of them is slow.
const SURFACE: Surface = Surface::WorkspaceEntries;

/// The scoped events these verbs are logged under.
const EVENT_LIST: &str = "workspace_library_entries_list_failed";
const EVENT_REMOVE: &str = "workspace_library_entry_remove_failed";

/// The event a completed removal reports under, on every outcome.
const EVENT_REMOVED: &str = "workspace_library_entry_removed";

/// The sentence a path segment that is not an identifier earns.
///
/// `Refusal::malformed` rather than a minted family code, which is what every
/// other path identifier in this daemon does — `parse_fleet_id` and
/// `model_entry::parse_entry_id` both. A code would promise a caller a
/// documented recovery for "you typed a UUID wrong".
const DETAIL_ENTRY_ID: &str = "entry_id must be a UUIDv7";

/// This collection's cursor payload.
///
/// Two columns, because one table has one order — the gallery's carries a tier
/// as well, since it resumes a walk across two. The workspace and limit are
/// carried for the same reason the gallery carries them: a token issued for
/// one walk must not silently seek inside another. Field ORDER is the canonical
/// key order, so reordering this declaration invalidates tokens in flight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    /// The payload generation this cursor was issued under.
    v: u8,
    /// The boundary row's onboarding instant.
    created_at: i64,
    /// The boundary row's identifier, compared bytewise.
    id: String,
    /// The workspace the walk was issued for.
    workspace_uuid: String,
    /// The page size the walk was issued under.
    limit: u32,
}

impl StructCursor for Cursor {
    fn generation(&self) -> u8 {
        self.v
    }
}

#[cfg(test)]
mod tests;
