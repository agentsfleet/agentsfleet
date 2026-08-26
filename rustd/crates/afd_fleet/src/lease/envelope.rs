//! What a runner is handed: the envelope, and the two things it can be built
//! from.
//!
//! [`super::assign`] decides WHICH fleet and event a runner gets; this module
//! is what that answer looks like once it exists. The split is by concern
//! rather than by length — the selection pass is about ordering and claims,
//! and this is about a producer's contract, which is the part that changes
//! when the wire changes.
//!
//! # Two sources, one shape
//!
//! Work arrives either off the stream (fresh) or out of a dead holder's lease
//! row (reclaim), and the runner must not be able to tell which. That is why
//! both build the SAME [`Acquired`]: a lease payload that differed by
//! provenance would leak a control-plane detail into the execution plane, and
//! the runner has no use for it — [`Kind`] exists for the audit row, not for
//! the runner.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_redis::FleetEvent;

use crate::error::{Result, envelope_field, row_malformed};
use crate::lease::affinity::{Claimed, Fence};
use crate::lease::reclaim::{Reclaimed, Reused};

/// The stream field carrying the event's type.
///
/// These four are `event_envelope.zig`'s `encodeForXAdd` argv, and the entry id
/// itself IS the event id — there is no `event_id` field. Declared here because
/// this is the only Rust reader of the fleet stream's shape; `afd_redis` hands
/// back untyped fields on purpose, so that it stays a transport and not a
/// second place the envelope is defined.
const FIELD_TYPE: &str = "type";

/// The stream field carrying who raised the event.
const FIELD_ACTOR: &str = "actor";

/// The stream field carrying the owning workspace.
const FIELD_WORKSPACE: &str = "workspace_id";

/// The stream field carrying the request body.
const FIELD_REQUEST: &str = "request";

/// The stream field carrying the producer's instant.
const FIELD_CREATED_AT: &str = "created_at";

/// Whether the work was pulled fresh or taken back from a dead holder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A new entry off the stream. The caller bills it.
    Fresh,
    /// A dead holder's event, re-leased. Its billing is carried, never
    /// re-charged.
    Reclaim,
}

impl Kind {
    /// The wire spelling the audit row records.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::Reclaim => "reclaim",
        }
    }
}

/// The chosen work: a claimed fleet, its fence, and the event to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Acquired {
    /// The fleet whose slot this runner now holds.
    pub fleet_id: Uuid7,
    /// The claim's token, which the issued lease row carries.
    pub fence: Fence,
    /// When the claim lapses.
    pub leased_until: UnixMillis,
    /// How the work was obtained.
    pub kind: Kind,
    /// The event to execute.
    pub event_id: String,
    /// Who raised it.
    pub actor: String,
    /// Its type.
    pub event_type: String,
    /// Its body, as stored.
    pub request_json: String,
    /// The workspace it belongs to.
    ///
    /// A `Uuid7` and not a `String`: it lands in a `uuid` column, and parsing
    /// it HERE — once, at the boundary — is what stops every downstream writer
    /// re-parsing it and each deciding for itself what a malformed value means.
    /// Interior code that trusts its types needs no defensive re-check.
    pub workspace_id: Uuid7,
    /// When the producer raised it.
    ///
    /// An instant rather than a bare `i64`, because the lease row binds four
    /// other integers beside it and two transposed compile clean.
    pub event_created_at: UnixMillis,
    /// The billing a reclaim carries forward. `None` on a fresh pull, which the
    /// caller bills itself.
    pub reused: Option<Reused>,
}

/// Build the acquired envelope from a reclaimed lease.
///
/// Fallible for one reason: the workspace comes back out of a `uuid` column,
/// so a value that will not parse means the row was written by something that
/// is not this daemon. Worth refusing rather than carrying forward.
pub(crate) fn from_reclaim(
    fleet_id: &Uuid7,
    claimed: &Claimed,
    prior: Reclaimed,
) -> Result<Acquired> {
    Ok(Acquired {
        fleet_id: fleet_id.clone(),
        fence: claimed.fence,
        leased_until: claimed.leased_until,
        kind: Kind::Reclaim,
        event_id: prior.event_id,
        actor: prior.actor,
        event_type: prior.event_type,
        request_json: prior.request_json,
        workspace_id: Uuid7::parse(&prior.workspace_id)
            .map_err(row_malformed("fleet.runner_leases", "workspace_id"))?,
        event_created_at: UnixMillis::from_millis(prior.event_created_at),
        reused: Some(prior.reused),
    })
}

/// Build the acquired envelope from a stream entry.
///
/// Every field the producer's contract names is REQUIRED, and a missing one
/// refuses the lease. An earlier revision defaulted them to empty strings,
/// which is exactly the sentinel RULE FN-RS bans: an empty `event_type` does
/// not fail here, it fails later — inside the runner, mid-execution, with
/// nothing left to say where the value went missing. Refusing at the boundary
/// names the field while the fleet and the entry are both still in hand.
pub(crate) fn from_fresh(
    fleet_id: &Uuid7,
    claimed: &Claimed,
    event: &FleetEvent,
) -> Result<Acquired> {
    let field = |name: &'static str| {
        event
            .field(name)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .ok_or_else(|| envelope_field(name))
    };
    Ok(Acquired {
        fleet_id: fleet_id.clone(),
        fence: claimed.fence,
        leased_until: claimed.leased_until,
        kind: Kind::Fresh,
        // The entry id IS the event id — there is no separate field.
        event_id: event.id.as_str().to_owned(),
        actor: field(FIELD_ACTOR)?,
        event_type: field(FIELD_TYPE)?,
        request_json: field(FIELD_REQUEST)?,
        workspace_id: Uuid7::parse(&field(FIELD_WORKSPACE)?)
            .map_err(row_malformed("core.fleet_events", FIELD_WORKSPACE))?,
        event_created_at: field(FIELD_CREATED_AT)?
            .parse()
            .map(UnixMillis::from_millis)
            .map_err(|_unparseable| envelope_field(FIELD_CREATED_AT))?,
        reused: None,
    })
}
