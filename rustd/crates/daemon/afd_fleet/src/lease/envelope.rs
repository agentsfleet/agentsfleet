//! Work read from the admissions ledger under the fleet's current fence.

use crate::lease::event::Received;
use crate::lease::installed::FleetWithSession;
use crate::lease::{affinity::Fence, reclaim::Reused};
use af_core::id::Uuid7;
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
    pub event_id: Uuid7,
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
    /// What the claim statement read and wrote beside the slot.
    pub opened: Opened,
}

/// Whether a claimed head belongs to a durable task (M245 §9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadKind {
    /// A chat message: the lease insert proves the setup, and the issue reads
    /// no task source.
    Plain,
    /// A durable task's admission: the issue reads its task source and
    /// attaches it.
    Task,
}

/// The fleet the claim statement read, and the event row it opened.
///
/// One statement claims the slot, reads the fleet and its setup, opens the
/// `fleet.events` row and stamps delivery (M245 §4), so nothing after the
/// claim reads any of these again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    /// Whether this delivery opened the event row, and the counters it read.
    pub received: Received,
    /// The payer: `fleet.fleets.tenant_id`, which the workspace's tenant key
    /// constrains to the workspace's own tenant.
    pub tenant_id: Uuid7,
    /// The catalogue version the claim read, which prices and platform
    /// resolutions are keyed on.
    pub catalogue_version: i64,
    /// The active platform default's versions, when one is active: the key the
    /// platform resolution is cached under.
    pub platform_key: Option<afd_credential::provider::cache::PlatformKeyKey>,
    /// Whether the fleet's runs read workspace-shared memory.
    pub memory_reads_workspace: bool,
    /// Whether the fleet's runs publish to workspace-shared memory.
    pub memory_publishes_workspace: bool,
    /// Whether a later message waited in the head's conversation when the
    /// claim read it. False lets the run skip its first fold ask.
    pub inputs_waiting: bool,
    /// Whether the head admission belongs to a durable task.
    pub head: HeadKind,
    /// The installed setup, parsed through the definition cache.
    pub(crate) setup: FleetWithSession,
}

#[cfg(test)]
impl Opened {
    /// A first delivery of a fleet with no installed setup, for a unit test
    /// that never reaches the parse.
    pub(crate) fn unread(workspace_id: Uuid7, tenant_id: Uuid7) -> Self {
        Self {
            received: Received {
                delivery: crate::lease::event::Delivery::First,
                counters: None,
                opened_at: Timestamp::UNIX_EPOCH,
            },
            tenant_id,
            catalogue_version: 0,
            platform_key: None,
            memory_reads_workspace: false,
            memory_publishes_workspace: false,
            inputs_waiting: true,
            head: HeadKind::Task,
            setup: FleetWithSession::unread(workspace_id),
        }
    }

    /// The same, for a fleet whose setup is installed at `revision`.
    pub(crate) fn installed(workspace_id: Uuid7, tenant_id: Uuid7, revision: Uuid7) -> Self {
        Self {
            setup: FleetWithSession::installed(workspace_id.clone(), revision),
            ..Self::unread(workspace_id, tenant_id)
        }
    }
}
