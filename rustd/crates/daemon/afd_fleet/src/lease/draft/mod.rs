//! A setup draft's turn on the common lease path: claim, offer, renewal,
//! report, each on the shared lease statements ([`super::sql::shared`]).
//!
//! A draft's lease is a fleet-like lease the draft owns
//! ([`super::LeaseOwner::Draft`]): the draft's slot (`runner.slots.id` is the
//! draft's id) fences it, its `event_id` is the draft message it runs, and the
//! runner reaches the daemon only through the same fenced verbs a fleet lease
//! uses. The draft's own rows (which message, its finish, the one candidate)
//! are `afd_authoring::setup_lease`'s, written in these transactions.
//!
//! What differs from a fleet's lease:
//! - No fleet gates and no fleet budget: the payer's balance is checked, and
//!   nothing is reserved.
//! - Key custody: a tenant's runner is never offered a draft whose message
//!   runs on the platform model, whose key the lease carries.
//! - A short `max_runtime_seconds`, the setup turn's wall, so one runaway
//!   costs minutes.
//! - No sandbox and one tool, `propose_setup`; no checkpoint. A lapsed lease is
//!   reoffered and reruns its message from the start, at most
//!   [`af_core::limits::EVENT_LEASES_MAX`] leases, then the message fails.
//! - Offered only to a protocol v3 runner; a v2 runner keeps the authoring
//!   attempt path (`afd_authoring::lease`) until the runner's setup agent
//!   replaces it.

mod claim;
mod renew;
mod report;

use af_core::id::Uuid7;

pub use claim::DraftOffer;

/// What a session keeps about a draft-owned lease it offered: the draft, so
/// its renewals, report and live answer take the draft's path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftTarget {
    /// The draft the lease runs a turn of.
    pub draft_id: Uuid7,
}

/// [`super::sql::draft::SELECT_DRAFT_LEASE`]'s row, by column name.
#[derive(Debug, sqlx::FromRow)]
struct DraftLease {
    draft_id: Uuid7,
    event_id: Uuid7,
    workspace_id: Uuid7,
    tenant_id: Uuid7,
    posture: String,
    provider: String,
    model: String,
    fencing_token: i64,
    status: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: jiff::Timestamp,
    cancelled: bool,
}

impl super::Leases {
    /// The draft-owned lease `lease` names, if `runner` holds it.
    async fn draft_lease(
        &self,
        lease: &Uuid7,
        runner: &Uuid7,
    ) -> crate::Result<Option<DraftLease>> {
        sqlx::query_as(super::sql::draft::SELECT_DRAFT_LEASE)
            .bind(lease)
            .bind(runner)
            .fetch_optional(&mut *self.pool().acquire().await?)
            .await
            .map_err(crate::error::query("read a draft lease"))
    }
}
