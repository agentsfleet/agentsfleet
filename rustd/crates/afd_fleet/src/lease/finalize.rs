//! Everything a won report writes AFTER the money has committed.
//!
//! The terminal event row, the session checkpoint, the stream acknowledgement,
//! the freed affinity slot, and the audit row that closes the lease's history.
//! Five writes, none atomic with each other, and that is the Zig's shape kept
//! deliberately rather than improved on: they are independent facts about a run
//! that is already over and already paid for, and a transaction spanning
//! Postgres and Redis is not available anyway.
//!
//! # Best-effort means the report still succeeds
//!
//! Every write here is attempted, logged on failure, and never propagated. The
//! claim and the settle already committed by the time this runs — the lease is
//! `reported` and the wallet is drawn down — so failing the response now would
//! tell the runner to retry a report whose money cannot be charged twice, and
//! the retry would be fenced anyway. What an operator gets instead is a warn
//! line naming which of the five did not land.
//!
//! The one that matters most is [`Leases::release_slot`]: without it the fleet
//! waits out the full lease TTL before its next event can be claimed. It is
//! still best-effort, because a fleet idle for thirty seconds is a far smaller
//! fault than a report that answers 500 after taking a tenant's money.
//!
//! # The cap is `is_char_boundary`, not a nibble walk
//!
//! `event_rows.truncateUtf8` walks back over continuation bytes by masking
//! `0xC0`, because Zig's standard library gave it nothing better. Rust has
//! [`str::is_char_boundary`], which asks the question directly. See
//! [`truncate`].

use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_redis::EventId;

use crate::error::Result;
use crate::lease::affinity::Fence;
use crate::lease::sql;
use crate::lease::sql::session::MAX_CHECKPOINT_RESPONSE_BYTES;
use crate::lease::store::Leases;
use crate::lease::verdict::{Terminal, truncate};

/// Statement name, for the context a query failure carries.
const CONTEXT_TERMINAL: &str = "fleet event terminal";

/// Statement name, for the context a query failure carries.
const CONTEXT_CHECKPOINT: &str = "fleet session checkpoint";

/// Statement name, for the context a query failure carries.
const CONTEXT_RELEASED: &str = "runner lease released event";

/// The session cursor a fleet resumes from.
///
/// Serialized through `serde` rather than assembled by hand: the Zig builds an
/// anonymous struct and stringifies it, which is the same thing, and the
/// failure arm — `catch "{}"` — is what a `Result` here says out loud instead.
#[derive(Debug, serde::Serialize)]
struct Checkpoint<'a> {
    last_event_id: &'a str,
    last_response: &'a str,
}

