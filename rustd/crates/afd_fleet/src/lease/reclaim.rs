//! Re-leasing a dead holder's event from Postgres alone.
//!
//! When a claim wins a fleet whose prior claim had lapsed, the dead holder's
//! still-`active` lease row carries the billing context and the event row it
//! names carries the body. [`Leases::reclaim_prior_active`] takes both in one
//! atomic statement and expires the old lease on the way, so the caller can
//! re-lease the SAME event under the fresh, higher fence — no Redis re-read
//! (the envelope is durable in Postgres) and no re-billing (the original lease
//! already debited).
//!
//! No prior active lease means the fleet is simply free, and the caller pulls a
//! fresh event instead.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use sqlx::Row as _;

use crate::error::{Result, query};
use crate::lease::store::Leases;
use crate::lease::sql;

/// Statement name, for the context a query failure carries.
const CONTEXT_RECLAIM: &str = "lease reclaim";

/// The billing a reclaimed lease carries FORWARD rather than re-charging.
///
/// Separate from [`Reclaimed`] because it is the half that must not be
/// re-derived: the original lease already debited the tenant, so a reclaim that
/// resolved these afresh could bill twice, and a rotation between the two
/// resolutions would bill the wrong rate. Carried, never recomputed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reused {
    /// The tenant the original lease debited.
    pub tenant_id: String,
    /// The billing posture it was charged under.
    pub posture: String,
    /// The model it was charged for.
    pub model: String,
}

/// A dead holder's work, re-leasable under a fresh fence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reclaimed {
    /// The lease that was expired to release this work.
    pub lease_id: String,
    /// The event to re-run.
    pub event_id: String,
    /// Who raised it.
    pub actor: String,
    /// Its type.
    pub event_type: String,
    /// Its body, as stored.
    pub request_json: String,
    /// When the producer raised it.
    pub event_created_at: i64,
    /// The workspace it belongs to.
    pub workspace_id: String,
    /// The billing carried forward.
    pub reused: Reused,
}

impl Leases {
    /// Expire `fleet_id`'s latest active lease and take back what it was
    /// executing.
    ///
    /// Answers `None` when the fleet has no active lease — it is simply free,
    /// and the caller pulls fresh work. Also answers `None` when the event row
    /// has been deleted out from under a live lease: the statement's join is
    /// INNER, so there is nothing to re-deliver, and the dead lease is still
    /// expired on the way past rather than left lingering.
    ///
    /// Call only after winning the claim, so the row found here is
    /// unambiguously the holder this runner displaced.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn reclaim_prior_active(
        &self,
        fleet_id: &Uuid7,
        now: UnixMillis,
    ) -> Result<Option<Reclaimed>> {
        let mut connection = self.pool().acquire().await?;
        let found = sqlx::query(sql::lease::RECLAIM_PRIOR_ACTIVE)
            .bind(fleet_id.as_str())
            .bind(sql::LEASE_STATUS_ACTIVE)
            .bind(sql::LEASE_STATUS_EXPIRED)
            .bind(now.as_millis())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_RECLAIM))?;

        let Some(row) = found else {
            return Ok(None);
        };
        let column = |index| {
            row.try_get::<String, _>(index)
                .map_err(query(CONTEXT_RECLAIM))
        };
        Ok(Some(Reclaimed {
            lease_id: column(0)?,
            event_id: column(1)?,
            actor: column(2)?,
            event_type: column(3)?,
            request_json: column(4)?,
            event_created_at: row.try_get(5).map_err(query(CONTEXT_RECLAIM))?,
            workspace_id: column(6)?,
            reused: Reused {
                tenant_id: column(7)?,
                posture: column(8)?,
                model: column(9)?,
            },
        }))
    }
}
