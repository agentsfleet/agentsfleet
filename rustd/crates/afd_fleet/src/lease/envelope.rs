//! Work read from the admissions ledger under the fleet's current fence.

use crate::lease::{affinity::Fence, reclaim::Reused};
use afd_core::id::Uuid7;
use jiff::Timestamp;

/// Whether the work was pulled fresh or taken back from a dead holder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A previously unissued admission. The caller bills it.
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
    pub leased_until: Timestamp,
    /// How the work was obtained.
    pub kind: Kind,
    /// The event to execute — the admission ledger's LOGICAL id, which the
    /// row, the lease, the ledger and every read address.
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
    pub event_created_at: Timestamp,
    /// The billing a reclaim carries forward. `None` on a fresh pull, which the
    /// caller bills itself.
    pub reused: Option<Reused>,
    /// Whether the runner runs this event in the sandbox it holds for the
    /// fleet. Only a fresh event whose claim found this runner's hold live: a
    /// reclaimed event's first attempt may have run in that sandbox, so it
    /// starts clean.
    pub resume_hold: bool,
    /// Whether this admission was delivered on an earlier attempt.
    pub redelivered: bool,
}