impl Leases {
    /// End the event with the runner's verdict.
    ///
    /// Guarded on the row still being `received`, so a terminal row is never
    /// reopened and a redelivery whose acknowledgement was lost cannot
    /// overwrite the settled result. Zero rows moved is that case, and it is
    /// logged rather than treated as a failure.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn mark_terminal(
        &self,
        fleet_id: &Uuid7,
        event_id: &str,
        outcome: Terminal<'_>,
        now: UnixMillis,
    ) -> Result<()> {
        let Terminal {
            verdict,
            response_text,
            tokens,
            wall_ms,
        } = outcome;
        let mut connection = self.pool().acquire().await?;
        let moved = sqlx::query(afd_events::sql::UPDATE_FLEET_EVENT_RESULT)
            .bind(fleet_id.as_str())
            .bind(event_id)
            .bind(verdict.status())
            .bind(response_text)
            .bind(tokens)
            .bind(wall_ms)
            .bind(now.as_millis())
            .bind(verdict.label())
            .bind(afd_core::event::status::RECEIVED)
            .bind(verdict.detail())
            .execute(&mut *connection)
            .await
            .map_err(crate::error::query(CONTEXT_TERMINAL))?;

        if moved.rows_affected() == 0 {
            let fleet = fleet_id.as_str();
            tracing::warn!(
                fleet_id = fleet,
                agentsfleet_event_id = event_id,
                event = "terminal_write_skipped_nonreceived",
                "the event was already terminal; the settled result stands"
            );
        }
        Ok(())
    }

    /// Record where this fleet's session resumes.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a cursor that will not
    /// serialize — which cannot happen for two string fields, and is reported
    /// rather than swallowed so that stays true by test rather than by belief.
    pub async fn checkpoint(
        &self,
        fleet_id: &Uuid7,
        last_event_id: &str,
        last_response: &str,
        now: UnixMillis,
    ) -> Result<()> {
        // `Value`, then `Display`. `serde_json::to_string` is fallible in its
        // signature and cannot fail for two string fields, which would leave an
        // error arm no test could reach and no caller could act on — the Zig
        // spells the same dead branch as `catch "{}"`, silently checkpointing a
        // fleet to nothing. Rendering a `Value` has no failure to absorb.
        let document = serde_json::json!(Checkpoint {
            last_event_id,
            last_response: truncate(last_response, MAX_CHECKPOINT_RESPONSE_BYTES),
        })
        .to_string();

        let mut connection = self.pool().acquire().await?;
        sqlx::query(sql::session::UPSERT_FLEET_SESSION)
            .bind(fleet_id.as_str())
            .bind(&document)
            .bind(now.as_millis())
            .execute(&mut *connection)
            .await
            .map_err(crate::error::query(CONTEXT_CHECKPOINT))?;
        Ok(())
    }

    /// Acknowledge the stream entry this lease executed.
    ///
    /// # Errors
    /// Reports a queue that would not answer.
    pub async fn acknowledge(&self, fleet_id: &Uuid7, event_id: &str) -> Result<()> {
        let acknowledged = self
            .streams()
            .ack(fleet_id.as_str(), &EventId::of(event_id))
            .await?;
        if !acknowledged {
            let fleet = fleet_id.as_str();
            tracing::warn!(
                fleet_id = fleet,
                agentsfleet_event_id = event_id,
                event = "xack_no_entry",
                "the stream entry was already acknowledged or trimmed"
            );
        }
        Ok(())
    }

    /// Close the lease's history with the row that pairs its acquisition.
    ///
    /// # Errors
    /// Reports an entropy source that could not produce the row's identifier,
    /// an instant that cannot be encoded, and a datastore that would not
    /// answer.
    pub async fn record_released(
        &self,
        runner_id: &Uuid7,
        lease_id: &str,
        fleet_id: &Uuid7,
        event_id: &str,
        now: UnixMillis,
    ) -> Result<()> {
        let mut bytes = [0u8; ENTROPY_LEN];
        self.entropy().fill(&mut bytes)?;
        let row_id = Uuid7::encode(now, bytes)?;

        let mut connection = self.pool().acquire().await?;
        sqlx::query(sql::report::INSERT_RUNNER_EVENT)
            .bind(row_id.as_str())
            .bind(runner_id.as_str())
            .bind(afd_runner::sql::event_type::LEASE_RELEASED)
            .bind(now.as_millis())
            .bind(afd_runner::sql::meta::LEASE_ID)
            .bind(lease_id)
            .bind(afd_runner::sql::meta::FLEET_ID)
            .bind(fleet_id.as_str())
            .bind(afd_runner::sql::meta::AGENTSFLEET_EVENT_ID)
            .bind(event_id)
            .execute(&mut *connection)
            .await
            .map_err(crate::error::query(CONTEXT_RELEASED))?;
        Ok(())
    }

    /// Free the fleet's slot so its next event becomes claimable.
    ///
    /// Token-guarded through [`Leases::release`], so a superseded holder cannot
    /// free the current one's slot.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn release_slot(
        &self,
        fleet_id: &Uuid7,
        fence: Fence,
        now: UnixMillis,
    ) -> Result<()> {
        self.release(fleet_id, fence, now).await
    }
}
