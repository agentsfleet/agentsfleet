//! What a fleet's create, read and every change answer with.

use af_core::id::Uuid7;
use afd_approval::GrantRow;
use jiff::Timestamp;

use crate::trigger::TriggerRow;

/// One bounded page and the opaque continuation after it.
#[derive(Debug, serde::Serialize)]
pub struct Page<T> {
    /// Current bounded page.
    pub items: Vec<T>,
    /// No table-wide count is performed.
    pub total: Option<u64>,
    /// Opaque boundary, null on the final page.
    pub next_cursor: Option<String>,
}

/// A fleet as its workspace reads it: the row, what wakes it besides its
/// schedules, what it asked to hold, and its counters.
///
/// Schedules are read by the route through `afd_cron`, which renders them
/// against the deployment's clock.
#[derive(Debug)]
pub struct SavedFleet {
    /// The fleet's identity, which is also its thread's.
    pub id: Uuid7,
    /// Validated instance slug.
    pub name: String,
    /// `active` or `archived`.
    pub status: String,
    /// Strong resource tag every change names.
    pub etag: String,
    /// The workspace model its runs resolve.
    pub model_ref: String,
    /// Where its HTTP tools may reach.
    pub http_egress: af_wire::policy::HttpEgress,
    /// Whether its HTTP tools may also reach the package registries.
    pub package_domains: bool,
    /// Its `fleet.triggers` rows, oldest first.
    pub triggers: Vec<TriggerRow>,
    /// Every grant it asked for, newest first.
    pub grants: Vec<GrantRow>,
    /// Whether it reads workspace memories.
    pub memory_reads_workspace: bool,
    /// Whether it publishes workspace memories.
    pub memory_publishes_workspace: bool,
    /// Creation instant.
    pub created_at: Timestamp,
    /// Latest change.
    pub updated_at: Timestamp,
    /// Exact lifetime event count.
    pub events_processed: String,
    /// Exact lifetime spend in nanos.
    pub budget_used_nanos: String,
    /// Unanswered approval gates.
    pub pending_approvals: i64,
}

/// What a create answers: the fleet, its first turn, and whether an earlier
/// call with the same operation already made both.
#[derive(Debug)]
pub struct CreatedFleet {
    /// The fleet as it reads after the create's commit.
    pub fleet: SavedFleet,
    /// The first message, as the fleet's first turn.
    pub turn_id: Uuid7,
    /// Whether an earlier create with the same operation made this fleet.
    pub replayed: bool,
}
