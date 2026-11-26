//! Why an issue wrote no run, and the host census that tells a full host
//! from an assignment that no longer held.

use af_core::id::Uuid7;

use crate::error::Result;
use crate::run::envelope::Acquired;
use crate::run::store::Runs;

/// Why the issue wrote no run: the guard that refused it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// The fleet is no longer active in its workspace.
    Scope,
    /// The host row is gone, or this connection no longer proves it.
    HostUnproven,
    /// The host is cordoned, draining, revoked or degraded.
    HostClosed,
    /// The placement choice moved since the assignment read it.
    ChoiceStale,
    /// The host may not take this tenant's work or this model key.
    Placement,
    /// Live runs fill every worker.
    CensusFull(Census),
    /// A worker was free, so the assignment no longer held: its thread
    /// fence, its deadline or the head turn moved under the issue.
    Fenced(Census),
    /// The turn's durable task could not be attached.
    Task,
    /// The host's selected decision profile does not read in this build.
    Profile,
}

impl Refused {
    /// The log's `reason` value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scope => "scope",
            Self::HostUnproven => "host_unproven",
            Self::HostClosed => "host_closed",
            Self::ChoiceStale => "choice_stale",
            Self::Placement => "placement",
            Self::CensusFull(_) => "census_full",
            Self::Fenced(_) => "fenced",
            Self::Task => "task",
            Self::Profile => "profile",
        }
    }

    /// Whether another pass can run the turn: this one lost only for room,
    /// time or the placement choice it read. A refusal of this host, or of the
    /// work itself, would meet the same pass again.
    #[must_use]
    pub const fn rerings(self) -> bool {
        matches!(
            self,
            Self::CensusFull(_) | Self::Fenced(_) | Self::ChoiceStale
        )
    }

    /// The host's census, read when the guarded insert wrote nothing.
    #[must_use]
    pub const fn census(self) -> Option<Census> {
        match self {
            Self::CensusFull(census) | Self::Fenced(census) => Some(census),
            _ => None,
        }
    }
}

/// The host's census beside a refused insert, under the same host lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Census {
    /// Live runs on the host.
    pub occupied: i64,
    /// Other fleets' idle holds on the host, which take no worker.
    pub held: i64,
    /// The host's assigned worker limit.
    pub worker_count: i32,
}

impl Runs {
    /// Which guard the insert met, from the host's census in the same
    /// transaction: a full host, or an assignment that no longer held.
    ///
    /// The host row stays locked, so no other issue can fill a worker between
    /// the insert and this read; a run that ended in between can only turn a
    /// full census into [`Refused::Fenced`], never the reverse.
    pub(super) async fn refused_insert(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        runner_id: &Uuid7,
        acquired: &Acquired,
        tenant_id: &Uuid7,
    ) -> Result<Refused> {
        let census = self
            .lock_host_capacity(
                transaction,
                runner_id,
                afd_runner::capacity::Work::Fleet(&acquired.fleet_id),
                Some(tenant_id),
            )
            .await?;
        Ok(census.map_or(Refused::HostUnproven, |capacity| {
            let counted = Census {
                occupied: capacity.occupied(),
                held: capacity.held(),
                worker_count: capacity.workers(),
            };
            if capacity.can_assign() {
                Refused::Fenced(counted)
            } else {
                Refused::CensusFull(counted)
            }
        }))
    }
}

#[cfg(test)]
#[path = "refused_tests.rs"]
mod tests;
