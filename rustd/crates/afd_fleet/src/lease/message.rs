//! The interim message verb: one line said in the event's thread before the
//! answer, prepared here and posted by `afd_outbound`'s posters.
//!
//! ```text
//!   proved lease ──► the event's thread ──► one more of eight ──► masked text
//!        │                  │                      │                   │
//!   stale: refuse     none: no channel       at cap: refuse       Interim
//! ```
//!
//! Every refusal is decided before the text is read, so a refused message
//! costs no vault read. The order puts the thread before the count, so a
//! message with nowhere to go does not spend one of the run's eight.
//!
//! # What the mask covers
//!
//! The fleet's declared static credentials, opened from the vault the way the
//! lease that runs it opened them, and masked with the runner's own `Scrub`.
//! A minted token is the broker's and is never held here; the runner masks
//! those before it sends, with the guard that minted them.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_outbound::Interim;
use afd_wire::message_verb::{MESSAGES_PER_RUN_MAX, MessageRequest};
use afr_secrets::{Scrub, StaticSecrets};
use sqlx::Row as _;

use crate::error::{Result, lease_not_found, message_limit_reached, message_no_channel, query};
use crate::lease::pull::Plane;
use crate::lease::sql;
use crate::lease::sql::standing::COUNT_MESSAGE;
use crate::lease::standing::Standing;
use crate::lease::store::Leases;

/// Statement name, for the context a count failure carries.
const CONTEXT_COUNT: &str = "count interim message";

impl Plane {
    /// One line for `lease_id`'s thread, fenced, counted and scrubbed.
    ///
    /// # Errors
    /// Refuses a lease that is not this runner's or not live, a superseded
    /// holder, an event that recorded no thread, and a run that already said
    /// its fill. Reports a datastore or vault that would not answer, and a
    /// mask that could not be built — in which case nothing is posted.
    pub async fn message(
        &self,
        runner_id: &Uuid7,
        lease_id: Uuid7,
        request: &MessageRequest<'_>,
        now: UnixMillis,
    ) -> Result<Interim> {
        let standing = self
            .standing(runner_id, lease_id, request.fencing_token, now)
            .await?;
        let reply = {
            let mut connection = self.leases.pool().acquire().await?;
            Leases::reply_destination(
                &mut connection,
                standing.fleet_id.as_str(),
                &standing.event_id,
            )
            .await?
        }
        .ok_or_else(message_no_channel)?;
        let counted = self
            .leases
            .count_message(&standing.lease_id, request.fencing_token, now)
            .await?;
        let Some(part) = counted else {
            // No slot taken: the run is at its cap, or a reclaim superseded the
            // holder after its standing was read. The second read says which.
            let Standing { lease_id, .. } = standing;
            self.standing(runner_id, lease_id, request.fencing_token, now)
                .await?;
            return Err(message_limit_reached());
        };
        let text = self.masked(&standing, &request.text).await?;
        let Standing {
            lease_id,
            fleet_id,
            workspace_id,
            event_id,
            ..
        } = standing;
        Ok(Interim {
            provider: reply.provider,
            destination: reply.address,
            workspace_id: workspace_id.into(),
            fleet_id: fleet_id.into(),
            event_id,
            lease_id: lease_id.into(),
            text,
            part,
        })
    }

    /// `text` with every declared static secret of the lease's fleet masked.
    ///
    /// What a message posts and what a schedule a fleet writes stores, so a
    /// secret the model echoes reaches neither a thread nor the scheduler.
    ///
    /// # Errors
    /// Refuses a fleet that is no longer installed. Reports a datastore or
    /// vault that would not answer, and a mask that could not be built.
    pub async fn masked(&self, standing: &Standing, text: &str) -> Result<String> {
        let installed = self
            .leases
            .installed(&standing.fleet_id)
            .await?
            .ok_or_else(lease_not_found)?;
        let names = installed.credential_names();
        if names.is_empty() {
            return Ok(text.to_owned());
        }
        let declared = self
            .vault
            .declared(&standing.workspace_id, &names, &self.connectors)
            .await?;
        let scrub = Scrub::of(StaticSecrets::of_map(declared.secrets_map()).values())?;
        Ok(scrub.text(text).into_owned())
    }
}

impl Leases {
    /// Counts one message against `lease_id` while it holds its fleet under
    /// `presented`, answering which line of the run it is, or `None` once the
    /// run has said [`MESSAGES_PER_RUN_MAX`] or the lease no longer holds.
    async fn count_message(
        &self,
        lease_id: &Uuid7,
        presented: u64,
        now: UnixMillis,
    ) -> Result<Option<u32>> {
        let mut connection = self.pool().acquire().await?;
        let counted = sqlx::query(COUNT_MESSAGE)
            .bind(lease_id.as_str())
            .bind(i32::try_from(MESSAGES_PER_RUN_MAX).unwrap_or(i32::MAX))
            .bind(i64::try_from(presented).unwrap_or(i64::MAX))
            .bind(sql::LEASE_STATUS_ACTIVE)
            .bind(now.as_millis())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_COUNT))?;
        counted
            .map(|row| {
                let posted: i32 = row.try_get(0).map_err(query(CONTEXT_COUNT))?;
                Ok(u32::try_from(posted).unwrap_or(MESSAGES_PER_RUN_MAX))
            })
            .transpose()
    }
}

#[cfg(test)]
#[path = "message/tests.rs"]
mod tests;
