//! What activating a fleet owes its trigger schedules: one registration each.
//!
//! Every way a fleet becomes active leaves its trigger schedules stored and
//! not yet registered: Save, a chat save, PATCH `active`, an approved setup
//! grant, an applied revision. Each ends in this one sweep, because a path
//! that skips it saves a fleet whose schedules never fire, and only the row's
//! `syncing` says so.
//!
//! # Bounded, and never the caller's failure
//!
//! The sweep gives every owed schedule one attempt under one shared limit. A
//! push the scheduler refuses is recorded on its row like every other push
//! (see [`crate::ScheduleService`]); a datastore error or the limit ends the
//! sweep with a log record. Either way the activation the caller committed
//! stands, and the row's own sync state is what a person retries from.

use std::time::Duration;

use af_core::id::Uuid7;
use jiff::Timestamp;

use crate::error::Result;
use crate::model::{Schedule, Source, SyncStatus};
use crate::qstash::Attempts;
use crate::service::Schedules;

/// The limit one activation shares across every schedule it owes.
const LIMIT: Duration = Duration::from_secs(5);

/// A fleet's trigger schedules saved locally while their registration is owed.
const EVENT_SYNC_PENDING: &str = "fleet_setup_schedule_sync_pending";

impl Schedules {
    /// Registers each of `fleet`'s trigger schedules that is not yet synced.
    ///
    /// Nothing for a fleet outside `workspace`, exactly as for one with no
    /// schedules. Each schedule is sent once: the sweep shares one limit, and
    /// repeating one would leave the rest unsynced.
    ///
    /// # Errors
    /// Reports a datastore that would not answer the read of `fleet`'s
    /// schedules, before anything is pushed. What follows that read is logged
    /// and left on the rows, never raised; see the module note.
    pub async fn register_triggers(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        now: Timestamp,
    ) -> Result<()> {
        if !self.store().holds_fleet(workspace, fleet).await? {
            return Ok(());
        }
        let owed: Vec<Schedule> = self
            .store()
            .list(fleet)
            .await?
            .into_iter()
            .filter(owes_registration)
            .collect();
        let code = match tokio::time::timeout(LIMIT, self.register_each(&owed, now)).await {
            Ok(Ok(())) => return Ok(()),
            Ok(Err(cause)) => cause.code(),
            Err(_elapsed) => af_core::error_code::SCHEDULE_SYNCING,
        };
        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let error_code = code.as_str();
        let fleet_id = fleet.as_str();
        tracing::warn!(event = EVENT_SYNC_PENDING, error_code, fleet_id);
        Ok(())
    }

    /// One claim and one push per schedule, ending at the first datastore
    /// error. A schedule another syncer holds is that syncer's to push.
    async fn register_each(&self, owed: &[Schedule], now: Timestamp) -> Result<()> {
        for schedule in owed {
            let token = self.store().mint(now)?;
            let claimed = self
                .store()
                .claim_current(&schedule.fleet_id, &schedule.schedule_id, &token)
                .await?;
            if let Some(held) = claimed {
                self.reconcile_with(&held, &token, Attempts::Once).await?;
            }
        }
        Ok(())
    }
}

/// Whether `schedule` is a trigger's and still owes the scheduler a push.
fn owes_registration(schedule: &Schedule) -> bool {
    schedule.source == Source::Trigger && schedule.sync_status != SyncStatus::Synced
}
